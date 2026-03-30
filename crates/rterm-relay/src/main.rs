use h3::ext::Protocol;
use rterm_relay::{https_server, wt_handler};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{debug, error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let addr: SocketAddr = "[::]:4433".parse()?;
    let (cert_pem, key_pem) = load_or_generate_cert();

    // Compute cert hash for WebTransport serverCertificateHashes.
    let cert_der = extract_cert_der(&cert_pem);
    use sha2::Digest;
    let hash = sha2::Sha256::digest(&cert_der);
    use base64::Engine;
    let cert_hash_b64 = base64::engine::general_purpose::STANDARD.encode(hash);

    let static_dir = find_static_dir();
    info!("Serving static files from: {}", static_dir.display());

    // Start HTTPS page server on TCP:4433.
    // The cert hash is injected into index.html so the WASM client
    // can use serverCertificateHashes automatically.
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

    // Start WebTransport relay on UDP:4433.
    let endpoint = create_endpoint(addr, &cert_pem, &key_pem)?;
    let lan_ip = get_lan_ip();

    info!("rterm-relay listening on https://localhost:4433");
    if let Some(ip) = &lan_ip {
        info!("LAN: https://{}:4433", ip);
    }
    info!("Open the URL in Chrome, accept the certificate warning, and you're in.");

    loop {
        let Some(incoming) = endpoint.accept().await else {
            break;
        };

        tokio::spawn(async move {
            match incoming.await {
                Ok(conn) => {
                    let remote = conn.remote_address();
                    debug!("QUIC connection from {}", remote);
                    if let Err(e) = handle_connection(conn).await {
                        error!("connection error from {}: {}", remote, e);
                    }
                }
                Err(e) => error!("QUIC handshake error: {}", e),
            }
        });
    }

    Ok(())
}

async fn handle_connection(
    conn: quinn::Connection,
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
                    info!("WebTransport terminal session");
                    let session =
                        h3_webtransport::server::WebTransportSession::accept(req, stream, h3_conn)
                            .await?;
                    wt_handler::handle_wt_session(session, "/bin/bash").await?;
                    return Ok(());
                }

                // Non-WebTransport HTTP/3 request — not expected.
                debug!("rejecting: {} {}", req.method(), req.uri());
                let mut stream = stream;
                let resp = http::Response::builder().status(404).body(()).unwrap();
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

/// Find the WASM dist directory. Checks these locations in order:
/// 1. ./dist (if running from rterm-wasm directory)
/// 2. ../rterm-wasm/dist (relative to rterm-relay)
/// 3. crates/rterm-wasm/dist (from workspace root)
fn find_static_dir() -> PathBuf {
    let candidates = [
        PathBuf::from("dist"),
        PathBuf::from("crates/rterm-wasm/dist"),
        PathBuf::from("../rterm-wasm/dist"),
    ];
    for dir in &candidates {
        if dir.join("index.html").exists() {
            return dir.clone();
        }
    }
    // Default — will 404 on requests but won't crash.
    info!("WARNING: WASM dist directory not found. Build it with:");
    info!("  cd crates/rterm-wasm && RUSTFLAGS=\"--cfg web_sys_unstable_apis\" trunk build");
    PathBuf::from("dist")
}

/// Load or generate a persistent TLS certificate.
/// Saved to ~/.config/rterm/ so it survives restarts.
/// Regenerated if expired or SANs don't match current IPs.
fn load_or_generate_cert() -> (Vec<u8>, Vec<u8>) {
    let config_dir = dirs_config_dir().join("rterm");
    let cert_path = config_dir.join("cert.pem");
    let key_path = config_dir.join("key.pem");

    // Try loading existing cert.
    if cert_path.exists()
        && key_path.exists()
        && let Ok(cert_pem) = std::fs::read(&cert_path)
        && let Ok(key_pem) = std::fs::read(&key_path)
    {
        if is_cert_valid(&cert_pem) {
            info!("using persistent cert from {}", cert_path.display());
            return (cert_pem, key_pem);
        }
        info!("cert expired, regenerating");
    }

    // Generate new cert.
    let (cert_pem, key_pem) = generate_fresh_cert();

    // Save to disk.
    if let Err(e) = std::fs::create_dir_all(&config_dir) {
        error!("failed to create config dir: {}", e);
    } else {
        if let Err(e) = std::fs::write(&cert_path, &cert_pem) {
            error!("failed to save cert: {}", e);
        }
        if let Err(e) = std::fs::write(&key_path, &key_pem) {
            error!("failed to save key: {}", e);
        }
        // Restrict permissions on the key file.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600));
        }
        info!("saved cert to {}", config_dir.display());
    }

    (cert_pem, key_pem)
}

fn generate_fresh_cert() -> (Vec<u8>, Vec<u8>) {
    use rcgen::{CertificateParams, KeyPair, PKCS_ECDSA_P256_SHA256};

    let mut sans = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    if let Ok(output) = std::process::Command::new("hostname").arg("-I").output() {
        let ips = String::from_utf8_lossy(&output.stdout);
        for ip in ips.split_whitespace() {
            if !sans.contains(&ip.to_string()) {
                sans.push(ip.to_string());
            }
        }
    }

    let mut params = CertificateParams::new(sans).expect("failed to create certificate params");
    let now = time::OffsetDateTime::now_utc();
    params.not_before = now;
    params.not_after = now + time::Duration::days(14);
    let key_pair =
        KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).expect("failed to generate key pair");
    let cert = params
        .self_signed(&key_pair)
        .expect("failed to self-sign certificate");
    (
        cert.pem().into_bytes(),
        key_pair.serialize_pem().into_bytes(),
    )
}

/// Check if a PEM cert is still valid (not expired).
/// WebTransport certs must be valid for at most 14 days.
/// We regenerate after 12 days to avoid edge-case expiry.
fn is_cert_valid(_cert_pem: &[u8]) -> bool {
    let config_dir = dirs_config_dir().join("rterm");
    let cert_path = config_dir.join("cert.pem");
    let Ok(metadata) = std::fs::metadata(&cert_path) else {
        return false;
    };
    let Ok(modified) = metadata.modified() else {
        return false;
    };
    let age = std::time::SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default();
    // Regenerate if older than 12 days (cert valid for 14).
    age < std::time::Duration::from_secs(12 * 24 * 3600)
}

fn dirs_config_dir() -> PathBuf {
    if let Some(dir) = dirs_config_dir_impl() {
        dir
    } else {
        PathBuf::from(".")
    }
}

fn dirs_config_dir_impl() -> Option<PathBuf> {
    std::env::var("XDG_CONFIG_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".config"))
        })
}

fn extract_cert_der(cert_pem: &[u8]) -> Vec<u8> {
    let certs: Vec<_> = rustls_pemfile::certs(&mut std::io::BufReader::new(cert_pem))
        .collect::<Result<Vec<_>, _>>()
        .expect("failed to parse PEM certificate");
    certs[0].to_vec()
}

fn create_endpoint(
    addr: SocketAddr,
    cert_pem: &[u8],
    key_pem: &[u8],
) -> Result<quinn::Endpoint, Box<dyn std::error::Error + Send + Sync>> {
    let certs = rustls_pemfile::certs(&mut std::io::BufReader::new(cert_pem))
        .collect::<Result<Vec<_>, _>>()?;
    let key = rustls_pemfile::private_key(&mut std::io::BufReader::new(key_pem))?
        .ok_or("no private key found")?;

    let mut tls_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;
    tls_config.alpn_protocols = vec![b"h3".to_vec()];

    let server_config = quinn::ServerConfig::with_crypto(Arc::new(
        quinn::crypto::rustls::QuicServerConfig::try_from(tls_config)?,
    ));

    Ok(quinn::Endpoint::server(server_config, addr)?)
}

fn get_lan_ip() -> Option<String> {
    let output = std::process::Command::new("hostname")
        .arg("-I")
        .output()
        .ok()?;
    let ips = String::from_utf8_lossy(&output.stdout);
    ips.split_whitespace().next().map(|s| s.to_string())
}
