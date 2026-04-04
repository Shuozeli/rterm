use crate::session_manager::SessionManager;
use crate::ws_handler::handle_ws_session;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls;
use tracing::{error, info};

pub async fn start_websocket_server(
    addr: SocketAddr,
    cert_pem: Vec<u8>,
    key_pem: Vec<u8>,
    auth_tokens: Vec<String>,
    session_mgr: Arc<SessionManager>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let certs = rustls_pemfile::certs(&mut std::io::BufReader::new(&cert_pem[..]))
        .collect::<Result<Vec<_>, _>>()?;
    let key = rustls_pemfile::private_key(&mut std::io::BufReader::new(&key_pem[..]))?
        .ok_or("no private key")?;

    let mut tls_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;
    tls_config.alpn_protocols = vec![b"http/1.1".to_vec()];

    let tls_acceptor = TlsAcceptor::from(Arc::new(tls_config));
    let listener = TcpListener::bind(addr).await?;
    info!("WebSocket server on wss://{}", addr);

    loop {
        let (stream, remote) = listener.accept().await?;
        let acceptor = TlsAcceptor::clone(&tls_acceptor);
        let mgr = Arc::clone(&session_mgr);
        let auth_tokens = auth_tokens.clone();

        tokio::spawn(async move {
            let tls_stream = match acceptor.accept(stream).await {
                Ok(s) => s,
                Err(e) => {
                    error!("TLS handshake error from {}: {}", remote, e);
                    return;
                }
            };

            // Use accept_hdr_async to inspect the request and validate token.
            let auth_tokens = auth_tokens.clone();
            let ws_stream = match tokio_tungstenite::accept_hdr_async(
                tls_stream,
                #[allow(clippy::result_large_err)]
                move |req: &http::Request<()>, res: http::Response<()>| {
                    // If auth_tokens is configured, validate token from query params.
                    if !auth_tokens.is_empty() {
                        let uri = req.uri();
                        let query = uri.query();
                        let valid = query
                            .map(|q| {
                                q.split('&').any(|param| {
                                    param
                                        .strip_prefix("token=")
                                        .map(|v| auth_tokens.contains(&v.to_string()))
                                        .unwrap_or(false)
                                })
                            })
                            .unwrap_or(false);

                        if !valid {
                            error!("WebSocket auth failed from {}: invalid token", remote);
                            let resp = http::Response::builder()
                                .status(401)
                                .body(Some("Unauthorized".to_string()))
                                .unwrap();
                            return Err(resp);
                        }
                    }
                    // No auth required.
                    Ok(res)
                },
            )
            .await
            {
                Ok(ws) => ws,
                Err(e) => {
                    // Auth error already logged above
                    error!("WebSocket handshake error from {}: {}", remote, e);
                    return;
                }
            };

            info!("WebSocket connection from {}", remote);

            // The handler reads the first client message for session name (if not in URL).
            if let Err(e) = handle_ws_session(ws_stream, &mgr, "").await {
                error!("WebSocket session error from {}: {}", remote, e);
            }
        });
    }
}
