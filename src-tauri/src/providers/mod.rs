//! Usage providers: local tool logs → `model::UsageSnapshot`. Owner: providers area.
//! Spec: docs/parity/providers.md. Every provider is "find files → parse → dedup → [Entry]" plus a
//! cache; all aggregation (today, week, month, daily series, 5 h block, cost) is shared (`entry.rs`).

#[allow(dead_code)] // public API for the limits area (per-account usage)
pub mod accounts;
mod antigravity;
mod cache;
mod codex;
mod cursor_api;
mod entry;
mod jsonl;
pub mod pricing;
mod roots;
mod sqlite;

use crate::model::{UsageSnapshot, EV_USAGE};
use cache::{Additional, JsonlCache, Kind, ADDITIONAL_FRESHNESS};
use entry::{Entry, Ts};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// Registration order: tab order, Settings toggle order, palette slots.
pub const PROVIDERS: [(&str, &str); 16] = [
    ("claude_code", "Claude Code"),
    ("codex", "Codex"),
    ("gemini", "Gemini"),
    ("antigravity", "Antigravity"),
    ("opencode", "OpenCode"),
    ("hermes", "Hermes Agent"),
    ("cursor", "Cursor"),
    ("grok", "Grok"),
    ("copilot", "Copilot"),
    ("kiro", "Kiro"),
    ("pi", "Pi"),
    ("omp", "omp"),
    ("omo", "omo"),
    ("aside", "Aside"),
    ("kimi_code", "Kimi Code"),
    ("devin", "Devin"),
];

/// Providers whose today carries a per-model breakdown (macOS `includeModels: true`).
const PER_MODEL: [&str; 4] = ["pi", "omp", "omo", "kimi_code"];
const EMPTY_RETRY: Duration = Duration::from_secs(20);

// ---- settings block ----

/// Keys this area owns inside `settings.json` (flattened into `settings::Settings`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderSettings {
    /// Opt-out list of provider ids, sorted (new providers are on by default).
    pub disabled_providers: Vec<String>,
    /// provider id → raw comma/newline separated folders (`*` wildcards).
    pub custom_scan_roots: BTreeMap<String, String>,
    /// Extra Claude config folders (other logins), comma/newline separated.
    pub additional_claude_config_dirs: String,
}

impl ProviderSettings {
    pub fn sanitize(&mut self) {
        self.disabled_providers.sort();
        self.disabled_providers.dedup();
        self.custom_scan_roots.retain(|_, v| !v.trim().is_empty());
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        !self.disabled_providers.iter().any(|d| d == id)
    }
}

// ---- state ----

struct Caches {
    jsonl: JsonlCache,
    additional: HashMap<&'static str, Additional>,
    antigravity: HashMap<String, antigravity::Blob>,
    cursor_api_cache: Option<PathBuf>,
    /// Last Claude scan, reused for account attribution.
    claude_entries: Vec<Entry>,
}

#[derive(Default)]
struct Control {
    previous: Option<ProviderSettings>,
    /// Bumped by every refresh; a pending empty-usage retry only fires if nothing ran since.
    generation: u64,
    retrying: bool,
}

type Shared = Mutex<UsageSnapshot>;

pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    let dir = app.path().app_data_dir()?;
    app.manage::<Shared>(Default::default());
    app.manage(Mutex::new(Caches {
        jsonl: JsonlCache::new(Some(dir.join("usage-cache.json"))),
        additional: HashMap::new(),
        antigravity: HashMap::new(),
        cursor_api_cache: Some(dir.join("cursor-usage-api-cache.json")),
        claude_entries: Vec::new(),
    }));
    app.manage(Mutex::new(Control::default()));
    Ok(())
}

/// Latest snapshot (empty before the first refresh).
pub fn current(app: &AppHandle) -> UsageSnapshot {
    app.state::<Shared>().lock().unwrap().clone()
}

// ---- reading ----

fn additional(c: &mut Caches, id: &'static str, s: &ProviderSettings, now: Ts) -> Vec<Entry> {
    let month = entry::month_key(now);
    let prev = c.additional.remove(id).filter(|p| p.month == month);
    if let Some(p) = prev.as_ref().filter(|p| now - p.loaded_at < ADDITIONAL_FRESHNESS) {
        let out = p.entries.clone();
        c.additional.insert(id, prev.unwrap());
        return out;
    }
    let period = entry::enrichment_scan_start(now);
    let roots = roots::roots_for(id, s);
    let existing = prev.as_ref().map(|p| p.entries.clone()).unwrap_or_default();
    let marks = prev.as_ref().map(|p| p.high_water.clone()).unwrap_or_default();
    let merge = |loaded: Vec<Entry>| entry::dedup_keep_max([existing.clone(), loaded].concat());
    let mut next = Additional { loaded_at: now, month, entries: Vec::new(), high_water: HashMap::new(), kiro_signatures: Default::default() };
    match id {
        "opencode" => {
            let newest = existing.iter().map(|e| e.date).reduce(f64::max);
            let since = newest.map_or(period, |n| period.max(n - 1.0));
            next.entries = merge(sqlite::opencode_entries(&roots, since));
        }
        "hermes" => next.entries = sqlite::hermes_entries(&roots, period),
        "kiro" => {
            let known = prev.as_ref().map(|p| p.kiro_signatures.clone()).unwrap_or_default();
            let (loaded, sigs) = sqlite::kiro(&roots, period, &known);
            next.entries = merge(loaded);
            next.kiro_signatures = sigs;
        }
        "aside" => next.entries = merge(sqlite::aside(&roots, period)),
        "cursor" => {
            let api = cursor_api::fetch_entries(&roots, period, c.cursor_api_cache.as_deref().filter(|_| !cfg!(test)));
            if api.authoritative {
                // A dashboard answer replaces everything, even when empty.
                next.entries = entry::dedup_keep_max(api.entries);
            } else {
                let local = sqlite::cursor_local(&roots, period, &marks);
                next.high_water = local.high_water;
                next.entries = if local.did_reset {
                    local.entries
                } else {
                    let kept: Vec<Entry> = existing.iter().filter(|e| !e.id.starts_with("cursor|api|")).cloned().collect();
                    entry::dedup_keep_max([kept, local.entries].concat())
                };
            }
        }
        "copilot" | "devin" => {
            let load = if id == "copilot" { sqlite::copilot(&roots, period, &marks) } else { sqlite::devin(&roots, period, &marks) };
            next.high_water = load.high_water;
            next.entries = if load.did_reset { load.entries } else { merge(load.entries) };
        }
        _ => {}
    }
    let out = next.entries.clone();
    c.additional.insert(id, next);
    out
}

fn read(c: &mut Caches, id: &'static str, s: &ProviderSettings, now: Ts) -> Vec<Entry> {
    let since = entry::enrichment_scan_start(now);
    let generic = |c: &mut Caches, kind| c.jsonl.generic(kind, &roots::roots_for(id, s), since);
    match id {
        "claude_code" => {
            let es = c.jsonl.claude(&roots::roots_for(id, s), since);
            c.claude_entries = es.clone();
            es
        }
        "codex" => c.jsonl.codex(&roots::roots_for(id, s), since),
        "gemini" => generic(c, Kind::Gemini),
        "grok" => generic(c, Kind::Grok),
        "pi" => generic(c, Kind::Pi),
        "omp" => generic(c, Kind::Omp),
        "omo" => generic(c, Kind::Omo),
        "kimi_code" => generic(c, Kind::Kimi),
        "antigravity" => {
            let (blobs, log) = antigravity::scan(&roots::roots_for(id, s), since, &c.antigravity);
            log.iter().for_each(|l| eprintln!("{l}"));
            c.antigravity = blobs;
            antigravity::assemble(&c.antigravity, entry::enrichment_scan_start(entry::now_ts()))
        }
        _ => additional(c, id, s, now),
    }
}

fn read_snapshot(c: &mut Caches, s: &ProviderSettings) -> UsageSnapshot {
    let now = entry::now_ts();
    let mut providers = Vec::new();
    for (id, name) in PROVIDERS.iter().filter(|(id, _)| s.is_enabled(id)) {
        let entries = read(c, id, s, now);
        let snap = entry::snapshot(id, name, &entries, now, PER_MODEL.contains(id));
        if entry::displayable(&snap, now) {
            providers.push(snap);
        }
    }
    c.jsonl.save_if_needed(now);
    UsageSnapshot { providers, fetched_at: Some(chrono::Local::now().to_rfc3339()), error: None }
}

/// Called by the shell scheduler. Reads, stores and returns the new snapshot; the shell emits it.
pub fn refresh(app: &AppHandle) -> UsageSnapshot {
    let settings = crate::settings::current(app).providers;
    let snap = {
        let caches = app.state::<Mutex<Caches>>();
        let mut c = caches.lock().unwrap();
        read_snapshot(&mut c, &settings)
    };
    *app.state::<Shared>().lock().unwrap() = snap.clone();
    schedule_empty_retry(app, &snap, &settings);
    snap
}

/// Every enabled provider answered but nothing is displayable: try once more after 20 s.
fn schedule_empty_retry(app: &AppHandle, snap: &UsageSnapshot, s: &ProviderSettings) {
    let ctl = app.state::<Mutex<Control>>();
    let mut ctl = ctl.lock().unwrap();
    ctl.generation += 1;
    let was_retry = std::mem::take(&mut ctl.retrying);
    let any_enabled = PROVIDERS.iter().any(|(id, _)| s.is_enabled(id));
    if was_retry || !any_enabled || snap.error.is_some() || !snap.providers.is_empty() {
        return;
    }
    let generation = ctl.generation;
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(EMPTY_RETRY);
        let ctl = app.state::<Mutex<Control>>();
        let mut c = ctl.lock().unwrap();
        if c.generation != generation {
            return;
        }
        c.retrying = true;
        drop(c);
        crate::shell::refresh_now(app.clone());
    });
}

/// Called by the shell on every settings change and once at launch.
pub fn on_settings(app: &AppHandle, s: &crate::settings::Settings) {
    let next = s.providers.clone();
    let previous = {
        let ctl = app.state::<Mutex<Control>>();
        let mut ctl = ctl.lock().unwrap();
        std::mem::replace(&mut ctl.previous, Some(next.clone()))
    };
    let Some(previous) = previous else { return };
    if previous == next {
        return;
    }
    let roots_changed = previous.custom_scan_roots != next.custom_scan_roots
        || previous.additional_claude_config_dirs != next.additional_claude_config_dirs;
    if roots_changed {
        roots::invalidate_claude_roots();
        app.state::<Mutex<Caches>>().lock().unwrap().additional.clear();
    }
    if previous.disabled_providers != next.disabled_providers {
        // Drop a switched-off tab now instead of waiting for the read.
        let shared = app.state::<Shared>();
        let mut snap = shared.lock().unwrap();
        snap.providers.retain(|p| next.is_enabled(&p.provider_id));
        let _ = app.emit(EV_USAGE, &*snap);
    }
    crate::shell::refresh_now(app.clone());
}

// ---- API for other areas (read-only) ----

/// Additional Claude config folders the app follows (detected logins, then Settings extras).
#[allow(dead_code)] // consumed by limits (additional Claude accounts)
pub fn claude_account_roots(app: &AppHandle) -> Vec<PathBuf> {
    roots::installed_account_roots(&crate::settings::current(app).providers.additional_claude_config_dirs)
}

/// Splits the last Claude scan between logins. `folders` = (account id, config dir), in tab order.
#[allow(dead_code)] // consumed by limits (per-account usage and 5 h blocks)
pub fn claude_attribution(
    app: &AppHandle,
    folders: &[(String, PathBuf)],
    windows: &HashMap<String, accounts::FiveHourWindow>,
) -> accounts::Attribution {
    let entries = app.state::<Mutex<Caches>>().lock().unwrap().claude_entries.clone();
    let mut ids: Vec<String> = Vec::new();
    let mut prompts: HashMap<String, accounts::Prompts> = HashMap::new();
    for (id, dir) in folders {
        if !prompts.contains_key(id) {
            ids.push(id.clone());
        }
        let p = prompts.entry(id.clone()).or_default();
        for (session, mut times) in accounts::prompts(dir) {
            let v = p.entry(session).or_default();
            v.append(&mut times);
            v.sort_by(f64::total_cmp);
        }
    }
    let list: Vec<accounts::Account> = ids.into_iter().map(|id| accounts::Account { prompts: prompts.remove(&id).unwrap_or_default(), id }).collect();
    accounts::attribute(&entries, &list, entry::now_ts(), windows)
}

/// Saved login of a Claude config folder (`.claude.json` → `oauthAccount`).
#[allow(dead_code)] // consumed by limits (account labels)
pub fn claude_saved_login(dir: &std::path::Path) -> Option<roots::SavedLogin> {
    roots::saved_login(dir)
}

// ---- commands ----

#[tauri::command]
fn usage_snapshot(app: AppHandle) -> UsageSnapshot {
    current(&app)
}

/// "Scans N extra folder(s) now" for a draft in Settings (walks the disk, so off the UI thread).
#[tauri::command]
async fn custom_scan_match_count(app: AppHandle, provider: String, raw: String) -> usize {
    let s = crate::settings::current(&app).providers;
    tauri::async_runtime::spawn_blocking(move || roots::surviving_extra_count(&roots::curated(&provider, &s), &raw))
        .await
        .unwrap_or(0)
}

#[derive(Serialize)]
struct ClaudeAccountFolders {
    /// Detected logins, `~`-abbreviated.
    detected: Vec<String>,
    /// Folders the draft resolves to.
    found: usize,
}

#[tauri::command]
async fn claude_account_folders(raw: String) -> ClaudeAccountFolders {
    tauri::async_runtime::spawn_blocking(move || {
        let home = roots::home();
        let abbreviate = |p: PathBuf| match p.strip_prefix(&home) {
            Ok(rest) => format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display()),
            Err(_) => p.display().to_string(),
        };
        let detected = if cfg!(test) { Vec::new() } else { roots::discovered_accounts(&home, roots::env("CLAUDE_CONFIG_DIR").as_deref()) };
        ClaudeAccountFolders { detected: detected.into_iter().map(abbreviate).collect(), found: roots::account_roots_from(&raw, &home).len() }
    })
    .await
    .unwrap_or(ClaudeAccountFolders { detected: Vec::new(), found: 0 })
}

crate::area_commands![usage_snapshot, custom_scan_match_count, claude_account_folders];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_order_and_sanitize() {
        assert_eq!(PROVIDERS.len(), 16);
        assert_eq!(PROVIDERS[0].0, "claude_code");
        assert_eq!(PROVIDERS[15].0, "devin");
        let mut s = ProviderSettings {
            disabled_providers: vec!["grok".into(), "codex".into(), "grok".into()],
            custom_scan_roots: [("gemini".to_string(), "  ".to_string()), ("pi".to_string(), "~/x".to_string())].into(),
            ..Default::default()
        };
        s.sanitize();
        assert_eq!(s.disabled_providers, vec!["codex", "grok"]);
        assert_eq!(s.custom_scan_roots.len(), 1);
        assert!(!s.is_enabled("codex") && s.is_enabled("claude_code"));
    }

    #[test]
    fn disabled_providers_are_not_read() {
        let mut c = Caches {
            jsonl: JsonlCache::new(None),
            additional: HashMap::new(),
            antigravity: HashMap::new(),
            cursor_api_cache: None,
            claude_entries: Vec::new(),
        };
        let s = ProviderSettings { disabled_providers: PROVIDERS.iter().map(|(id, _)| id.to_string()).collect(), ..Default::default() };
        let snap = read_snapshot(&mut c, &s);
        assert!(snap.providers.is_empty() && c.claude_entries.is_empty() && c.additional.is_empty());
    }
}

/// `cargo test real_logs -- --ignored --nocapture` prints this machine's usage.
#[cfg(test)]
#[test]
#[ignore]
fn real_logs() {
    let mut c = Caches {
        jsonl: JsonlCache::new(None),
        additional: HashMap::new(),
        antigravity: HashMap::new(),
        cursor_api_cache: None,
        claude_entries: Vec::new(),
    };
    for p in read_snapshot(&mut c, &ProviderSettings::default()).providers {
        println!("{} today={:?} month={:?}", p.provider_id, p.today.map(|t| t.usage.total), p.month_total.map(|m| m.usage.total));
    }
}
