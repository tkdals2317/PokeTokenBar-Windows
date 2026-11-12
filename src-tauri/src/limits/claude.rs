//! Claude official limits (limits.md §3.2-3.3): OAuth credential file → `api/oauth/usage` + profile,
//! the usage response model (legacy windows + `limits[]`), plan/account display, account folders.

use super::http::{self, Body, LimitError, Result};
use super::Window;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const PROFILE_URL: &str = "https://api.anthropic.com/api/oauth/profile";
const FIVE_HOUR: f64 = 5.0 * 3600.0;
const SEVEN_DAY: f64 = 7.0 * 24.0 * 3600.0;
/// A token this close to expiry counts as expired.
const EXPIRY_SKEW: f64 = 60.0;
const PROFILE_CACHE_MAX: usize = 8;

// ---- usage response model ----

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Win {
    pub utilization: Option<f64>,
    pub resets_at: Option<String>,
}

impl Win {
    fn reset(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.resets_at.as_deref().and_then(http::iso)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Entry {
    pub kind: Option<String>,
    pub percent: Option<f64>,
    pub resets_at: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Status {
    pub five_hour: Option<Win>,
    pub seven_day: Option<Win>,
    pub opus: Option<Win>,
    pub sonnet: Option<Win>,
    pub limits: Vec<Entry>,
    pub subscription_type: Option<String>,
    pub rate_limit_tier: Option<String>,
    pub plan_display_name: Option<String>,
    pub email: Option<String>,
    pub org: Option<String>,
}

fn win(v: &Value) -> Option<Win> {
    v.is_object().then(|| Win {
        utilization: v.get("utilization").and_then(Value::as_f64),
        resets_at: v.get("resets_at").and_then(http::text),
    })
}

pub fn parse_usage(body: &Value) -> Status {
    let limits = body["limits"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|e| Entry {
                    kind: http::text(&e["kind"]),
                    percent: e["percent"].as_f64(),
                    resets_at: http::text(&e["resets_at"]),
                    model: http::text(&e["scope"]["model"]["display_name"]),
                })
                .collect()
        })
        .unwrap_or_default();
    Status {
        five_hour: win(&body["five_hour"]),
        seven_day: win(&body["seven_day"]),
        opus: win(&body["seven_day_opus"]),
        sonnet: win(&body["seven_day_sonnet"]),
        limits,
        ..Default::default()
    }
}

impl Status {
    /// Legacy windows win; `limits[]` adds the rest. With no legacy windows at all, every entry shows.
    pub fn scoped(&self) -> Vec<&Entry> {
        let all = self.five_hour.is_none() && self.seven_day.is_none();
        self.limits
            .iter()
            .filter(|e| all || !matches!(e.kind.as_deref(), Some("session" | "weekly_all")))
            .collect()
    }

    /// Any window with usage or a running reset (`hasUsageData`, session-key org pick).
    pub fn has_usage_data(&self) -> bool {
        let w = [&self.five_hour, &self.seven_day, &self.opus, &self.sonnet]
            .into_iter()
            .flatten()
            .any(|w| w.utilization.unwrap_or(0.0) > 0.0 || w.resets_at.is_some());
        w || self.limits.iter().any(|e| e.percent.unwrap_or(0.0) > 0.0 || e.resets_at.is_some())
    }

    /// `planDisplay` (`Models.swift:281-317`).
    pub fn plan_display(&self) -> Option<String> {
        if let Some(n) = self.plan_display_name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
            return Some(n.to_string());
        }
        let sub = self.subscription_type.as_deref().filter(|s| !s.is_empty())?;
        let mut chars = sub.chars();
        let base: String = chars.next().map(|c| c.to_uppercase().chain(chars).collect())?;
        let Some(mult) = self.rate_limit_tier.as_deref().and_then(tier_multiplier) else { return Some(base) };
        if !sub.eq_ignore_ascii_case("max") {
            return Some(format!("{base} {mult}"));
        }
        Some(match mult.as_str() {
            "5x" => "Max".into(),
            "20x" => "Ultra".into(),
            m => format!("Max {m}"),
        })
    }

    /// `email`, or `email · org` when the org name is not just the email again.
    pub fn account_display(&self) -> Option<String> {
        let email = self.email.as_deref().filter(|e| !e.is_empty())?;
        match self.org.as_deref().filter(|o| !o.is_empty() && !o.contains(email)) {
            Some(org) => Some(format!("{email} · {org}")),
            None => Some(email.to_string()),
        }
    }

    /// Tab title: the org for a team plan, the email for a personal one, else the folder.
    pub fn title(&self, fallback: &str) -> String {
        let Some(email) = self.email.as_deref().filter(|e| !e.is_empty()) else { return fallback.into() };
        match self.org.as_deref().filter(|o| !o.is_empty() && !o.contains(email)) {
            Some(org) => org.into(),
            None => email.into(),
        }
    }

    /// Popover rows, in the macOS order. Keys stay `5h`/`7d`/`7d-opus`/`7d-sonnet` (economy reads them).
    pub fn windows(&self) -> Vec<Window> {
        let mut out = vec![];
        let legacy = [
            (&self.five_hour, "5h", "fiveHour", FIVE_HOUR),
            (&self.seven_day, "7d", "weekly", SEVEN_DAY),
            (&self.opus, "7d-opus", "weeklyOpus", SEVEN_DAY),
            (&self.sonnet, "7d-sonnet", "weeklySonnet", SEVEN_DAY),
        ];
        for (w, key, label, span) in legacy {
            let Some(w) = w else { continue };
            let Some(u) = w.utilization else { continue };
            out.push(Window {
                key: key.into(),
                label: label.into(),
                used_percent: u,
                resets_at: w.reset().map(|d| d.to_rfc3339()),
                span_secs: Some(span),
                not_started: u == 0.0 && w.reset().is_none(),
                ..Default::default()
            });
        }
        for (i, e) in self.scoped().into_iter().enumerate() {
            let Some(u) = e.percent else { continue };
            let reset = e.resets_at.as_deref().and_then(http::iso);
            let (label, arg) = match e.kind.as_deref() {
                Some("session") => ("fiveHour", None),
                Some("weekly_all") => ("weekly", None),
                Some("weekly_scoped") => ("scoped", e.model.clone()),
                other => {
                    let base = other.unwrap_or("limit").replace('_', " ");
                    ("raw", Some(e.model.as_ref().map_or(base.clone(), |m| format!("{base} {m}"))))
                }
            };
            out.push(Window {
                key: format!("scoped.{}.{}.{i}", e.kind.as_deref().unwrap_or("?"), e.model.as_deref().unwrap_or("?")),
                label: label.into(),
                arg,
                used_percent: u,
                resets_at: reset.map(|d| d.to_rfc3339()),
                span_secs: match e.kind.as_deref() {
                    Some("session") => Some(FIVE_HOUR),
                    Some("weekly_all" | "weekly_scoped") => Some(SEVEN_DAY),
                    _ => None,
                },
                not_started: u == 0.0 && reset.is_none(),
                ..Default::default()
            });
        }
        out
    }
}

fn tier_multiplier(tier: &str) -> Option<String> {
    tier.split('_')
        .find(|p| p.len() > 1 && p.ends_with('x') && p[..p.len() - 1].chars().all(|c| c.is_ascii_digit()))
        .map(str::to_string)
}

// ---- credential file ----

#[derive(Debug, Clone, PartialEq)]
pub struct Credential {
    pub token: String,
    /// Unix seconds; `None` never expires.
    pub expires_at: Option<f64>,
    pub subscription_type: Option<String>,
    pub rate_limit_tier: Option<String>,
}

impl Credential {
    pub fn expired(&self, now: f64) -> bool {
        self.expires_at.is_some_and(|e| e <= now + EXPIRY_SKEW)
    }
}

#[derive(Debug, PartialEq)]
pub enum CredFile {
    Found(Credential),
    /// Valid JSON without an account login (`"claudeAiOauth": null`, MCP-only files).
    NoAccountLogin,
    Unreadable,
}

/// Double, Int or numeric String; ≤ 0 = no expiry; > 1e10 is milliseconds.
fn expires_secs(v: &Value) -> Option<f64> {
    let n = http::num(v)?;
    if n <= 0.0 {
        return None;
    }
    Some(if n > 10_000_000_000.0 { n / 1000.0 } else { n })
}

pub fn parse_credential(v: &Value) -> CredFile {
    if !v.is_object() {
        return CredFile::Unreadable;
    }
    let o = &v["claudeAiOauth"];
    if !o.is_object() {
        return CredFile::NoAccountLogin;
    }
    match http::text(&o["accessToken"]) {
        Some(token) => CredFile::Found(Credential {
            token,
            expires_at: expires_secs(&o["expiresAt"]),
            subscription_type: http::text(&o["subscriptionType"]),
            rate_limit_tier: http::text(&o["rateLimitTier"]),
        }),
        None => CredFile::Unreadable,
    }
}

fn read_credential(root: &Path) -> Option<CredFile> {
    let bytes = std::fs::read(root.join(".credentials.json")).ok()?;
    Some(serde_json::from_slice::<Value>(&bytes).map_or(CredFile::Unreadable, |v| parse_credential(&v)))
}

fn now() -> f64 {
    chrono::Utc::now().timestamp_millis() as f64 / 1000.0
}

/// `OAuthAccessTokenCache.accessToken` on Windows: the file always wins (#227 `/login` switches
/// accounts), the in-memory copy is the fallback, and there is no Keychain to prompt.
pub fn access_token(root: &Path, is_default: bool, cache: &mut Option<Credential>, bypass: bool) -> Result<Credential> {
    let file = read_credential(root);
    if let Some(CredFile::Found(c)) = &file {
        if !c.expired(now()) {
            *cache = Some(c.clone());
            return Ok(c.clone());
        }
    }
    if let Some(c) = cache.as_ref().filter(|c| !bypass && !c.expired(now())) {
        return Ok(c.clone());
    }
    Err(match file {
        Some(CredFile::Found(_)) => LimitError::TokenExpired,
        // The diagnosis is skipped for the default folder when CLAUDE_CONFIG_DIR points elsewhere.
        Some(CredFile::NoAccountLogin) if !(is_default && http::env("CLAUDE_CONFIG_DIR").is_some()) => {
            LimitError::ReauthNeeded
        }
        _ => LimitError::NoCredential,
    })
}

fn oauth_get(url: &str, token: &str) -> Result<http::Resp> {
    let req = http::agent(15)
        .get(url)
        .set("Authorization", &format!("Bearer {token}"))
        .set("anthropic-beta", "oauth-2025-04-20");
    http::send(req, Body::Empty)
}

fn usage(token: &str) -> Result<Value> {
    oauth_get(USAGE_URL, token)?.ok_json()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Identity {
    pub email: String,
    pub org: Option<String>,
    pub plan: Option<String>,
}

pub fn parse_profile(v: &Value) -> Option<Identity> {
    Some(Identity {
        email: http::text(&v["account"]["email"])?,
        org: http::text(&v["organization"]["name"]),
        plan: http::text(&v["organization"]["plan_display_name"]),
    })
}

/// Best effort, cached per token (8 entries, cleared when full); failures are not cached.
fn profile(token: &str, cache: &mut HashMap<String, Identity>) -> Option<Identity> {
    if let Some(hit) = cache.get(token) {
        return Some(hit.clone());
    }
    let resp = oauth_get(PROFILE_URL, token).ok().filter(|r| r.status == 200)?;
    let id = parse_profile(&resp.json().ok()?)?;
    if cache.len() >= PROFILE_CACHE_MAX {
        cache.clear();
    }
    cache.insert(token.to_string(), id.clone());
    Some(id)
}

/// Per-folder token cache plus the shared profile cache.
#[derive(Default)]
pub struct OAuthCache {
    pub tokens: HashMap<PathBuf, Credential>,
    pub profiles: HashMap<String, Identity>,
}

/// `OAuthLimitsProvider.fetch`: one retry with a re-read token after 401/403.
pub fn fetch_oauth(root: &Path, is_default: bool, manual: bool, cache: &mut OAuthCache) -> Result<Status> {
    let mut slot = cache.tokens.remove(root);
    let result = (|| {
        let mut cred = access_token(root, is_default, &mut slot, manual)?;
        let body = match usage(&cred.token) {
            Err(e) if e.is_auth_rejection() => {
                slot = None;
                let fresh = access_token(root, is_default, &mut slot, true)?;
                if fresh.token == cred.token {
                    return Err(e);
                }
                cred = fresh;
                usage(&cred.token)?
            }
            r => r?,
        };
        let mut status = parse_usage(&body);
        status.subscription_type = cred.subscription_type.clone();
        status.rate_limit_tier = cred.rate_limit_tier.clone();
        if let Some(id) = profile(&cred.token, &mut cache.profiles) {
            status.email = Some(id.email);
            status.org = id.org;
            status.plan_display_name = id.plan;
        }
        Ok(status)
    })();
    if let Some(c) = slot {
        cache.tokens.insert(root.to_path_buf(), c);
    }
    result
}

// ---- account folders (`ClaudeAccountRoots`) ----

pub fn default_root() -> PathBuf {
    http::home().join(".claude")
}

/// `pathKey`: first 8 hex chars of sha256 of the folder path.
pub fn path_key(root: &Path) -> String {
    let digest = Sha256::digest(root.to_string_lossy().as_bytes());
    digest.iter().take(4).map(|b| format!("{b:02x}")).collect()
}

fn excluded() -> [PathBuf; 2] {
    [default_root(), http::home().join(".config").join("claude")]
}

/// Comma/newline separated folders that exist, minus the default ones, de-duplicated.
pub fn roots_from(raw: &str) -> Vec<PathBuf> {
    let ex = excluded();
    let mut out: Vec<PathBuf> = vec![];
    for part in raw.split([',', '\n', '\r']) {
        if part.trim().is_empty() {
            continue;
        }
        let p = http::expand(part);
        let p = p.components().collect::<PathBuf>();
        if p.is_absolute() && p.is_dir() && !ex.contains(&p) && !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// Saved login of a folder (`.claude.json` `oauthAccount`).
pub fn saved_identity(root: &Path) -> Option<(String, Option<String>)> {
    let v: Value = serde_json::from_slice(&std::fs::read(root.join(".claude.json")).ok()?).ok()?;
    let a = &v["oauthAccount"];
    Some((http::text(&a["emailAddress"])?, http::text(&a["organizationName"])))
}

/// `CLAUDE_CONFIG_DIR`, then `~/.claude-*` / `~/.claude_*` folders with a saved login, then Settings.
pub fn account_roots(setting: &str) -> Vec<PathBuf> {
    let mut out = roots_from(&http::env("CLAUDE_CONFIG_DIR").unwrap_or_default());
    let home = http::home();
    let mut names: Vec<String> = std::fs::read_dir(&home)
        .map(|d| d.flatten().filter_map(|e| e.file_name().into_string().ok()).collect())
        .unwrap_or_default();
    names.sort();
    for name in names.iter().filter(|n| n.starts_with(".claude-") || n.starts_with(".claude_")) {
        let p = home.join(name);
        if p.is_dir() && saved_identity(&p).is_some() && !out.contains(&p) {
            out.push(p);
        }
    }
    for p in roots_from(setting) {
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// `history.jsonl` mtime: when this login last sent a prompt (tracked account "automatic").
pub fn last_prompt(root: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(root.join("history.jsonl")).and_then(|m| m.modified()).ok()
}

/// `~/.claude-work` style label for a folder.
pub fn abbreviate(root: &Path) -> String {
    match root.strip_prefix(http::home()) {
        Ok(rest) => format!("~/{}", rest.to_string_lossy().replace('\\', "/")),
        Err(_) => root.to_string_lossy().into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn credential_formats() {
        let cred = |o: Value| match parse_credential(&json!({ "claudeAiOauth": o })) {
            CredFile::Found(c) => c,
            other => panic!("{other:?}"),
        };
        assert_eq!(cred(json!({"accessToken": "t", "expiresAt": 1_760_000_000_000i64})).expires_at, Some(1_760_000_000.0));
        assert_eq!(cred(json!({"accessToken": "t", "expiresAt": 1_760_000_000})).expires_at, Some(1_760_000_000.0));
        assert_eq!(cred(json!({"accessToken": "t", "expiresAt": "1760000000000"})).expires_at, Some(1_760_000_000.0));
        assert_eq!(cred(json!({"accessToken": "t", "expiresAt": 0})).expires_at, None);
        let c = cred(json!({"accessToken": "t", "subscriptionType": "max", "rateLimitTier": "default_claude_max_20x"}));
        assert!(!c.expired(1e12));
        assert_eq!(parse_credential(&json!({ "claudeAiOauth": null })), CredFile::NoAccountLogin);
        assert_eq!(parse_credential(&json!({ "mcpOAuth": {} })), CredFile::NoAccountLogin);
        assert_eq!(parse_credential(&json!([1])), CredFile::Unreadable);
        let near = Credential { token: "t".into(), expires_at: Some(1000.0), subscription_type: None, rate_limit_tier: None };
        assert!(near.expired(941.0) && !near.expired(939.0));
    }

    #[test]
    fn plan_display_rules() {
        let plan = |sub: Option<&str>, tier: Option<&str>, name: Option<&str>| {
            Status {
                subscription_type: sub.map(Into::into),
                rate_limit_tier: tier.map(Into::into),
                plan_display_name: name.map(Into::into),
                ..Default::default()
            }
            .plan_display()
        };
        assert_eq!(plan(Some("max"), Some("default_claude_max_20x"), None).as_deref(), Some("Ultra"));
        assert_eq!(plan(Some("max"), Some("default_claude_max_5x"), None).as_deref(), Some("Max"));
        assert_eq!(plan(Some("max"), Some("x_10x"), None).as_deref(), Some("Max 10x"));
        assert_eq!(plan(Some("pro"), Some("tier_2x"), None).as_deref(), Some("Pro 2x"));
        assert_eq!(plan(Some("pro"), Some("default"), None).as_deref(), Some("Pro"));
        assert_eq!(plan(Some("max"), None, Some("  Max 20x ")).as_deref(), Some("Max 20x"));
        assert_eq!(plan(None, Some("default_claude_max_20x"), None), None);
    }

    #[test]
    fn account_display_and_title() {
        let s = Status { email: Some("a@b.com".into()), org: Some("a@b.com's Organization".into()), ..Default::default() };
        assert_eq!(s.account_display().as_deref(), Some("a@b.com"));
        assert_eq!(s.title("~/.claude"), "a@b.com");
        let s = Status { email: Some("a@b.com".into()), org: Some("Acme".into()), ..Default::default() };
        assert_eq!(s.account_display().as_deref(), Some("a@b.com · Acme"));
        assert_eq!(s.title("~/.claude"), "Acme");
        assert_eq!(Status::default().title("~/.claude"), "~/.claude");
    }

    #[test]
    fn usage_rows_and_scoped_entries() {
        let body = json!({
            "five_hour": {"utilization": 62.0, "resets_at": "2026-10-10T11:10:00.123456+00:00"},
            "seven_day": {"utilization": 0.0, "resets_at": null},
            "seven_day_opus": null,
            "limits": [
                {"kind": "session", "percent": 62},
                {"kind": "weekly_all", "percent": 1},
                {"kind": "weekly_scoped", "percent": 40, "resets_at": "2026-10-12T00:00:00Z", "scope": {"model": {"display_name": "Opus"}}},
                {"kind": "weekly_scoped", "percent": 3},
                {"kind": "daily_extra", "percent": 5}
            ]
        });
        let s = parse_usage(&body);
        let w = s.windows();
        let keys: Vec<_> = w.iter().map(|w| (w.key.as_str(), w.label.as_str(), w.arg.as_deref())).collect();
        assert_eq!(keys, [
            ("5h", "fiveHour", None),
            ("7d", "weekly", None),
            ("scoped.weekly_scoped.Opus.0", "scoped", Some("Opus")),
            ("scoped.weekly_scoped.?.1", "scoped", None),
            ("scoped.daily_extra.?.2", "raw", Some("daily extra")),
        ]);
        assert_eq!(w[0].resets_at.as_deref(), Some("2026-10-10T11:10:00.123456+00:00"));
        assert!(w[1].not_started && !w[0].not_started);
        assert_eq!(w[2].span_secs, Some(SEVEN_DAY));
        assert_eq!(w[4].span_secs, None);
        assert!(s.has_usage_data());

        // Weekly-only plans (Ultra) send no five_hour: every limits[] entry shows.
        let weekly_only = parse_usage(&json!({"five_hour": null, "seven_day": null, "limits": [{"kind": "session", "percent": 9}]}));
        assert_eq!(weekly_only.windows()[0].label, "fiveHour");
        assert!(!parse_usage(&json!({"five_hour": {"utilization": 0.0}})).has_usage_data());
    }

    #[test]
    fn profile_needs_an_email() {
        let p = parse_profile(&json!({"account": {"email": "a@b.com"}, "organization": {"name": " ", "plan_display_name": "Max"}})).unwrap();
        assert_eq!((p.org, p.plan.as_deref()), (None, Some("Max")));
        assert!(parse_profile(&json!({"account": {"email": ""}})).is_none());
    }

    #[test]
    fn path_key_is_eight_hex_chars() {
        let k = path_key(Path::new("/Users/me/.claude-work"));
        assert_eq!(k.len(), 8);
        assert!(k.chars().all(|c| c.is_ascii_hexdigit()));
    }
}
