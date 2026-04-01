use crate::session_manager::SessionManager;
use crate::tls::create_endpoint;
use crate::{https_server, wt_handler};
use h3::ext::Protocol;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, error, info};

pub async fn start_webtransport_server(
    addr: SocketAddr,
    static_dir: PathBuf,
    cert_pem: Vec<u8>,
    key_pem: Vec<u8>,
    cert_hash_b64: String,
    session_mgr: Arc<SessionManager>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Start HTTPS page server on TCP.
    let https_addr = addr;
    let https_cert = cert_pem.clone();
    let https_key = key_pem.clone();
    let https_dir = static_dir.clone();
    let https_hash = cert_hash_b64.clone();

    tokio::spawn(async move {
        if let Err(e) =
            https_server::serve_https(https_addr, https_dir, https_cert, https_key, https_hash)
                .await
        {
            error!("HTTPS server error: {}", e);
        }
    });

    // Start WebTransport relay on UDP.
    let endpoint = create_endpoint(addr, &cert_pem, &key_pem)?;
    let lan_ip = crate::network::get_lan_ip();

    info!("rterm-relay listening on https://localhost:{}", addr.port());
    if let Some(ip) = &lan_ip {
        info!("LAN: https://{}:{}", ip, addr.port());
    }
    info!("Open the URL in Chrome, accept the certificate warning, and you're in.");

    tokio::spawn(async move {
        loop {
            let Some(incoming) = endpoint.accept().await else {
                break;
            };

            let session_mgr = Arc::clone(&session_mgr);
            tokio::spawn(async move {
                match incoming.await {
                    Ok(conn) => {
                        let remote = conn.remote_address();
                        debug!("QUIC connection from {}", remote);
                        if let Err(e) = handle_connection(conn, Arc::clone(&session_mgr)).await {
                            error!("connection error from {}: {}", remote, e);
                        }
                    }
                    Err(e) => error!("QUIC handshake error: {}", e),
                }
            });
        }
    });

    Ok(())
}

async fn handle_connection(
    conn: quinn::Connection,
    session_mgr: Arc<SessionManager>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

                let is_wt = req.method() == http::Method::CONNECT
                    && req
                        .extensions()
                        .get::<Protocol>()
                        .map(|p| p == &Protocol::WEB_TRANSPORT)
                        .unwrap_or(false);

                if is_wt {
                    let path = req.uri().path().to_string();
                    let session_name = path
                        .strip_prefix("/wt/")
                        .or_else(|| path.strip_prefix("/wt"))
                        .unwrap_or("")
                        .trim_matches('/');
                    let session_name = if session_name.is_empty() {
                        crate::session_manager::generate_session_name()
                    } else {
                        session_name.to_string()
                    };

                    info!("WebTransport session: {}", session_name);
                    let wt_session =
                        h3_webtransport::server::WebTransportSession::accept(req, stream, h3_conn)
                            .await?;
                    wt_handler::handle_wt_session(wt_session, &session_mgr, &session_name).await?;
                    return Ok(());
                }

                debug!("rejecting: {} {}", req.method(), req.uri());
                let mut stream = stream;
                let resp = http::Response::builder()
                    .status(404)
                    .body(())
                    .expect("valid HTTP response");
                stream.send_response(resp).await?;
                stream.finish().await?;
            }
            Ok(None) => break,
            Err(e) => {
                error!("h3 accept error: {}", e);
                break;
            }
        }
    }

    Ok(())
}
