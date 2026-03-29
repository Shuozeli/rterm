# Code Quality Findings

## 1. Duplication

### Duplicated FlatBuffers generated code between rterm-proto and rterm-wasm
- **Location:** `crates/rterm-proto/src/generated/rterm_generated.rs` (1031 lines)
- **Also at:** `crates/rterm-wasm/src/generated/rterm_generated.rs` (1031 lines, identical)
- **Problem:** The same generated FlatBuffers code is vendored in two crates. If the schema changes, both must be updated in sync or they drift.
- **Fix:** This is intentional -- rterm-wasm is excluded from the workspace (WASM-only) and cannot depend on rterm-proto which pulls in grpc-core (non-WASM). Low priority, accepted.
- **Status:** SKIPPED (architectural constraint)

### Duplicated FlatBuffers encoding logic in rterm-wasm/messages.rs vs rterm-proto/lib.rs
- **Location:** `crates/rterm-wasm/src/messages.rs:8-41` (encode_resize, encode_data_in)
- **Also at:** `crates/rterm-proto/src/lib.rs:62-89` (ClientMsg::encode_flatbuffer)
- **Problem:** The WASM crate re-implements the same FlatBuffers encoding that rterm-proto already provides, because rterm-wasm cannot depend on rterm-proto (grpc-core is not WASM-compatible).
- **Fix:** Same as above -- accepted due to WASM isolation. Low priority.
- **Status:** SKIPPED (architectural constraint)

### Duplicated path resolution and content-type guessing in static file serving
- **Location:** `crates/rterm-relay/src/static_files.rs:43-56` (resolve_path, guess_content_type)
- **Also at:** `crates/rterm-relay/src/https_server.rs:72-94` (inline path resolution, inline content-type guessing)
- **Problem:** Path traversal prevention and content-type mapping are duplicated between HTTP/3 and HTTPS servers. The HTTPS version is missing "png" and "ico" content types that the H3 version has.
- **Fix:** Extract the shared path resolution and content-type logic from `static_files.rs` and reuse it in `https_server.rs`. [DONE]

### Duplicated certificate generation logic
- **Location:** `crates/rterm-relay/src/main.rs:150-169` (generate_cert)
- **Also at:** `crates/rterm-relay/tests/pty_session.rs:26-36` (generate_cert)
- **Problem:** Near-identical self-signed cert generation in both locations. The test version has slightly different variable names but does the same thing.
- **Fix:** Move `generate_cert` to `rterm_relay::lib` or a `tls` module and reuse in both `main.rs` and tests. Low priority since the test version is only in tests.
- **Status:** SKIPPED (test helper duplication is low priority)

## 2. Dead Code / Placeholder Modules

### rterm-shell is an empty placeholder crate
- **Location:** `crates/rterm-shell/src/lib.rs:1-3`
- **Problem:** The entire crate contains only `pub fn init() { todo!("...") }`. It has no dependencies, no tests, and no real implementation. It is included in the workspace members.
- **Fix:** Low priority -- this is a planned future crate. Mark with a comment or consider removing from workspace until implemented.
- **Status:** SKIPPED (intentional placeholder for future work)

### Unused `_connected` field in demo app
- **Location:** `crates/rterm-gui/examples/demo.rs:41` (`_connected: bool`)
- **Problem:** The field is set to `false` at construction and never read or updated. The leading underscore suppresses warnings but the field is truly dead.
- **Fix:** Remove the `_connected` field from `TerminalApp`. [DONE]

### Unused `grid` variable in demo app
- **Location:** `crates/rterm-gui/examples/demo.rs:90`
- **Problem:** `let grid = terminal_grid(...)` result is unused. Clippy warns about this.
- **Fix:** Prefix with underscore: `let _grid = terminal_grid(...)`. [DONE]

## 3. Clippy Warnings (Compile-time Issues)

### Color::Default impl can be derived
- **Location:** `crates/rterm-core/src/color.rs:16-20`
- **Problem:** Clippy reports the manual `Default` impl is derivable. The `Default` variant happens to be named `Default`, so `#[derive(Default)]` with `#[default]` works.
- **Fix:** Replace the manual impl with `#[derive(Default)]` and `#[default]` on the `Default` variant. [DONE]

### OR pattern can be a range
- **Location:** `crates/rterm-core/src/terminal.rs:276`
- **Problem:** `0x0A | 0x0B | 0x0C` can be written as `0x0A..=0x0C`.
- **Fix:** Change to range pattern. [DONE]

### Collapsible if in input.rs
- **Location:** `crates/rterm-gui/src/input.rs:8-22`
- **Problem:** Nested `if modifiers.ctrl { if let Some(ch) = ... }` can be collapsed.
- **Fix:** Collapse to `if modifiers.ctrl && let Some(ch) = key_to_char(key)`. [DONE]

### Missing Default impl for TerminalServer
- **Location:** `crates/rterm-relay/src/service.rs:21-25`
- **Problem:** `TerminalServer::new()` exists but no `Default` impl, which clippy warns about.
- **Fix:** Add `impl Default for TerminalServer`. [DONE]

### Needless borrow in main.rs
- **Location:** `crates/rterm-relay/src/main.rs:25`
- **Problem:** `STANDARD.encode(&hash)` -- the `&` is unnecessary since `hash` already implements the required trait.
- **Fix:** Change to `STANDARD.encode(hash)`. [DONE]

## 4. Silent Failures

### erase_in_display and erase_in_line silently ignore unknown modes
- **Location:** `crates/rterm-core/src/buffer.rs:356-357` and `buffer.rs:380-381`
- **Problem:** The `_ => {}` catch-all arms silently ignore unknown erase modes. While this is standard for terminal emulators (unknown sequences are ignored), there is no logging or debug assertion.
- **Fix:** Acceptable for a VT emulator. Low priority, standard behavior.
- **Status:** SKIPPED (standard VT emulator behavior)

## 5. Performance

### Inefficient scrollback trimming with `remove(0)` on Vec
- **Location:** `crates/rterm-core/src/buffer.rs:323-326` (trim_scrollback)
- **Problem:** `self.scrollback.remove(0)` is O(n) for each removal because it shifts all elements. With max_scrollback of 10,000 lines, this can be slow when many lines are trimmed at once.
- **Fix:** Use `VecDeque` instead of `Vec` for `scrollback`, which provides O(1) `pop_front()`. Alternatively, use `drain(..excess)` to remove all excess lines in one operation. [DONE]

### Unnecessary `.clone()` in scroll operations
- **Location:** `crates/rterm-core/src/buffer.rs:289` (scroll_up), `buffer.rs:298` (scroll_up), `buffer.rs:313` (scroll_down)
- **Problem:** `self.grid[row] = self.grid[row + n].clone()` clones rows during scrolling. Since `Cell` is `Copy`, the clone is equivalent to copy, but the allocation of the `Vec` itself is still duplicated. This could be done with `swap` + clear patterns to avoid allocations.
- **Fix:** Low priority -- the Vec allocation is the real cost, but fixing requires restructuring the grid storage (e.g., ring buffer). Not worth the complexity now.
- **Status:** SKIPPED (optimization would require grid refactor)

## 6. Unsafe Patterns

### `unwrap()` calls in non-test relay code
- **Location:** `crates/rterm-relay/src/main.rs:163` (`CertificateParams::new(...).unwrap()`)
- **Location:** `crates/rterm-relay/src/main.rs:167` (`KeyPair::generate_for(...).unwrap()`)
- **Location:** `crates/rterm-relay/src/main.rs:168` (`params.self_signed(...).unwrap()`)
- **Location:** `crates/rterm-relay/src/main.rs:175-176` (cert parsing unwraps)
- **Problem:** Several `unwrap()` calls in cert generation and parsing. If the crypto library fails, the server panics without a clear error message.
- **Fix:** These are startup-only calls where failure is fatal, so `unwrap()` is acceptable. Could be improved with `.expect("meaningful message")` but low priority.
- **Status:** SKIPPED (startup-time panics are acceptable with expect messages)

## 7. Missing Abstractions

### Manual `Clone` impl for TerminalServer
- **Location:** `crates/rterm-relay/src/service.rs:34-40`
- **Problem:** Manual `Clone` implementation that just clones the `shell` field. This can be derived.
- **Fix:** Replace with `#[derive(Clone)]` on `TerminalServer`. [DONE]

## 8. Code Style (Low Priority)

### `and_then(|w| Some(...))` should be `.map()`
- **Location:** `crates/rterm-wasm/src/lib.rs:258`
- **Problem:** `web_sys::window().and_then(|w| Some(w.location()))` should be `.map(|w| w.location())`.
- **Fix:** This is in the WASM crate which is excluded from the workspace, but should be fixed if that crate is touched.
- **Status:** SKIPPED (WASM crate not in workspace build)
