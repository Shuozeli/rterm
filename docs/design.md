<!-- agent-updated: 2026-03-29T23:20:00Z -->
# rterm Design

## Goals

1. A correct, minimal terminal emulator built in Rust
2. egui compiled to WASM is the universal renderer (no HTML, no JS)
3. Runs on desktop (native shell), browser (WASM), and mobile (Flutter)
4. HTTP/3 (QUIC) as the transport layer. Native clients use gRPC/H3; WASM browser clients use WebTransport; Mobile uses WebSocket. No HTTP/2, no fallbacks
5. Custom font rendering with ligature support (rustybuzz + fontdue)
6. xterm-compatible terminal standard
7. Dogfoods flatbuffers-rs and pure-grpc-rs (fix bugs upstream as found)

## Non-Goals

- Tabs, panes, splits (use tmux or your window manager)
- HTTP/2 or any legacy transport
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
QUIC everywhere. Native clients use gRPC over HTTP/3 (quinn + pure-grpc-rs). Browser WASM uses WebTransport API with length-prefixed FlatBuffers on a bidi stream. Both run over QUIC -- no TCP, no WebSocket, no fallbacks.

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
- Typed FlatBuffers schema: ClientMessage (KeyInput, PasteInput, Resize, MouseEvent), ServerMessage (ScreenUpdate, ScreenSnapshot, ScrollbackData, Exit, Error, Bell)
- Channel-based session architecture (mpsc channels instead of transport trait)
- HTTP/3 transport in pure-grpc-rs (quinn-based server + client)
- WebTransport client for WASM (web-sys bindings, raw bidi stream with length-prefixed FlatBuffers)
- rterm-relay: standalone gRPC/HTTP/3 server with server-side VT emulation
- PtySpawner trait for testable PTY abstraction
- Server-side VT emulation + screen diffing (typed ScreenUpdate with changed cells only)
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
- WebView wrapper (wry or tauter)
- Embed WASM bundle as app assets
- Local PTY + localhost gRPC/HTTP/3 server (quinn)
- TOML config loading
- Goal: launch rterm-shell, get a working terminal window
- Mobile is Flutter (not rterm-shell)

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

### Why egui WASM (not native egui)?
- One WASM binary runs everywhere: browser (WebTransport/WebSocket), desktop, mobile
- Zero platform-specific rendering code
- Browser: WASM via WebTransport or WebSocket
- Mobile: Flutter-native rendering via CustomPaint (not WebView)

### Why vte and not alacritty_terminal?
- vte is the parser only; we build our own screen buffer
- Full control over the data model
- alacritty_terminal is tightly coupled to Alacritty's internals

### Configuration
- Desktop: TOML file at ~/.config/rterm/config.toml
- Browser/Mobile: localStorage
- All platforms: compiled-in defaults, config is optional
- No default config values that silently change behavior
