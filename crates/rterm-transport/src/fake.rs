use async_trait::async_trait;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

use crate::Transport;
use crate::error::TransportError;
use crate::pty::{PtyHandle, PtySpawner};

/// Control handle for verifying what the session sent to the PTY.
pub struct FakePtyControl {
    pub stdin_rx: mpsc::Receiver<Vec<u8>>,
    pub resize_rx: mpsc::Receiver<(u16, u16)>,
}

#[derive(Default)]
pub struct FakePtySpawner {
    pub stdout_data: Vec<Vec<u8>>,
    pub fail: bool,
    control: Arc<Mutex<Option<FakePtyControl>>>,
}

impl FakePtySpawner {
    pub fn new() -> Self {
        Self {
            stdout_data: Vec::new(),
            fail: false,
            control: Arc::new(Mutex::new(None)),
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

    /// Take the control handle after spawn() was called.
    /// Returns None if spawn wasn't called or already taken.
    pub fn take_control(&self) -> Option<FakePtyControl> {
        self.control.lock().unwrap().take()
    }
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

        let (stdin_tx, stdin_rx) = mpsc::channel(64);
        let (stdout_tx, stdout_rx) = mpsc::channel(64);
        let (resize_tx, resize_rx) = mpsc::channel(8);

        // Store control handles so tests can read stdin/resize.
        *self.control.lock().unwrap() = Some(FakePtyControl {
            stdin_rx,
            resize_rx,
        });

        // Send pre-loaded stdout data.
        let data = self.stdout_data.clone();
        tokio::spawn(async move {
            for chunk in data {
                if stdout_tx.send(chunk).await.is_err() {
                    break;
                }
            }
        });

        Ok(PtyHandle {
            stdin_tx,
            stdout_rx,
            resize_tx,
        })
    }
}

/// A fake Transport backed by in-memory channels, for testing.
pub struct FakeTransport {
    stdout_rx: mpsc::Receiver<Vec<u8>>,
    stdin_tx: mpsc::Sender<Vec<u8>>,
    resize_tx: mpsc::Sender<(u16, u16)>,
}

/// Control handle for a FakeTransport, allowing tests to feed data and observe writes.
pub struct FakeTransportControl {
    pub stdout_tx: mpsc::Sender<Vec<u8>>,
    pub stdin_rx: mpsc::Receiver<Vec<u8>>,
    pub resize_rx: mpsc::Receiver<(u16, u16)>,
}

impl FakeTransport {
    /// Create a FakeTransport and its control handle.
    pub fn new() -> (Self, FakeTransportControl) {
        let (stdin_tx, stdin_rx) = mpsc::channel(64);
        let (stdout_tx, stdout_rx) = mpsc::channel(64);
        let (resize_tx, resize_rx) = mpsc::channel(8);

        (
            Self {
                stdout_rx,
                stdin_tx,
                resize_tx,
            },
            FakeTransportControl {
                stdout_tx,
                stdin_rx,
                resize_rx,
            },
        )
    }
}

#[async_trait]
impl Transport for FakeTransport {
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
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fake_spawner_returns_handle() {
        let spawner = FakePtySpawner::new();
        let handle = spawner.spawn("bash", 80, 24).unwrap();
        drop(handle.stdin_tx);
        drop(handle.resize_tx);
    }

    #[tokio::test]
    async fn fake_spawner_sends_stdout() {
        let spawner = FakePtySpawner::new().with_stdout(vec![b"hello".to_vec(), b"world".to_vec()]);
        let mut handle = spawner.spawn("bash", 80, 24).unwrap();
        assert_eq!(handle.stdout_rx.recv().await.unwrap(), b"hello");
        assert_eq!(handle.stdout_rx.recv().await.unwrap(), b"world");
        assert!(handle.stdout_rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn fake_spawner_can_fail() {
        let spawner = FakePtySpawner::new().failing();
        assert!(spawner.spawn("bash", 80, 24).is_err());
    }

    #[tokio::test]
    async fn fake_spawner_control_reads_stdin() {
        let spawner = FakePtySpawner::new();
        let handle = spawner.spawn("bash", 80, 24).unwrap();
        let mut ctrl = spawner.take_control().unwrap();

        handle.stdin_tx.send(b"test input".to_vec()).await.unwrap();
        let received = ctrl.stdin_rx.recv().await.unwrap();
        assert_eq!(received, b"test input");
    }

    #[tokio::test]
    async fn fake_spawner_control_reads_resize() {
        let spawner = FakePtySpawner::new();
        let handle = spawner.spawn("bash", 80, 24).unwrap();
        let mut ctrl = spawner.take_control().unwrap();

        handle.resize_tx.send((120, 40)).await.unwrap();
        let (cols, rows) = ctrl.resize_rx.recv().await.unwrap();
        assert_eq!((cols, rows), (120, 40));
    }

    #[tokio::test]
    async fn fake_transport_read_write() {
        let (mut transport, mut ctrl) = FakeTransport::new();

        ctrl.stdout_tx.send(b"hello".to_vec()).await.unwrap();
        let data = transport.read().await.unwrap();
        assert_eq!(data, b"hello");

        transport.write(b"input").await.unwrap();
        let received = ctrl.stdin_rx.recv().await.unwrap();
        assert_eq!(received, b"input");
    }

    #[tokio::test]
    async fn fake_transport_resize() {
        let (mut transport, mut ctrl) = FakeTransport::new();

        transport.resize(120, 40).await.unwrap();
        let (cols, rows) = ctrl.resize_rx.recv().await.unwrap();
        assert_eq!((cols, rows), (120, 40));
    }

    #[tokio::test]
    async fn fake_transport_close() {
        let (mut transport, _ctrl) = FakeTransport::new();
        assert!(transport.close().await.is_ok());
    }

    #[tokio::test]
    async fn fake_transport_read_closed() {
        let (mut transport, ctrl) = FakeTransport::new();
        drop(ctrl.stdout_tx);
        assert!(matches!(
            transport.read().await,
            Err(TransportError::Closed)
        ));
    }
}
