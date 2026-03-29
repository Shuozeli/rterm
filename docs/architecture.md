# rterm Architecture

## Overview

rterm is a minimal, correct terminal emulator built in Rust. egui compiled to WASM is the universal renderer. The WASM browser client connects to a relay server over WebTransport using length-prefixed FlatBuffers. Native clients connect via gRPC over HTTP/3 (QUIC). On desktop, a native WebView shell (planned) loads the WASM bundle and provides a local PTY. In the browser, the same WASM connects to a remote relay. The WASM code has zero platform-specific branches.

## Design Principles

- **Alacritty philosophy**: No tabs, no panes, no splits. One terminal per window.
- **WASM-first**: egui WASM is the single rendering path for all platforms.
- **Logical correctness first**: VT emulation correctness before GUI polish.
- **HTTP/3 transport**: QUIC everywhere. No HTTP/2, no WebSocket, no fallbacks.
- **Dogfooding**: Uses flatbuffers-rs and pure-grpc-rs. Bugs found are fixed upstream.

## Crate Structure

```
rterm/
  crates/
    rterm-proto/    -- FlatBuffers schema, generated code, message types
    rterm-core/     -- VT emulation, screen buffer, cell types
    rterm-gui/      -- egui terminal widget, color palette, input encoding
    rterm-shell/    -- Native WebView wrapper + local PTY (stub, not yet implemented)
    rterm-relay/    -- WebTransport relay server + HTTPS page server
    rterm-wasm/     -- WASM browser client (excluded from workspace, built with trunk)
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
|  |  rterm-core (compiled to WASM)           |  |
|  |  - vte parser (persistent state)         |  |
|  |  - Screen buffer (cells + attributes)    |  |
|  |  - Scrollback ring buffer                |  |
|  |  - Synchronized output mode              |  |
|  +------------------------------------------+  |
|  |  Transport Client (rterm-wasm)           |  |
|  |  - WebTransport API (web-sys)            |  |
|  |  - Length-prefixed FlatBuffers protocol  |  |
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

One bidi streaming RPC per terminal session. Used by native gRPC clients.

### Connection Flow

#### WASM Browser Client

```
1. Browser loads WASM bundle from rterm-relay HTTPS server (TCP:4433)
2. Server injects cert hash into HTML as window.__RTERM_CERT_HASH__
3. WASM reads cert hash and connects via WebTransport (QUIC:4433)
   using serverCertificateHashes for self-signed cert trust
4. Client opens a bidi stream
5. Client sends length-prefixed Resize(cols, rows) as first message
6. Server spawns PTY with that size
7. Bidirectional streaming:
   - Client sends DataIn(bytes) for keyboard input
   - Server sends DataOut(bytes) for PTY output
   - Client sends Resize on window resize
8. Server sends Exit(code) when shell dies
9. Stream closes, connection closes
```

#### Native gRPC Client

```
1. Client connects via HTTP/3 (QUIC handshake)
2. Client opens gRPC bidi stream (TerminalService.Session)
3. Client sends Resize(cols, rows) as first message
4. Server spawns PTY with that size
5. Bidirectional streaming via gRPC
6. Server sends Exit(code) when shell dies
7. gRPC stream closes, QUIC connection closes
```

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
  -> Transport send (FlatBuffers ClientMessage::DataIn)
  -> WebTransport bidi stream (QUIC/UDP)
  -> PTY stdin
  -> Shell process
  -> PTY stdout
  -> WebTransport bidi stream (QUIC/UDP)
  -> Transport recv (FlatBuffers ServerMessage::DataOut)
  -> vte parser (tokenize escape sequences)
  -> Screen buffer mutations (cursor, colors, text, scroll)
  -> Synchronized output check (skip repaint during CSI ?2026 h batch)
  -> egui render loop reads cells
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
