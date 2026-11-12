//! Antigravity limits (limits.md §3.6): Google Cloud Code `retrieveUserQuotaSummary` with the
//! `agy` OAuth token from its token files or Windows Credential Manager (`gemini:antigravity`).

use super::http::{self, Body, LimitError, Result};
use super::Window;
use base64::Engine;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const ENDPOINTS: [&str; 2] = [
    "https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary",
    "https://cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary",
];
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
// Public installed-app client embedded in `agy`.
const CLIENT_ID: &str = "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com";
const CLIENT_SECRET: &str = "";
const FIVE_HOUR: f64 = 5.0 * 3600.0;
const SEVEN_DAY: f64 = 7.0 * 24.0 * 3600.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Credential {
    pub access: String,
    pub refresh: Option<String>,
    /// Unix seconds.
    pub expires_at: Option<f64>,
}

impl Credential {
    fn expired(&self) -> bool {
        self.expires_at.is_some_and(|e| e <= chrono::Utc::now().timestamp() as f64 + 60.0)
    }
}

/// The three token shapes, after an optional `go-keyring-base64:` prefix.
pub fn parse_credential(raw: &[u8]) -> Option<Credential> {
    let text = String::from_utf8_lossy(raw).trim().to_string();
    let json_bytes = match text.strip_prefix("go-keyring-base64:") {
        Some(b64) => base64::engine::general_purpose::STANDARD.decode(b64.trim()).ok()?,
        None => text.into_bytes(),
    };
    let v: Value = serde_json::from_slice(&json_bytes).ok()?;
    let expiry = |o: &Value| o["expiry"].as_str().and_then(http::iso).map(|d| d.timestamp() as f64);
    let t = &v["token"];
    if let Some(access) = http::text(&t["access_token"]) {
        return Some(Credential { access, refresh: http::text(&t["refresh_token"]), expires_at: expiry(t) });
    }
    if let Some(access) = http::text(t) {
        return Some(Credential { access, refresh: None, expires_at: None });
    }
    let access = http::text(&v["access_token"])?;
    Some(Credential { access, refresh: http::text(&v["refresh_token"]), expires_at: expiry(&v) })
}

pub fn token_files(state_dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    if let Some(p) = http::env("ANTIGRAVITY_TOKEN_FILE") {
        out.push(http::expand(&p));
    }
    out.push(state_dir.join("antigravity-token.json"));
    let g = http::home().join(".gemini");
    for dir in ["antigravity-cli", "antigravity-ide", "antigravity"] {
        for name in ["antigravity-oauth-token", "jetski-standalone-oauth-token"] {
            out.push(g.join(dir).join(name));
        }
    }
    out.push(g.join("antigravity-oauth-token"));
    out.push(g.join("jetski-standalone-oauth-token"));
    out
}

fn file_credential(state_dir: &Path) -> Option<Credential> {
    token_files(state_dir).iter().find_map(|p| parse_credential(&std::fs::read(p).ok()?))
}

/// go-keyring's Windows target is `service:user` as a generic credential, value stored as UTF-8 bytes.
#[cfg(windows)]
fn vault_credential() -> Option<Credential> {
    let e = keyring::Entry::new_with_target("gemini:antigravity", "gemini", "antigravity").ok()?;
    parse_credential(&e.get_secret().ok()?)
}

#[cfg(not(windows))]
fn vault_credential() -> Option<Credential> {
    None
}

/// Google does not rotate the refresh token, so it is kept.
fn refresh(c: &Credential) -> Result<Option<Credential>> {
    let Some(rt) = c.refresh.as_deref() else { return Ok(None) };
    let form = [("client_id", CLIENT_ID), ("client_secret", CLIENT_SECRET), ("grant_type", "refresh_token"), ("refresh_token", rt)];
    let r = http::send(http::agent(10).post(TOKEN_URL), Body::Form(&form))?;
    match r.status {
        200 => {
            let v = r.json()?;
            let Some(access) = http::text(&v["access_token"]) else { return Ok(None) };
            let ttl = v["expires_in"].as_f64().unwrap_or(3600.0);
            Ok(Some(Credential {
                access,
                refresh: c.refresh.clone(),
                expires_at: Some(chrono::Utc::now().timestamp() as f64 + ttl),
            }))
        }
        429 => Err(LimitError::RateLimited(r.retry_after)),
        s if s >= 500 => Err(LimitError::Http(s)),
        _ => Ok(None),
    }
}

/// A token that may be used now: refreshed when expired or forced.
fn resolve(c: Credential, force: bool, cache: &mut Option<Credential>) -> Result<String> {
    if !force && !c.expired() {
        *cache = Some(c.clone());
        return Ok(c.access);
    }
    *cache = Some(c.clone());
    match refresh(&c) {
        Ok(Some(fresh)) => {
            *cache = Some(fresh.clone());
            Ok(fresh.access)
        }
        Err(e) if c.expired() => Err(e),
        // A still-valid token survives a failed forced refresh; the API's 401 handles revocation.
        _ => Ok(c.access),
    }
}

fn same_source(cached: &Credential, file: &Credential) -> bool {
    cached.access == file.access || (cached.refresh.is_some() && cached.refresh == file.refresh)
}

/// `accessToken(allowKeychainPrompt:bypassCache:)`. Windows has no prompt, so the vault is read
/// on automatic polls too. `Ok(None)`: no login at all (section shows "Load official limits").
pub fn access_token(state_dir: &Path, bypass: bool, cache: &mut Option<Credential>) -> Result<Option<String>> {
    if let Some(file) = file_credential(state_dir) {
        if let Some(c) = cache.as_ref() {
            if !bypass && !c.expired() && file.expired() && same_source(c, &file) {
                return Ok(Some(c.access.clone()));
            }
        }
        return resolve(file, bypass, cache).map(Some);
    }
    if let Some(c) = cache.as_ref().filter(|c| !bypass && !c.expired()) {
        return Ok(Some(c.access.clone()));
    }
    if let Some(c) = vault_credential() {
        return resolve(c, bypass, cache).map(Some);
    }
    match cache.clone() {
        Some(c) if c.refresh.is_some() => resolve(c, true, cache).map(Some),
        _ => Ok(None),
    }
}

fn quota(token: &str) -> Result<Value> {
    let mut urls: Vec<String> = vec![];
    if let Some(base) = http::env("CLOUD_CODE_URL") {
        urls.push(format!("{}/v1internal:retrieveUserQuotaSummary", base.trim_end_matches('/')));
    }
    urls.extend(ENDPOINTS.iter().map(|s| s.to_string()));
    let mut last = LimitError::Network;
    for url in urls {
        let req = http::agent(15)
            .post(&url)
            .set("Authorization", &format!("Bearer {token}"))
            .set("Content-Type", "application/json")
            .set("User-Agent", "antigravity/2.9.1");
        match http::send(req, Body::Json(&json!({}))) {
            Ok(r) if r.status == 200 => return r.json(),
            Ok(r) if r.status == 429 => return Err(LimitError::RateLimited(r.retry_after)),
            Ok(r) if r.status == 401 || r.status == 403 => return Err(LimitError::Http(r.status)),
            Ok(r) => last = LimitError::Http(r.status),
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// `gemini` / `3p` grouping key shared by alerts and candy.
pub fn group_key(name: &str) -> &'static str {
    if name.to_lowercase().contains("gemini") { "gemini" } else { "3p" }
}

/// Groups → rows. A bucket without `remainingFraction` is dropped, not the whole response.
pub fn parse(v: &Value) -> Vec<Window> {
    let mut out = vec![];
    for g in v["groups"].as_array().into_iter().flatten() {
        let Some(group) = http::text(&g["displayName"]) else { continue };
        for b in g["buckets"].as_array().into_iter().flatten() {
            let (Some(id), Some(rem)) = (http::text(&b["bucketId"]), b["remainingFraction"].as_f64()) else { continue };
            let window = b["window"].as_str();
            let (label, span) = if window == Some("5h") || id.contains("5h") {
                ("fiveHour", Some(FIVE_HOUR))
            } else if window == Some("weekly") || id.contains("weekly") {
                ("weekly", Some(SEVEN_DAY))
            } else {
                ("limit", None)
            };
            out.push(Window {
                key: format!("{}.{id}", group_key(&group)),
                label: label.into(),
                group: Some(group.clone()),
                used_percent: ((1.0 - rem) * 100.0).clamp(0.0, 100.0),
                resets_at: b["resetTime"].as_str().and_then(http::iso).map(|d| d.to_rfc3339()),
                span_secs: span,
                ..Default::default()
            });
        }
    }
    out
}

/// A 401/403 invalidates the cache and retries once when re-acquiring gives a different token.
pub fn fetch(state_dir: &Path, manual: bool, cache: &mut Option<Credential>) -> Result<Option<Vec<Window>>> {
    let Some(token) = access_token(state_dir, manual, cache)? else { return Ok(None) };
    match quota(&token) {
        Err(e) if e.is_auth_rejection() => {
            *cache = None;
            match access_token(state_dir, true, cache)? {
                Some(fresh) if fresh != token => Ok(Some(parse(&quota(&fresh)?))),
                _ => Err(e),
            }
        }
        r => Ok(Some(parse(&r?))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_shapes() {
        let nested = br#"{"token":{"access_token":"a","refresh_token":"r","expiry":"2030-01-01T00:00:00Z"}}"#;
        let c = parse_credential(nested).unwrap();
        assert_eq!((c.access.as_str(), c.refresh.as_deref(), c.expires_at), ("a", Some("r"), Some(1_893_456_000.0)));
        assert_eq!(parse_credential(br#"{"token":"b"}"#).unwrap().access, "b");
        assert_eq!(parse_credential(br#"{"access_token":"c"}"#).unwrap().access, "c");
        let b64 = format!("go-keyring-base64:{}", base64::engine::general_purpose::STANDARD.encode(br#"{"token":"d"}"#));
        assert_eq!(parse_credential(b64.as_bytes()).unwrap().access, "d");
        assert!(parse_credential(b"nope").is_none());
    }

    #[test]
    fn groups_and_buckets() {
        let v = json!({"groups": [
            {"displayName": "Gemini Models", "buckets": [
                {"bucketId": "gemini-5h", "displayName": "x", "remainingFraction": 0.25, "resetTime": "2030-01-01T00:00:00Z"},
                {"bucketId": "gemini-weekly", "displayName": "x"},
                {"bucketId": "other", "displayName": "x", "window": "weekly", "remainingFraction": 1.5}
            ]},
            {"displayName": "Claude and GPT models", "buckets": [{"bucketId": "q", "displayName": "x", "remainingFraction": 1.0}]}
        ]});
        let w = parse(&v);
        let rows: Vec<_> = w.iter().map(|w| (w.key.as_str(), w.label.as_str(), w.used_percent)).collect();
        assert_eq!(rows, [("gemini.gemini-5h", "fiveHour", 75.0), ("gemini.other", "weekly", 0.0), ("3p.q", "limit", 0.0)]);
        assert_eq!(w[0].span_secs, Some(FIVE_HOUR));
    }
}
