//! Storage ring buffer for the grid.

use crate::grid::cell::GridCell;
use crate::grid::index::Line;
use crate::grid::row::Row;

/// Storage is a ring buffer holding rows.
///
/// The `zero` index is the logical line 0 — the topmost visible line.
/// Physical index = (zero + logical) % len.
#[derive(Debug, Clone)]
pub struct Storage<T> {
    /// Ring buffer of rows.
    inner: Vec<Row<T>>,
    /// Number of **visible** lines in the buffer (excluding history).
    visible_lines: usize,
    /// Number of lines currently in the buffer (history + visible).
    len: usize,
    /// Index into `inner` that corresponds to logical line 0.
    zero: usize,
}

impl<T: GridCell + Clone> Storage<T> {
    /// Create new storage with the given dimensions.
    pub fn new(template: &T, visible_lines: usize, columns: usize) -> Self {
        let len = visible_lines;
        let inner: Vec<Row<T>> = (0..len).map(|_| Row::new(template, columns)).collect();
        Storage {
            inner,
            visible_lines,
            len,
            zero: 0,
        }
    }

    /// Create storage with pre-allocated capacity.
    pub fn with_capacity(visible_lines: usize) -> Self {
        Storage {
            inner: Vec::with_capacity(visible_lines),
            visible_lines,
            len: 0,
            zero: 0,
        }
    }
}

impl<T> Storage<T> {
    /// Returns the total number of lines (history + visible).
    pub fn len(&self) -> usize {
        self.len
    }

    /// Returns the number of visible lines.
    pub fn visible_lines(&self) -> usize {
        self.visible_lines
    }

    /// Returns the number of history lines above the visible viewport.
    pub fn history_size(&self) -> usize {
        self.len.saturating_sub(self.visible_lines)
    }

    /// Returns true if there is no history.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Compute the physical index for a logical line.
    ///
    /// Line 0 maps to `self.zero`. Negative lines wrap around.
    /// Lines outside the buffer range are clamped.
    pub fn compute_index(&self, line: Line) -> usize {
        let total = self.len as i32;
        let zero = self.zero as i32;

        // Handle lines outside the buffer
        if line.0 < -(self.history_size() as i32) {
            // Clamp to first history line
            return ((zero + line.0 + total) % total) as usize;
        }
        if line.0 >= self.visible_lines as i32 {
            // Clamp to last visible line
            return ((zero + line.0 + total) % total) as usize;
        }

        ((zero + line.0 + total) % total) as usize
    }

    /// Returns a reference to the row at the given logical line.
    pub fn get(&self, line: Line) -> Option<&Row<T>> {
        if self.is_empty() {
            return None;
        }
        let idx = self.compute_index(line);
        self.inner.get(idx)
    }

    /// Returns a mutable reference to the row at the given logical line.
    pub fn get_mut(&mut self, line: Line) -> Option<&mut Row<T>> {
        if self.is_empty() {
            return None;
        }
        let idx = self.compute_index(line);
        self.inner.get_mut(idx)
    }
}

impl<T: GridCell + Clone> Storage<T> {
    /// Rotate the storage upward by `count` lines (scroll up).
    ///
    /// This is O(1) for the ring buffer — just moves the `zero` pointer.
    pub fn rotate(&mut self, count: usize) {
        if self.is_empty() || count == 0 {
            return;
        }
        let len = self.len;
        self.zero = (self.zero as isize - count as isize + len as isize) as usize % len;
    }

    /// Rotate the storage downward by `count` lines (scroll down).
    pub fn rotate_down(&mut self, count: usize) {
        if self.is_empty() || count == 0 {
            return;
        }
        let len = self.len;
        self.zero = (self.zero + count) % len;
    }

    /// Swap two lines in the buffer.
    pub fn swap(&mut self, a: Line, b: Line) {
        if self.is_empty() {
            return;
        }
        let idx_a = self.compute_index(a);
        let idx_b = self.compute_index(b);
        self.inner.swap(idx_a, idx_b);
    }

    /// Grow visible lines to `target` lines.
    pub fn grow_visible_lines(&mut self, template: &T, target: usize) {
        if target <= self.visible_lines {
            return;
        }

        // Grow inner capacity
        let additional = target - self.visible_lines;
        for _ in 0..additional {
            self.inner.push(Row::new(
                template,
                self.inner.first().map_or(0, |r| r.len()),
            ));
        }
        self.visible_lines = target;
        self.len = self.visible_lines;
    }

    /// Shrink visible lines to `target` lines.
    pub fn shrink_visible_lines(&mut self, target: usize) {
        if target >= self.visible_lines {
            return;
        }
        self.visible_lines = target;
        self.len = self.visible_lines;
    }

    /// Truncate to `target` visible lines.
    pub fn truncate(&mut self, target: usize) {
        if target >= self.len {
            return;
        }
        self.inner.truncate(target);
        self.len = target.min(self.visible_lines);
        self.visible_lines = self.len;
        self.zero = 0;
    }

    /// Initialize all rows up to their occupancy.
    pub fn initialize(&mut self, template: &T) {
        for row in &mut self.inner {
            row.initialize(template);
        }
    }

    /// Update occ for all rows after a resize.
    pub fn update_occ(&mut self) {
        for row in &mut self.inner {
            let len = row.len();
            if row.occ() > len {
                row.occ = len;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Cell;
    use crate::grid::index::Line;

    fn default_cell() -> Cell {
        Cell::default()
    }

    #[test]
    fn storage_compute_index() {
        let storage: Storage<Cell> = Storage::new(&default_cell(), 5, 10);

        // Line 0 should map to zero
        assert_eq!(storage.compute_index(Line(0)), 0);
        // Line 1 should map to 1
        assert_eq!(storage.compute_index(Line(1)), 1);
        // Line 4 should map to 4
        assert_eq!(storage.compute_index(Line(4)), 4);
    }

    #[test]
    fn storage_rotate() {
        let mut storage: Storage<Cell> = Storage::new(&default_cell(), 5, 10);

        // After rotate(1), line 0 should map to index 4 (wrapping)
        storage.rotate(1);
        assert_eq!(storage.compute_index(Line(0)), 4);
        assert_eq!(storage.compute_index(Line(1)), 0);
        assert_eq!(storage.compute_index(Line(4)), 3);
    }

    #[test]
    fn storage_swap() {
        let mut storage: Storage<Cell> = Storage::new(&default_cell(), 5, 10);

        // Get initial values
        let row0 = storage.get(Line(0)).unwrap().len();
        let row2 = storage.get(Line(2)).unwrap().len();

        storage.swap(Line(0), Line(2));

        assert_eq!(storage.get(Line(0)).unwrap().len(), row2);
        assert_eq!(storage.get(Line(2)).unwrap().len(), row0);
    }
}
