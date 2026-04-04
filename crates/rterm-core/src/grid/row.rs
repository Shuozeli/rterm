//! Row type for the grid ring buffer.

use std::ops::{Index, IndexMut};

use crate::grid::cell::GridCell;
use crate::grid::index::Column;

/// A row in the grid.
///
/// Tracks `occ` (occupancy) as the last modified column + 1,
/// so that operations like `reset()` only touch the cells that were actually written.
#[derive(Debug, Clone)]
pub struct Row<T> {
    inner: Vec<T>,
    /// Last modified column index + 1. All cells at or after `occ` are considered unused.
    pub(crate) occ: usize,
}

impl<T: Clone> Row<T> {
    /// Create a new row filled with the given cell template.
    pub fn new(template: &T, columns: usize) -> Self {
        let inner = vec![template.clone(); columns];
        let occ = 0;
        Row { inner, occ }
    }

    /// Create a row with pre-allocated capacity.
    pub fn with_capacity(columns: usize) -> Self {
        Row {
            inner: Vec::with_capacity(columns),
            occ: 0,
        }
    }

    /// Returns the number of cells in this row.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns true if the row has no cells.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns the occupancy (last modified column + 1).
    pub fn occ(&self) -> usize {
        self.occ
    }
}

impl<T: GridCell> Row<T> {
    /// Reset all cells in the row up to `occ`.
    pub fn reset(&mut self, template: &T) {
        for cell in &mut self.inner[..self.occ] {
            cell.reset(template);
        }
        self.occ = 0;
    }

    /// Returns true if all cells up to `occ` are empty.
    pub fn is_clear(&self) -> bool {
        self.inner[..self.occ].iter().all(|cell| cell.is_empty())
    }

    /// Shrink the row to `columns` cells.
    pub fn shrink(&mut self, columns: usize) {
        if columns < self.inner.len() {
            self.inner.truncate(columns);
            if self.occ > columns {
                self.occ = columns;
            }
        }
    }

    /// Initialize all cells up to `occ` with the template.
    pub fn initialize(&mut self, template: &T) {
        for cell in &mut self.inner[..self.occ] {
            cell.reset(template);
        }
    }
}

impl<T: GridCell + Clone> Row<T> {
    /// Grow the row to `columns` cells, filling new cells with the template.
    pub fn grow(&mut self, template: &T, columns: usize) {
        if columns > self.inner.len() {
            self.inner.resize(columns, template.clone());
        }
    }
}

impl<T> Index<Column> for Row<T> {
    type Output = T;

    fn index(&self, index: Column) -> &Self::Output {
        &self.inner[index.0]
    }
}

impl<T> IndexMut<Column> for Row<T> {
    fn index_mut(&mut self, index: Column) -> &mut Self::Output {
        // Mark this column as occupied
        if index.0 >= self.occ {
            self.occ = index.0 + 1;
        }
        &mut self.inner[index.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Cell;

    fn default_cell() -> Cell {
        Cell::default()
    }

    #[test]
    fn row_occ_tracking() {
        let template = default_cell();
        let mut row: Row<Cell> = Row::new(&template, 10);

        assert_eq!(row.occ(), 0);

        row[Column(2)] = default_cell();
        assert_eq!(row.occ(), 3);

        row[Column(5)] = default_cell();
        assert_eq!(row.occ(), 6);

        row[Column(0)] = default_cell();
        assert_eq!(row.occ(), 6);
    }

    #[test]
    fn row_reset_clears_occ() {
        let template = default_cell();
        let mut row: Row<Cell> = Row::new(&template, 10);

        row[Column(4)] = default_cell();
        assert_eq!(row.occ(), 5);

        row.reset(&template);
        assert_eq!(row.occ(), 0);
    }
}
