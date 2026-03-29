/// WebTransport terminal handler (v2: server-side VT emulation).
///
/// The server runs the VT emulator, diffs screen state, and sends
/// typed ScreenUpdate/ScreenSnapshot messages to the client.
use crate::pty::PtySession;
use crate::screen_diff::{self, PrevScreen};
use grpc_codec_flatbuffers::FlatBufferGrpcMessage;
use h3::quic::BidiStream as _;
use h3_webtransport::server::WebTransportSession;
use rterm_core::Terminal;
use rterm_proto::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, info};

/// Handle a WebTransport session: run VT emulation server-side,
/// send typed screen updates to the client.
pub async fn handle_wt_session(
    session: WebTransportSession<h3_quinn::Connection, bytes::Bytes>,
    shell: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
    let first_msg = read_message(&mut recv)
        .await?
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

    // Create the terminal emulator (server-side).
    let mut terminal = Terminal::new(cols as usize, rows as usize);
    let mut prev_screen = PrevScreen::new(cols as usize, rows as usize);

    // Send initial snapshot (blank screen).
    let ss = screen_diff::snapshot(terminal.screen());
    prev_screen.update_from_snapshot(&ss);
    let msg = ServerMsg::ScreenSnapshot(ss);
    write_message(&mut send, &msg.encode_flatbuffer()).await?;

    // Task: read client messages and forward to PTY.
    tokio::spawn(async move {
        loop {
            match read_message(&mut recv).await {
                Ok(Some(data)) => match ClientMsg::decode_flatbuffer(&data) {
                    Ok(ClientMsg::KeyInput(k)) => {
                        if stdin_tx.send(k.data).await.is_err() {
                            break;
                        }
                    }
                    Ok(ClientMsg::PasteInput(p)) => {
                        // TODO: bracket paste wrapping if mode is active.
                        if stdin_tx.send(p.text.into_bytes()).await.is_err() {
                            break;
                        }
                    }
                    Ok(ClientMsg::Resize(r)) => {
                        if resize_tx.send((r.cols, r.rows)).await.is_err() {
                            break;
                        }
                    }
                    Ok(ClientMsg::MouseEvent(_m)) => {
                        // TODO: encode mouse event as VT sequence.
                    }
                    Err(e) => {
                        debug!("decode error: {}", e);
                    }
                },
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

    // Main loop: read PTY output, run VT emulator, send screen diffs.
    while let Some(data) = stdout_rx.recv().await {
        // Feed PTY output through the VT emulator.
        terminal.feed(&data);

        // Skip sending updates while in synchronized output mode.
        if terminal.is_sync_mode() {
            continue;
        }

        // Diff the screen and send changes.
        if let Some(update) = prev_screen.diff(terminal.screen()) {
            let msg = ServerMsg::ScreenUpdate(update);
            if let Err(e) = write_message(&mut send, &msg.encode_flatbuffer()).await {
                debug!("send error: {}", e);
                break;
            }
        }
    }

    // Send Exit message.
    let exit_msg = ServerMsg::Exit(Exit { code: 0 });
    let _ = write_message(&mut send, &exit_msg.encode_flatbuffer()).await;

    info!("WebTransport session ended");
    Ok(())
}

/// Read a length-prefixed message from a WebTransport recv stream.
async fn read_message<S>(recv: &mut S) -> Result<Option<Vec<u8>>, String>
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
