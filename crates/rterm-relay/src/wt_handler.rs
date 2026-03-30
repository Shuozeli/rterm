/// WebTransport terminal handler (v3: session management).
use crate::managed_session::ManagedSession;
use crate::pty::RealPtySpawner;
use crate::session_manager::SessionManager;
use grpc_codec_flatbuffers::FlatBufferGrpcMessage;
use h3::quic::BidiStream as _;
use h3_webtransport::server::WebTransportSession;
use rterm_proto::*;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, info};

/// Handle a WebTransport session with session management.
pub async fn handle_wt_session(
    wt_session: WebTransportSession<h3_quinn::Connection, bytes::Bytes>,
    session_mgr: &SessionManager,
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
    info!("WebTransport bidi stream accepted");

    // Read the first message to determine session intent.
    let first_data = read_message(&mut recv).await?.ok_or("empty stream")?;
    let first_msg = ClientMsg::decode_flatbuffer(&first_data)
        .map_err(|e| format!("decode first message: {}", e))?;

    // Resolve or create the session based on the first message.
    let (session, initial_msgs) = match first_msg {
        ClientMsg::CreateSession(cs) => {
            let spawner = RealPtySpawner;
            let (id, name, token) = session_mgr
                .create(
                    if cs.name.is_some() { cs.name } else { None },
                    cs.shell,
                    cs.cols,
                    cs.rows,
                    &spawner,
                )
                .await
                .map_err(|e| format!("create session failed: {}", e))?;

            let session = session_mgr
                .attach(&id, &token)
                .await
                .map_err(|e| format!("attach after create: {}", e))?;

            let (client_tx, server_msgs) = attach_client(&session, cs.cols, cs.rows).await;

            let mut msgs = vec![ServerMsg::SessionCreated(SessionCreated {
                session_id: id,
                name,
                token,
            })];
            msgs.extend(server_msgs);

            (session, (client_tx, msgs))
        }
        ClientMsg::AttachSession(att) => {
            let session = session_mgr
                .attach(&att.session_id, &att.token)
                .await
                .map_err(|e| format!("attach failed: {}", e))?;

            let (client_tx, server_msgs) = attach_client(&session, att.cols, att.rows).await;

            let s = session.lock().await;
            let mut msgs = vec![ServerMsg::SessionAttached(SessionAttached {
                session_id: s.id.clone(),
                name: s.name.clone(),
            })];
            drop(s);
            msgs.extend(server_msgs);

            (session, (client_tx, msgs))
        }
        ClientMsg::Resize(r) => {
            // Legacy: bare Resize creates anonymous session.
            let spawner = RealPtySpawner;
            let (id, _name, token) = session_mgr
                .create(None, None, r.cols, r.rows, &spawner)
                .await
                .map_err(|e| format!("create anonymous session: {}", e))?;

            let session = session_mgr
                .attach(&id, &token)
                .await
                .map_err(|e| format!("attach anonymous: {}", e))?;

            let (client_tx, server_msgs) = attach_client(&session, r.cols, r.rows).await;
            (session, (client_tx, server_msgs))
        }
        _ => {
            return Err("first message must be CreateSession, AttachSession, or Resize".into());
        }
    };

    let (_client_tx, initial_server_msgs) = initial_msgs;

    // Send initial server messages (SessionCreated/Attached + ScreenSnapshot).
    for msg in initial_server_msgs {
        let encoded = msg.encode_flatbuffer();
        write_message(&mut send, &encoded).await?;
    }

    // Channel for server -> WebTransport.
    let (server_fwd_tx, mut server_fwd_rx) = mpsc::channel::<ServerMsg>(64);

    // Task: forward server messages from session to WebTransport.
    tokio::spawn(async move {
        while let Some(msg) = server_fwd_rx.recv().await {
            let encoded = msg.encode_flatbuffer();
            if let Err(e) = write_message(&mut send, &encoded).await {
                debug!("send error: {}", e);
                break;
            }
        }
    });

    // Replace the session's client_tx with our forwarding channel.
    {
        let mut s = session.lock().await;
        s.client_tx = Some(server_fwd_tx);
    }

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
                    data.extend_from_slice(b"\x1b[200~");
                    data.extend_from_slice(p.text.as_bytes());
                    data.extend_from_slice(b"\x1b[201~");
                    let _ = s.pty_stdin_tx.send(data).await;
                }
                Ok(ClientMsg::Resize(r)) => {
                    let mut s = session.lock().await;
                    s.cols = r.cols;
                    s.rows = r.rows;
                    s.terminal.resize(r.cols as usize, r.rows as usize);
                    let _ = s.pty_resize_tx.send((r.cols, r.rows)).await;
                }
                Ok(ClientMsg::DetachSession) => {
                    info!("client explicitly detached");
                    break;
                }
                Ok(ClientMsg::DestroySession(ds)) => {
                    let _ = session_mgr.destroy(&ds.session_id).await;
                    break;
                }
                Ok(_) => {} // Other messages ignored during session.
                Err(e) => debug!("decode error: {}", e),
            },
            Ok(None) => break, // Client disconnected.
            Err(e) => {
                debug!("read error: {}", e);
                break;
            }
        }
    }

    // Detach — session stays alive.
    {
        let mut s = session.lock().await;
        s.detach();
    }
    info!("client disconnected, session detached");

    Ok(())
}

/// Attach a client to a session. Returns (client_tx, initial messages to send).
async fn attach_client(
    session: &Arc<Mutex<ManagedSession>>,
    cols: u16,
    rows: u16,
) -> (mpsc::Sender<ServerMsg>, Vec<ServerMsg>) {
    let (client_tx, _) = mpsc::channel(64);
    let mut s = session.lock().await;
    let snapshot = s.attach(client_tx.clone(), cols, rows);

    let mut msgs = vec![ServerMsg::ScreenSnapshot(snapshot)];

    // If PTY already exited, send Exit.
    if let Some(code) = s.pty_exited {
        msgs.push(ServerMsg::Exit(Exit { code }));
    }

    (client_tx, msgs)
}

/// Read a length-prefixed message from a stream.
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

/// Write a length-prefixed message to a stream.
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
    async fn read_write_message_roundtrip() {
        let payload = b"hello world";
        let mut buf = Vec::new();
        write_message(&mut buf, payload).await.unwrap();
        let mut reader = Cursor::new(buf);
        let result = read_message(&mut reader).await.unwrap();
        assert_eq!(result, Some(payload.to_vec()));
    }

    #[tokio::test]
    async fn read_message_empty_stream() {
        let mut reader = Cursor::new(Vec::<u8>::new());
        assert_eq!(read_message(&mut reader).await.unwrap(), None);
    }

    #[tokio::test]
    async fn read_write_multiple_messages() {
        let mut buf = Vec::new();
        write_message(&mut buf, b"msg1").await.unwrap();
        write_message(&mut buf, b"msg2").await.unwrap();
        let mut reader = Cursor::new(buf);
        assert_eq!(
            read_message(&mut reader).await.unwrap(),
            Some(b"msg1".to_vec())
        );
        assert_eq!(
            read_message(&mut reader).await.unwrap(),
            Some(b"msg2".to_vec())
        );
        assert_eq!(read_message(&mut reader).await.unwrap(), None);
    }

    #[tokio::test]
    async fn write_message_length_prefix() {
        let mut buf = Vec::new();
        write_message(&mut buf, b"test").await.unwrap();
        assert_eq!(&buf[..4], &[0, 0, 0, 4]);
        assert_eq!(&buf[4..], b"test");
    }

    #[tokio::test]
    async fn read_write_empty_message() {
        let mut buf = Vec::new();
        write_message(&mut buf, b"").await.unwrap();
        let mut reader = Cursor::new(buf);
        assert_eq!(read_message(&mut reader).await.unwrap(), Some(vec![]));
    }

    #[tokio::test]
    async fn read_write_large_message() {
        let payload = vec![0xABu8; 100_000];
        let mut buf = Vec::new();
        write_message(&mut buf, &payload).await.unwrap();
        let mut reader = Cursor::new(buf);
        let result = read_message(&mut reader).await.unwrap().unwrap();
        assert_eq!(result.len(), 100_000);
    }
}
