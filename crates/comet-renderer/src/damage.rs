//! Damage tracking for incremental rendering.
//!
//! This module tracks which regions of the screen have changed and need
//! to be redrawn, minimizing GPU work by only updating damaged areas.

use crate::error::{RendererError, RendererResult};
use parking_lot::RwLock;
use smallvec::SmallVec;
use std::cmp::{max, min};
use std::ops::Range;

/// A rectangular damage region.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DamageRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl DamageRect {
    /// Creates a new damage rect.
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }

    /// Creates a damage rect from two corners.
    pub fn from_corners(x1: u32, y1: u32, x2: u32, y2: u32) -> Self {
        let x = min(x1, x2);
        let y = min(y1, y2);
        let width = max(x1, x2) - x;
        let height = max(y1, y2) - y;
        Self { x, y, width, height }
    }

    /// Creates a damage rect for a single cell.
    pub fn cell(x: u32, y: u32) -> Self {
        Self { x, y, width: 1, height: 1 }
    }

    /// Creates a damage rect for a full row.
    pub fn row(y: u32, width: u32) -> Self {
        Self { x: 0, y, width, height: 1 }
    }

    /// Creates a damage rect for a full column.
    pub fn column(x: u32, height: u32) -> Self {
        Self { x, y: 0, width: 1, height }
    }

    /// Returns true if the rect is empty.
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Returns the area of the rect.
    pub fn area(&self) -> u32 {
        self.width * self.height
    }

    /// Checks if this rect intersects with another.
    pub fn intersects(&self, other: &DamageRect) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }

    /// Checks if this rect contains another.
    pub fn contains(&self, other: &DamageRect) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.x + other.width <= self.x + self.width
            && other.y + other.height <= self.y + self.height
    }

    /// Unions this rect with another, returning a new rect.
    pub fn union(&self, other: &DamageRect) -> DamageRect {
        let x = min(self.x, other.x);
        let y = min(self.y, other.y);
        let max_x = max(self.x + self.width, other.x + other.width);
        let max_y = max(self.y + self.height, other.y + other.height);
        DamageRect::new(x, y, max_x - x, max_y - y)
    }

    /// Clips this rect to the given bounds.
    pub fn clip(&self, bounds: &DamageRect) -> DamageRect {
        let x = max(self.x, bounds.x);
        let y = max(self.y, bounds.y);
        let max_x = min(self.x + self.width, bounds.x + bounds.width);
        let max_y = min(self.y + self.height, bounds.y + bounds.height);

        if max_x <= x || max_y <= y {
            return DamageRect::default();
        }

        DamageRect::new(x, y, max_x - x, max_y - y)
    }
}

/// Damage tracker for incremental rendering.
///
/// Tracks damaged regions using a hybrid approach:
/// - Small number of rects: stored directly
/// - Large number: merged into larger rects to limit count
#[derive(Debug)]
pub struct DamageTracker {
    rects: RwLock<SmallVec<[DamageRect; 8]>>,
    max_rects: usize,
    screen_width: RwLock<u32>,
    screen_height: RwLock<u32>,
    full_damage: RwLock<bool>,
}

impl DamageTracker {
    /// Creates a new damage tracker.
    pub fn new(screen_width: u32, screen_height: u32) -> Self {
        Self {
            rects: RwLock::new(SmallVec::new()),
            max_rects: 64,
            screen_width: RwLock::new(screen_width),
            screen_height: RwLock::new(screen_height),
            full_damage: RwLock::new(false),
        }
    }

    /// Marks the entire screen as damaged.
    pub fn mark_full(&self) {
        *self.full_damage.write() = true;
        self.rects.write().clear();
    }

    /// Checks if full damage is marked.
    pub fn is_full(&self) -> bool {
        *self.full_damage.read()
    }

    /// Adds a damage rect.
    pub fn add(&self, rect: DamageRect) {
        if self.is_full() {
            return;
        }

        let screen_width = *self.screen_width.read();
        let screen_height = *self.screen_height.read();
        let clipped = rect.clip(&DamageRect::new(0, 0, screen_width, screen_height));
        if clipped.is_empty() {
            return;
        }

        let mut rects = self.rects.write();

        // Try to merge with existing rects (intersecting or adjacent)
        for existing in rects.iter_mut() {
            if existing.intersects(&clipped) || self.are_adjacent(existing, &clipped) {
                *existing = existing.union(&clipped);
                return;
            }
        }

        // Add new rect
        rects.push(clipped);

        // If too many rects, merge them
        if rects.len() > self.max_rects {
            self.merge_rects(&mut rects);
        }
    }

    /// Adds a cell damage.
    pub fn add_cell(&self, x: u32, y: u32) {
        self.add(DamageRect::cell(x, y));
    }

    /// Adds a row damage.
    pub fn add_row(&self, y: u32) {
        let width = *self.screen_width.read();
        self.add(DamageRect::row(y, width));
    }

    /// Adds a range of cells in a row.
    pub fn add_row_range(&self, y: u32, x_start: u32, x_end: u32) {
        if x_start >= x_end {
            return;
        }
        self.add(DamageRect::new(x_start, y, x_end - x_start, 1));
    }

    /// Adds multiple rects.
    pub fn add_all(&self, rects: &[DamageRect]) {
        for rect in rects {
            self.add(*rect);
        }
    }

    /// Gets all damage rects and clears the tracker.
    pub fn take_damage(&self) -> SmallVec<[DamageRect; 8]> {
        let mut rects = self.rects.write();
        let damage = rects.drain(..).collect();
        *self.full_damage.write() = false;
        damage
    }

    /// Gets current damage rects without clearing.
    pub fn get_damage(&self) -> SmallVec<[DamageRect; 8]> {
        self.rects.read().clone()
    }

    /// Clears all damage.
    pub fn clear(&self) {
        self.rects.write().clear();
        *self.full_damage.write() = false;
    }

    /// Returns the number of damage rects.
    pub fn len(&self) -> usize {
        self.rects.read().len()
    }

    /// Returns true if no damage is tracked.
    pub fn is_empty(&self) -> bool {
        !self.is_full() && self.rects.read().is_empty()
    }

    /// Resizes the tracker.
    pub fn resize(&self, width: u32, height: u32) {
        *self.screen_width.write() = width;
        *self.screen_height.write() = height;
        self.mark_full();
    }

    /// Merges overlapping/adjacent rects to reduce count.
    fn merge_rects(&self, rects: &mut SmallVec<[DamageRect; 8]>) {
        let mut merged = true;
        while merged && rects.len() > self.max_rects / 2 {
            merged = false;
            for i in 0..rects.len() {
                for j in (i + 1)..rects.len() {
                    if rects[i].intersects(&rects[j]) || self.are_adjacent(&rects[i], &rects[j]) {
                        rects[i] = rects[i].union(&rects[j]);
                        rects.remove(j);
                        merged = true;
                        break;
                    }
                }
                if merged {
                    break;
                }
            }
        }

        // If still too many, merge smallest first
        if rects.len() > self.max_rects {
            rects.sort_by_key(|r| r.area());
            while rects.len() > self.max_rects {
                let smallest = rects.remove(0);
                // Merge with closest
                let mut best_idx = 0;
                let mut best_dist = u32::MAX;
                for (i, r) in rects.iter().enumerate() {
                    let dist = self.rect_distance(&smallest, r);
                    if dist < best_dist {
                        best_dist = dist;
                        best_idx = i;
                    }
                }
                rects[best_idx] = rects[best_idx].union(&smallest);
            }
        }
    }

    /// Checks if two rects are adjacent (touching).
    fn are_adjacent(&self, a: &DamageRect, b: &DamageRect) -> bool {
        (a.x + a.width == b.x || b.x + b.width == a.x)
            && a.y < b.y + b.height
            && b.y < a.y + a.height
            || (a.y + a.height == b.y || b.y + b.height == a.y)
            && a.x < b.x + b.width
            && b.x < a.x + a.width
    }

    /// Distance between two rects.
    fn rect_distance(&self, a: &DamageRect, b: &DamageRect) -> u32 {
        let dx = if a.x + a.width < b.x {
            b.x - (a.x + a.width)
        } else if b.x + b.width < a.x {
            a.x - (b.x + b.width)
        } else {
            0
        };
        let dy = if a.y + a.height < b.y {
            b.y - (a.y + a.height)
        } else if b.y + b.height < a.y {
            a.y - (b.y + b.height)
        } else {
            0
        };
        dx + dy
    }
}

/// Damage region iterator for render passes.
pub struct DamageIterator {
    rects: SmallVec<[DamageRect; 8]>,
    index: usize,
}

impl DamageIterator {
    pub fn new(rects: SmallVec<[DamageRect; 8]>) -> Self {
        Self { rects, index: 0 }
    }
}

impl Iterator for DamageIterator {
    type Item = DamageRect;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index < self.rects.len() {
            let rect = self.rects[self.index];
            self.index += 1;
            Some(rect)
        } else {
            None
        }
    }
}

impl DamageTracker {
    /// Creates an iterator over damage rects.
    pub fn iter(&self) -> DamageIterator {
        DamageIterator::new(self.get_damage())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_damage_rect() {
        let rect = DamageRect::new(10, 20, 100, 50);
        assert_eq!(rect.area(), 5000);
        assert!(!rect.is_empty());

        let empty = DamageRect::default();
        assert!(empty.is_empty());
    }

    #[test]
    fn test_damage_rect_intersection() {
        let a = DamageRect::new(0, 0, 10, 10);
        let b = DamageRect::new(5, 5, 10, 10);
        let c = DamageRect::new(20, 20, 10, 10);

        assert!(a.intersects(&b));
        assert!(!a.intersects(&c));
    }

    #[test]
    fn test_damage_rect_union() {
        let a = DamageRect::new(0, 0, 10, 10);
        let b = DamageRect::new(15, 15, 10, 10);
        let union = a.union(&b);

        assert_eq!(union.x, 0);
        assert_eq!(union.y, 0);
        assert_eq!(union.width, 25);
        assert_eq!(union.height, 25);
    }

    #[test]
    fn test_damage_rect_clip() {
        let rect = DamageRect::new(5, 5, 20, 20);
        let bounds = DamageRect::new(10, 10, 10, 10);
        let clipped = rect.clip(&bounds);

        assert_eq!(clipped.x, 10);
        assert_eq!(clipped.y, 10);
        assert_eq!(clipped.width, 10);
        assert_eq!(clipped.height, 10);
    }

    #[test]
    fn test_damage_tracker() {
        let tracker = DamageTracker::new(80, 24);
        assert!(tracker.is_empty());

        tracker.add_cell(5, 10);
        assert_eq!(tracker.len(), 1);

        tracker.add_cell(6, 10);
        // Should merge with adjacent cell
        let damage = tracker.take_damage();
        assert_eq!(damage.len(), 1);
        assert_eq!(damage[0].width, 2);
    }

    #[test]
    fn test_damage_tracker_full() {
        let tracker = DamageTracker::new(80, 24);
        tracker.mark_full();
        assert!(tracker.is_full());

        tracker.clear();
        assert!(!tracker.is_full());
        assert!(tracker.is_empty());
    }

    #[test]
    fn test_damage_tracker_row() {
        let tracker = DamageTracker::new(80, 24);
        tracker.add_row(5);
        let damage = tracker.take_damage();
        assert_eq!(damage[0].width, 80);
        assert_eq!(damage[0].height, 1);
        assert_eq!(damage[0].y, 5);
    }
}