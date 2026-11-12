//! The popover: undecorated 360×604 window (360×460 for Settings) anchored to the tray icon,
//! clamped to the monitor work area, hidden on blur. ui-shell.md §3.7, §6.

use super::{on_screen, reveal, TRAY_ID};
use crate::model::{PopoverOpened, EV_POPOVER_OPENED};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewWindow};

pub const LABEL: &str = "main";
pub const WIDTH: f64 = 360.0;
pub const HEIGHT: f64 = 604.0;
pub const SETTINGS_HEIGHT: f64 = 460.0;
/// Gap to the work-area edges and the icon, physical px.
const MARGIN: f64 = 8.0;
/// A blur followed by focus within this window is a flap (menus, dialogs), not a dismissal.
const BLUR_DEBOUNCE: Duration = Duration::from_millis(120);
/// Clicking the tray icon blurs the popover first; ignore the click that caused that hide.
const REOPEN_GUARD: Duration = Duration::from_millis(250);

#[derive(Default)]
pub struct PopoverState {
    hidden_at: Option<Instant>,
    /// The user dragged the popover somewhere: keep it there (until the app quits) instead of
    /// re-anchoring it to the tray icon on every open / Settings switch.
    moved: bool,
}

/// The webview is about to start a window drag.
pub fn mark_moved(app: &AppHandle) {
    app.state::<Mutex<PopoverState>>().lock().unwrap().moved = true;
}

/// Keeps a `size` window whose top-left is at `pos` inside `work`.
pub fn clamp_into(pos: (f64, f64), size: (f64, f64), work: Rect) -> (f64, f64) {
    let clamp = |v: f64, lo: f64, hi: f64| v.min(hi).max(lo);
    (
        clamp(pos.0, work.x, work.x + work.w - size.0),
        clamp(pos.1, work.y, work.y + work.h - size.1),
    )
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Window origin for a `size` popover: centred on the icon, above it when the icon is in the
/// lower half of the monitor (taskbar at the bottom) else below, then clamped to the work area.
pub fn anchor(icon: Rect, size: (f64, f64), work: Rect) -> (f64, f64) {
    let (w, h) = size;
    let x = icon.x + icon.w / 2.0 - w / 2.0;
    let below = icon.y + icon.h / 2.0 < work.y + work.h / 2.0;
    let y = if below { icon.y + icon.h + MARGIN } else { icon.y - h - MARGIN };
    let clamp = |v: f64, lo: f64, hi: f64| v.min(hi).max(lo);
    (
        clamp(x, work.x + MARGIN, work.x + work.w - w - MARGIN),
        clamp(y, work.y + MARGIN, work.y + work.h - h - MARGIN),
    )
}

pub fn window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(LABEL)
}

pub fn visible(app: &AppHandle) -> bool {
    window(app).is_some_and(|w| on_screen(&w))
}

/// Tray icon rect, or a 1×1 point at the work area's bottom-right corner when unknown.
fn icon_rect(app: &AppHandle, scale: f64) -> Option<Rect> {
    let r = app.tray_by_id(TRAY_ID)?.rect().ok()??;
    let p = r.position.to_physical::<f64>(scale);
    let s = r.size.to_physical::<f64>(scale);
    Some(Rect { x: p.x, y: p.y, w: s.width, h: s.height })
}

/// Sizes and positions the popover for the current mode.
fn place(app: &AppHandle, w: &WebviewWindow, settings: bool) {
    let height = if settings { SETTINGS_HEIGHT } else { HEIGHT };
    let _ = w.set_size(LogicalSize::new(WIDTH, height));
    if app.state::<Mutex<PopoverState>>().lock().unwrap().moved {
        // Stay where the user left it, only pulled back if the new size would cross the edge.
        if let (Ok(pos), Ok(Some(monitor))) = (w.outer_position(), w.current_monitor()) {
            let wa = monitor.work_area();
            let work = Rect {
                x: wa.position.x as f64,
                y: wa.position.y as f64,
                w: wa.size.width as f64,
                h: wa.size.height as f64,
            };
            let s = monitor.scale_factor();
            let (x, y) = clamp_into((pos.x as f64, pos.y as f64), (WIDTH * s, height * s), work);
            let _ = w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
            return;
        }
    }
    let scale = w.scale_factor().unwrap_or(1.0);
    let icon = icon_rect(app, scale);
    let monitor = match icon {
        Some(i) => app.monitor_from_point(i.x + i.w / 2.0, i.y + i.h / 2.0).ok().flatten(),
        None => None,
    }
    .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else { return };
    let wa = monitor.work_area();
    let work = Rect {
        x: wa.position.x as f64,
        y: wa.position.y as f64,
        w: wa.size.width as f64,
        h: wa.size.height as f64,
    };
    let scale = monitor.scale_factor();
    let icon = icon.unwrap_or(Rect { x: work.x + work.w, y: work.y + work.h, w: 1.0, h: 1.0 });
    let (x, y) = anchor(icon, (WIDTH * scale, height * scale), work);
    let _ = w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

/// Shows the popover (never toggles it off) and tells the webview where to land.
pub fn show(app: &AppHandle, opened: PopoverOpened) {
    let Some(w) = window(app) else { return };
    place(app, &w, opened.settings);
    reveal(&w, true);
    let _ = app.emit(EV_POPOVER_OPENED, opened);
}

pub fn hide(app: &AppHandle) {
    if let Some(w) = window(app) {
        let _ = w.hide();
    }
    app.state::<Mutex<PopoverState>>().lock().unwrap().hidden_at = Some(Instant::now());
}

/// Left click on the tray icon.
pub fn toggle_from_tray(app: &AppHandle) {
    if visible(app) {
        hide(app);
        return;
    }
    let recently_hidden = app
        .state::<Mutex<PopoverState>>()
        .lock()
        .unwrap()
        .hidden_at
        .is_some_and(|t| t.elapsed() < REOPEN_GUARD);
    if !recently_hidden {
        show(app, PopoverOpened::default());
    }
}

/// Settings ↔ main content switch from the webview: resize in place, re-anchored.
pub fn set_mode(app: &AppHandle, settings: bool) {
    if let Some(w) = window(app) {
        place(app, &w, settings);
    }
}

/// `WindowEvent::Focused(false)` on the popover.
pub fn on_blur(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(BLUR_DEBOUNCE);
        if window(&app).is_some_and(|w| on_screen(&w) && !w.is_focused().unwrap_or(false)) {
            hide(&app);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_above_bottom_taskbar_and_clamps() {
        let work = Rect { x: 0.0, y: 0.0, w: 1920.0, h: 1040.0 }; // 40 px taskbar at the bottom
        let icon = Rect { x: 1800.0, y: 1044.0, w: 32.0, h: 32.0 };
        // Centred x would overflow the right edge; the bottom sits on the work area, not the taskbar.
        assert_eq!(anchor(icon, (360.0, 604.0), work), (1920.0 - 360.0 - 8.0, 1040.0 - 604.0 - 8.0));
        // Taskbar at the top: below the icon.
        let work = Rect { x: 0.0, y: 40.0, w: 1920.0, h: 1040.0 };
        let icon = Rect { x: 900.0, y: 4.0, w: 32.0, h: 32.0 };
        assert_eq!(anchor(icon, (360.0, 604.0), work), (736.0, 48.0));
    }

    #[test]
    fn moved_popover_is_kept_inside_the_work_area() {
        let work = Rect { x: 0.0, y: 0.0, w: 1920.0, h: 1040.0 };
        assert_eq!(clamp_into((100.0, 200.0), (360.0, 604.0), work), (100.0, 200.0));
        // Growing back from Settings (460) to 604 near the bottom edge pulls it up.
        assert_eq!(clamp_into((100.0, 500.0), (360.0, 604.0), work), (100.0, 436.0));
        assert_eq!(clamp_into((-50.0, -10.0), (360.0, 604.0), work), (0.0, 0.0));
    }
}
