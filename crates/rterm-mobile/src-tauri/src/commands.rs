use rterm_session::ManagedSession;
use rterm_session::SessionManager;
use rterm_transport::{PtyHandle, PtySpawner, SshAuth, SshConfig, SshTransport, Transport};
use serde::Serialize;
use std::sync::Arc;
use tauri::State;
use tokio::sync::mpsc;
use tracing::info;

/// Application state managed by Tauri.
pub struct AppState {
    pub session_mgr: Arc<SessionManager>,
    pub spawner: Arc<SshPtySpawner>,
}

/// Serializable session info for the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub name: String,
    pub cols: u16,
    pub rows: u16,
    pub last_activity_secs: u64,
}

/// Parses an SSH URI from a string.
///
/// Format: `ssh://user:password@host:port`
///    or:  `ssh://user@host:port` (no password)
///
/// Returns (hostname, port, username, password_opt) on success.
fn parse_ssh_uri(uri: &str) -> Option<(String, u16, String, Option<String>)> {
    let rest = uri.strip_prefix("ssh://")?;

    let (user_info, host_info) = rest.rsplit_once('@')?;

    let (username, password) = if let Some((u, p)) = user_info.split_once(':') {
        (u.to_string(), Some(p.to_string()))
    } else {
        (user_info.to_string(), None)
    };

    let (host, port) = if let Some((h, p)) = host_info.rsplit_once(':') {
        (h.to_string(), p.parse::<u16>().ok()?)
    } else {
        (host_info.to_string(), 22)
    };

    Some((host, port, username, password))
}

/// PtySpawner that creates SSH sessions instead of local PTYs.
///
/// Reuses the same pattern as rterm-agent: connects via SshTransport,
/// then bridges the async Transport into PtyHandle channels.
pub struct SshPtySpawner;

impl SshPtySpawner {
    /// Spawn a PTY using an existing SshConfig (supports key auth).
    pub fn spawn_with_config(
        &self,
        _name: &str,
        config: SshConfig,
    ) -> Result<PtyHandle, Box<dyn std::error::Error + Send + Sync>> {
        let host_display = format!("{}@{}:{}", config.username, config.hostname, config.port);
        info!("SSH connecting to {}", host_display);

        let mut transport = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(SshTransport::connect(config))
        })?;

        info!("SSH connected to {}", host_display);

        let (stdin_tx, mut stdin_rx) = mpsc::channel::<Vec<u8>>(64);
        let (stdout_tx, stdout_rx) = mpsc::channel::<Vec<u8>>(64);
        let (resize_tx, mut resize_rx) = mpsc::channel::<(u16, u16)>(8);

        tokio::spawn(async move {
            loop {
                tokio::select! {
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

impl PtySpawner for SshPtySpawner {
    fn spawn(
        &self,
        shell: &str,
        cols: u16,
        rows: u16,
    ) -> Result<PtyHandle, Box<dyn std::error::Error + Send + Sync>> {
        let (host, port, username, password) =
            parse_ssh_uri(shell).ok_or("invalid SSH URI: expected ssh://user:pass@host:port")?;

        let auth = match password {
            Some(p) => SshAuth::Password(p),
            None => return Err("password required (key auth not yet supported)".into()),
        };

        let config = SshConfig {
            hostname: host.clone(),
            port,
            username: username.clone(),
            auth,
            cols,
            rows,
        };

        self.spawn_with_config("", config)
    }
}

#[tauri::command]
pub async fn create_session(
    state: State<'_, AppState>,
    name: String,
    ssh_uri: String,
    cols: u16,
    rows: u16,
) -> Result<String, String> {
    state
        .session_mgr
        .get_or_create_with_shell(&name, &ssh_uri, cols, rows, state.spawner.as_ref())
        .await?;

    Ok(name)
}

/// Create a session with explicit SSH config (supports key auth).
#[allow(dead_code, clippy::too_many_arguments)]
#[tauri::command]
pub async fn create_session_with_auth(
    state: State<'_, AppState>,
    name: String,
    hostname: String,
    port: u16,
    username: String,
    auth_type: String,
    _password: Option<String>,
    key_pem: Option<String>,
    cols: u16,
    rows: u16,
) -> Result<String, String> {
    let (host, port, username, password) =
        parse_ssh_uri(&format!("ssh://{}@{}:{}", username, hostname, port))
            .ok_or("invalid SSH URI")?;

    let host_display = format!("{}@{}:{}", username, host, port);

    let auth = if auth_type == "key" {
        let pem = key_pem.ok_or("key_pem required for key auth")?;
        SshAuth::Key {
            private_key_pem: pem,
            passphrase: None,
        }
    } else {
        SshAuth::Password(password.ok_or("password required for password auth")?)
    };

    let config = SshConfig {
        hostname: host.clone(),
        port,
        username: username.clone(),
        auth,
        cols,
        rows,
    };

    tracing::info!("SSH connecting to {}", host_display);

    let spawner = state.spawner.as_ref();
    let pty_handle = spawner
        .spawn_with_config(&name, config)
        .map_err(|e| e.to_string())?;

    let ssh_uri = format!("ssh://{}@{}:{}", username, host, port);
    let (session, stdout_rx) =
        ManagedSession::from_pty(name.clone(), &ssh_uri, cols, rows, pty_handle);

    state
        .session_mgr
        .insert_session(&name, session, stdout_rx)
        .await;

    Ok(name)
}

#[tauri::command]
pub async fn list_sessions(state: State<'_, AppState>) -> Result<Vec<SessionInfo>, String> {
    let proto_sessions = state.session_mgr.list_sessions().await;
    let sessions = proto_sessions
        .into_iter()
        .map(|s| SessionInfo {
            name: s.name,
            cols: s.cols,
            rows: s.rows,
            last_activity_secs: s.last_activity,
        })
        .collect();
    Ok(sessions)
}

#[tauri::command]
pub async fn send_keys(
    state: State<'_, AppState>,
    session: String,
    data: String,
) -> Result<(), String> {
    let s = state
        .session_mgr
        .get(&session)
        .await
        .ok_or("session not found")?;
    let s = s.lock().await;
    s.pty_stdin_tx
        .send(data.into_bytes())
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_snapshot(state: State<'_, AppState>, session: String) -> Result<String, String> {
    let s = state
        .session_mgr
        .get(&session)
        .await
        .ok_or("session not found")?;
    let s = s.lock().await;
    Ok(s.plain_text())
}

/// Unified screen data response for the JS canvas renderer.
/// Uses "changes" as the field name for cell data (matching ScreenUpdateData).
#[derive(Debug, Clone, serde::Serialize)]
pub struct ScreenDataJson {
    cols: u16,
    rows: u16,
    cursor_row: u16,
    cursor_col: u16,
    cursor_visible: bool,
    cursor_style: u8,
    #[serde(rename = "changes")]
    cell_ranges: Vec<rterm_proto::CellRangeData>,
    mouse_tracking_mode: u8,
    alt_screen_active: bool,
    application_cursor_keys: bool,
}

#[tauri::command]
pub async fn get_screen_snapshot(
    state: State<'_, AppState>,
    session: String,
) -> Result<ScreenDataJson, String> {
    let s = state
        .session_mgr
        .get(&session)
        .await
        .ok_or("session not found")?;
    let s = s.lock().await;
    let snap = s.screen_snapshot();
    Ok(ScreenDataJson {
        cols: snap.cols,
        rows: snap.num_rows,
        cursor_row: snap.cursor.row,
        cursor_col: snap.cursor.col,
        cursor_visible: snap.cursor.visible,
        cursor_style: snap.cursor.style,
        cell_ranges: snap.rows,
        mouse_tracking_mode: snap.mouse_tracking_mode,
        alt_screen_active: snap.alt_screen_active,
        application_cursor_keys: snap.application_cursor_keys,
    })
}

#[tauri::command]
pub async fn kill_session(state: State<'_, AppState>, session: String) -> Result<(), String> {
    state.session_mgr.destroy(&session).await
}

#[tauri::command]
pub async fn resize_session(
    state: State<'_, AppState>,
    session: String,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    let s = state
        .session_mgr
        .get(&session)
        .await
        .ok_or("session not found")?;
    let mut s = s.lock().await;
    s.resize(cols, rows);
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
    fn parse_ssh_uri_password_with_at_sign() {
        let (host, port, user, pass) =
            parse_ssh_uri("ssh://root:p@ss:word@192.168.1.1:22").unwrap();
        assert_eq!(host, "192.168.1.1");
        assert_eq!(port, 22);
        assert_eq!(user, "root");
        assert_eq!(pass, Some("p@ss:word".to_string()));
    }
}
