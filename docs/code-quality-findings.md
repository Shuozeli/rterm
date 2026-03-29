<!-- agent-updated: 2026-03-29T23:20:00Z -->

# Code Quality Findings

## 1. Tests Outside `#[cfg(test)]` Module (Compilation Issue)

### Tests compiled into non-test builds in buffer.rs
- **Location:** `crates/rterm-core/src/buffer.rs:785-943`
- **Problem:** 13 tests (`scrollback_cell_valid`, `scrollback_cell_out_of_bounds`, `scrollback_cols_valid`, `scrollback_cols_out_of_bounds`, `scrollback_text_valid`, `scrollback_text_out_of_bounds`, `cell_mut_modify`, `erase_in_display_mode_0`, `erase_in_display_mode_1`, `erase_in_line_mode_2`, `resize_clamps_cursor`, `scroll_up_with_scrollback`, `insert_lines_outside_region`, `delete_lines_outside_region`, `set_scroll_region_invalid`) are placed after the closing brace of `mod tests` at line 783. They are top-level `#[test]` functions without `#[cfg(test)]`, meaning they will be compiled (though not run) in non-test builds, bloating production binaries.
- **Fix:** Move all 13 tests inside the `#[cfg(test)] mod tests { ... }` block.

## 2. Dead Code

### PtySession backward-compat shim is unused
- **Location:** `crates/rterm-relay/src/pty.rs:103-113` (`PtySession`)
- **Problem:** `PtySession` is declared as a "backward compat" wrapper with a `spawn` method that delegates to `RealPtySpawner`, but is never used anywhere in the codebase. All callers use `RealPtySpawner` directly via the `PtySpawner` trait.
- **Fix:** Remove `PtySession` struct and its `impl` block.

### rterm-shell is an empty placeholder crate
- **Location:** `crates/rterm-shell/src/lib.rs:1`
- **Problem:** The entire crate is `pub fn init() { todo!(...) }` with zero tests. Per CLAUDE.md: "Never 0%: Every module must have at least one test for its public API."
- **Fix:** *Skip* -- acknowledged placeholder per CLAUDE.md crate list.
- **Status:** SKIPPED (intentional placeholder)

## 3. Inconsistent Default Behavior

### TerminalServer::default() produces empty shell string
- **Location:** `crates/rterm-relay/src/service.rs:17` (`#[derive(Default)]`) vs line 22-25 (`fn new()`)
- **Problem:** `TerminalServer` derives `Default` which sets `shell` to `""` (empty). But `new()` sets it to `"/bin/bash"`. An empty shell string would cause a spawn failure at runtime. The test at line 123 masks this with `s.shell.is_empty() || s.shell == DEFAULT_SHELL`.
- **Fix:** Implement `Default` manually to use `DEFAULT_SHELL`, making `default()` and `new()` identical. Fix the test assertion.

## 4. Unsafe Patterns (unwrap in non-test code)

### unwrap() calls in main.rs cert generation
- **Location:** `crates/rterm-relay/src/main.rs:163,167,168,178` (`generate_cert`, `extract_cert_der`)
- **Problem:** Multiple `unwrap()` calls in cert generation. A failure panics with no useful error message.
- **Fix:** Replace with `expect()` containing descriptive messages. These are startup-only calls where failure is fatal, so panic is acceptable but the message should be helpful.

### unwrap() on HTTP Response builders
- **Location:** `crates/rterm-relay/src/static_files.rs:18,33`, `crates/rterm-relay/src/https_server.rs:98,103`, `crates/rterm-relay/src/main.rs:114`
- **Problem:** `http::Response::builder()...body(()).unwrap()` -- practically infallible but violates project rules.
- **Fix:** *Low priority.* Use `expect("valid HTTP response")` for clarity.
- **Status:** SKIPPED (cosmetic)

## 5. Grid Row Cloning Performance

### Vec clone during scroll/insert/delete operations
- **Location:** `crates/rterm-core/src/buffer.rs:290,299,316,405,424`
- **Problem:** `scroll_up`, `scroll_down`, `insert_lines`, `delete_lines` all clone `Vec<Cell>` rows. Each clone heap-allocates even though `Cell` is `Copy`. For large terminals (e.g., 200+ cols), this creates allocation pressure during rapid scrolling.
- **Fix:** *Skip* -- requires grid storage refactor (ring buffer). Not worth the complexity now.
- **Status:** SKIPPED (would require grid architecture change)

## 6. Duplication

### Duplicated FlatBuffers generated code (rterm-proto vs rterm-wasm)
- **Location:** `crates/rterm-proto/src/generated/` vs `crates/rterm-wasm/src/generated/`
- **Problem:** Two copies of generated FlatBuffers code with different sizes (2931 vs 2425 lines).
- **Fix:** *Skip* -- rterm-wasm is excluded from workspace due to WASM-only constraints.
- **Status:** SKIPPED (architectural constraint)

## Summary

| Priority | Issue | Fix Effort | Status |
|----------|-------|-----------|--------|
| High | Tests outside `#[cfg(test)]` in buffer.rs | Move 13 tests | DONE |
| High | Dead `PtySession` code in pty.rs | Delete 12 lines | DONE |
| Medium | `TerminalServer::default()` inconsistency | Implement Default manually | DONE |
| Medium | `unwrap()` in `generate_cert` / `extract_cert_der` | Add `expect()` messages | DONE |
| Skip | Grid row cloning (perf) | Requires grid refactor | SKIPPED |
| Skip | HTTP builder unwrap() | Cosmetic | SKIPPED |
| Skip | WASM duplicate generated code | Architectural | SKIPPED |
| Skip | rterm-shell placeholder | Intentional | SKIPPED |
