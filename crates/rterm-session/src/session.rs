/// A managed terminal session that lives independently of client connections.
use crate::screen_diff::{self, PrevScreen};
use crate::timeline::Timeline;
use rterm_proto::*;
use rterm_transport::PtySpawner;
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
    pub terminal: rterm_core::Terminal,
    // Screen differ — rebuilt on each attach.
    pub prev_screen: PrevScreen,
    // Timeline event log for replay capability.
    pub timeline: Timeline,

    // Per-client viewport: offset into scrollback (0 = at bottom / live).
    viewport_offset: u32,

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
        Ok(Self::from_pty(name, shell, cols, rows, pty))
    }

    /// Build a ManagedSession from a pre-created PtyHandle.
    pub fn from_pty(
        name: String,
        shell: &str,
        cols: u16,
        rows: u16,
        pty: rterm_transport::PtyHandle,
    ) -> (Self, mpsc::Receiver<Vec<u8>>) {
        let session = Self {
            name,
            state: SessionState::Detached,
            created_at: Instant::now(),
            last_activity: Instant::now(),
            shell: shell.to_string(),
            cols,
            rows,
            terminal: rterm_core::Terminal::new(cols as usize, rows as usize),
            prev_screen: PrevScreen::new(cols as usize, rows as usize),
            timeline: Timeline::new(),
            viewport_offset: 0,
            pty_stdin_tx: pty.stdin_tx,
            pty_resize_tx: pty.resize_tx,
            client_tx: None,
            pty_exited: None,
        };
        (session, pty.stdout_rx)
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
        // Push to timeline before feeding (timeline stores the raw output).
        let event_idx = self.timeline.push_server_output(data.to_vec());
        self.terminal.feed(data);
        self.last_activity = Instant::now();
        tracing::debug!(
            "[session {}] process_pty_output: {} bytes, client_tx present={}",
            self.name,
            data.len(),
            self.client_tx.is_some()
        );

        // Take a periodic snapshot for timeline replay capability.
        if event_idx.is_multiple_of(crate::timeline::SNAPSHOT_INTERVAL) {
            self.timeline.take_snapshot(&self.terminal);
        }

        if self.terminal.is_sync_mode() {
            return;
        }

        if let Some(mut update) = self.prev_screen.diff(self.terminal.screen()) {
            update.mouse_tracking_mode = self.terminal.modes.mouse_tracking_mode;
            update.alt_screen_active = self.terminal.is_alt_screen_active();
            update.application_cursor_keys = self.terminal.modes.application_cursor_keys;
            if let Some(tx) = &self.client_tx {
                let _ = tx.try_send(ServerMsg::ScreenUpdate(update));
                tracing::debug!("[session {}] sent ScreenUpdate to client", self.name);
            } else {
                tracing::debug!(
                    "[session {}] client_tx is None, dropping ScreenUpdate",
                    self.name
                );
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

    /// Return the current screen as a full styled snapshot for rendering.
    pub fn screen_snapshot(&self) -> ScreenSnapshotData {
        let mut snap = screen_diff::snapshot(self.terminal.screen());
        snap.mouse_tracking_mode = self.terminal.modes.mouse_tracking_mode;
        snap.alt_screen_active = self.terminal.is_alt_screen_active();
        snap.application_cursor_keys = self.terminal.modes.application_cursor_keys;
        snap
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

    /// Return scrollback lines starting at `offset` (0 = oldest).
    /// Returns up to `limit` lines.
    pub fn get_scrollback(&self, offset: usize, limit: usize) -> ScrollbackData {
        let scrollback = self.terminal.scrollback();
        let total = scrollback.len();

        // Clamp offset to available scrollback.
        let start = offset.min(total);
        let end = (start + limit).min(total);

        let lines: Vec<CellRangeData> = scrollback[start..end]
            .iter()
            .enumerate()
            .map(|(i, row)| {
                let cells: Vec<CellData> = row
                    .iter()
                    .map(|cell| CellData {
                        ch: cell.ch,
                        fg: screen_diff::pack_color(&cell.fg),
                        bg: screen_diff::pack_color(&cell.bg),
                        flags: cell.flags.bits(),
                    })
                    .collect();
                CellRangeData {
                    // Absolute row index in the scrollback buffer.
                    row: (start + i) as u16,
                    col_start: 0,
                    cells,
                }
            })
            .collect();

        ScrollbackData {
            lines,
            offset: start as u32,
            total: total as u32,
        }
    }

    /// Check if the session has timed out (detached too long).
    pub fn is_timed_out(&self, max_detach_secs: u64) -> bool {
        self.state == SessionState::Detached
            && self.last_activity.elapsed().as_secs() > max_detach_secs
    }

    /// Reconstruct terminal state at the given timeline event index.
    /// Returns a ScreenSnapshotData for rendering.
    pub fn get_state_at(&self, event_index: u64) -> Option<ScreenSnapshotData> {
        let recon = crate::timeline::StateReconstructor::new(&self.timeline);
        recon.screen_at(event_index)
    }

    /// Get scrollback lines at a specific timeline position.
    /// Returns scrollback lines near the given event index.
    pub fn get_scrollback_at(
        &self,
        event_index: u64,
        offset: usize,
        limit: usize,
    ) -> Option<ScrollbackData> {
        let recon = crate::timeline::StateReconstructor::new(&self.timeline);
        let (lines, total) = recon.scrollback_near(event_index, offset, limit)?;

        let data_lines: Vec<CellRangeData> = lines
            .iter()
            .enumerate()
            .map(|(i, row)| CellRangeData {
                // Absolute row index in the scrollback buffer at this timeline position.
                row: (offset + i) as u16,
                col_start: 0,
                cells: row.clone(),
            })
            .collect();

        Some(ScrollbackData {
            lines: data_lines,
            offset: offset as u32,
            total: total as u32,
        })
    }

    /// Get the current timeline info (total events, snapshot count).
    pub fn timeline_info(&self) -> (u64, usize) {
        (self.timeline.total_events(), self.timeline.num_snapshots())
    }

    /// Scroll the viewport by `lines` in `direction`.
    /// direction: 1 = up/back (older), -1 = down/forward (newer).
    /// Returns a ScreenSnapshotData covering all visible rows at the new viewport.
    pub fn scroll_viewport(&mut self, direction: i8, lines: u32) -> ScreenSnapshotData {
        let scrollback_len = self.terminal.scrollback_len() as u32;

        // Clamp lines to at least 1.
        let lines = lines.max(1);

        if direction > 0 {
            // Scrolling up/back — increase offset (older content).
            self.viewport_offset = (self.viewport_offset + lines).min(scrollback_len);
        } else {
            // Scrolling down/forward — decrease offset (toward live).
            self.viewport_offset = self.viewport_offset.saturating_sub(lines);
        }

        // Build snapshot: visible rows = scrollback[offset..] + current screen.
        let mut snapshot = self.build_viewport_snapshot();
        snapshot.mouse_tracking_mode = self.terminal.modes.mouse_tracking_mode;
        snapshot.alt_screen_active = self.terminal.is_alt_screen_active();
        snapshot.application_cursor_keys = self.terminal.modes.application_cursor_keys;
        snapshot
    }

    /// Reset viewport to live (offset = 0).
    pub fn reset_viewport(&mut self) {
        self.viewport_offset = 0;
    }

    /// Build a ScreenSnapshotData for the current viewport.
    /// The visible rows consist of the most recent `offset` lines of scrollback
    /// (if offset < scrollback_len), followed by the current screen rows.
    fn build_viewport_snapshot(&self) -> ScreenSnapshotData {
        let scrollback_len = self.terminal.scrollback_len();
        let cols = self.cols as usize;
        let rows = self.rows as usize;
        let offset = self.viewport_offset as usize;

        // How many scrollback rows to show (capped at available scrollback).
        let scrollback_rows = offset.min(scrollback_len);

        // Collect visible rows.
        let mut all_rows: Vec<CellRangeData> = Vec::with_capacity(rows);
        let scrollback = self.terminal.scrollback();
        let screen = self.terminal.screen();

        for pos in 0..rows {
            let cells: Vec<CellData> = if pos < scrollback_rows {
                // Show scrollback rows: indices scrollback_len - scrollback_rows + pos
                // up to scrollback_len - 1 (most recent).
                let scroll_idx = scrollback_len - scrollback_rows + pos;
                if let Some(row) = scrollback.get(scroll_idx) {
                    row.iter()
                        .map(|cell| CellData {
                            ch: cell.ch,
                            fg: screen_diff::pack_color(&cell.fg),
                            bg: screen_diff::pack_color(&cell.bg),
                            flags: cell.flags.bits(),
                        })
                        .collect()
                } else {
                    vec![]
                }
            } else {
                // Show current screen rows: screen row = pos - scrollback_rows
                let screen_row = pos - scrollback_rows;
                if screen_row < screen.rows() {
                    (0..cols)
                        .map(|col| {
                            let cell = screen.cell(screen_row, col);
                            CellData {
                                ch: cell.ch,
                                fg: screen_diff::pack_color(&cell.fg),
                                bg: screen_diff::pack_color(&cell.bg),
                                flags: cell.flags.bits(),
                            }
                        })
                        .collect()
                } else {
                    vec![]
                }
            };

            // Pad with empty cells if row is short.
            let cells = if cells.len() < cols {
                let mut c = cells;
                c.resize(
                    cols,
                    CellData {
                        ch: ' ',
                        fg: screen_diff::pack_color(&rterm_core::color::Color::Default),
                        bg: screen_diff::pack_color(&rterm_core::color::Color::Default),
                        flags: 0,
                    },
                );
                c
            } else {
                cells
            };

            all_rows.push(CellRangeData {
                row: pos as u16,
                col_start: 0,
                cells,
            });
        }

        ScreenSnapshotData {
            cols: self.cols,
            num_rows: rows as u16,
            rows: all_rows,
            cursor: CursorData {
                row: screen.cursor.row as u16,
                col: screen.cursor.col as u16,
                visible: screen.cursor.visible,
                style: self.terminal.cursor_style,
            },
            mouse_tracking_mode: self.terminal.modes.mouse_tracking_mode,
            alt_screen_active: self.terminal.is_alt_screen_active(),
            application_cursor_keys: self.terminal.modes.application_cursor_keys,
            title: self.terminal.title.clone(),
            viewport_offset: self.viewport_offset,
        }
    }
}

/// The session output loop — runs independently of client connections.
/// Reads PTY stdout, feeds Terminal, diffs and sends to client if attached.
pub async fn session_output_loop(
    session: std::sync::Arc<tokio::sync::Mutex<ManagedSession>>,
    mut stdout_rx: mpsc::Receiver<Vec<u8>>,
) {
    tracing::info!("[session_output_loop] started");
    while let Some(data) = stdout_rx.recv().await {
        tracing::debug!("[session_output_loop] received {} bytes", data.len());
        let mut s = session.lock().await;
        s.process_pty_output(&data);
    }
    tracing::info!("[session_output_loop] stdout_rx closed");

    // PTY exited.
    let mut s = session.lock().await;
    s.mark_dead(0);
    info!("session {} output loop ended", s.name);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rterm_transport::FakePtySpawner;

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
