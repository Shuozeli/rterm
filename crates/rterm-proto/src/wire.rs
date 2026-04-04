//! Wire protocol: length-prefixed FlatBuffers framing.
//!
//! All rterm transports (WebTransport, WebSocket, gRPC) use the same framing:
//! `[4-byte big-endian u32 length] [FlatBuffers payload]`
//!
//! This module provides the canonical implementation to avoid code duplication
//! and ensure all transports produce identical wire format.

/// Encode a payload with 4-byte big-endian length prefix.
///
/// # Wire Format
/// ```text
/// [0..4]: u32 BE length of payload
/// [4..]:  FlatBuffers bytes
/// ```
///
/// # Example
/// ```
/// use rterm_proto::wire::{encode_message, strip_length_prefix};
/// let payload = b"hello".to_vec();
/// let encoded = encode_message(payload);
/// assert_eq!(encoded.len(), 4 + 5);
/// assert_eq!(&encoded[0..4], &5u32.to_be_bytes());
/// assert_eq!(&encoded[4..], b"hello");
/// ```
pub fn encode_message(payload: Vec<u8>) -> Vec<u8> {
    let len = payload.len() as u32;
    let mut buf = Vec::with_capacity(4 + payload.len());
    buf.extend_from_slice(&len.to_be_bytes());
    buf.extend_from_slice(&payload);
    buf
}

/// Strip the 4-byte length prefix from a wire-format message.
///
/// Returns `Some(payload)` if the message is valid (at least 4 bytes with
/// sufficient length), or `None` if the message is truncated.
///
/// # Example
/// ```
/// use rterm_proto::wire::{encode_message, strip_length_prefix};
/// let wire = encode_message(b"test".to_vec());
/// let payload = strip_length_prefix(&wire).unwrap();
/// assert_eq!(payload, b"test");
/// ```
///
/// # Truncated Example
/// ```
/// use rterm_proto::wire::strip_length_prefix;
/// let wire = vec![0, 0, 0, 10]; // claims 10 bytes but none follow
/// assert!(strip_length_prefix(&wire).is_none());
/// ```
pub fn strip_length_prefix(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < 4 {
        return None;
    }
    let len = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
    if data.len() < 4 + len {
        return None;
    }
    Some(data[4..4 + len].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip() {
        let payload = b"hello world".to_vec();
        let encoded = encode_message(payload.clone());
        assert_eq!(encoded.len(), 4 + payload.len());

        let decoded = strip_length_prefix(&encoded).unwrap();
        assert_eq!(decoded, payload);
    }

    #[test]
    fn partial_read_truncated() {
        let payload = b"test".to_vec();
        let encoded = encode_message(payload);

        // Only length prefix
        let truncated = &encoded[..4];
        assert!(strip_length_prefix(truncated).is_none());

        // Length prefix + partial payload
        let truncated = &encoded[..6];
        assert!(strip_length_prefix(truncated).is_none());
    }

    #[test]
    fn empty_payload() {
        let payload = vec![];
        let encoded = encode_message(payload);
        assert_eq!(encoded.len(), 4);

        let decoded = strip_length_prefix(&encoded).unwrap();
        assert!(decoded.is_empty());
    }

    #[test]
    fn multiple_messages() {
        let msgs = vec![b"one".to_vec(), b"two".to_vec(), b"three".to_vec()];
        let mut wire = Vec::new();
        for m in &msgs {
            wire.extend_from_slice(&encode_message(m.clone()));
        }

        let mut offset = 0;
        for expected in &msgs {
            let remaining = &wire[offset..];
            let payload = strip_length_prefix(remaining).unwrap();
            assert_eq!(payload, *expected);
            offset += 4 + expected.len();
        }
    }

    #[test]
    fn exact_boundary() {
        // Length claims 4 bytes, exactly 4 bytes follow
        let encoded = encode_message(b"test".to_vec());
        assert_eq!(encoded.len(), 8);
        assert_eq!(strip_length_prefix(&encoded).unwrap(), b"test");
    }

    #[test]
    fn zero_length_claimed() {
        // Can have zero-length payload
        let encoded = encode_message(vec![]);
        assert_eq!(encoded.len(), 4);
        assert!(strip_length_prefix(&encoded).unwrap().is_empty());
    }
}
