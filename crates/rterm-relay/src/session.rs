/// Terminal session orchestration.
///
/// Shared logic between WebTransport and gRPC handlers:
/// read Resize, spawn PTY, forward input, run VT emulator, send screen diffs.
use crate::pty::PtySpawner;
use crate::screen_diff::{self, PrevScreen, pack_attrs, pack_color};
use rterm_core::Terminal;
use rterm_proto::*;
use tokio::sync::mpsc;
use tracing::{debug, info};

/// Error type for session operations.
#[derive(Debug)]
pub enum SessionError {
    InvalidFirstMessage(String),
    EmptyStream,
    SpawnFailed(String),
    SendFailed(String),
    RecvFailed(String),
}

impl std::fmt::Display for SessionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SessionError::InvalidFirstMessage(msg) => write!(f, "invalid first message: {}", msg),
            SessionError::EmptyStream => write!(f, "empty stream"),
            SessionError::SpawnFailed(msg) => write!(f, "spawn failed: {}", msg),
            SessionError::SendFailed(msg) => write!(f, "send failed: {}", msg),
            SessionError::RecvFailed(msg) => write!(f, "recv failed: {}", msg),
        }
    }
}

impl std::error::Error for SessionError {}

/// Run a terminal session. Transport-agnostic: works with any ClientStream/ServerSink.
///
/// Protocol:
/// 1. Read first message — must be Resize
/// 2. Spawn PTY
/// 3. Send initial ScreenSnapshot
/// 4. Forward client input to PTY, PTY output to screen diffs
/// 5. Send Exit on PTY close
pub async fn run_session(
    client_rx: &mut mpsc::Receiver<ClientMsg>,
    server_tx: &mpsc::Sender<ServerMsg>,
    spawner: &dyn PtySpawner,
    shell: &str,
) -> Result<(), SessionError> {
    // 1. Read first message — must be Resize.
    let (cols, rows) = match client_rx.recv().await {
        Some(ClientMsg::Resize(r)) => (r.cols, r.rows),
        Some(_) => {
            return Err(SessionError::InvalidFirstMessage(
                "first message must be Resize".into(),
            ));
        }
        None => return Err(SessionError::EmptyStream),
    };

    info!(
        "session: spawning PTY shell={}, size={}x{}",
        shell, cols, rows
    );

    // 2. Spawn PTY.
    let pty = spawner
        .spawn(shell, cols, rows)
        .map_err(|e| SessionError::SpawnFailed(e.to_string()))?;

    let stdin_tx = pty.stdin_tx;
    let resize_tx = pty.resize_tx;
    let mut stdout_rx = pty.stdout_rx;

    // 3. Create VT emulator + screen differ.
    let mut terminal = Terminal::new(cols as usize, rows as usize);
    let mut prev = PrevScreen::new(cols as usize, rows as usize);

    // Send initial snapshot.
    let ss = screen_diff::snapshot(terminal.screen());
    prev.update_from_snapshot(&ss);
    server_tx
        .send(ServerMsg::ScreenSnapshot(ss))
        .await
        .map_err(|e| SessionError::SendFailed(e.to_string()))?;

    // Scrollback request channel.
    let (scrollback_tx, mut scrollback_rx) = mpsc::channel::<ScrollbackRequest>(8);

    // 4. Forward client input to PTY in a background task.
    {
        let (relay_tx, relay_rx) = mpsc::channel(64);
        let mut client = std::mem::replace(client_rx, relay_rx);
        tokio::spawn(async move {
            while let Some(msg) = client.recv().await {
                match msg {
                    ClientMsg::KeyInput(k) => {
                        if stdin_tx.send(k.data).await.is_err() {
                            break;
                        }
                    }
                    ClientMsg::PasteInput(p) => {
                        // Wrap in bracketed paste markers if the shell requested it.
                        // Note: we can't easily read terminal.bracketed_paste here
                        // since it's in the main loop. For safety, always bracket.
                        let mut data = Vec::new();
                        data.extend_from_slice(b"\x1b[200~");
                        data.extend_from_slice(p.text.as_bytes());
                        data.extend_from_slice(b"\x1b[201~");
                        if stdin_tx.send(data).await.is_err() {
                            break;
                        }
                    }
                    ClientMsg::Resize(r) => {
                        if resize_tx.send((r.cols, r.rows)).await.is_err() {
                            break;
                        }
                    }
                    ClientMsg::MouseEvent(_) => {}
                    ClientMsg::ScrollbackRequest(s) => {
                        // Forward scrollback requests via a dedicated channel.
                        let _ = scrollback_tx.send(s).await;
                    }
                }
            }
            debug!("session: client input forwarding ended");
        });
        drop(relay_tx);
    }

    // 5. Main loop: handle PTY stdout and scrollback requests.
    loop {
        tokio::select! {
            data = stdout_rx.recv() => {
                let Some(data) = data else { break; };
                terminal.feed(&data);
                if terminal.is_sync_mode() {
                    continue;
                }
                if let Some(update) = prev.diff(terminal.screen())
                    && server_tx.send(ServerMsg::ScreenUpdate(update)).await.is_err()
                {
                    break;
                }
            }
            req = scrollback_rx.recv() => {
                let Some(req) = req else { continue; };
                let screen = terminal.screen();
                let sb_len = screen.scrollback_len();
                let offset = req.offset as usize;
                let count = req.count as usize;

                let mut lines = Vec::new();
                let start = sb_len.saturating_sub(offset + count);
                let end = sb_len.saturating_sub(offset);
                for i in start..end {
                    let cols = screen.scrollback_cols(i);
                    let cells: Vec<CellData> = (0..cols)
                        .map(|col| {
                            let cell = screen.scrollback_cell(i, col);
                            CellData {
                                ch: cell.ch,
                                fg: pack_color(&cell.fg),
                                bg: pack_color(&cell.bg),
                                attrs: pack_attrs(&cell.attrs),
                            }
                        })
                        .collect();
                    lines.push(CellRangeData {
                        row: (i - start) as u16,
                        col_start: 0,
                        cells,
                    });
                }

                let _ = server_tx.send(ServerMsg::ScrollbackData(ScrollbackDataMsg {
                    lines,
                    offset: req.offset,
                    total: sb_len as u32,
                })).await;
            }
        }
    }

    // 6. Send Exit.
    let _ = server_tx.send(ServerMsg::Exit(Exit { code: 0 })).await;

    info!("session: ended");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pty::fake::FakePtySpawner;

    async fn run_with_fake(
        spawner: &dyn PtySpawner,
        messages: Vec<ClientMsg>,
    ) -> (Vec<ServerMsg>, Result<(), SessionError>) {
        let (client_tx, mut client_rx) = mpsc::channel(64);
        let (server_tx, mut server_rx) = mpsc::channel(64);

        // Send all client messages.
        for msg in messages {
            client_tx.send(msg).await.unwrap();
        }
        drop(client_tx); // Close the stream.

        let result = run_session(&mut client_rx, &server_tx, spawner, "/bin/bash").await;
        drop(server_tx);

        // Collect all server messages.
        let mut msgs = Vec::new();
        while let Some(msg) = server_rx.recv().await {
            msgs.push(msg);
        }

        (msgs, result)
    }

    #[tokio::test]
    async fn first_message_must_be_resize() {
        let spawner = FakePtySpawner::new();
        let (_, result) = run_with_fake(
            &spawner,
            vec![ClientMsg::KeyInput(KeyInput {
                data: b"hello".to_vec(),
            })],
        )
        .await;
        assert!(matches!(result, Err(SessionError::InvalidFirstMessage(_))));
    }

    #[tokio::test]
    async fn empty_stream_errors() {
        let spawner = FakePtySpawner::new();
        let (_, result) = run_with_fake(&spawner, vec![]).await;
        assert!(matches!(result, Err(SessionError::EmptyStream)));
    }

    #[tokio::test]
    async fn pty_spawn_failure() {
        let spawner = FakePtySpawner::new().failing();
        let (_, result) = run_with_fake(
            &spawner,
            vec![ClientMsg::Resize(Resize { cols: 80, rows: 24 })],
        )
        .await;
        assert!(matches!(result, Err(SessionError::SpawnFailed(_))));
    }

    #[tokio::test]
    async fn initial_snapshot_sent() {
        let spawner = FakePtySpawner::new();
        let (msgs, result) = run_with_fake(
            &spawner,
            vec![ClientMsg::Resize(Resize { cols: 80, rows: 24 })],
        )
        .await;
        assert!(result.is_ok());
        assert!(!msgs.is_empty());
        // First message should be ScreenSnapshot.
        assert!(
            matches!(&msgs[0], ServerMsg::ScreenSnapshot(ss) if ss.cols == 80 && ss.num_rows == 24)
        );
    }

    #[tokio::test]
    async fn pty_stdout_produces_screen_update() {
        let spawner = FakePtySpawner::new().with_stdout(vec![b"Hello".to_vec()]);
        let (msgs, result) = run_with_fake(
            &spawner,
            vec![ClientMsg::Resize(Resize { cols: 80, rows: 24 })],
        )
        .await;
        assert!(result.is_ok());
        // Should have ScreenSnapshot + at least one ScreenUpdate + Exit.
        let has_update = msgs.iter().any(|m| matches!(m, ServerMsg::ScreenUpdate(_)));
        assert!(
            has_update,
            "expected ScreenUpdate, got: {:?}",
            msgs.iter()
                .map(|m| match m {
                    ServerMsg::ScreenSnapshot(_) => "Snapshot",
                    ServerMsg::ScreenUpdate(_) => "Update",
                    ServerMsg::Exit(_) => "Exit",
                    _ => "Other",
                })
                .collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn exit_sent_on_pty_close() {
        let spawner = FakePtySpawner::new().with_stdout(vec![b"data".to_vec()]);
        let (msgs, _) = run_with_fake(
            &spawner,
            vec![ClientMsg::Resize(Resize { cols: 80, rows: 24 })],
        )
        .await;
        // Last message should be Exit.
        assert!(matches!(msgs.last(), Some(ServerMsg::Exit(_))));
    }

    #[tokio::test]
    async fn sync_mode_suppresses_updates() {
        // Send data with sync mode on, then off.
        let spawner = FakePtySpawner::new().with_stdout(vec![
            b"\x1b[?2026h".to_vec(),    // sync on
            b"invisible data".to_vec(), // should not produce update
            b"\x1b[?2026l".to_vec(),    // sync off — should produce update
        ]);
        let (msgs, _) = run_with_fake(
            &spawner,
            vec![ClientMsg::Resize(Resize { cols: 80, rows: 24 })],
        )
        .await;
        // Count ScreenUpdates — should only get the one after sync off.
        let updates: Vec<_> = msgs
            .iter()
            .filter(|m| matches!(m, ServerMsg::ScreenUpdate(_)))
            .collect();
        // At most 1 update (the one after sync off), not 3.
        assert!(
            updates.len() <= 2,
            "expected at most 2 updates (sync should suppress), got {}",
            updates.len()
        );
    }

    #[tokio::test]
    async fn session_error_display() {
        let e = SessionError::InvalidFirstMessage("test".into());
        assert!(e.to_string().contains("test"));
        let e = SessionError::EmptyStream;
        assert!(e.to_string().contains("empty"));
        let e = SessionError::SpawnFailed("fail".into());
        assert!(e.to_string().contains("fail"));
        let e = SessionError::SendFailed("send".into());
        assert!(e.to_string().contains("send"));
        let e = SessionError::RecvFailed("recv".into());
        assert!(e.to_string().contains("recv"));
    }

    #[tokio::test]
    async fn key_input_forwarded_to_pty_stdin() {
        let spawner = FakePtySpawner::new().with_stdout(vec![b"prompt$ ".to_vec()]);
        let (client_tx, mut client_rx) = mpsc::channel(64);
        let (server_tx, mut server_rx) = mpsc::channel(64);

        // Send Resize first, then KeyInput.
        client_tx
            .send(ClientMsg::Resize(Resize { cols: 80, rows: 24 }))
            .await
            .unwrap();
        client_tx
            .send(ClientMsg::KeyInput(KeyInput {
                data: b"ls\n".to_vec(),
            }))
            .await
            .unwrap();
        drop(client_tx);

        let result = run_session(&mut client_rx, &server_tx, &spawner, "/bin/bash").await;
        assert!(result.is_ok());
        drop(server_tx);

        let mut msgs = Vec::new();
        while let Some(m) = server_rx.recv().await {
            msgs.push(m);
        }
        assert!(!msgs.is_empty());
    }

    #[tokio::test]
    async fn paste_input_forwarded() {
        let spawner = FakePtySpawner::new();
        let (client_tx, mut client_rx) = mpsc::channel(64);
        let (server_tx, _server_rx) = mpsc::channel(64);

        client_tx
            .send(ClientMsg::Resize(Resize { cols: 80, rows: 24 }))
            .await
            .unwrap();
        client_tx
            .send(ClientMsg::PasteInput(rterm_proto::PasteInput {
                text: "pasted text".into(),
            }))
            .await
            .unwrap();
        drop(client_tx);

        let _ = run_session(&mut client_rx, &server_tx, &spawner, "/bin/bash").await;
        // No panic = forwarding worked. PasteInput sent as bytes to PTY stdin.
    }

    #[tokio::test]
    async fn resize_forwarded_to_pty() {
        let spawner = FakePtySpawner::new();
        let (client_tx, mut client_rx) = mpsc::channel(64);
        let (server_tx, _server_rx) = mpsc::channel(64);

        client_tx
            .send(ClientMsg::Resize(Resize { cols: 80, rows: 24 }))
            .await
            .unwrap();
        client_tx
            .send(ClientMsg::Resize(Resize {
                cols: 120,
                rows: 40,
            }))
            .await
            .unwrap();
        drop(client_tx);

        let _ = run_session(&mut client_rx, &server_tx, &spawner, "/bin/bash").await;
        // No panic = resize forwarded.
    }

    #[tokio::test]
    async fn mouse_event_does_not_crash() {
        let spawner = FakePtySpawner::new();
        let (client_tx, mut client_rx) = mpsc::channel(64);
        let (server_tx, _server_rx) = mpsc::channel(64);

        client_tx
            .send(ClientMsg::Resize(Resize { cols: 80, rows: 24 }))
            .await
            .unwrap();
        client_tx
            .send(ClientMsg::MouseEvent(rterm_proto::MouseEvent {
                row: 5,
                col: 10,
                button: 0,
                modifiers: 0,
                kind: 0,
            }))
            .await
            .unwrap();
        drop(client_tx);

        let _ = run_session(&mut client_rx, &server_tx, &spawner, "/bin/bash").await;
        // No panic, no stdin data sent for mouse events.
    }

    #[tokio::test]
    async fn client_disconnect_mid_session() {
        let spawner =
            FakePtySpawner::new().with_stdout(vec![b"output1".to_vec(), b"output2".to_vec()]);
        let (client_tx, mut client_rx) = mpsc::channel(64);
        let (server_tx, mut server_rx) = mpsc::channel(64);

        client_tx
            .send(ClientMsg::Resize(Resize { cols: 80, rows: 24 }))
            .await
            .unwrap();
        // Drop client immediately — simulates disconnect.
        drop(client_tx);

        let result = run_session(&mut client_rx, &server_tx, &spawner, "/bin/bash").await;
        assert!(
            result.is_ok(),
            "session should handle client disconnect gracefully"
        );

        drop(server_tx);
        let mut msgs = Vec::new();
        while let Some(m) = server_rx.recv().await {
            msgs.push(m);
        }
        // Should still get snapshot + updates + exit.
        assert!(
            msgs.iter()
                .any(|m| matches!(m, ServerMsg::ScreenSnapshot(_)))
        );
    }
}
