/// A managed terminal session that lives independently of client connections.
use crate::pty::PtySpawner;
use crate::screen_diff::{self, PrevScreen};
use rterm_core::Terminal;
use rterm_proto::*;
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::info;

/// Return type for `ManagedSession::new`: the session and its PTY stdout channel.
type NewSessionResult =
    Result<(ManagedSession, mpsc::Receiver<Vec<u8>>), Box<dyn std::error::Error + Send + Sync>>;

/// Session state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    Attached,
    Detached,
    Dead,
}

/// A long-lived terminal session.
pub struct ManagedSession {
    pub name: String,
    pub state: SessionState,
    pub created_at: Instant,
    pub last_activity: Instant,
    pub shell: String,
    pub cols: u16,
    pub rows: u16,

    // VT emulator — kept alive across attach/detach.
    pub terminal: Terminal,
    // Screen differ — rebuilt on each attach.
    pub prev_screen: PrevScreen,

    // PTY channels.
    pub pty_stdin_tx: mpsc::Sender<Vec<u8>>,
    pub pty_resize_tx: mpsc::Sender<(u16, u16)>,

    // Channel to the attached client (None if detached).
    pub client_tx: Option<mpsc::Sender<ServerMsg>>,

    // Set when PTY exits while detached.
    pub pty_exited: Option<i32>,
}

impl ManagedSession {
    /// Create a new session. Spawns the PTY and starts the output loop.
    /// Returns (ManagedSession, stdout_rx) -- caller must start the output loop.
    pub fn new(
        name: String,
        shell: &str,
        cols: u16,
        rows: u16,
        spawner: &dyn PtySpawner,
    ) -> NewSessionResult {
        let pty = spawner.spawn(shell, cols, rows)?;

        let session = Self {
            name,
            state: SessionState::Detached, // starts detached, attach sets it
            created_at: Instant::now(),
            last_activity: Instant::now(),
            shell: shell.to_string(),
            cols,
            rows,
            terminal: Terminal::new(cols as usize, rows as usize),
            prev_screen: PrevScreen::new(cols as usize, rows as usize),
            pty_stdin_tx: pty.stdin_tx,
            pty_resize_tx: pty.resize_tx,
            client_tx: None,
            pty_exited: None,
        };

        Ok((session, pty.stdout_rx))
    }

    /// Attach a client. Returns a ScreenSnapshot of the current state.
    pub fn attach(
        &mut self,
        client_tx: mpsc::Sender<ServerMsg>,
        cols: u16,
        rows: u16,
    ) -> ScreenSnapshotData {
        // Displace existing client if any.
        if let Some(old_tx) = self.client_tx.take() {
            let _ = old_tx.try_send(ServerMsg::SessionDetached(rterm_proto::SessionDetached {
                session_id: self.name.clone(),
                reason: "displaced by new client".into(),
            }));
        }

        self.state = SessionState::Attached;
        self.client_tx = Some(client_tx);
        self.last_activity = Instant::now();

        // Resize if client has different dimensions.
        if cols != self.cols || rows != self.rows {
            self.cols = cols;
            self.rows = rows;
            self.terminal.resize(cols as usize, rows as usize);
            let _ = self.pty_resize_tx.try_send((cols, rows));
        }

        // Rebuild prev_screen for fresh diffing.
        let mut snapshot = screen_diff::snapshot(self.terminal.screen());
        snapshot.mouse_tracking_mode = self.terminal.modes.mouse_tracking_mode;
        snapshot.alt_screen_active = self.terminal.is_alt_screen_active();
        snapshot.application_cursor_keys = self.terminal.modes.application_cursor_keys;
        self.prev_screen = PrevScreen::new(self.cols as usize, self.rows as usize);
        self.prev_screen.update_from_snapshot(&snapshot);

        snapshot
    }

    /// Detach the current client.
    pub fn detach(&mut self) {
        self.client_tx = None;
        if self.state == SessionState::Attached {
            self.state = SessionState::Detached;
        }
    }

    /// Mark as dead (PTY exited).
    pub fn mark_dead(&mut self, exit_code: i32) {
        self.pty_exited = Some(exit_code);
        if let Some(tx) = &self.client_tx {
            let _ = tx.try_send(ServerMsg::Exit(Exit { code: exit_code }));
        }
        self.state = SessionState::Dead;
    }

    /// Process PTY output: feed terminal, diff, try to send to client.
    pub fn process_pty_output(&mut self, data: &[u8]) {
        self.terminal.feed(data);
        self.last_activity = Instant::now();

        if self.terminal.is_sync_mode() {
            return;
        }

        if let Some(mut update) = self.prev_screen.diff(self.terminal.screen()) {
            update.mouse_tracking_mode = self.terminal.modes.mouse_tracking_mode;
            update.alt_screen_active = self.terminal.is_alt_screen_active();
            update.application_cursor_keys = self.terminal.modes.application_cursor_keys;
            if let Some(tx) = &self.client_tx {
                let _ = tx.try_send(ServerMsg::ScreenUpdate(update));
            }
        }
    }

    /// Resize the terminal and notify the PTY.
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.cols = cols;
        self.rows = rows;
        self.terminal.resize(cols as usize, rows as usize);
        let _ = self.pty_resize_tx.try_send((cols, rows));
    }

    /// Return the current screen as plain text (one trimmed line per row).
    pub fn plain_text(&self) -> String {
        let screen = self.terminal.screen();
        let mut out = String::new();
        for row_idx in 0..screen.rows() {
            let mut line = String::new();
            for col_idx in 0..screen.cols() {
                let ch = screen.cell(row_idx, col_idx).ch;
                line.push(if ch == '\0' { ' ' } else { ch });
            }
            out.push_str(line.trim_end());
            out.push('\n');
        }
        out
    }

    /// Check if the session has timed out (detached too long).
    pub fn is_timed_out(&self, max_detach_secs: u64) -> bool {
        self.state == SessionState::Detached
            && self.last_activity.elapsed().as_secs() > max_detach_secs
    }
}

/// The session output loop — runs independently of client connections.
/// Reads PTY stdout, feeds Terminal, diffs and sends to client if attached.
pub async fn session_output_loop(
    session: std::sync::Arc<tokio::sync::Mutex<ManagedSession>>,
    mut stdout_rx: mpsc::Receiver<Vec<u8>>,
) {
    while let Some(data) = stdout_rx.recv().await {
        let mut s = session.lock().await;
        s.process_pty_output(&data);
    }

    // PTY exited.
    let mut s = session.lock().await;
    s.mark_dead(0);
    info!("session {} output loop ended", s.name);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pty::fake::FakePtySpawner;

    #[tokio::test]
    async fn create_session() {
        let spawner = FakePtySpawner::new();

        let (session, _rx) =
            ManagedSession::new("test".into(), "/bin/bash", 80, 24, &spawner).unwrap();
        assert_eq!(session.name, "test");
        assert_eq!(session.state, SessionState::Detached);
        assert_eq!(session.cols, 80);
    }

    #[tokio::test]
    async fn attach_returns_snapshot() {
        let spawner = FakePtySpawner::new();

        let (mut session, _rx) =
            ManagedSession::new("test".into(), "/bin/bash", 80, 24, &spawner).unwrap();

        let (client_tx, _client_rx) = mpsc::channel(64);
        let snapshot = session.attach(client_tx, 80, 24);
        assert_eq!(snapshot.cols, 80);
        assert_eq!(snapshot.num_rows, 24);
        assert_eq!(session.state, SessionState::Attached);
    }

    #[tokio::test]
    async fn attach_displaces_old_client() {
        let spawner = FakePtySpawner::new();

        let (mut session, _rx) =
            ManagedSession::new("test".into(), "/bin/bash", 80, 24, &spawner).unwrap();

        let (tx1, mut rx1) = mpsc::channel(64);
        session.attach(tx1, 80, 24);

        let (tx2, _rx2) = mpsc::channel(64);
        session.attach(tx2, 80, 24);

        // Old client should receive SessionDetached.
        let msg = rx1.recv().await.unwrap();
        assert!(matches!(msg, ServerMsg::SessionDetached(_)));
    }

    #[tokio::test]
    async fn detach_clears_client() {
        let spawner = FakePtySpawner::new();

        let (mut session, _rx) =
            ManagedSession::new("test".into(), "/bin/bash", 80, 24, &spawner).unwrap();

        let (tx, _rx) = mpsc::channel(64);
        session.attach(tx, 80, 24);
        assert_eq!(session.state, SessionState::Attached);

        session.detach();
        assert_eq!(session.state, SessionState::Detached);
        assert!(session.client_tx.is_none());
    }

    #[tokio::test]
    async fn mark_dead_sets_state() {
        let spawner = FakePtySpawner::new();

        let (mut session, _rx) =
            ManagedSession::new("test".into(), "/bin/bash", 80, 24, &spawner).unwrap();

        session.mark_dead(42);
        assert_eq!(session.state, SessionState::Dead);
        assert_eq!(session.pty_exited, Some(42));
    }

    #[tokio::test]
    async fn timeout_check() {
        let spawner = FakePtySpawner::new();

        let (session, _rx) =
            ManagedSession::new("test".into(), "/bin/bash", 80, 24, &spawner).unwrap();

        // Just created, should not be timed out.
        assert!(!session.is_timed_out(1800));
    }

    #[tokio::test]
    async fn output_loop_feeds_terminal() {
        let spawner = FakePtySpawner::new().with_stdout(vec![b"Hello".to_vec()]);

        let (session, stdout_rx) =
            ManagedSession::new("test".into(), "/bin/bash", 80, 24, &spawner).unwrap();

        let session = std::sync::Arc::new(tokio::sync::Mutex::new(session));
        session_output_loop(session.clone(), stdout_rx).await;

        let s = session.lock().await;
        assert_eq!(s.terminal.screen().cell(0, 0).ch, 'H');
        assert_eq!(s.state, SessionState::Dead); // PTY exited
    }

    #[tokio::test]
    async fn output_loop_sends_to_client() {
        let spawner = FakePtySpawner::new().with_stdout(vec![b"Hi".to_vec()]);

        let (mut session, stdout_rx) =
            ManagedSession::new("test".into(), "/bin/bash", 80, 24, &spawner).unwrap();

        let (client_tx, mut client_rx) = mpsc::channel(64);
        session.attach(client_tx, 80, 24);

        let session = std::sync::Arc::new(tokio::sync::Mutex::new(session));
        session_output_loop(session.clone(), stdout_rx).await;

        // Client should have received ScreenUpdate(s) + Exit.
        let mut got_update = false;
        while let Some(msg) = client_rx.recv().await {
            match msg {
                ServerMsg::ScreenUpdate(_) => got_update = true,
                ServerMsg::Exit(_) => break,
                _ => {}
            }
        }
        assert!(got_update);
    }

    #[tokio::test]
    async fn attach_with_resize() {
        let spawner = FakePtySpawner::new();

        let (mut session, _rx) =
            ManagedSession::new("test".into(), "/bin/bash", 80, 24, &spawner).unwrap();

        let (tx, _rx) = mpsc::channel(64);
        let snapshot = session.attach(tx, 120, 40);
        assert_eq!(snapshot.cols, 120);
        assert_eq!(snapshot.num_rows, 40);
        assert_eq!(session.cols, 120);
        assert_eq!(session.rows, 40);
    }

    #[tokio::test]
    async fn spawn_failure() {
        let spawner = FakePtySpawner::new().failing();

        let result = ManagedSession::new("test".into(), "/bin/bash", 80, 24, &spawner);
        assert!(result.is_err());
    }
}
