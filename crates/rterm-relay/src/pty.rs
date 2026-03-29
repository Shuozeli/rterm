use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::io::{Read, Write};
use tokio::sync::mpsc;
use tracing::{debug, error};

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

/// Fake PTY spawner for tests. Uses in-memory channels.
#[cfg(test)]
pub mod fake {
    use super::*;

    pub struct FakePtySpawner {
        /// Pre-loaded stdout data. Sent to the PTY handle's stdout_rx.
        pub stdout_data: Vec<Vec<u8>>,
        /// If true, spawn returns an error.
        pub fail: bool,
    }

    impl FakePtySpawner {
        pub fn new() -> Self {
            Self {
                stdout_data: Vec::new(),
                fail: false,
            }
        }

        pub fn with_stdout(mut self, data: Vec<Vec<u8>>) -> Self {
            self.stdout_data = data;
            self
        }

        pub fn failing(mut self) -> Self {
            self.fail = true;
            self
        }
    }

    /// Returned alongside PtyHandle so tests can read what was sent to stdin.
    pub struct FakePtyControl {
        pub stdin_rx: mpsc::Receiver<Vec<u8>>,
        pub resize_rx: mpsc::Receiver<(u16, u16)>,
    }

    impl PtySpawner for FakePtySpawner {
        fn spawn(
            &self,
            _shell: &str,
            _cols: u16,
            _rows: u16,
        ) -> Result<PtyHandle, Box<dyn std::error::Error + Send + Sync>> {
            if self.fail {
                return Err("fake PTY spawn failure".into());
            }

            let (stdin_tx, _stdin_rx) = mpsc::channel(64);
            let (stdout_tx, stdout_rx) = mpsc::channel(64);
            let (resize_tx, _resize_rx) = mpsc::channel(8);

            // Send pre-loaded stdout data.
            let data = self.stdout_data.clone();
            tokio::spawn(async move {
                for chunk in data {
                    if stdout_tx.send(chunk).await.is_err() {
                        break;
                    }
                }
                // Drop stdout_tx to signal EOF.
            });

            Ok(PtyHandle {
                stdin_tx,
                stdout_rx,
                resize_tx,
            })
        }
    }
}

// Backward compat: PtySession delegates to RealPtySpawner.
pub struct PtySession;

impl PtySession {
    pub fn spawn(
        shell: &str,
        cols: u16,
        rows: u16,
    ) -> Result<PtyHandle, Box<dyn std::error::Error + Send + Sync>> {
        RealPtySpawner.spawn(shell, cols, rows)
    }
}

#[cfg(test)]
mod tests {
    use super::fake::*;
    use super::*;

    #[test]
    fn pty_handle_fields() {
        // PtyHandle is just a struct of channels — verify it exists.
        fn assert_send<T: Send>() {}
        assert_send::<PtyHandle>();
    }

    #[test]
    fn real_spawner_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<RealPtySpawner>();
    }

    #[tokio::test]
    async fn fake_spawner_returns_handle() {
        let spawner = FakePtySpawner::new();
        let handle = spawner.spawn("bash", 80, 24).unwrap();
        // Handle should have valid channels.
        drop(handle.stdin_tx);
        drop(handle.resize_tx);
    }

    #[tokio::test]
    async fn fake_spawner_sends_stdout() {
        let spawner = FakePtySpawner::new().with_stdout(vec![b"hello".to_vec(), b"world".to_vec()]);
        let mut handle = spawner.spawn("bash", 80, 24).unwrap();

        let msg1 = handle.stdout_rx.recv().await.unwrap();
        assert_eq!(msg1, b"hello");
        let msg2 = handle.stdout_rx.recv().await.unwrap();
        assert_eq!(msg2, b"world");
        // EOF.
        assert!(handle.stdout_rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn fake_spawner_can_fail() {
        let spawner = FakePtySpawner::new().failing();
        let result = spawner.spawn("bash", 80, 24);
        assert!(result.is_err());
    }
}
