# rterm

Terminal emulator in the browser via egui WASM + WebTransport, with server-side VT emulation.

## Crates

- `rterm-core` — VT100/VT220 terminal emulation (screen buffer, cell grid, escape sequences)
- `rterm-proto` — FlatBuffers protocol with typed screen updates (Cell, CellRange, ScreenUpdate)
- `rterm-transport` — Transport trait abstraction (PTY, SSH, fake) with PtySpawner
- `rterm-session` — Session management (ManagedSession, SessionManager, screen diffing, automation)
- `rterm-service` — gRPC service handlers (TerminalServer, unary + bidi streaming RPCs)
- `rterm-relay` — HTTP/3 + WebTransport relay server (gRPC service, WebTransport handler)
- `rterm-gui` — egui terminal grid widget (colors, selection, scrolling) for native demo
- `rterm-wasm` — Browser thin renderer (excluded from workspace, built with `trunk`)
- `rterm-cli` — Automation CLI (Playwright-style terminal control via gRPC)
- `rterm-agent` — SSH terminal agent: localhost gRPC server with SshPtySpawner
- `rterm-shell` — Native WebView wrapper (placeholder)

## Build

```bash
cargo build --workspace          # native crates
cargo test --workspace           # 247 tests

# WASM (separate, excluded from workspace)
cd crates/rterm-wasm
RUSTFLAGS="--cfg web_sys_unstable_apis" trunk build

# Run relay (serves WASM + WebTransport on single port)
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

## Git Rules

- Pre-commit hook runs `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test --lib`
- CI runs format, clippy, build+test, docs, WASM check
- Do not push until CI is green
