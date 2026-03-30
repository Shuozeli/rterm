<!-- agent-updated: 2026-03-30T01:30:00Z -->

# Session Management Design

## Overview

Enable reconnection after disconnect, multiple concurrent sessions per relay, and optionally session persistence across relay restarts.

## Current State

Each WebTransport connection = one PTY session. No identity, no persistence, no reattach. When the client disconnects, everything is torn down.

## Session Lifecycle

```
         create
  [None] -------> [Active/Attached]
                     |         ^
              detach |         | attach
                     v         |
                  [Active/Detached]
                     |
              timeout|  or  destroy
                     v
                  [Dead]
```

## Session Identity

- **Session ID**: UUID v4 (unguessable, collision-free)
- **Session Token**: 256-bit random (base64url, 43 chars). Bearer credential for reattaching.
- Server stores SHA-256 hash of token (never plaintext).
- Client stores `{ session_id, token }` in `localStorage`.

## Architecture

### SessionManager

```
SessionManager {
    sessions: DashMap<SessionId, Arc<Mutex<ManagedSession>>>,
}

ManagedSession {
    id, token_hash, state, created_at, last_activity, shell, cols, rows,
    terminal: Terminal,           // kept alive across detach
    pty_handle: Option<PtyHandle>,
    client_tx: Option<Sender<ServerMsg>>,  // None if detached
    pty_exited: Option<i32>,
}
```

### Session Output Loop (per session, independent of client)

Each session spawns a long-lived task that reads PTY stdout and feeds it through Terminal. This runs regardless of client connection:

- **Attached**: diff and send ScreenUpdate to client_tx
- **Detached**: just feed Terminal (no diff needed, no buffering)
- **On reattach**: send fresh ScreenSnapshot from current Terminal state

This matches tmux/screen behavior — client jumps to current state, no replay.

### Transport Adapters (thin)

wt_handler and service become thin bridges:
- Parse first message as CreateSession/AttachSession/Resize
- Set/clear client_tx on attach/detach
- Forward input to PTY via session handle

## Protocol Changes

### New Client Messages

| Message | Purpose |
|---------|---------|
| `CreateSession { shell, cols, rows }` | Create a new session |
| `AttachSession { session_id, token, cols, rows }` | Reconnect to existing |
| `DetachSession {}` | Explicit detach (keep alive) |
| `DestroySession { session_id }` | Kill session |
| `ListSessions { tokens }` | List sessions client has access to |

### New Server Messages

| Message | Purpose |
|---------|---------|
| `SessionCreated { session_id, token }` | Response to create (token sent once) |
| `SessionAttached { session_id }` | Confirm attach |
| `SessionDetached { session_id }` | Client was displaced |
| `SessionDestroyed { session_id }` | Session killed |
| `SessionList { sessions }` | List of accessible sessions |

### Backward Compatibility

If the first message is a bare `Resize` (no session command), the server creates an anonymous session (no token, no reattach). Existing clients work unchanged.

## Connection Flow

### New Session
```
Client -> CreateSession { cols: 80, rows: 24 }
Server -> SessionCreated { id: "abc-123", token: "xyz..." }
Server -> ScreenSnapshot { ... }
         (normal terminal I/O)
```

### Reconnection
```
Client -> AttachSession { id: "abc-123", token: "xyz...", cols: 80, rows: 24 }
Server -> SessionAttached { id: "abc-123" }
Server -> ScreenSnapshot { ... current state ... }
         (normal terminal I/O resumes)
```

## Security

- Token = sole credential. 256-bit random, sent once at creation.
- Single-attach with takeover: new attach displaces old client (matches tmux).
- ListSessions requires presenting tokens to prove access.
- All over QUIC/TLS.

## Timeouts

| Timeout | Default | Purpose |
|---------|---------|---------|
| Detached session | 30 min | Kill PTY if no reattach |
| Max session lifetime | 24 hours | Safety valve |
| Reaper interval | 60 sec | Background cleanup task |

## Detached Behavior

While detached:
1. PTY output feeds Terminal (keeps screen state current)
2. No ScreenUpdate diffs computed (saves CPU)
3. No output buffered (saves memory)
4. On reattach: fresh ScreenSnapshot sent

## Implementation Phases

### Phase 1: SessionManager Infrastructure
- New FlatBuffers messages + Rust types
- SessionManager + ManagedSession structs
- Session output loop (PTY -> Terminal, independent of client)
- Create, attach, detach, destroy, timeout reaper

### Phase 2: Transport Integration
- wt_handler accepts SessionManager, handles session commands
- Backward compat (bare Resize = anonymous session)
- main.rs creates and shares SessionManager

### Phase 3: Client Reconnection
- WASM stores tokens in localStorage
- On page load: AttachSession if token exists, else CreateSession
- Reconnection with exponential backoff (1s, 2s, 4s, 8s, max 30s)
- "Reconnecting..." UI

### Phase 4: Multi-Session (P1)
- Session picker UI
- ListSessions implementation
- Switch sessions (detach + attach)
- Human-readable session labels

### Phase 5: Persistence (P2, stretch)
- Serialize Terminal state to ~/.config/rterm/sessions/
- Restore tombstone sessions on restart
- Requires Serialize/Deserialize on rterm-core types

## Key Decisions

1. **No output buffering while detached** — fresh snapshot on reattach (matches tmux)
2. **Single-attach with takeover** — new attach displaces old (covers reconnection)
3. **Token-based auth** — no user accounts needed for personal relay
4. **Session loop is a separate task** — clean separation from transport
5. **Backward-compatible** — bare Resize as first message still works

## Files to Modify

| File | Changes |
|------|---------|
| `rterm.fbs` | Add session management messages |
| `rterm-proto/src/lib.rs` | Add Rust types + encode/decode |
| `rterm-relay/src/session_manager.rs` | New: SessionManager |
| `rterm-relay/src/managed_session.rs` | New: ManagedSession + session loop |
| `rterm-relay/src/wt_handler.rs` | Accept SessionManager, handle session commands |
| `rterm-relay/src/main.rs` | Create SessionManager, start reaper |
| `rterm-wasm/src/lib.rs` | localStorage tokens, reconnection logic |
