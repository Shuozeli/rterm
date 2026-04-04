<!-- agent-updated: 2026-04-03T00:00:00Z -->
# rterm Mobile Shell Design

## Product

An SSH terminal client for iOS and Android. Like Termius, not like Termux.
No local shell, no package management, no PTY on device.

## Architecture: Flutter + WebView + rterm-wasm

```
+------------------------------------------------------------+
| Flutter App (Android / iOS)                                |
|                                                            |
|  +------------------------------------------------------+  |
|  | WebView                                              |  |
|  |                                                      |  |
|  |  rterm-wasm (egui)         Host list / settings      |  |
|  |  - terminal cell grid      - Flutter Widgets          |  |
|  |  - VT emulation             - SharedPreferences        |  |
|  |  - keyboard input                                     |  |
|  |                                                      |  |
|  +------------------------------------------------------+  |
|                                                            |
|  relay_url: 'https://relay.example.com:4433' (per-host)   |
+------------------------------------------------------------+
                           |
                           | HTTPS + WebTransport
                           v
+------------------------------------------------------------+
| rterm-relay (server)                                       |
|                                                            |
|  - Serves rterm-wasm (static files, HTTP/3)               |
|  - WebTransport endpoint (/wt/{session})                  |
|  - SSH session via SshTransport (russh)                   |
|  - rterm-core VT emulation                                |
+------------------------------------------------------------+
```

### Why Flutter + WebView over Tauri

| Concern | Tauri | Flutter + WebView |
|---------|-------|-------------------|
| Terminal renderer | JS canvas (reimplement) | egui wasm (reuse existing) |
| Renderer parity | Different renderer than desktop | Identical renderer |
| DevX | HTML/CSS/JS for chrome | Flutter widgets (typed, fast) |
| WebTransport | Needs custom Tauri transport | Native browser WebTransport |
| Code sharing | HTML/CSS/JS for chrome | Flutter widgets |

### How it works

**Flutter provides app chrome:**
- Host list screen (add/edit/delete hosts)
- Settings screen (relay URL)
- WebView widget that loads rterm-wasm

**rterm-wasm runs in WebView:**
- Loads from relay URL (same as desktop/browser)
- Reads session name from URL path
- Connects via WebTransport to relay — no changes needed
- Full egui renderer identical to desktop/browser

**rterm-relay unchanged:**
- Serves WASM static files on HTTP/3 port
- WebTransport endpoint for terminal sessions
- All SSH + VT logic in Rust

### Data Flow

1. User opens app → Flutter host list screen
2. User taps host → `TerminalScreen` creates WebView
3. WebView loads `https://<relay>/<session_name>` (e.g. `https://relay:4433/prod-server`)
4. rterm-wasm reads session name = "prod-server" from URL path
5. rterm-wasm opens WebTransport to `wss://<relay>/wt/prod-server`
6. Relay creates SSH session to target host
7. rterm-core handles VT emulation on relay
8. Screen updates streamed over WebTransport → rendered by egui

### Host Profiles

Stored in Flutter SharedPreferences as JSON:

```json
[
  {
    "id": "uuid",
    "name": "prod-server",
    "hostname": "10.0.0.5",
    "port": 22,
    "username": "deploy",
    "authType": "key",
    "privateKey": "-----BEGIN OPENSSH PRIVATE KEY-----\n...",
    "relayUrl": "https://relay.example.com:4433"
  }
]
```

Each host has its own `relayUrl` so different users/orgs can use their own relay.

## Build & Run

### Prerequisites

```bash
# Build rterm-wasm
cd crates/rterm-wasm
RUSTFLAGS="--cfg web_sys_unstable_apis" trunk build

# Run relay (serves WASM + WebTransport on port 4433)
cargo run -p rterm-relay

# Run Flutter app (Android emulator)
cd mobile
flutter run
```

### rterm-wasm build output

The WASM is built to `crates/rterm-wasm/dist/`:
- `index.html`
- `rterm-wasm-<hash>.js`
- `rterm-wasm-<hash>_bg.wasm`

The relay's `static_dir` auto-discovers this directory.

### WebTransport requirements

- **Android WebView**: Chromium-based — supports WebTransport natively (Chrome 89+)
- **iOS WKWebView**: Does NOT support WebTransport (as of 2024)
  - Fallback: connect via relay URL in Safari, or use iOS app with local proxy

## Repo Shape

```
rterm/
  crates/
    rterm-core/         # VT emulation
    rterm-proto/        # FlatBuffers codec
    rterm-transport/    # Transport trait + PTY + SSH
    rterm-session/      # Session + SessionManager
    rterm-service/      # gRPC service (for relay)
    rterm-relay/        # Server binary (HTTP/3 + WebTransport)
    rterm-wasm/         # Browser egui renderer
      dist/             # WASM build output (served by relay)
    rterm-gui/          # Desktop egui demo
    rterm-cli/          # Automation CLI
  mobile/               # Flutter app
    lib/
      main.dart
      models/
        host_profile.dart
      screens/
        host_list_screen.dart   # Host list
        host_edit_screen.dart   # Add/edit host
        terminal_screen.dart    # WebView → rterm-wasm
        settings_screen.dart    # App settings
      services/
        host_storage.dart       # SharedPreferences JSON storage
```

## Decisions Made

1. **Flutter for app chrome** — typed widgets, fast iteration, good mobile UX
2. **egui wasm in WebView** — identical renderer to desktop/browser, zero reimplementation
3. **WebTransport to relay** — rterm-wasm unchanged, just like desktop/browser
4. **Per-host relay URL** — each host specifies which relay to use
5. **No FFI** — WebView is the boundary, all terminal logic in WASM/relay
6. **SSH-only for mobile** — no local terminal
