<!-- agent-updated: 2026-04-02T22:00:00Z -->
# rterm Tasks

## Task Dependency Graph

```
Track A: VT Emulation              Track B: Crate Extraction → Mobile
─────────────────────              ──────────────────────────────────
#1  Audit VT gaps                  #3  Extract rterm-transport
      │                                  │           │
      ▼                                  ▼           ▼
#2  Fix P0/P1 VT gaps             #4  rterm-session  #6  SshTransport
                                         │                 │
                                         ▼                 │
                                   #5  rterm-service       │
                                         │                 │
                                         └────────┬────────┘
                                                  ▼
                                         #7  rterm-agent binary
                                                  │
                                                  ▼
                                         #8  Flutter scaffold
                                                  │
                                                  ▼
                                         #9  Flutter terminal + UX
```

Tracks A and B are independent and can run in parallel.

---

## Track A: VT Emulation

### Task 1: VT emulation coverage audit

**Status:** done (2026-04-02)
**Depends on:** nothing
**Deliverable:** fill in the "VT Coverage Gaps" table at the bottom of this doc

Audit rterm-core's VT emulation against alacritty and xterm ctlseqs.

**What to check:**

CSI sequences:
- Cursor: CUU, CUD, CUF, CUB, CHA, CUP, VPA, HPA, CNL, CPL
- Erase: ED, EL, ECH
- Insert/Delete: ICH, DCH, IL, DL
- Scroll: SU, SD
- Tab stops: HTS, TBC, CHT, CBT
- Cursor save/restore: DECSC, DECRC
- Cursor style: DECSCUSR (block/underline/bar, blinking)
- SGR completeness (basic done, sub-params 4:x done)
- DECSET/DECRST: enumerate which modes are handled vs ignored
- Device status: DSR, DA1, DA2
- Margins: DECSTBM (done), DECSLRM
- Repeat: REP

OSC sequences:
- OSC 0/1/2 (window/icon title)
- OSC 4 (color palette)
- OSC 7 (current directory)
- OSC 10/11 (fg/bg color query)
- OSC 52 (clipboard)
- OSC 112 (reset cursor color)
- OSC 133 (shell integration)

Other:
- Charset switching (SCS G0/G1, SI/SO, line drawing)
- C1 controls (8-bit)
- Soft reset (DECSTR)

**Method:**
1. Read `crates/rterm-core/src/terminal.rs` dispatch logic
2. Compare against alacritty (`/home/cyuan/projects/alacritty/alacritty_terminal/`)
3. Compare against xterm ctlseqs docs
4. Fill in the VT Coverage Gaps table

---

### Task 2: Implement P0/P1 VT gaps

**Status:** pending
**Depends on:** Task 1
**Deliverable:** code changes in rterm-core with unit tests per fix

Implement what the audit identifies as P0 (basic usage) and P1
(vim/tmux/htop work correctly). Likely candidates:

- Cursor style (DECSCUSR)
- Device attributes (DA1, DA2)
- Tab stops (HTS, TBC, CHT, CBT)
- DECSC/DECRC with attributes
- Soft reset (DECSTR)
- OSC 0/1/2 title
- Charset switching (SCS, SI/SO, line drawing)
- Missing DECSET/DECRST modes
- DSR responses

Each fix: feed escape bytes → assert terminal state.

---

## Track B: Crate Extraction and Mobile

### Task 3: Extract rterm-transport crate

**Status:** done (2026-04-02)
**Depends on:** nothing
**Deliverable:** `crates/rterm-transport/`, relay depends on it, all tests pass

**Transport trait:**
```rust
#[async_trait]
pub trait Transport: Send + Sync {
    async fn read(&mut self) -> Result<Vec<u8>, TransportError>;
    async fn write(&mut self, data: &[u8]) -> Result<(), TransportError>;
    async fn resize(&mut self, cols: u16, rows: u16) -> Result<(), TransportError>;
    async fn close(&mut self) -> Result<(), TransportError>;
}
```

**What moves from rterm-relay:**
| Source | Destination |
|--------|-------------|
| `relay/src/pty.rs` (PtySpawner, PtyHandle, RealPtySpawner) | `transport/src/pty.rs` (PtyTransport) |
| `relay/src/pty.rs` (FakePtySpawner) | `transport/src/fake.rs` (FakeTransport) |

Pure refactor. No new features.

---

### Task 4: Extract rterm-session crate

**Status:** done (2026-04-02)
**Depends on:** Task 3

**What moves from rterm-relay:**
| Source | Destination |
|--------|-------------|
| `relay/src/managed_session.rs` | `session/src/session.rs` (Session) |
| `relay/src/session_manager.rs` | `session/src/manager.rs` (SessionManager) |
| `relay/src/screen_diff.rs` | `session/src/screen_diff.rs` |
| `relay/src/service.rs` (RunCommand, WaitForText, PressKeys logic) | `session/src/automation.rs` |

Session uses Transport via trait object:
```rust
pub struct Session {
    pub name: String,
    pub terminal: Terminal,
    pub prev_screen: PrevScreen,
    pub transport: Box<dyn Transport>,
}
```

**Deps:** rterm-core, rterm-proto, rterm-transport

---

### Task 5: Extract rterm-service crate

**Status:** pending
**Depends on:** Task 4

**What moves from rterm-relay:**
| Source | Destination |
|--------|-------------|
| `relay/src/service.rs` (TerminalServer, all handler structs) | `service/src/lib.rs` |
| tower::Service HTTP routing impl | `service/src/lib.rs` |

**Key design — TransportFactory trait:**
```rust
pub trait TransportFactory: Send + Sync {
    async fn create(&self, config: &SessionConfig) -> Result<Box<dyn Transport>, Error>;
}
```

relay provides PtyTransportFactory. agent provides SshTransportFactory.

**What stays in rterm-relay (thin launcher):**
- main.rs, wt_server.rs, https_server.rs, config.rs, tls.rs, network.rs

---

### Task 6: Implement SshTransport using russh

**Status:** pending
**Depends on:** Task 3 (Transport trait)
**New deps:** russh, russh-keys

**SshTransport::connect(config) flow:**
1. TCP connect to hostname:port
2. SSH handshake (russh)
3. Authenticate (password or key)
4. Open session channel
5. Request PTY (cols, rows)
6. Start shell
7. Transport impl: read/write channel, resize via window-change

**Config:**
```rust
pub struct SshConfig {
    pub hostname: String,
    pub port: u16,
    pub username: String,
    pub auth: SshAuth,  // Password(String) | Key { private_key, passphrase }
    pub cols: u16,
    pub rows: u16,
}
```

**Tests:** mock SSH server (russh server API) + Docker sshd (#[ignore])

---

### Task 7: Build rterm-agent binary

**Status:** done (2026-04-02)
**Depends on:** Task 5 + Task 6

New `crates/rterm-agent/` binary crate.

**What it does:**
1. gRPC server on `127.0.0.1:0` (OS picks port)
2. Print `PORT=<port>` to stdout (Flutter reads this)
3. SshTransportFactory for sessions
4. Full TerminalService API from rterm-service

**Proto extension:** CreateSessionRequest needs SSH fields
(hostname, port, username, auth_type, credential)

**Cross-compile targets:**
- x86_64-unknown-linux-gnu (dev)
- aarch64-linux-android (Android)
- aarch64-apple-ios (iOS)

**Communication model:**
No FFI. No .so/.dylib. The agent is a standalone binary.
Flutter spawns it as a child process, reads the port from stdout,
then communicates exclusively via gRPC over localhost.

---

### Task 8: Flutter scaffold

**Status:** pending
**Depends on:** Task 7

Minimal Flutter app under `mobile/`:
- Host profile CRUD (save/edit/delete SSH hosts)
- Start/stop rterm-agent binary from Dart
- gRPC client connecting to agent on localhost
- Create session, send text, display plain text output

**Not in scope:** proper terminal rendering, accessory bar, SSH keys

---

### Task 9: Flutter terminal rendering + accessory key bar

**Status:** pending
**Depends on:** Task 8

**Terminal rendering** (CustomPaint or WebView+egui — TBD):
- Monospace cell grid, 256 + truecolor, all Flags attributes
- Cursor, wide chars, resize

**Accessory key bar** (Flutter widget):
- Esc, Tab, Ctrl (sticky), Alt (sticky), |, ~, /, -, arrows
- Sticky: tap to arm, next key includes modifier, auto-disarm
- Double-tap to lock

**Input:**
- Pinch-to-zoom (paint scaling)
- Space-bar cursor mode (long-press + drag = arrows)
- Hardware keyboard auto-hide accessory bar

---

## Completed Phases (historical)

### Phase 1: VT Emulation Core
- [x] Cell type with Flags u16 bitfield (alacritty-style)
- [x] Screen buffer (2D grid, cursor, scroll regions)
- [x] Scrollback buffer
- [x] vte parser integration
- [x] VT100 + VT220 core sequences
- [x] Alternate screen buffer
- [x] SGR sub-parameters (4:2 double underline, 4:3 undercurl, etc.)
- [x] 256 color + truecolor
- [x] Wide character support (WIDE_CHAR + WIDE_CHAR_SPACER)

### Phase 2: Protocol + Transport
- [x] FlatBuffers schema (Cell with u16 flags)
- [x] FlatBuffers codec (encode/decode all types)
- [x] WebTransport, gRPC/H2, gRPC/H3 transports
- [x] Length-prefixed FlatBuffers framing

### Phase 3: egui Terminal Widget
- [x] egui WASM renderer (colors, attributes, cursor)
- [x] Keyboard input, mouse forwarding, selection, scrollback
- [x] Paste, app cursor keys, underline variants

### Phase 4: Session Management
- [x] ManagedSession, SessionManager, screen diffing
- [x] Synchronized output (CSI ?2026)

### Phase 5: Automation API
- [x] rterm-cli (Playwright-style)
- [x] All unary RPCs (Create/Kill/Resize/List/Type/SendKeys/PressKeys/GetSnapshot/WaitForText/RunCommand)
- [x] 20 in-process + 8 Docker E2E tests

---

## VT Coverage Gaps

Audit completed 2026-04-02. Compared rterm-core `terminal.rs` + `buffer.rs` against
alacritty `alacritty_terminal/src/term/mod.rs` (vte 0.15 Handler trait).

### CSI Sequences -- Cursor Movement

| Sequence | Name | rterm Status | Priority | Notes |
|----------|------|--------------|----------|-------|
| CSI n A | CUU (Cursor Up) | done | P0 | |
| CSI n B | CUD (Cursor Down) | done | P0 | |
| CSI n C | CUF (Cursor Forward) | done | P0 | |
| CSI n D | CUB (Cursor Back) | done | P0 | |
| CSI n E | CNL (Cursor Next Line) | missing | P1 | Move down n + CR; alacritty: `move_down_and_cr` |
| CSI n F | CPL (Cursor Prev Line) | missing | P1 | Move up n + CR; alacritty: `move_up_and_cr` |
| CSI n G | CHA (Cursor Char Absolute) | done | P0 | |
| CSI r;c H | CUP (Cursor Position) | done | P0 | Also handles `f` (HVP) |
| CSI n d | VPA (Line Position Abs) | done | P0 | |
| CSI n ` | HPA (Char Position Abs) | missing | P1 | Same as CHA; alacritty: `goto_col` |

### CSI Sequences -- Erase

| Sequence | Name | rterm Status | Priority | Notes |
|----------|------|--------------|----------|-------|
| CSI n J | ED (Erase in Display) | done | P0 | Modes 0,1,2 handled; mode 3 (scrollback) missing |
| CSI 3 J | ED mode 3 (Erase Scrollback) | missing | P2 | alacritty: `ClearMode::Saved` |
| CSI n K | EL (Erase in Line) | done | P0 | |
| CSI n X | ECH (Erase Characters) | missing | P1 | Erase n chars at cursor without moving; alacritty: `erase_chars` |

### CSI Sequences -- Insert/Delete

| Sequence | Name | rterm Status | Priority | Notes |
|----------|------|--------------|----------|-------|
| CSI n @ | ICH (Insert Characters) | done | P0 | |
| CSI n P | DCH (Delete Characters) | done | P0 | |
| CSI n L | IL (Insert Lines) | done | P0 | |
| CSI n M | DL (Delete Lines) | done | P0 | |

### CSI Sequences -- Scroll

| Sequence | Name | rterm Status | Priority | Notes |
|----------|------|--------------|----------|-------|
| CSI n S | SU (Scroll Up) | done | P0 | |
| CSI n T | SD (Scroll Down) | done | P0 | |

### CSI Sequences -- Tab

| Sequence | Name | rterm Status | Priority | Notes |
|----------|------|--------------|----------|-------|
| CSI n I | CHT (Cursor Fwd Tab) | missing | P1 | Move forward n tab stops; alacritty: `move_forward_tabs` |
| CSI n Z | CBT (Cursor Back Tab) | missing | P1 | Move backward n tab stops; alacritty: `move_backward_tabs` |

### CSI Sequences -- SGR (Select Graphic Rendition)

| Sequence | Name | rterm Status | Priority | Notes |
|----------|------|--------------|----------|-------|
| SGR 0 | Reset | done | P0 | |
| SGR 1 | Bold | done | P0 | |
| SGR 2 | Dim | done | P0 | |
| SGR 3 | Italic | done | P0 | |
| SGR 4 | Underline | done | P0 | Sub-params 4:0-5 all handled |
| SGR 5 | Blink (slow) | missing | P2 | Not in alacritty either (no BLINK flag) |
| SGR 6 | Blink (rapid) | missing | P3 | Rarely used |
| SGR 7 | Inverse | done | P0 | |
| SGR 8 | Hidden | done | P0 | |
| SGR 9 | Strikeout | done | P0 | |
| SGR 21 | Double underline | done | P1 | |
| SGR 22-29 | Reset attrs | done | P0 | All individual resets handled |
| SGR 30-37 | FG basic colors | done | P0 | |
| SGR 38 | FG extended (256/RGB) | done | P0 | Both colon and semicolon forms |
| SGR 39 | FG default | done | P0 | |
| SGR 40-47 | BG basic colors | done | P0 | |
| SGR 48 | BG extended (256/RGB) | done | P0 | Both colon and semicolon forms |
| SGR 49 | BG default | done | P0 | |
| SGR 53 | Overline | missing | P3 | alacritty has no overline either |
| SGR 58 | Underline color | missing | P2 | alacritty: `Attr::UnderlineColor`; rterm has no underline_color field |
| SGR 59 | Reset underline color | missing | P2 | Paired with SGR 58 |
| SGR 90-97 | FG bright colors | done | P0 | |
| SGR 100-107 | BG bright colors | done | P0 | |

### CSI Sequences -- DECSET/DECRST (Private Modes, CSI ? n h/l)

| Mode | Name | rterm Status | Priority | Notes |
|------|------|--------------|----------|-------|
| 1 | DECCKM (App Cursor Keys) | done | P0 | |
| 3 | DECCOLM (132 Column Mode) | missing | P3 | alacritty handles it (deccolm); rarely used |
| 4 | IRM (Insert Mode) | done | P1 | rterm maps ?4, should be CSI 4 h/l (non-private) |
| 6 | DECOM (Origin Mode) | done | P1 | |
| 7 | DECAWM (Auto-Wrap) | done | P0 | |
| 12 | Blinking Cursor (att610) | missing | P2 | alacritty: `BlinkingCursor` |
| 25 | DECTCEM (Show Cursor) | done | P0 | |
| 1000 | Mouse Click Tracking | done | P0 | |
| 1002 | Mouse Cell Motion | done | P1 | |
| 1003 | Mouse All Motion | done | P1 | |
| 1004 | Focus In/Out Events | missing | P1 | alacritty: `ReportFocusInOut`; needed by vim/tmux |
| 1005 | UTF8 Mouse | missing | P2 | alacritty: `Utf8Mouse` |
| 1006 | SGR Mouse | done | P0 | |
| 1007 | Alternate Scroll | missing | P2 | alacritty: `AlternateScroll` |
| 1042 | Urgency Hints | missing | P3 | alacritty handles it |
| 1047 | Alternate Screen (no save) | done | P1 | |
| 1049 | Alternate Screen + Save/Restore | done | P0 | |
| 2004 | Bracketed Paste | done | P0 | |
| 2026 | Synchronized Output | done | P1 | |

### CSI Sequences -- Standard Modes (CSI n h/l, no ?)

| Mode | Name | rterm Status | Priority | Notes |
|------|------|--------------|----------|-------|
| 4 | IRM (Insert Mode) | partial | P1 | rterm handles via ?4 DECSET; should also handle CSI 4 h/l |
| 20 | LNM (Line Feed/New Line) | missing | P2 | alacritty: `LineFeedNewLine` |

### CSI Sequences -- Device/Report

| Sequence | Name | rterm Status | Priority | Notes |
|----------|------|--------------|----------|-------|
| CSI 5 n | DSR (Device Status) | missing | P1 | Should respond ESC[0n (terminal OK); alacritty handles |
| CSI 6 n | CPR (Cursor Position Report) | done | P0 | |
| CSI c | DA1 (Primary Device Attrs) | missing | P0 | Apps check this; alacritty responds ESC[?6c |
| CSI > c | DA2 (Secondary Device Attrs) | missing | P1 | alacritty responds ESC[>0;VERSION;1c |

### CSI Sequences -- Cursor Save/Restore/Style

| Sequence | Name | rterm Status | Priority | Notes |
|----------|------|--------------|----------|-------|
| CSI s | SCOSC (Save Cursor Pos) | missing | P2 | Same as DECSC but via CSI; some apps use this |
| CSI u | SCORC (Restore Cursor Pos) | missing | P2 | Same as DECRC but via CSI |
| CSI SP q | DECSCUSR (Cursor Style) | done | P1 | |
| CSI ! p | DECSTR (Soft Reset) | done | P1 | |

### CSI Sequences -- Margins

| Sequence | Name | rterm Status | Priority | Notes |
|----------|------|--------------|----------|-------|
| CSI t;b r | DECSTBM (Scroll Region) | done | P0 | |
| CSI l;r s | DECSLRM (L/R Margins) | missing | P3 | Conflicts with SCOSC; very rarely used |

### CSI Sequences -- Other

| Sequence | Name | rterm Status | Priority | Notes |
|----------|------|--------------|----------|-------|
| CSI n b | REP (Repeat Character) | missing | P2 | Repeat last printed char n times |
| CSI n t | Window Ops (XTWINOPS) | missing | P2 | alacritty handles modes 14 (pixels), 18 (chars) |
| CSI ? n $ y | DECRPM (Report Private Mode) | missing | P3 | alacritty: `report_private_mode` |
| CSI n $ y | DECRPM (Report Mode) | missing | P3 | alacritty: `report_mode` |
| CSI n i | MC (Media Copy/Print) | missing | P3 | Rarely used |

### ESC Sequences

| Sequence | Name | rterm Status | Priority | Notes |
|----------|------|--------------|----------|-------|
| ESC 7 | DECSC (Save Cursor) | done | P0 | |
| ESC 8 | DECRC (Restore Cursor) | done | P0 | |
| ESC D | IND (Index/Line Feed) | done | P0 | |
| ESC M | RI (Reverse Index) | done | P0 | |
| ESC E | NEL (Next Line) | missing | P1 | LF + CR; alacritty: `newline` |
| ESC c | RIS (Full Reset) | done | P0 | |
| ESC H | HTS (Set Tab Stop) | missing | P1 | Set tab stop at cursor col; alacritty: `set_horizontal_tabstop` |
| ESC # 8 | DECALN (Alignment Test) | missing | P2 | Fill screen with 'E'; alacritty: `decaln` |
| ESC ( B/0 | SCS G0 (Charset) | missing | P1 | Line drawing chars (G0); alacritty: `configure_charset` |
| ESC ) B/0 | SCS G1 (Charset) | missing | P1 | Line drawing chars (G1); alacritty: `configure_charset` |
| ESC = | DECKPAM (Keypad App Mode) | missing | P1 | alacritty: `set_keypad_application_mode` |
| ESC > | DECKPNM (Keypad Num Mode) | missing | P1 | alacritty: `unset_keypad_application_mode` |

### OSC Sequences

| Sequence | Name | rterm Status | Priority | Notes |
|----------|------|--------------|----------|-------|
| OSC 0 | Set Window Title | missing | P0 | alacritty: `set_title`; many apps set this |
| OSC 1 | Set Icon Name | missing | P3 | Usually handled same as OSC 0 |
| OSC 2 | Set Window Title | missing | P0 | Same handler as OSC 0 in most terminals |
| OSC 4 | Set Color Palette | missing | P2 | alacritty: `set_color` |
| OSC 7 | Set CWD (URI) | missing | P2 | Shell integration; not in alacritty handler directly |
| OSC 8 | Hyperlinks | missing | P2 | alacritty: `set_hyperlink` |
| OSC 10 | Query/Set FG Color | missing | P2 | alacritty: `dynamic_color_sequence` |
| OSC 11 | Query/Set BG Color | missing | P2 | alacritty: `dynamic_color_sequence` |
| OSC 12 | Query/Set Cursor Color | missing | P2 | alacritty: `dynamic_color_sequence` |
| OSC 22 | Set Mouse Cursor Icon | missing | P3 | alacritty: `set_mouse_cursor_icon` |
| OSC 52 | Clipboard Access | missing | P2 | alacritty: `clipboard_store`/`clipboard_load` |
| OSC 104 | Reset Color | missing | P2 | alacritty: `reset_color` |
| OSC 112 | Reset Cursor Color | missing | P2 | alacritty: `reset_color` |
| OSC 133 | Shell Integration (FinalTerm) | missing | P3 | Prompt detection markers |

### Control Characters

| Byte | Name | rterm Status | Priority | Notes |
|------|------|--------------|----------|-------|
| 0x07 | BEL (Bell) | missing | P1 | alacritty emits Event::Bell; rterm ignores |
| 0x08 | BS (Backspace) | done | P0 | |
| 0x09 | HT (Horizontal Tab) | done | P0 | Hardcoded 8-col stops; no custom tab stop support |
| 0x0A | LF (Line Feed) | done | P0 | |
| 0x0B | VT (Vertical Tab) | done | P0 | Treated as LF |
| 0x0C | FF (Form Feed) | done | P0 | Treated as LF |
| 0x0D | CR (Carriage Return) | done | P0 | |
| 0x0E | SO (Shift Out / G1) | missing | P1 | Activate G1 charset; alacritty: `set_active_charset` |
| 0x0F | SI (Shift In / G0) | missing | P1 | Activate G0 charset; alacritty: `set_active_charset` |
| 0x1A | SUB (Substitute) | missing | P3 | alacritty: `substitute` (unimplemented there too) |
| 0x1B | ESC | done | P0 | Via vte parser |

### DCS Sequences

| Sequence | Name | rterm Status | Priority | Notes |
|----------|------|--------------|----------|-------|
| DECRQSS | Request Selection or Setting | missing | P3 | Not in alacritty either |
| XTGETTCAP | xterm Get Termcap | missing | P3 | Not in alacritty either |

### Kitty Keyboard Protocol

| Sequence | Name | rterm Status | Priority | Notes |
|----------|------|--------------|----------|-------|
| CSI ? u | Report Keyboard Mode | missing | P2 | alacritty: `report_keyboard_mode` |
| CSI > n u | Push Keyboard Mode | missing | P2 | alacritty: `push_keyboard_mode` |
| CSI < n u | Pop Keyboard Mode | missing | P2 | alacritty: `pop_keyboard_modes` |
| CSI = n ; m u | Set Keyboard Mode | missing | P2 | alacritty: `set_keyboard_mode` |

### Other Features (Not Sequence-Specific)

| Feature | Name | rterm Status | Priority | Notes |
|---------|------|--------------|----------|-------|
| Tab stops | Custom tab stops (HTS/TBC) | missing | P1 | rterm uses hardcoded 8-col; no TabStops array |
| Charset mapping | Line drawing (SCS) | missing | P1 | G0/G1 charset tables with `map()` needed |
| Zero-width chars | Combining characters | missing | P2 | alacritty: `push_zerowidth` on previous cell |
| Saved cursor attrs | DECSC saves pen/charset | partial | P1 | rterm only saves row/col; should save pen, charset, origin |
| Insert mode on print | IRM shifts chars on input | missing | P1 | alacritty: shifts cells right in `input()` |
| Title stack | Push/Pop Title (CSI 22/23 t) | missing | P2 | alacritty: `push_title`/`pop_title` |
| LNM mode | LF implies CR | missing | P2 | alacritty: `LINE_FEED_NEW_LINE` |
| Erase uses BG color | ED/EL fill with pen BG | missing | P2 | alacritty: erased cells get cursor BG color |

### Summary

**Total sequences/features audited:** 95

| Status | Count |
|--------|-------|
| done | 42 |
| partial | 2 |
| missing | 51 |

**P0 missing (blocks basic shell usage):** DA1, OSC 0/2 (window title)
**P1 missing (blocks vim/tmux/htop):** CNL, CPL, ECH, HPA, CHT, CBT, DSR 5n, DA2, NEL, HTS, SCS G0/G1, SO/SI, DECKPAM/DECKPNM, BEL, tab stop management, saved cursor attrs, insert mode on input, focus in/out (1004)
