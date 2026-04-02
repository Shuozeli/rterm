use async_trait::async_trait;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use tokio::sync::mpsc;
use tracing::{debug, error};

use crate::Transport;
use crate::error::TransportError;

/// A handle to a spawned PTY: channels for stdin, stdout, and resize.
pub struct PtyHandle {
    pub stdin_tx: mpsc::Sender<Vec<u8>>,
    pub stdout_rx: mpsc::Receiver<Vec<u8>>,
    pub resize_tx: mpsc::Sender<(u16, u16)>,
}

/// Trait for spawning a PTY. Abstracted for testability.
pub trait PtySpawner: Send + Sync {
    fn spawn(
        &self,
        shell: &str,
        cols: u16,
        rows: u16,
    ) -> Result<PtyHandle, Box<dyn std::error::Error + Send + Sync>>;
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
