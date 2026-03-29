<!-- agent-updated: 2026-03-29T23:20:00Z -->
# rterm Tasks

## Phase 1: VT Emulation Core

Priority: logical correctness. No GUI.

- [x] Define Cell type (character, fg/bg color, attributes: bold/italic/underline/reverse/strikethrough)
- [x] Define Color type (default, indexed 0-255, RGB)
- [x] Define ScreenBuffer (2D grid of cells, cursor position, scroll region)
- [x] Implement ScreenBuffer operations: write char, move cursor, erase, scroll, insert/delete line
- [x] Implement scrollback ring buffer (circular, fixed max size)
- [x] Integrate vte crate: implement vte::Perform trait to dispatch parsed sequences
- [x] VT100 core: SGR (bold, underline, reverse, 8 foreground/background colors)
- [x] VT100 core: cursor movement (CUU/CUD/CUF/CUB/CUP/HVP)
- [x] VT100 core: erase (ED, EL)
- [x] VT100 core: scroll regions (DECSTBM), scroll up/down
- [ ] VT100 core: line drawing characters (DEC Special Graphics) -- relies on Unicode, no explicit charset switching
- [x] VT100 core: autowrap mode, origin mode, insert mode
- [x] VT220: insert/delete character (ICH, DCH), insert/delete line (IL, DL)
- [x] VT220: device status report (DSR -> CPR)
- [x] VT220: soft terminal reset (DECSTR)
- [x] Alternate screen buffer (switch/restore)
- [x] Application cursor keys mode (DECCKM)
- [x] Show/hide cursor (DECTCEM)
- [x] Test harness: feed raw bytes -> assert cell content, cursor position, attributes
- [x] Test with captured output: ls --color, vim startup, htop frames
- [ ] Dark launch validation: run against vttest captured sequences

## Phase 2: Protocol + Transport

### FlatBuffers Schema (rterm-proto)
- [x] Define FlatBuffers schema: ClientMessage (KeyInput, PasteInput, Resize, MouseEvent), ServerMessage (ScreenUpdate, ScreenSnapshot, ScrollbackData, Exit, Error, Bell)
- [x] Compile schema with flatbuffers-rs, generate Rust code
- [x] Typed screen protocol: Cell (ch, fg, bg, attrs), CellRange, ScreenUpdateData, ScreenSnapshotData, CursorData
- [x] Color packing utilities (RGB, indexed, default) and attribute bitflags
- [x] Transport uses channel-based session module instead of TerminalTransport trait
- [x] gRPC service definition: TerminalService.Session (bidi streaming)
- [x] Round-trip serialization tests for all message types
- [x] Fix any flatbuffers-rs bugs found during schema compilation

### HTTP/3 Support in pure-grpc-rs
- [x] Add quinn as HTTP/3 transport backend in pure-grpc-rs
- [x] Implement gRPC framing over HTTP/3 streams (per A69 proposal)
- [x] Server-side: accept QUIC connections, route to gRPC handlers
- [x] Client-side: open QUIC connection, create gRPC bidi streams
- [x] TLS/certificate handling for QUIC (self-signed for localhost, proper certs for remote)
- [x] WebTransport compatibility layer (HTTP/3 CONNECT for browser clients) -- implemented as raw WebTransport bidi stream with length-prefixed FlatBuffers
- [x] Fix any pure-grpc-rs bugs found during bidi streaming

### WebTransport Client for WASM
- [x] web-sys bindings for WebTransport API
- [x] Implement WebTransport bidi stream transport for WASM
- [x] Length-prefixed FlatBuffers framing over WebTransport stream

### rterm-relay Server
- [x] Standalone server (quinn + h3-webtransport for WebTransport, pure-grpc-rs for gRPC)
- [x] PTY spawning via PtySpawner trait (RealPtySpawner + FakePtySpawner for tests)
- [x] Server-side VT emulation: Terminal.feed() on PTY output, screen diffing via PrevScreen
- [x] Shared session module: session::run_session() used by both wt_handler and service
- [x] Typed screen updates: ScreenSnapshot on connect, ScreenUpdate diffs during session
- [x] Terminal resize: Resize message -> PTY TIOCSWINSZ
- [x] Connection lifecycle: connect, spawn PTY, VT emulate, diff, disconnect, kill PTY
- [x] TLS certificate configuration (auto-generated self-signed ECDSA P256, 14-day validity)
- [x] HTTPS page server for serving WASM bundle (hyper over TCP/TLS)
- [x] Cert hash injection into HTML for WebTransport serverCertificateHashes
- [x] Test: native gRPC client connects, interactive shell session
- [x] Test: resize during active session
- [x] Test: concurrent sessions with isolation
- [ ] Test: WASM WebTransport client automated test (manual testing done)

## Phase 3: egui Terminal Widget (WASM)

- [x] eframe WASM build setup (trunk)
- [x] Basic terminal grid: render ScreenBuffer cells using egui built-in monospace font
- [x] Cursor rendering (block shape)
- [x] Color rendering (8-color, 256-color, true color mapped to egui Color32)
- [x] Text attribute rendering (bold, italic, underline, reverse, dim, hidden, strikethrough)
- [x] WebTransport client in WASM (web-sys WebTransport API)
- [x] Wire: WebTransport recv -> vte parser -> ScreenBuffer -> egui render
- [x] Keyboard input -> VT sequence encoding -> WebTransport send
- [x] Mouse scroll -> scrollback navigation
- [x] Text selection (click + drag) with clipboard copy
- [x] Dynamic terminal resize based on available window space
- [x] Synchronized output mode (batch repaints during CSI ?2026 h)
- [ ] Cursor shapes (underline, bar -- currently block only)
- [ ] Integration test: automated browser terminal test

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

- [x] 256-color support (SGR 38;5;N / 48;5;N)
- [x] True color support (SGR 38;2;R;G;B / 48;2;R;G;B)
- [ ] SGR mouse reporting (mode 1006)
- [ ] Bracketed paste mode (mode 2004) -- acknowledged but not processed
- [ ] Focus events (mode 1004)
- [x] Synchronized output (mode 2026)
- [ ] Cursor shapes (DECSCUSR: block, underline, bar, blinking variants)
- [ ] OSC 0/2: window title
- [ ] OSC 8: hyperlinks
- [ ] OSC 52: clipboard access
- [ ] Colored underlines (SGR 58)
- [ ] Underline styles (single, double, curly, dotted, dashed)
- [x] Text selection (click + drag)
- [x] Clipboard copy/paste
- [ ] Scrollback search
- [ ] Window/terminal size query responses

## Phase 7: Mobile

- [ ] rterm-shell Android build (System WebView via wry)
- [ ] rterm-shell iOS build (WKWebView via wry)
- [ ] Touch keyboard integration
- [ ] Touch-based text selection
- [ ] Swipe scrollback
- [ ] Pinch-to-zoom font size
