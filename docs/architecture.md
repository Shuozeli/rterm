# rterm Architecture

## Overview

rterm is a minimal, correct terminal emulator built in Rust. egui compiled to WASM is the universal renderer. gRPC over HTTP/3 (QUIC) is the only transport. On desktop, a native WebView shell loads the WASM bundle and provides a local PTY behind a gRPC server. In the browser, the same WASM connects to a remote relay. The WASM code has zero platform-specific branches.

## Design Principles

- **Alacritty philosophy**: No tabs, no panes, no splits. One terminal per window.
- **WASM-first**: egui WASM is the single rendering path for all platforms.
- **Logical correctness first**: VT emulation correctness before GUI polish.
- **HTTP/3 only**: gRPC over QUIC everywhere. No HTTP/2, no WebSocket, no fallbacks.
- **Dogfooding**: Uses flatbuffers-rs and pure-grpc-rs. Bugs found are fixed upstream.

## Crate Structure

```
rterm/
  crates/
    rterm-proto/    -- FlatBuffers schema, generated code, transport trait
    rterm-core/     -- VT emulation, screen buffer, cell types
    rterm-gui/      -- egui terminal widget, glyph atlas, input encoding (WASM)
    rterm-shell/    -- Native WebView wrapper + local PTY + gRPC/HTTP/3 server
    rterm-relay/    -- Standalone remote gRPC/HTTP/3 server
```

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
|  |  Terminal Grid Widget                    |  |
|  |  - Custom glyph atlas                    |  |
|  |    (rustybuzz + fontdue -> GPU texture)   |  |
|  |  - Cursor rendering                      |  |
|  |  - Selection highlighting                |  |
|  |  - Scrollback view                       |  |
|  +------------------------------------------+  |
|  |  Input Handler                           |  |
|  |  - Keyboard -> VT sequences              |  |
|  |  - Mouse -> SGR mouse reports            |  |
|  +------------------------------------------+  |
|  |  rterm-core (compiled to WASM)           |  |
|  |  - vte parser                            |  |
|  |  - Screen buffer (cells + attributes)    |  |
|  |  - Scrollback ring buffer                |  |
|  +------------------------------------------+  |
|  |  Transport Client (rterm-proto)          |  |
|  |  - gRPC bidi stream over HTTP/3          |  |
|  |  - Native: quinn (QUIC)                  |  |
|  |  - Browser WASM: WebTransport API        |  |
|  |  - Sends ClientMessage (DataIn, Resize)  |  |
|  |  - Receives ServerMessage (DataOut, Exit)|  |
|  +------------------------------------------+  |
+-----------------------------------------------+
```

## Protocol

### Serialization: FlatBuffers

All messages are FlatBuffers-encoded. Defined once in rterm-proto, used by all crates.

```
Client -> Server:
  ClientMessage {
    DataIn { payload: [ubyte] }    -- keyboard/mouse input bytes
    Resize { cols: u16, rows: u16 } -- terminal resize
  }

Server -> Client:
  ServerMessage {
    DataOut { payload: [ubyte] }   -- PTY output bytes
    Exit { code: i32 }            -- shell exited
    Error { message: string }     -- error
  }
```

### Transport: gRPC over HTTP/3

One transport. No alternatives. No fallbacks.

```
+-----------------------------------+
|  FlatBuffers Messages             |
|  (rterm-proto schema)             |
+-----------------------------------+
|  gRPC framing                     |
|  (length-prefixed messages)       |
+-----------------------------------+
|  HTTP/3                           |
|  (QUIC/UDP)                       |
+-----------------------------------+
|  Native: quinn/quiche             |
|  Browser WASM: WebTransport API   |
+-----------------------------------+
```

Follows gRPC A69 proposal for gRPC over HTTP/3.

### gRPC Service

```
service TerminalService {
  rpc Session (stream ClientMessage) returns (stream ServerMessage);
}
```

One bidi streaming RPC per terminal session.

### Transport Trait

```rust
trait TerminalTransport {
    async fn send(&self, msg: ClientMessage) -> Result<()>;
    async fn recv(&self) -> Result<Option<ServerMessage>>;
    async fn close(&self) -> Result<()>;
}

// Implementations:
// QuicTransport       -- native, uses quinn for QUIC/HTTP/3
// WebTransportClient  -- browser WASM, uses WebTransport API via web-sys
```

### Connection Flow

```
1. Client connects via HTTP/3 (QUIC handshake)
2. Client opens gRPC bidi stream (TerminalService.Session)
3. Client sends Resize(cols, rows) as first message
4. Server spawns PTY with that size
5. Bidirectional streaming:
   - Client sends DataIn(bytes) for keyboard input
   - Server sends DataOut(bytes) for PTY output
   - Client sends Resize on window resize
6. Server sends Exit(code) when shell dies
7. gRPC stream closes, QUIC connection closes
```

## Platform Architecture

### Desktop (Linux/macOS/Windows)

```
rterm-shell (native binary)
  |
  +-- Opens WebView (webkit2gtk / WKWebView / WebView2)
  |     Loads egui WASM bundle from embedded assets
  |
  +-- Spawns PTY via portable-pty (local shell: bash/zsh)
  |
  +-- Runs localhost gRPC/HTTP/3 server (TerminalService)
        WASM connects via WebTransport -> gRPC
```

### Browser

```
Web server serves the WASM bundle (static files)
  |
  +-- egui WASM runs in browser tab
  |
  +-- Connects to remote rterm-relay via WebTransport -> gRPC/HTTP/3
```

### Mobile (Android/iOS)

```
rterm-shell (native app with system WebView)
  |
  +-- Opens WebView (System WebView / WKWebView)
  |     Loads egui WASM bundle from app assets
  |
  +-- Connects to remote rterm-relay via WebTransport -> gRPC/HTTP/3
       (no local PTY on mobile)
```

## Data Flow

```
Keyboard/Mouse (egui event)
  -> Input Handler (encode to VT sequences)
  -> gRPC send (FlatBuffers ClientMessage::DataIn)
  -> HTTP/3 (QUIC/UDP)
  -> PTY stdin
  -> Shell process
  -> PTY stdout
  -> HTTP/3 (QUIC/UDP)
  -> gRPC recv (FlatBuffers ServerMessage::DataOut)
  -> vte parser (tokenize escape sequences)
  -> Screen buffer mutations (cursor, colors, text, scroll)
  -> egui render loop reads cells
  -> Glyph atlas renders styled characters to WebGL canvas
```

## Font Rendering Pipeline

```
rustybuzz (text shaping, ligatures, kerning)
  -> fontdue (glyph rasterization to bitmaps)
  -> Texture atlas (CPU-side, packed glyph bitmaps)
  -> WebGL/WebGPU texture (uploaded to GPU)
  -> egui Mesh quads with UV coordinates (render from atlas)
```

Bundled font: JetBrains Mono.
TODO: Custom font loading / system font discovery.

## Terminal Standard

xterm-compatible, implemented in priority order:

| Priority | Standard | What |
|---|---|---|
| P0 | VT100 + VT220 core | Cursor, SGR 8-color, scroll regions, insert/delete, line drawing |
| P1 | xterm essentials | Alternate screen, 256/true color, cursor shapes, window title |
| P2 | Interaction | SGR mouse reporting, bracketed paste, focus events |
| P3 | Modern baseline | Synchronized output, OSC 52 clipboard, OSC 8 hyperlinks, colored underlines |
| P4 | Kitty keyboard | Progressive enhancement key encoding |
| P5 | Graphics | Sixel / Kitty graphics protocol |

## Key Dependencies

| Crate | Purpose |
|---|---|
| flatbuffers-rs | FlatBuffers compiler + runtime (Shuozeli) |
| pure-grpc-rs | gRPC framework with FlatBuffers codec (Shuozeli) |
| quinn | QUIC/HTTP/3 implementation (native) |
| web-sys | WebTransport API bindings (WASM) |
| vte | VT escape sequence parser |
| egui + eframe | GUI framework (compiles to WASM) |
| rustybuzz | Text shaping (pure Rust HarfBuzz, WASM-compatible) |
| fontdue | Glyph rasterization (pure Rust, WASM-compatible) |
| unicode-width | Character width calculation |
| portable-pty | Cross-platform PTY (rterm-shell, native only) |
| wry | WebView abstraction (rterm-shell, native only) |

## Configuration

| Platform | Source | Storage |
|---|---|---|
| Desktop | TOML file | ~/.config/rterm/config.toml |
| Browser | localStorage | Browser localStorage |
| Mobile | localStorage | App WebView localStorage |

All platforms have compiled-in defaults. Config is optional.
