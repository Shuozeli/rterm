<!-- agent-updated: 2026-04-04T00:00:00Z -->

# Code Quality Findings

This document records findings from the code audit of 2026-04-04. Previous findings from 2026-03-31 are preserved below in the Historical section.

---

## 1. Duplication

### encode_vt_mouse duplicated in wt_handler and ws_handler
- **Location:** `crates/rterm-relay/src/wt_handler.rs:204-251`
- **Also at:** `crates/rterm-relay/src/ws_handler.rs:209-244`
- **Problem:** Identical SGR mouse encoding logic exists in two files. Any change to mouse encoding must be made in both places.
- **Fix:** Extract into a shared helper in `rterm-relay/src/` (e.g., `mouse_encoding.rs`) and import from both handlers.

### Wrapper re-exports in rterm-relay
- **Location:** `crates/rterm-relay/src/session.rs`, `crates/rterm-relay/src/screen_diff.rs`
- **Problem:** These re-export from `rterm-session` which already exports them directly. Three levels of indirection: `rterm_relay::session` → `rterm_service::session` → `rterm_session::session`.
- **Fix:** Remove the re-exports from `rterm-relay/src/session.rs` and `rterm-relay/src/screen_diff.rs`. Callers should use `rterm_session` directly.

---

## 2. Dead Code / Unused

### GridIterator never used
- **Location:** `crates/rterm-core/src/grid/mod.rs:376-402`
- **Problem:** `GridIterator` struct with `Iterator` and `DoubleEndedIterator` impls has no callers.
- **Fix:** Remove `GridIterator` if truly unused, or add a `#[cfg(test)]` guard if only used in tests.

### Underscore-prefixed variable never read
- **Location:** `crates/rterm-core/src/grid/mod.rs:103`
```rust
let _region_end = region.end.line.0 as usize;
```
- **Problem:** Value computed but never used — suggests incomplete refactoring.
- **Fix:** Remove the unused variable.

### rterm-shell is entirely placeholder
- **Location:** `crates/rterm-shell/src/lib.rs:1-3`
- **Problem:** Contains only `todo!("rterm-shell: native WebView wrapper + local PTY + WebSocket bridge")`.
- **Fix:** Either implement or remove the crate from workspace.

---

## 3. Silent Failures

### try_send results silently dropped
- **Location:** `crates/rterm-session/src/session.rs:104-107`
```rust
let _ = old_tx.try_send(ServerMsg::SessionDetached(...));
```
- **Problem:** If old client's channel is full, the `SessionDetached` notification is silently lost.
- **Fix:** Log a warning when drop occurs, or use a different strategy (close notification, metrics).

### try_send for PTY resize silently dropped
- **Location:** `crates/rterm-session/src/session.rs:119` and `crates/rterm-session/src/session.rs:185`
```rust
let _ = self.pty_resize_tx.try_send((cols, rows));
```
- **Problem:** Both silently drop if channel is full — resize signals can be lost.
- **Fix:** Consider logging or using a bounded sender with proper backpressure handling.

---

## 4. No-Op Code

### Unnecessary .map(|i| i) identity
- **Location:** `crates/rterm-session/src/timeline.rs:290-295`
```rust
.binary_search_by_key(&event_index, |s| s.event_index)
    .map(|i| i)  // <-- unnecessary map identity
    .unwrap_or_else(|i| i.saturating_sub(1));
```
- **Problem:** `.map(|i| i)` is a no-op.
- **Fix:** Remove `.map(|i| i)`.

### Comment noise describing rejected approach
- **Location:** `crates/rterm-core/src/grid/mod.rs:105-117`
- **Problem:** Extensive comments describing a ring buffer optimization that was rejected.
- **Fix:** Remove the comment block describing the abandoned approach.

---

## 5. Unsafe / Panic Patterns

### unwrap() in non-test production code
- **Location:** `crates/rterm-core/src/buffer.rs:116-117`, `crates/rterm-core/src/buffer.rs:122-123`, `crates/rterm-core/src/grid/mod.rs:338`
```rust
self.grid.cell(point).expect("cell out of bounds")
```
- **Problem:** `Cell::from_u32` in proto decode can fail, but these assume coordinates are always valid.
- **Fix:** Handle the case where coordinates may be out of bounds — return an error or clamp.

### Mixed unwrap_or vs ok().map() error handling
- **Location:** `crates/rterm-proto/src/lib.rs:727-732`
```rust
let cmd = std::str::from_utf8(params[0]).unwrap_or("");
std::str::from_u8(params[1]).ok().map(|s| s.to_string())
```
- **Problem:** Inconsistent error handling — one silently defaults, the other uses `ok().map()`.
- **Fix:** Use consistent error handling.

---

## 6. Architecture Issues

### rterm-service vs rterm-relay naming confusion
- **Location:** `crates/rterm-service/src/lib.rs`
- **Problem:** `rterm-service` is a thin wrapper that re-exports from `rterm_relay::service`. The crate names are confusing.
- **Fix:** Either remove `rterm-service` as a separate crate, or give it a clearer purpose.

### rterm-mobile in workspace but untracked
- **Location:** `Cargo.toml` workspace members
- **Problem:** `rterm-mobile/` in git status as untracked but listed in workspace.
- **Fix:** Add to git tracking or remove from workspace members.

---

## 7. Message Encoding Gaps

### ClientMsg session management messages encode as NONE
- **Location:** `crates/rterm-proto/src/lib.rs:395-406`
- **Problem:** `CreateSession`, `AttachSession`, `DestroySession`, `ListSessions` fall through to a catch-all that encodes as `NONE` body type.
- **Fix:** Implement proper FlatBuffers encoding for each session management message type.

### ServerMsg session management messages encode as NONE
- **Location:** `crates/rterm-proto/src/lib.rs:537-547`
- **Problem:** Same as above for server-side session messages.
- **Fix:** Implement proper FlatBuffers encoding for each server session message type.

---

## 8. Config / Comment Issues

### Chinese comment in config
- **Location:** `crates/rterm-relay/src/config.rs:39`
```rust
/// Transport type for the WASM client connection (，决定客户端使用哪种传输协议).
```
- **Problem:** Chinese phrase mixed with English in public documentation.
- **Fix:** Remove the Chinese portion or translate fully to English.

---

## Priority Summary (New Findings)

| Priority | Issue |
|----------|-------|
| **High** | encode_vt_mouse duplication |
| **High** | ClientMsg/ServerMsg session encoding gaps |
| **High** | unwrap() in hot paths (buffer.rs, grid/mod.rs) |
| **Medium** | try_send silent drops in session.rs |
| **Medium** | Remove wrapper re-exports |
| **Medium** | GridIterator dead code |
| **Low** | timeline.rs .map(\|i\| i) no-op |
| **Low** | Comment noise in grid/mod.rs |
| **Low** | Chinese comment in config.rs |
| **Low** | rterm-shell placeholder decision |

---

## Summary Table (All Findings)

| # | Category | Issue | Severity | Status |
|---|----------|-------|----------|--------|
| 1 | Duplication | encode_vt_mouse in wt_handler and ws_handler | High | PENDING |
| 2 | Encoding | ClientMsg/ServerMsg session messages encode as NONE | High | PENDING |
| 3 | Unsafe | unwrap() in buffer.rs and grid/mod.rs | High | PENDING |
| 4 | Silent fail | try_send drops in session.rs | Medium | PENDING |
| 5 | Architecture | Remove wrapper re-exports in rterm-relay | Medium | PENDING |
| 6 | Dead code | GridIterator never used | Medium | PENDING |
| 7 | No-op | .map(\|i\| i) in timeline.rs | Low | PENDING |
| 8 | Comment | Ring buffer comment noise in grid/mod.rs | Low | PENDING |
| 9 | Config | Chinese comment in config.rs | Low | PENDING |
| 10 | Placeholder | rterm-shell decision needed | Low | PENDING |
| 11 | Architecture | rterm-service naming confusion | Low | PENDING |
| 12 | Workspace | rterm-mobile untracked in workspace | Low | PENDING |

---

<!-- Historical findings from 2026-03-31 below -->

---

# Historical Findings (2026-03-31)

## Summary Table (Previous)

| # | Category | Issue | Severity | Status |
|---|----------|-------|----------|--------|
| 1 | Formatting | `cargo fmt` failures in 10+ files | High (CI fail) | **DONE** |
| 2 | Unsafe | `expect()` in cert generation can panic at startup | High | PENDING |
| 3 | Unsafe | `expect()` in `main.rs` inside spawned tasks | High | PENDING |
| 4 | Logic | `is_cert_valid()` ignores argument, uses file mtime | Medium | **DONE** (renamed to `is_cert_file_fresh`, removed unused param) |
| 5 | Logic | `generate_session_name()` all three random calls return same value | Medium | **DONE** (fixed with distinct salts per index) |
| 6 | Dead code | `relay_tx` channel swap in `session.rs` is confusing no-op | Low | **DONE** (changed signature to take `Receiver` by value) |
| 7 | Dead code | `tls` field in `ListenerConfig` never used | Low | PENDING |
| 8 | Clippy | `#[allow(clippy::type_complexity)]` bypasses clippy rule | Low | **DONE** (type alias `NewSessionResult` introduced) |
| 9 | Duplication | gRPC framing construction in 3 places | Low | PENDING |
| 10 | Unsafe (low) | `unwrap()` on HTTP response builders | Low | PENDING |
| 11 | No tests | `network.rs` has no tests | Low | LOW PRIORITY |
| 12 | Placeholder | `rterm-shell` is `todo!()` | N/A | SKIPPED |
| 13 | Feature gap | `osc_dispatch` is no-op stub | N/A | OUT OF SCOPE |

---

## Historical Issue Details (2026-03-31)

### Issue 2: expect() in cert generation can panic at startup
- **Location:** `crates/rterm-relay/src/tls.rs:67,72,75,123`
- **Status:** PENDING

### Issue 3: expect() in main.rs inside spawned tasks
- **Location:** `crates/rterm-relay/src/main.rs:99,112`
- **Status:** PENDING

### Issue 7: tls field in ListenerConfig never used
- **Location:** `crates/rterm-relay/src/config.rs:17`
- **Status:** PENDING

### Issue 9: gRPC framing duplicated in 3 places
- **Location:** `crates/rterm-cli/src/main.rs:51-54`, `crates/rterm-relay/tests/e2e_test.rs:61-64,94-97`
- **Status:** PENDING

### Issue 10: unwrap() on HTTP response builders
- **Location:** `crates/rterm-relay/src/https_server.rs:82,118,123`, `crates/rterm-relay/src/wt_server.rs:117`, `crates/rterm-relay/src/static_files.rs:18,33`
- **Status:** LOW PRIORITY

### Issue 11: network.rs has no tests
- **Location:** `crates/rterm-relay/src/network.rs:1-8`
- **Status:** LOW PRIORITY
