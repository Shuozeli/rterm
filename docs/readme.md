<!-- agent-updated: 2026-04-09T22:00:00Z -->
# rterm

A minimal, correct terminal emulator built in Rust. The server runs VT emulation and sends typed screen updates to a thin egui WASM renderer. HTTP/3 (QUIC) is the only transport. No legacy.

## Design Principles

- **Minimal**: No tabs, no panes, no splits. One terminal per window.
- **Correct**: VT emulation correctness is the top priority.
- **Universal**: One WASM binary runs everywhere via egui.
- **Multi-transport**: WebTransport, WebSocket, and gRPC/H2/H3 all served on a single port.
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
  +-- Browser: connects to rterm-relay via WebTransport or WebSocket
  +-- Mobile: Flutter app with WebSocket client connecting to relay
  +-- CLI: rterm-cli automation via gRPC
```

## Crate Structure

| Crate | Description |
|---|---|
| rterm-core | VT100/VT220 emulation: vte parser, screen buffer, cell types |
| rterm-proto | FlatBuffers protocol with typed screen updates (Cell, CellRange, ScreenUpdate, ScreenSnapshot) |
| rterm-transport | Transport trait abstraction (PTY, SSH, fake) with PtySpawner |
| rterm-session | Session management: ManagedSession, SessionManager, screen diffing, automation |
| rterm-service | gRPC service handlers (TerminalServer, unary + bidi streaming RPCs) |
| rterm-relay | HTTP/3 + WebTransport + WebSocket relay server with server-side VT emulation + screen diffing |
| rterm-gui | egui terminal widget: color palette, input encoding, selection, scrollback |
| rterm-wasm | WASM browser thin renderer (excluded from workspace, built with trunk) |
| rterm-cli | Automation CLI (Playwright-style terminal control via gRPC) |
| rterm-agent | SSH terminal agent: localhost gRPC server with SshPtySpawner |
| rterm-mobile/src-tauri | Tauri mobile app (Flutter UI via webview) |

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
- Phase 2 (Protocol + Transport): done -- typed FlatBuffers protocol (ScreenUpdate, ScreenSnapshot, CellRange, Cell), WebTransport relay, WebSocket, gRPC/H2/H3
- Phase 3 (egui Terminal Widget): done -- WASM browser terminal working end-to-end with server-side VT emulation
- Phase 4 (Session Management): done -- ManagedSession, SessionManager, screen diffing, automation
- Phase 5 (Automation CLI): done -- rterm-cli with Playwright-style API, 20 in-process + 8 Docker E2E tests
- Phase 6 (SSH Agent): done -- rterm-agent for localhost gRPC + SSH PTY
- Phase 7 (Mobile): in progress -- Flutter app with WebSocket relay client, APK builds
