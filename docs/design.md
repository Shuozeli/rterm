# rterm Design

## Goals

1. A correct, minimal terminal emulator built in Rust
2. egui compiled to WASM is the universal renderer (no HTML, no JS)
3. Runs on desktop (native WebView shell), browser, and mobile
4. gRPC over HTTP/3 (QUIC) as the only transport. No HTTP/2, no WebSocket, no fallbacks
5. Custom font rendering with ligature support (rustybuzz + fontdue)
6. xterm-compatible terminal standard
7. Dogfoods flatbuffers-rs and pure-grpc-rs (fix bugs upstream as found)

## Non-Goals

- Tabs, panes, splits (use tmux or your window manager)
- HTTP/2, WebSocket, or any legacy transport
- Supporting legacy browsers (WebTransport required: Chrome 97+, Firefox 114+, Safari 18.2+)
- Custom VT parser (use vte)
- Kitty graphics protocol (initially)
- HTML/JS frontend of any kind

## Design Principles

### Alacritty Philosophy
One terminal per window. No built-in multiplexer. Minimalism reduces bugs and maintenance.

### WASM-First
The WASM build is the first-class citizen. Every platform runs the same WASM bundle. The native shell is a thin wrapper providing a WebView and a local PTY.

### Logical Correctness First
VT emulation correctness is the top priority. A correct terminal with ugly fonts is useful. A pretty terminal that garbles vim output is not.

### HTTP/3 Only
gRPC over QUIC everywhere. Native uses quinn. Browser WASM uses WebTransport API. One protocol, one transport, no negotiation, no fallbacks.

### Dogfooding
flatbuffers-rs and pure-grpc-rs are first-party dependencies. Bugs found during rterm development are fixed in those repos directly. pure-grpc-rs gains HTTP/3 support as part of this project.

## Phase Plan

### Phase 1: VT Emulation Core (logical correctness)
Build rterm-core with correct VT emulation. No GUI. Validate with automated tests.
- Cell type with full attribute support
- Screen buffer (2D grid, cursor, scroll regions)
- Scrollback ring buffer
- vte integration: parser -> dispatch -> screen buffer mutations
- VT100 + VT220 core sequences
- Alternate screen buffer
- Test harness: feed raw ANSI bytes, assert screen state

### Phase 2: Protocol + Transport
Build rterm-proto and the server side. Add HTTP/3 support to pure-grpc-rs.
- FlatBuffers schema (ClientMessage, ServerMessage)
- Transport trait (abstract over native QUIC and browser WebTransport)
- HTTP/3 transport in pure-grpc-rs (quinn-based server + client)
- WebTransport client for WASM (web-sys bindings)
- rterm-relay: standalone gRPC/HTTP/3 server
- PTY spawning via portable-pty
- Bidirectional byte streaming over gRPC bidi stream
- Test: connect via gRPC client, interactive shell session

### Phase 3: egui Terminal Widget (WASM)
Build the GUI. Start with basic text rendering.
- egui WASM build with eframe
- Terminal grid widget: render ScreenBuffer cells
- gRPC client in WASM (WebTransport -> pure-grpc-rs)
- Wire: gRPC recv -> vte parser -> ScreenBuffer -> egui render
- Keyboard input -> VT sequence encoding -> gRPC send
- Goal: interactive shell in browser via relay

### Phase 4: Native Shell (rterm-shell)
Build the desktop app.
- Minimal WebView wrapper (wry)
- Embed WASM bundle as app assets
- Local PTY + localhost gRPC/HTTP/3 server (quinn)
- TOML config loading
- Goal: launch rterm-shell, get a working terminal window

### Phase 5: Custom Font Rendering
Replace egui built-in text with custom glyph atlas.
- rustybuzz for text shaping (ligatures)
- fontdue for glyph rasterization
- Texture atlas management
- egui Mesh-based quad rendering from atlas
- Bundle JetBrains Mono
- TODO: custom font loading

### Phase 6: Completeness
- Full color support (256 + true color)
- SGR mouse reporting, bracketed paste, focus events
- Synchronized output, cursor shapes
- OSC 8 hyperlinks, OSC 52 clipboard
- Colored/styled underlines
- Text selection, clipboard, scrollback search

### Phase 7: Mobile
- rterm-shell for Android/iOS
- Touch input handling

## Key Technical Decisions

### Why HTTP/3 only?
- QUIC provides lower latency than TCP (0-RTT connection setup)
- True bidi streaming in browsers via WebTransport API (no gRPC-web hacks)
- One transport everywhere: native (quinn) and browser (WebTransport) speak the same protocol
- Simpler architecture: no transport negotiation, no fallback logic
- All major browsers support WebTransport (Chrome 97+, Firefox 114+, Safari 18.2+)
- This is a revolution project. No legacy.

### Why gRPC + FlatBuffers?
- Type-safe, versioned protocol from day one
- Zero-copy deserialization (FlatBuffers) for the hot path (PTY bytes)
- Bidi streaming (gRPC) maps perfectly to terminal I/O
- Dogfoods our own crates (flatbuffers-rs, pure-grpc-rs)

### Why egui WASM in a WebView (not native egui)?
- One WASM binary runs everywhere: browser, desktop WebView, mobile WebView
- Zero platform-specific rendering code
- WebView is available on all platforms

### Why vte and not alacritty_terminal?
- vte is the parser only; we build our own screen buffer
- Full control over the data model
- alacritty_terminal is tightly coupled to Alacritty's internals

### Configuration
- Desktop: TOML file at ~/.config/rterm/config.toml
- Browser/Mobile: localStorage
- All platforms: compiled-in defaults, config is optional
- No default config values that silently change behavior
