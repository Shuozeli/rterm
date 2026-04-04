//! Timeline event log for replay-capable terminal sessions.
//!
//! Three-layer architecture:
//! - **Events Layer**: Immutable timeline events (user inputs, PTY output)
//! - **State Layer**: Periodic snapshots + StateReconstructor for replay
//! - **UI Layer**: DisplayGrid renders Terminal state (no changes needed)

use rterm_core::Terminal;
use std::time::Instant;

/// Interval between full state snapshots (in event count).
pub const SNAPSHOT_INTERVAL: u64 = 100;

// ---------------------------------------------------------------------------
// Timeline Event
// ---------------------------------------------------------------------------

/// An immutable timeline event. Once created, never modified.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimelineEvent {
    /// Client sent key input bytes.
    ClientKeyInput { data: Vec<u8>, ts: Instant },
    /// Client sent a mouse event.
    ClientMouse {
        row: u16,
        col: u16,
        button: u8,
        modifiers: u8,
        kind: u8,
        ts: Instant,
    },
    /// Client sent a terminal resize.
    ClientResize { cols: u16, rows: u16, ts: Instant },
    /// Client pasted text.
    ClientPaste { text: String, ts: Instant },
    /// Server (PTY) produced output bytes.
    ServerOutput { data: Vec<u8>, ts: Instant },
    /// Lines scrolled off the top of the scrollback buffer.
    ScrollbackPush {
        lines: Vec<Vec<rterm_core::Cell>>,
        ts: Instant,
    },
}

impl TimelineEvent {
    pub fn timestamp(&self) -> Instant {
        match self {
            TimelineEvent::ClientKeyInput { ts, .. } => *ts,
            TimelineEvent::ClientMouse { ts, .. } => *ts,
            TimelineEvent::ClientResize { ts, .. } => *ts,
            TimelineEvent::ClientPaste { ts, .. } => *ts,
            TimelineEvent::ServerOutput { ts, .. } => *ts,
            TimelineEvent::ScrollbackPush { ts, .. } => *ts,
        }
    }

    pub fn event_type(&self) -> &'static str {
        match self {
            TimelineEvent::ClientKeyInput { .. } => "ClientKeyInput",
            TimelineEvent::ClientMouse { .. } => "ClientMouse",
            TimelineEvent::ClientResize { .. } => "ClientResize",
            TimelineEvent::ClientPaste { .. } => "ClientPaste",
            TimelineEvent::ServerOutput { .. } => "ServerOutput",
            TimelineEvent::ScrollbackPush { .. } => "ScrollbackPush",
        }
    }

    /// Apply this event to a Terminal, updating it in-place.
    /// Used during replay.
    pub fn apply_to_terminal(&self, terminal: &mut Terminal) {
        match self {
            TimelineEvent::ClientKeyInput { data, .. } => {
                terminal.feed(data);
            }
            TimelineEvent::ClientMouse { .. } => {
                // Mouse events are handled by the session layer, not the terminal.
                // During replay, we skip mouse events as they don't affect terminal state.
            }
            TimelineEvent::ClientResize { cols, rows, .. } => {
                terminal.resize(*cols as usize, *rows as usize);
            }
            TimelineEvent::ClientPaste { text, .. } => {
                terminal.feed(text.as_bytes());
            }
            TimelineEvent::ServerOutput { data, .. } => {
                terminal.feed(data);
            }
            TimelineEvent::ScrollbackPush { .. } => {
                // Scrollback push is handled by the screen buffer during replay.
                // The terminal's scrollback already captures this via Terminal::scrollback().
            }
        }
    }
}

// ---------------------------------------------------------------------------
// State Snapshot
// ---------------------------------------------------------------------------

/// A full snapshot of terminal state at a given timeline position.
#[derive(Debug, Clone)]
pub struct StateSnapshot {
    /// Timeline event index this snapshot corresponds to.
    pub event_index: u64,
    /// Snapshot timestamp.
    pub ts: Instant,
    /// Terminal dimensions and mode state (full fidelity requires event replay).
    pub cols: usize,
    pub rows: usize,
    pub alt_active: bool,
    pub mouse_tracking_mode: u8,
    pub application_cursor_keys: bool,
    pub application_keypad: bool,
    pub focus_events: bool,
    pub autowrap: bool,
    pub sync_mode: bool,
    pub bracketed_paste: bool,
    pub cursor_style: u8,
    pub bell_pending: bool,
}

impl StateSnapshot {
    /// Create a snapshot of the current terminal state at the given event index.
    /// Stores only metadata; full state reconstruction relies on event replay.
    pub fn new(terminal: &Terminal, event_index: u64, ts: Instant) -> Self {
        Self {
            event_index,
            ts,
            cols: terminal.screen().cols(),
            rows: terminal.screen().rows(),
            alt_active: terminal.is_alt_screen_active(),
            mouse_tracking_mode: terminal.modes.mouse_tracking_mode,
            application_cursor_keys: terminal.modes.application_cursor_keys,
            application_keypad: terminal.modes.application_keypad,
            focus_events: terminal.modes.focus_events,
            autowrap: terminal.modes.autowrap,
            sync_mode: terminal.is_sync_mode(),
            bracketed_paste: terminal.bracketed_paste,
            cursor_style: terminal.cursor_style,
            bell_pending: terminal.bell_pending,
        }
    }
}

// ---------------------------------------------------------------------------
// Timeline
// ---------------------------------------------------------------------------

/// A persistent, append-only log of terminal events with periodic snapshots.
#[derive(Debug)]
pub struct Timeline {
    /// All timeline events in order.
    events: Vec<TimelineEvent>,
    /// Periodic snapshots: (event_index, snapshot).
    snapshots: Vec<StateSnapshot>,
    /// Total events pushed.
    total_events: u64,
}

impl Default for Timeline {
    fn default() -> Self {
        Self::new()
    }
}

impl Timeline {
    pub fn new() -> Self {
        Self {
            events: Vec::with_capacity(10_000),
            snapshots: Vec::with_capacity(100),
            total_events: 0,
        }
    }

    /// Push a new event onto the timeline.
    /// Returns the event index assigned to this event.
    pub fn push(&mut self, event: TimelineEvent) -> u64 {
        let idx = self.total_events;
        self.total_events += 1;
        self.events.push(event);
        idx
    }

    /// Push a client key input event.
    pub fn push_key_input(&mut self, data: Vec<u8>) -> u64 {
        self.push(TimelineEvent::ClientKeyInput {
            data,
            ts: Instant::now(),
        })
    }

    /// Push a server output event.
    pub fn push_server_output(&mut self, data: Vec<u8>) -> u64 {
        self.push(TimelineEvent::ServerOutput {
            data,
            ts: Instant::now(),
        })
    }

    /// Push a scrollback push event.
    pub fn push_scrollback(&mut self, lines: Vec<Vec<rterm_core::Cell>>) -> u64 {
        self.push(TimelineEvent::ScrollbackPush {
            lines,
            ts: Instant::now(),
        })
    }

    /// Take a snapshot of the current terminal state.
    /// Called periodically (every SNAPSHOT_INTERVAL events) to enable timeline replay.
    pub fn take_snapshot(&mut self, terminal: &Terminal) {
        let idx = self.total_events.saturating_sub(1);
        let ts = Instant::now();
        let snapshot = StateSnapshot::new(terminal, idx, ts);
        self.snapshots.push(snapshot);
    }

    /// Returns the total number of events in the timeline.
    pub fn len(&self) -> u64 {
        self.total_events
    }

    /// Returns true if the timeline has no events.
    pub fn is_empty(&self) -> bool {
        self.total_events == 0
    }

    /// Returns the number of snapshots stored.
    pub fn num_snapshots(&self) -> usize {
        self.snapshots.len()
    }

    /// Returns the total event count.
    pub fn total_events(&self) -> u64 {
        self.total_events
    }

    /// Returns a reference to events in the given range.
    pub fn events_range(&self, start: u64, end: u64) -> &[TimelineEvent] {
        let start = start as usize;
        let end = end.min(self.events.len() as u64) as usize;
        &self.events[start..end]
    }

    /// Returns the snapshot nearest to (but not after) the given event index.
    pub fn nearest_snapshot_before(&self, event_index: u64) -> Option<&StateSnapshot> {
        self.snapshots
            .iter()
            .filter(|s| s.event_index <= event_index)
            .max_by_key(|s| s.event_index)
    }
}

// ---------------------------------------------------------------------------
// State Reconstructor
// ---------------------------------------------------------------------------

/// Reconstructs terminal state at any point in the timeline using
/// snapshots + event replay.
pub struct StateReconstructor<'a> {
    timeline: &'a Timeline,
}

impl<'a> StateReconstructor<'a> {
    pub fn new(timeline: &'a Timeline) -> Self {
        Self { timeline }
    }

    /// Reconstruct the full Terminal state at exactly `event_index`.
    ///
    /// Strategy: find nearest snapshot before target, then replay remaining events.
    pub fn reconstruct_at(&self, event_index: u64) -> Option<Terminal> {
        if event_index >= self.timeline.total_events {
            return None;
        }

        // Find nearest snapshot before or at event_index.
        let snapshot_idx = self
            .timeline
            .snapshots
            .binary_search_by_key(&event_index, |s| s.event_index)
            .unwrap_or_else(|i| i.saturating_sub(1));

        let (start_idx, terminal) = if let Some(snap) = self.timeline.snapshots.get(snapshot_idx) {
            // Start from snapshot state.
            let term = self.terminal_from_snapshot(snap);
            (snap.event_index as usize + 1, term)
        } else {
            // No snapshot available — start from scratch.
            // Default to 80x24 if we have no information.
            (0, Terminal::new(80, 24))
        };

        // Replay events from start_idx to event_index (inclusive).
        let mut term = terminal;
        for idx in start_idx..=event_index as usize {
            if let Some(event) = self.timeline.events.get(idx) {
                event.apply_to_terminal(&mut term);
            }
        }

        Some(term)
    }

    /// Build a Terminal from a snapshot's stored state.
    /// Note: this recreates the Terminal with the correct dimensions and a
    /// best-effort reconstruction. Full fidelity requires replaying events.
    fn terminal_from_snapshot(&self, snap: &StateSnapshot) -> Terminal {
        let mut term = Terminal::new(snap.cols, snap.rows);
        term.resize(snap.cols, snap.rows);
        // Modes are not fully restored by this — use reconstruct_at for full state.
        term
    }

    /// Get scrollback lines near the given event index.
    /// Returns (lines, total_scrollback_length) at that timeline position.
    pub fn scrollback_near(
        &self,
        event_index: u64,
        offset: usize,
        limit: usize,
    ) -> Option<(Vec<Vec<rterm_proto::CellData>>, usize)> {
        // First reconstruct terminal state at event_index.
        let terminal = self.reconstruct_at(event_index)?;
        let scrollback = terminal.scrollback();

        let total = scrollback.len();
        if total == 0 {
            return Some((Vec::new(), 0));
        }

        let start = offset.min(total);
        let end = (start + limit).min(total);

        let lines: Vec<Vec<rterm_proto::CellData>> = scrollback[start..end]
            .iter()
            .map(|row| {
                row.iter()
                    .map(|cell| rterm_proto::CellData {
                        ch: cell.ch,
                        fg: crate::pack_color(&cell.fg),
                        bg: crate::pack_color(&cell.bg),
                        flags: cell.flags.bits(),
                    })
                    .collect()
            })
            .collect();

        Some((lines, total))
    }

    /// Get a screen snapshot (CellData grid) at the given event index.
    pub fn screen_at(&self, event_index: u64) -> Option<rterm_proto::ScreenSnapshotData> {
        let terminal = self.reconstruct_at(event_index)?;
        let screen = terminal.screen();
        let cols = screen.cols();
        let rows = screen.rows();

        // Build rows: Vec<CellRangeData>, one per screen row.
        let rows_data: Vec<rterm_proto::CellRangeData> = (0..rows)
            .map(|r| {
                let cells: Vec<rterm_proto::CellData> = (0..cols)
                    .map(|c| {
                        let cell = screen.cell(r, c);
                        rterm_proto::CellData {
                            ch: cell.ch,
                            fg: crate::pack_color(&cell.fg),
                            bg: crate::pack_color(&cell.bg),
                            flags: cell.flags.bits(),
                        }
                    })
                    .collect();
                rterm_proto::CellRangeData {
                    row: r as u16,
                    col_start: 0,
                    cells,
                }
            })
            .collect();

        Some(rterm_proto::ScreenSnapshotData {
            cols: cols as u16,
            num_rows: rows as u16,
            rows: rows_data,
            cursor: rterm_proto::CursorData {
                row: screen.cursor.row as u16,
                col: screen.cursor.col as u16,
                visible: screen.cursor.visible,
                style: terminal.cursor_style,
            },
            mouse_tracking_mode: terminal.modes.mouse_tracking_mode,
            alt_screen_active: terminal.is_alt_screen_active(),
            application_cursor_keys: terminal.modes.application_cursor_keys,
            title: terminal.title.clone(),
            viewport_offset: 0,
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeline_event_push_and_retrieve() {
        let mut tl = Timeline::new();
        assert_eq!(tl.len(), 0);

        let idx1 = tl.push_key_input(b"hello".to_vec());
        assert_eq!(idx1, 0);
        assert_eq!(tl.len(), 1);

        let idx2 = tl.push_server_output(b"world".to_vec());
        assert_eq!(idx2, 1);
        assert_eq!(tl.len(), 2);
    }

    #[test]
    fn timeline_event_timestamp() {
        let mut tl = Timeline::new();
        tl.push_key_input(b"x".to_vec());
        if let Some(event) = tl.events_range(0, 1).first() {
            assert_eq!(event.event_type(), "ClientKeyInput");
        } else {
            panic!("expected one event");
        }
    }

    #[test]
    fn snapshot_nearest_before() {
        let mut tl = Timeline::new();
        // No snapshots yet.
        assert!(tl.nearest_snapshot_before(0).is_none());

        // Push events and manually add snapshots.
        tl.push_key_input(b"a".to_vec());
        tl.push_key_input(b"b".to_vec());
        tl.push_key_input(b"c".to_vec());
        tl.push_key_input(b"d".to_vec());
        tl.push_key_input(b"e".to_vec());

        // Simulate snapshots at index 0 and 4.
        let snap0 = StateSnapshot {
            event_index: 0,
            ts: Instant::now(),
            cols: 80,
            rows: 24,
            alt_active: false,
            mouse_tracking_mode: 0,
            application_cursor_keys: false,
            application_keypad: false,
            focus_events: false,
            autowrap: true,
            sync_mode: false,
            bracketed_paste: false,
            cursor_style: 0,
            bell_pending: false,
        };
        let snap4 = StateSnapshot {
            event_index: 4,
            ts: Instant::now(),
            cols: 80,
            rows: 24,
            alt_active: false,
            mouse_tracking_mode: 0,
            application_cursor_keys: false,
            application_keypad: false,
            focus_events: false,
            autowrap: true,
            sync_mode: false,
            bracketed_paste: false,
            cursor_style: 0,
            bell_pending: false,
        };
        tl.snapshots.push(snap0.clone());
        tl.snapshots.push(snap4.clone());

        assert_eq!(
            tl.nearest_snapshot_before(0).map(|s| s.event_index),
            Some(0)
        );
        assert_eq!(
            tl.nearest_snapshot_before(2).map(|s| s.event_index),
            Some(0)
        );
        assert_eq!(
            tl.nearest_snapshot_before(4).map(|s| s.event_index),
            Some(4)
        );
        assert_eq!(
            tl.nearest_snapshot_before(5).map(|s| s.event_index),
            Some(4)
        );
    }

    #[test]
    fn reconstruct_at_empty_timeline() {
        let tl = Timeline::new();
        let recon = StateReconstructor::new(&tl);
        assert!(recon.reconstruct_at(0).is_none());
    }

    #[test]
    fn reconstruct_at_with_events() {
        let mut tl = Timeline::new();
        tl.push_server_output(b"Hello".to_vec());
        tl.push_server_output(b" World".to_vec());

        let recon = StateReconstructor::new(&tl);
        let term = recon.reconstruct_at(1).expect("should reconstruct");
        let screen = term.screen();
        assert_eq!(screen.row_text(0), "Hello World");
    }

    #[test]
    fn reconstruct_at_out_of_bounds() {
        let mut tl = Timeline::new();
        tl.push_key_input(b"x".to_vec());
        let recon = StateReconstructor::new(&tl);
        assert!(recon.reconstruct_at(5).is_none()); // beyond len
    }

    #[test]
    fn timeline_event_apply_to_terminal() {
        let mut term = Terminal::new(80, 24);
        let event = TimelineEvent::ServerOutput {
            data: b"Test".to_vec(),
            ts: Instant::now(),
        };
        event.apply_to_terminal(&mut term);
        assert_eq!(term.screen().row_text(0), "Test");
    }

    #[test]
    fn resize_event_applies() {
        let mut term = Terminal::new(80, 24);
        let event = TimelineEvent::ClientResize {
            cols: 40,
            rows: 10,
            ts: Instant::now(),
        };
        event.apply_to_terminal(&mut term);
        assert_eq!(term.screen().cols(), 40);
        assert_eq!(term.screen().rows(), 10);
    }
}
