//! Floating pet window geometry (pet-settings-misc.md §3.1, FloatingPetPanel.swift :67-89, :516-532, :786-796).
//! Windows is y-down: every rect here is physical px with `y` = top. The persisted pet origin is the
//! pet's bottom-left (`x`, `bottom`), so the panel grows upward when a bubble shows.

pub const BUBBLE_HEADROOM: f64 = 72.0;
pub const BUBBLE_MIN_WIDTH: f64 = 180.0;
pub const RUN_CURSOR_GAP: f64 = 16.0;
pub const DEFAULT_INSET: f64 = 24.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn right(&self) -> f64 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }
    pub fn intersects(&self, o: &Rect) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
    }
}

/// `(petSize, petSize)`, or `(max(petSize, 180), petSize + 72)` while a bubble or hearts show.
pub fn panel_size(pet: f64, showing_bubble: bool, scale: f64) -> (f64, f64) {
    if showing_bubble {
        (pet.max(BUBBLE_MIN_WIDTH * scale), pet + BUBBLE_HEADROOM * scale)
    } else {
        (pet, pet)
    }
}

/// Panel frame for a pet whose bottom-left is `(x, bottom)`: centred on the pet, bottom-aligned.
pub fn panel_frame(pet_x: f64, pet_bottom: f64, pet: f64, size: (f64, f64)) -> Rect {
    let inset = ((size.0 - pet) / 2.0).max(0.0);
    Rect { x: pet_x - inset, y: pet_bottom - size.1, w: size.0, h: size.1 }
}

/// Inverse of `panel_frame`: the pet's bottom-left inside a panel frame.
pub fn pet_origin(frame: Rect, pet: f64) -> (f64, f64) {
    (frame.x + ((frame.w - pet) / 2.0).max(0.0), frame.bottom())
}

/// Bottom-right of the work area, 24 pt in from both edges.
pub fn default_origin(work: Option<Rect>, pet: f64, scale: f64) -> (f64, f64) {
    match work {
        Some(w) => (w.right() - pet - DEFAULT_INSET * scale, w.bottom() - DEFAULT_INSET * scale),
        None => (120.0, 120.0 + pet),
    }
}

/// Where the panel sits beside the cursor: right of it and below it when that fits, else the
/// other side, then clamped to the work area (left and bottom win when the panel is too big).
pub fn run_origin(cursor: (f64, f64), size: (f64, f64), work: Rect, gap: f64) -> (f64, f64) {
    let (w, h) = size;
    let fits_right = cursor.0 + gap + w <= work.right();
    let fits_below = cursor.1 + gap + h <= work.bottom();
    let x = if fits_right { cursor.0 + gap } else { cursor.0 - gap - w };
    let y = if fits_below { cursor.1 + gap } else { cursor.1 - gap - h };
    (x.min(work.right() - w).max(work.x), y.max(work.y).min(work.bottom() - h))
}

/// Ease-in-ease-out (cubic) for the 0.6 s run.
pub fn ease(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Rect = Rect { x: 0.0, y: 0.0, w: 1920.0, h: 1040.0 };

    #[test]
    fn panel_grows_upward_around_the_pet() {
        assert_eq!(panel_size(96.0, false, 1.0), (96.0, 96.0));
        assert_eq!(panel_size(96.0, true, 1.0), (180.0, 168.0));
        assert_eq!(panel_size(384.0, true, 1.5), (384.0, 384.0 + 108.0));
        let f = panel_frame(1000.0, 900.0, 96.0, (180.0, 168.0));
        assert_eq!(f, Rect { x: 958.0, y: 732.0, w: 180.0, h: 168.0 });
        assert_eq!(pet_origin(f, 96.0), (1000.0, 900.0));
        assert_eq!(default_origin(Some(WORK), 96.0, 1.0), (1800.0, 1016.0));
    }

    #[test]
    fn runs_beside_the_cursor_and_clamps() {
        let size = (180.0, 168.0);
        assert_eq!(run_origin((500.0, 400.0), size, WORK, 16.0), (516.0, 416.0));
        // Bottom-right corner: flips to the left and above.
        assert_eq!(run_origin((1900.0, 1030.0), size, WORK, 16.0), (1704.0, 846.0));
        // Bigger than the screen: left and bottom win.
        let tiny = Rect { x: 0.0, y: 0.0, w: 100.0, h: 100.0 };
        assert_eq!(run_origin((50.0, 50.0), size, tiny, 16.0), (0.0, -68.0));
        assert_eq!((ease(0.0), ease(0.5), ease(1.0)), (0.0, 0.5, 1.0));
    }
}
