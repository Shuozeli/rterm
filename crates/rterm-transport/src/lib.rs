pub mod error;
pub mod fake;
pub mod pty;
pub mod ssh;

use async_trait::async_trait;

pub use error::TransportError;
pub use fake::{FakePtyControl, FakePtySpawner, FakeTransport, FakeTransportControl};
pub use pty::{PtyHandle, PtySpawner, PtyTransport, RealPtySpawner};
pub use ssh::{SshAuth, SshConfig, SshTransport};

/// Async transport abstraction for terminal I/O (PTY, SSH, etc.).
#[async_trait]
pub trait Transport: Send + Sync {
    /// Read output from the transport (e.g., PTY stdout).
    async fn read(&mut self) -> Result<Vec<u8>, TransportError>;

    /// Write input to the transport (e.g., PTY stdin).
    async fn write(&mut self, data: &[u8]) -> Result<(), TransportError>;

    /// Resize the terminal.
    async fn resize(&mut self, cols: u16, rows: u16) -> Result<(), TransportError>;

    /// Close the transport.
    async fn close(&mut self) -> Result<(), TransportError>;
}
