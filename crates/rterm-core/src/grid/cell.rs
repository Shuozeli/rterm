//! GridCell trait and ResetDiscriminant for rterm Cell.

use crate::cell::Cell;

/// GridCell is a cell that can be stored in the grid.
pub trait GridCell: Sized {
    /// Returns true if the cell is empty (space with default colors and no flags).
    fn is_empty(&self) -> bool;

    /// Reset the cell to the template.
    fn reset(&mut self, template: &Self);

    /// Get the cell's flags.
    fn flags(&self) -> &crate::cell::Flags;

    /// Get a mutable reference to the cell's flags.
    fn flags_mut(&mut self) -> &mut crate::cell::Flags;
}

impl GridCell for Cell {
    fn is_empty(&self) -> bool {
        self.ch == ' '
            && self.bg == crate::color::Color::Default
            && self.fg == crate::color::Color::Default
            && self.flags.is_empty()
    }

    fn reset(&mut self, template: &Self) {
        // Reset cell to template's styling and clear content
        // For scrolling: cells at the bottom get cleared to blank
        self.ch = ' ';
        self.bg = template.bg;
        self.fg = template.fg;
        self.flags = crate::cell::Flags::empty();
    }

    fn flags(&self) -> &crate::cell::Flags {
        &self.flags
    }

    fn flags_mut(&mut self) -> &mut crate::cell::Flags {
        &mut self.flags
    }
}

/// Reset discriminant for optimized cell resets.
///
/// Only resets a cell if the discriminant value changed,
/// avoiding unnecessary writes when the pen hasn't changed.
pub trait ResetDiscriminant<T>: GridCell {
    /// Returns the discriminant value that determines when a cell needs reset.
    fn discriminant(&self) -> T;

    /// Reset the cell with the given template if discriminant changed.
    fn reset_if_discriminant_changed(&mut self, template: &Self)
    where
        T: PartialEq,
    {
        if self.discriminant() != template.discriminant() {
            self.reset(template);
        }
    }
}

impl ResetDiscriminant<crate::color::Color> for Cell {
    fn discriminant(&self) -> crate::color::Color {
        self.bg
    }
}
