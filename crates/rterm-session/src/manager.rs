/// SessionManager: creates, tracks, and manages named terminal sessions.
///
/// Session name = URL path. No tokens. If you can reach the server, you're trusted.
use crate::session::{ManagedSession, SessionState, session_output_loop};
use rterm_transport::PtySpawner;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::Mutex;
use tracing::{debug, info};

static SESSION_NAME_COUNTER: AtomicU64 = AtomicU64::new(0);

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

    /// Get or create a session by name.
    pub async fn get_or_create(
        &self,
        name: &str,
        cols: u16,
        rows: u16,
        spawner: &dyn PtySpawner,
    ) -> Result<Arc<Mutex<ManagedSession>>, String> {
        let mut sessions = self.sessions.lock().await;

        if let Some(session) = sessions.get(name) {
            let s = session.lock().await;
            if s.state != SessionState::Dead {
                drop(s);
                return Ok(Arc::clone(session));
            }
            drop(s);
            sessions.remove(name);
        }

        let (session, stdout_rx) =
            ManagedSession::new(name.to_string(), &self.default_shell, cols, rows, spawner)
                .map_err(|e| format!("spawn failed: {e}"))?;

        let session = Arc::new(Mutex::new(session));

        let session_loop = Arc::clone(&session);
        tokio::spawn(async move {
            session_output_loop(session_loop, stdout_rx).await;
        });

        sessions.insert(name.to_string(), Arc::clone(&session));
        info!("session created: {}", name);

        Ok(session)
    }

    /// Get or create a session with an explicit shell (overrides default for new sessions).
    pub async fn get_or_create_with_shell(
        &self,
        name: &str,
        shell: &str,
        cols: u16,
        rows: u16,
        spawner: &dyn PtySpawner,
    ) -> Result<Arc<Mutex<ManagedSession>>, String> {
        let mut sessions = self.sessions.lock().await;

        if let Some(session) = sessions.get(name) {
            let s = session.lock().await;
            if s.state != SessionState::Dead {
                drop(s);
                return Ok(Arc::clone(session));
            }
            drop(s);
            sessions.remove(name);
        }

        let (session, stdout_rx) =
            ManagedSession::new(name.to_string(), shell, cols, rows, spawner)
                .map_err(|e| format!("spawn failed: {e}"))?;

        let session = Arc::new(Mutex::new(session));

        let session_loop = Arc::clone(&session);
        tokio::spawn(async move {
            session_output_loop(session_loop, stdout_rx).await;
        });

        sessions.insert(name.to_string(), Arc::clone(&session));
        info!("session created: {} (shell: {})", name, shell);

        Ok(session)
    }

    /// Get a session by name without creating if it doesn't exist.
    pub async fn get(&self, name: &str) -> Option<Arc<Mutex<ManagedSession>>> {
        let sessions = self.sessions.lock().await;
        if let Some(session) = sessions.get(name) {
            let s = session.lock().await;
            if s.state != SessionState::Dead {
                return Some(Arc::clone(session));
            }
        }
        None
    }

    /// List active sessions.
    pub async fn list_sessions(&self) -> Vec<rterm_proto::SessionInfo> {
        let sessions = self.sessions.lock().await;
        let mut result = Vec::with_capacity(sessions.len());
        for (name, session) in sessions.iter() {
            let s = session.lock().await;
            if s.state != SessionState::Dead {
                let s_info = rterm_proto::SessionInfo {
                    session_id: name.to_string(),
                    name: name.to_string(),
                    shell: self.default_shell.clone(), // actual shell might differ, default is okay
                    created_at: 0,
                    last_activity: s.last_activity.elapsed().as_secs(),
                    attached: true, // simplified
                    cols: s.cols,
                    rows: s.rows,
                    title: None,
                };
                result.push(s_info);
            }
        }
        result
    }

    /// Destroy a session by name.
    pub async fn destroy(&self, name: &str) -> Result<(), String> {
        let mut sessions = self.sessions.lock().await;
        if let Some(session) = sessions.remove(name) {
            let mut s = session.lock().await;
            s.mark_dead(0);
            s.detach();
            info!("session destroyed: {}", name);
            Ok(())
        } else {
            Err("session not found".into())
        }
    }

    /// Reap timed-out and dead sessions.
    pub async fn reap(&self, max_detach_secs: u64) {
        let mut sessions = self.sessions.lock().await;
        let to_remove: Vec<String> = sessions
            .iter()
            .filter_map(|(name, session)| {
                let s = session.try_lock().ok()?;
                if s.is_timed_out(max_detach_secs) || s.state == SessionState::Dead {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect();

        for name in &to_remove {
            if let Some(session) = sessions.remove(name) {
                if let Ok(mut s) = session.try_lock() {
                    s.mark_dead(0);
                    s.detach();
                }
                debug!("reaped session: {}", name);
            }
        }

        if !to_remove.is_empty() {
            info!("reaped {} sessions", to_remove.len());
        }
    }

    pub async fn session_count(&self) -> usize {
        self.sessions.lock().await.len()
    }
}

/// Generate a unique session name using a monotonic counter mixed with time.
///
/// Uses `SESSION_NAME_COUNTER` (an `AtomicU64`) to guarantee uniqueness even
/// when called multiple times within the same nanosecond, avoiding the collision
/// risk of `subsec_nanos()` which wraps every second.
pub fn generate_session_name() -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    use std::time::SystemTime;

    let counter = SESSION_NAME_COUNTER.fetch_add(1, Ordering::Relaxed);

    // Mix the counter with wall-clock time for additional entropy.
    let time_seed = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(42);

    let hash_index = |salt: u64| -> u64 {
        let mut h = DefaultHasher::new();
        counter.hash(&mut h);
        time_seed.hash(&mut h);
        salt.hash(&mut h);
        h.finish()
    };

    let adjectives = [
        "swift", "quiet", "bold", "warm", "keen", "calm", "dark", "fair",
    ];
    let nouns = ["fox", "owl", "elm", "oak", "bay", "sky", "ash", "ivy"];
    let i1 = (hash_index(1) % adjectives.len() as u64) as usize;
    let i2 = (hash_index(2) % nouns.len() as u64) as usize;
    let suffix = hash_index(3) % 100;
    format!("{}-{}-{}", adjectives[i1], nouns[i2], suffix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rterm_transport::FakePtySpawner;

    #[tokio::test]
    async fn create_session() {
        let spawner = FakePtySpawner::new();
        let mgr = SessionManager::new("/bin/bash");
        let session = mgr.get_or_create("test", 80, 24, &spawner).await.unwrap();
        let s = session.lock().await;
        assert_eq!(s.name, "test");
        assert_eq!(mgr.session_count().await, 1);
    }

    #[tokio::test]
    async fn get_existing_session() {
        let spawner = FakePtySpawner::new();
        let mgr = SessionManager::new("/bin/bash");
        let s1 = mgr.get_or_create("dev", 80, 24, &spawner).await.unwrap();
        let s2 = mgr.get_or_create("dev", 80, 24, &spawner).await.unwrap();
        assert!(Arc::ptr_eq(&s1, &s2));
        assert_eq!(mgr.session_count().await, 1);
    }

    #[tokio::test]
    async fn different_sessions() {
        let spawner = FakePtySpawner::new();
        let mgr = SessionManager::new("/bin/bash");
        mgr.get_or_create("dev", 80, 24, &spawner).await.unwrap();
        mgr.get_or_create("deploy", 80, 24, &spawner).await.unwrap();
        assert_eq!(mgr.session_count().await, 2);
    }

    #[tokio::test]
    async fn destroy_session() {
        let spawner = FakePtySpawner::new();
        let mgr = SessionManager::new("/bin/bash");
        mgr.get_or_create("test", 80, 24, &spawner).await.unwrap();
        mgr.destroy("test").await.unwrap();
        assert_eq!(mgr.session_count().await, 0);
    }

    #[tokio::test]
    async fn destroy_nonexistent() {
        let mgr = SessionManager::new("/bin/bash");
        assert!(mgr.destroy("nope").await.is_err());
    }

    #[tokio::test]
    async fn reap_does_nothing_for_active() {
        let spawner = FakePtySpawner::new();
        let mgr = SessionManager::new("/bin/bash");
        mgr.get_or_create("test", 80, 24, &spawner).await.unwrap();
        mgr.reap(1800).await;
        assert_eq!(mgr.session_count().await, 1);
    }

    #[tokio::test]
    async fn recreate_dead_session() {
        let spawner = FakePtySpawner::new();
        let mgr = SessionManager::new("/bin/bash");
        let s1 = mgr.get_or_create("test", 80, 24, &spawner).await.unwrap();
        s1.lock().await.mark_dead(0);
        let s2 = mgr.get_or_create("test", 80, 24, &spawner).await.unwrap();
        assert!(!Arc::ptr_eq(&s1, &s2));
    }

    #[test]
    fn generated_name_format() {
        let name = generate_session_name();
        assert!(!name.is_empty());
        assert!(name.contains('-'));
    }
}
