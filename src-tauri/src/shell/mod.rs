//! App shell: tray, popover window, refresh scheduler, settings side effects, crash log.
//! Owner: foundation. Areas plug in through the hook calls in `refresh_usage`/`refresh_limits`/
//! `apply_settings` (see ARCHITECTURE.md), never by editing this file.

mod i18n;
pub mod popover;
pub mod tray;

use crate::model::{PopoverOpened, EV_LIMITS, EV_SETTINGS, EV_USAGE};
use crate::settings::Settings;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Listener, Manager, WebviewWindow, WindowEvent};
use tauri_plugin_notification::NotificationExt;

pub const TRAY_ID: &str = "main";
const SCHEDULER_TICK: Duration = Duration::from_secs(5);

pub fn notify(app: &AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}

/// A minimized window still reports `is_visible() == true` but sits off-screen, and with
/// `skipTaskbar` there is no taskbar button to restore it (e.g. after Win+D). Treat it as hidden.
pub fn on_screen(w: &WebviewWindow) -> bool {
    w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false)
}

pub fn reveal(w: &WebviewWindow, focus: bool) {
    if w.is_minimized().unwrap_or(false) {
        let _ = w.unminimize();
    }
    let _ = w.show();
    if focus {
        let _ = w.set_focus();
    }
}

pub fn popover_visible(app: &AppHandle) -> bool {
    popover::visible(app)
}

/// Every area's `on_settings` hook, in setup order.
fn settings_hooks(app: &AppHandle, s: &Settings) {
    crate::providers::on_settings(app, s);
    crate::limits::on_settings(app, s);
    crate::companion::on_settings(app, s);
    crate::home::on_settings(app, s);
    crate::economy::on_settings(app, s);
    crate::pet::on_settings(app, s);
}

/// Stores settings and runs every area's reaction to them.
pub fn apply_settings(app: &AppHandle, value: Settings) -> Settings {
    let value = crate::settings::store(app, value);
    settings_hooks(app, &value);
    tray::update(app);
    let _ = app.emit(EV_SETTINGS, &value);
    value
}

// ---- refresh pipeline ----

fn refresh_usage(app: &AppHandle) {
    let snap = crate::providers::refresh(app);
    tray::update(app);
    crate::companion::on_usage(app, &snap);
    crate::home::on_usage(app, &snap);
    let _ = app.emit(EV_USAGE, &snap);
}

fn refresh_limits(app: &AppHandle) {
    let all = crate::limits::refresh(app);
    crate::economy::on_limits(app, &all);
    tray::update(app);
    let _ = app.emit(EV_LIMITS, all);
}

/// One loop for both refreshes so the interval setting applies without a restart.
fn scheduler(app: AppHandle) {
    refresh_limits(&app);
    refresh_usage(&app);
    let (mut last_usage, mut last_limits) = (Instant::now(), Instant::now());
    loop {
        std::thread::sleep(SCHEDULER_TICK);
        let minutes = crate::settings::current(&app).shell.refresh_minutes;
        if minutes == 0 {
            continue; // manual only
        }
        let interval = Duration::from_secs(minutes as u64 * 60);
        if last_limits.elapsed() >= interval /* 429 backoff lives in limits */ {
            refresh_limits(&app);
            last_limits = Instant::now();
        }
        if last_usage.elapsed() >= interval {
            refresh_usage(&app);
            last_usage = Instant::now();
        }
    }
}

/// Release builds abort on panic without a message (panic = "abort", no console on Windows),
/// so the reason is appended to `crash.log` in the app data folder first.
pub fn install_crash_log(path: PathBuf) {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        use std::io::Write;
        let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
        let line = format!(
            "{} v{} thread '{thread}': {info}\n{}\n\n",
            chrono::Local::now().to_rfc3339(),
            env!("CARGO_PKG_VERSION"),
            std::backtrace::Backtrace::force_capture()
        );
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
            let _ = f.write_all(line.as_bytes());
        }
        default(info);
    }));
}

// ---- wiring ----

/// Runs after every area's `setup`, so their state exists.
pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    app.manage(Mutex::new(popover::PopoverState::default()));
    settings_hooks(app.handle(), &crate::settings::current(app.handle()));
    tray::create(app)?;
    // Growth % also moves outside refreshes (Rare Candy, egg purchase, hatch follow-ups).
    let h = app.handle().clone();
    app.listen(crate::model::EV_COMPANION, move |_| tray::update(&h));
    crate::pet::start_tray_sprite(app.handle());
    let handle = app.handle().clone();
    std::thread::spawn(move || scheduler(handle));
    Ok(())
}

pub fn on_window_event(w: &tauri::Window, e: &WindowEvent) {
    match e {
        // Closing hides; the app lives in the tray.
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            let _ = w.hide();
        }
        WindowEvent::Focused(false) if w.label() == popover::LABEL => popover::on_blur(w.app_handle()),
        _ => {}
    }
}

/// A second launch (shortcut, autostart, installer) just opens the running copy's popover.
pub fn on_second_instance(app: &AppHandle) {
    popover::show(app, PopoverOpened::default());
}

// ---- commands ----

#[tauri::command]
fn get_settings(app: AppHandle) -> Settings {
    crate::settings::current(&app)
}

#[tauri::command]
fn set_settings(app: AppHandle, value: Settings) -> Settings {
    apply_settings(&app, value)
}

#[tauri::command]
pub fn refresh_now(app: AppHandle) {
    // Off the UI thread: a hatch or the limits API may hit the network.
    std::thread::spawn(move || {
        refresh_limits(&app);
        refresh_usage(&app);
    });
}

/// "main" shows the popover (the pet's click), "pet" flips the floating pet setting.
#[tauri::command]
fn toggle_window(app: AppHandle, label: String) {
    if label == "pet" {
        let mut s = crate::settings::current(&app);
        s.pet.floating_pet_enabled = !s.pet.floating_pet_enabled;
        apply_settings(&app, s);
    } else if popover_visible(&app) {
        popover::hide(&app);
    } else {
        popover::show(&app, PopoverOpened::default());
    }
}

#[tauri::command]
fn show_popover(app: AppHandle, opened: Option<PopoverOpened>) {
    popover::show(&app, opened.unwrap_or_default());
}

#[tauri::command]
fn hide_popover(app: AppHandle) {
    popover::hide(&app);
}

/// The webview switched between main content (360×604) and Settings (360×460).
#[tauri::command]
fn set_popover_mode(app: AppHandle, settings: bool) {
    popover::set_mode(&app, settings);
}

#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

/// The popover's drag area was grabbed: from now on it stays where the user puts it.
#[tauri::command]
fn popover_drag_started(app: AppHandle) {
    popover::mark_moved(&app);
}

crate::area_commands![get_settings, set_settings, refresh_now, toggle_window, show_popover, hide_popover, set_popover_mode, popover_drag_started, quit];
