//! Pokémon companion: egg → hatch → evolve → graduate into the Pokédex (docs/parity/companion.md).
//! Rules live in `state.rs` (pure); this file wires them to usage refreshes, PokéAPI and the UI.
//! Owner: companion area. See docs/parity/ARCHITECTURE.md.

pub mod api;
pub mod balance;
pub mod form;
pub mod pokeapi;
pub mod profile;
pub mod state;
pub mod text;

#[allow(unused_imports)] // re-exported for other areas (economy reads `clamp_difficulty`)
pub use balance::{DIFFICULTY_RANGE, EGG_HATCH_THRESHOLD, Rarity, clamp_difficulty, phase_threshold, scaled};
#[allow(unused_imports)]
pub use state::{DexEntry, Event, EvoNode, Line, Mon, Names, State};

use crate::model::{EV_COMPANION, EV_COMPANION_EVENT, UsageSnapshot};
use crate::store::save_json;
use fastrand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// Gen-V animated sprites exist up to #649, so lines are cut there.
pub const MAX_SPECIES_ID: u32 = 649;
pub const DITTO_ID: u32 = 132;

/// `PokemonNature.allCases` order (Model:400-404); the nature roll indexes into it.
pub const NATURES: [&str; 25] = [
    "hardy", "lonely", "brave", "adamant", "naughty", "bold", "docile", "relaxed", "impish", "lax",
    "timid", "hasty", "serious", "jolly", "naive", "modest", "mild", "quiet", "bashful", "rash",
    "calm", "gentle", "sassy", "careful", "quirky",
];

/// Keys this area owns inside `settings.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompanionSettings {
    /// Species shown by the floating pet and tray instead of the active Pokémon (must be owned).
    pub representative: Option<u32>,
    pub representative_form: Option<String>,
    pub companion_notifications: bool,
}

impl Default for CompanionSettings {
    fn default() -> Self {
        CompanionSettings { representative: None, representative_form: None, companion_notifications: true }
    }
}

impl CompanionSettings {
    pub fn sanitize(&mut self) {
        match self.representative {
            None => self.representative_form = None,
            Some(id) => self.representative_form = form::resolved(id, self.representative_form.as_deref()),
        }
    }
}

// ---- persistence ----

/// Lenient decode: a field that fails to parse falls back to its default instead of the whole file.
pub fn decode_state(bytes: &[u8]) -> Result<State, String> {
    let v: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if let Ok(s) = serde_json::from_value::<State>(v.clone()) {
        return Ok(s);
    }
    let obj = v.as_object().ok_or("not an object")?;
    let good: serde_json::Map<String, Value> = obj
        .iter()
        .filter(|(k, v)| serde_json::from_value::<State>(json!({ k.as_str(): v })).is_ok())
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    serde_json::from_value(Value::Object(good)).map_err(|e| e.to_string())
}

pub struct Store {
    pub path: PathBuf,
    pub state: State,
}

impl Store {
    /// Loads, migrates (difficulty, profiles), sanitizes and repairs the evolution plan.
    pub fn load(path: PathBuf) -> Self {
        let mut state = match fs::read(&path) {
            Err(_) => State::default(),
            Ok(bytes) => decode_state(&bytes).unwrap_or_else(|_| {
                let _ = fs::rename(&path, path.with_extension("json.corrupt"));
                State::default()
            }),
        };
        let mut rng = Rng::new();
        state.migrate_difficulty();
        state.sanitize();
        state.normalize_evolution(&mut rng);
        if state.migrate_profiles() {
            // One recoverable copy before the first save with profiles. Never overwritten.
            let backup = path.with_file_name("companion.pre-profiles-v1.json");
            if path.exists() && !backup.exists() {
                let _ = fs::copy(&path, &backup);
            }
        }
        let store = Store { path, state };
        if store.path.exists() {
            store.save();
        }
        store
    }

    pub fn save(&self) {
        save_json(&self.path, &self.state);
    }

    pub fn dir(&self) -> &Path {
        self.path.parent().unwrap_or(Path::new("."))
    }
}

/// The one lock for `companion.json`. Economy mutates its block through the same lock.
pub type SharedStore = Mutex<Store>;

pub fn setup(app: &tauri::App, dir: &Path) -> tauri::Result<()> {
    let mut store = Store::load(dir.join("companion.json"));
    // The Ditto disguise rolls only in release builds, like the bundled-app gate on macOS.
    store.state.rt.ditto_rolling = !cfg!(debug_assertions);
    app.manage::<SharedStore>(Mutex::new(store));
    Ok(())
}

fn lang(app: &AppHandle) -> String {
    crate::settings::current(app).shell.language
}

/// Runs `f` on the saved state under the lock, saves, and emits `companion` (plus notifications,
/// `companion-event`s and follow-up fetches the change caused).
pub fn update<T>(app: &AppHandle, f: impl FnOnce(&mut State) -> T) -> T {
    let lang = lang(app);
    let out = {
        let store = app.state::<SharedStore>();
        let mut s = store.lock().unwrap();
        s.state.rt.lang = lang;
        let out = f(&mut s.state);
        s.save();
        out
    };
    after_change(app, true);
    out
}

/// Sends what the last change queued, emits the view, keeps the representative owned,
/// and (optionally) fetches what the new state needs off the calling thread.
fn after_change(app: &AppHandle, spawn_follow_up: bool) {
    let settings = crate::settings::current(app);
    let (outbox, events, view, owned, event_until) = {
        let store = app.state::<SharedStore>();
        let mut s = store.lock().unwrap();
        let st = &mut s.state;
        let outbox = std::mem::take(&mut st.rt.outbox);
        let events = std::mem::take(&mut st.rt.events);
        let owned = settings.companion.representative.is_none_or(|id| st.owns_species(id, settings.companion.representative_form.as_deref()));
        (outbox, events, view(st, &settings), owned, st.rt.event_until)
    };
    // Windows toasts need the installed AUMID; dev builds have none (macOS: bundled app only).
    if settings.companion.companion_notifications && !cfg!(debug_assertions) {
        for (title, body) in &outbox {
            crate::shell::notify(app, title, body);
        }
    }
    let _ = app.emit(EV_COMPANION, view);
    for e in events {
        let _ = app.emit(EV_COMPANION_EVENT, e);
    }
    if !owned {
        let mut s = settings.clone();
        s.companion.representative = None;
        s.companion.representative_form = None;
        crate::shell::apply_settings(app, s);
    }
    if let Some(until) = event_until {
        // Re-emit once the levelUp window closes so the status line moves on without a refresh.
        let app = app.clone();
        std::thread::spawn(move || {
            std::thread::sleep(until.saturating_duration_since(Instant::now()) + Duration::from_millis(100));
            let expired = {
                let store = app.state::<SharedStore>();
                let mut s = store.lock().unwrap();
                let before = s.state.rt.event_until;
                s.state.expire_events(Instant::now());
                before.is_some() && s.state.rt.event_until.is_none()
            };
            if expired {
                after_change(&app, false);
            }
        });
    }
    if spawn_follow_up {
        let app = app.clone();
        std::thread::spawn(move || {
            if follow_up(&app) {
                after_change(&app, false);
            }
        });
    }
}

static FOLLOW_UP_BUSY: AtomicBool = AtomicBool::new(false);

/// Network work the state asked for: the Ditto reveal line, the current species' details,
/// and localized form names. Returns true when it changed the state.
fn follow_up(app: &AppHandle) -> bool {
    if FOLLOW_UP_BUSY.swap(true, Ordering::SeqCst) {
        return false;
    }
    let store = app.state::<SharedStore>();
    let (dir, ditto, current, forms) = {
        let s = store.lock().unwrap();
        let st = &s.state;
        let forms: Vec<u32> = st
            .active
            .as_ref()
            .map(|a| form::form_species(a.base_id()).into_iter().filter_map(|id| form::name_resource(id, a.form.as_deref())).collect())
            .unwrap_or_default();
        let current = st.active.as_ref().map(|a| a.current_id()).filter(|id| !st.rt.details.contains_key(id));
        let forms = forms.into_iter().filter(|r| !st.rt.form_labels.contains_key(r)).collect::<Vec<_>>();
        (s.dir().to_path_buf(), st.rt.ditto_pending, current, forms)
    };
    let mut changed = false;
    for res in forms {
        if let Ok(n) = pokeapi::names(&dir, "pokemon-form", &res.to_string()) {
            store.lock().unwrap().state.rt.form_labels.insert(res, n);
            changed = true;
        }
    }
    if ditto {
        if let Ok(line) = pokeapi::line(DITTO_ID) {
            let lang = lang(app);
            let mut s = store.lock().unwrap();
            s.state.rt.lang = lang;
            s.state.reveal_ditto(line, &mut Rng::new());
            s.save();
            changed = true;
        }
    }
    let current = current.or_else(|| {
        let s = store.lock().unwrap();
        s.state.active.as_ref().map(|a| a.current_id()).filter(|id| !s.state.rt.details.contains_key(id))
    });
    if let Some(id) = current {
        if let Ok(d) = pokeapi::details(&dir, id) {
            let mut s = store.lock().unwrap();
            s.state.add_details(d);
            s.save();
            changed = true;
        }
    }
    FOLLOW_UP_BUSY.store(false, Ordering::SeqCst);
    changed
}

/// Loads the localized names of the egg's pre-rolled form into the label cache.
fn warm_form_labels(app: &AppHandle, dir: &Path, base: u32, pending: Option<&str>) {
    for res in form::form_species(base).into_iter().filter_map(|id| form::name_resource(id, pending)) {
        if let Ok(n) = pokeapi::names(dir, "pokemon-form", &res.to_string()) {
            app.state::<SharedStore>().lock().unwrap().state.rt.form_labels.insert(res, n);
        }
    }
}

/// REST fallback when the base index is unavailable: up to 16 uniform probes over 1…649.
fn choose_base_rest(tier: Option<Rarity>, rng: &mut Rng) -> Option<u32> {
    for _ in 0..16 {
        let id = (rng.u64(..) % MAX_SPECIES_ID as u64) as u32 + 1;
        match pokeapi::base_species(id) {
            Ok(Some(rate)) if tier.is_none_or(|t| t.includes(rate)) => return Some(id),
            Ok(_) => continue,
            Err(_) => return None,
        }
    }
    None
}

/// Egg prefetch and hatch (companion.md §3.6). Network calls run without holding the lock.
fn hatch_flow(app: &AppHandle, rng: &mut Rng) {
    let store = app.state::<SharedStore>();
    let (dir, pending, tier) = {
        let s = store.lock().unwrap();
        if s.state.active.is_some() || !s.state.baseline_set {
            return;
        }
        (s.dir().to_path_buf(), s.state.pending_hatch_id, s.state.egg_tier)
    };
    let mark_delayed = || {
        let mut s = store.lock().unwrap();
        s.state.rt.hatch_retry_delayed = s.state.egg_ready();
    };
    let id = match pending {
        Some(id) => id,
        None => {
            let chosen = match pokeapi::base_index(&dir) {
                Ok(index) => store.lock().unwrap().state.choose_base(&index, rng),
                Err(_) => choose_base_rest(tier, rng),
            };
            let Some(id) = chosen else { return mark_delayed() };
            let mut s = store.lock().unwrap();
            s.state.pending_hatch_id = Some(id);
            s.state.pending_form = s.state.roll_line_form(id, rng);
            s.save();
            id
        }
    };
    let Ok(line) = pokeapi::line(id) else { return mark_delayed() };
    let pending_form = store.lock().unwrap().state.pending_form.clone();
    warm_form_labels(app, &dir, id, pending_form.as_deref());
    if let Ok(d) = pokeapi::details(&dir, id) {
        store.lock().unwrap().state.rt.details.insert(id, d);
    }
    let lang = lang(app);
    let mut s = store.lock().unwrap();
    if s.state.egg_ready() && s.state.pending_hatch_id == Some(id) {
        s.state.rt.lang = lang;
        s.state.hatch(line, rng);
        s.save();
    }
}

/// Seconds since the newest provider usage; 7 days when none reports a time; `None` before data.
fn idle_secs(snap: &UsageSnapshot) -> Option<u64> {
    snap.fetched_at.as_ref()?;
    let newest = snap
        .providers
        .iter()
        .filter_map(|p| chrono::DateTime::parse_from_rfc3339(p.last_usage.as_deref()?).ok())
        .max();
    Some(match newest {
        Some(t) => (chrono::Utc::now() - t.with_timezone(&chrono::Utc)).num_seconds().max(0) as u64,
        None => 7 * 86400,
    })
}

fn limit_warning(app: &AppHandle, crit: f64) -> bool {
    crate::limits::current(app).iter().flat_map(|l| l.windows.iter()).any(|w| w.used_percent >= crit)
}

/// Called by the shell after every usage refresh: credit growth, hatch, notify, emit.
pub fn on_usage(app: &AppHandle, snap: &UsageSnapshot) {
    let settings = crate::settings::current(app);
    let date = crate::model::today_key();
    let today = snap.today_by_provider(&date);
    let has_usage_data = snap.fetched_at.is_some() && !snap.providers.is_empty();
    let has_current = has_usage_data && !today.is_empty();
    let tokens_today: u64 = today.values().sum();
    let tpm: f64 = snap.providers.iter().filter_map(|p| p.active_block.as_ref()).map(|b| b.tokens_per_minute).sum();
    let warning = limit_warning(app, settings.limits.crit_threshold);
    let mut rng = Rng::new();
    {
        let store = app.state::<SharedStore>();
        let mut s = store.lock().unwrap();
        let st = &mut s.state;
        st.rt.lang = settings.shell.language.clone();
        let delta = st.credit(&today, &date, has_usage_data);
        st.feed(delta, &mut rng);
        // Re-checks thresholds without new usage (relaunch, a Ditto reveal that failed offline).
        if delta == 0 && st.active.is_some() {
            st.apply_usage(0, &mut rng);
        }
        let now = Instant::now();
        st.expire_events(now);
        let idle = st.effective_idle(idle_secs(snap), &date, tokens_today, has_current, now);
        st.rt.inputs = state::Inputs { has_usage_data, today_tokens: tokens_today, tokens_per_minute: tpm, limit_warning: warning, idle_secs: idle };
        s.save();
    }
    hatch_flow(app, &mut rng);
    follow_up(app); // already on the scheduler thread
    after_change(app, false);
}

/// "Growth 42%" / "Hatch 42%" for the tray tooltip (integer floor, always the raised companion).
pub fn menu_growth_text(app: &AppHandle, lang: &str) -> String {
    let store = app.state::<SharedStore>();
    let (pct, egg) = store.lock().unwrap().state.growth_percent();
    text::menu_growth(lang, pct, egg)
}

/// What the UI needs: saved state plus derived display values (companion.md §5).
pub fn view(s: &State, settings: &crate::settings::Settings) -> Value {
    let now = Instant::now();
    let display = s.display_state(now);
    let a = s.active.as_ref();
    let faces_back = a.is_some() && (settings.pet.always_faces_back || display == state::DisplayState::Sulk);
    let current = a.map(|a| a.current_id());
    let rep = settings.companion.representative.filter(|id| s.owns_species(*id, settings.companion.representative_form.as_deref()));
    let subject = match (rep, a) {
        (Some(id), _) => {
            let f = settings.companion.representative_form.as_deref();
            Some(json!({ "id": id, "shiny": s.owns_shiny_species(id, f), "form": form::resolved(id, f), "back": faces_back }))
        }
        (None, Some(a)) => Some(json!({
            "id": a.current_id(), "shiny": a.displays_shiny(),
            "form": form::resolved(a.current_id(), a.form.as_deref()), "back": faces_back,
        })),
        (None, None) => None,
    };
    let threshold = s.threshold();
    json!({
        "active": s.active,
        "egg_usage": s.egg_usage,
        "egg_threshold": s.egg_threshold(),
        "threshold": threshold,
        "difficulty": s.difficulty(),
        "dex": s.dex,
        "egg_tier": s.egg_tier,
        "hatch_retry_delayed": s.rt.hatch_retry_delayed,
        "display_state": display,
        "faces_back": faces_back,
        "just_evolved_to": s.rt.just_evolved_to,
        "just_graduated": s.rt.just_graduated,
        "current_id": current,
        "current_form": a.and_then(|a| form::resolved(a.current_id(), a.form.as_deref())),
        "displays_shiny": a.is_some_and(|a| a.displays_shiny()),
        "current_types": current.and_then(|id| s.rt.details.get(&id)).map(|d| &d.types),
        "is_final_stage": a.is_some_and(|a| a.is_final_stage()),
        "growth_multiplier": a.map(|a| a.growth_multiplier()).filter(|m| *m > 1),
        "tokens_to_next": a.zip(threshold).map_or(0, |(a, t)| t.saturating_sub(a.used_at_stage)),
        "line_nodes": s.line_nodes().into_iter().map(|(id, st)| json!({ "id": id, "state": st })).collect::<Vec<_>>(),
        "form_labels": s.rt.form_labels,
        "celebration": s.rt.celebration,
        "candy": s.rt.candy.map(|(seq, xp)| json!({ "seq": seq, "xp": xp })),
        "mint_seq": s.rt.mint_seq,
        "latest_event": s.rt.latest_event,
        "representative_subject": subject,
        "shiny_denominator": s.shiny_denominator(),
    })
}

#[tauri::command]
fn companion(app: AppHandle) -> Value {
    let settings = crate::settings::current(&app);
    let store = app.state::<SharedStore>();
    let s = store.lock().unwrap();
    view(&s.state, &settings)
}

#[tauri::command]
fn set_difficulty(app: AppHandle, value: f64) -> Value {
    update(&app, |s| s.set_difficulty(value));
    companion(app)
}

/// Default-form battle data (types, base stats, abilities, BW2 learnset), cached 30 days.
#[tauri::command]
async fn pokemon_details(app: AppHandle, id: u32) -> Result<profile::Details, String> {
    if let Some(d) = app.state::<SharedStore>().lock().unwrap().state.rt.details.get(&id) {
        return Ok(d.clone());
    }
    let dir = app.state::<SharedStore>().lock().unwrap().dir().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || pokeapi::details(&dir, id)).await.map_err(|e| e.to_string())?
}

/// `{lang: name}` of a type, ability, move or pokemon-form resource, cached 30 days.
#[tauri::command]
async fn pokemon_names(app: AppHandle, kind: String, name: String) -> Result<HashMap<String, String>, String> {
    let dir = app.state::<SharedStore>().lock().unwrap().dir().to_path_buf();
    tauri::async_runtime::spawn_blocking(move || pokeapi::names(&dir, &kind, &name)).await.map_err(|e| e.to_string())?
}

/// Called by the shell on every settings change and once at launch. Re-emits the view so the
/// back view and representative subject follow the new settings.
pub fn on_settings(app: &tauri::AppHandle, s: &crate::settings::Settings) {
    let store = app.state::<SharedStore>();
    let v = {
        let mut st = store.lock().unwrap();
        st.state.rt.lang = s.shell.language.clone();
        view(&st.state, s)
    };
    let _ = app.emit(EV_COMPANION, v);
}

crate::area_commands![companion, set_difficulty, pokemon_details, pokemon_names];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DailyUsage, ProviderSnapshot};

    #[test]
    fn today_map_skips_stale_snapshots() {
        let p = |id: &str, date: &str, total| ProviderSnapshot {
            provider_id: id.into(),
            today: Some(DailyUsage { date: date.into(), usage: crate::model::Usage { total, ..Default::default() }, models: None }),
            ..Default::default()
        };
        let snap = UsageSnapshot {
            providers: vec![p("claude_code", "d1", 5), p("codex", "d0", 9), ProviderSnapshot { provider_id: "x".into(), ..Default::default() }],
            fetched_at: Some("t".into()),
            error: None,
        };
        assert_eq!(snap.today_by_provider("d1"), HashMap::from([("claude_code".to_string(), 5)]));
        assert_eq!(idle_secs(&snap), Some(7 * 86400));
        assert_eq!(idle_secs(&UsageSnapshot::default()), None);
    }

    #[test]
    fn store_load_migrates_v02_saves() {
        let dir = std::env::temp_dir().join(format!("ptb-companion-{}", fastrand::u64(..)));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("companion.json");
        let v02 = r#"{"baseline_set":true,"ledger":{"claude":5},"last_date":"2026-01-01","egg_usage":10,
            "active":{"line":{"base_id":1,"tree":{"id":1,"children":[{"id":2,"children":[]}]},"rarity":"common","names":{}},
            "path_ids":[1],"planned_ids":[1,2],"stage_index":0,"used_at_stage":3,"is_shiny":false,"nature":"bold"},
            "dex":[{"base_id":4,"final_id":5,"chain":[4,5],"rarity":"common","is_shiny":true,"nature":"calm","names":{},"caught_at":"x"}],
            "collected_finals":["4:5"],"growth_difficulty":0.0,"spent_tokens":7}"#;
        fs::write(&path, v02).unwrap();
        let s = Store::load(path.clone()).state;
        assert_eq!(s.growth_difficulty, 1.0);
        let a = s.active.as_ref().unwrap();
        assert_eq!((a.total_forms, a.used_at_stage), (2, 3));
        assert!(a.profile.is_some() && s.dex[0].profile.is_some() && !s.dex[0].id.is_empty());
        assert_eq!(s.economy.spent_tokens, 7);
        assert!(dir.join("companion.pre-profiles-v1.json").exists());
        fs::write(&path, b"not json").unwrap();
        assert!(Store::load(path.clone()).state.active.is_none());
        assert!(dir.join("companion.json.corrupt").exists());
        let _ = fs::remove_dir_all(dir);
    }
}
