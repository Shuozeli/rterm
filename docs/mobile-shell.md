<!-- agent-updated: 2026-04-09T22:00:00Z -->
# rterm Mobile Shell Design

## Product

An SSH terminal client for iOS and Android. Like Termius, not like Termux.
No local shell, no package management, no PTY on device.

## Architecture: Flutter Native (No WebView, No WASM)

```
+------------------------------------------------------------+
| Flutter App (Android / iOS)                                |
|                                                            |
|  +------------------------------------------------------+  |
|  | Native Flutter Widgets                               |  |
|  |                                                      |  |
|  |  TerminalGrid (CustomPaint)    Host list / settings |  |
|  |  - renders ScreenBuffer        - Flutter Widgets     |  |
|  |  - native Canvas drawing         - SharedPreferences |  |
|  |  - keyboard input                                     |  |
|  |                                                      |  |
|  +------------------------------------------------------+  |
|                                                            |
|  relay_url: '100.95.116.72' (default, per-host configurable)|
+------------------------------------------------------------+
                           |
                           | WebSocket (ws://host:4435/ws)
                           | FlatBuffers + 4-byte BE u32 length prefix
                           v
+------------------------------------------------------------+
| rterm-relay (server)                                       |
|                                                            |
|  - WebSocket endpoint (/ws)                               |
|  - SSH session via SshTransport (russh)                  |
|  - rterm-core VT emulation                                |
|  - Serves mobile/web build for browser clients            |
+------------------------------------------------------------+
```

### Flutter vs WebView/WASM

| Concern | WebView + WASM | Flutter Native (current) |
|---------|----------------|------------------------|
| Terminal renderer | JS canvas in rterm-wasm | Native Flutter CustomPaint |
| Rendering parity | Same as browser/desktop | Different renderer (Flutter canvas) |
| Performance | WASM overhead | Direct native drawing |
| Complexity | Requires WASM build pipeline | Pure Dart/Flutter |
| Code sharing | Shares egui renderer | Shares FlatBuffers schema only |

### How It Works

**Flutter provides full app:**
- Host list screen (add/edit/delete hosts)
- Settings screen (relay URL)
- Native terminal grid rendered via `CustomPaint`
- WebSocket client connects directly to relay

**Native rendering:**
- `TerminalGrid` widget uses `CustomPainter` to draw cells directly on `Canvas`
- `ScreenBuffer` model holds terminal state (cells, cursor, attributes)
- VT escape sequence handling happens on the relay, not in Flutter

**rterm-relay unchanged:**
- WebSocket endpoint on port 4435
- SSH session via SshTransport (russh)
- All VT emulation logic in Rust

### Data Flow

1. User opens app -> Flutter host list screen
2. User taps host -> `TerminalScreen` connects via WebSocket
3. WebSocket connects to `ws://<relay>:4435/ws`
4. Flutter sends `Resize` then `CreateSession` messages (FlatBuffers + 4-byte BE length prefix)
5. Relay creates SSH session to target host
6. rterm-core handles VT emulation on relay
7. Screen updates streamed as FlatBuffers `ScreenSnapshot` / `ScreenUpdate` messages
8. Flutter renders using `CustomPaint` - no WASM, no WebView

### Host Profiles

Stored in Flutter SharedPreferences as JSON:

```json
[
  {
    "id": "uuid",
    "name": "prod-server",
    "hostname": "100.95.116.72",
    "port": 22,
    "username": "deploy",
    "authType": "password",
    "password": "secret",
    "relayUrl": null
  }
]
```

Each host can optionally specify its own `relayUrl` to use a different relay server.

## Connection Flow

```
Flutter App                          rterm-relay
     |                                    |
     |------- TCP connect :4435 --------->|
     |                                    |
     |------- WebSocket handshake -------->|
     |<------ WebSocket accepted ----------|
     |                                    |
     |------- Resize (cols, rows) -------->|
     |------- CreateSession (name) ------>|
     |                                    |
     |<----- ScreenSnapshot (FlatBuffers) -|
     |<----- ScreenUpdate (FlatBuffers) --|
     |        ...                         |
     |------- KeyInput (bytes) --------->|
     |------- Resize (cols, rows) -------->|
     |------- DetachSession -------------->|
```

Protocol format: FlatBuffers message with 4-byte BE u32 length prefix (wire format defined in `rterm_proto::wire`).

## mobile/web: Separate Dart Web Build

The `mobile/web/` subdirectory is a **separate Dart web application** built for browser use (not the mobile Flutter app). It is served by the relay for browser-based terminal access.

```
mobile/
├── lib/                    # Flutter mobile app (Native rendering)
│   ├── main.dart
│   ├── screens/terminal_screen.dart   # WebSocket + CustomPaint
│   ├── widgets/terminal_grid.dart     # CustomPainter for cells
│   ├── services/websocket_client.dart # WebSocket to relay :4435
│   ├── models/
│   └── generated/                    # FlatBuffers generated Dart
│
└── web/                    # Separate Dart web app (served by relay)
    ├── lib/
    │   ├── main.dart
    │   ├── websocket_client.dart
    │   ├── terminal_renderer.dart     # Canvas-based renderer
    │   └── demo.dart
    └── index.html
```

The relay's static file server serves the `mobile/web/build/` output for browser clients connecting via HTTPS.

## Build & Run

### Prerequisites

```bash
# Run relay (WebSocket on port 4435, static files for mobile/web)
cargo run -p rterm-relay

# Run Flutter app (Android emulator / iOS simulator)
cd mobile
flutter run
```

### Protocol Dependencies

The Flutter app uses:
- `web_socket_channel` - WebSocket client
- `flat_buffers` - FlatBuffers schema serialization
- `protobuf` / `grpc` - (available but not primary transport)

### WebSocket Protocol

- **Port**: 4435 (insecure, for development)
- **Path**: `/ws`
- **Format**: `[4-byte BE u32 length]` + `[FlatBuffers payload]`
- **Messages**: Resize, CreateSession, AttachSession, KeyInput, DetachSession, DestroySession

## Repo Structure

```
rterm/
  crates/
    rterm-core/         # VT emulation
    rterm-proto/        # FlatBuffers schema + codec
    rterm-transport/    # Transport trait + PTY + SSH
    rterm-session/      # Session + SessionManager
    rterm-service/      # gRPC service
    rterm-relay/        # Server binary (WebSocket, WebTransport, gRPC)
    rterm-wasm/         # Browser egui renderer (separate from mobile)
      dist/             # WASM build output
    rterm-gui/          # Desktop egui demo
    rterm-cli/          # Automation CLI
  mobile/               # Flutter app (NATIVE rendering, NOT WASM)
    lib/
      main.dart
      models/
        host_profile.dart
        screen_buffer.dart
        cell.dart
      screens/
        host_list_screen.dart   # Host list
        terminal_screen.dart    # WebSocket + CustomPaint terminal
      widgets/
        terminal_grid.dart      # CustomPainter terminal renderer
      services/
        host_storage.dart       # SharedPreferences JSON storage
        websocket_client.dart   # WebSocket client to relay :4435
      utils/
        screen_converter.dart   # FlatBuffers -> ScreenBuffer
      generated/                 # FlatBuffers generated Dart code
  mobile/web/            # SEPARATE Dart web app (served by relay)
```

## Key Files

| File | Purpose |
|------|---------|
| `mobile/lib/screens/terminal_screen.dart` | Terminal screen, WebSocket connection, keyboard handling |
| `mobile/lib/widgets/terminal_grid.dart` | Native Flutter `CustomPaint` terminal renderer |
| `mobile/lib/services/websocket_client.dart` | WebSocket client with FlatBuffers encoding |
| `mobile/lib/models/screen_buffer.dart` | In-memory terminal screen model |
| `mobile/lib/models/cell.dart` | Terminal cell with attributes (colors, bold, underline) |
| `crates/rterm-relay/src/ws_handler.rs` | Relay WebSocket handler with length-prefix framing |
| `crates/rterm-proto/schema/rterm.fbs` | FlatBuffers schema definition |

## Decisions Made

1. **Flutter native rendering** - Native `CustomPaint` instead of WebView/WASM for better performance and simpler build pipeline
2. **WebSocket to relay** - Direct WebSocket connection on port 4435, not WebTransport
3. **FlatBuffers protocol** - Efficient binary serialization with 4-byte BE length prefix
4. **Per-host relay URL** - Each host specifies which relay to use
5. **No FFI** - All terminal logic in relay, Flutter only handles rendering and input
6. **SSH-only for mobile** - No local terminal
