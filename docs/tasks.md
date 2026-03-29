# rterm Tasks

## Phase 1: VT Emulation Core

Priority: logical correctness. No GUI.

- [ ] Define Cell type (character, fg/bg color, attributes: bold/italic/underline/reverse/strikethrough)
- [ ] Define Color type (default, indexed 0-255, RGB)
- [ ] Define ScreenBuffer (2D grid of cells, cursor position, scroll region)
- [ ] Implement ScreenBuffer operations: write char, move cursor, erase, scroll, insert/delete line
- [ ] Implement scrollback ring buffer (circular, fixed max size)
- [ ] Integrate vte crate: implement vte::Perform trait to dispatch parsed sequences
- [ ] VT100 core: SGR (bold, underline, reverse, 8 foreground/background colors)
- [ ] VT100 core: cursor movement (CUU/CUD/CUF/CUB/CUP/HVP)
- [ ] VT100 core: erase (ED, EL)
- [ ] VT100 core: scroll regions (DECSTBM), scroll up/down
- [ ] VT100 core: line drawing characters (DEC Special Graphics)
- [ ] VT100 core: autowrap mode, origin mode, insert mode
- [ ] VT220: insert/delete character (ICH, DCH), insert/delete line (IL, DL)
- [ ] VT220: device status report (DSR -> CPR)
- [ ] VT220: soft terminal reset (DECSTR)
- [ ] Alternate screen buffer (switch/restore)
- [ ] Application cursor keys mode (DECCKM)
- [ ] Show/hide cursor (DECTCEM)
- [ ] Test harness: feed raw bytes -> assert cell content, cursor position, attributes
- [ ] Test with captured output: ls --color, vim startup, htop frames
- [ ] Dark launch validation: run against vttest captured sequences

## Phase 2: Protocol + Transport

### FlatBuffers Schema (rterm-proto)
- [ ] Define FlatBuffers schema: ClientMessage (DataIn, Resize), ServerMessage (DataOut, Exit, Error)
- [ ] Compile schema with flatbuffers-rs, generate Rust code
- [ ] Define TerminalTransport trait (send, recv, close)
- [ ] gRPC service definition: TerminalService.Session (bidi streaming)
- [ ] Round-trip serialization tests for all message types
- [ ] Fix any flatbuffers-rs bugs found during schema compilation

### HTTP/3 Support in pure-grpc-rs
- [ ] Add quinn as HTTP/3 transport backend in pure-grpc-rs
- [ ] Implement gRPC framing over HTTP/3 streams (per A69 proposal)
- [ ] Server-side: accept QUIC connections, route to gRPC handlers
- [ ] Client-side: open QUIC connection, create gRPC bidi streams
- [ ] TLS/certificate handling for QUIC (self-signed for localhost, proper certs for remote)
- [ ] WebTransport compatibility layer (HTTP/3 CONNECT for browser clients)
- [ ] Fix any pure-grpc-rs bugs found during bidi streaming

### WebTransport Client for WASM
- [ ] web-sys bindings for WebTransport API
- [ ] Implement TerminalTransport trait over WebTransport bidi stream
- [ ] gRPC framing over WebTransport stream

### rterm-relay Server
- [ ] Standalone gRPC/HTTP/3 server (quinn + pure-grpc-rs)
- [ ] PTY spawning via portable-pty (configurable shell)
- [ ] Bidirectional byte streaming: gRPC bidi stream <-> PTY
- [ ] Terminal resize: Resize message -> PTY TIOCSWINSZ
- [ ] Connection lifecycle: connect, spawn PTY, stream, disconnect, kill PTY
- [ ] TLS certificate configuration
- [ ] Test: native gRPC client connects, interactive shell session
- [ ] Test: WASM WebTransport client connects, interactive shell session
- [ ] Test: resize during vim session

## Phase 3: egui Terminal Widget (WASM)

- [ ] eframe WASM build setup (trunk or wasm-pack)
- [ ] Basic terminal grid: render ScreenBuffer cells using egui built-in monospace font
- [ ] Cursor rendering (block, underline, bar shapes)
- [ ] Color rendering (8-color, 256-color, true color mapped to egui Color32)
- [ ] Text attribute rendering (bold, italic, underline, reverse)
- [ ] gRPC transport client in WASM (WebTransport -> pure-grpc-rs)
- [ ] Wire: gRPC recv -> vte parser -> ScreenBuffer -> egui render
- [ ] Keyboard input -> VT sequence encoding -> gRPC send
- [ ] Mouse scroll -> scrollback navigation
- [ ] Integration test: connect to rterm-relay, interactive shell in browser

## Phase 4: Native Shell (rterm-shell)

- [ ] Minimal binary: open WebView via wry
- [ ] Embed WASM bundle (index.html + .wasm + .js) as static assets
- [ ] Local PTY spawning via portable-pty
- [ ] Localhost gRPC/HTTP/3 server (quinn + pure-grpc-rs, self-signed cert)
- [ ] Pass gRPC endpoint to WASM (via URL params or JS injection)
- [ ] TOML config loading (~/.config/rterm/config.toml)
- [ ] Inject config into WASM
- [ ] Window title from OSC 0/2 sequences
- [ ] Clipboard integration
- [ ] Test: launch rterm-shell, interactive terminal session

## Phase 5: Custom Font Rendering

- [ ] rustybuzz integration: shape text runs, extract glyph IDs + positions
- [ ] fontdue integration: rasterize glyphs to bitmaps
- [ ] Texture atlas: pack glyph bitmaps, upload as WebGL texture
- [ ] egui Mesh rendering: draw quads with UV coords from atlas
- [ ] Bundle JetBrains Mono as default font
- [ ] Ligature rendering (verify with -> => != === etc.)
- [ ] CJK double-width character handling
- [ ] TODO: custom font loading / dynamic font switching

## Phase 6: Completeness

- [ ] 256-color support (SGR 38;5;N / 48;5;N)
- [ ] True color support (SGR 38;2;R;G;B / 48;2;R;G;B)
- [ ] SGR mouse reporting (mode 1006)
- [ ] Bracketed paste mode (mode 2004)
- [ ] Focus events (mode 1004)
- [ ] Synchronized output (mode 2026)
- [ ] Cursor shapes (DECSCUSR: block, underline, bar, blinking variants)
- [ ] OSC 0/2: window title
- [ ] OSC 8: hyperlinks
- [ ] OSC 52: clipboard access
- [ ] Colored underlines (SGR 58)
- [ ] Underline styles (single, double, curly, dotted, dashed)
- [ ] Text selection (click + drag)
- [ ] Clipboard copy/paste
- [ ] Scrollback search
- [ ] Window/terminal size query responses

## Phase 7: Mobile

- [ ] rterm-shell Android build (System WebView via wry)
- [ ] rterm-shell iOS build (WKWebView via wry)
- [ ] Touch keyboard integration
- [ ] Touch-based text selection
- [ ] Swipe scrollback
- [ ] Pinch-to-zoom font size
