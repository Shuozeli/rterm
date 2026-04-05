//! rterm-agent: SSH terminal sessions over localhost gRPC.
//!
//! This binary is the "client mode" of rterm. It runs on the user's device,
//! connects to remote hosts via SSH, and exposes the same gRPC TerminalService
//! API as rterm-relay. Flutter (or any gRPC client) connects to it on localhost.
//!
//! ## SSH connection convention
//!
//! The `shell` field in CreateSessionRequest encodes the SSH target as a URI:
//!
//!   ssh://user:password@host:port
//!   ssh://user@host:port          (key-based auth, uses default key)
//!
//! If no SSH URI is provided in `shell`, the agent falls back to the default
//! SSH target configured via CLI flags (--ssh-host, --ssh-user, etc.).

use clap::Parser;
use grpc_server::{NamedService, Router, Server};
use rterm_service::TerminalServer;
use rterm_session::SessionManager;
use rterm_transport::{PtyHandle, PtySpawner, SshAuth, SshConfig, SshTransport, Transport};
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tracing::{error, info};

#[derive(Parser)]
#[command(name = "rterm-agent", about = "SSH terminal agent with localhost gRPC")]
struct Cli {
    /// Port to listen on (0 = OS picks a free port).
    #[arg(long, default_value = "0")]
    port: u16,

    /// Default SSH host to connect to.
    #[arg(long, default_value = "127.0.0.1")]
    ssh_host: String,

    /// Default SSH port.
    #[arg(long, default_value = "22")]
    ssh_port: u16,

    /// Default SSH username.
    #[arg(long, default_value = "root")]
    ssh_user: String,

    /// Default SSH password (if not using key auth).
    #[arg(long)]
    ssh_password: Option<String>,
}

/// Parses an SSH URI from the `shell` field of CreateSessionRequest.
///
/// Format: `ssh://user:password@host:port`
///    or:  `ssh://user@host:port` (password-less, falls back to default)
///
/// Returns (hostname, port, username, password_opt) on success.
fn parse_ssh_uri(uri: &str) -> Option<(String, u16, String, Option<String>)> {
    let rest = uri.strip_prefix("ssh://")?;

    // Split user_info from host_info at '@'
    let (user_info, host_info) = rest.rsplit_once('@')?;

    // Parse user:password or just user
    let (username, password) = if let Some((u, p)) = user_info.split_once(':') {
        (u.to_string(), Some(p.to_string()))
    } else {
        (user_info.to_string(), None)
    };

    // Parse host:port
    let (host, port) = if let Some((h, p)) = host_info.rsplit_once(':') {
        (h.to_string(), p.parse::<u16>().ok()?)
    } else {
        (host_info.to_string(), 22)
    };

    Some((host, port, username, password))
}

/// PtySpawner implementation that creates SSH sessions instead of local PTYs.
///
/// When `spawn()` is called, it connects to the SSH server, opens a PTY channel,
/// and bridges the SSH transport into the same channel-based PtyHandle that
/// the rest of the rterm-session/rterm-service stack expects.
struct SshPtySpawner {
    default_host: String,
    default_port: u16,
    default_user: String,
    default_password: Option<String>,
}

impl SshPtySpawner {
    fn new(cli: &Cli) -> Self {
        Self {
            default_host: cli.ssh_host.clone(),
            default_port: cli.ssh_port,
            default_user: cli.ssh_user.clone(),
            default_password: cli.ssh_password.clone(),
        }
    }

    /// Resolve SSH config from the shell parameter or fall back to defaults.
    fn resolve_ssh_config(
        &self,
        shell: &str,
        cols: u16,
        rows: u16,
    ) -> Result<SshConfig, Box<dyn std::error::Error + Send + Sync>> {
        if let Some((host, port, user, password)) = parse_ssh_uri(shell) {
            let auth = match password {
                Some(p) => SshAuth::Password(p),
                None => match &self.default_password {
                    Some(p) => SshAuth::Password(p.clone()),
                    None => {
                        return Err("no password provided and key auth not yet supported".into());
                    }
                },
            };
            Ok(SshConfig {
                hostname: host,
                port,
                username: user,
                auth,
                cols,
                rows,
            })
        } else {
            // Use defaults
            let auth = match &self.default_password {
                Some(p) => SshAuth::Password(p.clone()),
                None => {
                    return Err(
                        "no SSH password configured (use --ssh-password or ssh:// URI)".into(),
                    );
                }
            };
            Ok(SshConfig {
                hostname: self.default_host.clone(),
                port: self.default_port,
                username: self.default_user.clone(),
                auth,
                cols,
                rows,
            })
        }
    }
}

impl PtySpawner for SshPtySpawner {
    fn spawn(
        &self,
        shell: &str,
        cols: u16,
        rows: u16,
    ) -> Result<PtyHandle, Box<dyn std::error::Error + Send + Sync>> {
        let config = self.resolve_ssh_config(shell, cols, rows)?;

        let host_display = format!("{}@{}:{}", config.username, config.hostname, config.port);
        info!("SSH connecting to {}", host_display);

        // Run the async SSH connect synchronously. block_in_place allows blocking
        // in a tokio multi-thread runtime without deadlocking.
        let mut transport = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(SshTransport::connect(config))
        })?;

        info!("SSH connected to {}", host_display);

        // Bridge the async Transport into PtyHandle channels.
        let (stdin_tx, mut stdin_rx) = mpsc::channel::<Vec<u8>>(64);
        let (stdout_tx, stdout_rx) = mpsc::channel::<Vec<u8>>(64);
        let (resize_tx, mut resize_rx) = mpsc::channel::<(u16, u16)>(8);

        // Spawn a task that reads from SSH and forwards to stdout channel.
        // Also handles stdin writes and resize.
        //
        // We need to split the transport usage: reads in one task, writes in another.
        // But Transport takes &mut self for both. So we use a single task that
        // multiplexes reads, writes, and resizes via select.
        //
        // However, Transport::read() is a long-lived blocking call. We need to
        // run it concurrently with writes. The solution: use the underlying
        // channels that SshTransport already has (data_rx for reads, write_half
        // for writes). But those are private.
        //
        // Alternative: spawn read and write in separate tasks using channels as
        // intermediaries. The read side of SshTransport runs in one task.
        // For writes, we send through a channel to a write task.
        //
        // Actually, the simplest approach: one tokio task that does select! on
        // transport.read(), stdin_rx.recv(), and resize_rx.recv(). Since read()
        // is cancel-safe (it's just mpsc recv internally), this works.

        tokio::spawn(async move {
            loop {
                tokio::select! {
                    // Read from SSH -> forward to stdout
                    read_result = transport.read() => {
                        match read_result {
                            Ok(data) => {
                                if stdout_tx.send(data).await.is_err() {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    // Read from stdin channel -> write to SSH
                    stdin_data = stdin_rx.recv() => {
                        match stdin_data {
                            Some(data) => {
                                if transport.write(&data).await.is_err() {
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                    // Handle resize
                    resize = resize_rx.recv() => {
                        match resize {
                            Some((c, r)) => {
                                if transport.resize(c, r).await.is_err() {
                                    break;
                                }
                            }
                            None => break,
                        }
                    }
                }
            }
            let _ = transport.close().await;
        });

        Ok(PtyHandle {
            stdin_tx,
            stdout_rx,
            resize_tx,
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();

    let session_mgr = Arc::new(SessionManager::new("ssh"));
    let spawner = Arc::new(SshPtySpawner::new(&cli));

    let terminal_server = TerminalServer::with_spawner("ssh", Arc::clone(&session_mgr), spawner);

    let router = Router::new().add_service(TerminalServer::NAME, terminal_server);

    // Bind to localhost only. Port 0 = OS picks a free port.
    let addr = format!("127.0.0.1:{}", cli.port);
    let listener = TcpListener::bind(&addr).await?;
    let local_addr = listener.local_addr()?;

    // Print PORT= to stdout so the parent process (Flutter) can read the actual port.
    println!("PORT={}", local_addr.port());

    info!("rterm-agent gRPC server listening on {}", local_addr);

    // Start the timeout reaper (every 60s, kill sessions detached > 30 min).
    let reaper_mgr = Arc::clone(&session_mgr);
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            reaper_mgr.reap(1800).await;
        }
    });

    // Serve gRPC (plaintext H2 on localhost).
    if let Err(e) = Server::builder()
        .timeout(Duration::from_secs(30))
        .serve_with_listener(listener, router)
        .await
    {
        error!("gRPC server error: {}", e);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ssh_uri_full() {
        let (host, port, user, pass) = parse_ssh_uri("ssh://admin:secret@10.0.0.1:2222").unwrap();
        assert_eq!(host, "10.0.0.1");
        assert_eq!(port, 2222);
        assert_eq!(user, "admin");
        assert_eq!(pass, Some("secret".to_string()));
    }

    #[test]
    fn parse_ssh_uri_no_password() {
        let (host, port, user, pass) = parse_ssh_uri("ssh://deploy@example.com:22").unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 22);
        assert_eq!(user, "deploy");
        assert_eq!(pass, None);
    }

    #[test]
    fn parse_ssh_uri_default_port() {
        let (host, port, user, pass) = parse_ssh_uri("ssh://user@myhost").unwrap();
        assert_eq!(host, "myhost");
        assert_eq!(port, 22);
        assert_eq!(user, "user");
        assert_eq!(pass, None);
    }

    #[test]
    fn parse_ssh_uri_invalid() {
        assert!(parse_ssh_uri("not-ssh://foo@bar").is_none());
        assert!(parse_ssh_uri("/bin/bash").is_none());
        assert!(parse_ssh_uri("").is_none());
    }

    #[test]
    fn parse_ssh_uri_password_with_special_chars() {
        let (host, port, user, pass) =
            parse_ssh_uri("ssh://root:p@ss:word@192.168.1.1:22").unwrap();
        assert_eq!(host, "192.168.1.1");
        assert_eq!(port, 22);
        assert_eq!(user, "root");
        // Note: rsplit_once('@') gets the LAST '@', so "root:p@ss:word" is user_info
        // and "p@ss:word" contains the password portion "p@ss:word".
        // split_once(':') on "root:p@ss:word" gives user="root", pass="p@ss:word"
        assert_eq!(pass, Some("p@ss:word".to_string()));
    }

    #[test]
    fn ssh_pty_spawner_resolve_config_from_uri() {
        let cli = Cli {
            port: 0,
            ssh_host: "default.host".into(),
            ssh_port: 22,
            ssh_user: "defaultuser".into(),
            ssh_password: Some("defaultpass".into()),
        };
        let spawner = SshPtySpawner::new(&cli);
        let config = spawner
            .resolve_ssh_config("ssh://admin:mysecret@10.0.0.5:2222", 80, 24)
            .unwrap();
        assert_eq!(config.hostname, "10.0.0.5");
        assert_eq!(config.port, 2222);
        assert_eq!(config.username, "admin");
        assert!(matches!(config.auth, SshAuth::Password(ref p) if p == "mysecret"));
        assert_eq!(config.cols, 80);
        assert_eq!(config.rows, 24);
    }

    #[test]
    fn ssh_pty_spawner_resolve_config_defaults() {
        let cli = Cli {
            port: 0,
            ssh_host: "myserver.local".into(),
            ssh_port: 2222,
            ssh_user: "myuser".into(),
            ssh_password: Some("mypass".into()),
        };
        let spawner = SshPtySpawner::new(&cli);
        let config = spawner.resolve_ssh_config("/bin/bash", 120, 40).unwrap();
        assert_eq!(config.hostname, "myserver.local");
        assert_eq!(config.port, 2222);
        assert_eq!(config.username, "myuser");
        assert!(matches!(config.auth, SshAuth::Password(ref p) if p == "mypass"));
    }

    #[test]
    fn ssh_pty_spawner_no_password_errors() {
        let cli = Cli {
            port: 0,
            ssh_host: "host".into(),
            ssh_port: 22,
            ssh_user: "user".into(),
            ssh_password: None,
        };
        let spawner = SshPtySpawner::new(&cli);
        let result = spawner.resolve_ssh_config("/bin/bash", 80, 24);
        assert!(result.is_err());
    }

    #[test]
    fn ssh_pty_spawner_uri_no_password_uses_default() {
        let cli = Cli {
            port: 0,
            ssh_host: "host".into(),
            ssh_port: 22,
            ssh_user: "user".into(),
            ssh_password: Some("fallback".into()),
        };
        let spawner = SshPtySpawner::new(&cli);
        let config = spawner
            .resolve_ssh_config("ssh://admin@remote:22", 80, 24)
            .unwrap();
        assert_eq!(config.username, "admin");
        assert_eq!(config.hostname, "remote");
        assert!(matches!(config.auth, SshAuth::Password(ref p) if p == "fallback"));
    }
}
