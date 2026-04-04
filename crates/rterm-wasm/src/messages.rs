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

/// Encode a PasteInput ClientMessage.
pub fn encode_paste_input(text: &str) -> Vec<u8> {
    let mut fbb = FlatBufferBuilder::new();
    let payload = fbb.create_string(text);
    let pi = fbs::PasteInput::create(&mut fbb, &fbs::PasteInputArgs { text: Some(payload) });
    let msg = fbs::ClientMessage::create(&mut fbb, &fbs::ClientMessageArgs {
        body_type: fbs::ClientBody::PasteInput,
        body: Some(pi.as_union_value()),
    });
    fbb.finish(msg, None);
    fbb.finished_data().to_vec()
}



/// Encode a MouseEvent ClientMessage.
pub fn encode_mouse_event(row: u16, col: u16, button: u8, modifiers: u8, kind: u8) -> Vec<u8> {
    let mut fbb = FlatBufferBuilder::new();
    let me = fbs::MouseEvent::create(&mut fbb, &fbs::MouseEventArgs {
        row, col, button, modifiers, kind: fbs::MouseEventKind(kind)
    });
    let msg = fbs::ClientMessage::create(&mut fbb, &fbs::ClientMessageArgs {
        body_type: fbs::ClientBody::MouseEvent,
        body: Some(me.as_union_value()),
    });
    fbb.finish(msg, None);
    fbb.finished_data().to_vec()
}

/// Encode a ScrollbackRequest ClientMessage.
pub fn encode_scrollback_request(offset: u32, limit: u32) -> Vec<u8> {
    let mut fbb = FlatBufferBuilder::new();
    let sr = fbs::ScrollbackRequest::create(&mut fbb, &fbs::ScrollbackRequestArgs {
        offset,
        limit,
    });
    let msg = fbs::ClientMessage::create(&mut fbb, &fbs::ClientMessageArgs {
        body_type: fbs::ClientBody::ScrollbackRequest,
        body: Some(sr.as_union_value()),
    });
    fbb.finish(msg, None);
    fbb.finished_data().to_vec()
}

/// Encode a Scroll ClientMessage (out-of-band scroll request).
pub fn encode_scroll(direction: i8, lines: u32) -> Vec<u8> {
    let mut fbb = FlatBufferBuilder::new();
    let scroll = fbs::Scroll::create(&mut fbb, &fbs::ScrollArgs {
        direction,
        lines,
    });
    let msg = fbs::ClientMessage::create(&mut fbb, &fbs::ClientMessageArgs {
        body_type: fbs::ClientBody::Scroll,
        body: Some(scroll.as_union_value()),
    });
    fbb.finish(msg, None);
    fbb.finished_data().to_vec()
}

/// Encode a ResetViewport ClientMessage (return to live viewport).
pub fn encode_reset_viewport() -> Vec<u8> {
    let mut fbb = FlatBufferBuilder::new();
    let rv = fbs::ResetViewport::create(&mut fbb, &fbs::ResetViewportArgs {});
    let msg = fbs::ClientMessage::create(&mut fbb, &fbs::ClientMessageArgs {
        body_type: fbs::ClientBody::ResetViewport,
        body: Some(rv.as_union_value()),
    });
    fbb.finish(msg, None);
    fbb.finished_data().to_vec()
}

/// Decoded server message for the WASM renderer.
pub enum ServerMsg {
    ScreenSnapshot(ScreenData),
    ScreenUpdate(ScreenData),
    Exit(i32),
    Error(String),
    Bell,
    Scrollback(ScrollbackData),
}

/// Scrollback data returned by the relay.
pub struct ScrollbackData {
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
    pub mouse_tracking_mode: u8,
    pub alt_screen_active: bool,
    pub application_cursor_keys: bool,
    /// Viewport offset: non-zero when this snapshot represents a scrolled viewport.
    pub viewport_offset: u32,
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
    pub flags: u16,
}

// Attribute bitflags (must match rterm-core::cell::Flags bit layout).
pub const ATTR_INVERSE: u16 = 0x0001;
pub const ATTR_BOLD: u16 = 0x0002;
pub const ATTR_ITALIC: u16 = 0x0004;
pub const ATTR_UNDERLINE: u16 = 0x0008;
pub const ATTR_WIDE: u16 = 0x0020;
pub const ATTR_WIDE_SPACER: u16 = 0x0040;
pub const ATTR_DIM: u16 = 0x0080;
pub const ATTR_HIDDEN: u16 = 0x0100;
pub const ATTR_STRIKEOUT: u16 = 0x0200;
pub const ATTR_DOUBLE_UNDERLINE: u16 = 0x0800;
pub const ATTR_UNDERCURL: u16 = 0x1000;
pub const ATTR_DOTTED_UNDERLINE: u16 = 0x2000;
pub const ATTR_DASHED_UNDERLINE: u16 = 0x4000;
pub const ATTR_ALL_UNDERLINES: u16 =
    ATTR_UNDERLINE | ATTR_DOUBLE_UNDERLINE | ATTR_UNDERCURL | ATTR_DOTTED_UNDERLINE | ATTR_DASHED_UNDERLINE;

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
                mouse_tracking_mode: ss.mouse_tracking_mode(),
                alt_screen_active: ss.alt_screen_active(),
                application_cursor_keys: ss.application_cursor_keys(),
                viewport_offset: ss.viewport_offset(),
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
                mouse_tracking_mode: su.mouse_tracking_mode(),
                alt_screen_active: su.alt_screen_active(),
                application_cursor_keys: su.application_cursor_keys(),
                viewport_offset: 0,
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
                flags: c.flags(),
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
