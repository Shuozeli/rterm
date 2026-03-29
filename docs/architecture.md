<!-- agent-updated: 2026-03-29T23:20:00Z -->
# rterm Architecture

## Overview

rterm is a minimal, correct terminal emulator built in Rust. The relay server runs VT emulation server-side and sends typed screen updates to clients over FlatBuffers. egui compiled to WASM is the universal renderer -- it receives pre-parsed cell data (characters, colors, attributes) and paints them directly. The WASM browser client connects to a relay server over WebTransport using length-prefixed FlatBuffers. Native clients connect via gRPC over HTTP/3 (QUIC). On desktop, a native WebView shell (planned) loads the WASM bundle and provides a local PTY. The WASM code has zero platform-specific branches.

## Design Principles

- **Alacritty philosophy**: No tabs, no panes, no splits. One terminal per window.
- **WASM-first**: egui WASM is the single rendering path for all platforms.
- **Server-side VT emulation**: The relay runs the VT emulator and sends typed screen diffs. Clients are thin renderers.
- **Logical correctness first**: VT emulation correctness before GUI polish.
- **HTTP/3 transport**: QUIC everywhere. No HTTP/2, no WebSocket, no fallbacks.
- **Dogfooding**: Uses flatbuffers-rs and pure-grpc-rs. Bugs found are fixed upstream.

## Crate Structure

```
rterm/
  crates/
    rterm-proto/    -- FlatBuffers protocol: typed screen updates (Cell, CellRange, ScreenUpdate, ScreenSnapshot)
    rterm-core/     -- VT100/VT220 emulation: vte parser, screen buffer, cell types
    rterm-gui/      -- egui terminal widget: color palette, input encoding, selection, scrollback
    rterm-shell/    -- Native WebView wrapper + local PTY (stub, not yet implemented)
    rterm-relay/    -- HTTP/3 + WebTransport relay server (PTY spawning, VT emulation, screen diffing)
    rterm-wasm/     -- WASM browser thin renderer (excluded from workspace, built with trunk)
```

rterm-wasm is excluded from the default Cargo workspace because it targets wasm32 only. Build it with: `cd crates/rterm-wasm && RUSTFLAGS="--cfg web_sys_unstable_apis" trunk build`

## Related Repositories (bug fixes in scope)

| Repo | Role | Fix Scope |
|---|---|---|
| flatbuffers-rs | FlatBuffers compiler + runtime | Bugs found via rterm-proto |
| pure-grpc-rs | gRPC framework + FlatBuffers codec | Bugs found via rterm transport; HTTP/3 support |

## System Architecture

```
+-----------------------------------------------+
|  WASM Bundle (identical on every platform)     |
|                                                |
|  egui (WebGL/WebGPU canvas)                    |
|  +------------------------------------------+  |
|  |  Terminal Grid Widget (rterm-gui)        |  |
|  |  - Monospace cell rendering              |  |
|  |  - Full 256 + true color                 |  |
|  |  - Cursor rendering (block)              |  |
|  |  - Selection highlighting                |  |
|  |  - Scrollback view with mouse wheel      |  |
|  +------------------------------------------+  |
|  |  Input Handler (rterm-gui)               |  |
|  |  - Keyboard -> VT sequences              |  |
|  |  - Ctrl+key combinations                 |  |
|  |  - Application cursor keys mode          |  |
|  +------------------------------------------+  |
|  |  Screen State (rterm-wasm)               |  |
|  |  - Applies ScreenUpdate cell changes     |  |
|  |  - Applies ScreenSnapshot full state     |  |
|  |  - Scrollback data management            |  |
|  |  - No VT parsing on client               |  |
|  +------------------------------------------+  |
|  |  Transport Client (rterm-wasm)           |  |
|  |  - WebTransport API (web-sys)            |  |
|  |  - Length-prefixed FlatBuffers protocol  |  |
|  |  - Sends KeyInput, PasteInput, Resize,   |  |
|  |    MouseEvent                            |  |
|  |  - Receives ScreenUpdate, ScreenSnapshot,|  |
|  |    ScrollbackData, Exit, Error, Bell     |  |
|  +------------------------------------------+  |
+-----------------------------------------------+
```

## Server-Side VT Emulation

The relay server runs the VT emulator, not the client. This is the key architectural decision:

```
+-----------------------------------------------+
|  rterm-relay (server)                          |
|                                                |
|  +------------------------------------------+  |
|  |  session::run_session (shared logic)     |  |
|  |  - Read initial Resize from client       |  |
|  |  - Spawn PTY via PtySpawner trait        |  |
|  |  - Send initial ScreenSnapshot           |  |
|  |  - Forward client input to PTY           |  |
|  |  - Feed PTY stdout through Terminal      |  |
|  |  - Diff screen state via PrevScreen      |  |
|  |  - Send ScreenUpdate with changed cells  |  |
|  |  - Send Exit when PTY closes             |  |
|  +------------------------------------------+  |
|  |  wt_handler (WebTransport adapter)       |  |
|  |  - Bridges bidi stream <-> channels      |  |
|  |  - Delegates to session::run_session     |  |
|  +------------------------------------------+  |
|  |  service (gRPC adapter)                  |  |
|  |  - Bridges Streaming <-> channels        |  |
|  |  - Delegates to session::run_session     |  |
|  +------------------------------------------+  |
+-----------------------------------------------+
```

Both transport adapters are thin: they only translate between their protocol (WebTransport bidi stream or gRPC Streaming) and mpsc channels, then call `session::run_session()` for all business logic.

## Protocol

### Serialization: FlatBuffers

All messages are FlatBuffers-encoded. Defined once in rterm-proto, used by all crates.

```
Client -> Server:
  ClientMessage {
    KeyInput { data: [ubyte] }          -- keyboard input bytes (VT sequences)
    PasteInput { text: string }         -- pasted text
    Resize { cols: u16, rows: u16 }     -- terminal resize
    MouseEvent { row, col, button,      -- mouse event (currently ignored)
                 modifiers, kind }
  }

Server -> Client:
  ServerMessage {
    ScreenUpdate {                      -- incremental screen diff
      changes: [CellRange]              -- changed cell ranges
      cursor: CursorState               -- cursor position + visibility
      cols: u16, rows: u16              -- screen dimensions
      title: string (optional)          -- window title
    }
    ScreenSnapshot {                    -- full screen state
      rows: [CellRange]                -- all cells
      cursor: CursorState
      cols: u16, num_rows: u16
      title: string (optional)
      scrollback_len: u32
    }
    ScrollbackData {                    -- scrollback query response
      lines: [CellRange]
      offset: u32, total: u32
    }
    Exit { code: i32 }                 -- shell exited
    Error { message: string }          -- error
    Bell {}                            -- terminal bell
  }

CellRange {
  row: u16, col_start: u16
  cells: [Cell]                        -- contiguous changed cells
}

Cell {
  ch: u32                              -- Unicode codepoint
  fg: u32                              -- packed color (RGB, indexed, or default)
  bg: u32                              -- packed color
  attrs: u8                            -- bitflags (bold, italic, underline, etc.)
}
```

Color packing: `0x00RRGGBB` for RGB, `0xFF0000II` for indexed, `0xFFFFFFFF` for default.

Attribute bitflags: bold(1), italic(2), underline(4), strikethrough(8), reverse(16), dim(32), hidden(64).

### Transport: Dual Path

Two transport paths serve different client types:

**1. WASM Browser Client (WebTransport + length-prefixed FlatBuffers)**

```
+-----------------------------------+
|  FlatBuffers Messages             |
|  (rterm-proto schema)             |
+-----------------------------------+
|  Length-prefixed framing           |
|  (4-byte BE length + payload)     |
+-----------------------------------+
|  WebTransport bidi stream         |
+-----------------------------------+
|  HTTP/3 (QUIC/UDP)               |
|  Browser: WebTransport API        |
+-----------------------------------+
```

The WASM client (rterm-wasm) opens a WebTransport connection to the relay, creates a bidi stream, and exchanges length-prefixed FlatBuffers messages directly. This is simpler than gRPC framing since it runs on a raw bidi stream.

**2. Native gRPC Client (gRPC/H3 via pure-grpc-rs)**

```
+-----------------------------------+
|  FlatBuffers Messages             |
|  (rterm-proto schema)             |
+-----------------------------------+
|  gRPC framing                     |
|  (length-prefixed messages)       |
+-----------------------------------+
|  HTTP/3                           |
|  (QUIC/UDP via quinn)             |
+-----------------------------------+
```

Native clients (e.g., the rterm-gui demo) use standard gRPC bidi streaming over HTTP/3 via pure-grpc-rs with the FlatBuffers codec.

### gRPC Service

```
service TerminalService {
  rpc Session (stream ClientMessage) returns (stream ServerMessage);
}
```

One bidi streaming RPC per terminal session. Used by native gRPC clients. The server runs VT emulation and sends typed screen updates -- clients never see raw escape sequences.

### Connection Flow

#### WASM Browser Client

```
1. Browser loads WASM bundle from rterm-relay HTTPS server (TCP:4433)
2. Server injects cert hash into HTML as window.__RTERM_CERT_HASH__
3. WASM reads cert hash and connects via WebTransport (QUIC:4433)
   using serverCertificateHashes for self-signed cert trust
4. Client opens a bidi stream
5. Client sends length-prefixed Resize(cols, rows) as first message
6. Server spawns PTY with that size, creates VT emulator
7. Server sends ScreenSnapshot (full initial screen state)
8. Bidirectional streaming:
   - Client sends KeyInput/PasteInput for keyboard/paste input
   - Client sends Resize on window resize
   - Server feeds PTY output through VT emulator
   - Server diffs screen state and sends ScreenUpdate (changed cells only)
   - Synchronized output (CSI ?2026) suppresses updates during batches
9. Server sends Exit(code) when shell dies
10. Stream closes, connection closes
```

#### Native gRPC Client

```
1. Client connects via HTTP/3 (QUIC handshake)
2. Client opens gRPC bidi stream (TerminalService.Session)
3. Client sends Resize(cols, rows) as first message
4. Server spawns PTY, creates VT emulator
5. Server sends ScreenSnapshot (full initial screen state)
6. Bidirectional streaming via gRPC:
   - Client sends KeyInput/PasteInput/Resize/MouseEvent
   - Server sends ScreenUpdate/ScreenSnapshot/ScrollbackData/Bell
7. Server sends Exit(code) when shell dies
8. gRPC stream closes, QUIC connection closes
```

## Composable Server Architecture

### Trait Boundaries

- **PtySpawner trait**: Abstracts PTY creation. `RealPtySpawner` uses portable-pty; `FakePtySpawner` uses in-memory channels for testing.
- **PtyHandle**: Data bag with three channels (`stdin_tx`, `stdout_rx`, `resize_tx`). No methods.
- **Channel-based transport**: `session::run_session` accepts `mpsc::Receiver<ClientMsg>` and `mpsc::Sender<ServerMsg>` -- no trait objects for transport.

### Screen Diffing

- **PrevScreen**: Tracks previous screen state (packed cells). `diff()` compares current ScreenBuffer against previous state and returns `ScreenUpdateData` with only changed cell ranges.
- **snapshot()**: Creates a full `ScreenSnapshotData` from the current buffer state. Used for initial connection and resize.
- Cell data is packed into CellRange runs for efficient transmission.

## Platform Architecture

### Desktop (Linux/macOS/Windows)

```
rterm-shell (native binary) [NOT YET IMPLEMENTED]
  |
  +-- Opens WebView (webkit2gtk / WKWebView / WebView2)
  |     Loads egui WASM bundle from embedded assets
  |
  +-- Spawns PTY via portable-pty (local shell: bash/zsh)
  |
  +-- Runs localhost gRPC/HTTP/3 server (TerminalService)
        WASM connects via WebTransport -> gRPC
```

Currently, a native egui demo exists (rterm-gui example) that connects to rterm-relay via gRPC/H3 without a WebView.

### Browser

```
rterm-relay serves the WASM bundle over HTTPS (TCP:4433, hyper)
  |
  +-- egui WASM runs in browser tab
  |
  +-- Connects to rterm-relay via WebTransport (QUIC:4433)
  |     Uses length-prefixed FlatBuffers on bidi stream
  |
  +-- Cert hash auto-injected into HTML by relay server
  |
  +-- Server runs VT emulator, sends typed screen updates
```

### Mobile (Android/iOS)

```
rterm-shell (native app with system WebView) [NOT YET IMPLEMENTED]
  |
  +-- Opens WebView (System WebView / WKWebView)
  |     Loads egui WASM bundle from app assets
  |
  +-- Connects to remote rterm-relay via WebTransport
       (no local PTY on mobile)
```

## Data Flow

```
Keyboard/Mouse (egui event)
  -> Input Handler (encode to VT sequences)
  -> Transport send (FlatBuffers ClientMessage::KeyInput/PasteInput)
  -> WebTransport bidi stream (QUIC/UDP)
  -> PTY stdin
  -> Shell process
  -> PTY stdout
  -> Terminal.feed() (server-side VT emulation)
  -> Screen buffer mutations (cursor, colors, text, scroll)
  -> Synchronized output check (skip diff during CSI ?2026 h batch)
  -> PrevScreen.diff() (compare against previous state)
  -> ScreenUpdate with changed CellRanges
  -> FlatBuffers ServerMessage::ScreenUpdate
  -> WebTransport bidi stream (QUIC/UDP)
  -> Client applies cell changes to local screen state
  -> egui render loop paints cells
  -> Monospace cell grid rendered to WebGL canvas
```

## Font Rendering Pipeline

Currently using egui's built-in monospace font rendering. The planned custom pipeline:

```
rustybuzz (text shaping, ligatures, kerning)
  -> fontdue (glyph rasterization to bitmaps)
  -> Texture atlas (CPU-side, packed glyph bitmaps)
  -> WebGL/WebGPU texture (uploaded to GPU)
  -> egui Mesh quads with UV coordinates (render from atlas)
```

Bundled font (planned): JetBrains Mono.
TODO: Custom font loading / system font discovery.

## Terminal Standard

xterm-compatible, implemented in priority order:

| Priority | Standard | What | Status |
|---|---|---|---|
| P0 | VT100 + VT220 core | Cursor, SGR 8-color, scroll regions, insert/delete, line drawing | Done |
| P1 | xterm essentials | Alternate screen, 256/true color, cursor shapes, window title | Mostly done (cursor shapes and window title TODO) |
| P2 | Interaction | SGR mouse reporting, bracketed paste, focus events | Not started |
| P3 | Modern baseline | Synchronized output, OSC 52 clipboard, OSC 8 hyperlinks, colored underlines | Sync output done, rest TODO |
| P4 | Kitty keyboard | Progressive enhancement key encoding | Not started |
| P5 | Graphics | Sixel / Kitty graphics protocol | Not started |

## Key Dependencies

| Crate | Purpose |
|---|---|
| flatbuffers-rs | FlatBuffers compiler + runtime (Shuozeli) |
| pure-grpc-rs | gRPC framework with FlatBuffers codec and H3 transport (Shuozeli) |
| quinn | QUIC/HTTP/3 implementation (rterm-relay) |
| h3 / h3-quinn / h3-webtransport | HTTP/3 and WebTransport server (rterm-relay) |
| web-sys | WebTransport API bindings (rterm-wasm) |
| vte | VT escape sequence parser |
| egui + eframe | GUI framework (compiles to WASM) |
| wasm-bindgen | WASM interop (rterm-wasm) |
| portable-pty | Cross-platform PTY (rterm-relay) |
| hyper | HTTPS page server (rterm-relay) |
| rustls / tokio-rustls | TLS for HTTPS and QUIC |
| rcgen | Self-signed certificate generation |

Planned (not yet in use): rustybuzz (text shaping), fontdue (glyph rasterization), unicode-width (character width), wry (WebView for rterm-shell).

## Configuration

| Platform | Source | Storage |
|---|---|---|
| Desktop | TOML file | ~/.config/rterm/config.toml |
| Browser | localStorage | Browser localStorage |
| Mobile | localStorage | App WebView localStorage |

All platforms have compiled-in defaults. Config is optional.
