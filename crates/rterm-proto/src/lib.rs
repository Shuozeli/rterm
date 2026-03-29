#[allow(unused_imports, dead_code, clippy::all, non_snake_case)]
mod generated;

// Re-export the raw FlatBuffers types for direct use.
pub use generated::rterm::protocol as fbs;

/// Re-export flatbuffers for consumers to build messages.
pub use flatbuffers;

use grpc_codec_flatbuffers::FlatBufferGrpcMessage;

// --- Owned message types for gRPC (Send + 'static) ---

/// Client-to-server: keyboard input bytes.
#[derive(Debug, Clone)]
pub struct DataIn {
    pub payload: Vec<u8>,
}

/// Client-to-server: terminal resize.
#[derive(Debug, Clone)]
pub struct Resize {
    pub cols: u16,
    pub rows: u16,
}

/// A client message: either data input or a resize event.
#[derive(Debug, Clone)]
pub enum ClientMsg {
    DataIn(DataIn),
    Resize(Resize),
}

/// Server-to-client: PTY output bytes.
#[derive(Debug, Clone)]
pub struct DataOut {
    pub payload: Vec<u8>,
}

/// Server-to-client: shell process exited.
#[derive(Debug, Clone)]
pub struct Exit {
    pub code: i32,
}

/// Server-to-client: error message.
#[derive(Debug, Clone)]
pub struct ServerError {
    pub message: String,
}

/// A server message: PTY output, exit, or error.
#[derive(Debug, Clone)]
pub enum ServerMsg {
    DataOut(DataOut),
    Exit(Exit),
    Error(ServerError),
}

// --- FlatBufferGrpcMessage implementations ---

impl FlatBufferGrpcMessage for ClientMsg {
    fn encode_flatbuffer(&self) -> Vec<u8> {
        let mut fbb = flatbuffers::FlatBufferBuilder::new();
        match self {
            ClientMsg::DataIn(d) => {
                let payload = fbb.create_vector(&d.payload);
                let data_in = fbs::DataIn::create(&mut fbb, &fbs::DataInArgs {
                    payload: Some(payload),
                });
                let msg = fbs::ClientMessage::create(&mut fbb, &fbs::ClientMessageArgs {
                    body_type: fbs::ClientBody::DataIn,
                    body: Some(data_in.as_union_value()),
                });
                fbb.finish(msg, None);
            }
            ClientMsg::Resize(r) => {
                let resize = fbs::Resize::create(&mut fbb, &fbs::ResizeArgs {
                    cols: r.cols,
                    rows: r.rows,
                });
                let msg = fbs::ClientMessage::create(&mut fbb, &fbs::ClientMessageArgs {
                    body_type: fbs::ClientBody::Resize,
                    body: Some(resize.as_union_value()),
                });
                fbb.finish(msg, None);
            }
        }
        fbb.finished_data().to_vec()
    }

    fn decode_flatbuffer(data: &[u8]) -> Result<Self, String> {
        let msg = flatbuffers::root::<fbs::ClientMessage>(data)
            .map_err(|e| format!("invalid ClientMessage: {e}"))?;
        match msg.body_type() {
            fbs::ClientBody::DataIn => {
                let d = msg.body_as_data_in().ok_or("missing DataIn body")?;
                Ok(ClientMsg::DataIn(DataIn {
                    payload: d.payload().map(|p| p.bytes().to_vec()).unwrap_or_default(),
                }))
            }
            fbs::ClientBody::Resize => {
                let r = msg.body_as_resize().ok_or("missing Resize body")?;
                Ok(ClientMsg::Resize(Resize {
                    cols: r.cols(),
                    rows: r.rows(),
                }))
            }
            _ => Err("unknown ClientBody type".into()),
        }
    }
}

impl FlatBufferGrpcMessage for ServerMsg {
    fn encode_flatbuffer(&self) -> Vec<u8> {
        let mut fbb = flatbuffers::FlatBufferBuilder::new();
        match self {
            ServerMsg::DataOut(d) => {
                let payload = fbb.create_vector(&d.payload);
                let data_out = fbs::DataOut::create(&mut fbb, &fbs::DataOutArgs {
                    payload: Some(payload),
                });
                let msg = fbs::ServerMessage::create(&mut fbb, &fbs::ServerMessageArgs {
                    body_type: fbs::ServerBody::DataOut,
                    body: Some(data_out.as_union_value()),
                });
                fbb.finish(msg, None);
            }
            ServerMsg::Exit(e) => {
                let exit = fbs::Exit::create(&mut fbb, &fbs::ExitArgs { code: e.code });
                let msg = fbs::ServerMessage::create(&mut fbb, &fbs::ServerMessageArgs {
                    body_type: fbs::ServerBody::Exit,
                    body: Some(exit.as_union_value()),
                });
                fbb.finish(msg, None);
            }
            ServerMsg::Error(e) => {
                let message = fbb.create_string(&e.message);
                let error = fbs::Error::create(&mut fbb, &fbs::ErrorArgs {
                    message: Some(message),
                });
                let msg = fbs::ServerMessage::create(&mut fbb, &fbs::ServerMessageArgs {
                    body_type: fbs::ServerBody::Error,
                    body: Some(error.as_union_value()),
                });
                fbb.finish(msg, None);
            }
        }
        fbb.finished_data().to_vec()
    }

    fn decode_flatbuffer(data: &[u8]) -> Result<Self, String> {
        let msg = flatbuffers::root::<fbs::ServerMessage>(data)
            .map_err(|e| format!("invalid ServerMessage: {e}"))?;
        match msg.body_type() {
            fbs::ServerBody::DataOut => {
                let d = msg.body_as_data_out().ok_or("missing DataOut body")?;
                Ok(ServerMsg::DataOut(DataOut {
                    payload: d.payload().map(|p| p.bytes().to_vec()).unwrap_or_default(),
                }))
            }
            fbs::ServerBody::Exit => {
                let e = msg.body_as_exit().ok_or("missing Exit body")?;
                Ok(ServerMsg::Exit(Exit { code: e.code() }))
            }
            fbs::ServerBody::Error => {
                let e = msg.body_as_error().ok_or("missing Error body")?;
                Ok(ServerMsg::Error(ServerError {
                    message: e.message().unwrap_or("").to_string(),
                }))
            }
            _ => Err("unknown ServerBody type".into()),
        }
    }
}

/// gRPC service path for the Terminal service.
pub const TERMINAL_SERVICE_PATH: &str = "/rterm.protocol.TerminalService/Session";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_client_data_in() {
        let msg = ClientMsg::DataIn(DataIn { payload: b"hello".to_vec() });
        let encoded = msg.encode_flatbuffer();
        let decoded = ClientMsg::decode_flatbuffer(&encoded).unwrap();
        match decoded {
            ClientMsg::DataIn(d) => assert_eq!(d.payload, b"hello"),
            _ => panic!("expected DataIn"),
        }
    }

    #[test]
    fn round_trip_client_resize() {
        let msg = ClientMsg::Resize(Resize { cols: 80, rows: 24 });
        let encoded = msg.encode_flatbuffer();
        let decoded = ClientMsg::decode_flatbuffer(&encoded).unwrap();
        match decoded {
            ClientMsg::Resize(r) => {
                assert_eq!(r.cols, 80);
                assert_eq!(r.rows, 24);
            }
            _ => panic!("expected Resize"),
        }
    }

    #[test]
    fn round_trip_server_data_out() {
        let msg = ServerMsg::DataOut(DataOut {
            payload: b"\x1b[31mred\x1b[0m".to_vec(),
        });
        let encoded = msg.encode_flatbuffer();
        let decoded = ServerMsg::decode_flatbuffer(&encoded).unwrap();
        match decoded {
            ServerMsg::DataOut(d) => assert_eq!(d.payload, b"\x1b[31mred\x1b[0m"),
            _ => panic!("expected DataOut"),
        }
    }

    #[test]
    fn round_trip_server_exit() {
        let msg = ServerMsg::Exit(Exit { code: 42 });
        let encoded = msg.encode_flatbuffer();
        let decoded = ServerMsg::decode_flatbuffer(&encoded).unwrap();
        match decoded {
            ServerMsg::Exit(e) => assert_eq!(e.code, 42),
            _ => panic!("expected Exit"),
        }
    }

    #[test]
    fn round_trip_server_error() {
        let msg = ServerMsg::Error(ServerError { message: "PTY died".into() });
        let encoded = msg.encode_flatbuffer();
        let decoded = ServerMsg::decode_flatbuffer(&encoded).unwrap();
        match decoded {
            ServerMsg::Error(e) => assert_eq!(e.message, "PTY died"),
            _ => panic!("expected Error"),
        }
    }

    #[test]
    fn decode_invalid_data_fails() {
        assert!(ClientMsg::decode_flatbuffer(&[0, 0]).is_err());
        assert!(ServerMsg::decode_flatbuffer(&[0, 0]).is_err());
    }
}
