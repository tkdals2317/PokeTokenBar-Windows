mod companion;
mod limits;
mod pokeapi;
mod settings;
mod usage;

use companion::{load_json, save_json, Event, Store};
use serde_json::{json, Value};
use settings::Settings;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, State, WindowEvent,
};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_notification::NotificationExt;

const TRAY_ID: &str = "main";
/// The limits API is never polled more often than this, whatever the refresh interval.
const MIN_LIMITS_INTERVAL: Duration = Duration::from_secs(5 * 60);
const SCHEDULER_TICK: Duration = Duration::from_secs(5);

struct SettingsStore {
    path: PathBuf,
    value: Settings,
}

type SharedStore = Mutex<Store>;
type SharedSettings = Mutex<SettingsStore>;
type SharedLimits = Mutex<Vec<limits::ProviderLimits>>;
type SharedUsage = Mutex<usage::TodayUsage>;
/// "provider.window" → alert tier already notified, so each crossing alerts once.
type SharedAlerts = Mutex<HashMap<String, u8>>;

fn current_settings(app: &AppHandle) -> Settings {
    app.state::<SharedSettings>().lock().unwrap().value.clone()
}

// ---- commands ----

#[tauri::command]
fn today_usage(state: State<SharedUsage>) -> usage::TodayUsage {
    state.lock().unwrap().clone()
}

#[tauri::command]
fn limits(state: State<SharedLimits>) -> Vec<limits::ProviderLimits> {
    state.lock().unwrap().clone()
}

#[tauri::command]
fn companion(store: State<SharedStore>) -> Value {
    view(&store.lock().unwrap())
}

#[tauri::command]
fn get_settings(app: AppHandle) -> Settings {
    current_settings(&app)
}

#[tauri::command]
fn set_settings(app: AppHandle, value: Settings) -> Settings {
    let value = value.sanitized();
    {
        let state = app.state::<SharedSettings>();
        let mut s = state.lock().unwrap();
        s.value = value.clone();
        save_json(&s.path, &s.value);
    }
    apply_pet_visibility(&app, value.pet_enabled);
    update_tray(&app);
    let _ = app.emit("settings", &value);
    value
}

#[tauri::command]
fn set_difficulty(app: AppHandle, value: f64) -> Value {
    let store = app.state::<SharedStore>();
    let mut s = store.lock().unwrap();
    s.state.set_difficulty(value);
    s.save();
    let v = view(&s);
    let _ = app.emit("companion", &v);
    v
}

#[tauri::command]
fn get_autostart(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<bool, String> {
    let launcher = app.autolaunch();
    let r = if enabled { launcher.enable() } else { launcher.disable() };
    r.map_err(|e| e.to_string())?;
    Ok(launcher.is_enabled().unwrap_or(enabled))
}

#[tauri::command]
fn refresh_now(app: AppHandle) {
    // Off the UI thread: a hatch or the limits API may hit the network.
    std::thread::spawn(move || {
        refresh_limits(&app);
        refresh(&app);
    });
}

#[tauri::command]
fn toggle_window(app: AppHandle, label: String) {
    toggle(&app, &label);
}

#[tauri::command]
fn quit(app: AppHandle) {
    app.exit(0);
}

// ---- refresh ----

/// What the UI needs: saved state plus the thresholds it can't derive.
fn view(store: &Store) -> Value {
    let s = &store.state;
    json!({
        "active": s.active,
        "egg_usage": s.egg_usage,
        "egg_threshold": s.egg_threshold(),
        "threshold": s.active.as_ref().map(|m| m.threshold(s.difficulty())),
        "difficulty": s.difficulty(),
        "dex": s.dex,
    })
}

fn notify(app: &AppHandle, title: &str, body: &str) {
    let _ = app.notification().builder().title(title).body(body).show();
}

fn refresh_limits(app: &AppHandle) {
    let all = limits::read_all();
    let s = current_settings(app);
    {
        let alerts = app.state::<SharedAlerts>();
        let mut alerted = alerts.lock().unwrap();
        for p in &all {
            for w in &p.windows {
                let key = format!("{}.{}", p.provider, w.key);
                let tier = settings::alert_tier(&s, w.used_percent);
                let before = alerted.insert(key, tier).unwrap_or(0);
                if s.limit_notifications && tier > before {
                    let title = settings::text(&s.language, if tier == 2 { "crit" } else { "warn" });
                    notify(app, title, &format!("{} {} {:.0}%", p.provider, w.key, w.used_percent));
                }
            }
        }
    }
    *app.state::<SharedLimits>().lock().unwrap() = all.clone();
    update_tray(app);
    let _ = app.emit("limits", all);
}

/// Tray tooltip (and macOS menu bar title) from the cached usage and limits.
fn update_tray(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    let s = current_settings(app);
    let total = app.state::<SharedUsage>().lock().unwrap().total;
    let peak = limits::peak(&app.state::<SharedLimits>().lock().unwrap());
    let mut parts = vec![];
    if s.tray_tokens {
        parts.push(format!("{} {}", settings::text(&s.language, "today"), usage::compact(total)));
    }
    if let (true, Some(p)) = (s.tray_limit, peak) {
        let p = if s.limit_display == "remaining" { 100.0 - p } else { p };
        parts.push(format!("{} {p:.0}%", settings::text(&s.language, "limit")));
    }
    let label = parts.join(" · ");
    let tooltip = if label.is_empty() { "PokeTokenBar".to_string() } else { format!("PokeTokenBar · {label}") };
    let _ = tray.set_tooltip(Some(tooltip));
    // Text next to the icon only renders on macOS; Windows shows the tooltip.
    let _ = tray.set_title(Some(label));
}

/// Hatches the egg if it's ready. Network calls run without holding the store lock.
fn hatch_if_ready(app: &AppHandle, rng: &mut fastrand::Rng) -> Vec<Event> {
    let store = app.state::<SharedStore>();
    let dir = {
        let s = store.lock().unwrap();
        if !s.state.ready_to_hatch() {
            return vec![];
        }
        s.dir().to_path_buf()
    };
    // Any failure keeps the egg; the next tick retries.
    let Ok(index) = pokeapi::base_index(&dir) else { return vec![] };
    let Some(base) = companion::choose_base(&index, &store.lock().unwrap().state, rng) else {
        return vec![];
    };
    let Ok(line) = pokeapi::line(base) else { return vec![] };
    let mut s = store.lock().unwrap();
    if !s.state.ready_to_hatch() {
        return vec![];
    }
    s.state.hatch(line, rng)
}

fn refresh(app: &AppHandle) {
    let u = usage::read_today();
    *app.state::<SharedUsage>().lock().unwrap() = u.clone();
    update_tray(app);

    let today = HashMap::from([
        ("claude".to_string(), u.claude.total()),
        ("codex".to_string(), u.codex.total()),
    ]);
    let date = chrono::Local::now().format("%Y-%m-%d").to_string();
    let mut rng = fastrand::Rng::new();
    let store = app.state::<SharedStore>();
    let mut events = {
        let mut s = store.lock().unwrap();
        let delta = s.state.credit(&today, &date);
        s.state.feed(delta, &mut rng)
    };
    events.extend(hatch_if_ready(app, &mut rng));

    let settings = current_settings(app);
    let s = store.lock().unwrap();
    s.save();
    let _ = app.emit("usage", u);
    let _ = app.emit("companion", view(&s));
    for e in events {
        if settings.companion_notifications {
            let (key, id) = match e {
                Event::Hatched { id, shiny: true } => ("shiny", id),
                Event::Hatched { id, .. } => ("hatched", id),
                Event::Evolved { id } => ("evolved", id),
                Event::Graduated { id } => ("graduated", id),
            };
            notify(app, settings::text(&settings.language, key), &s.state.name_of(id, &settings.language));
        }
        let _ = app.emit("companion-event", e);
    }
}

/// One loop for both refreshes so the interval setting applies without a restart.
fn scheduler(app: AppHandle) {
    refresh_limits(&app);
    refresh(&app);
    let (mut last_usage, mut last_limits) = (Instant::now(), Instant::now());
    loop {
        std::thread::sleep(SCHEDULER_TICK);
        let minutes = current_settings(&app).refresh_minutes;
        if minutes == 0 {
            continue; // manual only
        }
        let interval = Duration::from_secs(minutes as u64 * 60);
        if last_limits.elapsed() >= interval.max(MIN_LIMITS_INTERVAL) {
            refresh_limits(&app);
            last_limits = Instant::now();
        }
        if last_usage.elapsed() >= interval {
            refresh(&app);
            last_usage = Instant::now();
        }
    }
}

/// Release builds abort on panic without a message (panic = "abort", no console on Windows),
/// so the reason is appended to `crash.log` in the app data folder first.
fn install_crash_log(path: PathBuf) {
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

// ---- windows ----

fn apply_pet_visibility(app: &AppHandle, visible: bool) {
    if let Some(w) = app.get_webview_window("pet") {
        let _ = if visible { w.show() } else { w.hide() };
    }
}

/// "main" is the usage popover, "pet" the floating Pokémon (its visibility is a setting).
fn toggle(app: &AppHandle, label: &str) {
    let Some(w) = app.get_webview_window(label) else { return };
    let visible = w.is_visible().unwrap_or(false);
    if label == "pet" {
        let mut s = current_settings(app);
        s.pet_enabled = !visible;
        set_settings(app.clone(), s);
        return;
    }
    if visible {
        let _ = w.hide();
    } else {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // Must be first: a second launch (shortcut, autostart, installer) exits and just
        // brings the running copy's window forward instead of adding another pet and tray icon.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            today_usage,
            companion,
            limits,
            get_settings,
            set_settings,
            set_difficulty,
            get_autostart,
            set_autostart,
            refresh_now,
            toggle_window,
            quit
        ])
        .setup(|app| {
            // Menu bar app like the original: no Dock icon.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            install_crash_log(dir.join("crash.log"));
            let settings_path = dir.join("settings.json");
            let settings = load_json::<Settings>(&settings_path).sanitized();
            apply_pet_visibility(app.handle(), settings.pet_enabled);
            app.manage::<SharedSettings>(Mutex::new(SettingsStore { path: settings_path, value: settings }));
            app.manage::<SharedStore>(Mutex::new(Store::load(dir.join("companion.json"))));
            app.manage::<SharedLimits>(Mutex::new(vec![]));
            app.manage::<SharedUsage>(Mutex::new(usage::TodayUsage::default()));
            app.manage::<SharedAlerts>(Mutex::new(HashMap::new()));

            let refresh_i = MenuItem::with_id(app, "refresh", "Refresh", true, None::<&str>)?;
            let pet_i = MenuItem::with_id(app, "pet", "Show/Hide Pet", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&refresh_i, &pet_i, &quit_i])?;

            TrayIconBuilder::with_id(TRAY_ID)
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, e| match e.id.as_ref() {
                    "refresh" => refresh_now(app.clone()),
                    "pet" => toggle(app, "pet"),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, e| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = e
                    {
                        toggle(tray.app_handle(), "main");
                    }
                })
                .build(app)?;

            let handle = app.handle().clone();
            std::thread::spawn(move || scheduler(handle));
            Ok(())
        })
        .on_window_event(|w, e| {
            // Closing the popover hides it; the app lives in the tray.
            if let WindowEvent::CloseRequested { api, .. } = e {
                api.prevent_close();
                let _ = w.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
