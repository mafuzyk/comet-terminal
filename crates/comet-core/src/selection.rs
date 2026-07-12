//! Text selection for terminal.

use crate::cell::Cell;

/// Selection mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionMode {
    #[default]
    Character,
    Word,
    Line,
}

/// Text selection state.
#[derive(Debug, Clone, Default)]
pub struct Selection {
    start: Option<(usize, usize)>, // (col, row) - absolute coordinates
    end: Option<(usize, usize)>,
    mode: SelectionMode,
}

impl Selection {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if there's an active selection.
    pub fn is_active(&self) -> bool {
        self.start.is_some() && self.end.is_some()
    }

    /// Sets the selection mode.
    pub fn set_mode(&mut self, mode: SelectionMode) {
        self.mode = mode;
    }

    /// Returns the selection mode.
    pub fn mode(&self) -> SelectionMode {
        self.mode
    }

    /// Starts a new selection at the given position.
    pub fn start(&mut self, col: usize, row: usize) {
        self.start = Some((col, row));
        self.end = Some((col, row));
    }

    /// Updates the end of the selection.
    pub fn update(&mut self, col: usize, row: usize) {
        if self.start.is_some() {
            self.end = Some((col, row));
        }
    }

    /// Ends the selection.
    pub fn end(&mut self) {
        // Selection stays active until cleared
    }

    /// Clears the selection.
    pub fn clear(&mut self) {
        self.start = None;
        self.end = None;
    }

    /// Sets the selection start position.
    pub fn set_start(&mut self, col: usize, row: usize) {
        self.start = Some((col, row));
    }

    /// Sets the selection end position.
    pub fn set_end(&mut self, col: usize, row: usize) {
        self.end = Some((col, row));
    }

    /// Returns the selection bounds as (start_col, start_row, end_col, end_row)
    /// in absolute coordinates, normalized so start <= end in reading order.
    pub fn bounds(&self) -> Option<(usize, usize, usize, usize)> {
        let start = self.start?;
        let end = self.end?;

        if start.1 < end.1 || (start.1 == end.1 && start.0 <= end.0) {
            Some((start.0, start.1, end.0, end.1))
        } else {
            Some((end.0, end.1, start.0, start.1))
        }
    }

    /// Checks if a given cell (col, row) is within the selection.
    /// Uses absolute coordinates (including scrollback).
    ///
    /// Character/Word mode: stream-based — intermediate rows are fully selected.
    /// Line mode: full rows.
    pub fn contains(&self, col: usize, row: usize) -> bool {
        if let Some((start_col, start_row, end_col, end_row)) = self.bounds() {
            if row < start_row || row > end_row {
                return false;
            }
            match self.mode {
                SelectionMode::Line => true,
                SelectionMode::Character | SelectionMode::Word => {
                    if row == start_row && col < start_col {
                        return false;
                    }
                    if row == end_row && col > end_col {
                        return false;
                    }
                    true
                }
            }
        } else {
            false
        }
    }

    /// Gets the selected text as a string.
    pub fn get_text<F>(&self, mut get_cell: F) -> String
    where
        F: FnMut(usize, usize) -> Option<Cell>,
    {
        let Some((start_col, start_row, end_col, end_row)) = self.bounds() else {
            return String::new();
        };

        let mut result = String::new();

        for row in start_row..=end_row {
            let start_c = if row == start_row { start_col } else { 0 };
            let end_c = if row == end_row { end_col } else { usize::MAX };

            let mut line = String::new();
            let mut has_content = false;

            for col in start_c..=end_c {
                if let Some(cell) = get_cell(col, row) {
                    if cell.character != ' ' || !line.is_empty() {
                        line.push(cell.character);
                        has_content = true;
                    } else if !line.is_empty() {
                        line.push(' ');
                    }
                } else {
                    break;
                }
            }

            // Trim trailing spaces
            let line = line.trim_end();
            if has_content || row < end_row {
                result.push_str(line);
                if row < end_row {
                    result.push('\n');
                }
            }
        }

        result
    }

    /// Expands the selection to word boundaries.
    pub fn expand_to_word<F>(&mut self, mut get_cell: F)
    where
        F: FnMut(usize, usize) -> Option<Cell>,
    {
        self.mode = SelectionMode::Word;
        if let Some((start_col, start_row, end_col, end_row)) = self.bounds() {
            let mut col = start_col;

            // Expand start backward to word boundary
            while col > 0 {
                if let Some(cell) = get_cell(col - 1, start_row) {
                    let ch = cell.character;
                    if ch.is_alphanumeric() || ch == '_' {
                        col -= 1;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            let new_start_col = col;

            // Expand end forward to word boundary
            let mut col = end_col;
            while let Some(cell) = get_cell(col, end_row) {
                let ch = cell.character;
                if ch.is_alphanumeric() || ch == '_' {
                    col += 1;
                } else {
                    break;
                }
            }
            let new_end_col = col;

            self.start = Some((new_start_col, start_row));
            self.end = Some((new_end_col, end_row));
        }
    }

    /// Expands the selection to full lines.
    pub fn expand_to_line(&mut self) {
        self.mode = SelectionMode::Line;
        if let Some((_start_col, start_row, _end_col, end_row)) = self.bounds() {
            self.start = Some((0, start_row));
            self.end = Some((usize::MAX, end_row));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Cell;
    use crate::grid::Grid;

    #[test]
    fn test_selection_basic() {
        let mut sel = Selection::new();
        assert!(!sel.is_active());

        sel.start(5, 2);
        sel.update(10, 2);
        assert!(sel.is_active());

        let bounds = sel.bounds().unwrap();
        assert_eq!(bounds, (5, 2, 10, 2));
    }

    #[test]
    fn test_selection_bounds_reversed() {
        let mut sel = Selection::new();
        sel.start(10, 2);
        sel.update(5, 2);

        let bounds = sel.bounds().unwrap();
        assert_eq!(bounds, (5, 2, 10, 2));
    }

    #[test]
    fn test_selection_multiline() {
        let mut sel = Selection::new();
        sel.start(5, 1);
        sel.update(10, 3);

        let bounds = sel.bounds().unwrap();
        assert_eq!(bounds, (5, 1, 10, 3));
    }

    #[test]
    fn test_selection_contains() {
        let mut sel = Selection::new();
        sel.start(5, 2);
        sel.update(10, 4);

        // Stream-based: intermediate rows (3) are fully selected
        assert!(sel.contains(7, 3));
        assert!(sel.contains(5, 2));
        assert!(sel.contains(10, 4));
        assert!(sel.contains(4, 3)); // intermediate row, any col
        assert!(sel.contains(11, 3)); // intermediate row, any col
        assert!(!sel.contains(7, 1)); // above selection
        assert!(!sel.contains(7, 5)); // below selection
    }

    #[test]
    fn test_selection_clear() {
        let mut sel = Selection::new();
        sel.start(5, 2);
        assert!(sel.is_active());

        sel.clear();
        assert!(!sel.is_active());
        assert!(sel.bounds().is_none());
    }
}
