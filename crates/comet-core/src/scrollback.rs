//! Scrollback buffer for terminal history.
use crate::cell::Cell;
use std::collections::{HashMap, VecDeque};

/// A row of cells in the scrollback buffer.
#[derive(Debug, Clone)]
pub struct Row {
    pub cells: Vec<Cell>,
    /// Hyperlink URIs per column (key = column index, value = URI).
    pub hyperlinks: HashMap<usize, String>,
}

impl Row {
    /// Creates a new row with the given width, filled with blank cells.
    pub fn new(width: usize) -> Self {
        Self {
            cells: vec![Cell::default(); width],
            hyperlinks: HashMap::new(),
        }
    }

    /// Creates a row from a slice of cells.
    pub fn from_cells(cells: &[Cell]) -> Self {
        Self {
            cells: cells.to_vec(),
            hyperlinks: HashMap::new(),
        }
    }

    /// Returns the width of the row.
    pub fn width(&self) -> usize {
        self.cells.len()
    }
}

/// Scrollback buffer for terminal history.
///
/// Uses a `VecDeque` for efficient push_front/pop_back operations.
/// The most recent line is at the front (index 0).
#[derive(Debug, Clone)]
pub struct ScrollbackBuffer {
    /// The stored history lines (most recent first).
    lines: VecDeque<Row>,
    /// Maximum number of lines to keep.
    max_size: usize,
    /// Current viewport offset from the bottom (0 = at bottom/newest).
    viewport_offset: usize,
}

impl ScrollbackBuffer {
    /// Creates a new scrollback buffer with the given maximum size.
    pub fn new(max_size: usize, _width: usize) -> Self {
        Self {
            lines: VecDeque::with_capacity(max_size),
            max_size,
            viewport_offset: 0,
        }
    }

    /// Returns the maximum size of the scrollback buffer.
    pub fn max_size(&self) -> usize {
        self.max_size
    }

    /// Returns the current number of lines in the scrollback.
    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// Returns true if the scrollback is empty.
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    /// Pushes a line to the scrollback (when scrolling up).
    ///
    /// The line should be a copy of the top visible line before it scrolls off.
    pub fn push_line(&mut self, line: Row) {
        if self.lines.len() >= self.max_size {
            self.lines.pop_back();
        }
        self.lines.push_front(line);
        // If we're scrolled up, maintain the same visual position
        if self.viewport_offset > 0 {
            self.viewport_offset += 1;
        }
    }

    /// Scrolls the viewport up by `amount` lines.
    ///
    /// Returns the actual number of lines scrolled.
    pub fn scroll_up(&mut self, amount: usize) -> usize {
        let available = self.lines.len().saturating_sub(self.viewport_offset);
        let actual = amount.min(available);
        self.viewport_offset += actual;
        actual
    }

    /// Scrolls the viewport down by `amount` lines.
    ///
    /// Returns the actual number of lines scrolled.
    pub fn scroll_down(&mut self, amount: usize) -> usize {
        let actual = amount.min(self.viewport_offset);
        self.viewport_offset -= actual;
        actual
    }

    /// Scrolls to the top of the scrollback.
    pub fn scroll_to_top(&mut self) {
        self.viewport_offset = self.lines.len();
    }

    /// Scrolls to the bottom (current output).
    pub fn scroll_to_bottom(&mut self) {
        self.viewport_offset = 0;
    }

    /// Returns the current viewport offset (0 = at bottom).
    pub fn viewport_offset(&self) -> usize {
        self.viewport_offset
    }

    /// Returns true if the viewport is at the bottom (showing live output).
    pub fn is_at_bottom(&self) -> bool {
        self.viewport_offset == 0
    }

    /// Returns the total number of lines available (scrollback + visible).
    pub fn total_lines(&self) -> usize {
        self.lines.len()
    }

    /// Gets a line at the given absolute index (0 = oldest in scrollback).
    pub fn get_line(&self, index: usize) -> Option<&Row> {
        self.lines.get(index)
    }

    /// Gets a line relative to the viewport (0 = top of current viewport).
    pub fn get_viewport_line(&self, viewport_row: usize) -> Option<&Row> {
        let absolute = self.viewport_offset + viewport_row;
        self.lines.get(absolute)
    }

    /// Clears the scrollback buffer.
    pub fn clear(&mut self) {
        self.lines.clear();
        self.viewport_offset = 0;
    }

    /// Resizes the buffer width, updating all stored rows.
    pub fn resize(&mut self, new_width: usize) {
        for row in self.lines.iter_mut() {
            if row.cells.len() < new_width {
                row.cells
                    .extend(std::iter::repeat(Cell::default()).take(new_width - row.cells.len()));
            } else {
                row.cells.truncate(new_width);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell::Cell;

    #[test]
    fn test_scrollback_push_and_scroll() {
        let mut sb = ScrollbackBuffer::new(100, 80);

        // Push some lines
        for i in 0..5 {
            let mut row = Row::new(80);
            row.cells[0] = Cell {
                character: char::from_digit(i as u32, 10).unwrap_or('0'),
                ..Default::default()
            };
            sb.push_line(row);
        }

        assert_eq!(sb.len(), 5);
        assert_eq!(sb.viewport_offset(), 0);

        // Scroll up
        sb.scroll_up(2);
        assert_eq!(sb.viewport_offset(), 2);

        // Scroll down
        sb.scroll_down(1);
        assert_eq!(sb.viewport_offset(), 1);

        // Scroll to top
        sb.scroll_to_top();
        assert_eq!(sb.viewport_offset(), 5);

        // Scroll to bottom
        sb.scroll_to_bottom();
        assert_eq!(sb.viewport_offset(), 0);
    }

    #[test]
    fn test_scrollback_max_size() {
        let mut sb = ScrollbackBuffer::new(3, 10);

        for i in 0..5 {
            let mut row = Row::new(10);
            row.cells[0] = Cell {
                character: char::from_digit(i as u32, 10).unwrap_or('0'),
                ..Default::default()
            };
            sb.push_line(row);
        }

        // Should only keep the last 3
        assert_eq!(sb.len(), 3);
    }

    #[test]
    fn test_scrollback_viewport_lines() {
        let mut sb = ScrollbackBuffer::new(100, 80);

        for i in 0..10 {
            let mut row = Row::new(80);
            row.cells[0] = Cell {
                character: char::from_digit(i as u32, 10).unwrap_or('0'),
                ..Default::default()
            };
            sb.push_line(row);
        }

        // At bottom, viewport shows lines 9,8,7,6,5,4,3,2,1,0 (newest to oldest)
        // But viewport_offset=0 means we're at bottom
        sb.scroll_up(3);
        // Now viewport shows lines 6,5,4,3,2,1,0 (indices from scrollback)
        let line = sb.get_viewport_line(0).unwrap();
        assert_eq!(line.cells[0].character, '6');
    }
}
