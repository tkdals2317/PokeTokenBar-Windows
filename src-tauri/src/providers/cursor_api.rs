//! Cursor dashboard usage API (unofficial; providers.md §4.7 A, CursorUsageAPI.swift). When it
//! answers it is authoritative and replaces local bubbles. Disk cache keeps the last good answer.

use super::entry::*;
use super::roots::env;
use super::sqlite::{cursor_auth_value, make_entry};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const URL: &str = "https://cursor.com/api/dashboard/get-filtered-usage-events";
const TOKEN_URL: &str = "https://api2.cursor.sh/oauth/token";
const CLIENT_ID: &str = "KbZUR41cY7W6zRSdpSUJ7I7mLYBKOCmB";
pub const PAGE_SIZE: u64 = 100;
const MAX_PAGES: u64 = 200;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const DEADLINE: Duration = Duration::from_secs(120);
const CACHE_MAX_AGE: f64 = 6.0 * 3600.0;

pub struct ApiResult {
    pub entries: Vec<Entry>,
    pub authoritative: bool,
}

#[derive(Serialize, Deserialize, Clone)]
struct DiskCache {
    fetched_at: Ts,
    account_identifier: Option<String>,
    covered_since: Option<Ts>,
    entries: Vec<Entry>,
}

#[derive(Clone, PartialEq)]
struct Credential {
    /// `None` = env override (not refreshable).
    root: Option<PathBuf>,
    access: String,
    refresh: Option<String>,
}

static REFRESHED: Mutex<Option<Credential>> = Mutex::new(None);
static MEMORY: Mutex<Option<DiskCache>> = Mutex::new(None);

pub fn enabled() -> bool {
    env("CURSOR_USAGE_API").as_deref() != Some("0")
}

fn selected(roots: &[PathBuf]) -> Option<Credential> {
    if let Some(t) = env("CURSOR_SESSION_TOKEN").map(|t| t.trim().to_string()).filter(|t| !t.is_empty()) {
        return Some(Credential { root: None, access: t, refresh: None });
    }
    roots.iter().find_map(|root| {
        let one = std::slice::from_ref(root);
        let access = cursor_auth_value("cursorAuth/accessToken", one)?;
        // Never combine Stable's access token with Nightly's refresh token.
        Some(Credential { root: Some(root.clone()), access, refresh: cursor_auth_value("cursorAuth/refreshToken", one) })
    })
}

fn current(roots: &[PathBuf]) -> Option<Credential> {
    let source = selected(roots);
    let mut r = REFRESHED.lock().unwrap();
    let Some(source) = source else {
        *r = None;
        return None;
    };
    if let Some(c) = r.as_ref().filter(|c| c.root == source.root) {
        return Some(c.clone());
    }
    *r = None;
    Some(source)
}

fn refresh(rejected: &Credential, roots: &[PathBuf]) -> Option<Credential> {
    rejected.root.as_ref()?;
    let cur = current(roots).filter(|c| c.root == rejected.root)?;
    if cur.access != rejected.access {
        return Some(cur);
    }
    let rt = cur.refresh.clone()?;
    let resp = ureq::post(TOKEN_URL)
        .timeout(REQUEST_TIMEOUT)
        .send_json(json!({"grant_type": "refresh_token", "client_id": CLIENT_ID, "refresh_token": rt}))
        .ok()?;
    let o: Value = resp.into_json().ok()?;
    if bool_value(o.get("shouldLogout")) {
        return None;
    }
    let access = non_empty(o.get("access_token"))?;
    let c = Credential { root: rejected.root.clone(), access, refresh: non_empty(o.get("refresh_token")).or(Some(rt)) };
    if selected(roots).map(|s| s.root) != Some(rejected.root.clone()) {
        return None;
    }
    *REFRESHED.lock().unwrap() = Some(c.clone());
    Some(c)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            let hex = std::str::from_utf8(&b[i + 1..i + 3]).ok();
            if let Some(v) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}

fn b64url(s: &str) -> Option<Vec<u8>> {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let (mut buf, mut bits) = (0u32, 0);
    for c in s.bytes().filter(|c| *c != b'=') {
        let c = match c {
            b'-' => b'+',
            b'_' => b'/',
            c => c,
        };
        buf = buf << 6 | T.iter().position(|&x| x == c)? as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}

pub fn jwt_subject(jwt: &str) -> Option<String> {
    let payload = jwt.split('.').nth(1)?;
    let v: Value = serde_json::from_slice(&b64url(payload)?).ok()?;
    non_empty(v.get("sub"))
}

pub fn account_identifier(token: &str) -> String {
    let d = percent_decode(token);
    if let Some((sub, _)) = d.split_once("::").filter(|(s, _)| !s.is_empty()) {
        return format!("subject:{sub}");
    }
    if let Some(sub) = jwt_subject(&d) {
        return format!("subject:{sub}");
    }
    format!("token:{:x}", Sha256::digest(d.as_bytes()))
}

/// Dashboard cookie is `sub::jwt`.
pub fn session_cookie(token: &str) -> String {
    let d = percent_decode(token);
    if d.contains("::") {
        return token.to_string();
    }
    jwt_subject(&d).map_or_else(|| token.to_string(), |s| format!("{s}::{d}"))
}

pub fn bearer(token: &str) -> String {
    let d = percent_decode(token);
    d.split_once("::").map_or(d.clone(), |(_, b)| b.to_string())
}

pub fn has_next_page(pagination: Option<&Value>, total: Option<u64>, page: u64, count: usize) -> bool {
    if let Some(b) = pagination.and_then(|p| p.get("hasNextPage")).and_then(Value::as_bool) {
        return b;
    }
    if let Some(n) = pagination.and_then(|p| p.get("numPages")).and_then(Value::as_u64) {
        return page < n;
    }
    if let Some(t) = total {
        return page * PAGE_SIZE < t;
    }
    count as u64 >= PAGE_SIZE
}

fn epoch_date(v: f64) -> Option<Ts> {
    if v > 1e12 {
        Some(v / 1000.0)
    } else if v >= 1e9 {
        Some(v)
    } else {
        None
    }
}

fn stringish(v: Option<&Value>) -> Option<String> {
    match v? {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

pub fn event_date(e: &Value) -> Option<Ts> {
    if let Some(raw) = stringish(e.get("timestamp")).filter(|_| e.get("timestamp").is_some_and(Value::is_string)) {
        return match raw.parse::<f64>() {
            Ok(n) => epoch_date(n).or_else(|| parse_iso(&raw)),
            Err(_) => parse_iso(&raw),
        };
    }
    e.get("timestamp").and_then(Value::as_f64).and_then(epoch_date)
}

pub fn parse_event(e: &Value, row: usize, since: Ts) -> Option<Entry> {
    let date = event_date(e).filter(|d| *d >= since)?;
    let model = stringish(e.get("model")).unwrap_or_else(|| "unknown".into());
    let stable = stringish(e.get("id")).or_else(|| stringish(e.get("eventId"))).or_else(|| stringish(e.get("requestId")));
    let u = e.get("tokenUsage").cloned().unwrap_or(Value::Null);
    let stamp = stringish(e.get("timestamp")).unwrap_or_else(|| rfc3339(date));
    let id = stable.map_or_else(|| format!("cursor|api|{stamp}|{model}|{row}"), |s| format!("cursor|api|{s}"));
    make_entry(
        id,
        date,
        &model,
        [
            loose_int(u.get("inputTokens")),
            loose_int(u.get("outputTokens")),
            loose_int(u.get("cacheWriteTokens")),
            loose_int(u.get("cacheReadTokens")),
        ],
        0,
        loose_double(u.get("totalCents")).map(|c| c / 100.0),
        false,
    )
}

enum Net {
    Ok(Vec<Entry>),
    Fail(String),
    AuthRejected(String),
}

fn fetch(token: &str, since: Ts) -> Net {
    let start = format!("{}", (since * 1000.0).round() as i64);
    let end = format!("{}", (now_ts() * 1000.0).round() as i64);
    let deadline = Instant::now() + DEADLINE;
    let (mut page, mut row, mut bearer_mode) = (1u64, 0usize, false);
    let mut out = Vec::new();
    while page <= MAX_PAGES {
        if Instant::now() >= deadline {
            return Net::Fail(format!("pagination deadline exceeded after page {}", page - 1));
        }
        let mut req = ureq::post(URL)
            .timeout(REQUEST_TIMEOUT)
            .set("Content-Type", "application/json")
            .set("User-Agent", "Mozilla/5.0")
            .set("Origin", "https://cursor.com")
            .set("Referer", "https://cursor.com/dashboard/usage");
        req = if bearer_mode {
            req.set("Authorization", &format!("Bearer {}", bearer(token)))
        } else {
            req.set("Cookie", &format!("WorkosCursorSessionToken={}", session_cookie(token)))
        };
        let body = json!({"teamId": 0, "startDate": start, "endDate": end, "page": page, "pageSize": PAGE_SIZE});
        let resp = match req.send_json(body) {
            Ok(r) => r,
            Err(ureq::Error::Status(s @ (401 | 403), _)) => {
                if bearer_mode {
                    return Net::AuthRejected(format!("auth rejected for all modes (last http {s})"));
                }
                bearer_mode = true;
                continue;
            }
            Err(ureq::Error::Status(s, _)) => return Net::Fail(format!("http {s} on page {page}")),
            Err(_) => return Net::Fail(format!("transport error on page {page}")),
        };
        let Ok(o) = resp.into_json::<Value>() else { return Net::Fail(format!("invalid JSON on page {page}")) };
        let Some(events) = ["usageEventsDisplay", "usageEvents", "events"].iter().find_map(|k| o.get(*k).and_then(Value::as_array))
        else {
            return Net::Fail(format!("missing usageEvents/events on page {page}"));
        };
        for e in events {
            out.extend(parse_event(e, row, since));
            row += 1;
        }
        let total = o.get("totalUsageEventsCount").and_then(|v| match v {
            Value::String(s) => s.replace(',', "").parse::<u64>().ok(),
            v => v.as_f64().map(clamp_token),
        });
        if !has_next_page(o.get("pagination"), total, page, events.len()) {
            return Net::Ok(dedup_keep_max(out));
        }
        if events.is_empty() {
            return Net::Fail(format!("pagination indicated next page but page {page} was empty"));
        }
        page += 1;
    }
    Net::Fail(format!("pagination exceeded {MAX_PAGES} pages"))
}

fn cached(path: Option<&Path>, account: &str, since: Ts) -> Option<DiskCache> {
    let mut mem = MEMORY.lock().unwrap();
    if mem.is_none() {
        *mem = path.and_then(|p| serde_json::from_slice::<DiskCache>(&std::fs::read(p).ok()?).ok());
    }
    let c = mem.clone().filter(|c| c.account_identifier.as_deref().is_none_or(|a| a == account))?;
    c.covered_since.is_none_or(|s| s <= since).then_some(c)
}

/// The API path. `cache_file` is `None` in tests.
pub fn fetch_entries(roots: &[PathBuf], since: Ts, cache_file: Option<&Path>) -> ApiResult {
    let none = ApiResult { entries: Vec::new(), authoritative: false };
    if !enabled() {
        return none;
    }
    let Some(mut cred) = current(roots) else { return none };
    let mut result = fetch(&cred.access, since);
    if matches!(result, Net::AuthRejected(_)) {
        if let Some(r) = refresh(&cred, roots) {
            cred = r;
            result = fetch(&cred.access, since);
        }
    }
    let account = account_identifier(&cred.access);
    match result {
        Net::Ok(fresh) => {
            let c = DiskCache { fetched_at: now_ts(), account_identifier: Some(account), covered_since: Some(since), entries: fresh.clone() };
            if let Some(p) = cache_file {
                crate::store::save_json(p, &c);
            }
            *MEMORY.lock().unwrap() = Some(c);
            ApiResult { entries: fresh.into_iter().filter(|e| e.date >= since).collect(), authoritative: true }
        }
        Net::Fail(reason) | Net::AuthRejected(reason) => {
            eprintln!("cursor api: fetch failed — {reason}");
            let Some(stale) = cached(cache_file, &account, since) else { return none };
            if now_ts() - stale.fetched_at > CACHE_MAX_AGE {
                return none;
            }
            ApiResult {
                authoritative: stale.covered_since.is_none_or(|s| s <= since),
                entries: stale.entries.into_iter().filter(|e| e.date >= since).collect(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // {"sub":"user_1"}
    const JWT: &str = "h.eyJzdWIiOiJ1c2VyXzEifQ.s";

    #[test]
    fn token_shapes() {
        assert_eq!(jwt_subject(JWT).as_deref(), Some("user_1"));
        assert_eq!(session_cookie(JWT), format!("user_1::{JWT}"));
        assert_eq!(session_cookie("a%3A%3Ab"), "a%3A%3Ab");
        assert_eq!(bearer("a%3A%3Ab"), "b");
        assert_eq!(account_identifier(JWT), "subject:user_1");
        assert!(account_identifier("opaque").starts_with("token:"));
    }

    #[test]
    fn pagination_rules() {
        assert!(!has_next_page(Some(&json!({"hasNextPage": false, "numPages": 9})), None, 1, 100));
        assert!(has_next_page(Some(&json!({"numPages": 2})), None, 1, 0));
        assert!(!has_next_page(None, Some(150), 2, 50));
        assert!(has_next_page(None, None, 1, 100));
    }

    #[test]
    fn events() {
        let e = json!({"timestamp": "1700000000000", "model": "gpt-5", "id": "ev1",
            "tokenUsage": {"inputTokens": 3, "outputTokens": "4", "totalCents": 12.5}});
        let x = parse_event(&e, 0, 0.0).unwrap();
        assert_eq!((x.id.as_str(), x.date, x.output, x.explicit_cost), ("cursor|api|ev1", 1_700_000_000.0, 4, Some(0.125)));
        let iso = json!({"timestamp": "2026-01-01T00:00:00Z", "tokenUsage": {"inputTokens": 1}});
        assert!(parse_event(&iso, 7, 0.0).unwrap().id.ends_with("|unknown|7"));
        assert!(parse_event(&json!({"timestamp": 5}), 0, 0.0).is_none());
    }
}
