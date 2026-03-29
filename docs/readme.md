# rterm

A minimal, correct terminal emulator built in Rust. egui compiled to WASM is the universal renderer. HTTP/3 (QUIC) is the only transport. No legacy.

## Design Principles

- **Minimal**: No tabs, no panes, no splits. One terminal per window.
- **Correct**: VT emulation correctness is the top priority.
- **Universal**: One WASM binary runs everywhere via egui.
- **HTTP/3 only**: QUIC transport. No HTTP/2, no WebSocket, no fallbacks.
- **Ligatures** (planned): Custom font rendering via rustybuzz + fontdue.

## Architecture

```
WASM Bundle (egui + rterm-core)
  |
  +-- WebTransport (QUIC) to PTY backend
  |
  +-- Desktop: rterm-shell (WebView + local PTY) [not yet implemented]
  +-- Browser: connects to rterm-relay via WebTransport
  +-- Mobile: rterm-shell (WebView + remote relay) [not yet implemented]
```

## Crate Structure

| Crate | Description |
|---|---|
| rterm-proto | FlatBuffers schema, generated code, message types |
| rterm-core | VT emulation: vte parser, screen buffer, cell types |
| rterm-gui | egui terminal widget, color palette, input encoding |
| rterm-shell | Native WebView wrapper + local PTY (stub, not yet implemented) |
| rterm-relay | WebTransport relay server + HTTPS page server |
| rterm-wasm | WASM browser client (excluded from workspace, built with trunk) |

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

- Phase 1 (VT Emulation Core): done -- full VT100/VT220 emulation with 137 tests
- Phase 2 (Protocol + Transport): done -- FlatBuffers protocol, WebTransport relay, gRPC/H3
- Phase 3 (egui Terminal Widget): done -- WASM browser terminal working end-to-end
- Phase 4 (Native Shell): not started -- rterm-shell is a stub
- Phase 5 (Custom Font Rendering): not started -- using egui built-in monospace font
- Phase 6 (Completeness): partially started -- synchronized output implemented
- Phase 7 (Mobile): not started
