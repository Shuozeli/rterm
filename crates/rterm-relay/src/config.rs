use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_static_dir")]
    pub static_dir: PathBuf,

    #[serde(default = "default_listeners", rename = "listener")]
    pub listeners: Vec<ListenerConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ListenerConfig {
    pub protocol: ProtocolType,
    pub port: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProtocolType {
    Webtransport,
    GrpcH2,
    GrpcH3,
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
        },
        ListenerConfig {
            protocol: ProtocolType::GrpcH2,
            port: 4434,
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
            listeners: default_listeners(),
        }
    }
}
