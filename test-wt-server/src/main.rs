//! Minimal WebTransport server to test WebTransport connectivity.
//!
//! Tests: does WebTransport work with self-signed certs in this environment?
use bytes::Bytes;
use h3::ext::Protocol;
use h3::quic::BidiStream as _;
use h3_webtransport::server::WebTransportSession;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{debug, error, info};

// === Certificate generation ===

fn generate_cert() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    use rcgen::{CertificateParams, KeyPair, PKCS_ECDSA_P256_SHA256};

    let mut params = CertificateParams::new(vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ])
    .unwrap();

    // Set validity to 14 days (maximum for serverCertificateHashes).
    let now = time::OffsetDateTime::now_utc();
    params.not_before = now;
    params.not_after = now + time::Duration::days(14);

    let key_pair = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).unwrap();
    let cert = params.self_signed(&key_pair).unwrap();

    let cert_der = cert.der().to_vec();
    let cert_pem = cert.pem().into_bytes();
    let key_pem = key_pair.serialize_pem().into_bytes();
    (cert_pem, key_pem, cert_der)
}

// === QUIC endpoint setup ===

fn create_endpoint(
    addr: SocketAddr,
    cert_pem: &[u8],
    key_pem: &[u8],
) -> std::io::Result<quinn::Endpoint> {
    use rustls_pemfile::{certs, private_key};

    let certs = certs(&mut std::io::BufReader::new(cert_pem))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let key = private_key(&mut std::io::BufReader::new(key_pem))?
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no private key"))?;

    let mut tls_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    tls_config.alpn_protocols = vec![b"h3".to_vec()];

    let server_config = quinn::ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(tls_config)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
    ));

    Ok(quinn::Endpoint::server(server_config, addr)?)
}

// === HTTP/3 + WebTransport handler ===

async fn handle_connection(conn: quinn::Connection, cert_hash_b64: String) -> Result<(), Box<dyn std::error::Error>> {
    let remote = conn.remote_address();
    info!("QUIC connection from {}", remote);

    let mut h3_conn = h3::server::builder()
        .enable_webtransport(true)
        .enable_extended_connect(true)
        .enable_datagram(true)
        .max_webtransport_sessions(1)
        .send_grease(true)
        .build(h3_quinn::Connection::new(conn))
        .await?;

    loop {
        match h3_conn.accept().await {
            Ok(Some(resolver)) => {
                let (req, stream) = resolver.resolve_request().await?;
                debug!(
                    "request: {} {} {:?}",
                    req.method(),
                    req.uri(),
                    req.extensions().get::<Protocol>()
                );

                let is_wt = req.method() == http::Method::CONNECT
                    && req
                        .extensions()
                        .get::<Protocol>()
                        .map(|p| p == &Protocol::WEB_TRANSPORT)
                        .unwrap_or(false);

                if is_wt {
                    info!("WebTransport session request: {}", req.uri());
                    let session =
                        WebTransportSession::accept(req, stream, h3_conn).await?;
                    info!("WebTransport session established");
                    handle_wt_session(session).await?;
                    return Ok(());
                } else if req.method() == http::Method::GET {
                    // Serve a simple test page.
                    serve_html(stream, &cert_hash_b64).await?;
                } else {
                    let mut stream = stream;
                    let resp = http::Response::builder()
                        .status(404)
                        .body(())
                        .unwrap();
                    stream.send_response(resp).await?;
                    stream.finish().await?;
                }
            }
            Ok(None) => break,
            Err(err) => {
                error!("h3 accept error: {}", err);
                break;
            }
        }
    }

    Ok(())
}

async fn handle_wt_session(
    session: WebTransportSession<h3_quinn::Connection, bytes::Bytes>,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        match session.accept_bi().await {
            Ok(Some(accepted)) => {
                let stream = match accepted {
                    h3_webtransport::server::AcceptedBi::BidiStream(_session_id, stream) => stream,
                    h3_webtransport::server::AcceptedBi::Request(_, _) => {
                        debug!("ignoring HTTP request on WebTransport session");
                        continue;
                    }
                };
                let (mut send, mut recv) = stream.split();
                info!("WebTransport bidi stream accepted");

                tokio::spawn(async move {
                    use tokio::io::{AsyncReadExt, AsyncWriteExt};
                    let mut buf = [0u8; 4096];
                    loop {
                        match recv.read(&mut buf).await {
                            Ok(0) => {
                                debug!("bidi stream recv done");
                                break;
                            }
                            Ok(n) => {
                                let mut out = Vec::with_capacity(5 + n);
                                out.extend_from_slice(b"echo:");
                                out.extend_from_slice(&buf[..n]);
                                if let Err(err) = send.write_all(&out).await {
                                    debug!("send error: {}", err);
                                    break;
                                }
                            }
                            Err(err) => {
                                debug!("recv error: {}", err);
                                break;
                            }
                        }
                    }
                });
            }
            Ok(None) => {
                info!("WebTransport session closed");
                break;
            }
            Err(err) => {
                error!("WebTransport accept_bi error: {}", err);
                break;
            }
        }
    }

    Ok(())
}

async fn serve_html(
    mut stream: h3::server::RequestStream<h3_quinn::BidiStream<bytes::Bytes>, bytes::Bytes>,
    cert_hash_b64: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let html = format!(r#"<!DOCTYPE html>
<html>
<head><title>Minimal WebTransport Test</title></head>
<body>
<h1>Minimal WebTransport Test</h1>
<p>Open Chrome DevTools console to see connection status.</p>
<script>
const hashB64 = "{hash}";
const url = `https://${{location.hostname}}:${{location.port}}/wt`;

async function run() {{
    try {{
        console.log('Connecting to WebTransport:', url);
        console.log('Cert hash:', hashB64);

        // Decode base64 hash to Uint8Array.
        const hashBytes = Uint8Array.from(atob(hashB64), c => c.charCodeAt(0));

        const transport = new WebTransport(url, {{
            serverCertificateHashes: [{{
                algorithm: 'sha-256',
                value: hashBytes.buffer,
            }}],
        }});

        transport.onconnectionstatechange = () => {{
            console.log('Connection state:', transport.connectionState);
        }};

        await transport.ready;
        console.log('WebTransport ready!');

        const s = await transport.createBidirectionalStream();
        const writer = s.writable.getWriter();
        const reader = s.readable.getReader();

        const msg = 'hello from browser';
        console.log('Sending:', msg);
        await writer.write(new TextEncoder().encode(msg));

        const {{ value }} = await reader.read();
        console.log('Received:', new TextDecoder().decode(value));

        document.body.innerHTML += '<p style="color:green">SUCCESS! WebTransport works.</p>';
        transport.close();
    }} catch (err) {{
        console.error('Error:', err);
        document.body.innerHTML += `<p style="color:red">FAILED: ${{err.message}}</p>`;
    }}
}}
run();
</script>
</body>
</html>"#, hash = cert_hash_b64);

    let resp = http::Response::builder()
        .status(200)
        .header("content-type", "text/html")
        .body(())
        .unwrap();
    stream.send_response(resp).await?;
    stream.send_data(Bytes::from(html)).await?;
    stream.finish().await?;
    Ok(())
}

// === Main ===

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let addr: SocketAddr = "0.0.0.0:1443".parse()?;
    let (cert_pem, key_pem, cert_der) = generate_cert();

    // Compute cert hash for Chrome's serverCertificateHashes option.
    use sha2::Digest;
    let hash = sha2::Sha256::digest(&cert_der);
    use base64::Engine;
    let cert_hash_b64 = base64::engine::general_purpose::STANDARD.encode(hash);
    info!("Certificate SHA-256 (base64): {}", cert_hash_b64);

    let endpoint = create_endpoint(addr, &cert_pem, &key_pem)?;
    info!("Minimal WebTransport server listening on quic://{}", addr);
    info!("Open https://localhost:1443/ in Chrome to test");

    // Wrap in Arc so we can pass to the async handler.
    let cert_hash_b64 = Arc::new(cert_hash_b64);

    loop {
        let Some(incoming) = endpoint.accept().await else {
            break;
        };

        let cert_hash_b64 = Arc::clone(&cert_hash_b64);
        tokio::spawn(async move {
            match incoming.await {
                Ok(conn) => {
                    if let Err(err) = handle_connection(conn, cert_hash_b64.as_ref().clone()).await {
                        error!("connection error: {}", err);
                    }
                }
                Err(err) => {
                    error!("QUIC handshake error: {}", err);
                }
            }
        });
    }

    Ok(())
}
