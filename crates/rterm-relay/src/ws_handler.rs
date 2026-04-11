use crate::mouse_encoding::encode_vt_mouse;
use crate::pty::RealPtySpawner;
use crate::session_manager::SessionManager;
use futures_util::{SinkExt, StreamExt};
use grpc_codec_flatbuffers::FlatBufferGrpcMessage;
use rterm_proto::wire::{encode_message, strip_length_prefix};
use rterm_proto::*;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_tungstenite::{WebSocketStream, tungstenite};
use tracing::{debug, info, warn};

/// Handle a WebSocket session.
/// `session_name` is extracted from the URL path by the caller.
pub async fn handle_ws_session<S>(
    ws_stream: WebSocketStream<S>,
    session_mgr: &SessionManager,
    session_name: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
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
    // Wrap server_fwd_tx in Arc<Mutex<Option<_>>> so detach() can drop it
    // and close the channel, causing the forwarder task to exit.
    let (server_fwd_tx, mut server_fwd_rx) = mpsc::channel::<ServerMsg>(64);
    let server_fwd_tx = Arc::new(std::sync::Mutex::new(Some(server_fwd_tx)));
    let snapshot = {
        let mut s = session.lock().await;
        // We need to clone the inner sender, but we only have Arc<Mutex<Option<Sender>>>
        // Clone the Arc so the forwarder and attach both have their own reference
        let inner_tx = server_fwd_tx.lock().unwrap();
        s.attach(inner_tx.as_ref().unwrap().clone(), cols, rows)
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
    // Uses Arc<Mutex<Option<Sender>>> so detach() can close the channel.
    let fwd_server_fwd_tx = server_fwd_tx.clone();
    tokio::spawn(async move {
        info!("[WS] forward task started");
        // Check if the channel has been closed by detach() before each recv.
        // If detach() drops the sender, recv() will return None and we exit.
        while let Some(msg) = server_fwd_rx.recv().await {
            // Before processing, check if detach was called (sender set to None).
            if fwd_server_fwd_tx.lock().unwrap().is_none() {
                info!("[WS] forwarder: detach signal, stopping");
                break;
            }
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
        info!("[WS] forward task ended");
    });

    // Read client input and forward to PTY.
    while let Some(msg) = ws_stream.next().await {
        let data = match msg {
            Ok(tungstenite::Message::Binary(d)) => {
                info!("[WS] client msg binary, len={}", d.len());
                // Issue #9: Bytes is already reference-counted, borrow instead of copying.
                d.as_ref().to_vec()
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
                // Fix Issue #1 & #3: Drop lock BEFORE send to avoid deadlock
                // if channel buffer fills up (session_output_loop also needs the lock).
                let pty_stdin_tx = {
                    let s = session.lock().await;
                    s.pty_stdin_tx.clone()
                };
                // Lock dropped here (end of block)
                if pty_stdin_tx.send(k.data).await.is_err() {
                    warn!("[WS] PTY stdin send failed, session may be dead");
                    break;
                }
            }
            Ok(ClientMsg::PasteInput(p)) => {
                // Build data while holding lock, then send without lock.
                let data = {
                    let s = session.lock().await;
                    let mut data = Vec::new();
                    if s.terminal.bracketed_paste {
                        data.extend_from_slice(b"\x1b[200~");
                    }
                    data.extend_from_slice(p.text.as_bytes());
                    if s.terminal.bracketed_paste {
                        data.extend_from_slice(b"\x1b[201~");
                    }
                    data
                };
                let pty_stdin_tx = {
                    let s = session.lock().await;
                    s.pty_stdin_tx.clone()
                };
                if pty_stdin_tx.send(data).await.is_err() {
                    warn!("[WS] PTY stdin send failed (paste), session may be dead");
                    break;
                }
            }
            Ok(ClientMsg::Resize(r)) => {
                let mut s = session.lock().await;
                s.resize(r.cols, r.rows);
            }

            Ok(ClientMsg::MouseEvent(m)) => {
                // Fix Issue #1 & #3: Drop lock BEFORE send to avoid deadlock.
                let pty_stdin_tx = {
                    let s = session.lock().await;
                    if s.terminal.modes.mouse_tracking_mode > 0 {
                        Some(s.pty_stdin_tx.clone())
                    } else {
                        None
                    }
                };
                if let Some(pty_stdin_tx) = pty_stdin_tx {
                    let bytes = encode_vt_mouse(&m);
                    // Don't hold lock across send
                    if pty_stdin_tx.send(bytes).await.is_err() {
                        warn!("[WS] PTY stdin send failed (mouse), session may be dead");
                        break;
                    }
                }
            }

            Ok(ClientMsg::Scrollback(r)) => {
                // Get data under lock, then send without lock.
                let (scrollback, sender) = {
                    let s = session.lock().await;
                    let scrollback = s.get_scrollback(r.offset as usize, r.limit as usize);
                    let sender = s.client_tx.clone();
                    (scrollback, sender)
                };
                if let Some(sender) = sender
                    && sender
                        .send(ServerMsg::Scrollback(scrollback))
                        .await
                        .is_err()
                {
                    tracing::warn!("client_tx send failed for Scrollback");
                }
            }

            Ok(ClientMsg::Scroll(msg)) => {
                // Fix Issue #1 & #3: Drop lock BEFORE send to avoid deadlock.
                let direction = msg.direction;
                let lines = msg.lines;
                let (key_data, sender) = {
                    let s = session.lock().await;
                    if s.terminal.is_alt_screen_active() {
                        let key = if direction > 0 {
                            b"\x1b[A".to_vec()
                        } else {
                            b"\x1b[B".to_vec()
                        };
                        (Some((s.pty_stdin_tx.clone(), key)), None)
                    } else {
                        (None, s.client_tx.clone())
                    }
                };
                if let Some((tx, data)) = key_data
                    && tx.send(data).await.is_err()
                {
                    tracing::warn!("pty_stdin_tx send failed for Scroll");
                }
                if let Some(sender) = sender {
                    let snapshot = {
                        let mut s = session.lock().await;
                        s.scroll_viewport(direction, lines)
                    };
                    if sender
                        .send(ServerMsg::ScreenSnapshot(snapshot))
                        .await
                        .is_err()
                    {
                        tracing::warn!("client_tx send failed for ScreenSnapshot");
                    }
                }
            }

            Ok(ClientMsg::ResetViewport) => {
                // Get snapshot under lock, then send without lock.
                let (snapshot, sender) = {
                    let mut s = session.lock().await;
                    s.reset_viewport();
                    (s.screen_snapshot(), s.client_tx.clone())
                };
                if let Some(sender) = sender
                    && sender
                        .send(ServerMsg::ScreenSnapshot(snapshot))
                        .await
                        .is_err()
                {
                    tracing::warn!("client_tx send failed for ResetViewport");
                }
            }

            Ok(unhandled) => {
                tracing::debug!("unhandled ClientMsg variant: {:?}", unhandled);
            }
            Err(e) => warn!("decode error: {}", e),
        }
    }

    // Detach — session stays alive for reconnection.
    // Fix Issue #2: Drop the server_fwd_tx sender so the forwarder task's
    // channel is closed and the forwarder exits.
    {
        let mut guard = server_fwd_tx.lock().unwrap();
        *guard = None;
    }
    session.lock().await.detach();
    info!(
        "client disconnected from WebSocket session '{}', session detached",
        session_name
    );

    Ok(())
}
