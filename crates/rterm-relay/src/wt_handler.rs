/// WebTransport terminal handler (v2: delegates to session::run_session).
use crate::pty::RealPtySpawner;
use crate::session;
use grpc_codec_flatbuffers::FlatBufferGrpcMessage;
use h3::quic::BidiStream as _;
use h3_webtransport::server::WebTransportSession;
use rterm_proto::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tracing::{debug, info};

/// Handle a WebTransport session: bridge bidi stream to session::run_session.
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

    // Channel pair: WebTransport recv -> client_tx -> session
    let (client_tx, mut client_rx) = mpsc::channel::<ClientMsg>(64);
    // Channel pair: session -> server_rx -> WebTransport send
    let (server_tx, mut server_rx) = mpsc::channel::<ServerMsg>(64);

    // Task: read from WebTransport, decode, forward to session.
    tokio::spawn(async move {
        loop {
            match read_message(&mut recv).await {
                Ok(Some(data)) => match ClientMsg::decode_flatbuffer(&data) {
                    Ok(msg) => {
                        if client_tx.send(msg).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => debug!("decode error: {}", e),
                },
                Ok(None) => break,
                Err(e) => {
                    debug!("read error: {}", e);
                    break;
                }
            }
        }
    });

    // Task: read from session, encode, write to WebTransport.
    tokio::spawn(async move {
        while let Some(msg) = server_rx.recv().await {
            let encoded = msg.encode_flatbuffer();
            if let Err(e) = write_message(&mut send, &encoded).await {
                debug!("send error: {}", e);
                break;
            }
        }
    });

    // Run the session (blocking until PTY exits).
    let spawner = RealPtySpawner;
    session::run_session(&mut client_rx, &server_tx, &spawner, shell)
        .await
        .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { Box::new(e) })?;

    info!("WebTransport session ended");
    Ok(())
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
        write_message(&mut buf, b"msg3").await.unwrap();

        let mut reader = Cursor::new(buf);
        assert_eq!(
            read_message(&mut reader).await.unwrap(),
            Some(b"msg1".to_vec())
        );
        assert_eq!(
            read_message(&mut reader).await.unwrap(),
            Some(b"msg2".to_vec())
        );
        assert_eq!(
            read_message(&mut reader).await.unwrap(),
            Some(b"msg3".to_vec())
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
