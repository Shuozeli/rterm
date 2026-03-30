<!-- agent-updated: 2026-03-30T05:10:00Z -->

# Postmortem: Scrollback Not Working in Browser

**Date:** 2026-03-30
**Severity:** User-facing feature broken
**Duration:** ~2 hours debugging + fixing
**Status:** Resolved

## Summary

Mouse wheel scrolling in the browser terminal did not work. Users could not scroll back through terminal history. The root cause was a chain of 5 independent bugs that all had to be fixed for scrollback to function.

## Timeline

1. User reports "scroll doesn't work" in Chrome
2. Investigation reveals scrollback was implemented for the old raw-bytes protocol but never wired into the new session-managed architecture
3. Five bugs identified and fixed sequentially

## Root Causes

### Bug 1: ScrollbackRequest not handled in wt_handler

**What:** The WebTransport handler (`wt_handler.rs`) had a catch-all `Ok(_) => {}` that silently swallowed `ScrollbackRequest` messages. The old `session.rs` handled scrollback via `tokio::select!`, but the new session-managed `wt_handler.rs` forwarded input directly to PTY without checking for scrollback requests.

**Fix:** Added `ScrollbackRequest` match arm in `wt_handler.rs` that calls `ManagedSession::get_scrollback()` and sends the response through the client channel.

### Bug 2: scrollback_total always zero

**What:** The WASM client clamped `scroll_offset` to `scrollback_total`, which was initialized to 0 and never updated. The `ScreenSnapshot` message carries `scrollback_len` but the client never read it. The `ScreenData` struct in the WASM client didn't even have a `scrollback_len` field.

**Fix:**
- Added `scrollback_len` field to `ScreenData` in WASM messages
- Set `scrollback_total` from `ScreenSnapshot.scrollback_len` in `apply_snapshot()`
- Added fallback: default `scrollback_total` to 10000 if not set (server clamps to actual)

### Bug 3: egui MouseWheel events not firing in WASM

**What:** The scroll handler only listened for `egui::Event::MouseWheel`, but egui in WASM may not fire this event for all scroll input methods (trackpad, touch). The browser captures wheel events for page scrolling before egui sees them.

**Fix:** Added `ui.input().smooth_scroll_delta.y` as a fallback source for scroll delta. This captures trackpad and touch scroll gestures that egui processes through a different path.

### Bug 4: Scroll direction inverted

**What:** Scrolling up (positive delta in browser convention) decreased the scroll offset instead of increasing it. The formula was `offset - lines` but should have been `offset + lines` since positive delta means "scroll up = show older content = increase offset."

**Fix:** Changed `scroll_offset as isize - lines` to `scroll_offset as isize + lines`.

### Bug 5: Scrollback lines rendered in wrong order / showing duplicates

**What:** The `paint_grid` function showed scrollback lines at the wrong positions. The scrollback data from the server was indexed by absolute position in the scrollback buffer, but the renderer used the view row index directly, causing misalignment and repeated lines (the "lots of 57" the user saw).

**Fix:** Corrected the index mapping: `sb_idx = sb_count - sb_visible + row`, where `sb_count` is the number of scrollback lines received and `sb_visible` is how many should be shown on screen. Most recent scrollback line appears at the bottom of the scrollback area, just above the live screen.

## Contributing Factors

1. **Architecture change without migration.** Scrollback was implemented for the old `session::run_session` path but not ported when we moved to `SessionManager` + `ManagedSession` + `wt_handler`.

2. **No integration test for scrollback.** The PTY integration tests (`pty_session.rs`) tested echo, resize, vim, etc. but never tested scrollback. If we had a test that ran `seq 1 100` and verified scrollback retrieval, bugs 1-2 would have been caught immediately.

3. **Browser vs native differences.** Bugs 3-4 only manifest in the WASM/browser environment. The headless Chrome E2E tests couldn't catch these because WebGL context loss prevented visual verification.

4. **Multiple serialization layers.** The scrollback data passes through: `ScreenBuffer.scrollback` -> `ManagedSession.get_scrollback()` -> FlatBuffers `ScrollbackData` -> WebTransport -> WASM `decode_server_msg` -> `DisplayGrid.apply_scrollback()` -> `paint_grid()`. Each layer had its own indexing convention, and the mismatch between server (chronological) and client (view-relative) indices caused bug 5.

## What We Fixed

| Bug | File | Lines changed |
|-----|------|--------------|
| ScrollbackRequest not handled | `wt_handler.rs` | +10 |
| scrollback_total always zero | `messages.rs`, `render.rs` | +8 |
| MouseWheel not firing | `scroll.rs` | +3 |
| Scroll direction inverted | `scroll.rs` | +1 (sign flip) |
| Wrong render order | `render.rs` | +15 |

## Tests Added

18 session management integration tests including:
- `scrollback_after_output` — generates 50 lines, verifies scrollback data present
- `scrollback_empty_terminal` — no output = no scrollback
- `scrollback_with_large_offset` — offset beyond buffer is clamped

## Lessons

1. **When refactoring transport layers, audit all message types.** The `wt_handler` rewrite dropped scrollback handling because it wasn't in the "critical path" (KeyInput, Resize, ScreenUpdate). A checklist of all `ClientMsg` variants would have caught this.

2. **Test the scroll path end-to-end.** Even a simple test that generates output and requests scrollback would have caught 3 of the 5 bugs.

3. **Browser scroll conventions are not intuitive.** "Positive delta = scroll up = show older content" is the opposite of what you might assume. Always test scroll direction in the actual browser.

4. **Default to a permissive value, let the server clamp.** Setting `scrollback_total = 10000` as a default lets the client always attempt to scroll. The server returns whatever it actually has. This is more robust than requiring the exact count upfront.
