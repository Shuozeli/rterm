<!-- agent-updated: 2026-03-31T00:00:00Z -->

# Code Quality Findings

This document records findings from the code audit of 2026-03-31. Previous resolved issues are retained at the bottom as historical record.

---

## 1. Formatting (Highest — CI Will Fail)

### Many files unformatted
- **Location:** `crates/rterm-cli/src/main.rs`, `crates/rterm-proto/src/lib.rs`, `crates/rterm-proto/src/generated/rterm_generated.rs`, `crates/rterm-relay/src/{config,lib,main,service,session_manager,tls}.rs`, `crates/rterm-relay/tests/e2e_test.rs`
- **Problem:** `cargo fmt -- --check` exits non-zero. The pre-commit hook and CI both run `cargo fmt --check`, so any push with these files would be blocked.
- **Fix:** Run `cargo fmt`. All files are now formatted. **DONE**

---

## 2. Unsafe Patterns (Non-test `unwrap()` / `expect()`)

### `#[allow(clippy::type_complexity)]` bypasses clippy
- **Location:** `crates/rterm-relay/src/managed_session.rs:47`
- **Problem:** `ManagedSession::new` returns `Result<(Self, mpsc::Receiver<Vec<u8>>), Box<dyn Error>>`. Clippy flags the return type as complex but the fix here is not to suppress it, it is to introduce a named type alias.
- **Fix:** Replace the `#[allow]` with a type alias: `type NewSessionResult = Result<(ManagedSession, mpsc::Receiver<Vec<u8>>), Box<dyn std::error::Error + Send + Sync>>;`
- **Status:** PENDING

### `expect()` in `tls.rs` certificate generation panics at startup
- **Location:** `crates/rterm-relay/src/tls.rs:67,72,75,123`
- **Problem:** `generate_fresh_cert` uses three `.expect()` calls for TLS operations that could fail (e.g., due to FIPS restrictions, OS entropy issues). `extract_cert_der` panics with an index out-of-bounds if `certs` is empty (line 124: `certs[0]`). These run at server startup; a panic here crashes the whole binary with no recovery path.
- **Fix:** Return `Result` from `generate_fresh_cert` and `extract_cert_der`, propagate errors to `main`.
- **Status:** PENDING

### `expect()` in `main.rs` TLS server startup
- **Location:** `crates/rterm-relay/src/main.rs:99,112`
- **Problem:** `.expect("tls config")` and `.expect("bind h3")` panic inside `tokio::spawn` closures. A panic in a spawned task does not propagate to the parent — it silently crashes that protocol listener (gRPC H2 or H3) while the rest of the server continues running unnoticed.
- **Fix:** Replace with `?` after converting the closures to `async fn` helpers that return `Result`.
- **Status:** PENDING

### `unwrap()` in non-test `https_server.rs` and `wt_server.rs` HTTP response builders
- **Location:** `crates/rterm-relay/src/https_server.rs:82,118,123`, `crates/rterm-relay/src/wt_server.rs:117`, `crates/rterm-relay/src/static_files.rs:18,33`
- **Problem:** `http::Response::builder()...body(()).unwrap()` — practically infallible, but any future header addition that fails would panic silently.
- **Fix:** Replace with `.expect("valid HTTP response")` for clarity. Low priority.
- **Status:** LOW PRIORITY

---

## 3. Dead / No-op Code

### `relay_tx` created and immediately dropped in `session.rs`
- **Location:** `crates/rterm-relay/src/session.rs:89,123`
- **Problem:** In `run_session`, the input-forwarding task is built by creating `(relay_tx, relay_rx)`, swapping `relay_rx` into `client_rx`, spawning a task that reads from the original `client`, and then immediately `drop(relay_tx)` at line 123. The `relay_tx` is never sent any messages — it exists only to give the task a channel that closes immediately, unblocking the task's `recv()` when done. This is a confusing round-trip: `relay_tx` is dropped right after creation, causing `relay_rx.recv()` to return `None` immediately in the outer loop. The outer `tokio::select!` at line 128 then only polls `stdout_rx`, never `relay_rx`. This means the outer loop ignores resize events sent by the input task to `resize_tx`. The design works, but the `relay_tx/relay_rx` swap is dead complexity — the outer loop never reads `relay_rx`.
- **Fix:** Remove the channel swap entirely. The outer loop only needs `stdout_rx`. The input task reads from the original `client_rx` directly, which is what happens logically after the swap anyway. Simplify to just `tokio::spawn` the input forwarding with the original `client_rx`.
- **Status:** PENDING

### `tls` field in `ListenerConfig` is read but never used
- **Location:** `crates/rterm-relay/src/config.rs:17`
- **Problem:** `ListenerConfig` has a `tls: bool` field deserialized from `rterm.toml`, but the relay always uses TLS for all listeners regardless of this flag. The field is deserialized (so parsing fails if absent) but never checked at runtime.
- **Fix:** Either remove the field (simplify the config schema) or actually honor it (allow plaintext gRPC H2 for testing). The intent is unclear.
- **Status:** PENDING

### `network.rs` is a one-function module with no tests
- **Location:** `crates/rterm-relay/src/network.rs:1-8`
- **Problem:** `get_lan_ip()` runs `hostname -I` as a subprocess. It has no tests. Per CLAUDE.md: "Never 0%: Every module must have at least one test." Also this function is effectively unreachable for testing (runs a system command).
- **Fix:** Add at least one smoke test that verifies the function returns `Some` or `None` without panicking on the current host. Or inline the call into `wt_server.rs` since it is called in one place.
- **Status:** LOW PRIORITY

---

## 4. Duplication

### gRPC framing logic duplicated in `rterm-cli/src/main.rs` and `rterm-relay/tests/e2e_test.rs`
- **Location:** `crates/rterm-cli/src/main.rs:51-54` and `crates/rterm-relay/tests/e2e_test.rs:61-64` and `crates/rterm-relay/tests/e2e_test.rs:94-97`
- **Problem:** The 5-byte gRPC framing (1-byte compression flag + 4-byte big-endian length) is manually constructed in three places. Any protocol change (e.g., adding compression) must be updated in all three spots.
- **Fix:** Extract a shared `encode_grpc_frame(payload: &[u8]) -> Vec<u8>` helper into `rterm-proto` or `rterm-cli` lib. The test file could import it from the CLI crate or proto crate.
- **Status:** LOW PRIORITY

### `plain_text` construction duplicated between `service.rs` and CLI
- **Location:** `crates/rterm-relay/src/service.rs:108-116`
- **Problem:** `GetSnapshotSvc::call` manually constructs `plain_text` by iterating over snapshot rows and trimming. However, `GetSnapshotResponse` already carries `plain_text` as a field. The construction logic here could be factored into a helper on `ScreenSnapshotData`.
- **Fix:** Add a `to_plain_text()` method on `ScreenSnapshotData` in `rterm-proto` and call it from the service.
- **Status:** LOW PRIORITY

---

## 5. Logic / Correctness

### `is_cert_valid()` ignores its `cert_pem` argument — uses file mtime instead
- **Location:** `crates/rterm-relay/src/tls.rs:85`
- **Problem:** The function is declared `fn is_cert_valid(_cert_pem: &[u8]) -> bool` but ignores the cert bytes entirely. It checks the file's modification time on disk instead. This means:
  1. If the cert was copied from elsewhere (mtime newer than content creation), validity is wrong.
  2. The function always opens the same hardcoded path regardless of the input, making it untestable and misleading.
- **Fix:** Either parse the cert's `notAfter` field directly from `cert_pem` using `x509-cert` or `rcgen`, or rename the function to `is_cert_file_fresh()` and update the signature to take no argument.
- **Status:** PENDING

### `generate_session_name()` uses `RandomState` as RNG — not random
- **Location:** `crates/rterm-relay/src/session_manager.rs:147-163`
- **Problem:** `RandomState::new()` seeds a HashMap hasher which is not a CSPRNG. The seed is randomized per process startup (using OS randomness), but within a single process all calls use the same seed prefix. Calling `rand()` three times with `h.write_u8(0)` will produce the same value all three times (same hasher, same input). This means all three indices will be the same within one invocation, giving names like `swift-swift-N`. Worse, session names across a process run may be predictable.
- **Fix:** Use `rand::random()` or `std::collections::hash_map::DefaultHasher` with different inputs, or simply use `uuid::Uuid::new_v4().to_string()`.
- **Status:** PENDING

---

## 6. Placeholder / Incomplete

### `rterm-shell` is a `todo!()` crate
- **Location:** `crates/rterm-shell/src/lib.rs:1-3`
- **Problem:** Entire crate is a single `todo!()` call. Zero tests. Registered in workspace `Cargo.toml`.
- **Fix:** Acknowledged placeholder per CLAUDE.md. No action needed.
- **Status:** SKIPPED (intentional placeholder)

### `osc_dispatch` is a no-op stub in `terminal.rs`
- **Location:** `crates/rterm-core/src/terminal.rs:438-440`
- **Problem:** `osc_dispatch` handles OSC sequences (window title, hyperlinks, clipboard) with a comment `// TODO: handle OSC 0/2, OSC 8, OSC 52`. Window title (OSC 2) is a commonly used sequence in terminal sessions. Without it, the browser client never receives a title update.
- **Fix:** Implement at minimum OSC 0/2 (set `title` in screen state) and surface it in `ScreenSnapshotData`. The infrastructure for the `title` field already exists in `ScreenSnapshotData`.
- **Status:** OUT OF SCOPE for this audit (feature work)

---

## 7. Architecture

### `#[allow(clippy::type_complexity)]` on `ManagedSession::new`
- **See section 2.** Named type alias would remove the suppression entirely.

---

## Summary Table

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

## Historical Findings (Previous Audit — Resolved)

| Issue | Status |
|-------|--------|
| Tests outside `#[cfg(test)]` in buffer.rs | DONE |
| Dead `PtySession` code in pty.rs | DONE |
| `TerminalServer::default()` inconsistency | DONE |
| `unwrap()` in `generate_cert`/`extract_cert_der` | DONE (previously) |
| Grid row cloning (perf) | SKIPPED |
| HTTP builder unwrap() (cosmetic) | SKIPPED |
| WASM duplicate generated code | SKIPPED |
