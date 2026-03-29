# rterm

A minimal, correct terminal emulator built in Rust. egui compiled to WASM is the universal renderer. gRPC over HTTP/3 is the only transport. No legacy.

## Design Principles

- **Minimal**: No tabs, no panes, no splits. One terminal per window.
- **Correct**: VT emulation correctness is the top priority.
- **Universal**: One WASM binary runs everywhere via egui.
- **HTTP/3 only**: gRPC over QUIC. No HTTP/2, no WebSocket, no fallbacks.
- **Ligatures**: Custom font rendering via rustybuzz + fontdue.

## Architecture

```
WASM Bundle (egui + rterm-core)
  |
  +-- gRPC over HTTP/3 (QUIC) to PTY backend
  |
  +-- Desktop: rterm-shell (WebView + local PTY + gRPC/H3 server)
  +-- Browser: connects to remote rterm-relay via WebTransport
  +-- Mobile: rterm-shell (WebView + remote relay)
```

## Crate Structure

| Crate | Description |
|---|---|
| rterm-proto | FlatBuffers schema, generated code, transport trait |
| rterm-core | VT emulation: vte parser, screen buffer, cell types |
| rterm-gui | egui terminal widget, glyph atlas, input encoding (WASM) |
| rterm-shell | Native WebView wrapper + local PTY + gRPC/HTTP/3 server |
| rterm-relay | Standalone remote gRPC/HTTP/3 server |

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

# Browser (requires trunk)
cd crates/rterm-gui && trunk serve

# Relay server
cargo run -p rterm-relay

# Desktop
cargo run -p rterm-shell
```

## Status

Phase 1: VT Emulation Core (in progress)
