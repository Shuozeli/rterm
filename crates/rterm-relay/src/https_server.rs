/// Simple HTTPS server for serving the WASM page over TCP/TLS.
/// This allows Chrome to accept the self-signed cert via the normal warning dialog.
use crate::static_files::{guess_content_type, resolve_path};
use hyper::body::Incoming;
use hyper_util::rt::{TokioExecutor, TokioIo};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{debug, info};

pub async fn serve_https(
    addr: SocketAddr,
    static_dir: PathBuf,
    cert_pem: Vec<u8>,
    key_pem: Vec<u8>,
    cert_hash_b64: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let certs = rustls_pemfile::certs(&mut std::io::BufReader::new(&cert_pem[..]))
        .collect::<Result<Vec<_>, _>>()?;
    let key = rustls_pemfile::private_key(&mut std::io::BufReader::new(&key_pem[..]))?
        .ok_or("no private key")?;

    let mut tls_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;
    tls_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    let tls_acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(tls_config));
    let listener = TcpListener::bind(addr).await?;
    info!("HTTPS page server on https://{}", addr);

    let static_dir = Arc::new(static_dir);
    let cert_hash_b64 = Arc::new(cert_hash_b64);

    loop {
        let (stream, _remote) = listener.accept().await?;
        let acceptor = tls_acceptor.clone();
        let dir = Arc::clone(&static_dir);
        let hash = Arc::clone(&cert_hash_b64);

        tokio::spawn(async move {
            let tls_stream = match acceptor.accept(stream).await {
                Ok(s) => s,
                Err(e) => {
                    debug!("TLS handshake error: {}", e);
                    return;
                }
            };

            let service = hyper::service::service_fn(move |req: hyper::Request<Incoming>| {
                let dir = Arc::clone(&dir);
                let hash = Arc::clone(&hash);
                async move { serve_static_hyper(&dir, req.uri().path(), &hash).await }
            });

            let io = TokioIo::new(tls_stream);
            if let Err(e) = hyper_util::server::conn::auto::Builder::new(TokioExecutor::new())
                .serve_connection(io, service)
                .await
            {
                debug!("HTTP connection error: {}", e);
            }
        });
    }
}

async fn serve_static_hyper(
    static_dir: &Path,
    uri_path: &str,
    cert_hash_b64: &str,
) -> Result<hyper::Response<http_body_util::Full<bytes::Bytes>>, std::convert::Infallible> {
    let file_path = resolve_path(uri_path, static_dir);

    match tokio::fs::read(&file_path).await {
        Ok(data) => {
            let content_type = guess_content_type(&file_path);

            // For HTML files, inject the cert hash as a global JS variable
            // so the WASM client can use it for serverCertificateHashes.
            let data = if content_type.starts_with("text/html") {
                let html = String::from_utf8_lossy(&data);
                let inject = format!(
                    r#"<script>window.__RTERM_CERT_HASH__ = "{}";</script>"#,
                    cert_hash_b64
                );
                let injected = html.replace("</head>", &format!("{}</head>", inject));
                bytes::Bytes::from(injected)
            } else {
                bytes::Bytes::from(data)
            };

            Ok(hyper::Response::builder()
                .status(200)
                .header("content-type", content_type)
                .header("cache-control", "no-cache")
                .body(http_body_util::Full::new(data))
                .unwrap())
        }
        Err(_) => Ok(hyper::Response::builder()
            .status(404)
            .body(http_body_util::Full::new(bytes::Bytes::from("Not Found")))
            .unwrap()),
    }
}
