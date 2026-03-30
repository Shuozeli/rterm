/// FlatBuffers message encoding/decoding for the WASM client (v2 typed protocol).
use crate::generated::rterm::protocol as fbs;
use flatbuffers::FlatBufferBuilder;

/// Encode a Resize ClientMessage.
pub fn encode_resize(cols: u16, rows: u16) -> Vec<u8> {
    let mut fbb = FlatBufferBuilder::new();
    let resize = fbs::Resize::create(&mut fbb, &fbs::ResizeArgs { cols, rows });
    let msg = fbs::ClientMessage::create(&mut fbb, &fbs::ClientMessageArgs {
        body_type: fbs::ClientBody::Resize,
        body: Some(resize.as_union_value()),
    });
    fbb.finish(msg, None);
    fbb.finished_data().to_vec()
}

/// Encode a KeyInput ClientMessage.
pub fn encode_key_input(data: &[u8]) -> Vec<u8> {
    let mut fbb = FlatBufferBuilder::new();
    let payload = fbb.create_vector(data);
    let ki = fbs::KeyInput::create(&mut fbb, &fbs::KeyInputArgs { data: Some(payload) });
    let msg = fbs::ClientMessage::create(&mut fbb, &fbs::ClientMessageArgs {
        body_type: fbs::ClientBody::KeyInput,
        body: Some(ki.as_union_value()),
    });
    fbb.finish(msg, None);
    fbb.finished_data().to_vec()
}

/// Encode a ScrollbackRequest ClientMessage.
pub fn encode_scrollback_request(offset: u32, count: u32) -> Vec<u8> {
    let mut fbb = FlatBufferBuilder::new();
    let sr = fbs::ScrollbackRequest::create(&mut fbb, &fbs::ScrollbackRequestArgs { offset, count });
    let msg = fbs::ClientMessage::create(&mut fbb, &fbs::ClientMessageArgs {
        body_type: fbs::ClientBody::ScrollbackRequest,
        body: Some(sr.as_union_value()),
    });
    fbb.finish(msg, None);
    fbb.finished_data().to_vec()
}

/// Decoded server message for the WASM renderer.
pub enum ServerMsg {
    ScreenSnapshot(ScreenData),
    ScreenUpdate(ScreenData),
    ScrollbackData(ScrollbackDataMsg),
    Exit(i32),
    Error(String),
    Bell,
}

pub struct ScrollbackDataMsg {
    pub lines: Vec<CellRange>,
    pub offset: u32,
    pub total: u32,
}

pub struct ScreenData {
    pub changes: Vec<CellRange>,
    pub cursor_row: u16,
    pub cursor_col: u16,
    pub cursor_visible: bool,
    pub cursor_style: u8,
    pub cols: u16,
    pub rows: u16,
    pub scrollback_len: u32,
}

pub struct CellRange {
    pub row: u16,
    pub col_start: u16,
    pub cells: Vec<CellData>,
}

#[derive(Clone, Copy)]
pub struct CellData {
    pub ch: char,
    pub fg: u32,
    pub bg: u32,
    pub attrs: u8,
}

// Attribute bitflags (must match rterm-proto).
pub const ATTR_BOLD: u8 = 1 << 0;
pub const ATTR_ITALIC: u8 = 1 << 1;
pub const ATTR_UNDERLINE: u8 = 1 << 2;
pub const ATTR_STRIKETHROUGH: u8 = 1 << 3;
pub const ATTR_REVERSE: u8 = 1 << 4;
pub const ATTR_DIM: u8 = 1 << 5;
pub const ATTR_HIDDEN: u8 = 1 << 6;
pub const ATTR_WIDE: u8 = 1 << 7;

pub const COLOR_DEFAULT: u32 = 0xFFFFFFFF;

/// Decode a ServerMessage from FlatBuffers bytes.
pub fn decode_server_msg(data: &[u8]) -> Result<ServerMsg, String> {
    let msg = flatbuffers::root::<fbs::ServerMessage>(data)
        .map_err(|e| format!("invalid ServerMessage: {e}"))?;

    match msg.body_type() {
        fbs::ServerBody::ScreenSnapshot => {
            let ss = msg.body_as_screen_snapshot().ok_or("missing ScreenSnapshot")?;
            let cursor = ss.cursor().ok_or("missing cursor")?;
            Ok(ServerMsg::ScreenSnapshot(ScreenData {
                changes: decode_cell_ranges(ss.rows())?,
                cursor_row: cursor.row(),
                cursor_col: cursor.col(),
                cursor_visible: cursor.visible(), cursor_style: cursor.style(),
                cols: ss.cols(),
                rows: ss.num_rows(),
                scrollback_len: ss.scrollback_len(),
            }))
        }
        fbs::ServerBody::ScreenUpdate => {
            let su = msg.body_as_screen_update().ok_or("missing ScreenUpdate")?;
            let cursor = su.cursor().ok_or("missing cursor")?;
            Ok(ServerMsg::ScreenUpdate(ScreenData {
                changes: decode_cell_ranges(su.changes())?,
                cursor_row: cursor.row(),
                cursor_col: cursor.col(),
                cursor_visible: cursor.visible(), cursor_style: cursor.style(),
                cols: su.cols(),
                rows: su.rows(),
                scrollback_len: su.scrollback_len(),
            }))
        }
        fbs::ServerBody::ScrollbackData => {
            let sd = msg.body_as_scrollback_data().ok_or("missing ScrollbackData")?;
            Ok(ServerMsg::ScrollbackData(ScrollbackDataMsg {
                lines: decode_cell_ranges(sd.lines())?,
                offset: sd.offset(),
                total: sd.total(),
            }))
        }
        fbs::ServerBody::Exit => {
            let e = msg.body_as_exit().ok_or("missing Exit")?;
            Ok(ServerMsg::Exit(e.code()))
        }
        fbs::ServerBody::Error => {
            let e = msg.body_as_error().ok_or("missing Error")?;
            Ok(ServerMsg::Error(e.message().unwrap_or("").to_string()))
        }
        fbs::ServerBody::Bell => Ok(ServerMsg::Bell),
        _ => Err("unknown ServerBody".into()),
    }
}

fn decode_cell_ranges(
    ranges: Option<flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fbs::CellRange<'_>>>>,
) -> Result<Vec<CellRange>, String> {
    let ranges = ranges.ok_or("missing ranges")?;
    Ok(ranges.iter().map(|cr| {
        let cells = cr.cells().map(|cells| {
            cells.iter().map(|c| CellData {
                ch: char::from_u32(c.ch()).unwrap_or(' '),
                fg: c.fg(),
                bg: c.bg(),
                attrs: c.attrs(),
            }).collect()
        }).unwrap_or_default();
        CellRange {
            row: cr.row(),
            col_start: cr.col_start(),
            cells,
        }
    }).collect())
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
    fn encode_key_input_roundtrip() {
        let data = encode_key_input(b"hello");
        let msg = flatbuffers::root::<fbs::ClientMessage>(&data).unwrap();
        assert_eq!(msg.body_type(), fbs::ClientBody::KeyInput);
    }
}
