use crate::mouse_encoding::encode_vt_mouse;
use crate::pty::RealPtySpawner;
use crate::session_manager::SessionManager;
use grpc_codec_flatbuffers::FlatBufferGrpcMessage;
use h3::quic::BidiStream as _;
use h3_webtransport::server::WebTransportSession;
use rterm_proto::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tracing::{debug, info};

/// Handle a WebTransport session.
/// `session_name` is extracted from the URL path by the caller.
pub async fn handle_wt_session(
    wt_session: WebTransportSession<h3_quinn::Connection, bytes::Bytes>,
    session_mgr: &SessionManager,
    session_name: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let accepted = wt_session
        .accept_bi()
        .await?
        .ok_or("no bidi stream from client")?;

    let stream = match accepted {
        h3_webtransport::server::AcceptedBi::BidiStream(_id, stream) => stream,
        h3_webtransport::server::AcceptedBi::Request(_, _) => {
            return Err("expected bidi stream, got HTTP request".into());
        }
    };

    let (mut send, mut recv) = stream.split();
    info!("WebTransport session: name={}", session_name);

    // Read first message — must be Resize (to get terminal dimensions).
    let first_data = read_message(&mut recv)
        .await?
        .ok_or("empty stream — expected Resize")?;
    let first_msg =
        ClientMsg::decode_flatbuffer(&first_data).map_err(|e| format!("decode: {}", e))?;

    let (cols, rows) = match first_msg {
        ClientMsg::Resize(r) => (r.cols, r.rows),
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
    let encoded = ServerMsg::ScreenSnapshot(snapshot).encode_flatbuffer();
    write_message(&mut send, &encoded).await?;

    // If PTY already exited, send Exit.
    {
        let s = session.lock().await;
        if let Some(code) = s.pty_exited {
            let encoded = ServerMsg::Exit(Exit { code }).encode_flatbuffer();
            write_message(&mut send, &encoded).await?;
        }
    }

    // Task: forward server messages to WebTransport.
    tokio::spawn(async move {
        while let Some(msg) = server_fwd_rx.recv().await {
            let encoded = msg.encode_flatbuffer();
            if write_message(&mut send, &encoded).await.is_err() {
                break;
            }
        }
    });

    // Read client input and forward to PTY.
    loop {
        match read_message(&mut recv).await {
            Ok(Some(data)) => match ClientMsg::decode_flatbuffer(&data) {
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
                    // Forward mouse events to PTY when mouse tracking is enabled.
                    // The WASM client sends scroll events as MouseEvent with buttons 64/65.
                    let s = session.lock().await;
                    if s.terminal.modes.mouse_tracking_mode > 0 {
                        // Encode as VT mouse protocol and send to PTY.
                        let bytes = encode_vt_mouse(&m);
                        let _ = s.pty_stdin_tx.send(bytes).await;
                    }
                }

                Ok(ClientMsg::Scrollback(r)) => {
                    let s = session.lock().await;
                    let scrollback = s.get_scrollback(r.offset as usize, r.limit as usize);
                    // Send through the server_fwd channel so the spawned task writes it.
                    let _ = server_fwd_tx.send(ServerMsg::Scrollback(scrollback)).await;
                }

                Ok(ClientMsg::Scroll(s)) => {
                    let mut session = session.lock().await;
                    // If alternate screen is active (vim, zellij, etc.), forward scroll as
                    // up/down key presses so the app can handle its own scrolling.
                    if session.terminal.is_alt_screen_active() {
                        let key = if s.direction > 0 {
                            b"\x1b[A".to_vec() // ArrowUp
                        } else {
                            b"\x1b[B".to_vec() // ArrowDown
                        };
                        let _ = session.pty_stdin_tx.send(key).await;
                    } else {
                        // Normal mode: scroll the viewport through scrollback history.
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
            },
            Ok(None) => break,
            Err(e) => {
                debug!("read error: {}", e);
                break;
            }
        }
    }

    // Detach — session stays alive for reconnection.
    session.lock().await.detach();
    info!(
        "client disconnected from session '{}', session detached",
        session_name
    );

    Ok(())
}

pub(crate) async fn read_message<S>(recv: &mut S) -> Result<Option<Vec<u8>>, String>
where
    S: tokio::io::AsyncRead + Unpin,
{
    let mut len_buf = [0u8; 4];
    match recv.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(format!("read length: {}", e)),
    }
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut payload = vec![0u8; len];
    recv.read_exact(&mut payload)
        .await
        .map_err(|e| format!("read payload: {}", e))?;
    Ok(Some(payload))
}

pub(crate) async fn write_message<S>(send: &mut S, payload: &[u8]) -> Result<(), String>
where
    S: tokio::io::AsyncWrite + Unpin,
{
    let len = payload.len() as u32;
    let mut buf = Vec::with_capacity(4 + payload.len());
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(payload);
    send.write_all(&buf)
        .await
        .map_err(|e| format!("write: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[tokio::test]
    async fn read_write_roundtrip() {
        let mut buf = Vec::new();
        write_message(&mut buf, b"hello").await.unwrap();
        let mut reader = Cursor::new(buf);
        assert_eq!(
            read_message(&mut reader).await.unwrap(),
            Some(b"hello".to_vec())
        );
    }

    #[tokio::test]
    async fn read_empty_stream() {
        let mut reader = Cursor::new(Vec::<u8>::new());
        assert_eq!(read_message(&mut reader).await.unwrap(), None);
    }

    #[tokio::test]
    async fn length_prefix_format() {
        let mut buf = Vec::new();
        write_message(&mut buf, b"test").await.unwrap();
        assert_eq!(&buf[..4], &[0, 0, 0, 4]);
        assert_eq!(&buf[4..], b"test");
    }
}
