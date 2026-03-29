/// FlatBuffers message encoding/decoding for the WASM client.
/// Uses the generated types directly (no grpc-core dependency).
use crate::generated::rterm::protocol as fbs;

use flatbuffers::FlatBufferBuilder;

/// Encode a Resize ClientMessage as FlatBuffers bytes.
pub fn encode_resize(cols: u16, rows: u16) -> Vec<u8> {
    let mut fbb = FlatBufferBuilder::new();
    let resize = fbs::Resize::create(&mut fbb, &fbs::ResizeArgs { cols, rows });
    let msg = fbs::ClientMessage::create(
        &mut fbb,
        &fbs::ClientMessageArgs {
            body_type: fbs::ClientBody::Resize,
            body: Some(resize.as_union_value()),
        },
    );
    fbb.finish(msg, None);
    fbb.finished_data().to_vec()
}

/// Encode a DataIn ClientMessage as FlatBuffers bytes.
pub fn encode_data_in(payload: &[u8]) -> Vec<u8> {
    let mut fbb = FlatBufferBuilder::new();
    let payload_vec = fbb.create_vector(payload);
    let data_in = fbs::DataIn::create(
        &mut fbb,
        &fbs::DataInArgs {
            payload: Some(payload_vec),
        },
    );
    let msg = fbs::ClientMessage::create(
        &mut fbb,
        &fbs::ClientMessageArgs {
            body_type: fbs::ClientBody::DataIn,
            body: Some(data_in.as_union_value()),
        },
    );
    fbb.finish(msg, None);
    fbb.finished_data().to_vec()
}

/// Decode a ServerMessage from FlatBuffers bytes.
/// Returns the payload bytes for DataOut, or None for other message types.
pub fn decode_server_msg(data: &[u8]) -> Result<ServerMsg, String> {
    let msg = flatbuffers::root::<fbs::ServerMessage>(data)
        .map_err(|e| format!("invalid ServerMessage: {e}"))?;
    match msg.body_type() {
        fbs::ServerBody::DataOut => {
            let d = msg.body_as_data_out().ok_or("missing DataOut body")?;
            let payload = d.payload().map(|p| p.bytes().to_vec()).unwrap_or_default();
            Ok(ServerMsg::DataOut(payload))
        }
        fbs::ServerBody::Exit => {
            let e = msg.body_as_exit().ok_or("missing Exit body")?;
            Ok(ServerMsg::Exit(e.code()))
        }
        fbs::ServerBody::Error => {
            let e = msg.body_as_error().ok_or("missing Error body")?;
            Ok(ServerMsg::Error(
                e.message().unwrap_or("").to_string(),
            ))
        }
        _ => Err("unknown ServerBody type".into()),
    }
}

pub enum ServerMsg {
    DataOut(Vec<u8>),
    Exit(i32),
    Error(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_resize_roundtrip() {
        let data = encode_resize(80, 24);
        let msg = flatbuffers::root::<fbs::ClientMessage>(&data).unwrap();
        assert_eq!(msg.body_type(), fbs::ClientBody::Resize);
        let r = msg.body_as_resize().unwrap();
        assert_eq!(r.cols(), 80);
        assert_eq!(r.rows(), 24);
    }

    #[test]
    fn encode_data_in_roundtrip() {
        let data = encode_data_in(b"hello");
        let msg = flatbuffers::root::<fbs::ClientMessage>(&data).unwrap();
        assert_eq!(msg.body_type(), fbs::ClientBody::DataIn);
        let d = msg.body_as_data_in().unwrap();
        assert_eq!(d.payload().unwrap().bytes(), b"hello");
    }
}
