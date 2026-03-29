/// WebTransport terminal handler.
///
/// Accepts WebTransport sessions, reads length-prefixed FlatBuffers ClientMessages
/// from a bidi stream, bridges to a PTY, and sends ServerMessages back.
use crate::pty::PtySession;
use h3::quic::BidiStream as _;
use h3_webtransport::server::WebTransportSession;
use rterm_proto::{ClientMsg, DataOut, ServerMsg};
use grpc_codec_flatbuffers::FlatBufferGrpcMessage;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, info};

/// Handle a WebTransport session: bridge bidi stream to PTY.
pub async fn handle_wt_session(
    session: WebTransportSession<h3_quinn::Connection, bytes::Bytes>,
    shell: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Accept a bidi stream from the client.
    let accepted = session
        .accept_bi()
        .await?
        .ok_or("no bidi stream from client")?;

    let stream = match accepted {
        h3_webtransport::server::AcceptedBi::BidiStream(_session_id, stream) => stream,
        h3_webtransport::server::AcceptedBi::Request(_, _) => {
            return Err("expected bidi stream, got HTTP request".into());
        }
    };

    let (mut send, mut recv) = stream.split();
    info!("WebTransport bidi stream accepted");

    // Read the first message — must be Resize.
    let first_msg = read_message(&mut recv).await?
        .ok_or("empty stream — expected initial Resize")?;
    let first = ClientMsg::decode_flatbuffer(&first_msg)
        .map_err(|e| format!("decode first message: {}", e))?;

    let (cols, rows) = match first {
        ClientMsg::Resize(r) => (r.cols, r.rows),
        _ => return Err("first message must be Resize".into()),
    };

    info!("spawning PTY: shell={}, size={}x{}", shell, cols, rows);

    let pty = PtySession::spawn(shell, cols, rows)?;
    let stdin_tx = pty.stdin_tx;
    let resize_tx = pty.resize_tx;
    let mut stdout_rx = pty.stdout_rx;

    // Task: read client messages and forward to PTY.
    tokio::spawn(async move {
        loop {
            match read_message(&mut recv).await {
                Ok(Some(data)) => {
                    match ClientMsg::decode_flatbuffer(&data) {
                        Ok(ClientMsg::DataIn(d)) => {
                            if stdin_tx.send(d.payload).await.is_err() {
                                break;
                            }
                        }
                        Ok(ClientMsg::Resize(r)) => {
                            if resize_tx.send((r.cols, r.rows)).await.is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            debug!("decode error: {}", e);
                        }
                    }
                }
                Ok(None) => {
                    debug!("client bidi stream ended");
                    break;
                }
                Err(e) => {
                    debug!("read error: {}", e);
                    break;
                }
            }
        }
    });

    // Main loop: forward PTY stdout to client as ServerMessages.
    while let Some(data) = stdout_rx.recv().await {
        let msg = ServerMsg::DataOut(DataOut { payload: data });
        let encoded = msg.encode_flatbuffer();
        if let Err(e) = write_message(&mut send, &encoded).await {
            debug!("send error: {}", e);
            break;
        }
    }

    info!("WebTransport session ended");
    Ok(())
}

/// Read a length-prefixed message from a WebTransport recv stream.
/// Format: [4-byte big-endian length] [payload]
async fn read_message<S>(recv: &mut S) -> Result<Option<Vec<u8>>, String>
where
    S: tokio::io::AsyncRead + Unpin,
{
    // Read 4-byte length prefix.
    let mut len_buf = [0u8; 4];
    match recv.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(format!("read length: {}", e)),
    }
    let len = u32::from_be_bytes(len_buf) as usize;

    // Read payload.
    let mut payload = vec![0u8; len];
    recv.read_exact(&mut payload)
        .await
        .map_err(|e| format!("read payload: {}", e))?;

    Ok(Some(payload))
}

/// Write a length-prefixed message to a WebTransport send stream.
async fn write_message<S>(send: &mut S, payload: &[u8]) -> Result<(), String>
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
