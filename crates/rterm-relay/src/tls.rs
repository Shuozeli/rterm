use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{error, info};

type TlsResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Load or generate a persistent TLS certificate.
/// Saved to ~/.config/rterm/ so it survives restarts.
/// Regenerated if expired or SANs don't match current IPs.
pub fn load_or_generate_cert() -> TlsResult<(Vec<u8>, Vec<u8>)> {
    let config_dir = dirs_config_dir().join("rterm");
    let cert_path = config_dir.join("cert.pem");
    let key_path = config_dir.join("key.pem");

    // Try loading existing cert.
    if cert_path.exists()
        && key_path.exists()
        && let Ok(cert_pem) = std::fs::read(&cert_path)
        && let Ok(key_pem) = std::fs::read(&key_path)
    {
        if is_cert_file_fresh() {
            info!("using persistent cert from {}", cert_path.display());
            return Ok((cert_pem, key_pem));
        }
        info!("cert expired, regenerating");
    }

    // Generate new cert.
    let (cert_pem, key_pem) = generate_fresh_cert()?;

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

    Ok((cert_pem, key_pem))
}

fn generate_fresh_cert() -> TlsResult<(Vec<u8>, Vec<u8>)> {
    let mut sans = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ];
    if let Ok(output) = std::process::Command::new("hostname").arg("-I").output() {
        let ips = String::from_utf8_lossy(&output.stdout);
        for ip in ips.split_whitespace() {
            if !sans.contains(&ip.to_string()) {
                sans.push(ip.to_string());
            }
        }
    }

    use rcgen::{CertificateParams, KeyPair, PKCS_ECDSA_P256_SHA256};

    let mut params = CertificateParams::new(sans)?;
    let now = time::OffsetDateTime::now_utc();
    params.not_before = now;
    params.not_after = now + time::Duration::days(14);
    let key_pair = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)?;
    let cert = params.self_signed(&key_pair)?;
    Ok((
        cert.pem().into_bytes(),
        key_pair.serialize_pem().into_bytes(),
    ))
}

/// Check if the on-disk cert file is still fresh enough to use.
/// WebTransport certs must be valid for at most 14 days.
/// We regenerate after 12 days to avoid edge-case expiry.
fn is_cert_file_fresh() -> bool {
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

pub fn extract_cert_der(cert_pem: &[u8]) -> TlsResult<Vec<u8>> {
    let certs: Vec<_> = rustls_pemfile::certs(&mut std::io::BufReader::new(cert_pem))
        .collect::<Result<Vec<_>, _>>()?;
    certs
        .into_iter()
        .next()
        .map(|c| c.to_vec())
        .ok_or_else(|| "PEM contained no certificates".into())
}

pub fn create_endpoint(
    addr: SocketAddr,
    cert_pem: &[u8],
    key_pem: &[u8],
) -> Result<quinn::Endpoint, Box<dyn std::error::Error + Send + Sync>> {
    let certs: Vec<_> = rustls_pemfile::certs(&mut std::io::BufReader::new(cert_pem))
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
