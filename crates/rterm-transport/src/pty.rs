use async_trait::async_trait;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::thread::JoinHandle;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, error};

use crate::Transport;
use crate::error::TransportError;

/// A handle to a spawned PTY: channels for stdin, stdout, and resize.
pub struct PtyHandle {
    pub stdin_tx: mpsc::Sender<Vec<u8>>,
    pub stdout_rx: mpsc::Receiver<Vec<u8>>,
    pub resize_tx: mpsc::Sender<(u16, u16)>,
}

/// A handle to a spawned exec PTY: stdout channel and exit code receiver.
pub struct ExecHandle {
    /// Receives stdout chunks from the command.
    pub stdout_rx: mpsc::Receiver<Vec<u8>>,
    /// Receives the exit code (i32) when the command completes.
    pub exit_code_rx: oneshot::Receiver<i32>,
    #[allow(dead_code)]
    pub(crate) kill_tx: oneshot::Sender<()>,
    #[allow(dead_code)]
    pub(crate) thread: JoinHandle<()>,
}

/// Trait for spawning a PTY. Abstracted for testability.
pub trait PtySpawner: Send + Sync {
    fn spawn(
        &self,
        shell: &str,
        cols: u16,
        rows: u16,
    ) -> Result<PtyHandle, Box<dyn std::error::Error + Send + Sync>>;

    /// Spawn an ephemeral exec PTY that runs a single command then exits.
    fn spawn_exec(
        &self,
        command: &str,
        cwd: &str,
        cols: u16,
        rows: u16,
    ) -> Result<ExecHandle, Box<dyn std::error::Error + Send + Sync>>;
}

/// Real PTY spawner using portable-pty and the OS.
pub struct RealPtySpawner;

impl PtySpawner for RealPtySpawner {
    fn spawn(
        &self,
        shell: &str,
        cols: u16,
        rows: u16,
    ) -> Result<PtyHandle, Box<dyn std::error::Error + Send + Sync>> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        let mut cmd = CommandBuilder::new(shell);
        cmd.env("TERM", "xterm-256color");
        let _child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);

        let master = pair.master;
        let mut writer = master.take_writer()?;
        let mut reader = master.try_clone_reader()?;

        let (stdin_tx, mut stdin_rx) = mpsc::channel::<Vec<u8>>(64);
        std::thread::spawn(move || {
            while let Some(data) = stdin_rx.blocking_recv() {
                if writer.write_all(&data).is_err() {
                    break;
                }
            }
            debug!("PTY stdin writer thread exited");
        });

        let (stdout_tx, stdout_rx) = mpsc::channel::<Vec<u8>>(64);
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if stdout_tx.blocking_send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        debug!("PTY read error: {}", e);
                        break;
                    }
                }
            }
            debug!("PTY stdout reader thread exited");
        });

        let (resize_tx, mut resize_rx) = mpsc::channel::<(u16, u16)>(8);
        tokio::spawn(async move {
            while let Some((cols, rows)) = resize_rx.recv().await {
                if let Err(e) = master.resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                }) {
                    error!("PTY resize error: {}", e);
                }
            }
            debug!("PTY resize task exited, master dropped");
        });

        Ok(PtyHandle {
            stdin_tx,
            stdout_rx,
            resize_tx,
        })
    }

    fn spawn_exec(
        &self,
        command: &str,
        cwd: &str,
        cols: u16,
        rows: u16,
    ) -> Result<ExecHandle, Box<dyn std::error::Error + Send + Sync>> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;

        // Build the shell command: cd to cwd then run the command
        let full_command = format!("cd {} && {}", cwd, command);
        let mut cmd = CommandBuilder::new("bash");
        cmd.args(["-c", &full_command]);
        cmd.env("TERM", "xterm-256color");

        let mut child = pair.slave.spawn_command(cmd)?;
        drop(pair.slave);

        let master = pair.master;
        let mut reader = master.try_clone_reader()?;

        let (stdout_tx, stdout_rx) = mpsc::channel::<Vec<u8>>(64);
        let (exit_code_tx, exit_code_rx) = oneshot::channel();
        let (kill_tx, mut kill_rx) = oneshot::channel();

        // Clone the child killer before moving child into the thread.
        // We store this in ExecHandle so the caller can explicitly kill the child.
        let mut child_killer = child.as_mut().clone_killer();

        // Spawn thread to read stdout and forward to channel
        let _stdout_thread = std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        if stdout_tx.blocking_send(buf[..n].to_vec()).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        debug!("exec PTY read error: {}", e);
                        break;
                    }
                }
            }
            debug!("exec PTY stdout reader thread exited");
        });

        // Spawn thread to wait for child exit and listen for kill signal.
        // If kill_rx receives before child exits, call kill() to send SIGKILL.
        let thread = std::thread::spawn(move || {
            // Use select-style polling: check kill_rx first, then try wait()
            loop {
                // Check if we should kill the child
                if kill_rx.try_recv().is_ok() {
                    // Kill signal received - send SIGKILL
                    let _ = child_killer.kill();
                    debug!("exec PTY child killed via SIGKILL");
                    let _ = exit_code_tx.send(-1);
                    return;
                }

                // Try to wait with WNOHANG equivalent via try_wait
                match child.as_mut().try_wait() {
                    Ok(Some(es)) => {
                        // Child exited
                        let code = es.exit_code() as i32;
                        debug!("exec PTY child exited with code: {}", code);
                        let _ = exit_code_tx.send(code);
                        return;
                    }
                    Ok(None) => {
                        // Child still running, loop and check kill_rx again
                        std::hint::spin_loop();
                    }
                    Err(e) => {
                        // wait failed
                        debug!("exec PTY child wait error: {}", e);
                        let _ = exit_code_tx.send(-1);
                        return;
                    }
                }
            }
        });

        Ok(ExecHandle {
            stdout_rx,
            exit_code_rx,
            kill_tx,
            thread,
        })
    }
}

/// Transport adapter over a PtyHandle.
pub struct PtyTransport {
    stdout_rx: mpsc::Receiver<Vec<u8>>,
    stdin_tx: mpsc::Sender<Vec<u8>>,
    resize_tx: mpsc::Sender<(u16, u16)>,
}

impl PtyTransport {
    /// Create a PtyTransport from a PtyHandle, consuming it.
    pub fn from_handle(handle: PtyHandle) -> Self {
        Self {
            stdout_rx: handle.stdout_rx,
            stdin_tx: handle.stdin_tx,
            resize_tx: handle.resize_tx,
        }
    }
}

#[async_trait]
impl Transport for PtyTransport {
    async fn read(&mut self) -> Result<Vec<u8>, TransportError> {
        self.stdout_rx.recv().await.ok_or(TransportError::Closed)
    }

    async fn write(&mut self, data: &[u8]) -> Result<(), TransportError> {
        self.stdin_tx
            .send(data.to_vec())
            .await
            .map_err(|_| TransportError::Closed)
    }

    async fn resize(&mut self, cols: u16, rows: u16) -> Result<(), TransportError> {
        self.resize_tx
            .send((cols, rows))
            .await
            .map_err(|_| TransportError::Closed)
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(()) // channels drop on close
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pty_handle_fields() {
        fn assert_send<T: Send>() {}
        assert_send::<PtyHandle>();
    }

    #[test]
    fn real_spawner_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<RealPtySpawner>();
    }

    #[test]
    fn pty_transport_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<PtyTransport>();
    }
}
