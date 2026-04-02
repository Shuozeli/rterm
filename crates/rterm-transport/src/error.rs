use std::fmt;

/// Errors that can occur during transport operations.
#[derive(Debug)]
pub enum TransportError {
    /// The transport channel has been closed.
    Closed,
    /// A spawn or I/O error from the underlying transport.
    Spawn(Box<dyn std::error::Error + Send + Sync>),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TransportError::Closed => write!(f, "transport closed"),
            TransportError::Spawn(e) => write!(f, "transport spawn error: {e}"),
        }
    }
}

impl std::error::Error for TransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TransportError::Closed => None,
            TransportError::Spawn(e) => Some(e.as_ref()),
        }
    }
}
