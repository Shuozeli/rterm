/// SessionManager: creates, tracks, and manages terminal sessions.
use crate::managed_session::{ManagedSession, SessionState, session_output_loop};
use crate::pty::PtySpawner;
use rterm_proto::*;
use sha2::Digest;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info};

pub struct SessionManager {
    sessions: Arc<Mutex<HashMap<String, Arc<Mutex<ManagedSession>>>>>,
    default_shell: String,
}

impl SessionManager {
    pub fn new(default_shell: &str) -> Self {
        Self {
            sessions: Arc::new(Mutex::new(HashMap::new())),
            default_shell: default_shell.to_string(),
        }
    }

    /// Create a new session. Returns (session_id, session_name, token).
    pub async fn create(
        &self,
        name: Option<String>,
        shell: Option<String>,
        cols: u16,
        rows: u16,
        spawner: &dyn PtySpawner,
    ) -> Result<(String, String, String), String> {
        let session_id = uuid_v4();
        let session_name = name.unwrap_or_else(generate_name);
        let shell = shell.unwrap_or_else(|| self.default_shell.clone());
        let token = generate_token();
        let token_hash = hash_token(&token);

        let (session, stdout_rx) = ManagedSession::new(
            session_id.clone(),
            session_name.clone(),
            token_hash,
            &shell,
            cols,
            rows,
            spawner,
        )
        .map_err(|e| format!("spawn failed: {e}"))?;

        let session = Arc::new(Mutex::new(session));

        // Start the output loop (runs independently of client).
        let session_loop = Arc::clone(&session);
        tokio::spawn(async move {
            session_output_loop(session_loop, stdout_rx).await;
        });

        // Register the session.
        self.sessions
            .lock()
            .await
            .insert(session_id.clone(), session);

        info!(
            "session created: id={}, name={}, shell={}",
            session_id, session_name, shell
        );

        Ok((session_id, session_name, token))
    }

    /// Attach to an existing session. Validates the token.
    /// Returns the session Arc (caller sends ScreenSnapshot).
    pub async fn attach(
        &self,
        session_id: &str,
        token: &str,
    ) -> Result<Arc<Mutex<ManagedSession>>, String> {
        let sessions = self.sessions.lock().await;
        let session = sessions
            .get(session_id)
            .ok_or_else(|| format!("session not found: {}", session_id))?;

        let s = session.lock().await;
        let token_hash = hash_token(token);
        if s.token_hash != token_hash {
            return Err("invalid token".into());
        }
        if s.state == SessionState::Dead {
            return Err("session is dead".into());
        }
        drop(s);

        Ok(Arc::clone(session))
    }

    /// Destroy a session by ID.
    pub async fn destroy(&self, session_id: &str) -> Result<(), String> {
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.remove(session_id) {
            let mut s = session.lock().await;
            s.mark_dead(0);
            s.detach();
            info!("session destroyed: {}", session_id);
            Ok(())
        } else {
            Err("session not found".into())
        }
    }

    /// List sessions that match the provided tokens.
    pub async fn list(&self, tokens: &[String]) -> Vec<SessionInfo> {
        let sessions = self.sessions.lock().await;
        let token_hashes: Vec<[u8; 32]> = tokens.iter().map(|t| hash_token(t)).collect();

        sessions
            .values()
            .filter_map(|session| {
                let s = session.try_lock().ok()?;
                if !token_hashes.iter().any(|h| *h == s.token_hash) {
                    return None;
                }
                Some(SessionInfo {
                    session_id: s.id.clone(),
                    name: s.name.clone(),
                    shell: s.shell.clone(),
                    created_at: s.created_at.elapsed().as_millis().wrapping_neg() as u64, // approximate
                    last_activity: s.last_activity.elapsed().as_millis() as u64,
                    attached: s.state == SessionState::Attached,
                    cols: s.cols,
                    rows: s.rows,
                    title: None,
                })
            })
            .collect()
    }

    /// Reap timed-out sessions. Called periodically.
    pub async fn reap(&self, max_detach_secs: u64) {
        let mut sessions = self.sessions.lock().await;
        let mut to_remove = Vec::new();

        for (id, session) in sessions.iter() {
            if let Ok(s) = session.try_lock()
                && (s.is_timed_out(max_detach_secs) || s.state == SessionState::Dead)
            {
                to_remove.push(id.clone());
            }
        }

        for id in &to_remove {
            if let Some(session) = sessions.remove(id) {
                if let Ok(mut s) = session.try_lock() {
                    s.mark_dead(0);
                    s.detach();
                }
                debug!("reaped session: {}", id);
            }
        }

        if !to_remove.is_empty() {
            info!("reaped {} sessions", to_remove.len());
        }
    }

    /// Get the number of active sessions.
    pub async fn session_count(&self) -> usize {
        self.sessions.lock().await.len()
    }
}

fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let random: u64 = rand_u64();
    format!("{:016x}-{:016x}", now, random)
}

fn rand_u64() -> u64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let s = RandomState::new();
    let mut h = s.build_hasher();
    h.write_u8(0);
    h.finish()
}

fn generate_name() -> String {
    let adjectives = [
        "swift", "quiet", "bold", "warm", "keen", "calm", "dark", "fair",
    ];
    let nouns = ["fox", "owl", "elm", "oak", "bay", "sky", "ash", "ivy"];
    let idx1 = (rand_u64() % adjectives.len() as u64) as usize;
    let idx2 = (rand_u64() % nouns.len() as u64) as usize;
    let num = rand_u64() % 100;
    format!("{}-{}-{}", adjectives[idx1], nouns[idx2], num)
}

fn generate_token() -> String {
    use base64::Engine;
    let mut bytes = [0u8; 32];
    for b in &mut bytes {
        *b = (rand_u64() & 0xFF) as u8;
    }
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn hash_token(token: &str) -> [u8; 32] {
    let hash = sha2::Sha256::digest(token.as_bytes());
    let mut result = [0u8; 32];
    result.copy_from_slice(&hash);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pty::fake::FakePtySpawner;

    #[tokio::test]
    async fn create_session() {
        let spawner = FakePtySpawner::new();
        let mgr = SessionManager::new("/bin/bash");
        let (id, name, token) = mgr
            .create(Some("test".into()), None, 80, 24, &spawner)
            .await
            .unwrap();
        assert!(!id.is_empty());
        assert_eq!(name, "test");
        assert!(!token.is_empty());
        assert_eq!(mgr.session_count().await, 1);
    }

    #[tokio::test]
    async fn create_auto_name() {
        let spawner = FakePtySpawner::new();
        let mgr = SessionManager::new("/bin/bash");
        let (_, name, _) = mgr.create(None, None, 80, 24, &spawner).await.unwrap();
        assert!(!name.is_empty());
        // Auto-generated names have format "adjective-noun-number".
        assert!(name.contains('-'));
    }

    #[tokio::test]
    async fn attach_with_valid_token() {
        let spawner = FakePtySpawner::new();
        let mgr = SessionManager::new("/bin/bash");
        let (id, _, token) = mgr.create(None, None, 80, 24, &spawner).await.unwrap();

        let session = mgr.attach(&id, &token).await.unwrap();
        let s = session.lock().await;
        assert_eq!(s.id, id);
    }

    #[tokio::test]
    async fn attach_with_invalid_token() {
        let spawner = FakePtySpawner::new();
        let mgr = SessionManager::new("/bin/bash");
        let (id, _, _) = mgr.create(None, None, 80, 24, &spawner).await.unwrap();

        let result = mgr.attach(&id, "wrong-token").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn attach_nonexistent_session() {
        let mgr = SessionManager::new("/bin/bash");
        let result = mgr.attach("nonexistent", "token").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn destroy_session() {
        let spawner = FakePtySpawner::new();
        let mgr = SessionManager::new("/bin/bash");
        let (id, _, _) = mgr.create(None, None, 80, 24, &spawner).await.unwrap();
        assert_eq!(mgr.session_count().await, 1);

        mgr.destroy(&id).await.unwrap();
        assert_eq!(mgr.session_count().await, 0);
    }

    #[tokio::test]
    async fn list_with_valid_token() {
        let spawner = FakePtySpawner::new();
        let mgr = SessionManager::new("/bin/bash");
        let (_, _, token) = mgr
            .create(Some("mysession".into()), None, 80, 24, &spawner)
            .await
            .unwrap();

        let list = mgr.list(&[token]).await;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "mysession");
    }

    #[tokio::test]
    async fn list_with_wrong_token() {
        let spawner = FakePtySpawner::new();
        let mgr = SessionManager::new("/bin/bash");
        mgr.create(None, None, 80, 24, &spawner).await.unwrap();

        let list = mgr.list(&["wrong-token".into()]).await;
        assert_eq!(list.len(), 0);
    }

    #[tokio::test]
    async fn reap_does_nothing_for_active() {
        let spawner = FakePtySpawner::new();
        let mgr = SessionManager::new("/bin/bash");
        mgr.create(None, None, 80, 24, &spawner).await.unwrap();

        mgr.reap(1800).await;
        assert_eq!(mgr.session_count().await, 1); // not reaped
    }

    #[tokio::test]
    async fn hash_token_deterministic() {
        let h1 = hash_token("test-token");
        let h2 = hash_token("test-token");
        assert_eq!(h1, h2);

        let h3 = hash_token("different");
        assert_ne!(h1, h3);
    }
}
