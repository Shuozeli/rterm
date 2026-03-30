#[allow(unused_imports, dead_code, clippy::all, non_snake_case)]
mod generated;

/// Re-export the raw FlatBuffers types.
pub use generated::rterm::protocol as fbs;

/// Re-export flatbuffers for consumers.
pub use flatbuffers;

use grpc_codec_flatbuffers::FlatBufferGrpcMessage;

// ============================================================================
// Client → Server messages
// ============================================================================

#[derive(Debug, Clone)]
pub struct KeyInput {
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct PasteInput {
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct Resize {
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone)]
pub struct MouseEvent {
    pub row: u16,
    pub col: u16,
    pub button: u8,
    pub modifiers: u8,
    pub kind: u8, // MouseEventKind
}

#[derive(Debug, Clone)]
pub struct ScrollbackRequest {
    pub offset: u32,
    pub count: u32,
}

#[derive(Debug, Clone)]
pub enum ClientMsg {
    KeyInput(KeyInput),
    PasteInput(PasteInput),
    Resize(Resize),
    MouseEvent(MouseEvent),
    ScrollbackRequest(ScrollbackRequest),
}

// ============================================================================
// Server → Client messages
// ============================================================================

/// Packed color: 0x00RRGGBB for RGB, 0xFF0000II for indexed, 0xFFFFFFFF for default.
pub const COLOR_DEFAULT: u32 = 0xFFFFFFFF;

pub fn pack_color_indexed(idx: u8) -> u32 {
    0xFF000000 | (idx as u32)
}

pub fn pack_color_rgb(r: u8, g: u8, b: u8) -> u32 {
    ((r as u32) << 16) | ((g as u32) << 8) | (b as u32)
}

pub fn unpack_color(packed: u32) -> ColorKind {
    if packed == COLOR_DEFAULT {
        ColorKind::Default
    } else if packed & 0xFF000000 == 0xFF000000 {
        ColorKind::Indexed((packed & 0xFF) as u8)
    } else {
        ColorKind::Rgb(
            ((packed >> 16) & 0xFF) as u8,
            ((packed >> 8) & 0xFF) as u8,
            (packed & 0xFF) as u8,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorKind {
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

/// Attribute bitflags.
pub const ATTR_BOLD: u8 = 1 << 0;
pub const ATTR_ITALIC: u8 = 1 << 1;
pub const ATTR_UNDERLINE: u8 = 1 << 2;
pub const ATTR_STRIKETHROUGH: u8 = 1 << 3;
pub const ATTR_REVERSE: u8 = 1 << 4;
pub const ATTR_DIM: u8 = 1 << 5;
pub const ATTR_HIDDEN: u8 = 1 << 6;

#[derive(Debug, Clone)]
pub struct CellData {
    pub ch: char,
    pub fg: u32,
    pub bg: u32,
    pub attrs: u8,
}

#[derive(Debug, Clone)]
pub struct CellRangeData {
    pub row: u16,
    pub col_start: u16,
    pub cells: Vec<CellData>,
}

#[derive(Debug, Clone)]
pub struct CursorData {
    pub row: u16,
    pub col: u16,
    pub visible: bool,
}

#[derive(Debug, Clone)]
pub struct ScreenUpdateData {
    pub changes: Vec<CellRangeData>,
    pub cursor: CursorData,
    pub cols: u16,
    pub rows: u16,
    pub title: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ScreenSnapshotData {
    pub rows: Vec<CellRangeData>,
    pub cursor: CursorData,
    pub cols: u16,
    pub num_rows: u16,
    pub title: Option<String>,
    pub scrollback_len: u32,
}

#[derive(Debug, Clone)]
pub struct ScrollbackDataMsg {
    pub lines: Vec<CellRangeData>,
    pub offset: u32,
    pub total: u32,
}

#[derive(Debug, Clone)]
pub struct Exit {
    pub code: i32,
}

#[derive(Debug, Clone)]
pub struct ServerError {
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum ServerMsg {
    ScreenUpdate(ScreenUpdateData),
    ScreenSnapshot(ScreenSnapshotData),
    ScrollbackData(ScrollbackDataMsg),
    Exit(Exit),
    Error(ServerError),
    Bell,
}

// ============================================================================
// FlatBufferGrpcMessage implementations
// ============================================================================

impl FlatBufferGrpcMessage for ClientMsg {
    fn encode_flatbuffer(&self) -> Vec<u8> {
        let mut fbb = flatbuffers::FlatBufferBuilder::new();
        match self {
            ClientMsg::KeyInput(k) => {
                let data = fbb.create_vector(&k.data);
                let ki = fbs::KeyInput::create(&mut fbb, &fbs::KeyInputArgs { data: Some(data) });
                let msg = fbs::ClientMessage::create(
                    &mut fbb,
                    &fbs::ClientMessageArgs {
                        body_type: fbs::ClientBody::KeyInput,
                        body: Some(ki.as_union_value()),
                    },
                );
                fbb.finish(msg, None);
            }
            ClientMsg::PasteInput(p) => {
                let text = fbb.create_string(&p.text);
                let pi =
                    fbs::PasteInput::create(&mut fbb, &fbs::PasteInputArgs { text: Some(text) });
                let msg = fbs::ClientMessage::create(
                    &mut fbb,
                    &fbs::ClientMessageArgs {
                        body_type: fbs::ClientBody::PasteInput,
                        body: Some(pi.as_union_value()),
                    },
                );
                fbb.finish(msg, None);
            }
            ClientMsg::Resize(r) => {
                let resize = fbs::Resize::create(
                    &mut fbb,
                    &fbs::ResizeArgs {
                        cols: r.cols,
                        rows: r.rows,
                    },
                );
                let msg = fbs::ClientMessage::create(
                    &mut fbb,
                    &fbs::ClientMessageArgs {
                        body_type: fbs::ClientBody::Resize,
                        body: Some(resize.as_union_value()),
                    },
                );
                fbb.finish(msg, None);
            }
            ClientMsg::MouseEvent(m) => {
                let me = fbs::MouseEvent::create(
                    &mut fbb,
                    &fbs::MouseEventArgs {
                        row: m.row,
                        col: m.col,
                        button: m.button,
                        modifiers: m.modifiers,
                        kind: fbs::MouseEventKind(m.kind),
                    },
                );
                let msg = fbs::ClientMessage::create(
                    &mut fbb,
                    &fbs::ClientMessageArgs {
                        body_type: fbs::ClientBody::MouseEvent,
                        body: Some(me.as_union_value()),
                    },
                );
                fbb.finish(msg, None);
            }
            ClientMsg::ScrollbackRequest(s) => {
                let sr = fbs::ScrollbackRequest::create(
                    &mut fbb,
                    &fbs::ScrollbackRequestArgs {
                        offset: s.offset,
                        count: s.count,
                    },
                );
                let msg = fbs::ClientMessage::create(
                    &mut fbb,
                    &fbs::ClientMessageArgs {
                        body_type: fbs::ClientBody::ScrollbackRequest,
                        body: Some(sr.as_union_value()),
                    },
                );
                fbb.finish(msg, None);
            }
        }
        fbb.finished_data().to_vec()
    }

    fn decode_flatbuffer(data: &[u8]) -> Result<Self, String> {
        let msg = flatbuffers::root::<fbs::ClientMessage>(data)
            .map_err(|e| format!("invalid ClientMessage: {e}"))?;
        match msg.body_type() {
            fbs::ClientBody::KeyInput => {
                let k = msg.body_as_key_input().ok_or("missing KeyInput")?;
                Ok(ClientMsg::KeyInput(KeyInput {
                    data: k.data().map(|d| d.bytes().to_vec()).unwrap_or_default(),
                }))
            }
            fbs::ClientBody::PasteInput => {
                let p = msg.body_as_paste_input().ok_or("missing PasteInput")?;
                Ok(ClientMsg::PasteInput(PasteInput {
                    text: p.text().unwrap_or("").to_string(),
                }))
            }
            fbs::ClientBody::Resize => {
                let r = msg.body_as_resize().ok_or("missing Resize")?;
                Ok(ClientMsg::Resize(Resize {
                    cols: r.cols(),
                    rows: r.rows(),
                }))
            }
            fbs::ClientBody::ScrollbackRequest => {
                let s = msg
                    .body_as_scrollback_request()
                    .ok_or("missing ScrollbackRequest")?;
                Ok(ClientMsg::ScrollbackRequest(ScrollbackRequest {
                    offset: s.offset(),
                    count: s.count(),
                }))
            }
            fbs::ClientBody::MouseEvent => {
                let m = msg.body_as_mouse_event().ok_or("missing MouseEvent")?;
                Ok(ClientMsg::MouseEvent(MouseEvent {
                    row: m.row(),
                    col: m.col(),
                    button: m.button(),
                    modifiers: m.modifiers(),
                    kind: m.kind().0,
                }))
            }
            _ => Err("unknown ClientBody".into()),
        }
    }
}

impl FlatBufferGrpcMessage for ServerMsg {
    fn encode_flatbuffer(&self) -> Vec<u8> {
        let mut fbb = flatbuffers::FlatBufferBuilder::new();
        match self {
            ServerMsg::ScreenUpdate(su) => {
                encode_screen_update(&mut fbb, su);
            }
            ServerMsg::ScreenSnapshot(ss) => {
                encode_screen_snapshot(&mut fbb, ss);
            }
            ServerMsg::ScrollbackData(sd) => {
                let lines = encode_cell_ranges(&mut fbb, &sd.lines);
                let lines_vec = fbb.create_vector(&lines);
                let sbd = fbs::ScrollbackData::create(
                    &mut fbb,
                    &fbs::ScrollbackDataArgs {
                        lines: Some(lines_vec),
                        offset: sd.offset,
                        total: sd.total,
                    },
                );
                let msg = fbs::ServerMessage::create(
                    &mut fbb,
                    &fbs::ServerMessageArgs {
                        body_type: fbs::ServerBody::ScrollbackData,
                        body: Some(sbd.as_union_value()),
                    },
                );
                fbb.finish(msg, None);
            }
            ServerMsg::Exit(e) => {
                let exit = fbs::Exit::create(&mut fbb, &fbs::ExitArgs { code: e.code });
                let msg = fbs::ServerMessage::create(
                    &mut fbb,
                    &fbs::ServerMessageArgs {
                        body_type: fbs::ServerBody::Exit,
                        body: Some(exit.as_union_value()),
                    },
                );
                fbb.finish(msg, None);
            }
            ServerMsg::Error(e) => {
                let message = fbb.create_string(&e.message);
                let error = fbs::Error::create(
                    &mut fbb,
                    &fbs::ErrorArgs {
                        message: Some(message),
                    },
                );
                let msg = fbs::ServerMessage::create(
                    &mut fbb,
                    &fbs::ServerMessageArgs {
                        body_type: fbs::ServerBody::Error,
                        body: Some(error.as_union_value()),
                    },
                );
                fbb.finish(msg, None);
            }
            ServerMsg::Bell => {
                let bell = fbs::Bell::create(&mut fbb, &fbs::BellArgs {});
                let msg = fbs::ServerMessage::create(
                    &mut fbb,
                    &fbs::ServerMessageArgs {
                        body_type: fbs::ServerBody::Bell,
                        body: Some(bell.as_union_value()),
                    },
                );
                fbb.finish(msg, None);
            }
        }
        fbb.finished_data().to_vec()
    }

    fn decode_flatbuffer(data: &[u8]) -> Result<Self, String> {
        let msg = flatbuffers::root::<fbs::ServerMessage>(data)
            .map_err(|e| format!("invalid ServerMessage: {e}"))?;
        match msg.body_type() {
            fbs::ServerBody::ScreenUpdate => {
                let su = msg.body_as_screen_update().ok_or("missing ScreenUpdate")?;
                Ok(ServerMsg::ScreenUpdate(decode_screen_update(&su)?))
            }
            fbs::ServerBody::ScreenSnapshot => {
                let ss = msg
                    .body_as_screen_snapshot()
                    .ok_or("missing ScreenSnapshot")?;
                Ok(ServerMsg::ScreenSnapshot(decode_screen_snapshot(&ss)?))
            }
            fbs::ServerBody::ScrollbackData => {
                let sd = msg
                    .body_as_scrollback_data()
                    .ok_or("missing ScrollbackData")?;
                Ok(ServerMsg::ScrollbackData(ScrollbackDataMsg {
                    lines: decode_cell_ranges(sd.lines())?,
                    offset: sd.offset(),
                    total: sd.total(),
                }))
            }
            fbs::ServerBody::Exit => {
                let e = msg.body_as_exit().ok_or("missing Exit")?;
                Ok(ServerMsg::Exit(Exit { code: e.code() }))
            }
            fbs::ServerBody::Error => {
                let e = msg.body_as_error().ok_or("missing Error")?;
                Ok(ServerMsg::Error(ServerError {
                    message: e.message().unwrap_or("").to_string(),
                }))
            }
            fbs::ServerBody::Bell => Ok(ServerMsg::Bell),
            _ => Err("unknown ServerBody".into()),
        }
    }
}

// ============================================================================
// Encoding helpers
// ============================================================================

fn encode_cell_ranges<'a>(
    fbb: &mut flatbuffers::FlatBufferBuilder<'a>,
    ranges: &[CellRangeData],
) -> Vec<flatbuffers::WIPOffset<fbs::CellRange<'a>>> {
    ranges
        .iter()
        .map(|cr| {
            let cells: Vec<fbs::Cell> = cr
                .cells
                .iter()
                .map(|c| fbs::Cell::new(c.ch as u32, c.fg, c.bg, c.attrs))
                .collect();
            let cells_vec = fbb.create_vector(&cells);
            fbs::CellRange::create(
                fbb,
                &fbs::CellRangeArgs {
                    row: cr.row,
                    col_start: cr.col_start,
                    cells: Some(cells_vec),
                },
            )
        })
        .collect()
}

fn encode_screen_update(fbb: &mut flatbuffers::FlatBufferBuilder<'_>, su: &ScreenUpdateData) {
    let changes = encode_cell_ranges(fbb, &su.changes);
    let changes_vec = fbb.create_vector(&changes);
    let cursor = fbs::CursorState::create(
        fbb,
        &fbs::CursorStateArgs {
            row: su.cursor.row,
            col: su.cursor.col,
            visible: su.cursor.visible,
        },
    );
    let title = su.title.as_ref().map(|t| fbb.create_string(t));
    let screen = fbs::ScreenUpdate::create(
        fbb,
        &fbs::ScreenUpdateArgs {
            changes: Some(changes_vec),
            cursor: Some(cursor),
            cols: su.cols,
            rows: su.rows,
            title,
        },
    );
    let msg = fbs::ServerMessage::create(
        fbb,
        &fbs::ServerMessageArgs {
            body_type: fbs::ServerBody::ScreenUpdate,
            body: Some(screen.as_union_value()),
        },
    );
    fbb.finish(msg, None);
}

fn encode_screen_snapshot(fbb: &mut flatbuffers::FlatBufferBuilder<'_>, ss: &ScreenSnapshotData) {
    let rows = encode_cell_ranges(fbb, &ss.rows);
    let rows_vec = fbb.create_vector(&rows);
    let cursor = fbs::CursorState::create(
        fbb,
        &fbs::CursorStateArgs {
            row: ss.cursor.row,
            col: ss.cursor.col,
            visible: ss.cursor.visible,
        },
    );
    let title = ss.title.as_ref().map(|t| fbb.create_string(t));
    let snapshot = fbs::ScreenSnapshot::create(
        fbb,
        &fbs::ScreenSnapshotArgs {
            rows: Some(rows_vec),
            cursor: Some(cursor),
            cols: ss.cols,
            num_rows: ss.num_rows,
            title,
            scrollback_len: ss.scrollback_len,
        },
    );
    let msg = fbs::ServerMessage::create(
        fbb,
        &fbs::ServerMessageArgs {
            body_type: fbs::ServerBody::ScreenSnapshot,
            body: Some(snapshot.as_union_value()),
        },
    );
    fbb.finish(msg, None);
}

// ============================================================================
// Decoding helpers
// ============================================================================

fn decode_cell_ranges(
    ranges: Option<flatbuffers::Vector<'_, flatbuffers::ForwardsUOffset<fbs::CellRange<'_>>>>,
) -> Result<Vec<CellRangeData>, String> {
    let ranges = ranges.ok_or("missing cell ranges")?;
    let mut result = Vec::new();
    for cr in ranges.iter() {
        let cells = cr.cells().ok_or("missing cells")?;
        let cell_data: Vec<CellData> = cells
            .iter()
            .map(|c| CellData {
                ch: char::from_u32(c.ch()).unwrap_or(' '),
                fg: c.fg(),
                bg: c.bg(),
                attrs: c.attrs(),
            })
            .collect();
        result.push(CellRangeData {
            row: cr.row(),
            col_start: cr.col_start(),
            cells: cell_data,
        });
    }
    Ok(result)
}

fn decode_screen_update(su: &fbs::ScreenUpdate<'_>) -> Result<ScreenUpdateData, String> {
    let cursor_fb = su.cursor().ok_or("missing cursor")?;
    Ok(ScreenUpdateData {
        changes: decode_cell_ranges(su.changes())?,
        cursor: CursorData {
            row: cursor_fb.row(),
            col: cursor_fb.col(),
            visible: cursor_fb.visible(),
        },
        cols: su.cols(),
        rows: su.rows(),
        title: su.title().map(|t| t.to_string()),
    })
}

fn decode_screen_snapshot(ss: &fbs::ScreenSnapshot<'_>) -> Result<ScreenSnapshotData, String> {
    let cursor_fb = ss.cursor().ok_or("missing cursor")?;
    Ok(ScreenSnapshotData {
        rows: decode_cell_ranges(ss.rows())?,
        cursor: CursorData {
            row: cursor_fb.row(),
            col: cursor_fb.col(),
            visible: cursor_fb.visible(),
        },
        cols: ss.cols(),
        num_rows: ss.num_rows(),
        title: ss.title().map(|t| t.to_string()),
        scrollback_len: ss.scrollback_len(),
    })
}

/// gRPC service path for the Terminal service.
pub const TERMINAL_SERVICE_PATH: &str = "/rterm.protocol.TerminalService/Session";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_key_input() {
        let msg = ClientMsg::KeyInput(KeyInput {
            data: b"hello".to_vec(),
        });
        let decoded = ClientMsg::decode_flatbuffer(&msg.encode_flatbuffer()).unwrap();
        match decoded {
            ClientMsg::KeyInput(k) => assert_eq!(k.data, b"hello"),
            _ => panic!("expected KeyInput"),
        }
    }

    #[test]
    fn round_trip_paste() {
        let msg = ClientMsg::PasteInput(PasteInput {
            text: "pasted text".into(),
        });
        let decoded = ClientMsg::decode_flatbuffer(&msg.encode_flatbuffer()).unwrap();
        match decoded {
            ClientMsg::PasteInput(p) => assert_eq!(p.text, "pasted text"),
            _ => panic!("expected PasteInput"),
        }
    }

    #[test]
    fn round_trip_resize() {
        let msg = ClientMsg::Resize(Resize {
            cols: 120,
            rows: 40,
        });
        let decoded = ClientMsg::decode_flatbuffer(&msg.encode_flatbuffer()).unwrap();
        match decoded {
            ClientMsg::Resize(r) => {
                assert_eq!(r.cols, 120);
                assert_eq!(r.rows, 40);
            }
            _ => panic!("expected Resize"),
        }
    }

    #[test]
    fn round_trip_screen_update() {
        let msg = ServerMsg::ScreenUpdate(ScreenUpdateData {
            changes: vec![CellRangeData {
                row: 0,
                col_start: 0,
                cells: vec![
                    CellData {
                        ch: 'H',
                        fg: COLOR_DEFAULT,
                        bg: COLOR_DEFAULT,
                        attrs: ATTR_BOLD,
                    },
                    CellData {
                        ch: 'i',
                        fg: pack_color_rgb(255, 0, 0),
                        bg: COLOR_DEFAULT,
                        attrs: 0,
                    },
                ],
            }],
            cursor: CursorData {
                row: 0,
                col: 2,
                visible: true,
            },
            cols: 80,
            rows: 24,
            title: Some("test".into()),
        });
        let decoded = ServerMsg::decode_flatbuffer(&msg.encode_flatbuffer()).unwrap();
        match decoded {
            ServerMsg::ScreenUpdate(su) => {
                assert_eq!(su.cols, 80);
                assert_eq!(su.changes.len(), 1);
                assert_eq!(su.changes[0].cells[0].ch, 'H');
                assert_eq!(su.changes[0].cells[0].attrs, ATTR_BOLD);
                assert_eq!(su.changes[0].cells[1].fg, pack_color_rgb(255, 0, 0));
                assert_eq!(su.cursor.row, 0);
                assert_eq!(su.cursor.col, 2);
                assert_eq!(su.title.as_deref(), Some("test"));
            }
            _ => panic!("expected ScreenUpdate"),
        }
    }

    #[test]
    fn round_trip_screen_snapshot() {
        let msg = ServerMsg::ScreenSnapshot(ScreenSnapshotData {
            rows: vec![CellRangeData {
                row: 0,
                col_start: 0,
                cells: vec![CellData {
                    ch: 'A',
                    fg: COLOR_DEFAULT,
                    bg: COLOR_DEFAULT,
                    attrs: 0,
                }],
            }],
            cursor: CursorData {
                row: 0,
                col: 1,
                visible: true,
            },
            cols: 80,
            num_rows: 24,
            title: None,
            scrollback_len: 100,
        });
        let decoded = ServerMsg::decode_flatbuffer(&msg.encode_flatbuffer()).unwrap();
        match decoded {
            ServerMsg::ScreenSnapshot(ss) => {
                assert_eq!(ss.scrollback_len, 100);
                assert_eq!(ss.rows[0].cells[0].ch, 'A');
            }
            _ => panic!("expected ScreenSnapshot"),
        }
    }

    #[test]
    fn round_trip_exit() {
        let msg = ServerMsg::Exit(Exit { code: 42 });
        let decoded = ServerMsg::decode_flatbuffer(&msg.encode_flatbuffer()).unwrap();
        match decoded {
            ServerMsg::Exit(e) => assert_eq!(e.code, 42),
            _ => panic!("expected Exit"),
        }
    }

    #[test]
    fn round_trip_bell() {
        let msg = ServerMsg::Bell;
        let decoded = ServerMsg::decode_flatbuffer(&msg.encode_flatbuffer()).unwrap();
        assert!(matches!(decoded, ServerMsg::Bell));
    }

    #[test]
    fn color_packing() {
        assert_eq!(unpack_color(COLOR_DEFAULT), ColorKind::Default);
        assert_eq!(
            unpack_color(pack_color_indexed(200)),
            ColorKind::Indexed(200)
        );
        assert_eq!(
            unpack_color(pack_color_rgb(100, 150, 200)),
            ColorKind::Rgb(100, 150, 200)
        );
    }

    #[test]
    fn color_indexed_boundary() {
        assert_eq!(unpack_color(pack_color_indexed(0)), ColorKind::Indexed(0));
        assert_eq!(
            unpack_color(pack_color_indexed(255)),
            ColorKind::Indexed(255)
        );
    }

    #[test]
    fn color_rgb_boundary() {
        assert_eq!(
            unpack_color(pack_color_rgb(0, 0, 0)),
            ColorKind::Rgb(0, 0, 0)
        );
        assert_eq!(
            unpack_color(pack_color_rgb(255, 255, 255)),
            ColorKind::Rgb(255, 255, 255)
        );
    }

    #[test]
    fn round_trip_mouse_event() {
        let msg = ClientMsg::MouseEvent(MouseEvent {
            row: 10,
            col: 20,
            button: 0,
            modifiers: 5,
            kind: 2,
        });
        let decoded = ClientMsg::decode_flatbuffer(&msg.encode_flatbuffer()).unwrap();
        match decoded {
            ClientMsg::MouseEvent(m) => {
                assert_eq!(m.row, 10);
                assert_eq!(m.col, 20);
                assert_eq!(m.button, 0);
                assert_eq!(m.modifiers, 5);
                assert_eq!(m.kind, 2);
            }
            _ => panic!("expected MouseEvent"),
        }
    }

    #[test]
    fn round_trip_scrollback_data() {
        let msg = ServerMsg::ScrollbackData(ScrollbackDataMsg {
            lines: vec![CellRangeData {
                row: 5,
                col_start: 0,
                cells: vec![
                    CellData {
                        ch: 'A',
                        fg: COLOR_DEFAULT,
                        bg: COLOR_DEFAULT,
                        attrs: 0,
                    },
                    CellData {
                        ch: 'B',
                        fg: pack_color_rgb(255, 0, 0),
                        bg: COLOR_DEFAULT,
                        attrs: ATTR_BOLD,
                    },
                ],
            }],
            offset: 10,
            total: 100,
        });
        let decoded = ServerMsg::decode_flatbuffer(&msg.encode_flatbuffer()).unwrap();
        match decoded {
            ServerMsg::ScrollbackData(sd) => {
                assert_eq!(sd.offset, 10);
                assert_eq!(sd.total, 100);
                assert_eq!(sd.lines.len(), 1);
                assert_eq!(sd.lines[0].row, 5);
                assert_eq!(sd.lines[0].cells[0].ch, 'A');
                assert_eq!(sd.lines[0].cells[1].fg, pack_color_rgb(255, 0, 0));
                assert_eq!(sd.lines[0].cells[1].attrs, ATTR_BOLD);
            }
            _ => panic!("expected ScrollbackData"),
        }
    }

    #[test]
    fn round_trip_error_msg() {
        let msg = ServerMsg::Error(ServerError {
            message: "something broke".into(),
        });
        let decoded = ServerMsg::decode_flatbuffer(&msg.encode_flatbuffer()).unwrap();
        match decoded {
            ServerMsg::Error(e) => assert_eq!(e.message, "something broke"),
            _ => panic!("expected Error"),
        }
    }

    #[test]
    fn round_trip_screen_update_no_title() {
        let msg = ServerMsg::ScreenUpdate(ScreenUpdateData {
            changes: vec![],
            cursor: CursorData {
                row: 0,
                col: 0,
                visible: true,
            },
            cols: 80,
            rows: 24,
            title: None,
        });
        let decoded = ServerMsg::decode_flatbuffer(&msg.encode_flatbuffer()).unwrap();
        match decoded {
            ServerMsg::ScreenUpdate(su) => {
                assert!(su.title.is_none());
                assert_eq!(su.cols, 80);
            }
            _ => panic!("expected ScreenUpdate"),
        }
    }

    #[test]
    fn round_trip_snapshot_with_title() {
        let msg = ServerMsg::ScreenSnapshot(ScreenSnapshotData {
            rows: vec![],
            cursor: CursorData {
                row: 5,
                col: 10,
                visible: false,
            },
            cols: 120,
            num_rows: 40,
            title: Some("my terminal".into()),
            scrollback_len: 500,
        });
        let decoded = ServerMsg::decode_flatbuffer(&msg.encode_flatbuffer()).unwrap();
        match decoded {
            ServerMsg::ScreenSnapshot(ss) => {
                assert_eq!(ss.title.as_deref(), Some("my terminal"));
                assert_eq!(ss.scrollback_len, 500);
                assert!(!ss.cursor.visible);
            }
            _ => panic!("expected ScreenSnapshot"),
        }
    }

    #[test]
    fn decode_invalid_client_msg() {
        let result = ClientMsg::decode_flatbuffer(&[0xFF, 0x00, 0x01]);
        assert!(result.is_err());
    }

    #[test]
    fn decode_invalid_server_msg() {
        let result = ServerMsg::decode_flatbuffer(&[0xFF, 0x00, 0x01]);
        assert!(result.is_err());
    }

    #[test]
    fn attr_bitflags() {
        assert_eq!(ATTR_BOLD, 1);
        assert_eq!(ATTR_ITALIC, 2);
        assert_eq!(ATTR_UNDERLINE, 4);
        assert_eq!(ATTR_STRIKETHROUGH, 8);
        assert_eq!(ATTR_REVERSE, 16);
        assert_eq!(ATTR_DIM, 32);
        assert_eq!(ATTR_HIDDEN, 64);
        // No overlap.
        let all = ATTR_BOLD
            | ATTR_ITALIC
            | ATTR_UNDERLINE
            | ATTR_STRIKETHROUGH
            | ATTR_REVERSE
            | ATTR_DIM
            | ATTR_HIDDEN;
        assert_eq!(all, 127);
    }
}
