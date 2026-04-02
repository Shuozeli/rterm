//! Re-exports from rterm-transport for backward compatibility.

pub use rterm_transport::{PtyHandle, PtySpawner, RealPtySpawner};

pub mod fake {
    pub use rterm_transport::{FakePtyControl, FakePtySpawner};
}

#[cfg(test)]
mod tests {
    use super::fake::*;
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
}
