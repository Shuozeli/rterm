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
pub struct CreateSession {
    pub name: Option<String>,
    pub shell: Option<String>,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone)]
pub struct AttachSession {
    pub session_id: String,
    pub token: String,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone)]
pub struct DestroySession {
    pub session_id: String,
}

#[derive(Debug, Clone)]
pub struct ListSessions {
    pub tokens: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum ClientMsg {
    KeyInput(KeyInput),
    PasteInput(PasteInput),
    Resize(Resize),
    MouseEvent(MouseEvent),
    CreateSession(CreateSession),
    AttachSession(AttachSession),
    DetachSession,
    DestroySession(DestroySession),
    ListSessions(ListSessions),
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

/// Attribute bitflags (u16), matching rterm_core::cell::Flags layout.
pub const ATTR_INVERSE: u16 = 1 << 0;
pub const ATTR_BOLD: u16 = 1 << 1;
pub const ATTR_ITALIC: u16 = 1 << 2;
pub const ATTR_UNDERLINE: u16 = 1 << 3;
pub const ATTR_WRAPLINE: u16 = 1 << 4;
pub const ATTR_WIDE_CHAR: u16 = 1 << 5;
pub const ATTR_WIDE_CHAR_SPACER: u16 = 1 << 6;
pub const ATTR_DIM: u16 = 1 << 7;
pub const ATTR_HIDDEN: u16 = 1 << 8;
pub const ATTR_STRIKEOUT: u16 = 1 << 9;
pub const ATTR_LEADING_WIDE_CHAR_SPACER: u16 = 1 << 10;
pub const ATTR_DOUBLE_UNDERLINE: u16 = 1 << 11;
pub const ATTR_UNDERCURL: u16 = 1 << 12;
pub const ATTR_DOTTED_UNDERLINE: u16 = 1 << 13;
pub const ATTR_DASHED_UNDERLINE: u16 = 1 << 14;

#[derive(Debug, Clone)]
pub struct CellData {
    pub ch: char,
    pub fg: u32,
    pub bg: u32,
    pub flags: u16,
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
    pub style: u8,
}

#[derive(Debug, Clone)]
pub struct ScreenUpdateData {
    pub changes: Vec<CellRangeData>,
    pub cursor: CursorData,
    pub cols: u16,
    pub rows: u16,
    pub title: Option<String>,
    pub mouse_tracking_mode: u8,
    pub alt_screen_active: bool,
    pub application_cursor_keys: bool,
}

#[derive(Debug, Clone)]
pub struct ScreenSnapshotData {
    pub rows: Vec<CellRangeData>,
    pub cursor: CursorData,
    pub cols: u16,
    pub num_rows: u16,
    pub title: Option<String>,
    pub mouse_tracking_mode: u8,
    pub alt_screen_active: bool,
    pub application_cursor_keys: bool,
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
pub struct SessionCreated {
    pub session_id: String,
    pub name: String,
    pub token: String,
}

#[derive(Debug, Clone)]
pub struct SessionAttached {
    pub session_id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct SessionDetached {
    pub session_id: String,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct SessionDestroyed {
    pub session_id: String,
}

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub session_id: String,
    pub name: String,
    pub shell: String,
    pub created_at: u64,
    pub last_activity: u64,
    pub attached: bool,
    pub cols: u16,
    pub rows: u16,
    pub title: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SessionListData {
    pub sessions: Vec<SessionInfo>,
}

#[derive(Debug, Clone)]
pub enum ServerMsg {
    ScreenUpdate(ScreenUpdateData),
    ScreenSnapshot(ScreenSnapshotData),
    Exit(Exit),
    Error(ServerError),
    Bell,
    SessionCreated(SessionCreated),
    SessionAttached(SessionAttached),
    SessionDetached(SessionDetached),
    SessionDestroyed(SessionDestroyed),
    SessionList(SessionListData),
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

            _ => {
                // Session management messages — encode as needed.
                // For now, create an empty message.
                let msg = fbs::ClientMessage::create(
                    &mut fbb,
                    &fbs::ClientMessageArgs {
                        body_type: fbs::ClientBody::NONE,
                        body: None,
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
            _ => {
                // Session management messages — TODO: full encode.
                let msg = fbs::ServerMessage::create(
                    &mut fbb,
                    &fbs::ServerMessageArgs {
                        body_type: fbs::ServerBody::NONE,
                        body: None,
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
                .map(|c| fbs::Cell::new(c.ch as u32, c.fg, c.bg, c.flags))
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
            style: su.cursor.style,
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
            mouse_tracking_mode: su.mouse_tracking_mode,
            alt_screen_active: su.alt_screen_active,
            application_cursor_keys: su.application_cursor_keys,
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
            style: ss.cursor.style,
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
            mouse_tracking_mode: ss.mouse_tracking_mode,
            alt_screen_active: ss.alt_screen_active,
            application_cursor_keys: ss.application_cursor_keys,
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
                flags: c.flags(),
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
            style: cursor_fb.style(),
        },
        cols: su.cols(),
        rows: su.rows(),
        title: su.title().map(|t| t.to_string()),
        mouse_tracking_mode: su.mouse_tracking_mode(),
        alt_screen_active: su.alt_screen_active(),
        application_cursor_keys: su.application_cursor_keys(),
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
            style: cursor_fb.style(),
        },
        cols: ss.cols(),
        num_rows: ss.num_rows(),
        title: ss.title().map(|t| t.to_string()),
        mouse_tracking_mode: ss.mouse_tracking_mode(),
        alt_screen_active: ss.alt_screen_active(),
        application_cursor_keys: ss.application_cursor_keys(),
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
                        flags: ATTR_BOLD,
                    },
                    CellData {
                        ch: 'i',
                        fg: pack_color_rgb(255, 0, 0),
                        bg: COLOR_DEFAULT,
                        flags: 0,
                    },
                ],
            }],
            cursor: CursorData {
                row: 0,
                col: 2,
                visible: true,
                style: 0,
            },
            cols: 80,
            rows: 24,
            title: Some("test".into()),
            mouse_tracking_mode: 0,
            alt_screen_active: false,
            application_cursor_keys: false,
        });
        let decoded = ServerMsg::decode_flatbuffer(&msg.encode_flatbuffer()).unwrap();
        match decoded {
            ServerMsg::ScreenUpdate(su) => {
                assert_eq!(su.cols, 80);
                assert_eq!(su.changes.len(), 1);
                assert_eq!(su.changes[0].cells[0].ch, 'H');
                assert_eq!(su.changes[0].cells[0].flags, ATTR_BOLD);
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
                    flags: 0,
                }],
            }],
            cursor: CursorData {
                row: 0,
                col: 1,
                visible: true,
                style: 0,
            },
            cols: 80,
            num_rows: 24,
            title: None,
            mouse_tracking_mode: 0,
            alt_screen_active: false,
            application_cursor_keys: false,
        });
        let decoded = ServerMsg::decode_flatbuffer(&msg.encode_flatbuffer()).unwrap();
        match decoded {
            ServerMsg::ScreenSnapshot(ss) => {
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
                style: 0,
            },
            cols: 80,
            rows: 24,
            title: None,
            mouse_tracking_mode: 0,
            alt_screen_active: false,
            application_cursor_keys: false,
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
                style: 0,
            },
            cols: 120,
            num_rows: 40,
            title: Some("my terminal".into()),
            mouse_tracking_mode: 0,
            alt_screen_active: false,
            application_cursor_keys: false,
        });
        let decoded = ServerMsg::decode_flatbuffer(&msg.encode_flatbuffer()).unwrap();
        match decoded {
            ServerMsg::ScreenSnapshot(ss) => {
                assert_eq!(ss.title.as_deref(), Some("my terminal"));
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
        assert_eq!(ATTR_INVERSE, 1 << 0);
        assert_eq!(ATTR_BOLD, 1 << 1);
        assert_eq!(ATTR_ITALIC, 1 << 2);
        assert_eq!(ATTR_UNDERLINE, 1 << 3);
        assert_eq!(ATTR_WRAPLINE, 1 << 4);
        assert_eq!(ATTR_WIDE_CHAR, 1 << 5);
        assert_eq!(ATTR_WIDE_CHAR_SPACER, 1 << 6);
        assert_eq!(ATTR_DIM, 1 << 7);
        assert_eq!(ATTR_HIDDEN, 1 << 8);
        assert_eq!(ATTR_STRIKEOUT, 1 << 9);
        assert_eq!(ATTR_LEADING_WIDE_CHAR_SPACER, 1 << 10);
        assert_eq!(ATTR_DOUBLE_UNDERLINE, 1 << 11);
        assert_eq!(ATTR_UNDERCURL, 1 << 12);
        assert_eq!(ATTR_DOTTED_UNDERLINE, 1 << 13);
        assert_eq!(ATTR_DASHED_UNDERLINE, 1 << 14);
        // No overlap: all 15 bits are distinct.
        let all = ATTR_INVERSE
            | ATTR_BOLD
            | ATTR_ITALIC
            | ATTR_UNDERLINE
            | ATTR_WRAPLINE
            | ATTR_WIDE_CHAR
            | ATTR_WIDE_CHAR_SPACER
            | ATTR_DIM
            | ATTR_HIDDEN
            | ATTR_STRIKEOUT
            | ATTR_LEADING_WIDE_CHAR_SPACER
            | ATTR_DOUBLE_UNDERLINE
            | ATTR_UNDERCURL
            | ATTR_DOTTED_UNDERLINE
            | ATTR_DASHED_UNDERLINE;
        assert_eq!(all, 0x7FFF);
    }

    // ── Automation message round-trip tests ─────────────────────────────────

    #[test]
    fn round_trip_create_session_request() {
        let msg = CreateSessionRequest {
            session_name: "myses".into(),
            shell: "/bin/zsh".into(),
            cols: 120,
            rows: 40,
        };
        let decoded = CreateSessionRequest::decode_flatbuffer(&msg.encode_flatbuffer()).unwrap();
        assert_eq!(decoded.session_name, "myses");
        assert_eq!(decoded.shell, "/bin/zsh");
        assert_eq!(decoded.cols, 120);
        assert_eq!(decoded.rows, 40);
    }

    #[test]
    fn round_trip_create_session_response() {
        let ok = CreateSessionResponse {
            success: true,
            error: String::new(),
        };
        let d = CreateSessionResponse::decode_flatbuffer(&ok.encode_flatbuffer()).unwrap();
        assert!(d.success);
        assert!(d.error.is_empty());

        let err = CreateSessionResponse {
            success: false,
            error: "already exists".into(),
        };
        let d = CreateSessionResponse::decode_flatbuffer(&err.encode_flatbuffer()).unwrap();
        assert!(!d.success);
        assert_eq!(d.error, "already exists");
    }

    #[test]
    fn round_trip_kill_session_request() {
        let msg = KillSessionRequest {
            session_name: "to-kill".into(),
        };
        let decoded = KillSessionRequest::decode_flatbuffer(&msg.encode_flatbuffer()).unwrap();
        assert_eq!(decoded.session_name, "to-kill");
    }

    #[test]
    fn round_trip_kill_session_response() {
        let ok = KillSessionResponse {
            success: true,
            error: String::new(),
        };
        let d = KillSessionResponse::decode_flatbuffer(&ok.encode_flatbuffer()).unwrap();
        assert!(d.success);

        let err = KillSessionResponse {
            success: false,
            error: "not found".into(),
        };
        let d = KillSessionResponse::decode_flatbuffer(&err.encode_flatbuffer()).unwrap();
        assert!(!d.success);
        assert_eq!(d.error, "not found");
    }

    #[test]
    fn round_trip_resize_session_request() {
        let msg = ResizeSessionRequest {
            session_name: "s".into(),
            cols: 200,
            rows: 50,
        };
        let decoded = ResizeSessionRequest::decode_flatbuffer(&msg.encode_flatbuffer()).unwrap();
        assert_eq!(decoded.session_name, "s");
        assert_eq!(decoded.cols, 200);
        assert_eq!(decoded.rows, 50);
    }

    #[test]
    fn round_trip_resize_session_response() {
        let ok = ResizeSessionResponse {
            success: true,
            error: String::new(),
        };
        let d = ResizeSessionResponse::decode_flatbuffer(&ok.encode_flatbuffer()).unwrap();
        assert!(d.success);

        let err = ResizeSessionResponse {
            success: false,
            error: "session not found".into(),
        };
        let d = ResizeSessionResponse::decode_flatbuffer(&err.encode_flatbuffer()).unwrap();
        assert!(!d.success);
        assert_eq!(d.error, "session not found");
    }

    #[test]
    fn round_trip_send_keys_request() {
        let msg = SendKeysRequest {
            session_name: "s".into(),
            keys: vec![0x1b, 0x5b, 0x41], // ESC [ A = arrow up
        };
        let decoded = SendKeysRequest::decode_flatbuffer(&msg.encode_flatbuffer()).unwrap();
        assert_eq!(decoded.session_name, "s");
        assert_eq!(decoded.keys, vec![0x1b, 0x5b, 0x41]);
    }

    #[test]
    fn round_trip_send_keys_response() {
        let ok = SendKeysResponse {
            success: true,
            error: String::new(),
        };
        let d = SendKeysResponse::decode_flatbuffer(&ok.encode_flatbuffer()).unwrap();
        assert!(d.success);

        let err = SendKeysResponse {
            success: false,
            error: "PTY closed".into(),
        };
        let d = SendKeysResponse::decode_flatbuffer(&err.encode_flatbuffer()).unwrap();
        assert!(!d.success);
        assert_eq!(d.error, "PTY closed");
    }

    #[test]
    fn round_trip_wait_for_text_request() {
        let msg = WaitForTextRequest {
            session_name: "s".into(),
            pattern: ">>>".into(),
            timeout_ms: 5000,
        };
        let decoded = WaitForTextRequest::decode_flatbuffer(&msg.encode_flatbuffer()).unwrap();
        assert_eq!(decoded.session_name, "s");
        assert_eq!(decoded.pattern, ">>>");
        assert_eq!(decoded.timeout_ms, 5000);
    }

    #[test]
    fn round_trip_wait_for_text_request_zero_timeout() {
        // timeout_ms=0 is the assert path (check once, return immediately).
        let msg = WaitForTextRequest {
            session_name: "s".into(),
            pattern: "INSERT".into(),
            timeout_ms: 0,
        };
        let decoded = WaitForTextRequest::decode_flatbuffer(&msg.encode_flatbuffer()).unwrap();
        assert_eq!(decoded.timeout_ms, 0);
    }

    #[test]
    fn round_trip_wait_for_text_response() {
        let found = WaitForTextResponse {
            found: true,
            plain_text: "hello\n".into(),
        };
        let d = WaitForTextResponse::decode_flatbuffer(&found.encode_flatbuffer()).unwrap();
        assert!(d.found);
        assert_eq!(d.plain_text, "hello\n");

        let not_found = WaitForTextResponse {
            found: false,
            plain_text: String::new(),
        };
        let d = WaitForTextResponse::decode_flatbuffer(&not_found.encode_flatbuffer()).unwrap();
        assert!(!d.found);
    }

    #[test]
    fn round_trip_press_keys_request() {
        let msg = PressKeysRequest {
            session_name: "s".into(),
            keys: vec!["Up".into(), "Up".into(), "Enter".into()],
        };
        let decoded = PressKeysRequest::decode_flatbuffer(&msg.encode_flatbuffer()).unwrap();
        assert_eq!(decoded.session_name, "s");
        assert_eq!(decoded.keys, vec!["Up", "Up", "Enter"]);
    }

    #[test]
    fn round_trip_press_keys_request_empty() {
        let msg = PressKeysRequest {
            session_name: "s".into(),
            keys: vec![],
        };
        let decoded = PressKeysRequest::decode_flatbuffer(&msg.encode_flatbuffer()).unwrap();
        assert!(decoded.keys.is_empty());
    }

    #[test]
    fn round_trip_press_keys_response() {
        let ok = PressKeysResponse {
            success: true,
            error: String::new(),
        };
        let d = PressKeysResponse::decode_flatbuffer(&ok.encode_flatbuffer()).unwrap();
        assert!(d.success);

        let err = PressKeysResponse {
            success: false,
            error: "unknown key name: \"Bogus\"".into(),
        };
        let d = PressKeysResponse::decode_flatbuffer(&err.encode_flatbuffer()).unwrap();
        assert!(!d.success);
        assert!(d.error.contains("Bogus"));
    }

    #[test]
    fn round_trip_cell_data_underline_flags() {
        let underline_flags = [
            ("DOUBLE_UNDERLINE", ATTR_DOUBLE_UNDERLINE),
            ("UNDERCURL", ATTR_UNDERCURL),
            ("DOTTED_UNDERLINE", ATTR_DOTTED_UNDERLINE),
            ("DASHED_UNDERLINE", ATTR_DASHED_UNDERLINE),
        ];

        for (name, flag) in underline_flags {
            let msg = ServerMsg::ScreenUpdate(ScreenUpdateData {
                changes: vec![CellRangeData {
                    row: 0,
                    col_start: 0,
                    cells: vec![CellData {
                        ch: 'X',
                        fg: COLOR_DEFAULT,
                        bg: COLOR_DEFAULT,
                        flags: flag,
                    }],
                }],
                cursor: CursorData {
                    row: 0,
                    col: 1,
                    visible: true,
                    style: 0,
                },
                cols: 80,
                rows: 24,
                title: None,
                mouse_tracking_mode: 0,
                alt_screen_active: false,
                application_cursor_keys: false,
            });
            let decoded = ServerMsg::decode_flatbuffer(&msg.encode_flatbuffer()).unwrap();
            match decoded {
                ServerMsg::ScreenUpdate(su) => {
                    assert_eq!(
                        su.changes[0].cells[0].flags, flag,
                        "flag {name} did not round-trip"
                    );
                }
                _ => panic!("expected ScreenUpdate for {name}"),
            }
        }
    }

    #[test]
    fn round_trip_run_command_request() {
        let msg = RunCommandRequest {
            session_name: "s".into(),
            command: "echo hello-world".into(),
            timeout_ms: 10000,
        };
        let decoded = RunCommandRequest::decode_flatbuffer(&msg.encode_flatbuffer()).unwrap();
        assert_eq!(decoded.session_name, "s");
        assert_eq!(decoded.command, "echo hello-world");
        assert_eq!(decoded.timeout_ms, 10000);
    }

    #[test]
    fn round_trip_run_command_response() {
        let ok = RunCommandResponse {
            output: "hello-world".into(),
            timed_out: false,
        };
        let d = RunCommandResponse::decode_flatbuffer(&ok.encode_flatbuffer()).unwrap();
        assert_eq!(d.output, "hello-world");
        assert!(!d.timed_out);

        let timeout = RunCommandResponse {
            output: String::new(),
            timed_out: true,
        };
        let d = RunCommandResponse::decode_flatbuffer(&timeout.encode_flatbuffer()).unwrap();
        assert!(d.timed_out);
    }

    #[test]
    fn round_trip_run_command_response_multiline() {
        let msg = RunCommandResponse {
            output: "line1\nline2\nline3".into(),
            timed_out: false,
        };
        let decoded = RunCommandResponse::decode_flatbuffer(&msg.encode_flatbuffer()).unwrap();
        assert_eq!(decoded.output, "line1\nline2\nline3");
        assert!(!decoded.timed_out);
    }
}
#[derive(Debug, Clone)]
pub struct GetSnapshotRequest {
    pub session_name: String,
}

impl FlatBufferGrpcMessage for GetSnapshotRequest {
    fn encode_flatbuffer(&self) -> Vec<u8> {
        let mut fbb = flatbuffers::FlatBufferBuilder::new();
        let name = fbb.create_string(&self.session_name);
        let req = fbs::GetSnapshotRequest::create(
            &mut fbb,
            &fbs::GetSnapshotRequestArgs {
                session_name: Some(name),
            },
        );
        fbb.finish(req, None);
        fbb.finished_data().to_vec()
    }
    fn decode_flatbuffer(data: &[u8]) -> Result<Self, String> {
        let req = flatbuffers::root::<fbs::GetSnapshotRequest>(data)
            .map_err(|e| format!("decode error: {}", e))?;
        Ok(GetSnapshotRequest {
            session_name: req.session_name().unwrap_or("").to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct GetSnapshotResponse {
    pub snapshot: ScreenSnapshotData,
    pub plain_text: String,
}

impl FlatBufferGrpcMessage for GetSnapshotResponse {
    fn encode_flatbuffer(&self) -> Vec<u8> {
        let mut fbb = flatbuffers::FlatBufferBuilder::new();

        let mut row_offsets = Vec::with_capacity(self.snapshot.rows.len());
        for row in &self.snapshot.rows {
            let mut fbs_cells = Vec::with_capacity(row.cells.len());
            for cell in &row.cells {
                fbs_cells.push(fbs::Cell::new(cell.ch as u32, cell.fg, cell.bg, cell.flags));
            }
            let cells_vec = fbb.create_vector(&fbs_cells);
            row_offsets.push(fbs::CellRange::create(
                &mut fbb,
                &fbs::CellRangeArgs {
                    row: row.row,
                    col_start: row.col_start,
                    cells: Some(cells_vec),
                },
            ));
        }
        let rows_vec = fbb.create_vector(&row_offsets);

        // cursor
        let cursor = fbs::CursorState::create(
            &mut fbb,
            &fbs::CursorStateArgs {
                row: self.snapshot.cursor.row,
                col: self.snapshot.cursor.col,
                visible: self.snapshot.cursor.visible,
                style: self.snapshot.cursor.style,
            },
        );

        let title = self.snapshot.title.as_deref().map(|t| fbb.create_string(t));

        let snap = fbs::ScreenSnapshot::create(
            &mut fbb,
            &fbs::ScreenSnapshotArgs {
                rows: Some(rows_vec),
                cursor: Some(cursor),
                cols: self.snapshot.cols,
                num_rows: self.snapshot.num_rows,
                title,
                mouse_tracking_mode: self.snapshot.mouse_tracking_mode,
                alt_screen_active: self.snapshot.alt_screen_active,
                application_cursor_keys: self.snapshot.application_cursor_keys,
            },
        );

        let text = fbb.create_string(&self.plain_text);

        let res = fbs::GetSnapshotResponse::create(
            &mut fbb,
            &fbs::GetSnapshotResponseArgs {
                snapshot: Some(snap),
                plain_text: Some(text),
            },
        );
        fbb.finish(res, None);
        fbb.finished_data().to_vec()
    }

    fn decode_flatbuffer(data: &[u8]) -> Result<Self, String> {
        let res = flatbuffers::root::<fbs::GetSnapshotResponse>(data)
            .map_err(|e| format!("decode error: {}", e))?;

        let snap = res.snapshot().ok_or("missing snapshot")?;

        let mut rows = Vec::new();
        if let Some(r_vec) = snap.rows() {
            for r in r_vec {
                let mut cells = Vec::new();
                if let Some(c_vec) = r.cells() {
                    for c in c_vec {
                        cells.push(CellData {
                            ch: char::from_u32(c.ch()).unwrap_or(' '),
                            fg: c.fg(),
                            bg: c.bg(),
                            flags: c.flags(),
                        });
                    }
                }
                rows.push(CellRangeData {
                    row: r.row(),
                    col_start: r.col_start(),
                    cells,
                });
            }
        }

        let cursor = snap.cursor().ok_or("missing cursor")?;
        let cursor_data = CursorData {
            row: cursor.row(),
            col: cursor.col(),
            visible: cursor.visible(),
            style: cursor.style(),
        };

        let snapshot = ScreenSnapshotData {
            rows,
            cursor: cursor_data,
            cols: snap.cols(),
            num_rows: snap.num_rows(),
            title: snap.title().map(|t| t.to_string()),
            mouse_tracking_mode: snap.mouse_tracking_mode(),
            alt_screen_active: snap.alt_screen_active(),
            application_cursor_keys: snap.application_cursor_keys(),
        };

        Ok(GetSnapshotResponse {
            snapshot,
            plain_text: res.plain_text().unwrap_or("").to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct TypeRequest {
    pub session_name: String,
    pub text: String,
}

impl FlatBufferGrpcMessage for TypeRequest {
    fn encode_flatbuffer(&self) -> Vec<u8> {
        let mut fbb = flatbuffers::FlatBufferBuilder::new();
        let name = fbb.create_string(&self.session_name);
        let txt = fbb.create_string(&self.text);
        let req = fbs::TypeRequest::create(
            &mut fbb,
            &fbs::TypeRequestArgs {
                session_name: Some(name),
                text: Some(txt),
            },
        );
        fbb.finish(req, None);
        fbb.finished_data().to_vec()
    }
    fn decode_flatbuffer(data: &[u8]) -> Result<Self, String> {
        let req = flatbuffers::root::<fbs::TypeRequest>(data)
            .map_err(|e| format!("decode error: {}", e))?;
        Ok(TypeRequest {
            session_name: req.session_name().unwrap_or("").to_string(),
            text: req.text().unwrap_or("").to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct TypeResponse {
    pub success: bool,
    pub error: String,
}

impl FlatBufferGrpcMessage for TypeResponse {
    fn encode_flatbuffer(&self) -> Vec<u8> {
        let mut fbb = flatbuffers::FlatBufferBuilder::new();
        let err = fbb.create_string(&self.error);
        let res = fbs::TypeResponse::create(
            &mut fbb,
            &fbs::TypeResponseArgs {
                success: self.success,
                error: Some(err),
            },
        );
        fbb.finish(res, None);
        fbb.finished_data().to_vec()
    }
    fn decode_flatbuffer(data: &[u8]) -> Result<Self, String> {
        let res = flatbuffers::root::<fbs::TypeResponse>(data)
            .map_err(|e| format!("decode error: {}", e))?;
        Ok(TypeResponse {
            success: res.success(),
            error: res.error().unwrap_or("").to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct UnaryListSessionsRequest {}

impl FlatBufferGrpcMessage for UnaryListSessionsRequest {
    fn encode_flatbuffer(&self) -> Vec<u8> {
        let mut fbb = flatbuffers::FlatBufferBuilder::new();
        let req =
            fbs::UnaryListSessionsRequest::create(&mut fbb, &fbs::UnaryListSessionsRequestArgs {});
        fbb.finish(req, None);
        fbb.finished_data().to_vec()
    }
    fn decode_flatbuffer(_data: &[u8]) -> Result<Self, String> {
        Ok(UnaryListSessionsRequest {})
    }
}

#[derive(Debug, Clone)]
pub struct UnaryListSessionsResponse {
    pub sessions: Vec<SessionInfo>,
}

impl FlatBufferGrpcMessage for UnaryListSessionsResponse {
    fn encode_flatbuffer(&self) -> Vec<u8> {
        let mut fbb = flatbuffers::FlatBufferBuilder::new();

        let mut session_offsets = Vec::with_capacity(self.sessions.len());
        for s in &self.sessions {
            let s_id = fbb.create_string(&s.session_id);
            let s_name = fbb.create_string(&s.name);
            let s_shell = fbb.create_string(&s.shell);
            let s_title = s.title.as_deref().map(|t| fbb.create_string(t));

            session_offsets.push(fbs::SessionInfo::create(
                &mut fbb,
                &fbs::SessionInfoArgs {
                    session_id: Some(s_id),
                    name: Some(s_name),
                    shell: Some(s_shell),
                    created_at: s.created_at,
                    last_activity: s.last_activity,
                    attached: s.attached,
                    cols: s.cols,
                    rows: s.rows,
                    title: s_title,
                },
            ));
        }
        let sessions_vec = fbb.create_vector(&session_offsets);

        let res = fbs::UnaryListSessionsResponse::create(
            &mut fbb,
            &fbs::UnaryListSessionsResponseArgs {
                sessions: Some(sessions_vec),
            },
        );
        fbb.finish(res, None);
        fbb.finished_data().to_vec()
    }

    fn decode_flatbuffer(data: &[u8]) -> Result<Self, String> {
        let res = flatbuffers::root::<fbs::UnaryListSessionsResponse>(data)
            .map_err(|e| format!("decode error: {}", e))?;

        let mut sessions = Vec::new();
        if let Some(s_vec) = res.sessions() {
            for s in s_vec {
                sessions.push(SessionInfo {
                    session_id: s.session_id().unwrap_or("").to_string(),
                    name: s.name().unwrap_or("").to_string(),
                    shell: s.shell().unwrap_or("").to_string(),
                    created_at: s.created_at(),
                    last_activity: s.last_activity(),
                    attached: s.attached(),
                    cols: s.cols(),
                    rows: s.rows(),
                    title: s.title().map(|t| t.to_string()),
                });
            }
        }

        Ok(UnaryListSessionsResponse { sessions })
    }
}

// ============================================================================
// Automation message binary encoding helpers (simple TLV-like format)
// ============================================================================
//
// These new automation messages use a hand-rolled binary format so we can add
// new RPC types without running the flatc compiler. Both the CLI client and the
// relay server encode/decode with the same helpers, so there is no external
// consumer that requires actual FlatBuffers.

struct AutoWriter(Vec<u8>);

impl AutoWriter {
    fn new() -> Self {
        Self(Vec::new())
    }

    fn str(&mut self, s: &str) {
        let b = s.as_bytes();
        self.0.extend_from_slice(&(b.len() as u32).to_be_bytes());
        self.0.extend_from_slice(b);
    }

    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }

    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }

    fn bool(&mut self, v: bool) {
        self.0.push(v as u8);
    }

    fn bytes(&mut self, b: &[u8]) {
        self.0.extend_from_slice(&(b.len() as u32).to_be_bytes());
        self.0.extend_from_slice(b);
    }

    fn u32_usize(&mut self, v: usize) {
        self.0.extend_from_slice(&(v as u32).to_be_bytes());
    }

    fn finish(self) -> Vec<u8> {
        self.0
    }
}

struct AutoReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> AutoReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn u32(&mut self) -> Result<u32, String> {
        if self.pos + 4 > self.data.len() {
            return Err("truncated u32".into());
        }
        let v = u32::from_be_bytes(self.data[self.pos..self.pos + 4].try_into().unwrap());
        self.pos += 4;
        Ok(v)
    }

    fn str(&mut self) -> Result<String, String> {
        let len = self.u32()? as usize;
        if self.pos + len > self.data.len() {
            return Err("truncated string".into());
        }
        let s = std::str::from_utf8(&self.data[self.pos..self.pos + len])
            .map_err(|e| e.to_string())?
            .to_string();
        self.pos += len;
        Ok(s)
    }

    fn u16(&mut self) -> Result<u16, String> {
        if self.pos + 2 > self.data.len() {
            return Err("truncated u16".into());
        }
        let v = u16::from_be_bytes(self.data[self.pos..self.pos + 2].try_into().unwrap());
        self.pos += 2;
        Ok(v)
    }

    fn u64(&mut self) -> Result<u64, String> {
        if self.pos + 8 > self.data.len() {
            return Err("truncated u64".into());
        }
        let v = u64::from_be_bytes(self.data[self.pos..self.pos + 8].try_into().unwrap());
        self.pos += 8;
        Ok(v)
    }

    fn bool(&mut self) -> Result<bool, String> {
        if self.pos >= self.data.len() {
            return Err("truncated bool".into());
        }
        let v = self.data[self.pos] != 0;
        self.pos += 1;
        Ok(v)
    }

    fn bytes(&mut self) -> Result<Vec<u8>, String> {
        let len = self.u32()? as usize;
        if self.pos + len > self.data.len() {
            return Err("truncated bytes".into());
        }
        let b = self.data[self.pos..self.pos + len].to_vec();
        self.pos += len;
        Ok(b)
    }
}

// ============================================================================
// Automation protocol types
// ============================================================================

/// Explicitly create a named session (lazy sessions are also supported via
/// TypeAction/GetSnapshot, but this gives control over shell and size).
#[derive(Debug, Clone)]
pub struct CreateSessionRequest {
    pub session_name: String,
    /// Shell to launch. Empty string → server default (bash).
    pub shell: String,
    pub cols: u16,
    pub rows: u16,
}

impl FlatBufferGrpcMessage for CreateSessionRequest {
    fn encode_flatbuffer(&self) -> Vec<u8> {
        let mut w = AutoWriter::new();
        w.str(&self.session_name);
        w.str(&self.shell);
        w.u16(self.cols);
        w.u16(self.rows);
        w.finish()
    }
    fn decode_flatbuffer(data: &[u8]) -> Result<Self, String> {
        let mut r = AutoReader::new(data);
        Ok(CreateSessionRequest {
            session_name: r.str()?,
            shell: r.str()?,
            cols: r.u16()?,
            rows: r.u16()?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct CreateSessionResponse {
    pub success: bool,
    pub error: String,
}

impl FlatBufferGrpcMessage for CreateSessionResponse {
    fn encode_flatbuffer(&self) -> Vec<u8> {
        let mut w = AutoWriter::new();
        w.bool(self.success);
        w.str(&self.error);
        w.finish()
    }
    fn decode_flatbuffer(data: &[u8]) -> Result<Self, String> {
        let mut r = AutoReader::new(data);
        Ok(CreateSessionResponse {
            success: r.bool()?,
            error: r.str()?,
        })
    }
}

/// Kill (destroy) a named session.
#[derive(Debug, Clone)]
pub struct KillSessionRequest {
    pub session_name: String,
}

impl FlatBufferGrpcMessage for KillSessionRequest {
    fn encode_flatbuffer(&self) -> Vec<u8> {
        let mut w = AutoWriter::new();
        w.str(&self.session_name);
        w.finish()
    }
    fn decode_flatbuffer(data: &[u8]) -> Result<Self, String> {
        let mut r = AutoReader::new(data);
        Ok(KillSessionRequest {
            session_name: r.str()?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct KillSessionResponse {
    pub success: bool,
    pub error: String,
}

impl FlatBufferGrpcMessage for KillSessionResponse {
    fn encode_flatbuffer(&self) -> Vec<u8> {
        let mut w = AutoWriter::new();
        w.bool(self.success);
        w.str(&self.error);
        w.finish()
    }
    fn decode_flatbuffer(data: &[u8]) -> Result<Self, String> {
        let mut r = AutoReader::new(data);
        Ok(KillSessionResponse {
            success: r.bool()?,
            error: r.str()?,
        })
    }
}

/// Resize a session's terminal.
#[derive(Debug, Clone)]
pub struct ResizeSessionRequest {
    pub session_name: String,
    pub cols: u16,
    pub rows: u16,
}

impl FlatBufferGrpcMessage for ResizeSessionRequest {
    fn encode_flatbuffer(&self) -> Vec<u8> {
        let mut w = AutoWriter::new();
        w.str(&self.session_name);
        w.u16(self.cols);
        w.u16(self.rows);
        w.finish()
    }
    fn decode_flatbuffer(data: &[u8]) -> Result<Self, String> {
        let mut r = AutoReader::new(data);
        Ok(ResizeSessionRequest {
            session_name: r.str()?,
            cols: r.u16()?,
            rows: r.u16()?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ResizeSessionResponse {
    pub success: bool,
    pub error: String,
}

impl FlatBufferGrpcMessage for ResizeSessionResponse {
    fn encode_flatbuffer(&self) -> Vec<u8> {
        let mut w = AutoWriter::new();
        w.bool(self.success);
        w.str(&self.error);
        w.finish()
    }
    fn decode_flatbuffer(data: &[u8]) -> Result<Self, String> {
        let mut r = AutoReader::new(data);
        Ok(ResizeSessionResponse {
            success: r.bool()?,
            error: r.str()?,
        })
    }
}

/// Send raw PTY bytes (for special keys: arrows, Ctrl+C, Escape, etc.).
#[derive(Debug, Clone)]
pub struct SendKeysRequest {
    pub session_name: String,
    /// Raw PTY bytes (e.g. `\x03` for Ctrl+C, `\x1b[A` for arrow-up).
    pub keys: Vec<u8>,
}

impl FlatBufferGrpcMessage for SendKeysRequest {
    fn encode_flatbuffer(&self) -> Vec<u8> {
        let mut w = AutoWriter::new();
        w.str(&self.session_name);
        w.bytes(&self.keys);
        w.finish()
    }
    fn decode_flatbuffer(data: &[u8]) -> Result<Self, String> {
        let mut r = AutoReader::new(data);
        Ok(SendKeysRequest {
            session_name: r.str()?,
            keys: r.bytes()?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SendKeysResponse {
    pub success: bool,
    pub error: String,
}

impl FlatBufferGrpcMessage for SendKeysResponse {
    fn encode_flatbuffer(&self) -> Vec<u8> {
        let mut w = AutoWriter::new();
        w.bool(self.success);
        w.str(&self.error);
        w.finish()
    }
    fn decode_flatbuffer(data: &[u8]) -> Result<Self, String> {
        let mut r = AutoReader::new(data);
        Ok(SendKeysResponse {
            success: r.bool()?,
            error: r.str()?,
        })
    }
}

/// Block until `pattern` appears on the screen (or until `timeout_ms` elapses).
/// The server polls the VT state every 100 ms.
#[derive(Debug, Clone)]
pub struct WaitForTextRequest {
    pub session_name: String,
    pub pattern: String,
    /// Maximum wait in milliseconds.
    pub timeout_ms: u64,
}

impl FlatBufferGrpcMessage for WaitForTextRequest {
    fn encode_flatbuffer(&self) -> Vec<u8> {
        let mut w = AutoWriter::new();
        w.str(&self.session_name);
        w.str(&self.pattern);
        w.u64(self.timeout_ms);
        w.finish()
    }
    fn decode_flatbuffer(data: &[u8]) -> Result<Self, String> {
        let mut r = AutoReader::new(data);
        Ok(WaitForTextRequest {
            session_name: r.str()?,
            pattern: r.str()?,
            timeout_ms: r.u64()?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct WaitForTextResponse {
    /// `true` if the pattern was found before the timeout.
    pub found: bool,
    /// Plain-text snapshot at the time the pattern matched (or at timeout).
    pub plain_text: String,
}

impl FlatBufferGrpcMessage for WaitForTextResponse {
    fn encode_flatbuffer(&self) -> Vec<u8> {
        let mut w = AutoWriter::new();
        w.bool(self.found);
        w.str(&self.plain_text);
        w.finish()
    }
    fn decode_flatbuffer(data: &[u8]) -> Result<Self, String> {
        let mut r = AutoReader::new(data);
        Ok(WaitForTextResponse {
            found: r.bool()?,
            plain_text: r.str()?,
        })
    }
}

/// Send one or more named keys to a session.
///
/// The server resolves each name to the correct PTY byte sequence, taking into
/// account the session's current `application_cursor_keys` VT mode (which vim
/// and other TUIs activate, changing arrow key encoding from `\x1b[A` to `\x1bOA`).
///
/// Supported names: Enter, Escape/Esc, Tab, Backspace, Delete,
/// Up/ArrowUp, Down/ArrowDown, Left/ArrowLeft, Right/ArrowRight,
/// Home, End, PageUp, PageDown,
/// Ctrl+C/C-c, Ctrl+D/C-d, Ctrl+Z/C-z, Ctrl+L/C-l,
/// Ctrl+A/C-a, Ctrl+E/C-e, Ctrl+U/C-u, Ctrl+W/C-w,
/// F1–F12.
#[derive(Debug, Clone)]
pub struct PressKeysRequest {
    pub session_name: String,
    /// Named key list, e.g. `["Up", "Up", "Enter"]`.
    pub keys: Vec<String>,
}

impl FlatBufferGrpcMessage for PressKeysRequest {
    fn encode_flatbuffer(&self) -> Vec<u8> {
        let mut w = AutoWriter::new();
        w.str(&self.session_name);
        w.u32_usize(self.keys.len());
        for k in &self.keys {
            w.str(k);
        }
        w.finish()
    }
    fn decode_flatbuffer(data: &[u8]) -> Result<Self, String> {
        let mut r = AutoReader::new(data);
        let session_name = r.str()?;
        let count = r.u32()? as usize;
        let mut keys = Vec::with_capacity(count);
        for _ in 0..count {
            keys.push(r.str()?);
        }
        Ok(PressKeysRequest { session_name, keys })
    }
}

#[derive(Debug, Clone)]
pub struct PressKeysResponse {
    pub success: bool,
    pub error: String,
}

impl FlatBufferGrpcMessage for PressKeysResponse {
    fn encode_flatbuffer(&self) -> Vec<u8> {
        let mut w = AutoWriter::new();
        w.bool(self.success);
        w.str(&self.error);
        w.finish()
    }
    fn decode_flatbuffer(data: &[u8]) -> Result<Self, String> {
        let mut r = AutoReader::new(data);
        Ok(PressKeysResponse {
            success: r.bool()?,
            error: r.str()?,
        })
    }
}

/// Run a shell command and capture its output.
///
/// The server appends a unique sentinel (`; echo "RTERM_DONE_<id>"`) to the
/// command so it can reliably detect when the command has finished without
/// relying on shell prompt detection.
#[derive(Debug, Clone)]
pub struct RunCommandRequest {
    pub session_name: String,
    pub command: String,
    /// Maximum wait in milliseconds (default: 10 000 ms).
    pub timeout_ms: u64,
}

impl FlatBufferGrpcMessage for RunCommandRequest {
    fn encode_flatbuffer(&self) -> Vec<u8> {
        let mut w = AutoWriter::new();
        w.str(&self.session_name);
        w.str(&self.command);
        w.u64(self.timeout_ms);
        w.finish()
    }
    fn decode_flatbuffer(data: &[u8]) -> Result<Self, String> {
        let mut r = AutoReader::new(data);
        Ok(RunCommandRequest {
            session_name: r.str()?,
            command: r.str()?,
            timeout_ms: r.u64()?,
        })
    }
}

#[derive(Debug, Clone)]
pub struct RunCommandResponse {
    /// Combined terminal output captured after the command was sent.
    pub output: String,
    pub timed_out: bool,
}

impl FlatBufferGrpcMessage for RunCommandResponse {
    fn encode_flatbuffer(&self) -> Vec<u8> {
        let mut w = AutoWriter::new();
        w.str(&self.output);
        w.bool(self.timed_out);
        w.finish()
    }
    fn decode_flatbuffer(data: &[u8]) -> Result<Self, String> {
        let mut r = AutoReader::new(data);
        Ok(RunCommandResponse {
            output: r.str()?,
            timed_out: r.bool()?,
        })
    }
}
