//! Positions the strip/popup next to the caret or selection, inside the monitor work area.
pub use crate::core::types::{Point, Rect, Size};

/// Above the anchor, left-aligned; below if there's no room above; clamped to `work_area`.
pub fn place_near(anchor: Rect, size: Size, work_area: Rect, gap: i32) -> Point {
    let mut y = anchor.y - size.h - gap;
    if y < work_area.y {
        y = anchor.y + anchor.h + gap;
    }
    let max_x = work_area.x + work_area.w - size.w;
    let max_y = work_area.y + work_area.h - size.h;
    Point { x: anchor.x.min(max_x).max(work_area.x), y: y.min(max_y).max(work_area.y) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: Size = Size { w: 300, h: 40 };
    const WORK: Rect = Rect { x: 0, y: 0, w: 1920, h: 1040 };

    #[test]
    fn above_when_room() {
        let a = Rect { x: 500, y: 500, w: 2, h: 20 };
        assert_eq!(place_near(a, SIZE, WORK, 8), Point { x: 500, y: 452 });
    }
    #[test]
    fn flips_below_at_top() {
        let a = Rect { x: 500, y: 10, w: 2, h: 20 };
        assert_eq!(place_near(a, SIZE, WORK, 8).y, 38);
    }
    #[test]
    fn clamps_right_edge() {
        let a = Rect { x: 1800, y: 500, w: 2, h: 20 };
        assert_eq!(place_near(a, SIZE, WORK, 8).x, 1620);
    }
    #[test]
    fn clamps_left_edge_negative_monitor() {
        let work = Rect { x: -1920, y: 0, w: 1920, h: 1080 };
        let a = Rect { x: -1950, y: 500, w: 2, h: 20 };
        assert_eq!(place_near(a, SIZE, work, 8).x, -1920);
    }
    #[test]
    fn clamps_bottom_when_flipped_below() {
        let work = Rect { x: 0, y: 0, w: 1920, h: 60 };
        let a = Rect { x: 0, y: 5, w: 2, h: 20 };
        assert_eq!(place_near(a, SIZE, work, 8).y, 20);
    }
}
