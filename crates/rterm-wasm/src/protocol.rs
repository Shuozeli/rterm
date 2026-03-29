/// Simple length-prefixed FlatBuffers protocol over WebTransport.
///
/// Each message is: [4-byte big-endian length] [flatbuffers payload]
///
/// This is simpler than full gRPC framing since we're on a raw bidi stream,
/// not HTTP request-response pairs.
/// Encode a FlatBuffers message with length prefix.
pub fn encode_message(payload: &[u8]) -> Vec<u8> {
    let len = payload.len() as u32;
    let mut buf = Vec::with_capacity(4 + payload.len());
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(payload);
    buf
}

/// Receive buffer that accumulates partial reads and yields complete messages.
pub struct RecvBuffer {
    buf: Vec<u8>,
}

impl RecvBuffer {
    pub fn new() -> Self {
        Self { buf: Vec::new() }
    }

    /// Push received bytes into the buffer.
    pub fn push(&mut self, data: &[u8]) {
        self.buf.extend_from_slice(data);
    }

    /// Try to extract a complete message. Returns the payload (without length prefix).
    pub fn try_read_message(&mut self) -> Option<Vec<u8>> {
        if self.buf.len() < 4 {
            return None;
        }
        let len = u32::from_be_bytes([self.buf[0], self.buf[1], self.buf[2], self.buf[3]]) as usize;
        if self.buf.len() < 4 + len {
            return None;
        }
        let payload = self.buf[4..4 + len].to_vec();
        self.buf.drain(..4 + len);
        Some(payload)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip() {
        let payload = b"hello world";
        let encoded = encode_message(payload);
        assert_eq!(encoded.len(), 4 + payload.len());

        let mut buf = RecvBuffer::new();
        buf.push(&encoded);
        let decoded = buf.try_read_message().unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn partial_read() {
        let payload = b"test";
        let encoded = encode_message(payload);

        let mut buf = RecvBuffer::new();
        // Push only the length prefix.
        buf.push(&encoded[..4]);
        assert!(buf.try_read_message().is_none());

        // Push the rest.
        buf.push(&encoded[4..]);
        let decoded = buf.try_read_message().unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn multiple_messages() {
        let mut buf = RecvBuffer::new();
        buf.push(&encode_message(b"one"));
        buf.push(&encode_message(b"two"));

        assert_eq!(buf.try_read_message().unwrap(), b"one");
        assert_eq!(buf.try_read_message().unwrap(), b"two");
        assert!(buf.try_read_message().is_none());
    }

    #[test]
    fn empty_message() {
        let mut buf = RecvBuffer::new();
        buf.push(&encode_message(b""));
        let decoded = buf.try_read_message().unwrap();
        assert!(decoded.is_empty());
    }
}
