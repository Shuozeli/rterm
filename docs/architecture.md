<!-- agent-updated: 2026-04-02T22:00:00Z -->
# rterm Architecture

## Overview

rterm is a terminal system built in Rust with two deployment modes:

- **Server mode (relay):** PTY runs on a remote server, clients connect via
  WebTransport (browser) or gRPC (native/mobile). VT emulation is server-side.
- **Client mode (agent):** SSH connection runs from the device, VT emulation
  is local. A local gRPC service exposes the same API as the relay.

Both modes share the same session/VT/gRPC layers. The only difference is the
byte source: PTY (server) vs SSH (client).

rterm-core (the VT emulator) is pure Rust with zero OS dependencies. It compiles
to any target: WASM, ARM, x86_64.

## Design Principles

- **rterm-core is the product:** A portable, correct VT emulator that any client
  can embed. The relay and agent are just different wrappers around it.
- **gRPC is the universal boundary:** All inter-process communication uses gRPC.
  No FFI, no shared libraries, no .so/.dylib. Clean process boundaries.
- **Rust everywhere except UI:** VT emulation, SSH, session management, gRPC
  service — all Rust. Only the mobile UI layer (Flutter) is non-Rust.
- **Transport-agnostic sessions:** A session is a Transport + VT emulator.
  The Transport trait abstracts PTY, SSH, or test fakes.
- **Logical correctness first:** VT emulation correctness before GUI polish.

## Crate Structure

```
rterm/
  crates/
    rterm-core/          VT100/VT220 emulation engine
    │                    Pure Rust. No OS deps. Compiles everywhere.
    │                    vte parser, screen buffer, cell grid, Flags bitfield.
    │
    rterm-proto/         FlatBuffers protocol + gRPC service definitions
    │                    Wire types: CellData, ScreenUpdate, ScreenSnapshot.
    │                    Codec: FlatBuffers encode/decode for all message types.
    │
    rterm-transport/     I/O source abstraction (NEW, to be extracted)
    │                    trait Transport { read, write, resize, close }
    │                    PtyTransport  — wraps portable-pty (server mode)
    │                    SshTransport  — wraps russh (client mode)
    │                    FakeTransport — in-memory channels (tests)
    │
    rterm-session/       Session = Transport + VT + screen state (NEW, to be extracted)
    │                    Session — owns Terminal + Transport, feeds bytes, diffs screen
    │                    SessionManager — named sessions, reaper, attach/detach
    │                    Automation — RunCommand, WaitForText, PressKeys logic
    │
    rterm-service/       gRPC service layer (NEW, to be extracted)
    │                    TerminalService — gRPC handlers, delegates to SessionManager
    │                    Works with ANY Transport (PTY or SSH)
    │
    rterm-relay/         Binary: server mode
    │                    Wires up PtyTransport + rterm-service + WebTransport
    │                    Serves WASM bundle over HTTPS
    │                    Thin launcher after extraction
    │
    rterm-agent/         Binary: client mode
    │                    SshPtySpawner bridges SSH into PtyHandle channels
    │                    Plaintext gRPC/H2 on 127.0.0.1 (localhost only)
    │                    CreateSession shell field: ssh://user:pass@host:port
    │
    rterm-wasm/          Browser renderer (egui, connects to relay via WebTransport)
    rterm-gui/           Desktop demo (egui, connects via gRPC)
    rterm-cli/           Automation CLI (connects via gRPC)

  mobile/
    Flutter app          Dart UI, connects via gRPC to rterm-agent (localhost)
                         or rterm-relay (remote)
```

## The Transport Trait

The key abstraction that enables both server and client modes:

```rust
#[async_trait]
pub trait Transport: Send + Sync {
    async fn read(&mut self) -> Result<Vec<u8>, TransportError>;
    async fn write(&mut self, data: &[u8]) -> Result<(), TransportError>;
    async fn resize(&mut self, cols: u16, rows: u16) -> Result<(), TransportError>;
    async fn close(&mut self) -> Result<(), TransportError>;
}
```

| Implementation | Byte source | Used by |
|---------------|-------------|---------|
| PtyTransport | Local PTY (portable-pty) | rterm-relay |
| SshTransport | SSH channel (russh) | rterm-agent |
| FakeTransport | In-memory channels | Tests |

## System Architecture

### Server Mode (relay — for browser + automation)

```
Browser / rterm-cli / rterm-gui
        │
        │ gRPC or WebTransport
        ▼
+------------------------------------------+
│ rterm-relay                              │
│                                          │
│  rterm-service (gRPC handlers)           │
│       │                                  │
│  rterm-session (SessionManager)          │
│       │                                  │
│  Session = PtyTransport + rterm-core     │
│       │                                  │
│  PtyTransport ──── PTY ──── /bin/bash    │
+------------------------------------------+
```

### Client Mode (agent — for mobile SSH)

```
Flutter app (or any gRPC client)
        │
        │ gRPC (localhost)
        ▼
+------------------------------------------+
│ rterm-agent (runs on device)             │
│                                          │
│  rterm-service (same gRPC handlers)      │
│       │                                  │
│  rterm-session (same SessionManager)     │
│       │                                  │
│  Session = SshTransport + rterm-core     │
│       │                                  │
│  SshTransport ──── SSH ──── remote host  │
+------------------------------------------+
```

**Flutter does not know or care** which mode it is talking to. Same gRPC API,
same proto, same behavior. The user picks "SSH direct" or "relay server"
in connection settings.

### Combined Mode (future)

rterm-relay could also accept SSH connections alongside PTY sessions,
making it a jump host / bastion. Same crate structure, just both
Transport implementations active.

## Protocol

### FlatBuffers Messages

```
Client → Server:
  KeyInput { data: [ubyte] }
  PasteInput { text: string }
  Resize { cols: u16, rows: u16 }
  MouseEvent { row, col, button, modifiers, kind }

Server → Client:
  ScreenUpdate { changes: [CellRange], cursor, cols, rows, title }
  ScreenSnapshot { rows: [CellRange], cursor, cols, num_rows, title, scrollback_len }
  ScrollbackData { lines: [CellRange], offset, total }
  Exit { code: i32 }
  Error { message: string }
  Bell {}

Cell { ch: u32, fg: u32, bg: u32, flags: u16 }
```

Color packing: `0x00RRGGBB` (RGB), `0xFF0000II` (indexed), `0xFFFFFFFF` (default).

Flags: u16 bitfield matching alacritty layout (BOLD, ITALIC, UNDERLINE,
DOUBLE_UNDERLINE, UNDERCURL, DOTTED_UNDERLINE, DASHED_UNDERLINE, DIM,
INVERSE, HIDDEN, STRIKEOUT, WIDE_CHAR, WIDE_CHAR_SPACER, WRAPLINE).

### gRPC Service

```
service TerminalService {
  // Streaming session (browser/native)
  rpc Session (stream ClientMessage) returns (stream ServerMessage);

  // Automation API (unary RPCs)
  rpc ListActiveSessions (Request) returns (Response);
  rpc CreateSession (Request) returns (Response);
  rpc KillSession (Request) returns (Response);
  rpc ResizeSession (Request) returns (Response);
  rpc TypeAction (Request) returns (Response);
  rpc SendKeys (Request) returns (Response);
  rpc PressKeys (Request) returns (Response);
  rpc GetSnapshot (Request) returns (Response);
  rpc WaitForText (Request) returns (Response);
  rpc RunCommand (Request) returns (Response);
}
```

### Transport Paths

| Client | Transport | Use case |
|--------|-----------|----------|
| rterm-wasm (browser) | WebTransport bidi stream (QUIC) | Browser terminal |
| rterm-gui (desktop) | gRPC/H3 (QUIC) | Desktop demo |
| rterm-cli | gRPC/H2 (TLS) | Automation |
| Flutter app (mobile) | gRPC/H2 (localhost, plaintext) | Mobile SSH client |

## Platform Architecture

### Browser

```
rterm-relay serves WASM bundle over HTTPS
  → egui WASM runs in browser tab
  → Connects via WebTransport to relay
  → Server-side VT emulation, typed screen updates
```

### Desktop

```
rterm-gui connects to rterm-relay via gRPC/H3
  → or: rterm-agent runs locally with SshTransport
  → egui renders cell grid natively
```

### Mobile (Flutter)

```
rterm-agent runs as a local process on the device
  → Manages SSH connections + VT emulation
  → Exposes gRPC on localhost

Flutter app connects to localhost gRPC
  → Session list, accessory key bar, settings (Dart)
  → Terminal rendering (Flutter CustomPaint or WebView with egui)

Alternatively: Flutter connects directly to a remote rterm-relay
  → Same gRPC API, no local agent needed
```

## Data Flow

### Server mode (relay)

```
Keyboard → Client (VT encode) → gRPC/WebTransport → relay
  → PTY stdin → Shell → PTY stdout
  → Terminal.feed() (server VT emulation)
  → PrevScreen.diff() → ScreenUpdate
  → gRPC/WebTransport → Client → render cells
```

### Client mode (agent)

```
Keyboard → Flutter → gRPC (localhost) → rterm-agent
  → SshTransport.write() → SSH channel → remote host
  → SSH channel → SshTransport.read()
  → Terminal.feed() (local VT emulation)
  → PrevScreen.diff() → ScreenUpdate
  → gRPC (localhost) → Flutter → render cells
```

## Extraction Plan

What moves out of rterm-relay into shared crates:

| Current location | Moves to | What |
|-----------------|----------|------|
| relay/managed_session.rs | rterm-session | Session + VT + screen state |
| relay/session_manager.rs | rterm-session | Named session registry |
| relay/screen_diff.rs | rterm-session | Screen diffing |
| relay/service.rs (gRPC handlers) | rterm-service | TerminalService impl |
| relay/service.rs (RunCommand etc) | rterm-session | Automation logic |
| relay/pty.rs | rterm-transport | PtyTransport + trait |
| (new) russh wrapper | rterm-transport | SshTransport |
| relay/main.rs | stays | Thin launcher |
| relay/wt_server.rs | stays | WebTransport (relay-specific) |
| relay/https_server.rs | stays | HTTPS static files (relay-specific) |

## New Dependencies

| Crate | Purpose | Pure Rust |
|-------|---------|-----------|
| russh | SSH client | Yes |
| russh-keys | SSH key management | Yes |
| async-trait | Async trait methods | Yes |

## Terminal Standard

xterm-compatible, implemented in priority order:

| Priority | Standard | Status |
|----------|----------|--------|
| P0 | VT100 + VT220 core (cursor, SGR, scroll regions, insert/delete) | Done |
| P1 | xterm essentials (alt screen, 256/true color, window title) | Mostly done |
| P1.5 | Underline variants (double, undercurl, dotted, dashed) via SGR 4:x | Done |
| P2 | Interaction (SGR mouse, bracketed paste, focus events) | Partial |
| P3 | Modern baseline (sync output, OSC 52 clipboard, OSC 8 hyperlinks) | Sync done |
| P4 | Kitty keyboard protocol | Not started |
| P5 | Sixel / Kitty graphics | Not started |

## Key Dependencies

| Crate | Purpose |
|-------|---------|
| vte | VT escape sequence parser (rterm-core) |
| bitflags | Cell attribute flags (rterm-core) |
| unicode-width | Character width (rterm-core) |
| flatbuffers-rs | FlatBuffers codec (rterm-proto) |
| pure-grpc-rs | gRPC framework + H3 transport (rterm-service) |
| quinn | QUIC/HTTP/3 (rterm-relay) |
| h3 / h3-webtransport | WebTransport server (rterm-relay) |
| portable-pty | PTY abstraction (rterm-transport) |
| russh | SSH client (rterm-transport) |
| egui + eframe | GUI framework (rterm-gui, rterm-wasm) |
| web-sys | WebTransport API (rterm-wasm) |
