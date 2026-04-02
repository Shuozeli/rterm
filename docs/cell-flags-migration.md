<!-- agent-updated: 2026-04-02T00:00:00Z -->
# Cell/Flags Migration: Alacritty-style Design

Migrates rterm's `Cell`/`CellAttributes`/`Color` types to match alacritty's
design. Scope: structural changes + new attribute flags. Out of scope: `CellExtra`
(zero-width chars, hyperlinks, per-cell underline color).

## What Changes

### `CellAttributes` (7 bool fields) → `Flags` (u16 bitflags)

```
Before:
  pub attrs: CellAttributes  // bold, italic, underline, strikethrough, reverse, dim, hidden
  pub wide_continuation: bool

After:
  pub flags: Flags           // u16 bitfield, matches alacritty layout exactly
```

### `Flags` bit layout

```
INVERSE                  = 0b0000_0000_0000_0001   // was: reverse
BOLD                     = 0b0000_0000_0000_0010
ITALIC                   = 0b0000_0000_0000_0100
BOLD_ITALIC              = 0b0000_0000_0000_0110   // compound
UNDERLINE                = 0b0000_0000_0000_1000
WRAPLINE                 = 0b0000_0000_0001_0000   // NEW: soft wrap marker
WIDE_CHAR                = 0b0000_0000_0010_0000   // NEW: left-half of wide char
WIDE_CHAR_SPACER         = 0b0000_0000_0100_0000   // was: wide_continuation
DIM                      = 0b0000_0000_1000_0000
HIDDEN                   = 0b0000_0001_0000_0000
STRIKEOUT                = 0b0000_0010_0000_0000   // was: strikethrough
LEADING_WIDE_CHAR_SPACER = 0b0000_0100_0000_0000   // NEW
DOUBLE_UNDERLINE         = 0b0000_1000_0000_0000   // NEW: SGR 21
UNDERCURL                = 0b0001_0000_0000_0000   // NEW: SGR 4:3
DOTTED_UNDERLINE         = 0b0010_0000_0000_0000   // NEW: SGR 4:4
DASHED_UNDERLINE         = 0b0100_0000_0000_0000   // NEW: SGR 4:5
ALL_UNDERLINES           = UNDERLINE | DOUBLE_UNDERLINE | UNDERCURL | DOTTED | DASHED
```

### `Cell` struct

```rust
// Before
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub attrs: CellAttributes,
    pub wide_continuation: bool,
}

// After
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub flags: Flags,
}
```

### `Color` enum — unchanged

`Color::Default`, `Color::Indexed(u8)`, `Color::Rgb(u8,u8,u8)` stay as-is.

### SGR behavior change: SGR 21

**Breaking behavior change.** Current (non-standard): SGR 21 resets bold.
After migration (standard, matches alacritty/xterm): SGR 21 = `DOUBLE_UNDERLINE`.

Bold is only reset by SGR 22 (resets both BOLD and DIM together).

### SGR new codes

```
4:1 (or 4 alone)  → UNDERLINE            (unchanged)
4:2               → DOUBLE_UNDERLINE
4:3               → UNDERCURL
4:4               → DOTTED_UNDERLINE
4:5               → DASHED_UNDERLINE
21                → DOUBLE_UNDERLINE     (was: bold-off — BEHAVIOR CHANGE)
24                → remove ALL_UNDERLINES
```

### Proto wire format — clean break

`CellData.attrs: u8` → `CellData.flags: u16` in FlatBuffers schema.
FlatBuffers struct layout: `(u32, u32, u32, u8)` → `(u32, u32, u32, u16)`.
Both relay and WASM must be deployed together. No backward-compat shim needed
(no production users).

---

## Migration Map (File by File)

### Phase 1 — rterm-core (blocking for all other phases)

**`crates/rterm-core/Cargo.toml`**
- Add `bitflags = "2"` dependency

**`crates/rterm-core/src/cell.rs`**
- Remove `CellAttributes` struct
- Add `Flags` bitflags (u16, bitflags 2.x macro)
- Remove `wide_continuation: bool` from `Cell`
- Remove `attrs: CellAttributes` from `Cell`
- Add `flags: Flags` to `Cell`
- Update `Cell::default()`: `flags: Flags::empty()`
- Update `Cell::with_char()`: `flags: Flags::empty()`
- Update `Cell::reset()`: `*self = Self::default()`
- Update `lib.rs` export: `pub use cell::{Cell, Flags};`

**`crates/rterm-core/src/buffer.rs`**
- `Pen.attrs: CellAttributes` → `Pen.flags: Flags`
- `write_char`: `self.grid[row][col].wide_continuation` → `.flags.contains(Flags::WIDE_CHAR_SPACER)`
- `write_char`: left-half cell gets `flags: self.pen.flags | Flags::WIDE_CHAR`
- `write_char`: right-half cell gets `flags: self.pen.flags | Flags::WIDE_CHAR_SPACER`

**`crates/rterm-core/src/terminal.rs`**
- Replace `use crate::cell::CellAttributes` with `use crate::cell::Flags`
- Rewrite `handle_sgr()` to iterate param groups (not flattened), supporting sub-params
- SGR 0: `pen.flags = Flags::empty()`
- SGR 1: `pen.flags.insert(Flags::BOLD)`
- SGR 2: `pen.flags.insert(Flags::DIM)`
- SGR 3: `pen.flags.insert(Flags::ITALIC)`
- SGR 4 (with sub-params): map 4/4:1→UNDERLINE, 4:2→DOUBLE_UNDERLINE, 4:3→UNDERCURL, 4:4→DOTTED_UNDERLINE, 4:5→DASHED_UNDERLINE, 4:0→remove ALL_UNDERLINES
- SGR 7: `pen.flags.insert(Flags::INVERSE)`
- SGR 8: `pen.flags.insert(Flags::HIDDEN)`
- SGR 9: `pen.flags.insert(Flags::STRIKEOUT)`
- SGR 21: `pen.flags.insert(Flags::DOUBLE_UNDERLINE)` ← BEHAVIOR CHANGE (was bold-off)
- SGR 22: `pen.flags.remove(Flags::BOLD | Flags::DIM)`
- SGR 23: `pen.flags.remove(Flags::ITALIC)`
- SGR 24: `pen.flags.remove(Flags::ALL_UNDERLINES)`
- SGR 27: `pen.flags.remove(Flags::INVERSE)`
- SGR 28: `pen.flags.remove(Flags::HIDDEN)`
- SGR 29: `pen.flags.remove(Flags::STRIKEOUT)`
- Colors (30-37, 38, 39, 40-47, 48, 49, 90-97, 100-107): unchanged

**Tests to update in rterm-core:**
- `cell.rs`: `default_cell_is_blank`, `cell_reset`, `attributes_normal_is_default`
- `buffer.rs`: `write_char_with_pen`, `unicode_wide_char_test`
- `terminal.rs`: `sgr_bold_and_color`, `sgr_all_attributes`, `sgr_reset_individual_attrs` (SGR 21 behavior change), `ls_color_output`
- `tests/real_output.rs`: all `.attrs.X` → `.flags.contains(Flags::X)`, `.wide_continuation` → `.flags.contains(Flags::WIDE_CHAR_SPACER)`
- `tests/claude_render_test.rs`: same pattern

Field rename map for tests:
| Old | New |
|-----|-----|
| `.attrs.bold` | `.flags.contains(Flags::BOLD)` |
| `.attrs.italic` | `.flags.contains(Flags::ITALIC)` |
| `.attrs.underline` | `.flags.contains(Flags::UNDERLINE)` |
| `.attrs.strikethrough` | `.flags.contains(Flags::STRIKEOUT)` |
| `.attrs.reverse` | `.flags.contains(Flags::INVERSE)` |
| `.attrs.dim` | `.flags.contains(Flags::DIM)` |
| `.attrs.hidden` | `.flags.contains(Flags::HIDDEN)` |
| `.wide_continuation` | `.flags.contains(Flags::WIDE_CHAR_SPACER)` |

---

### Phase 2 — rterm-proto (depends on Phase 1)

**`crates/rterm-proto/schema/rterm.fbs`**
- Change `Cell` struct: `attrs: uint8` → `flags: uint16`
- Add comment documenting bit positions

**Regenerate FlatBuffers bindings:**
```bash
flatc --rust -o crates/rterm-proto/src/generated/ crates/rterm-proto/schema/rterm.fbs
```

**`crates/rterm-proto/src/lib.rs`**
- `CellData.attrs: u8` → `CellData.flags: u16`
- `encode_cell_ranges`: `fbs::Cell::new(ch, fg, bg, cell.attrs)` → `..., cell.flags`
- `decode_cell_ranges`: `attrs: c.attrs()` → `flags: c.flags()`
- Remove `ATTR_*: u8` constants (or replace with `pub use rterm_core::cell::Flags`)
- Update proto round-trip tests for `flags: u16`

---

### Phase 3 — rterm-relay (depends on Phase 1)

**`crates/rterm-relay/src/screen_diff.rs`**
- `use rterm_core::cell::CellAttributes` → `use rterm_core::cell::Flags`
- Remove `pack_attrs(attrs: &CellAttributes) -> u8`
- Cell tuple type: `(char, u32, u32, u8)` → `(char, u32, u32, u16)`
- `cell_to_data`: `flags: cell.flags.bits()` (WIDE_CHAR/WIDE_CHAR_SPACER now on the cell; remove the old `is_wide_char` ATTR_WIDE special case)
- `update_from_snapshot`: `.attrs` → `.flags`
- Diff comparison tuple: update `0u8` default → `0u16`

---

### Phase 4 — rterm-gui (depends on Phase 1)

**`crates/rterm-gui/src/grid.rs`**
- `use rterm_core::cell::CellAttributes` → `use rterm_core::cell::Flags`
- `cell.attrs.reverse` → `cell.flags.contains(Flags::INVERSE)`
- `cell.attrs.underline` → `cell.flags.contains(Flags::UNDERLINE)`
- `cell.attrs.strikethrough` → `cell.flags.contains(Flags::STRIKEOUT)`
- `apply_dim_hidden` signature: `&CellAttributes` → `Flags` (by value, it's Copy)
- Underline render: add branches for DOUBLE_UNDERLINE, UNDERCURL, DOTTED_UNDERLINE, DASHED_UNDERLINE
- Underline condition: `cell.attrs.underline || cell.attrs.strikethrough` → `cell.flags.intersects(Flags::ALL_UNDERLINES | Flags::STRIKEOUT)`

---

### Phase 5 — rterm-wasm (depends on Phase 2)

**`crates/rterm-wasm/src/messages.rs`** (or equivalent)
- `CellData.attrs: u8` → `CellData.flags: u16`
- Replace `ATTR_*: u8` constants with `u16` at new bit positions matching `Flags`
- Add `ATTR_WIDE_SPACER`, `ATTR_DOUBLE_UNDERLINE`, `ATTR_UNDERCURL`, `ATTR_DOTTED_UNDERLINE`, `ATTR_DASHED_UNDERLINE`
- `decode_cell_ranges`: `attrs: c.attrs()` → `flags: c.flags()`

**`crates/rterm-wasm/src/render.rs`**
- All `cell.attrs & ATTR_X` → `cell.flags & ATTR_X`
- Wide-char skip: add `if cell.flags & ATTR_WIDE_SPACER != 0 { continue; }`
- Underline render: add new variants

**Bit position map for WASM constants (old u8 → new u16):**

| Constant | Old bit | New bit |
|----------|---------|---------|
| ATTR_BOLD | 0 | 1 |
| ATTR_ITALIC | 1 | 2 |
| ATTR_UNDERLINE | 2 | 3 |
| ATTR_STRIKEOUT | 3 | 9 |
| ATTR_REVERSE / ATTR_INVERSE | 4 | 0 |
| ATTR_DIM | 5 | 7 |
| ATTR_HIDDEN | 6 | 8 |
| ATTR_WIDE | 7 | 5 |
| ATTR_WIDE_SPACER | (new) | 6 |
| ATTR_DOUBLE_UNDERLINE | (new) | 11 |
| ATTR_UNDERCURL | (new) | 12 |
| ATTR_DOTTED_UNDERLINE | (new) | 13 |
| ATTR_DASHED_UNDERLINE | (new) | 14 |

---

### Phase 6 — display_grid in rterm-core (depends on Phase 2)

**`crates/rterm-core/src/display_grid.rs`** (if it exists)
- `DisplayCell.attrs: u8` → `DisplayCell.flags: u16`
- Default value: `flags: 0u16`

---

## Risk Areas

1. **SGR 21 behavior change** — existing tests assert SGR 21 resets bold (wrong behavior). Must update tests to expect DOUBLE_UNDERLINE instead.

2. **Bit-position remapping in WASM** — old `ATTR_*` bit positions do not match new `Flags` bit positions. Any JavaScript code reading raw FlatBuffer bytes (not via the Rust render layer) will misparse attributes. All FlatBuffers consumers must be updated atomically.

3. **WIDE_CHAR flag must be set at write time** — currently `ATTR_WIDE` in the proto was set by `cell_to_data()` via `is_wide_char(ch)`. After migration, `WIDE_CHAR` must be set in `buffer.rs::write_char()` at cell write time. Verify `Flags::WIDE_CHAR` is inserted on the left-half cell, not just `WIDE_CHAR_SPACER` on the right-half.

4. **FlatBuffers regeneration** — must run `flatc` after schema change. If not installed: `cargo install flatc` or use the pre-built binary. Check if there is a `build.rs` in `rterm-proto`.

5. **Sub-parameter SGR parsing** — current code flattens all params with `.flat_map(|p| p.iter().copied())`. For `4:2` (double underline), the group is `&[4, 2]`. Restructure `handle_sgr` to iterate param groups, reading `group[0]` as the SGR code and `group.get(1)` as the sub-parameter.

---

## Deployment

Server and WASM client must be deployed together (clean break on wire format).
No migration period needed — no production users.
