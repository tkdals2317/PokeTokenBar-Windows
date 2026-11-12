//! Official usage limits. Owner: limits area.
//! - Claude: `~/.claude/.credentials.json` OAuth token → `api.anthropic.com/api/oauth/usage`.
//!   Claude Code refreshes that token itself; an expired one means "run Claude Code once".
//! - Codex: the latest `rate_limits` snapshot Codex already writes into its rollout logs
//!   (`token_count` events), so no `codex app-server` process is needed.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

const CLAUDE_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
/// Newest rollout files scanned for a Codex snapshot.
const CODEX_FILES_TO_SCAN: usize = 5;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Window {
    /// "5h", "7d", "7d-opus" … ; the UI localizes.
    pub key: String,
    pub used_percent: f64,
    pub resets_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderLimits {
    pub provider: &'static str,
    pub windows: Vec<Window>,
    /// Shown instead of bars when nothing could be read.
    pub error: Option<String>,
}

impl ProviderLimits {
    fn from(provider: &'static str, r: Result<Vec<Window>, String>) -> Self {
        match r {
            Ok(windows) => ProviderLimits { provider, windows, error: None },
            Err(e) => ProviderLimits { provider, windows: vec![], error: Some(e) },
        }
    }
}

pub fn read_all() -> Vec<ProviderLimits> {
    let home = std::env::home_dir().unwrap_or_default();
    vec![
        ProviderLimits::from("claude", claude(&home)),
        ProviderLimits::from("codex", codex(&home.join(".codex/sessions"))),
    ]
}

fn claude(home: &Path) -> Result<Vec<Window>, String> {
    let creds: Value = fs::read(home.join(".claude/.credentials.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or("no_credentials")?;
    let oauth = &creds["claudeAiOauth"];
    let token = oauth["accessToken"].as_str().ok_or("no_credentials")?;
    if let Some(ms) = oauth["expiresAt"].as_i64() {
        if ms < Utc::now().timestamp_millis() {
            return Err("token_expired".into());
        }
    }
    let resp = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .build()
        .get(CLAUDE_USAGE_URL)
        .set("Authorization", &format!("Bearer {token}"))
        .set("anthropic-beta", "oauth-2025-04-20")
        .call();
    let body: Value = match resp {
        Ok(r) => r.into_json().map_err(|e| e.to_string())?,
        Err(ureq::Error::Status(401 | 403, _)) => return Err("token_expired".into()),
        Err(ureq::Error::Status(429, _)) => return Err("rate_limited".into()),
        Err(e) => return Err(e.to_string()),
    };
    Ok(parse_claude(&body))
}

pub fn parse_claude(body: &Value) -> Vec<Window> {
    [("five_hour", "5h"), ("seven_day", "7d"), ("seven_day_opus", "7d-opus"), ("seven_day_sonnet", "7d-sonnet")]
        .iter()
        .filter_map(|(field, key)| {
            let w = body.get(*field)?;
            Some(Window {
                key: key.to_string(),
                used_percent: w.get("utilization")?.as_f64()?,
                resets_at: w["resets_at"].as_str().map(str::to_string),
            })
        })
        .collect()
}

fn newest_rollouts(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<(std::time::SystemTime, PathBuf)>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            match e.metadata() {
                Ok(m) if m.is_dir() => walk(&p, out),
                Ok(m) if p.extension().is_some_and(|x| x == "jsonl") => {
                    out.push((m.modified().unwrap_or(std::time::UNIX_EPOCH), p))
                }
                _ => {}
            }
        }
    }
    let mut files = vec![];
    walk(root, &mut files);
    files.sort_by(|a, b| b.0.cmp(&a.0));
    files.into_iter().take(CODEX_FILES_TO_SCAN).map(|(_, p)| p).collect()
}

fn codex(root: &Path) -> Result<Vec<Window>, String> {
    for file in newest_rollouts(root) {
        let Ok(text) = fs::read_to_string(&file) else { continue };
        let latest = text
            .lines()
            .rev()
            .filter(|l| l.contains("\"rate_limits\""))
            .find_map(|l| parse_codex_line(l, Utc::now()).filter(|w| !w.is_empty()));
        if let Some(windows) = latest {
            return Ok(windows);
        }
    }
    Err("no_data".into())
}

/// A window whose reset time has passed is shown as 0%: the snapshot predates the reset.
pub fn parse_codex_line(line: &str, now: DateTime<Utc>) -> Option<Vec<Window>> {
    let v: Value = serde_json::from_str(line).ok()?;
    let limits = &v["payload"]["rate_limits"];
    let windows = ["primary", "secondary"]
        .iter()
        .filter_map(|slot| {
            let w = limits.get(*slot).filter(|w| w.is_object())?;
            let minutes = w["window_minutes"].as_u64()?;
            let resets = w["resets_at"].as_i64().and_then(|s| DateTime::from_timestamp(s, 0));
            let expired = resets.is_some_and(|r| r <= now);
            Some(Window {
                key: match minutes {
                    m if m % 1440 == 0 => format!("{}d", m / 1440),
                    m if m % 60 == 0 => format!("{}h", m / 60),
                    m => format!("{m}m"),
                },
                used_percent: if expired { 0.0 } else { w["used_percent"].as_f64()? },
                resets_at: resets.filter(|_| !expired).map(|r| r.to_rfc3339()),
            })
        })
        .collect();
    Some(windows)
}


// ---- area wiring (see ARCHITECTURE.md) ----

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
    /// "automatic" | "highest" | an account id
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

impl LimitSettings {
    pub fn sanitize(&mut self) {
        if self.limit_display != "remaining" {
            self.limit_display = "used".into();
        }
        self.warn_threshold = self.warn_threshold.clamp(50.0, 95.0);
        self.crit_threshold = self.crit_threshold.clamp(80.0, 100.0).max(self.warn_threshold);
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
}

/// Notification titles. es/fr/pt/de/ru fall back to English until this area ports them.
fn text(lang: &str, key: &str) -> &'static str {
    match (lang, key) {
        ("ko", "warn") => "한도 경고",
        ("ko", "crit") => "한도 임박",
        ("ja", "warn") => "上限の警告",
        ("ja", "crit") => "上限間近",
        (_, "warn") => "Limit warning",
        (_, "crit") => "Limit almost reached",
        _ => "",
    }
}

type Shared = std::sync::Mutex<Vec<ProviderLimits>>;
/// "provider.window" → alert tier already notified, so each crossing alerts once.
type Alerts = std::sync::Mutex<std::collections::HashMap<String, u8>>;

pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    use tauri::Manager;
    app.manage::<Shared>(Default::default());
    app.manage::<Alerts>(Default::default());
    Ok(())
}

pub fn current(app: &tauri::AppHandle) -> Vec<ProviderLimits> {
    use tauri::Manager;
    app.state::<Shared>().lock().unwrap().clone()
}

/// Called by the shell scheduler (never more often than every 5 min, or on manual refresh).
/// Reads, alerts, stores and returns; the shell emits `limits` and updates the tray.
pub fn refresh(app: &tauri::AppHandle) -> Vec<ProviderLimits> {
    use tauri::Manager;
    let all = read_all();
    let s = crate::settings::current(app);
    {
        let alerts = app.state::<Alerts>();
        let mut alerted = alerts.lock().unwrap();
        for p in &all {
            for w in &p.windows {
                let key = format!("{}.{}", p.provider, w.key);
                let tier = s.limits.alert_tier(w.used_percent);
                let before = alerted.insert(key, tier).unwrap_or(0);
                if s.limits.limit_notifications && tier > before {
                    let title = text(&s.shell.language, if tier == 2 { "crit" } else { "warn" });
                    crate::shell::notify(app, title, &format!("{} {} {:.0}%", p.provider, w.key, w.used_percent));
                }
            }
        }
    }
    *app.state::<Shared>().lock().unwrap() = all.clone();
    all
}

/// Bottom line of the tray tooltip (`menuLimitParts`), `None` when hidden.
/// ponytail: highest window per provider; the spec's per-provider headline/pace rules and the
/// "used today" filter are this area's to port (limits.md §3.14).
pub fn menu_limit_line(app: &tauri::AppHandle, s: &crate::settings::Settings) -> Option<String> {
    if !s.shell.show_limit_in_menu {
        return None;
    }
    let parts: Vec<String> = current(app)
        .iter()
        .filter_map(|p| {
            let u = p.windows.iter().map(|w| w.used_percent).reduce(f64::max)?;
            let shown = if s.limits.limit_display == "remaining" { (100.0 - u).max(0.0) } else { u };
            let label = match p.provider {
                "claude" => "Claude",
                "codex" => "Codex",
                other => other,
            };
            Some(format!("{label} {}", crate::format::percent(shown)))
        })
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

#[tauri::command]
fn limits(app: tauri::AppHandle) -> Vec<ProviderLimits> {
    current(&app)
}

crate::area_commands![limits];

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_skips_null_windows() {
        let body = json!({
            "five_hour": { "utilization": 7.0, "resets_at": "2026-10-10T11:10:00+00:00" },
            "seven_day": { "utilization": 42.0, "resets_at": null },
            "seven_day_opus": null
        });
        let w = parse_claude(&body);
        assert_eq!(w.len(), 2);
        assert_eq!((w[0].key.as_str(), w[0].used_percent), ("5h", 7.0));
        assert_eq!(w[1].resets_at, None);
    }

    #[test]
    fn codex_parses_and_zeroes_expired_windows() {
        let line = r#"{"type":"event_msg","payload":{"type":"token_count","rate_limits":{
            "primary":{"used_percent":30.0,"window_minutes":300,"resets_at":2000},
            "secondary":{"used_percent":12.0,"window_minutes":10080,"resets_at":1000}}}}"#
            .replace('\n', "");
        let now = DateTime::from_timestamp(1500, 0).unwrap();
        let w = parse_codex_line(&line, now).unwrap();
        assert_eq!((w[0].key.as_str(), w[0].used_percent), ("5h", 30.0));
        assert_eq!((w[1].key.as_str(), w[1].used_percent, w[1].resets_at.clone()), ("7d", 0.0, None));

        let empty = r#"{"payload":{"rate_limits":{"primary":null,"secondary":null}}}"#;
        assert_eq!(parse_codex_line(empty, now), Some(vec![]));
    }

    #[test]
    fn sanitizes_thresholds_and_tiers() {
        let mut s = LimitSettings { warn_threshold: 90.0, crit_threshold: 85.0, ..Default::default() };
        s.sanitize();
        assert_eq!(s.crit_threshold, 90.0);
        let s = LimitSettings::default();
        assert_eq!([79.0, 80.0, 95.0].map(|p| s.alert_tier(p)), [0, 1, 2]);
    }

    /// `cargo test real_limits -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_limits() {
        println!("{:#?}", read_all());
    }
}
