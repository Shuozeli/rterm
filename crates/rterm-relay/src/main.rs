use clap::Parser;
use grpc_server::{H3Server, NamedService, Router, Server};
use rterm_relay::config::{ClientTransport, Config, find_static_dir};
use rterm_relay::service::TerminalServer;
use rterm_relay::session_manager::SessionManager;
use rterm_relay::tls::{extract_cert_der, load_or_generate_cert};
use rterm_relay::ws_server::start_websocket_server;
use rterm_relay::wt_server::start_webtransport_server;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

#[derive(Parser)]
struct Cli {
    #[arg(short, long)]
    config: Option<PathBuf>,
    /// Transport type for the WASM client: "webtransport" or "websocket".
    /// Must match how the WASM client was built (via cargo features).
    #[arg(long, default_value = "webtransport", value_parser = clap::value_parser!(ClientTransport))]
    transport: ClientTransport,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();

    let config = if let Some(path) = cli.config {
        Config::load_from_file(path)?
    } else if Path::new("rterm.toml").exists() {
        Config::load_from_file("rterm.toml")?
    } else {
        info!("No rterm.toml found, using default config");
        Config::default_config()
    };

    let (cert_pem, key_pem) = load_or_generate_cert()?;

    // Compute cert hash for WebTransport serverCertificateHashes.
    let cert_der = extract_cert_der(&cert_pem)?;
    use sha2::Digest;
    let hash = sha2::Sha256::digest(&cert_der);
    use base64::Engine;
    let cert_hash_b64 = base64::engine::general_purpose::STANDARD.encode(hash);

    let static_dir = if config.static_dir.exists() {
        config.static_dir.clone()
    } else {
        find_static_dir()
    };
    info!("Serving static files from: {}", static_dir.display());

    // Create the session manager.
    let session_mgr = Arc::new(SessionManager::new("/bin/bash"));

    // Start the timeout reaper (every 60 seconds, kill sessions detached > 30 min).
    let reaper_mgr = Arc::clone(&session_mgr);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            reaper_mgr.reap(1800).await; // 30 min
        }
    });

    for listener_cfg in config.listeners {
        let bind_addr = listener_cfg.bind.as_deref().unwrap_or("::");
        let addr = format!("{}:{}", bind_addr, listener_cfg.port).parse()?;

        match listener_cfg.protocol {
            rterm_relay::config::ProtocolType::Webtransport => {
                let static_dir = static_dir.clone();
                let cert_pem = cert_pem.clone();
                let key_pem = key_pem.clone();
                let cert_hash_b64 = cert_hash_b64.clone();
                let auth_tokens = config.auth_tokens.clone();
                let session_mgr = Arc::clone(&session_mgr);
                let transport = cli.transport;

                tokio::spawn(async move {
                    if let Err(e) = start_webtransport_server(
                        addr,
                        static_dir,
                        cert_pem,
                        key_pem,
                        cert_hash_b64,
                        transport,
                        auth_tokens,
                        session_mgr,
                    )
                    .await
                    {
                        error!(
                            "WebTransport server error on port {}: {}",
                            listener_cfg.port, e
                        );
                    }
                });
            }
            rterm_relay::config::ProtocolType::GrpcH2 => {
                let router = Router::new().add_service(
                    TerminalServer::NAME,
                    TerminalServer::new(Arc::clone(&session_mgr)),
                );
                let cert = cert_pem.clone();
                let key = key_pem.clone();
                let port = listener_cfg.port;
                tokio::spawn(async move {
                    info!("gRPC HTTPS (H2) on port {}", port);
                    let svc = match Server::builder()
                        .timeout(Duration::from_secs(30))
                        .tls(&cert, &key)
                    {
                        Ok(s) => s,
                        Err(e) => {
                            error!("TLS config failed for gRPC H2 on port {}: {}", port, e);
                            return;
                        }
                    };
                    if let Err(e) = svc.serve(addr, router).await {
                        error!("gRPC H2 server error on port {}: {}", port, e);
                    }
                });
            }
            rterm_relay::config::ProtocolType::GrpcH3 => {
                let router = Router::new().add_service(
                    TerminalServer::NAME,
                    TerminalServer::new(Arc::clone(&session_mgr)),
                );
                let cert = cert_pem.clone();
                let key = key_pem.clone();
                let port = listener_cfg.port;
                tokio::spawn(async move {
                    info!("gRPC H3 on port {}", port);
                    let endpoint = match H3Server::bind(addr, &cert, &key) {
                        Ok(e) => e,
                        Err(e) => {
                            error!("H3 bind failed on port {}: {}", port, e);
                            return;
                        }
                    };
                    if let Err(e) = H3Server::builder().serve_endpoint(endpoint, router).await {
                        error!("gRPC H3 server error on port {}: {}", port, e);
                    }
                });
            }
            rterm_relay::config::ProtocolType::WebSocket => {
                let cert_pem = cert_pem.clone();
                let key_pem = key_pem.clone();
                let auth_tokens = config.auth_tokens.clone();
                let session_mgr = Arc::clone(&session_mgr);
                tokio::spawn(async move {
                    if let Err(e) =
                        start_websocket_server(addr, cert_pem, key_pem, auth_tokens, session_mgr)
                            .await
                    {
                        error!(
                            "WebSocket server error on port {}: {}",
                            listener_cfg.port, e
                        );
                    }
                });
            }
        }
    }

    // Park the main thread
    std::future::pending::<()>().await;
    Ok(())
}
