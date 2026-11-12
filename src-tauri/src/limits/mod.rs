//! Official usage limits (limits.md). Owner: limits area.
//! Sources: Claude (OAuth file + claude.ai session key, every account folder), Codex (app-server,
//! rollout-log fallback), Antigravity, Cursor, OpenCode Go, Grok, plus the statuspage banner.
//! Each source keeps its last good value on failure; alerts are edge-triggered per window.

mod antigravity;
mod claude;
mod codex;
mod cursor;
mod grok;
mod http;
mod opencode;
mod session_key;
mod status;
mod strings;

use chrono::{DateTime, Utc};
use http::LimitError;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};

pub use status::ProviderStatus;

const BACKOFF_FIRST: f64 = 300.0;
const BACKOFF_MAX: f64 = 3600.0;
const BUBBLE_TTL_SECS: u64 = 6;
pub const EV_EXTRAS: &str = "limits-extras";
pub const EV_ALERT: &str = "limits-alert";

// ---- contract (mirrored in shared.ts `#region limits`) ----

/// One limit row. `provider`, `key`, `used_percent` and `resets_at` are read by economy (candy).
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Window {
    /// Unique inside its provider entry. Claude: `5h`, `7d`, `7d-opus`, `7d-sonnet`, `scoped.<kind>.<model>.<i>`;
    /// Codex: `<bucket>.primary|secondary|individual`; Antigravity: `<gemini|3p>.<bucketId>`.
    pub key: String,
    /// Row name for the UI: fiveHour, weekly, weeklyOpus, weeklySonnet, scoped(arg=model), raw(arg=name),
    /// codex(arg=minutes), personal, limit, cursorIncluded, cursorAuto, cursorApi, monthly, credits(arg=period).
    pub label: String,
    pub arg: Option<String>,
    /// Codex bucket name or Antigravity group name.
    pub group: Option<String>,
    pub used_percent: f64,
    /// RFC 3339 (UTC).
    pub resets_at: Option<String>,
    /// Window length for the pace marker; `None` = no marker.
    pub span_secs: Option<f64>,
    /// `x / y` detail (Codex personal spend limit).
    pub detail: Option<String>,
    /// 0% with no reset: "Starts with your next message".
    pub not_started: bool,
    /// Synthetic 0% from an outdated log snapshot (Codex fallback). Candy must skip it (#434).
    pub expired: bool,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct ClaudeAccount {
    /// `default` or the folder's 8-hex path key.
    pub id: String,
    /// Prefix of alert and candy keys: `claude` or `claude.<id>`.
    pub key_prefix: String,
    pub folder: String,
    pub title: String,
    pub is_default: bool,
    /// The account the menu bar, warning state, mood and forecast follow.
    pub tracked: bool,
    pub session_key: bool,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct ProviderLimits {
    /// claude_code | codex | antigravity | cursor | opencode | grok
    pub provider: String,
    /// Claude only.
    pub account: Option<ClaudeAccount>,
    pub windows: Vec<Window>,
    /// Last failure code (`http::LimitError::code`); values are kept.
    pub error: Option<String>,
    /// RFC 3339 of the last successful read.
    pub updated_at: Option<String>,
    /// `oauth` | `sessionKey` | `token`: the banner and 50 % opacity.
    pub auth_expired: Option<String>,
    pub plan: Option<String>,
    pub account_label: Option<String>,
    /// Codex: some bucket reports `rateLimitReachedType`.
    pub limit_reached: bool,
    /// Cursor `displayMessage`.
    pub message: Option<String>,
    pub remaining_usd: Option<f64>,
    /// Grok products with usage.
    pub products: Vec<grok::Product>,
}

impl ProviderLimits {
    fn new(provider: &str) -> Self {
        ProviderLimits { provider: provider.into(), ..Default::default() }
    }
}

/// Everything besides the rows: statuses, session keys, Claude refresh state.
#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Extras {
    pub statuses: Vec<ProviderStatus>,
    pub session_keys: Vec<SessionKeyView>,
    /// An additional Claude folder could not load yet (refresh row).
    pub claude_pending: bool,
    pub claude_accounts: usize,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct SessionKeyView {
    /// `default` or the folder path key.
    pub account: String,
    pub folder: String,
    pub title: String,
    pub configured: bool,
    pub expired: bool,
    pub organizations: Vec<session_key::Org>,
    pub organization_id: Option<String>,
}

/// One threshold crossing (`UsageStore.LimitAlert`); also the floating-pet bubble payload.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LimitAlert {
    pub key: String,
    pub title: String,
    pub body: String,
    pub critical: bool,
    pub utilization: f64,
}

/// A window eligible for Rare Candy (`UsageStore.candyEligibleWindows`), for economy.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[allow(dead_code)] // read by companion / pet / economy (ARCHITECTURE.md cross-area calls)
pub struct CandyWindow {
    pub key: String,
    pub name: String,
    /// `session` | `weekly`
    pub kind: &'static str,
    pub utilization: f64,
    /// Reset date; `None` for Codex, which re-arms only after a sub-100% sample (#434).
    pub epoch: Option<String>,
}

// ---- settings ----

/// Keys this area owns inside `settings.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LimitSettings {
    /// "used" | "remaining"
    pub limit_display: String,
    pub limit_notifications: bool,
    pub warn_threshold: f64,
    pub crit_threshold: f64,
    pub status_checks_enabled: bool,
    /// "automatic" | "default" | "highest" | "account:<id>"
    pub claude_tracked_account: String,
}

impl Default for LimitSettings {
    fn default() -> Self {
        LimitSettings {
            limit_display: "used".into(),
            limit_notifications: true,
            warn_threshold: 80.0,
            crit_threshold: 95.0,
            status_checks_enabled: true,
            claude_tracked_account: "automatic".into(),
        }
    }
}

/// Minimum distance between the warning and critical thresholds.
const THRESHOLD_GAP: f64 = 5.0;

impl LimitSettings {
    pub fn sanitize(&mut self) {
        if self.limit_display != "remaining" {
            self.limit_display = "used".into();
        }
        let finite = |v: f64, d: f64| if v.is_finite() { v } else { d };
        self.crit_threshold = finite(self.crit_threshold, 95.0).clamp(80.0, 100.0);
        self.warn_threshold = finite(self.warn_threshold, 80.0).clamp(50.0, 95.0).min(self.crit_threshold - THRESHOLD_GAP);
        let t = self.claude_tracked_account.as_str();
        if !(matches!(t, "automatic" | "default" | "highest") || t.starts_with("account:")) {
            self.claude_tracked_account = "automatic".into();
        }
    }

    /// Alert tier for a utilization: 0 none, 1 warning, 2 critical.
    pub fn alert_tier(&self, used_percent: f64) -> u8 {
        if used_percent >= self.crit_threshold {
            2
        } else if used_percent >= self.warn_threshold {
            1
        } else {
            0
        }
    }

    pub fn display(&self, u: f64) -> f64 {
        if self.limit_display == "remaining" { (100.0 - u).max(0.0) } else { u }
    }
}

// ---- strings ----

fn lang_index(lang: &str) -> usize {
    crate::settings::LANGUAGES.iter().position(|l| *l == lang).unwrap_or(1)
}

fn tr(table: &[&'static str; 8], lang: &str) -> &'static str {
    table[lang_index(lang)]
}

/// Fills `{}` placeholders in order.
fn fill(template: &str, args: &[&str]) -> String {
    let mut out = String::new();
    let mut parts = template.split("{}");
    out.push_str(parts.next().unwrap_or(""));
    for (i, p) in parts.enumerate() {
        out.push_str(args.get(i).copied().unwrap_or(""));
        out.push_str(p);
    }
    out
}

fn codex_window(minutes: Option<&str>, lang: &str) -> String {
    match minutes.and_then(|m| m.parse::<u64>().ok()) {
        Some(300) => tr(&strings::FIVE_HOUR_SESSION, lang).into(),
        Some(10080) => tr(&strings::WEEKLY, lang).into(),
        Some(m) if m >= 60 && m % 60 == 0 => fill(tr(&strings::HOURS, lang), &[&(m / 60).to_string()]),
        Some(m) => fill(tr(&strings::MINUTES, lang), &[&m.to_string()]),
        None => tr(&strings::LIMIT, lang).into(),
    }
}

fn agy_group_title(raw: &str, lang: &str) -> String {
    let l = raw.to_lowercase();
    if l.contains("gemini") {
        tr(&strings::ANTIGRAVITY_GEMINI_GROUP, lang).into()
    } else if l.contains("claude") || l.contains("gpt") || l.contains("3p") {
        tr(&strings::ANTIGRAVITY_THIRD_PARTY_GROUP, lang).into()
    } else {
        raw.into()
    }
}

/// Generic row name (popover labels), used for alert names.
fn row_name(w: &Window, lang: &str) -> String {
    match w.label.as_str() {
        "fiveHour" => tr(&strings::FIVE_HOUR_SESSION, lang).into(),
        "weekly" => tr(&strings::WEEKLY, lang).into(),
        "weeklyOpus" => tr(&strings::WEEKLY_OPUS, lang).into(),
        "weeklySonnet" => tr(&strings::WEEKLY_SONNET, lang).into(),
        "scoped" => match &w.arg {
            Some(m) => fill(tr(&strings::WEEKLY_MODEL, lang), &[m]),
            None => tr(&strings::WEEKLY_SCOPED, lang).into(),
        },
        "codex" => codex_window(w.arg.as_deref(), lang),
        "personal" => tr(&strings::CODEX_PERSONAL_LIMIT, lang).into(),
        "raw" => w.arg.clone().unwrap_or_default(),
        _ => tr(&strings::LIMIT, lang).into(),
    }
}

// ---- internal state ----

#[derive(Debug, Clone, Copy, Default)]
struct Backoff {
    interval: f64,
    until: Option<DateTime<Utc>>,
}

impl Backoff {
    /// 300 s the first time, then doubled up to an hour; Retry-After wins when present.
    fn hit(&mut self, retry_after: Option<f64>, now: DateTime<Utc>) {
        self.interval = if self.interval <= 0.0 { BACKOFF_FIRST } else { (self.interval * 2.0).min(BACKOFF_MAX) };
        let secs = retry_after.unwrap_or(self.interval);
        self.until = Some(now + chrono::Duration::milliseconds((secs * 1000.0) as i64));
    }

    fn active(&self, now: DateTime<Utc>) -> bool {
        self.until.is_some_and(|u| now < u)
    }
}

struct ClaudeSrc {
    root: PathBuf,
    id: String,
    status: Option<claude::Status>,
    updated_at: Option<DateTime<Utc>>,
    /// "oauth" | "sessionKey"
    expiry: Option<&'static str>,
    error: Option<LimitError>,
    backoff: Backoff,
}

#[derive(Default)]
struct Src {
    value: Option<ProviderLimits>,
    updated_at: Option<DateTime<Utc>>,
    auth_expired: bool,
    error: Option<LimitError>,
}

/// Persisted bits (`limits-state.json`).
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Persisted {
    /// Candy keys of extra Claude accounts once seen below 100% (`claudeAccountCandyArmed`).
    candy_armed: BTreeSet<String>,
}

struct Work {
    dir: PathBuf,
    claude: Vec<ClaudeSrc>,
    sources: HashMap<&'static str, Src>,
    statuses: Vec<ProviderStatus>,
    oauth: claude::OAuthCache,
    agy: Option<antigravity::Credential>,
    cursor: cursor::Cache,
    /// window key → tier already notified (in memory: the first poll after launch re-alerts).
    alerts: HashMap<String, u8>,
    orgs: HashMap<String, Vec<session_key::Org>>,
    persisted: Persisted,
    claude_pending: bool,
}

#[derive(Default)]
struct Shared {
    limits: Vec<ProviderLimits>,
    extras: Extras,
    bubble: Option<(LimitAlert, Instant)>,
}

type WorkState = Mutex<Work>;
type SharedState = Mutex<Shared>;

pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    let dir = app.path().app_data_dir()?;
    let persisted = crate::store::load_json(&dir.join("limits-state.json"));
    app.manage::<WorkState>(Mutex::new(Work {
        dir,
        claude: vec![],
        sources: HashMap::new(),
        statuses: vec![],
        oauth: Default::default(),
        agy: None,
        cursor: Default::default(),
        alerts: HashMap::new(),
        orgs: HashMap::new(),
        persisted,
        claude_pending: false,
    }));
    app.manage::<SharedState>(Default::default());
    Ok(())
}

pub fn current(app: &AppHandle) -> Vec<ProviderLimits> {
    app.state::<SharedState>().lock().unwrap().limits.clone()
}

fn enabled(s: &crate::settings::Settings, id: &str) -> bool {
    !s.providers.disabled_providers.iter().any(|d| d == id)
}

// ---- Claude ----

/// `ChainedLimitsProvider`: session key first, OAuth as the fallback. When the key is set but
/// fails, the fallback never prompts and its failure reports the key's error ("re-paste key"),
/// except a Cloudflare challenge, which says nothing about the key.
fn fetch_claude(w: &mut Work, root: &std::path::Path, account: Option<&str>, manual: bool) -> Result<claude::Status, LimitError> {
    let is_default = account.is_none();
    let Some(stored) = session_key::load(&w.dir, account) else {
        return claude::fetch_oauth(root, is_default, manual, &mut w.oauth);
    };
    match session_key::fetch(&stored) {
        Ok((status, changed)) => {
            if let Some(org) = changed {
                session_key::save(&w.dir, account, &session_key::Stored { organization_id: Some(org), ..stored });
            }
            Ok(status)
        }
        Err(e) => match claude::fetch_oauth(root, is_default, false, &mut w.oauth) {
            Ok(s) => Ok(s),
            Err(fallback) if e == LimitError::Cloudflare => Err(fallback),
            Err(_) => Err(e),
        },
    }
}

fn refresh_claude(w: &mut Work, s: &crate::settings::Settings, manual: bool, now: DateTime<Utc>) {
    let mut roots = vec![claude::default_root()];
    roots.extend(claude::account_roots(&s.providers.additional_claude_config_dirs));
    let mut old: Vec<ClaudeSrc> = std::mem::take(&mut w.claude);
    let mut pending = false;
    for (i, root) in roots.into_iter().enumerate() {
        let id = if i == 0 { "default".to_string() } else { claude::path_key(&root) };
        let mut src = match old.iter().position(|o| o.root == root) {
            Some(p) => old.swap_remove(p),
            None => ClaudeSrc { root: root.clone(), id: id.clone(), status: None, updated_at: None, expiry: None, error: None, backoff: Backoff::default() },
        };
        // Automatic polls skip a folder in 429 backoff; a manual refresh ignores it.
        if !manual && src.backoff.active(now) {
            w.claude.push(src);
            continue;
        }
        let account = (i > 0).then_some(id.as_str());
        match fetch_claude(w, &root, account, manual) {
            Ok(mut status) => {
                if status.email.is_none() {
                    if let Some((email, org)) = claude::saved_identity(&root) {
                        status.email = Some(email);
                        status.org = org;
                    }
                }
                src.status = Some(status);
                src.updated_at = Some(now);
                src.expiry = None;
                src.error = None;
                src.backoff = Backoff::default();
            }
            Err(e) => {
                match &e {
                    LimitError::SessionKeyInvalid => src.expiry = Some("sessionKey"),
                    e if e.is_auth_rejection() || *e == LimitError::TokenExpired => src.expiry = Some("oauth"),
                    LimitError::RateLimited(after) => src.backoff.hit(*after, now),
                    _ => {}
                }
                if i > 0 && src.status.is_none() && src.expiry.is_none() {
                    pending = true;
                }
                src.error = Some(e);
            }
        }
        w.claude.push(src);
    }
    w.claude_pending = pending;
}

/// Accounts to show: the default first (also as a placeholder next to other accounts when its
/// login exists), extra folders when loaded or expired, one tab per email + organization.
fn claude_entries(w: &Work, s: &crate::settings::Settings) -> Vec<ProviderLimits> {
    let mut out: Vec<ProviderLimits> = vec![];
    let mut seen: Vec<(String, String)> = vec![];
    for (i, src) in w.claude.iter().enumerate() {
        let is_default = i == 0;
        let mut status = src.status.clone();
        if status.is_none() && !is_default && src.expiry.is_none() {
            continue;
        }
        if status.is_none() {
            if let Some((email, org)) = claude::saved_identity(&src.root) {
                status = Some(claude::Status { email: Some(email), org, ..Default::default() });
            }
        }
        let st = status.unwrap_or_default();
        if let Some(email) = &st.email {
            let id = (email.clone(), st.org.clone().unwrap_or_default());
            if seen.contains(&id) {
                continue;
            }
            seen.push(id);
        }
        let folder = claude::abbreviate(&src.root);
        out.push(ProviderLimits {
            account: Some(ClaudeAccount {
                id: src.id.clone(),
                key_prefix: if is_default { "claude".into() } else { format!("claude.{}", src.id) },
                title: st.title(&folder),
                folder,
                is_default,
                tracked: false,
                session_key: session_key::load(&w.dir, (!is_default).then_some(src.id.as_str())).is_some(),
            }),
            windows: if src.status.is_some() { st.windows() } else { vec![] },
            error: src.error.as_ref().map(LimitError::code),
            updated_at: src.updated_at.map(|d| d.to_rfc3339()),
            auth_expired: src.expiry.map(str::to_string),
            plan: st.plan_display(),
            account_label: st.account_display(),
            ..ProviderLimits::new("claude_code")
        });
    }
    // A placeholder default tab only next to other accounts, and only when its login exists.
    if out.len() > 1 && out[0].windows.is_empty() && out[0].auth_expired.is_none() && claude::saved_identity(&claude::default_root()).is_none() {
        out.remove(0);
    }
    let tracked = tracked_index(&out, &w.claude, &s.limits.claude_tracked_account);
    if let Some(t) = tracked {
        if let Some(a) = out[t].account.as_mut() {
            a.tracked = true;
        }
    }
    out
}

fn utilizations(p: &ProviderLimits) -> impl Iterator<Item = f64> + '_ {
    p.windows.iter().map(|w| w.used_percent)
}

/// `UsageStore.trackedAccount`: only accounts with values can be followed; a pinned account that
/// disappeared falls back to automatic (latest `history.jsonl`, ties keep the list order).
fn tracked_index(accounts: &[ProviderLimits], srcs: &[ClaudeSrc], mode: &str) -> Option<usize> {
    let loaded: Vec<usize> = (0..accounts.len()).filter(|i| !accounts[*i].windows.is_empty()).collect();
    let id = |i: usize| accounts[i].account.as_ref().map(|a| a.id.clone()).unwrap_or_default();
    let last_used = || {
        let mut best: Option<(usize, Option<std::time::SystemTime>)> = None;
        for &i in &loaded {
            let when = srcs.iter().find(|s| s.id == id(i)).and_then(|s| claude::last_prompt(&s.root));
            if best.is_none_or(|(_, b)| when > b) {
                best = Some((i, when));
            }
        }
        best.map(|(i, _)| i)
    };
    match mode {
        "default" => loaded.iter().copied().find(|i| accounts[*i].account.as_ref().is_some_and(|a| a.is_default)).or_else(last_used),
        "highest" => {
            let score = |i: usize| utilizations(&accounts[i]).fold(-1.0, f64::max);
            loaded.iter().copied().fold(None, |b: Option<usize>, i| if b.is_none_or(|b| score(i) > score(b)) { Some(i) } else { b })
        }
        m if m.starts_with("account:") => loaded.iter().copied().find(|i| id(*i) == m["account:".len()..]).or_else(last_used),
        _ => last_used(),
    }
}

// ---- other providers ----

/// Success stores and clears the expired flag; `None` (not installed / not signed in) clears the
/// value; a failure keeps the value and flags 401/403 as expired.
fn apply(w: &mut Work, id: &'static str, on: bool, now: DateTime<Utc>, fetch: impl FnOnce(&mut Work) -> Result<Option<ProviderLimits>, LimitError>) {
    if !on {
        w.sources.remove(id);
        return;
    }
    let r = fetch(w);
    let src = w.sources.entry(id).or_default();
    match r {
        Ok(Some(v)) => {
            src.value = Some(v);
            src.updated_at = Some(now);
            src.auth_expired = false;
            src.error = None;
        }
        Ok(None) => *src = Src::default(),
        Err(e) => {
            if e.is_auth_rejection() {
                src.auth_expired = true;
            }
            src.error = Some(e);
        }
    }
}

fn refresh_sources(w: &mut Work, s: &crate::settings::Settings, manual: bool, now: DateTime<Utc>) {
    let custom_cursor = s.providers.custom_scan_roots.get("cursor").cloned();
    apply(w, "codex", enabled(s, "codex"), now, |w| {
        let prev_none = w.sources.get("codex").is_none_or(|s| s.value.is_none());
        let r = match codex::fetch() {
            // With no earlier value, the log snapshot beats an empty tab.
            Err(e) if prev_none => codex::from_logs(&http::home().join(".codex").join("sessions"), now).map(Some).ok_or(e),
            r => r,
        };
        r.map(|o| {
            o.filter(|c| !c.windows.is_empty()).map(|c| ProviderLimits {
                windows: c.windows,
                plan: c.plan,
                limit_reached: c.limit_reached,
                ..ProviderLimits::new("codex")
            })
        })
    });
    apply(w, "antigravity", enabled(s, "antigravity"), now, |w| {
        let dir = w.dir.clone();
        antigravity::fetch(&dir, manual, &mut w.agy)
            .map(|o| o.map(|windows| ProviderLimits { windows, ..ProviderLimits::new("antigravity") }))
    });
    apply(w, "cursor", enabled(s, "cursor"), now, |w| {
        cursor::fetch(custom_cursor.as_deref(), &mut w.cursor).map(|o| {
            o.map(|c| ProviderLimits {
                windows: c.windows,
                message: c.message,
                remaining_usd: c.remaining_usd,
                ..ProviderLimits::new("cursor")
            })
        })
    });
    apply(w, "opencode", enabled(s, "opencode"), now, |_| {
        opencode::fetch().map(|o| o.map(|windows| ProviderLimits { windows, ..ProviderLimits::new("opencode") }))
    });
    apply(w, "grok", enabled(s, "grok"), now, |_| {
        grok::fetch().map(|o| o.map(|g| ProviderLimits { windows: g.windows, products: g.products, ..ProviderLimits::new("grok") }))
    });
}

/// The rows the UI and the other areas see, in provider order.
fn build(w: &Work, s: &crate::settings::Settings) -> Vec<ProviderLimits> {
    let mut out = if enabled(s, "claude_code") { claude_entries(w, s) } else { vec![] };
    if out.is_empty() && enabled(s, "claude_code") {
        out.push(ProviderLimits::new("claude_code")); // the tab still offers "Load official limits"
    }
    for id in ["codex", "antigravity", "cursor", "opencode", "grok"] {
        if !enabled(s, id) {
            continue;
        }
        let src = w.sources.get(id);
        let mut p = src.and_then(|s| s.value.clone()).unwrap_or_else(|| ProviderLimits::new(id));
        if let Some(src) = src {
            p.updated_at = src.updated_at.map(|d| d.to_rfc3339());
            p.auth_expired = src.auth_expired.then(|| "token".into());
            p.error = src.error.as_ref().map(LimitError::code);
        }
        // Antigravity always has a section (refresh row); the others only with values or a banner.
        if id == "antigravity" || !p.windows.is_empty() || p.auth_expired.is_some() {
            out.push(p);
        }
    }
    out
}

fn extras(w: &Work, limits: &[ProviderLimits]) -> Extras {
    let accounts: Vec<&ProviderLimits> = limits.iter().filter(|p| p.account.is_some()).collect();
    let mut keys = vec![];
    for (i, src) in w.claude.iter().enumerate() {
        let account = (i > 0).then_some(src.id.as_str());
        let shown = accounts.iter().find(|p| p.account.as_ref().is_some_and(|a| a.id == src.id));
        let stored = session_key::load(&w.dir, account);
        let folder = claude::abbreviate(&src.root);
        keys.push(SessionKeyView {
            account: src.id.clone(),
            title: shown.and_then(|p| p.account.as_ref()).map_or(folder.clone(), |a| a.title.clone()),
            folder,
            configured: stored.is_some(),
            expired: src.expiry == Some("sessionKey"),
            organizations: w.orgs.get(&src.id).cloned().unwrap_or_default(),
            organization_id: stored.and_then(|s| s.organization_id),
        });
    }
    Extras { statuses: w.statuses.clone(), session_keys: keys, claude_pending: w.claude_pending, claude_accounts: accounts.len() }
}

// ---- alerts ----

fn alert_windows(limits: &[ProviderLimits], lang: &str) -> Vec<(String, String, f64, Option<String>)> {
    let accounts = limits.iter().filter(|p| p.account.is_some()).count();
    let mut out = vec![];
    for p in limits {
        for w in &p.windows {
            let (key, name, account) = match p.provider.as_str() {
                "claude_code" => {
                    let a = p.account.as_ref().unwrap();
                    let suffix = match w.key.as_str() {
                        "5h" => "fiveHour".to_string(),
                        "7d" => "sevenDay".into(),
                        "7d-opus" => "sevenDayOpus".into(),
                        "7d-sonnet" => "sevenDaySonnet".into(),
                        k => k.to_string(),
                    };
                    let name = match w.key.as_str() {
                        "5h" => tr(&strings::CLAUDE_FIVE_HOUR, lang).to_string(),
                        "7d" => tr(&strings::CLAUDE_WEEKLY, lang).to_string(),
                        _ => format!("Claude {}", row_name(w, lang)),
                    };
                    (format!("{}.{suffix}", a.key_prefix), name, (accounts > 1).then(|| a.title.clone()))
                }
                "codex" if w.label == "personal" => (format!("codex.{}", w.key), row_name(w, lang), None),
                "codex" => (format!("codex.{}", w.key), format!("{} {}", w.group.clone().unwrap_or_default(), row_name(w, lang)), None),
                "antigravity" => (
                    format!("antigravity.{}", w.key),
                    format!("{} {}", agy_group_title(w.group.as_deref().unwrap_or(""), lang), row_name(w, lang)),
                    None,
                ),
                _ => continue, // Cursor, OpenCode and Grok do not alert.
            };
            out.push((key, name, w.used_percent, account));
        }
    }
    out
}

/// `evaluateLimitAlerts`: alert only when the tier rises; below warn removes the key (re-arms).
/// A 2 → 1 drop keeps 2, so climbing back does not re-alert.
fn evaluate(
    windows: &[(String, String, f64, Option<String>)],
    s: &LimitSettings,
    lang: &str,
    tiers: &mut HashMap<String, u8>,
) -> Vec<LimitAlert> {
    let mut alerts = vec![];
    for (key, name, u, account) in windows {
        let tier = s.alert_tier(*u);
        if tier == 0 {
            tiers.remove(key);
            continue;
        }
        if tier <= tiers.get(key).copied().unwrap_or(0) {
            continue;
        }
        tiers.insert(key.clone(), tier);
        let mut body = fill(tr(&strings::NOTIF_BODY, lang), &[name, &crate::format::percent(*u)]);
        if let Some(a) = account {
            body = format!("{body} · {a}");
        }
        let title = tr(if tier == 2 { &strings::NOTIF_CRITICAL } else { &strings::NOTIF_WARNING }, lang);
        alerts.push(LimitAlert { key: key.clone(), title: title.into(), body, critical: tier == 2, utilization: *u });
    }
    alerts
}

/// Critical first, then the highest utilization.
fn bubble_pick(alerts: &[LimitAlert]) -> Option<&LimitAlert> {
    alerts.iter().max_by(|a, b| (a.critical, a.utilization).partial_cmp(&(b.critical, b.utilization)).unwrap_or(std::cmp::Ordering::Equal))
}

// ---- refresh ----

/// Called by the shell scheduler (automatic) and by `limits_refresh` (manual: re-reads tokens,
/// ignores the 429 backoff). Runs one at a time; a concurrent call waits and runs after.
pub fn refresh(app: &AppHandle) -> Vec<ProviderLimits> {
    run(app, false)
}

fn run(app: &AppHandle, manual: bool) -> Vec<ProviderLimits> {
    let s = crate::settings::current(app);
    let now = Utc::now();
    let work = app.state::<WorkState>();
    let mut w = work.lock().unwrap_or_else(|e| e.into_inner());
    if enabled(&s, "claude_code") {
        refresh_claude(&mut w, &s, manual, now);
    } else {
        w.claude.clear();
    }
    refresh_sources(&mut w, &s, manual, now);
    if s.limits.status_checks_enabled {
        for fresh in status::fetch() {
            w.statuses.retain(|o| o.provider != fresh.provider);
            w.statuses.push(fresh);
        }
    } else {
        w.statuses.clear();
    }
    let all = build(&w, &s);
    arm_candy(&mut w, &all);
    let lang = s.shell.language.as_str();
    let alerts = evaluate(&alert_windows(&all, lang), &s.limits, lang, &mut w.alerts);
    let ex = extras(&w, &all);
    drop(w);
    if s.limits.limit_notifications {
        for a in &alerts {
            crate::shell::notify(app, &a.title, &a.body);
        }
    }
    let shared = app.state::<SharedState>();
    let mut sh = shared.lock().unwrap();
    if s.pet.floating_pet_enabled && s.pet.floating_pet_bubble_alerts {
        if let Some(b) = bubble_pick(&alerts) {
            sh.bubble = Some((b.clone(), Instant::now()));
            let _ = app.emit(EV_ALERT, b);
        }
    }
    sh.limits = all.clone();
    if sh.extras != ex {
        sh.extras = ex.clone();
        let _ = app.emit(EV_EXTRAS, &ex);
    }
    all
}

fn arm_candy(w: &mut Work, limits: &[ProviderLimits]) {
    let before = w.persisted.candy_armed.len();
    for p in limits.iter().filter(|p| p.account.as_ref().is_some_and(|a| !a.is_default)) {
        let prefix = &p.account.as_ref().unwrap().key_prefix;
        for (key, suffix) in [("5h", "fiveHour"), ("7d", "sevenDay")] {
            if p.windows.iter().any(|w| w.key == key && w.used_percent < 100.0) {
                w.persisted.candy_armed.insert(format!("{prefix}.{suffix}"));
            }
        }
    }
    if w.persisted.candy_armed.len() != before {
        crate::store::save_json(&w.dir.join("limits-state.json"), &w.persisted);
    }
}

/// Re-sends the current rows after a change outside a refresh (session key, settings).
fn publish(app: &AppHandle) {
    let s = crate::settings::current(app);
    let work = app.state::<WorkState>();
    let Ok(w) = work.try_lock() else { return }; // a running refresh publishes anyway
    let all = build(&w, &s);
    let ex = extras(&w, &all);
    drop(w);
    let shared = app.state::<SharedState>();
    let mut sh = shared.lock().unwrap();
    let changed = sh.limits != all;
    sh.limits = all.clone();
    sh.extras = ex.clone();
    drop(sh);
    let _ = app.emit(EV_EXTRAS, &ex);
    if changed {
        crate::economy::on_limits(app, &all);
        crate::shell::tray::update(app);
        let _ = app.emit(crate::model::EV_LIMITS, &all);
    }
}

// ---- derived values for other areas ----

fn used_today(app: &AppHandle) -> std::collections::HashSet<String> {
    crate::providers::current(app)
        .today_by_provider(&crate::model::today_key())
        .into_iter()
        .filter(|(_, n)| *n > 0)
        .map(|(id, _)| id)
        .collect()
}

fn tracked(limits: &[ProviderLimits]) -> Option<&ProviderLimits> {
    limits.iter().find(|p| p.account.as_ref().is_some_and(|a| a.tracked))
}

/// `forecastDepletion`: minutes until 100% from this block's burn, or `None` when unreliable.
pub fn forecast_minutes(block_tokens: u64, tokens_per_minute: f64, utilization: f64) -> Option<f64> {
    if !(5.0..100.0).contains(&utilization) || block_tokens == 0 || tokens_per_minute < 10_000.0 {
        return None;
    }
    let minutes = (100.0 - utilization) * (block_tokens as f64 / utilization) / tokens_per_minute;
    (minutes.is_finite() && minutes < 1440.0).then_some(minutes)
}

/// The tracked account's 5-hour forecast: `(depletion, before_reset)`.
fn forecast(app: &AppHandle, limits: &[ProviderLimits], now: DateTime<Utc>) -> Option<(DateTime<Utc>, bool)> {
    let w = tracked(limits)?.windows.iter().find(|w| w.key == "5h")?;
    let reset = http::iso(w.resets_at.as_deref()?)?;
    if w.used_percent >= 100.0 {
        return Some((now, true));
    }
    let snap = crate::providers::current(app);
    let block = snap.providers.iter().find(|p| p.provider_id == "claude_code")?.active_block.clone()?;
    let minutes = forecast_minutes(block.total, block.tokens_per_minute, w.used_percent)?;
    let at = now + chrono::Duration::seconds((minutes * 60.0) as i64);
    Some((at, at < reset))
}

/// Menu bar warning and companion mood (`isLimitWarning`): any tracked-Claude, Codex or
/// Antigravity window at or above critical, or a 5-hour forecast that runs out before the reset.
#[allow(dead_code)] // read by companion / pet / economy (ARCHITECTURE.md cross-area calls)
pub fn is_limit_warning(app: &AppHandle) -> bool {
    let s = crate::settings::current(app);
    let limits = current(app);
    let crit = s.limits.crit_threshold;
    let hot = |p: &ProviderLimits| p.windows.iter().any(|w| !w.expired && w.used_percent >= crit);
    tracked(&limits).is_some_and(hot)
        || limits.iter().filter(|p| p.provider == "codex" || p.provider == "antigravity").any(hot)
        || forecast(app, &limits, Utc::now()).is_some_and(|(_, before)| before)
}

/// Highest utilization over providers used today (Claude tracked account, Codex windows without
/// the spend cap, Antigravity); for compact surfaces such as the pet's hover text.
#[allow(dead_code)] // read by companion / pet / economy (ARCHITECTURE.md cross-area calls)
pub fn highest_limit_utilization(app: &AppHandle) -> Option<f64> {
    let today = used_today(app);
    let limits = current(app);
    let mut all = vec![];
    if today.contains("claude_code") {
        all.extend(tracked(&limits).into_iter().flat_map(utilizations));
    }
    for p in limits.iter().filter(|p| (p.provider == "codex" || p.provider == "antigravity") && today.contains(&p.provider)) {
        all.extend(p.windows.iter().filter(|w| w.label != "personal").map(|w| w.used_percent));
    }
    all.into_iter().reduce(f64::max)
}

/// The floating-pet bubble for the latest alert, for 6 s after it fired.
pub fn bubble_alert(app: &AppHandle) -> Option<LimitAlert> {
    let shared = app.state::<SharedState>();
    let sh = shared.lock().unwrap();
    sh.bubble.as_ref().filter(|(_, at)| at.elapsed().as_secs() < BUBBLE_TTL_SECS).map(|(a, _)| a.clone())
}

/// Windows eligible for Rare Candy (shop-bag-dex.md §3.6): Claude 5h/weekly per account (extra
/// accounts only once armed), Codex primary/secondary without an epoch, Antigravity 5h/weekly.
/// Opus/Sonnet, scoped windows, spend caps and synthetic expired log values are left out.
#[allow(dead_code)] // read by companion / pet / economy (ARCHITECTURE.md cross-area calls)
pub fn candy_windows(app: &AppHandle) -> Vec<CandyWindow> {
    let lang = crate::settings::current(app).shell.language;
    let armed = app.state::<WorkState>().lock().map(|w| w.persisted.candy_armed.clone()).unwrap_or_default();
    candy_windows_of(&current(app), &lang, &armed)
}

fn candy_windows_of(limits: &[ProviderLimits], lang: &str, armed: &BTreeSet<String>) -> Vec<CandyWindow> {
    let accounts = limits.iter().filter(|p| p.account.is_some()).count();
    let mut out = vec![];
    for p in limits {
        match p.provider.as_str() {
            "claude_code" => {
                let a = p.account.as_ref().unwrap();
                let suffix = if accounts > 1 { format!(" ({})", a.title) } else { String::new() };
                for (key, sk, kind, name) in [("5h", "fiveHour", "session", &strings::CLAUDE_FIVE_HOUR), ("7d", "sevenDay", "weekly", &strings::CLAUDE_WEEKLY)] {
                    let Some(w) = p.windows.iter().find(|w| w.key == key) else { continue };
                    let ck = format!("{}.{sk}", a.key_prefix);
                    if a.is_default || armed.contains(&ck) {
                        out.push(CandyWindow { key: ck, name: format!("{}{suffix}", tr(name, lang)), kind, utilization: w.used_percent, epoch: w.resets_at.clone() });
                    }
                }
            }
            "codex" => {
                for w in p.windows.iter().filter(|w| w.label == "codex" && !w.expired) {
                    let weekly = w.arg.as_deref().and_then(|m| m.parse::<u64>().ok()).is_some_and(|m| m > 1440);
                    out.push(CandyWindow {
                        key: format!("codex.{}", w.key),
                        name: format!("{} {}", w.group.clone().unwrap_or_default(), row_name(w, lang)),
                        kind: if weekly { "weekly" } else { "session" },
                        utilization: w.used_percent,
                        epoch: None,
                    });
                }
            }
            "antigravity" => {
                for w in &p.windows {
                    let g = w.group.as_deref().unwrap_or("");
                    let gk = antigravity::group_key(g);
                    let (slot, kind) = match w.label.as_str() {
                        "fiveHour" => ("5h", "session"),
                        "weekly" => ("weekly", "weekly"),
                        _ => continue,
                    };
                    let key = format!("antigravity.{gk}.{slot}");
                    if out.iter().any(|c: &CandyWindow| c.key == key) {
                        continue; // first bucket of its kind per group
                    }
                    out.push(CandyWindow { key, name: format!("{} {}", agy_group_title(g, lang), row_name(w, lang)), kind, utilization: w.used_percent, epoch: w.resets_at.clone() });
                }
            }
            _ => {}
        }
    }
    out
}

/// Bottom line of the tray tooltip (`menuLimitParts`), `None` when hidden. Only providers used
/// today; Claude shows its tracked account's 5-hour window, else weekly. With several Claude
/// accounts the tracked one is named on a second line (`trackedAccountToolTip`).
pub fn menu_limit_line(app: &AppHandle, s: &crate::settings::Settings) -> Option<String> {
    if !s.shell.show_limit_in_menu {
        return None;
    }
    let today = used_today(app);
    let limits = current(app);
    let (parts, tracked_title) = menu_parts(&limits, &today, &s.limits);
    if parts.is_empty() {
        return None;
    }
    let line = parts.join(" · ");
    let accounts = limits.iter().filter(|p| p.account.is_some()).count();
    Some(match tracked_title.filter(|_| accounts > 1) {
        Some(t) => format!("{line}\n{}", fill(tr(&strings::TRACKED_ACCOUNT_TOOL_TIP, &s.shell.language), &[&t])),
        None => line,
    })
}

fn menu_parts(limits: &[ProviderLimits], today: &std::collections::HashSet<String>, s: &LimitSettings) -> (Vec<String>, Option<String>) {
    let mut parts = vec![];
    let mut add = |label: &str, u: f64| parts.push(format!("{label} {}", crate::format::percent(s.display(u))));
    let mut title = None;
    if today.contains("claude_code") {
        if let Some(t) = tracked(limits) {
            if let Some(w) = t.windows.iter().find(|w| w.key == "5h").or_else(|| t.windows.iter().find(|w| w.key == "7d")) {
                add("Claude", w.used_percent);
                title = t.account.as_ref().map(|a| a.title.clone());
            }
        }
    }
    let find = |id: &str| limits.iter().find(|p| p.provider == id && today.contains(id));
    let max = |p: &ProviderLimits, f: &dyn Fn(&Window) -> bool| p.windows.iter().filter(|w| f(w)).map(|w| w.used_percent).reduce(f64::max);
    if let Some(u) = find("codex").and_then(|p| max(p, &|w| w.key.ends_with(".primary"))) {
        add("Codex", u);
    }
    if let Some(u) = find("antigravity").and_then(|p| max(p, &|w| w.label == "fiveHour")) {
        add("AGY", u);
    }
    if let Some(u) = find("cursor").and_then(|p| max(p, &|w| w.key == "included")) {
        add("Cursor", u);
    }
    if let Some(u) = find("opencode").and_then(|p| max(p, &|w| w.key == "rolling")) {
        add("OpenCode", u);
    }
    if let Some(u) = find("grok").and_then(|p| max(p, &|w| w.key == "credits")) {
        add("Grok", u);
    }
    (parts, title)
}

// ---- commands ----

#[tauri::command]
fn limits(app: AppHandle) -> Vec<ProviderLimits> {
    current(&app)
}

#[tauri::command]
fn limits_extras(app: AppHandle) -> Extras {
    app.state::<SharedState>().lock().unwrap().extras.clone()
}

#[tauri::command]
fn limit_bubble(app: AppHandle) -> Option<LimitAlert> {
    bubble_alert(&app)
}

/// The "Refresh" / "Retry" buttons: re-reads every token and ignores the backoff. Returns the
/// default Claude account's error code, if any, for the line under the button.
#[tauri::command]
async fn limits_refresh(app: AppHandle) -> Option<String> {
    let handle = app.clone();
    let all = tauri::async_runtime::spawn_blocking(move || run(&handle, true)).await.unwrap_or_default();
    crate::economy::on_limits(&app, &all);
    crate::shell::tray::update(&app);
    let _ = app.emit(crate::model::EV_LIMITS, &all);
    all.iter().find(|p| p.account.as_ref().is_some_and(|a| a.is_default)).and_then(|p| p.error.clone())
}

fn work_dir(app: &AppHandle) -> PathBuf {
    app.state::<WorkState>().lock().unwrap_or_else(|e| e.into_inner()).dir.clone()
}

fn account_arg(account: &Option<String>) -> Option<&str> {
    account.as_deref().filter(|a| *a != "default")
}

/// Settings "Save": validates against claude.ai, stores, refreshes. Errors return their code.
#[tauri::command]
async fn session_key_save(app: AppHandle, account: Option<String>, key: String) -> Result<(), String> {
    let (stored, orgs) = tauri::async_runtime::spawn_blocking(move || session_key::validate(&key))
        .await
        .map_err(|_| "network".to_string())?
        .map_err(|e| e.code())?;
    let dir = work_dir(&app);
    if !session_key::save(&dir, account_arg(&account), &stored) {
        return Err("format".into());
    }
    let id = account.unwrap_or_else(|| "default".into());
    {
        let work = app.state::<WorkState>();
        let mut w = work.lock().unwrap_or_else(|e| e.into_inner());
        w.orgs.insert(id.clone(), orgs);
        if let Some(src) = w.claude.iter_mut().find(|s| s.id == id) {
            src.expiry = None;
        }
    }
    let _ = limits_refresh(app).await;
    Ok(())
}

#[tauri::command]
async fn session_key_delete(app: AppHandle, account: Option<String>) {
    session_key::delete(&work_dir(&app), account_arg(&account));
    {
        let work = app.state::<WorkState>();
        let mut w = work.lock().unwrap_or_else(|e| e.into_inner());
        let id = account.clone().unwrap_or_else(|| "default".into());
        w.orgs.remove(&id);
        if let Some(src) = w.claude.iter_mut().find(|s| s.id == id && s.expiry == Some("sessionKey")) {
            src.expiry = None;
        }
    }
    let _ = limits_refresh(app).await;
}

/// Fills the organization picker the first time Settings opens (`refreshSessionOrganizations`).
#[tauri::command]
async fn session_key_orgs(app: AppHandle, account: Option<String>) -> Result<Vec<session_key::Org>, String> {
    let dir = work_dir(&app);
    let Some(stored) = session_key::load(&dir, account_arg(&account)) else { return Ok(vec![]) };
    let orgs = tauri::async_runtime::spawn_blocking(move || session_key::organizations(&stored.key))
        .await
        .map_err(|_| "network".to_string())?
        .map_err(|e| e.code())?;
    {
        let work = app.state::<WorkState>();
        let mut w = work.lock().unwrap_or_else(|e| e.into_inner());
        w.orgs.insert(account.unwrap_or_else(|| "default".into()), orgs.clone());
    }
    publish(&app);
    Ok(orgs)
}

#[tauri::command]
async fn session_key_select_org(app: AppHandle, account: Option<String>, id: String) {
    let dir = work_dir(&app);
    if let Some(stored) = session_key::load(&dir, account_arg(&account)) {
        session_key::save(&dir, account_arg(&account), &session_key::Stored { organization_id: Some(id), ..stored });
        let _ = limits_refresh(app).await;
    }
}

/// Called by the shell on every settings change and once at launch: disabled providers and the
/// status-check switch take effect without waiting for the next poll.
pub fn on_settings(app: &AppHandle, s: &crate::settings::Settings) {
    {
        let work = app.state::<WorkState>();
        let Ok(mut w) = work.try_lock() else { return };
        if !s.limits.status_checks_enabled {
            w.statuses.clear();
        }
        if !enabled(s, "claude_code") {
            w.claude.clear();
        }
        w.sources.retain(|id, _| enabled(s, id));
    }
    publish(app);
}

crate::area_commands![limits, limits_extras, limit_bubble, limits_refresh, session_key_save, session_key_delete, session_key_orgs, session_key_select_org];

#[cfg(test)]
mod tests {
    use super::*;

    fn win(key: &str, label: &str, u: f64) -> Window {
        Window { key: key.into(), label: label.into(), used_percent: u, ..Default::default() }
    }

    fn claude(id: &str, windows: Vec<Window>) -> ProviderLimits {
        ProviderLimits {
            account: Some(ClaudeAccount {
                id: id.into(),
                key_prefix: if id == "default" { "claude".into() } else { format!("claude.{id}") },
                title: format!("{id}@x.com"),
                is_default: id == "default",
                tracked: id == "default",
                ..Default::default()
            }),
            windows,
            ..ProviderLimits::new("claude_code")
        }
    }

    #[test]
    fn sanitizes_thresholds_with_gap() {
        let mut s = LimitSettings { warn_threshold: 95.0, crit_threshold: 90.0, ..Default::default() };
        s.sanitize();
        assert_eq!((s.warn_threshold, s.crit_threshold), (85.0, 90.0));
        let mut s = LimitSettings { warn_threshold: 30.0, crit_threshold: 120.0, claude_tracked_account: "x".into(), ..Default::default() };
        s.sanitize();
        assert_eq!((s.warn_threshold, s.crit_threshold, s.claude_tracked_account.as_str()), (50.0, 100.0, "automatic"));
        let s = LimitSettings::default();
        assert_eq!([79.0, 80.0, 95.0].map(|p| s.alert_tier(p)), [0, 1, 2]);
    }

    #[test]
    fn alerts_are_edge_triggered_and_rearm_below_warn() {
        let s = LimitSettings::default();
        let mut tiers = HashMap::new();
        let w = |u: f64| vec![("claude.fiveHour".to_string(), "Claude 5-hour session".to_string(), u, None)];
        let a = evaluate(&w(82.0), &s, "en", &mut tiers);
        assert_eq!((a[0].title.as_str(), a[0].body.as_str(), a[0].critical), ("Limit warning", "Claude 5-hour session at 82%", false));
        assert!(evaluate(&w(84.0), &s, "en", &mut tiers).is_empty());
        assert_eq!(evaluate(&w(96.5), &s, "ko", &mut tiers)[0].body, "Claude 5-hour session 한도 96.5% 사용");
        assert!(evaluate(&w(90.0), &s, "en", &mut tiers).is_empty()); // 2 → 1 keeps 2
        assert!(evaluate(&w(97.0), &s, "en", &mut tiers).is_empty());
        assert!(evaluate(&w(10.0), &s, "en", &mut tiers).is_empty()); // re-armed
        assert_eq!(evaluate(&w(81.0), &s, "en", &mut tiers).len(), 1);
    }

    #[test]
    fn alert_names_and_keys() {
        let mut agy = ProviderLimits::new("antigravity");
        agy.windows = vec![Window { group: Some("Gemini Models".into()), ..win("gemini.g-5h", "fiveHour", 1.0) }];
        let mut codex = ProviderLimits::new("codex");
        codex.windows = vec![
            Window { group: Some("Codex".into()), arg: Some("300".into()), ..win("codex.primary", "codex", 1.0) },
            win("codex.individual", "personal", 1.0),
        ];
        let mut grok = ProviderLimits::new("grok");
        grok.windows = vec![win("credits", "credits", 99.0)];
        let all = [
            claude("default", vec![win("5h", "fiveHour", 1.0), Window { arg: Some("Opus".into()), ..win("scoped.weekly_scoped.Opus.0", "scoped", 1.0) }]),
            claude("ab12cd34", vec![win("7d-sonnet", "weeklySonnet", 1.0)]),
            codex,
            agy,
            grok,
        ];
        let got: Vec<_> = alert_windows(&all, "en").into_iter().map(|(k, n, _, a)| (k, n, a)).collect();
        let want = [
            ("claude.fiveHour", "Claude 5-hour session", Some("default@x.com")),
            ("claude.scoped.weekly_scoped.Opus.0", "Claude Weekly Opus", Some("default@x.com")),
            ("claude.ab12cd34.sevenDaySonnet", "Claude Weekly Sonnet", Some("ab12cd34@x.com")),
            ("codex.codex.primary", "Codex 5-hour session", None),
            ("codex.codex.individual", "Codex personal limit", None),
            ("antigravity.gemini.g-5h", "Gemini Models 5-hour session", None),
        ];
        let want: Vec<_> = want.iter().map(|(k, n, a)| (k.to_string(), n.to_string(), a.map(String::from))).collect();
        assert_eq!(got, want);
    }

    #[test]
    fn bubble_prefers_critical_then_highest() {
        let a = |c: bool, u: f64| LimitAlert { key: format!("{u}"), title: String::new(), body: String::new(), critical: c, utilization: u };
        let alerts = [a(false, 94.0), a(true, 95.0), a(true, 99.0)];
        assert_eq!(bubble_pick(&alerts).unwrap().utilization, 99.0);
        assert!(bubble_pick(&[]).is_none());
    }

    #[test]
    fn backoff_doubles_to_an_hour_and_honours_retry_after() {
        let now = Utc::now();
        let mut b = Backoff::default();
        b.hit(None, now);
        assert_eq!(b.interval, 300.0);
        assert!(b.active(now + chrono::Duration::seconds(299)) && !b.active(now + chrono::Duration::seconds(301)));
        for _ in 0..5 {
            b.hit(None, now);
        }
        assert_eq!(b.interval, 3600.0);
        b.hit(Some(10.0), now);
        assert!(!b.active(now + chrono::Duration::seconds(11)));
    }

    #[test]
    fn forecast_needs_enough_signal() {
        assert_eq!(forecast_minutes(1_000_000, 20_000.0, 50.0), Some(50.0));
        assert_eq!(forecast_minutes(1_000_000, 9_999.0, 50.0), None);
        assert_eq!(forecast_minutes(1_000_000, 20_000.0, 4.9), None);
        assert_eq!(forecast_minutes(0, 20_000.0, 50.0), None);
        assert_eq!(forecast_minutes(100_000_000, 10_000.0, 5.0), None); // ≥ 1 day
    }

    #[test]
    fn menu_parts_follow_used_today_and_headline() {
        let today: std::collections::HashSet<String> = ["claude_code", "codex"].map(String::from).into();
        let mut codex = ProviderLimits::new("codex");
        codex.windows = vec![win("codex.primary", "codex", 30.0), win("other.primary", "codex", 45.0), win("codex.secondary", "codex", 90.0)];
        let mut cursor = ProviderLimits::new("cursor");
        cursor.windows = vec![win("included", "cursorIncluded", 10.0)];
        let weekly_only = claude("default", vec![win("7d", "weekly", 62.5)]);
        let s = LimitSettings::default();
        let (parts, title) = menu_parts(&[weekly_only.clone(), codex.clone(), cursor.clone()], &today, &s);
        assert_eq!(parts, ["Claude 62.5%", "Codex 45%"]);
        assert_eq!(title.as_deref(), Some("default@x.com"));
        let remaining = LimitSettings { limit_display: "remaining".into(), ..Default::default() };
        assert_eq!(menu_parts(&[weekly_only, codex], &today, &remaining).0, ["Claude 37.5%", "Codex 55%"]);
    }

    #[test]
    fn candy_windows_skip_unarmed_and_synthetic() {
        let mut codex = ProviderLimits::new("codex");
        codex.windows = vec![
            Window { group: Some("Codex".into()), arg: Some("300".into()), ..win("codex.primary", "codex", 100.0) },
            Window { group: Some("Codex".into()), arg: Some("10080".into()), expired: true, ..win("codex.secondary", "codex", 0.0) },
            win("codex.individual", "personal", 100.0),
        ];
        let mut agy = ProviderLimits::new("antigravity");
        agy.windows = vec![
            Window { group: Some("Claude and GPT".into()), ..win("3p.a-5h", "fiveHour", 1.0) },
            Window { group: Some("Claude and GPT".into()), ..win("3p.b-5h", "fiveHour", 2.0) },
        ];
        let all = [
            claude("default", vec![win("5h", "fiveHour", 100.0), win("7d-opus", "weeklyOpus", 100.0)]),
            claude("ab12cd34", vec![win("5h", "fiveHour", 100.0)]),
            codex,
            agy,
        ];
        let got = candy_windows_of(&all, "en", &BTreeSet::new());
        let keys: Vec<_> = got.iter().map(|c| (c.key.as_str(), c.kind, c.epoch.is_none())).collect();
        assert_eq!(keys, [("claude.fiveHour", "session", true), ("codex.codex.primary", "session", true), ("antigravity.3p.5h", "session", true)]);
        assert_eq!(got[0].name, "Claude 5-hour session (default@x.com)");
        let armed: BTreeSet<String> = ["claude.ab12cd34.fiveHour".to_string()].into();
        assert_eq!(candy_windows_of(&all, "en", &armed).len(), 4);
    }

    #[test]
    fn fills_placeholders() {
        assert_eq!(fill("{} at {}", &["A", "5%"]), "A at 5%");
        assert_eq!(codex_window(Some("120"), "en"), "2h");
        assert_eq!(codex_window(Some("90"), "de"), "90 Min.");
        assert_eq!(codex_window(None, "ja"), "上限");
    }

    /// `cargo test real_limits -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_limits() {
        let mut cache = claude::OAuthCache::default();
        println!("{:#?}", claude::fetch_oauth(&claude::default_root(), true, true, &mut cache));
        println!("{:#?}", codex::fetch());
        println!("{:#?}", status::fetch());
    }
}
