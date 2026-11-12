//! Floating pet window, animated tray sprite, agent hooks, save transfer/snapshots, app log,
//! autostart and other app services. Owner: pet-settings-misc area (pet-settings-misc.md).

pub mod agent;
mod geometry;
pub mod log;
mod save;
mod sprite;
mod sys;

use crate::store::{load_json, save_json};
use geometry::Rect;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow, WindowEvent};
use tauri_plugin_autostart::ManagerExt;

pub const PET: &str = "pet";
pub const CALLOUT: &str = "pet-callout";
/// Payload `PetView`, to the pet window.
pub const EV_PET_STATE: &str = "pet-state";
/// Payload `LimitBubble`, to the pet window.
pub const EV_PET_LIMIT_ALERT: &str = "pet-limit-alert";
/// Payload `Option<String>`, to the callout window.
pub const EV_PET_CALLOUT: &str = "pet-callout";

const SULK_AFTER: u64 = 3 * 3600;
const PRAYERS_PER_REFRESH: u64 = 30;
const PRAYER_REFRESH_INTERVAL: Duration = Duration::from_secs(30);
const RUN_DURATION: Duration = Duration::from_millis(600);
const OBSERVE_TICK: Duration = Duration::from_secs(5);
const SNAPSHOT_CHECK: Duration = Duration::from_secs(10 * 60);
/// 6 pt above the pet, minus the 4 px shadow margin the callout webview keeps around its box.
const CALLOUT_GAP: f64 = 2.0;

/// Keys this area owns inside `settings.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PetSettings {
    #[serde(alias = "pet_enabled")]
    pub floating_pet_enabled: bool,
    pub always_faces_back: bool,
    /// 48…384 step 8
    pub floating_pet_size: u32,
    pub floating_pet_bubble_alerts: bool,
    pub floating_pet_agent_activity: bool,
    /// "powerSaver" | "balanced" | "smooth" — sprite frame floor 0.4 / 0.2 / 0.1 s
    pub animation_quality: String,
}

impl Default for PetSettings {
    fn default() -> Self {
        PetSettings {
            floating_pet_enabled: false,
            always_faces_back: false,
            floating_pet_size: 96,
            floating_pet_bubble_alerts: true,
            floating_pet_agent_activity: true,
            animation_quality: "powerSaver".into(),
        }
    }
}

impl PetSettings {
    pub fn sanitize(&mut self) {
        self.floating_pet_size = (self.floating_pet_size.clamp(48, 384) + 4) / 8 * 8;
        if !["powerSaver", "balanced", "smooth"].contains(&self.animation_quality.as_str()) {
            self.animation_quality = "powerSaver".into();
        }
    }
}

/// `pet.json`: what survives a restart.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Persist {
    /// Pet bottom-left `(x, bottom)` in physical screen px.
    origin: Option<(f64, f64)>,
    /// Prayers over the current egg (reset once it hatches).
    egg_prayers: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    Home,
    Running,
    Away,
    Returning,
}

/// What the pet webview renders besides settings/companion/usage.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PetView {
    pub faces_back: bool,
    /// Seconds without usage while sulking (≥ 3 h), else `None`.
    pub sulk_idle: Option<u64>,
    pub egg_prayers: u64,
    pub motion_allowed: bool,
    pub low_power: bool,
    pub agent: agent::Summary,
}

/// A limit alert for the pet's speech bubble (`UsageStore.LimitAlert`).
#[derive(Debug, Clone, Serialize)]
pub struct LimitBubble {
    pub critical: bool,
    /// Display name of the window, e.g. "Claude 5-hour session".
    pub window: String,
    /// 0…100.
    pub utilization: f64,
    pub account: Option<String>,
}

struct Runtime {
    path: PathBuf,
    persist: Persist,
    tracker: agent::Tracker,
    timer_gen: u64,
    phase: Phase,
    run_gen: u64,
    away: Option<(f64, f64)>,
    bubble: bool,
    /// Positions this module set, so `Moved` can tell them from a user drag.
    placed: VecDeque<(i32, i32)>,
    callout: Option<String>,
    prayer_milestone: u64,
    prayer_refresh_at: Option<Instant>,
    usage_seen: Option<(String, u64)>,
    rise_at: Option<SystemTime>,
    sulk_idle: Option<u64>,
    low_power: bool,
    reduce_motion: bool,
    last_view: Option<PetView>,
}

impl Runtime {
    fn save(&self) {
        save_json(&self.path, &self.persist);
    }
    fn motion_allowed(&self) -> bool {
        !self.low_power && !self.reduce_motion
    }
}

fn rt(app: &AppHandle) -> MutexGuard<'_, Runtime> {
    app.state::<Mutex<Runtime>>().inner().lock().unwrap()
}

/// State folder: `PTB_STATE_DIR` when set (trimmed, non-empty), else the app data folder.
pub fn state_dir(app: &tauri::App) -> tauri::Result<PathBuf> {
    match std::env::var("PTB_STATE_DIR").map(|s| s.trim().to_string()) {
        Ok(s) if !s.is_empty() => Ok(PathBuf::from(s)),
        _ => app.path().app_data_dir(),
    }
}

// ---- subject (representative or active, like `displayed()` in shared.ts) ----

/// Species the tray and pet show, with shininess; `None` = egg.
fn subject(app: &AppHandle) -> Option<(u32, bool)> {
    let rep = crate::settings::current(app).companion.representative;
    let store = app.state::<crate::companion::SharedStore>();
    let s = &store.lock().unwrap().state;
    if let Some(id) = rep {
        let in_dex = s.dex.iter().filter(|d| d.chain.contains(&id));
        let active = s.active.as_ref().filter(|m| m.path_ids[..=m.stage_index.min(m.path_ids.len() - 1)].contains(&id));
        let shiny = in_dex.clone().any(|d| d.is_shiny) || active.is_some_and(|m| m.is_shiny);
        if in_dex.count() > 0 || active.is_some() {
            return Some((id, shiny));
        }
    }
    s.active.as_ref().map(|m| (m.current_id(), m.is_shiny))
}

fn has_active(app: &AppHandle) -> bool {
    app.state::<crate::companion::SharedStore>().lock().unwrap().state.active.is_some()
}

/// Back sprite on every surface: forced by the setting or while sulking. An egg never turns around.
pub fn faces_back(app: &AppHandle) -> bool {
    has_active(app) && (crate::settings::current(app).pet.always_faces_back || rt(app).sulk_idle.is_some())
}

fn view(app: &AppHandle) -> PetView {
    let faces_back = faces_back(app);
    let r = rt(app);
    PetView {
        faces_back,
        sulk_idle: r.sulk_idle,
        egg_prayers: r.persist.egg_prayers,
        motion_allowed: r.motion_allowed(),
        low_power: r.low_power,
        agent: r.tracker.summary(),
    }
}

fn emit_view(app: &AppHandle) {
    let v = view(app);
    let mut r = rt(app);
    if r.last_view.as_ref() != Some(&v) {
        r.last_view = Some(v.clone());
        drop(r);
        let _ = app.emit_to(PET, EV_PET_STATE, v);
    }
}

// ---- window geometry ----

fn pet_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(PET)
}

fn visible(app: &AppHandle) -> bool {
    pet_window(app).is_some_and(|w| crate::shell::on_screen(&w))
}

fn work_areas(app: &AppHandle) -> Vec<Rect> {
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| {
            let a = m.work_area();
            Rect { x: a.position.x as f64, y: a.position.y as f64, w: a.size.width as f64, h: a.size.height as f64 }
        })
        .collect()
}

fn primary_work(app: &AppHandle) -> Option<Rect> {
    let m = app.primary_monitor().ok().flatten()?;
    let a = m.work_area();
    Some(Rect { x: a.position.x as f64, y: a.position.y as f64, w: a.size.width as f64, h: a.size.height as f64 })
}

/// Pet edge in physical px and the window's scale factor.
fn pet_px(app: &AppHandle, w: &WebviewWindow) -> (f64, f64) {
    let scale = w.scale_factor().unwrap_or(1.0);
    (crate::settings::current(app).pet.floating_pet_size as f64 * scale, scale)
}

/// Saved origin when its frame still touches a screen, else bottom-right of the main screen.
fn home_origin(app: &AppHandle, pet: f64, scale: f64, size: (f64, f64)) -> (f64, f64) {
    let saved = rt(app).persist.origin;
    if let Some((x, b)) = saved {
        let frame = geometry::panel_frame(x, b, pet, size);
        if work_areas(app).iter().any(|a| a.intersects(&frame)) {
            return (x, b);
        }
    }
    geometry::default_origin(primary_work(app), pet, scale)
}

fn place(app: &AppHandle, w: &WebviewWindow, f: Rect) {
    let pos = (f.x.round() as i32, f.y.round() as i32);
    {
        let mut r = rt(app);
        r.placed.push_back(pos);
        while r.placed.len() > 16 {
            r.placed.pop_front();
        }
    }
    let _ = w.set_size(PhysicalSize::new(f.w.round() as u32, f.h.round() as u32));
    let _ = w.set_position(PhysicalPosition::new(pos.0, pos.1));
}

/// Frame for the current phase and bubble state.
fn target(app: &AppHandle, w: &WebviewWindow) -> Rect {
    let (pet, scale) = pet_px(app, w);
    let (phase, away, bubble) = {
        let r = rt(app);
        (r.phase, r.away, r.bubble)
    };
    let size = geometry::panel_size(pet, bubble, scale);
    let (x, b) = match (phase, away) {
        (Phase::Away, Some(o)) => o,
        _ => home_origin(app, pet, scale, size),
    };
    geometry::panel_frame(x, b, pet, size)
}

fn sync(app: &AppHandle) {
    let Some(w) = pet_window(app) else { return };
    if matches!(rt(app).phase, Phase::Running | Phase::Returning) {
        return; // the animation owns the frame
    }
    let f = target(app, &w);
    place(app, &w, f);
    reposition_callout(app);
}

fn outer_rect(w: &WebviewWindow) -> Option<Rect> {
    let p = w.outer_position().ok()?;
    let s = w.outer_size().ok()?;
    Some(Rect { x: p.x as f64, y: p.y as f64, w: s.width as f64, h: s.height as f64 })
}

/// Animates the window frame over 0.6 s; `done` runs only if no newer move superseded this one.
fn animate(app: &AppHandle, to: Rect, generation: u64, done: impl FnOnce(&AppHandle) + Send + 'static) {
    let Some(w) = pet_window(app) else { return };
    let Some(from) = outer_rect(&w) else { return };
    let app = app.clone();
    std::thread::spawn(move || {
        let start = Instant::now();
        loop {
            if rt(&app).run_gen != generation {
                return;
            }
            let t = start.elapsed().as_secs_f64() / RUN_DURATION.as_secs_f64();
            let k = geometry::ease(t);
            let lerp = |a: f64, b: f64| a + (b - a) * k;
            place(&app, &w, Rect { x: lerp(from.x, to.x), y: lerp(from.y, to.y), w: lerp(from.w, to.w), h: lerp(from.h, to.h) });
            if t >= 1.0 {
                break;
            }
            std::thread::sleep(Duration::from_millis(16));
        }
        if rt(&app).run_gen == generation {
            done(&app);
        }
    });
}

/// Runs beside the cursor when an agent reports `done` (once; a second `done` while away stays put).
fn run_to_cursor(app: &AppHandle) {
    let Some(w) = pet_window(app) else { return };
    let (pet, scale) = pet_px(app, &w);
    let Ok(c) = app.cursor_position() else { return sync(app) };
    let work = app
        .monitor_from_point(c.x, c.y)
        .ok()
        .flatten()
        .map(|m| {
            let a = m.work_area();
            Rect { x: a.position.x as f64, y: a.position.y as f64, w: a.size.width as f64, h: a.size.height as f64 }
        })
        .or_else(|| primary_work(app));
    let Some(work) = work else { return sync(app) };
    let size = geometry::panel_size(pet, true, scale);
    let (x, y) = geometry::run_origin((c.x, c.y), size, work, geometry::RUN_CURSOR_GAP * scale);
    let to = Rect { x, y, w: size.0, h: size.1 };
    let generation = {
        let mut r = rt(app);
        r.phase = Phase::Running;
        r.run_gen += 1;
        r.away = Some(geometry::pet_origin(to, pet));
        r.run_gen
    };
    hide_callout(app);
    animate(app, to, generation, |app| {
        rt(app).phase = Phase::Away;
        sync(app);
    });
}

fn return_home(app: &AppHandle) {
    let phase = rt(app).phase;
    if !matches!(phase, Phase::Away | Phase::Running) || !visible(app) {
        set_home(app);
        return sync(app);
    }
    if !rt(app).motion_allowed() {
        set_home(app);
        return sync(app);
    }
    let Some(w) = pet_window(app) else { return };
    let generation = {
        let mut r = rt(app);
        r.phase = Phase::Home; // so `target` resolves home
        r.away = None;
        r.run_gen += 1;
        r.run_gen
    };
    let to = target(app, &w);
    rt(app).phase = Phase::Returning;
    animate(app, to, generation, |app| {
        rt(app).phase = Phase::Home;
        sync(app);
    });
}

fn set_home(app: &AppHandle) {
    let mut r = rt(app);
    r.phase = Phase::Home;
    r.away = None;
    r.run_gen += 1;
}

/// `Moved` on the pet window: a user drag makes the new place home; our own moves are ignored.
fn on_pet_moved(app: &AppHandle, pos: PhysicalPosition<i32>) {
    {
        let mut r = rt(app);
        if let Some(i) = r.placed.iter().position(|p| *p == (pos.x, pos.y)) {
            r.placed.drain(..=i);
            return;
        }
    }
    let Some(w) = pet_window(app) else { return };
    let Some(frame) = outer_rect(&w) else { return };
    let (pet, _) = pet_px(app, &w);
    let origin = geometry::pet_origin(Rect { x: pos.x as f64, y: pos.y as f64, ..frame }, pet);
    {
        let mut r = rt(app);
        r.phase = Phase::Home;
        r.away = None;
        r.run_gen += 1;
        r.persist.origin = Some(origin);
        r.save();
    }
    reposition_callout(app);
}

// ---- hover callout window (click-through, separate like the macOS NSPanel) ----

fn callout_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(CALLOUT)
}

fn hide_callout(app: &AppHandle) {
    rt(app).callout = None;
    if let Some(c) = callout_window(app) {
        let _ = c.hide();
    }
}

/// Centred on the pet window, bottom edge 6 pt above its top.
fn reposition_callout(app: &AppHandle) {
    let (Some(c), Some(p)) = (callout_window(app), pet_window(app)) else { return };
    if rt(app).callout.is_none() {
        return;
    }
    let (Some(pf), Ok(cs)) = (outer_rect(&p), c.outer_size()) else { return };
    let scale = p.scale_factor().unwrap_or(1.0);
    let x = pf.x + pf.w / 2.0 - cs.width as f64 / 2.0;
    let y = pf.y - CALLOUT_GAP * scale - cs.height as f64;
    let _ = c.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
}

// ---- agent hooks ----

/// Handles `poketokenbar://` URLs. Returns true when `url` uses the scheme (handled or dropped), so
/// callers never surface UI for it.
pub fn handle_url(app: &AppHandle, url: &str) -> bool {
    if !url.trim().to_ascii_lowercase().starts_with(&format!("{}:", agent::SCHEME)) {
        return false;
    }
    let s = crate::settings::current(app).pet;
    if !(s.floating_pet_enabled && s.floating_pet_agent_activity) {
        return true;
    }
    if let Some(e) = agent::parse(url) {
        rt(app).tracker.apply(e, Instant::now());
        after_agent_change(app);
    }
    true
}

/// Second-instance arguments (Windows launches a new process per URL). True when any was ours.
pub fn handle_args(app: &AppHandle, args: &[String]) -> bool {
    args.iter().fold(false, |any, a| handle_url(app, a) || any)
}

fn arm_agent_timer(app: &AppHandle) {
    let (generation, deadline) = {
        let mut r = rt(app);
        r.timer_gen += 1;
        (r.timer_gen, r.tracker.deadline())
    };
    let Some(deadline) = deadline else { return };
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        let changed = {
            let mut r = rt(&app);
            if r.timer_gen != generation {
                return;
            }
            r.tracker.expire(Instant::now())
        };
        if changed {
            after_agent_change(&app);
        } else {
            arm_agent_timer(&app); // early fire: re-arm
        }
    });
}

fn after_agent_change(app: &AppHandle) {
    arm_agent_timer(app);
    let (done, runs) = {
        let r = rt(app);
        (r.tracker.summary().done.is_some(), r.phase != Phase::Away && r.motion_allowed())
    };
    if done {
        if runs && visible(app) { run_to_cursor(app) } else { sync(app) }
    } else {
        return_home(app);
    }
    emit_view(app);
}

/// Limit-alert speech bubble (6 s). The limits area calls this with the bubble it picked for a refresh.
#[allow(dead_code)] // called by the limits area's alert pipeline
pub fn show_limit_bubble(app: &AppHandle, alert: LimitBubble) {
    let s = crate::settings::current(app).pet;
    if s.floating_pet_enabled && s.floating_pet_bubble_alerts {
        let _ = app.emit_to(PET, EV_PET_LIMIT_ALERT, alert);
    }
}

// ---- observer: sulk, power state, prayers, auto snapshots ----

fn parse_time(s: &str) -> Option<SystemTime> {
    chrono::DateTime::parse_from_rfc3339(s).ok().map(SystemTime::from)
}

/// `effectiveIdle = min(reported, now − last observed rise of today's total)`; sulk from 3 h.
fn observe_sulk(app: &AppHandle, now: SystemTime) {
    let snap = crate::providers::current(app);
    let date = crate::model::today_key();
    let total = snap.today_total(&date);
    let reported = snap
        .providers
        .iter()
        .filter_map(|p| p.last_usage.as_deref().and_then(parse_time))
        .max()
        .map(|t| now.duration_since(t).unwrap_or_default().as_secs());
    let active = has_active(app);
    let mut r = rt(app);
    if let Some((d, t)) = &r.usage_seen {
        if *d == date && total > *t {
            r.rise_at = Some(now);
        }
    }
    r.usage_seen = Some((date, total));
    let observed = r.rise_at.map(|t| now.duration_since(t).unwrap_or_default().as_secs());
    let idle = match (reported, observed) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, _) => a,
    };
    r.sulk_idle = idle.filter(|i| active && snap.fetched_at.is_some() && *i >= SULK_AFTER);
}

fn auto_snapshot(app: &AppHandle) {
    let (file, state) = companion_state(app);
    if !save::has_progress(&state) {
        return;
    }
    let dir = save::snapshot_dir(&file);
    let newest = save::list_snapshots(&dir, valid_state).into_iter().next();
    if newest.is_none_or(|s| (chrono::Utc::now() - s.at).num_seconds() >= save::AUTO_SNAPSHOT_AGE_SECS) {
        if let Err(e) = save::create_snapshot(&dir, &envelope(app, &state), chrono::Local::now()) {
            log::write(&format!("auto snapshot failed: {e}"));
        }
    }
}

fn observer(app: AppHandle) {
    let mut last_snapshot_check: Option<Instant> = None;
    loop {
        observe_sulk(&app, SystemTime::now());
        let (low, reduce) = (sys::low_power(), sys::reduce_motion());
        let active = has_active(&app);
        {
            let mut r = rt(&app);
            r.low_power = low;
            r.reduce_motion = reduce;
            if active && r.persist.egg_prayers > 0 {
                r.persist.egg_prayers = 0; // the egg hatched
                r.prayer_milestone = 0;
                r.save();
            }
        }
        if last_snapshot_check.is_none_or(|t| t.elapsed() >= SNAPSHOT_CHECK) {
            last_snapshot_check = Some(Instant::now());
            auto_snapshot(&app);
        }
        emit_view(&app);
        std::thread::sleep(OBSERVE_TICK);
    }
}

// ---- save transfer ----

fn valid_state(v: &Value) -> bool {
    serde_json::from_value::<crate::companion::State>(v.clone()).is_ok()
}

/// State file path and its JSON.
fn companion_state(app: &AppHandle) -> (PathBuf, Value) {
    let store = app.state::<crate::companion::SharedStore>();
    let s = store.lock().unwrap();
    (s.path.clone(), serde_json::to_value(&s.state).unwrap_or_default())
}

fn device_name() -> String {
    ["COMPUTERNAME", "HOSTNAME"].iter().find_map(|k| std::env::var(k).ok()).unwrap_or_else(|| "PC".into())
}

fn envelope(app: &AppHandle, state: &Value) -> Vec<u8> {
    save::encode(state, &app.package_info().version.to_string(), &device_name(), chrono::Utc::now())
}

/// Backs up the current state, rebases crediting on today's usage, and installs `incoming`.
fn install_state(app: &AppHandle, mut incoming: Value) -> Result<(), save::SaveError> {
    let (file, current) = companion_state(app);
    let bytes = serde_json::to_vec_pretty(&current).map_err(|_| save::SaveError::BackupFailed)?;
    save::backup(&file, &bytes, chrono::Local::now()).map_err(|_| save::SaveError::BackupFailed)?;
    let snap = crate::providers::current(app);
    let date = crate::model::today_key();
    save::rebase(&mut incoming, &current, &snap.today_by_provider(&date), &date, snap.fetched_at.is_some());
    let state: crate::companion::State = serde_json::from_value(incoming).map_err(|_| save::SaveError::NotASaveFile)?;
    crate::companion::update(app, |s| *s = state);
    {
        let mut r = rt(app);
        r.persist.egg_prayers = 0;
        r.prayer_milestone = 0;
        r.save();
    }
    emit_view(app);
    Ok(())
}

#[derive(Serialize)]
struct ImportPreview {
    path: String,
    incoming: save::Summary,
    exported_at: String,
    source_device: String,
    current: save::Summary,
}

fn read_save(path: &Path) -> Result<save::Envelope, save::SaveError> {
    let bytes = std::fs::read(path).map_err(|_| save::SaveError::Io)?;
    save::decode(&bytes, valid_state)
}

// ---- setup / hooks ----

pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    let handle = app.handle();
    let data = state_dir(app)?;
    let log_dir = app.path().app_log_dir().unwrap_or_else(|_| data.join("logs"));
    log::init(log_dir, &data.join("crash.log"), &app.package_info().version.to_string());

    handle.plugin(tauri_plugin_dialog::init())?;
    handle.plugin(tauri_plugin_deep_link::init())?;
    handle.plugin(
        tauri::plugin::Builder::<tauri::Wry, ()>::new("pet-lifecycle")
            .on_event(|_, e| {
                if let tauri::RunEvent::Exit = e {
                    log::clean_exit();
                }
            })
            .build(),
    )?;
    {
        use tauri_plugin_deep_link::DeepLinkExt;
        #[cfg(any(windows, target_os = "linux"))]
        if let Err(e) = app.deep_link().register(agent::SCHEME) {
            log::write(&format!("deep link registration failed: {e}"));
        }
        let h = handle.clone();
        app.deep_link().on_open_url(move |e| {
            for u in e.urls() {
                handle_url(&h, u.as_str());
            }
        });
    }

    let path = data.join("pet.json");
    app.manage(Mutex::new(Runtime {
        persist: load_json(&path),
        path,
        tracker: agent::Tracker::default(),
        timer_gen: 0,
        phase: Phase::Home,
        run_gen: 0,
        away: None,
        bubble: false,
        placed: VecDeque::new(),
        callout: None,
        prayer_milestone: 0,
        prayer_refresh_at: None,
        usage_seen: None,
        rise_at: None,
        sulk_idle: None,
        low_power: sys::low_power(),
        reduce_motion: sys::reduce_motion(),
        last_view: None,
    }));
    if let Some(w) = pet_window(handle) {
        let _ = w.set_focusable(false);
        let _ = w.set_visible_on_all_workspaces(true);
        let h = handle.clone();
        w.on_window_event(move |e| {
            if let WindowEvent::Moved(p) = e {
                on_pet_moved(&h, *p);
            }
        });
    }
    if handle.autolaunch().is_enabled().unwrap_or(false) {
        sys::set_crash_restart(true);
    }
    let h = handle.clone();
    std::thread::spawn(move || observer(h));
    Ok(())
}

/// Called by the shell whenever settings change (and once at launch).
pub fn on_settings(app: &AppHandle, s: &crate::settings::Settings) {
    if !(s.pet.floating_pet_enabled && s.pet.floating_pet_agent_activity) {
        let mut r = rt(app);
        r.tracker.reset();
        r.timer_gen += 1;
    }
    let Some(w) = pet_window(app) else { return };
    if s.pet.floating_pet_enabled {
        sync(app);
        crate::shell::reveal(&w, false);
    } else {
        set_home(app); // a hidden pet comes back at home
        hide_callout(app);
        let _ = w.hide();
    }
    emit_view(app);
}

/// Tray sprite hook, called once after the tray exists.
pub fn start_tray_sprite(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        sprite::run(app, |app| {
            let s = crate::settings::current(app);
            let low = rt(app).low_power;
            let subject = subject(app);
            sprite::Key {
                species: subject.map(|s| s.0),
                shiny: subject.is_some_and(|s| s.1),
                back: faces_back(app),
                floor: sprite::frame_floor(&s.pet.animation_quality, low),
            }
        })
    });
}

// ---- commands ----

#[tauri::command]
fn get_autostart(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<bool, String> {
    let launcher = app.autolaunch();
    let r = if enabled { launcher.enable() } else { launcher.disable() };
    if let Err(e) = r {
        log::write(&format!("login item update failed: {e}"));
        return Err(e.to_string());
    }
    let on = launcher.is_enabled().unwrap_or(enabled);
    sys::set_crash_restart(on);
    Ok(on)
}

#[tauri::command]
fn pet_state(app: AppHandle) -> PetView {
    view(&app)
}

/// The pet webview shows (or stopped showing) a speech bubble or hearts: grow upward / shrink.
#[tauri::command]
fn pet_layout(app: AppHandle, showing_bubble: bool) {
    let changed = {
        let mut r = rt(&app);
        std::mem::replace(&mut r.bubble, showing_bubble) != showing_bubble
    };
    if changed {
        sync(&app);
    }
}

/// A click on the pet: dismiss agent bubbles; over an egg, pray. Returns the prayer count.
#[tauri::command]
fn pet_click(app: AppHandle, pray: bool) -> u64 {
    let changed = rt(&app).tracker.dismiss_bubble();
    if changed {
        after_agent_change(&app);
    }
    if !pray || has_active(&app) {
        return rt(&app).persist.egg_prayers;
    }
    let (count, refresh) = {
        let mut r = rt(&app);
        r.persist.egg_prayers += 1;
        r.save();
        let count = r.persist.egg_prayers;
        let milestone = count / PRAYERS_PER_REFRESH;
        if milestone < r.prayer_milestone {
            r.prayer_milestone = milestone; // a new egg
        }
        let refresh = milestone > r.prayer_milestone
            && r.prayer_refresh_at.is_none_or(|t| t.elapsed() >= PRAYER_REFRESH_INTERVAL);
        if refresh {
            r.prayer_milestone = milestone;
            r.prayer_refresh_at = Some(Instant::now());
        }
        (count, refresh)
    };
    if refresh {
        crate::shell::refresh_now(app.clone());
    }
    emit_view(&app);
    count
}

/// Shows the hover callout above the pet (`text` = None hides it). Size is the measured logical size.
#[tauri::command]
async fn pet_callout(app: AppHandle, text: Option<String>, width: f64, height: f64) -> Result<(), String> {
    let Some(text) = text.filter(|_| visible(&app)) else {
        hide_callout(&app);
        return Ok(());
    };
    rt(&app).callout = Some(text.clone());
    let c = match callout_window(&app) {
        Some(c) => c,
        None => {
            let c = tauri::WebviewWindowBuilder::new(&app, CALLOUT, tauri::WebviewUrl::App("index.html#pet".into()))
                .title("PokeTokenBar Callout")
                .transparent(true)
                .decorations(false)
                .shadow(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .resizable(false)
                .focused(false)
                .focusable(false)
                .visible(false)
                .visible_on_all_workspaces(true)
                .inner_size(width, height)
                .build()
                .map_err(|e| e.to_string())?;
            let _ = c.set_ignore_cursor_events(true);
            c
        }
    };
    let _ = c.set_size(tauri::LogicalSize::new(width, height));
    reposition_callout(&app);
    let _ = app.emit_to(CALLOUT, EV_PET_CALLOUT, Some(&text));
    let _ = c.show();
    Ok(())
}

#[tauri::command]
fn pet_callout_text(app: AppHandle) -> Option<String> {
    rt(&app).callout.clone()
}

#[tauri::command]
fn pet_log(message: String) {
    log::write(&message);
}

#[derive(Serialize)]
struct LogInfo {
    path: String,
    os: String,
}

#[tauri::command]
fn log_info() -> LogInfo {
    LogInfo {
        path: log::path().map(|p| p.display().to_string()).unwrap_or_default(),
        os: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
    }
}

/// Reveals the log file in Explorer.
#[tauri::command]
fn reveal_log() -> Result<(), String> {
    let path = log::path().ok_or("no log")?;
    if !path.exists() {
        log::write("log file opened");
    }
    tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| e.to_string())
}

#[tauri::command]
fn transfer_summary(app: AppHandle) -> save::Summary {
    save::summary(&companion_state(&app).1)
}

/// Save dialog → write the envelope → reveal it. `false` when cancelled.
#[tauri::command]
async fn export_save(app: AppHandle, title: String) -> Result<bool, save::SaveError> {
    use tauri_plugin_dialog::DialogExt;
    let Some(path) = app
        .dialog()
        .file()
        .set_title(title)
        .set_file_name(save::export_name(chrono::Local::now()))
        .add_filter("JSON", &["json"])
        .set_can_create_directories(true)
        .blocking_save_file()
        .and_then(|p| p.into_path().ok())
    else {
        return Ok(false);
    };
    let bytes = envelope(&app, &companion_state(&app).1);
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &bytes).and_then(|_| std::fs::rename(&tmp, &path)).map_err(|e| {
        log::write(&format!("save export failed: {e}"));
        save::SaveError::Io
    })?;
    let _ = tauri_plugin_opener::reveal_item_in_dir(&path);
    Ok(true)
}

/// Open dialog → read and validate. `None` when cancelled.
#[tauri::command]
async fn pick_import_save(app: AppHandle, title: String) -> Result<Option<ImportPreview>, save::SaveError> {
    use tauri_plugin_dialog::DialogExt;
    let Some(path) = app
        .dialog()
        .file()
        .set_title(title)
        .add_filter("JSON", &["json"])
        .blocking_pick_file()
        .and_then(|p| p.into_path().ok())
    else {
        return Ok(None);
    };
    let e = read_save(&path).inspect_err(|e| log::write(&format!("save import read failed: {e:?}")))?;
    Ok(Some(ImportPreview {
        path: path.display().to_string(),
        incoming: save::summary(&e.state),
        exported_at: e.exported_at.to_rfc3339(),
        source_device: e.source_device,
        current: save::summary(&companion_state(&app).1),
    }))
}

#[tauri::command]
fn apply_import(app: AppHandle, path: String) -> Result<save::Summary, save::SaveError> {
    let e = read_save(Path::new(&path))?;
    let summary = save::summary(&e.state);
    install_state(&app, e.state).inspect_err(|e| log::write(&format!("save import apply failed: {e:?}")))?;
    Ok(summary)
}

#[tauri::command]
fn create_snapshot(app: AppHandle) -> Result<(), save::SaveError> {
    let (file, state) = companion_state(&app);
    save::create_snapshot(&save::snapshot_dir(&file), &envelope(&app, &state), chrono::Local::now()).map(|_| ()).map_err(|e| {
        log::write(&format!("manual snapshot creation failed: {e}"));
        save::SaveError::Io
    })
}

#[tauri::command]
fn list_snapshots(app: AppHandle) -> Vec<save::Snapshot> {
    save::list_snapshots(&save::snapshot_dir(&companion_state(&app).0), valid_state)
}

/// Loads the target first (the safety snapshot may prune it), snapshots the current state, then applies.
#[tauri::command]
fn restore_snapshot(app: AppHandle, id: String) -> Result<save::Summary, save::SaveError> {
    let (file, current) = companion_state(&app);
    let dir = save::snapshot_dir(&file);
    let target = save::load_snapshot(&dir, &id, valid_state)?;
    let _ = save::create_snapshot(&dir, &envelope(&app, &current), chrono::Local::now());
    let summary = save::summary(&target);
    install_state(&app, target).inspect_err(|e| log::write(&format!("snapshot restore failed: {e:?}")))?;
    Ok(summary)
}

crate::area_commands![
    get_autostart, set_autostart, pet_state, pet_layout, pet_click, pet_callout, pet_callout_text, pet_log,
    log_info, reveal_log, transfer_summary, export_save, pick_import_save, apply_import, create_snapshot,
    list_snapshots, restore_snapshot,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_snaps_to_steps_of_eight() {
        let mut s = PetSettings { floating_pet_size: 30, ..Default::default() };
        s.sanitize();
        assert_eq!(s.floating_pet_size, 48);
        s.floating_pet_size = 101;
        s.sanitize();
        assert_eq!(s.floating_pet_size, 104);
        s.floating_pet_size = 999;
        s.sanitize();
        assert_eq!(s.floating_pet_size, 384);
    }
}
