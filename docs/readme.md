<!-- agent-updated: 2026-03-29T23:20:00Z -->
# rterm

A minimal, correct terminal emulator built in Rust. The server runs VT emulation and sends typed screen updates to a thin egui WASM renderer. HTTP/3 (QUIC) is the only transport. No legacy.

## Design Principles

- **Minimal**: No tabs, no panes, no splits. One terminal per window.
- **Correct**: VT emulation correctness is the top priority.
- **Universal**: One WASM binary runs everywhere via egui.
- **HTTP/3 only**: QUIC transport. No HTTP/2, no WebSocket, no fallbacks.
- **Ligatures** (planned): Custom font rendering via rustybuzz + fontdue.

## Architecture

```
rterm-relay (server)
  |
  +-- Spawns PTY per connection
  +-- Runs VT emulator (rterm-core) server-side
  +-- Diffs screen state, sends typed ScreenUpdate messages
  |
WASM Bundle (egui thin renderer)
  |
  +-- Applies ScreenUpdate/ScreenSnapshot cell changes
  +-- Paints cells via egui terminal grid widget
  |
  +-- Desktop: rterm-shell (WebView + local PTY) [not yet implemented]
  +-- Browser: connects to rterm-relay via WebTransport
  +-- Mobile: rterm-shell (WebView + remote relay) [not yet implemented]
```

## Crate Structure

| Crate | Description |
|---|---|
| rterm-proto | FlatBuffers protocol with typed screen updates (Cell, CellRange, ScreenUpdate, ScreenSnapshot) |
| rterm-core | VT100/VT220 emulation: vte parser, screen buffer, cell types |
| rterm-gui | egui terminal widget: color palette, input encoding, selection, scrollback |
| rterm-shell | Native WebView wrapper + local PTY (stub, not yet implemented) |
| rterm-relay | HTTP/3 + WebTransport relay server with server-side VT emulation + screen diffing |
| rterm-wasm | WASM browser thin renderer (excluded from workspace, built with trunk) |

## Related Repos

| Repo | Role |
|---|---|
| flatbuffers-rs | FlatBuffers compiler + runtime (bugs fixed as found) |
| pure-grpc-rs | gRPC framework + HTTP/3 transport (bugs fixed as found) |

## Terminal Standard

xterm-compatible (VT220 + xterm color/mouse/paste/screen extensions).

## Building

```bash
# Run tests (VT emulation correctness)
cargo test -p rterm-core

# Run relay integration tests (requires PTY)
cargo test -p rterm-relay

# Build WASM browser client
cd crates/rterm-wasm && RUSTFLAGS="--cfg web_sys_unstable_apis" trunk build

# Start relay server (serves WASM bundle + WebTransport on port 4433)
cargo run -p rterm-relay

# Then open https://localhost:4433 in Chrome and accept the cert warning.

# Native egui demo (connects to running relay via gRPC/H3)
cargo run -p rterm-gui --example demo
```

## Status

- Phase 1 (VT Emulation Core): done -- full VT100/VT220 emulation with 247 tests across all crates
- Phase 2 (Protocol + Transport): done -- typed FlatBuffers protocol (ScreenUpdate, ScreenSnapshot, CellRange, Cell), WebTransport relay, gRPC/H3
- Phase 3 (egui Terminal Widget): done -- WASM browser terminal working end-to-end with server-side VT emulation
- Major refactor completed: server-side VT emulation, typed screen protocol, trait extraction (PtySpawner), shared session module, screen diffing
- Phase 4 (Native Shell): not started -- rterm-shell is a stub
- Phase 5 (Custom Font Rendering): not started -- using egui built-in monospace font
- Phase 6 (Completeness): partially started -- synchronized output implemented
- Phase 7 (Mobile): not started
