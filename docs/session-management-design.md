<!-- agent-updated: 2026-04-09T22:00:00Z -->

# Session Management Design

## Overview

Enable reconnection after disconnect, multiple concurrent sessions per relay, and session resurrection after relay restart.

Informed by zellij's architecture: server is a state holder, clients are disposable consumers.

## Current State

Each WebTransport connection = one PTY session. No identity, no persistence, no reattach. When the client disconnects, everything is torn down.

## Design Principles (from zellij analysis)

1. **PTYs are server-side entities, independent of clients.** Clients are just input producers and output consumers. The session output loop runs regardless of client connection state.

2. **Reconnection is trivial.** Client connects, presents a token, gets a fresh screen snapshot. No replay, no diff history. Just "here's what the screen looks like now."

3. **Session state has two layers.** Lightweight metadata (who's connected, session ID) is separate from heavy state (Terminal, PTY, screen buffers). This allows quick client add/remove without touching session internals.

4. **Bounded channels for backpressure.** Slow clients get dropped, not queued infinitely. The server never blocks on a slow consumer.

5. **Single-attach with takeover.** New attach displaces old client. This handles "my tab crashed, I want back in" without complexity.

## Session Lifecycle

```
         create
  [None] -------> [Active/Attached]
                     |         ^
              detach |         | attach (takeover)
                     v         |
                  [Active/Detached]
                     |
              timeout|  or  destroy
                     v
                  [Dead] --------> [Resurrectable]
                                    (if state was serialized)
```

## Session Identity

- **Session Name**: Human-readable string (e.g., "dev", "deploy"). Auto-generated if not provided.
- **Session ID**: UUID v4 (internal, unguessable).
- **Session Token**: 256-bit random (base64url, 43 chars). Bearer credential for reattaching.
- Server stores SHA-256 hash of token (never plaintext).
- Client stores `{ session_id, session_name, token, relay_url }` in `localStorage`.

Why names + IDs: Names are for humans (`rterm attach dev`). IDs are for machines (unique, no collisions). Tokens are for auth (prove you own this session).

## Architecture

> **Note:** Session management was moved from `rterm-relay` to the `rterm-session` crate.
> - `crates/rterm-session/src/manager.rs` -- SessionManager
> - `crates/rterm-session/src/session.rs` -- ManagedSession, session_output_loop
> - `crates/rterm-relay/src/session_manager.rs` -- thin re-export for backward compat
> - `crates/rterm-relay/src/managed_session.rs` -- thin re-export for backward compat

### Two-Layer State (inspired by zellij)

```
SessionManager
├── sessions: HashMap<SessionId, Arc<tokio::Mutex<ManagedSession>>>
└── reaper_task: background timeout cleanup

ManagedSession (heavy, per-session)
├── metadata
│   ├── id: SessionId
│   ├── name: String
│   ├── token_hash: [u8; 32]
│   ├── state: Attached | Detached | Dead
│   ├── created_at, last_activity
│   ├── shell: String
│   ├── cols, rows: u16
│   └── title: Option<String>
├── terminal: Terminal            // VT emulator, kept alive across detach
├── prev_screen: PrevScreen       // rebuilt on each attach
├── pty_stdout_rx: Receiver       // moved into session loop task
├── pty_stdin_tx: Sender          // shared with attached client
├── pty_resize_tx: Sender         // shared with attached client
├── client_tx: Option<Sender<ServerMsg>>  // None if detached
└── pty_exited: Option<i32>       // set when PTY dies
```

### Session Output Loop (per session, independent of client)

Each session spawns a **long-lived tokio task** that reads PTY stdout and feeds it through Terminal. This is the core insight from zellij: the output loop is NOT owned by the client connection.

```rust
async fn session_output_loop(session: Arc<Mutex<ManagedSession>>) {
    loop {
        // 1. Read PTY stdout (outside lock)
        let data = stdout_rx.recv().await;

        // 2. Lock session, feed terminal
        let mut s = session.lock().await;
        s.terminal.feed(&data);
        s.last_activity = Instant::now();

        // 3. If client attached and not in sync mode, diff and send
        if !s.terminal.is_sync_mode() {
            if let Some(client_tx) = &s.client_tx {
                if let Some(update) = s.prev_screen.diff(s.terminal.screen()) {
                    // Bounded channel — if client is slow, drop the update
                    let _ = client_tx.try_send(ServerMsg::ScreenUpdate(update));
                }
            }
            // If detached: Terminal is updated, nothing else to do.
        }
    }
}
```

Key differences from current `run_session`:
- The loop does NOT own the PTY channels — they're in ManagedSession
- The loop does NOT handle client input — that's the transport adapter's job
- The loop uses `try_send` (bounded, non-blocking) — slow clients don't block the server
- The loop survives client disconnect — it only stops when PTY exits

### Transport Adapters (thin, stateless)

wt_handler and service become thin bridges:

```
Client connects -> parse first message:
  CreateSession  -> SessionManager.create() -> attach
  AttachSession  -> SessionManager.attach(id, token) -> attach
  Resize (legacy) -> SessionManager.create_anonymous() -> attach

While attached:
  KeyInput     -> session.pty_stdin_tx.send(data)
  PasteInput   -> session.pty_stdin_tx.send(bracketed)
  Resize       -> session.pty_resize_tx.send(cols, rows)
  DetachSession -> session.detach()
  DestroySession -> SessionManager.destroy(id)

On disconnect (network drop):
  session.detach()  // PTY stays alive, Terminal keeps updating
```

## Protocol Changes

### New Client Messages

| Message | Purpose |
|---------|---------|
| `CreateSession { name, shell, cols, rows }` | Create a named session |
| `AttachSession { session_id, token, cols, rows }` | Reconnect to existing |
| `DetachSession {}` | Explicit detach (keep alive) |
| `DestroySession { session_id }` | Kill session and PTY |
| `ListSessions { tokens }` | List accessible sessions |

### New Server Messages

| Message | Purpose |
|---------|---------|
| `SessionCreated { session_id, name, token }` | Token sent once at creation |
| `SessionAttached { session_id, name }` | Confirm attach |
| `SessionDetached { reason }` | You were displaced or server detached you |
| `SessionDestroyed { session_id }` | Session killed |
| `SessionList { sessions: [SessionInfo] }` | Accessible session list |

### SessionInfo

```
SessionInfo {
    session_id, name, shell,
    created_at, last_activity,  // unix timestamps
    attached: bool,
    cols, rows,
    title: Option<String>,
}
```

### Backward Compatibility

If the first message is a bare `Resize` (no session command), the server creates an anonymous session (no token, no reattach, no name). Existing clients work unchanged.

## Connection Flow

### New Session
```
Client -> CreateSession { name: "dev", cols: 120, rows: 40 }
Server -> SessionCreated { id: "abc-123", name: "dev", token: "xyz..." }
Server -> ScreenSnapshot { full screen state }
         (normal terminal I/O)
Client stores { id, name, token } in localStorage
```

### Reconnection (page reload, network drop)
```
Client reads { id, token } from localStorage
Client -> AttachSession { id: "abc-123", token: "xyz...", cols: 120, rows: 40 }
Server -> SessionAttached { id: "abc-123", name: "dev" }
Server -> ScreenSnapshot { current screen state }
         (terminal I/O resumes instantly)
```

### Takeover (old tab still connected)
```
New client -> AttachSession { id: "abc-123", token: "xyz..." }
Server -> to old client: SessionDetached { reason: "displaced" }
Server -> to new client: SessionAttached { id: "abc-123" }
Server -> to new client: ScreenSnapshot { ... }
```

## Security

- **Token = sole credential.** 256-bit random, sent once at creation, stored client-side.
- **Server stores hash only.** SHA-256 of token. Token never on disk.
- **Single-attach with takeover.** Matches zellij/tmux. New attach displaces old.
- **ListSessions requires tokens.** Client sends tokens it holds, server returns matching sessions.
- **Transport: QUIC/TLS.** Tokens never in plaintext outside TLS.

## Timeouts

| Timeout | Default | Purpose |
|---------|---------|---------|
| Detached session | 30 min | Kill PTY if no reattach |
| Max session lifetime | 24 hours | Safety valve |
| Reaper interval | 60 sec | Background cleanup |
| Channel capacity | 64 msgs | Backpressure (try_send, drop if full) |

## Detached Behavior

While detached:
1. Session output loop continues running
2. PTY output feeds Terminal (screen state stays current)
3. No ScreenUpdate diffs sent (no client to receive)
4. No output buffered (saves memory — just keep Terminal current)
5. On reattach: rebuild PrevScreen, send fresh ScreenSnapshot

This matches zellij and tmux behavior — instant jump to current state.

## Session Resurrection (P2, stretch)

### What zellij does
Serializes tab/pane layout + terminal contents to `~/.cache/zellij/`. On resurrection, spawns new PTYs but restores the visual state.

### What we'll do
On graceful shutdown (SIGTERM):
1. Serialize each session's `Terminal` state to `~/.config/rterm/sessions/<id>.bin`
2. Requires `Serialize`/`Deserialize` on rterm-core types (Cell, ScreenBuffer, Color, etc.)

On restart:
1. Load serialized sessions as "tombstone" sessions (screen visible, no live PTY)
2. On client reattach: show restored screen, optionally spawn fresh PTY
3. Scrollback history preserved from before restart

### What can't be preserved
- PTY process (OS limitation)
- VTE parser internal state (not serializable, but a fresh one works fine)

## Implementation Phases

### Phase 1: SessionManager + ManagedSession
- `crates/rterm-session/src/manager.rs`: create, attach, detach, destroy, list, reaper
- `crates/rterm-session/src/session.rs`: ManagedSession struct, session output loop task
- New FlatBuffers messages + Rust types in rterm-proto
- Unit tests with FakePtySpawner

### Phase 2: Transport Integration
- wt_handler accepts `Arc<SessionManager>`, dispatches session commands
- main.rs creates SessionManager, starts reaper task
- Backward compat: bare Resize = anonymous session
- Integration tests: create, attach, detach, timeout

### Phase 3: Client Reconnection
- WASM stores tokens in localStorage
- On page load: AttachSession if token exists, else CreateSession
- Reconnection with exponential backoff (1s, 2s, 4s, 8s, max 30s)
- "Reconnecting..." overlay in UI
- Handle SessionDetached (displaced by another tab)

### Phase 4: Multi-Session UI
- Session picker in WASM client
- ListSessions request/response
- Switch sessions (detach current + attach different)
- Human-readable session names
- Session labels/metadata

### Phase 5: Resurrection (stretch)
- Add serde to rterm-core types
- Graceful shutdown handler (SIGTERM)
- Serialize Terminal state to disk
- Load tombstone sessions on restart

## Key Decisions

1. **No output buffering while detached** — fresh snapshot on reattach (matches zellij/tmux)
2. **Single-attach with takeover** — new attach displaces old (handles crash recovery)
3. **Token-based auth** — no user accounts for personal relay
4. **Session output loop is independent** — runs regardless of client (core zellij insight)
5. **Bounded channels with try_send** — slow clients dropped, server never blocks
6. **Backward-compatible** — bare Resize still creates anonymous session
7. **Two-layer state** — lightweight metadata separated from heavy Terminal/PTY state
8. **Names + IDs** — names for humans, IDs for machines, tokens for auth

## Files to Create/Modify

| File | Changes |
|------|---------|
| `rterm.fbs` | Add session management messages to ClientBody/ServerBody |
| `rterm-proto/src/lib.rs` | Add Rust types + encode/decode for session messages |
| `crates/rterm-session/src/manager.rs` | **Moved**: SessionManager (create/attach/detach/destroy/list/reaper) — was `rterm-relay/src/session_manager.rs` |
| `crates/rterm-session/src/session.rs` | **Moved**: ManagedSession + session output loop — was `rterm-relay/src/managed_session.rs` |
| `rterm-relay/src/wt_handler.rs` | Accept Arc<SessionManager>, dispatch session commands |
| `rterm-relay/src/main.rs` | Create SessionManager, start reaper, pass to handlers |
| `crates/rterm-relay/src/lib.rs` | Re-exports from `rterm_session` for backward compat |
| `rterm-wasm/src/lib.rs` | localStorage tokens, reconnection, session picker |
| `rterm-wasm/src/messages.rs` | Encode/decode session message types |
