use crate::mouse_encoding::encode_vt_mouse;
use crate::pty::RealPtySpawner;
use crate::session_manager::SessionManager;
use futures_util::{SinkExt, StreamExt};
use grpc_codec_flatbuffers::FlatBufferGrpcMessage;
use rterm_proto::wire::{encode_message, strip_length_prefix};
use rterm_proto::*;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::{WebSocketStream, tungstenite};
use tracing::{debug, info};

/// Handle a WebSocket session.
/// `session_name` is extracted from the URL path by the caller.
pub async fn handle_ws_session(
    ws_stream: WebSocketStream<TlsStream>,
    session_mgr: &SessionManager,
    session_name: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (mut ws_sink, mut ws_stream) = ws_stream.split();
    info!("[WS] connection started, session_name={}", session_name);

    // Read first message — must be Resize (to get terminal dimensions).
    // Messages are length-prefixed: [4-byte BE u32] [FlatBuffers payload]
    info!("[WS] waiting for first message...");
    let first_msg = match ws_stream.next().await {
        Some(Ok(tungstenite::Message::Binary(data))) => {
            info!("[WS] received binary msg, len={}", data.len());
            let payload = strip_length_prefix(&data).ok_or("invalid length prefix")?;
            info!("[WS] payload after strip len, len={}", payload.len());
            ClientMsg::decode_flatbuffer(&payload).map_err(|e| format!("decode: {}", e))?
        }
        Some(Ok(tungstenite::Message::Text(text))) => {
            info!("[WS] received text msg, len={}", text.len());
            let payload = strip_length_prefix(text.as_bytes()).ok_or("invalid length prefix")?;
            ClientMsg::decode_flatbuffer(&payload).map_err(|e| format!("decode: {}", e))?
        }
        Some(Ok(msg)) => {
            return Err(format!("expected binary resize message, got {:?}", msg).into());
        }
        Some(Err(e)) => return Err(format!("read error: {}", e).into()),
        None => return Err("empty stream — expected Resize".into()),
    };

    let (cols, rows) = match first_msg {
        ClientMsg::Resize(r) => {
            info!("[WS] resize: {}x{}", r.cols, r.rows);
            (r.cols, r.rows)
        }
        _ => return Err("first message must be Resize".into()),
    };

    // Get or create the named session.
    let spawner = RealPtySpawner;
    let session = session_mgr
        .get_or_create(session_name, cols, rows, &spawner)
        .await
        .map_err(|e| format!("session error: {}", e))?;

    // Attach client.
    let (server_fwd_tx, mut server_fwd_rx) = mpsc::channel::<ServerMsg>(64);
    let snapshot = {
        let mut s = session.lock().await;
        s.attach(server_fwd_tx.clone(), cols, rows)
    };

    // Send initial ScreenSnapshot.
    info!(
        "[WS] sending ScreenSnapshot, cols={}, num_rows={}",
        snapshot.cols, snapshot.num_rows
    );
    let encoded = encode_message(ServerMsg::ScreenSnapshot(snapshot).encode_flatbuffer());
    info!("[WS] encoded snapshot len={}", encoded.len());
    ws_sink
        .send(tungstenite::Message::Binary(encoded.into()))
        .await?;

    // If PTY already exited, send Exit.
    {
        let s = session.lock().await;
        if let Some(code) = s.pty_exited {
            let encoded = encode_message(ServerMsg::Exit(Exit { code }).encode_flatbuffer());
            ws_sink
                .send(tungstenite::Message::Binary(encoded.into()))
                .await?;
        }
    }

    // Task: forward server messages to WebSocket.
    tokio::spawn(async move {
        info!("[WS] forward task started");
        while let Some(msg) = server_fwd_rx.recv().await {
            let encoded = encode_message(msg.encode_flatbuffer());
            info!("[WS] forwarding msg, encoded len={}", encoded.len());
            if ws_sink
                .send(tungstenite::Message::Binary(encoded.into()))
                .await
                .is_err()
            {
                info!("[WS] send failed, breaking");
                break;
            }
        }
    });

    // Read client input and forward to PTY.
    while let Some(msg) = ws_stream.next().await {
        let data = match msg {
            Ok(tungstenite::Message::Binary(d)) => {
                info!("[WS] client msg binary, len={}", d.len());
                d.to_vec()
            }
            Ok(tungstenite::Message::Text(t)) => {
                info!("[WS] client msg text, len={}", t.len());
                t.as_bytes().to_vec()
            }
            Ok(tungstenite::Message::Close(_)) => {
                info!("[WS] client closed");
                break;
            }
            Ok(_) => continue,
            Err(e) => {
                debug!("WebSocket read error: {}", e);
                break;
            }
        };

        let payload = match strip_length_prefix(&data) {
            Some(p) => p,
            None => {
                debug!("invalid length prefix, skipping message");
                continue;
            }
        };
        match ClientMsg::decode_flatbuffer(&payload) {
            Ok(ClientMsg::KeyInput(k)) => {
                let s = session.lock().await;
                let _ = s.pty_stdin_tx.send(k.data).await;
            }
            Ok(ClientMsg::PasteInput(p)) => {
                let s = session.lock().await;
                let mut data = Vec::new();
                if s.terminal.bracketed_paste {
                    data.extend_from_slice(b"\x1b[200~");
                }
                data.extend_from_slice(p.text.as_bytes());
                if s.terminal.bracketed_paste {
                    data.extend_from_slice(b"\x1b[201~");
                }
                let _ = s.pty_stdin_tx.send(data).await;
            }
            Ok(ClientMsg::Resize(r)) => {
                let mut s = session.lock().await;
                s.resize(r.cols, r.rows);
            }

            Ok(ClientMsg::MouseEvent(m)) => {
                let s = session.lock().await;
                if s.terminal.modes.mouse_tracking_mode > 0 {
                    let bytes = encode_vt_mouse(&m);
                    let _ = s.pty_stdin_tx.send(bytes).await;
                }
            }

            Ok(ClientMsg::Scrollback(r)) => {
                let s = session.lock().await;
                let scrollback = s.get_scrollback(r.offset as usize, r.limit as usize);
                let _ = server_fwd_tx.send(ServerMsg::Scrollback(scrollback)).await;
            }

            Ok(ClientMsg::Scroll(s)) => {
                let mut session = session.lock().await;
                if session.terminal.is_alt_screen_active() {
                    let key = if s.direction > 0 {
                        b"\x1b[A".to_vec()
                    } else {
                        b"\x1b[B".to_vec()
                    };
                    let _ = session.pty_stdin_tx.send(key).await;
                } else {
                    let snapshot = session.scroll_viewport(s.direction, s.lines);
                    let _ = server_fwd_tx
                        .send(ServerMsg::ScreenSnapshot(snapshot))
                        .await;
                }
            }

            Ok(ClientMsg::ResetViewport) => {
                let mut session = session.lock().await;
                session.reset_viewport();
                let snapshot = session.screen_snapshot();
                let _ = server_fwd_tx
                    .send(ServerMsg::ScreenSnapshot(snapshot))
                    .await;
            }

            Ok(_) => {}
            Err(e) => debug!("decode error: {}", e),
        }
    }

    // Detach — session stays alive for reconnection.
    session.lock().await.detach();
    info!(
        "client disconnected from WebSocket session '{}', session detached",
        session_name
    );

    Ok(())
}

// Alias for the TLS stream type.
type TlsStream = tokio_rustls::server::TlsStream<TcpStream>;
