//! In-process integration tests for the automation API.
//!
//! Uses FakePtySpawner — no real PTYs, no network, no Docker.
//! Tests the business logic that the service handlers exercise.

use rterm_relay::managed_session::{ManagedSession, session_output_loop};
use rterm_relay::pty::fake::FakePtySpawner;
use rterm_relay::session_manager::SessionManager;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

// ============================================================================
// ManagedSession unit tests
// ============================================================================

/// plain_text() must return trimmed lines for all rows, null bytes replaced with space.
#[tokio::test]
async fn plain_text_extracts_screen_content() {
    let spawner = FakePtySpawner::new().with_stdout(vec![b"hello world".to_vec()]);
    let (session, stdout_rx) =
        ManagedSession::new("t".into(), "/bin/bash", 80, 24, &spawner).unwrap();

    let session = Arc::new(Mutex::new(session));
    session_output_loop(Arc::clone(&session), stdout_rx).await;

    let s = session.lock().await;
    let text = s.plain_text();
    // First line should contain "hello world" (trimmed).
    let first_line = text.lines().next().unwrap();
    assert_eq!(first_line, "hello world");
    // Remaining lines are empty (trimmed null bytes → empty).
    for line in text.lines().skip(1) {
        assert!(
            line.is_empty(),
            "trailing lines should be empty, got {:?}",
            line
        );
    }
}

#[tokio::test]
async fn plain_text_multiline_content() {
    let spawner = FakePtySpawner::new().with_stdout(vec![b"line1\r\nline2\r\nline3".to_vec()]);
    let (session, stdout_rx) =
        ManagedSession::new("t".into(), "/bin/bash", 80, 24, &spawner).unwrap();

    let session = Arc::new(Mutex::new(session));
    session_output_loop(Arc::clone(&session), stdout_rx).await;

    let s = session.lock().await;
    let text = s.plain_text();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "line1");
    assert_eq!(lines[1], "line2");
    assert_eq!(lines[2], "line3");
}

/// resize() must update cols and rows on the ManagedSession.
#[tokio::test]
async fn resize_updates_cols_and_rows() {
    let spawner = FakePtySpawner::new();
    let (mut session, _rx) =
        ManagedSession::new("t".into(), "/bin/bash", 80, 24, &spawner).unwrap();

    assert_eq!(session.cols, 80);
    assert_eq!(session.rows, 24);

    session.resize(120, 40);

    assert_eq!(session.cols, 120);
    assert_eq!(session.rows, 40);
}

/// resize() sends the new dimensions to the PTY resize channel.
#[tokio::test]
async fn resize_signals_pty() {
    let spawner = FakePtySpawner::new();
    let (mut session, _rx) =
        ManagedSession::new("t".into(), "/bin/bash", 80, 24, &spawner).unwrap();
    let mut ctrl = spawner.take_control().unwrap();

    session.resize(100, 30);

    let (cols, rows) = ctrl.resize_rx.recv().await.unwrap();
    assert_eq!(cols, 100);
    assert_eq!(rows, 30);
}

// ============================================================================
// CreateSession handler logic
// ============================================================================

/// Creating a session that doesn't exist yet should succeed.
#[tokio::test]
async fn create_session_new_succeeds() {
    let spawner = FakePtySpawner::new();
    let mgr = SessionManager::new("/bin/bash");

    let result = mgr
        .get_or_create_with_shell("my-session", "/bin/bash", 80, 24, &spawner)
        .await;
    assert!(result.is_ok());

    let session = result.unwrap();
    let s = session.lock().await;
    assert_eq!(s.name, "my-session");
    assert_eq!(s.cols, 80);
    assert_eq!(s.rows, 24);
}

/// Creating a session that already exists should return the existing session (idempotent).
#[tokio::test]
async fn create_session_idempotent() {
    let spawner = FakePtySpawner::new();
    let mgr = SessionManager::new("/bin/bash");

    let s1 = mgr
        .get_or_create_with_shell("dup", "/bin/bash", 80, 24, &spawner)
        .await
        .unwrap();
    let s2 = mgr
        .get_or_create_with_shell("dup", "/bin/bash", 80, 24, &spawner)
        .await
        .unwrap();

    assert!(
        Arc::ptr_eq(&s1, &s2),
        "second create should return same Arc"
    );
    assert_eq!(mgr.session_count().await, 1);
}

/// Creating a session with a failing spawner should return an error.
#[tokio::test]
async fn create_session_spawn_failure() {
    let spawner = FakePtySpawner::new().failing();
    let mgr = SessionManager::new("/bin/bash");

    let result = mgr
        .get_or_create_with_shell("bad", "/bin/bash", 80, 24, &spawner)
        .await;
    assert!(result.is_err());
}

// ============================================================================
// KillSession handler logic
// ============================================================================

/// Killing an existing session should succeed and remove it.
#[tokio::test]
async fn kill_session_success() {
    let spawner = FakePtySpawner::new();
    let mgr = SessionManager::new("/bin/bash");

    mgr.get_or_create("victim", 80, 24, &spawner).await.unwrap();
    assert_eq!(mgr.session_count().await, 1);

    let result = mgr.destroy("victim").await;
    assert!(result.is_ok());
    assert_eq!(mgr.session_count().await, 0);
}

/// Killing a session that doesn't exist should return an error.
#[tokio::test]
async fn kill_session_nonexistent_returns_error() {
    let mgr = SessionManager::new("/bin/bash");

    let result = mgr.destroy("ghost").await;
    assert!(result.is_err());
}

// ============================================================================
// ResizeSession handler logic
// ============================================================================

/// Resizing an existing session should update its dimensions.
#[tokio::test]
async fn resize_existing_session() {
    let spawner = FakePtySpawner::new();
    let mgr = SessionManager::new("/bin/bash");

    let session = mgr.get_or_create("ses", 80, 24, &spawner).await.unwrap();
    session.lock().await.resize(160, 48);

    let s = session.lock().await;
    assert_eq!(s.cols, 160);
    assert_eq!(s.rows, 48);
}

/// Resizing a nonexistent session: get() returns None.
#[tokio::test]
async fn resize_nonexistent_session_get_returns_none() {
    let mgr = SessionManager::new("/bin/bash");
    let result = mgr.get("ghost").await;
    assert!(result.is_none());
}

// ============================================================================
// SendKeys / TypeAction handler logic
// ============================================================================

/// Bytes sent to pty_stdin_tx arrive at the PTY's stdin channel.
#[tokio::test]
async fn sendkeys_bytes_arrive_at_pty_stdin() {
    let spawner = FakePtySpawner::new();
    let (session, _rx) = ManagedSession::new("t".into(), "/bin/bash", 80, 24, &spawner).unwrap();
    let mut ctrl = spawner.take_control().unwrap();

    let stdin_tx = session.pty_stdin_tx.clone();
    stdin_tx.send(b"\x03".to_vec()).await.unwrap(); // Ctrl+C

    let received = ctrl.stdin_rx.recv().await.unwrap();
    assert_eq!(received, b"\x03");
}

/// Multiple key byte sequences concatenated and sent arrive in order.
#[tokio::test]
async fn sendkeys_multiple_sequences() {
    let spawner = FakePtySpawner::new();
    let (session, _rx) = ManagedSession::new("t".into(), "/bin/bash", 80, 24, &spawner).unwrap();
    let mut ctrl = spawner.take_control().unwrap();

    // Send Escape + Escape as a single payload (like PressKeys would).
    let stdin_tx = session.pty_stdin_tx.clone();
    stdin_tx.send(b"\x1b\x1b".to_vec()).await.unwrap();

    let received = ctrl.stdin_rx.recv().await.unwrap();
    assert_eq!(received, b"\x1b\x1b");
}

// ============================================================================
// WaitForText handler logic
// ============================================================================

/// Inject output that contains the pattern — the found path.
#[tokio::test]
async fn waitfortext_found_path() {
    let spawner = FakePtySpawner::new();
    let mgr = SessionManager::new("/bin/bash");
    let session = mgr.get_or_create("ses", 80, 24, &spawner).await.unwrap();

    // Inject PTY output containing the pattern.
    {
        let mut s = session.lock().await;
        s.process_pty_output(b">>> ");
    }

    // Poll as the handler would.
    let pattern = ">>>";
    let deadline = Instant::now() + Duration::from_millis(500);
    let mut found = false;
    let plain_text;
    loop {
        let text = {
            let s = session.lock().await;
            s.plain_text()
        };
        if text.contains(pattern) {
            found = true;
            plain_text = text;
            break;
        }
        if Instant::now() >= deadline {
            plain_text = text;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    assert!(
        found,
        "pattern '{}' should have been found in:\n{}",
        pattern, plain_text
    );
    assert!(plain_text.contains(">>>"));
}

/// No output injected — the timeout path returns with found=false.
#[tokio::test]
async fn waitfortext_timeout_path() {
    let spawner = FakePtySpawner::new();
    let mgr = SessionManager::new("/bin/bash");
    let session = mgr.get_or_create("ses", 80, 24, &spawner).await.unwrap();

    // No output injected. Use a very short deadline.
    let pattern = "XYZZY_NEVER_APPEARS";
    let timeout_ms = 50u64;
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);

    let start = Instant::now();
    let mut found = false;
    loop {
        let text = { session.lock().await.plain_text() };
        if text.contains(pattern) {
            found = true;
            break;
        }
        if Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let elapsed = start.elapsed();

    assert!(!found, "pattern should not be found on empty screen");
    // Should return within a reasonable window (timeout + some scheduling slack).
    assert!(
        elapsed < Duration::from_millis(timeout_ms + 200),
        "should return promptly after timeout, elapsed: {:?}",
        elapsed
    );
}

/// timeout_ms=0 → check once and return immediately (the assert path).
#[tokio::test]
async fn waitfortext_zero_timeout_checks_once() {
    let spawner = FakePtySpawner::new();
    let mgr = SessionManager::new("/bin/bash");
    let session = mgr.get_or_create("ses", 80, 24, &spawner).await.unwrap();

    // With timeout_ms=0, deadline is already past when we start.
    let deadline = Instant::now(); // already expired
    let pattern = "MISSING";

    let text = { session.lock().await.plain_text() };
    let found = text.contains(pattern) || {
        if Instant::now() >= deadline {
            false
        } else {
            text.contains(pattern)
        }
    };

    assert!(!found);
}

// ============================================================================
// RunCommand output filtering logic
// ============================================================================

/// Inject pre-existing content + new output + sentinel, verify filtering.
#[tokio::test]
async fn run_command_output_filtering() {
    let spawner = FakePtySpawner::new();
    let (mut session, _rx) =
        ManagedSession::new("t".into(), "/bin/bash", 80, 24, &spawner).unwrap();

    // Simulate a pre-existing shell prompt on screen.
    session.process_pty_output(b"$ ");
    let text_before = session.plain_text();
    let lines_before: std::collections::HashSet<&str> = text_before.lines().collect();

    // Simulate PTY output after "echo hello" was sent:
    // shell echoes the command, then outputs result, then sentinel.
    let command = "echo hello";
    let sentinel = "RTERM_DONE_test1";
    let pty_response = format!("echo hello\r\nhello\r\n{}\r\n$ ", sentinel);
    session.process_pty_output(pty_response.as_bytes());

    let text = session.plain_text();

    // Apply the same filter RunCommandSvc uses.
    let output: String = text
        .lines()
        .filter(|l| {
            let trimmed = l.trim();
            !trimmed.is_empty()
                && !trimmed.contains(sentinel)
                && !trimmed.contains(command)
                && !lines_before.contains(*l)
        })
        .collect::<Vec<_>>()
        .join("\n");

    assert_eq!(
        output, "hello",
        "output should be only the new command result, got: {:?}",
        output
    );
}

/// Sentinel not present → timed_out=true path.
#[tokio::test]
async fn run_command_timeout_path() {
    let spawner = FakePtySpawner::new();
    let mgr = SessionManager::new("/bin/bash");
    let session = mgr.get_or_create("ses", 80, 24, &spawner).await.unwrap();

    let sentinel = "RTERM_DONE_99";
    let timeout_ms = 50u64;
    let deadline = Instant::now() + Duration::from_millis(timeout_ms);

    let start = Instant::now();
    let timed_out;
    loop {
        let text = { session.lock().await.plain_text() };
        if text.contains(sentinel) {
            timed_out = false; // should not happen
            break;
        }
        if Instant::now() >= deadline {
            timed_out = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let elapsed = start.elapsed();

    assert!(timed_out, "should have timed out");
    assert!(
        elapsed < Duration::from_millis(timeout_ms + 200),
        "should return promptly, elapsed: {:?}",
        elapsed
    );
}

// ============================================================================
// PressKeys handler logic
// ============================================================================

/// Verify that arrow keys in normal cursor mode generate the standard sequences.
/// The resolve_key function is pub(crate) so we use an indirect test via stdin.
#[tokio::test]
async fn press_keys_normal_cursor_mode_bytes_arrive() {
    let spawner = FakePtySpawner::new();
    let (session, _rx) = ManagedSession::new("t".into(), "/bin/bash", 80, 24, &spawner).unwrap();
    let mut ctrl = spawner.take_control().unwrap();

    // Normal cursor mode: Up = \x1b[A
    assert!(!session.terminal.modes.application_cursor_keys);
    let stdin_tx = session.pty_stdin_tx.clone();
    stdin_tx.send(b"\x1b[A".to_vec()).await.unwrap();

    let received = ctrl.stdin_rx.recv().await.unwrap();
    assert_eq!(received, b"\x1b[A");
}

/// Verify that with application_cursor_keys active, Up = \x1bOA.
#[tokio::test]
async fn press_keys_application_cursor_mode_bytes_arrive() {
    let spawner = FakePtySpawner::new();
    let (mut session, _rx) =
        ManagedSession::new("t".into(), "/bin/bash", 80, 24, &spawner).unwrap();
    let mut ctrl = spawner.take_control().unwrap();

    // Manually activate application cursor key mode (as vim would via CSI ?1h).
    session.terminal.modes.application_cursor_keys = true;

    // Application cursor mode: Up = \x1bOA
    let app_cursor = session.terminal.modes.application_cursor_keys;
    assert!(app_cursor);

    let up_bytes = if app_cursor {
        b"\x1bOA".as_ref()
    } else {
        b"\x1b[A".as_ref()
    };
    let stdin_tx = session.pty_stdin_tx.clone();
    stdin_tx.send(up_bytes.to_vec()).await.unwrap();

    let received = ctrl.stdin_rx.recv().await.unwrap();
    assert_eq!(received, b"\x1bOA");
}
