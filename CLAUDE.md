# rterm

Terminal emulator with server-side VT emulation. Supports browser (WASM), mobile (Flutter), and CLI clients.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         Clients                                   │
├─────────────┬─────────────┬─────────────┬───────────────────────┤
│   Browser   │   Mobile    │    CLI      │   SSH Agent           │
│  (WASM)    │  (Flutter)  │  (Rust)    │  (rterm-agent)        │
└──────┬──────┴──────┬──────┴──────┬──────┴───────────┬───────────┘
       │             │             │                  │
       │ WebTransport│  WebSocket  │     gRPC         │ gRPC
       │             │             │                  │
┌──────▼─────────────▼─────────────▼──────────────────▼───────────┐
│                      rterm-relay                                │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────────────┐ │
│  │ WebTransport │  │  WebSocket  │  │   gRPC H2/H3         │ │
│  │   Server     │  │    Server   │  │   (TerminalServer)   │ │
│  └──────┬───────┘  └──────┬───────┘  └──────────┬───────────┘ │
│         │                 │                      │             │
│         └─────────────────┼──────────────────────┘             │
│                           │                                    │
│  ┌───────────────────────▼────────────────────────────────┐  │
│  │              Session Manager (rterm-session)              │  │
│  │   ┌─────────────┐  ┌─────────────┐  ┌────────────────┐  │  │
│  │   │ PtySpawner  │  │Screen Diff  │  │ Terminal Core  │  │  │
│  │   │ (PTY/SSH)  │  │             │  │ (VT100/VT220)  │  │  │
│  │   └─────────────┘  └─────────────┘  └────────────────┘  │  │
│  └──────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

## Crates

- `rterm-core` — VT100/VT220 terminal emulation (screen buffer, cell grid, escape sequences)
- `rterm-proto` — FlatBuffers protocol with typed screen updates (Cell, CellRange, ScreenUpdate)
- `rterm-transport` — Transport trait abstraction (PTY, SSH, fake) with PtySpawner
- `rterm-session` — Session management (ManagedSession, SessionManager, screen diffing, automation)
- `rterm-service` — gRPC service handlers (TerminalServer, unary + bidi streaming RPCs)
- `rterm-relay` — HTTP/3 + WebTransport relay server (WebSocket, WebTransport, gRPC handlers)
- `rterm-gui` — egui terminal grid widget (colors, selection, scrolling) for native demo
- `rterm-wasm` — Browser thin renderer (excluded from workspace, built with `trunk`)
- `rterm-cli` — Automation CLI (Playwright-style terminal control via gRPC)
- `rterm-agent` — SSH terminal agent: localhost gRPC server with SshPtySpawner

## Mobile App (Flutter)

Located in `mobile/` — Native Flutter app for Android/iOS.

**Architecture**: Flutter-native rendering via `CustomPaint`. No WASM/WebView.

```
mobile/
├── lib/
│   ├── main.dart                    # App entry point
│   ├── models/                      # Data models (Cell, ScreenBuffer, HostProfile)
│   ├── services/                    # WebSocket client, host storage
│   ├── screens/                    # TerminalScreen, HostListScreen
│   ├── widgets/                    # TerminalGrid (CustomPaint rendering)
│   └── utils/                     # ScreenConverter (FlatBuffers → models)
└── generated/                      # FlatBuffers generated code
```

**Connection Flow**:
1. Connect to relay via WebSocket
2. Send `Resize` message (terminal dimensions)
3. Send `CreateSession` message
4. Receive `ScreenSnapshot` and `ScreenUpdate` messages
5. Render using Flutter `CustomPaint`

**Build & Run**:
```bash
cd mobile

# Build APK
flutter build apk --debug

# Run on device/emulator
flutter run

# The relay must be running with WebSocket support:
# cargo run -p rterm-relay -- --ws-insecure --insecure
```

**Relay Configuration** (rterm.toml):
```toml
[[listener]]
protocol = "web-socket"
port = 4435
bind = "0.0.0.0"  # Or specific IP
```

## Build

```bash
cargo build --workspace          # native crates
cargo test --workspace           # 247 tests

# WASM (separate, excluded from workspace)
cd crates/rterm-wasm
RUSTFLAGS="--cfg web_sys_unstable_apis" trunk build

# Run relay (serves WASM + WebTransport + WebSocket on single port)
cargo run -p rterm-relay
# Open https://localhost:4433/ in Chrome with --webtransport-developer-mode
```

## Architecture Rules

### Composable Struct Design

Every component must be testable in isolation through dependency injection:

1. **Trait boundaries at I/O edges.** Any struct that talks to the OS, network, or external process must accept its dependency as a trait, not call it directly.
   - `PtySpawner` trait instead of calling `native_pty_system()` directly
   - `AsyncRead`/`AsyncWrite` generics instead of concrete h3/quinn stream types
   - Channel-based interfaces (`mpsc::Sender`/`Receiver`) instead of trait objects where simpler

2. **Fakes, not mocks.** Test doubles must be real implementations backed by in-memory channels or buffers:
   - `FakePtySpawner` — returns channel pairs, test controls stdout and reads stdin
   - `std::io::Cursor` — fakes `AsyncRead`/`AsyncWrite` for message framing tests
   - `mpsc::channel` — fakes transport streams for session tests

3. **Shared logic in dedicated modules.** When two handlers need the same logic, extract it:
   - `session::run_session()` — shared between `wt_handler` and `service` (both are thin adapters)
   - `screen_diff` — shared screen diffing logic used by session module
   - `static_files` — pure functions (`resolve_path`, `guess_content_type`) separate from I/O

4. **Thin transport adapters.** Protocol-specific handlers should only do protocol translation:
   - `wt_handler`: WebTransport bidi stream -> channels -> `session::run_session`
   - `service`: gRPC Streaming -> channels -> `session::run_session`
   - All business logic lives in the shared session module

5. **No god structs.** A struct should own its data OR coordinate other structs, not both:
   - `PtyHandle` is a data bag (three channels) — no methods
   - `PtySpawner` creates `PtyHandle` — single responsibility
   - `PrevScreen` owns diff state — single responsibility
   - `Terminal` owns VT emulation state — delegates to `ScreenBuffer`

### Testing Standards

- **Coverage target:** 95%+ for core logic (buffer, terminal, session, proto, screen_diff)
- **Coverage acceptable:** 60%+ for I/O adapters (wt_handler, service, https_server, pty)
- **Never 0%:** Every module must have at least one test for its public API
- **Test with `cargo llvm-cov`:** `cargo llvm-cov --lib -p rterm-core -p rterm-proto -p rterm-relay --summary-only`

### VT Emulator Rules

- **CSI with intermediates (`>`, `<`, `?`, `!`) must NOT be dispatched as standard CSI.** Check intermediates before handling `m` (SGR), `h`/`l` (modes), etc.
- **Parser must be persistent** across `feed()` calls — escape sequences split across network chunks must work.
- **Synchronized output** (`CSI ?2026 h/l`) must suppress screen updates between begin/end markers.

## Schema & Code Generation Rules

**ALWAYS use the existing schema format's native code generator first.**

- `.fbs` (FlatBuffers) → `flatc --dart` (Dart), `flatc -r -g` (Rust from `crates/rterm-proto/schema/`)
- `.proto` → `protoc` (only if no `.fbs` exists for that schema)
- **NEVER create a parallel `.proto` when a `.fbs` already defines the same schema** — this causes duplication and divergence.
- **NEVER introduce protobuf for Dart if FlatBuffers Dart generation exists** — `flatc --dart` is the correct path.
- When asked to generate code from a schema: (1) find the existing schema file, (2) use its native generator, (3) if the generator is missing, install/build it first.

**Wrong pattern (what NOT to do):**
```
# BAD: creating .proto from scratch when .fbs already exists
protoc --dart_out ... rterm.proto    # WRONG
dart pub add protobuf grpc           # WRONG — wrong serialization format

# CORRECT: generate Dart from existing .fbs
flatc --dart -o mobile/lib/generated crates/rterm-proto/schema/rterm.fbs
```

## Task Scope Rules

- When asked to add Dart code generation, first check existing schema files (`**/*.fbs`, `**/*.proto`) in the project.
- When asked to add a new language binding, use the schema's existing code generator (e.g., `flatc --ts` for TypeScript from `.fbs`), not a different toolchain.
- If the native generator for a target language doesn't exist, report it clearly and stop — do not substitute another format or toolchain.
- **Before using any new package manager, library, or tool**: verify it doesn't duplicate something already in the project (e.g., adding `protobuf` when `flatbuffers` is already used).

## Git Rules

- Pre-commit hook runs `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test --lib`
- CI runs format, clippy, build+test, docs, WASM check
- Do not push until CI is green
