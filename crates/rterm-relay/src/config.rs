use serde::Deserialize;
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_static_dir")]
    pub static_dir: PathBuf,

    /// Auth tokens for relay connections.
    /// If set, clients must provide `?token=<auth_token>` in the URL.
    /// Any token in the list grants access.
    #[serde(default)]
    pub auth_tokens: Vec<String>,

    #[serde(default = "default_listeners", rename = "listener")]
    pub listeners: Vec<ListenerConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListenerConfig {
    pub protocol: ProtocolType,
    pub port: u16,
    /// Optional bind address. Defaults to [::] (all interfaces).
    /// Examples: "100.95.116.72" (Tailscale), "127.0.0.1" (localhost only).
    #[serde(default)]
    pub bind: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProtocolType {
    Webtransport,
    GrpcH2,
    GrpcH3,
    WebSocket,
}

/// Transport type for the WASM client connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ClientTransport {
    #[default]
    WebTransport,
    WebSocket,
}

impl fmt::Display for ClientTransport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClientTransport::WebTransport => write!(f, "webtransport"),
            ClientTransport::WebSocket => write!(f, "websocket"),
        }
    }
}

impl std::str::FromStr for ClientTransport {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "webtransport" => Ok(ClientTransport::WebTransport),
            "websocket" => Ok(ClientTransport::WebSocket),
            _ => Err(format!(
                "unknown transport: {}. expected 'webtransport' or 'websocket'",
                s
            )),
        }
    }
}

pub fn find_static_dir() -> PathBuf {
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
    tracing::info!("WARNING: WASM dist directory not found. Build it with:");
    tracing::info!(
        "  cd crates/rterm-wasm && RUSTFLAGS=\"--cfg web_sys_unstable_apis\" trunk build"
    );
    PathBuf::from("dist")
}

fn default_static_dir() -> PathBuf {
    find_static_dir()
}

fn default_listeners() -> Vec<ListenerConfig> {
    vec![
        ListenerConfig {
            protocol: ProtocolType::Webtransport,
            port: 4433,
            bind: None,
        },
        ListenerConfig {
            protocol: ProtocolType::GrpcH2,
            port: 4434,
            bind: None,
        },
        ListenerConfig {
            protocol: ProtocolType::WebSocket,
            port: 4435,
            bind: None,
        },
    ]
}

impl Config {
    pub fn load_from_file<P: AsRef<Path>>(
        path: P,
    ) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let content = std::fs::read_to_string(path)?;
        let config = toml::from_str(&content)?;
        Ok(config)
    }

    pub fn default_config() -> Self {
        Self {
            static_dir: default_static_dir(),
            auth_tokens: Vec::new(),
            listeners: default_listeners(),
        }
    }
}
