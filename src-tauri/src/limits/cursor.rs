//! Cursor limits (limits.md §3.7): Connect RPC `GetCurrentPeriodUsage` with the Cursor IDE login
//! from `state.vscdb` (or `CURSOR_SESSION_TOKEN`), refreshed in memory on 401/403.

use super::http::{self, Body, LimitError, Result};
use super::Window;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

const USAGE_URL: &str = "https://api2.cursor.sh/aiserver.v1.DashboardService/GetCurrentPeriodUsage";
const TOKEN_URL: &str = "https://api2.cursor.sh/oauth/token";
const CLIENT_ID: &str = "KbZUR41cY7W6zRSdpSUJ7I7mLYBKOCmB";

/// What the Cursor tab shows besides its rows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Status {
    pub windows: Vec<Window>,
    pub message: Option<String>,
    pub remaining_usd: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    /// `None` for `CURSOR_SESSION_TOKEN` (not refreshable).
    pub root: Option<PathBuf>,
    pub access: String,
    pub refresh: Option<String>,
}

/// The refreshed token, kept in memory only and only for the source it came from.
#[derive(Default)]
pub struct Cache {
    refreshed: Option<Source>,
    from: Option<Source>,
}

pub fn roots(custom: Option<&str>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = match http::env("CURSOR_DATA_DIR") {
        Some(dirs) => dirs.split([',', '\n', ';']).filter(|s| !s.trim().is_empty()).map(http::expand).collect(),
        None => {
            let base = http::env("APPDATA").map(PathBuf::from).unwrap_or_else(|| http::home().join("AppData").join("Roaming"));
            ["Cursor", "Cursor Nightly"].iter().map(|n| base.join(n).join("User").join("globalStorage")).collect()
        }
    };
    for p in custom.unwrap_or("").split([',', '\n']).filter(|s| !s.trim().is_empty()).map(http::expand) {
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// `ItemTable` value, read-only, retried while Cursor holds the database busy.
fn item(root: &Path, key: &str) -> Option<String> {
    let db = root.join("state.vscdb");
    if !db.is_file() {
        return None;
    }
    for attempt in 0..6 {
        let read = rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).and_then(|c| {
            c.query_row("SELECT value FROM ItemTable WHERE key = ?1 LIMIT 1", [key], |r| r.get::<_, String>(0))
        });
        match read {
            Ok(v) => return Some(v.trim().to_string()).filter(|v| !v.is_empty()),
            Err(rusqlite::Error::QueryReturnedNoRows) => return None,
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(50 * (attempt + 1))),
        }
    }
    None
}

fn selected(custom: Option<&str>) -> Option<Source> {
    if let Some(token) = http::env("CURSOR_SESSION_TOKEN") {
        return Some(Source { root: None, access: token, refresh: None });
    }
    roots(custom).into_iter().find_map(|root| {
        let access = item(&root, "cursorAuth/accessToken")?;
        // Never pair one root's access token with another root's refresh token.
        let refresh = item(&root, "cursorAuth/refreshToken");
        Some(Source { root: Some(root), access, refresh })
    })
}

/// Percent-decoded; what follows `::` when the token is a `sub::jwt` session cookie.
pub fn bearer(token: &str) -> String {
    let decoded = percent_encoding::percent_decode_str(token).decode_utf8_lossy().to_string();
    match decoded.split_once("::") {
        Some((_, jwt)) => jwt.to_string(),
        None => decoded,
    }
}

fn period_usage(token: &str) -> Result<Value> {
    let req = http::agent(10)
        .post(USAGE_URL)
        .set("Content-Type", "application/json")
        .set("Connect-Protocol-Version", "1")
        .set("Authorization", &format!("Bearer {}", bearer(token)));
    let r = http::send(req, Body::Json(&json!({}))).map_err(|_| LimitError::Http(0))?;
    match r.status {
        200..=299 => r.json(),
        429 => Err(LimitError::RateLimited(None)),
        s => Err(LimitError::Http(s)),
    }
}

fn refresh_token(c: &Source) -> Option<Source> {
    let rt = c.refresh.as_deref()?;
    let body = json!({"grant_type": "refresh_token", "client_id": CLIENT_ID, "refresh_token": rt});
    let r = http::send(http::agent(10).post(TOKEN_URL).set("Content-Type", "application/json"), Body::Json(&body)).ok()?;
    let v = r.json().ok().filter(|_| (200..=299).contains(&r.status))?;
    if v["shouldLogout"] == true {
        return None;
    }
    let access = http::text(&v["access_token"])?;
    Some(Source { root: c.root.clone(), access, refresh: http::text(&v["refresh_token"]).or_else(|| c.refresh.clone()) })
}

/// Epoch ms or s as a string (`> 1e12` ms, `> 1e9` s).
pub fn epoch(raw: &Value) -> Option<chrono::DateTime<chrono::Utc>> {
    let n = http::num(raw)?;
    if n > 1e12 {
        http::epoch_secs(n / 1000.0)
    } else if n > 1e9 {
        http::epoch_secs(n)
    } else {
        None
    }
}

/// `None` when nothing is visible (no allowance and no total percent).
pub fn parse(v: &Value) -> Option<Status> {
    let u = &v["planUsage"];
    let limit = u["limit"].as_f64().filter(|l| *l > 0.0);
    if !u.is_object() || (limit.is_none() && u["totalPercentUsed"].as_f64().is_none()) {
        return None;
    }
    let used = u["totalPercentUsed"]
        .as_f64()
        .or_else(|| Some(u["includedSpend"].as_f64()? / limit? * 100.0));
    let row = |key: &str, label: &str, pct: f64, reset: Option<String>| Window {
        key: key.into(),
        label: label.into(),
        used_percent: pct,
        resets_at: reset,
        ..Default::default()
    };
    let mut windows = vec![];
    if let Some(p) = used {
        windows.push(row("included", "cursorIncluded", p, epoch(&v["billingCycleEnd"]).map(|d| d.to_rfc3339())));
    }
    if let Some(p) = u["autoPercentUsed"].as_f64() {
        windows.push(row("auto", "cursorAuto", p, None));
    }
    if let Some(p) = u["apiPercentUsed"].as_f64() {
        windows.push(row("api", "cursorApi", p, None));
    }
    Some(Status {
        windows,
        message: http::text(&v["displayMessage"]),
        remaining_usd: used.and(u["remaining"].as_f64()).map(|c| c / 100.0),
    })
}

/// `Ok(None)`: disabled (`CURSOR_USAGE_API=0`), not logged in, or nothing to show.
pub fn fetch(custom: Option<&str>, cache: &mut Cache) -> Result<Option<Status>> {
    if http::env("CURSOR_USAGE_API").as_deref() == Some("0") {
        return Ok(None);
    }
    let Some(source) = selected(custom) else {
        *cache = Cache::default();
        return Ok(None);
    };
    if cache.from.as_ref() != Some(&source) {
        *cache = Cache { refreshed: None, from: Some(source.clone()) };
    }
    let current = cache.refreshed.clone().unwrap_or_else(|| source.clone());
    match period_usage(&current.access) {
        Err(e) if e.is_auth_rejection() && current.root.is_some() => {
            let Some(fresh) = refresh_token(&current) else { return Err(e) };
            // A logout or account switch while refreshing must not bring the old login back.
            if selected(custom).as_ref() != Some(&source) {
                return Err(e);
            }
            cache.refreshed = Some(fresh.clone());
            Ok(parse(&period_usage(&fresh.access)?))
        }
        r => Ok(parse(&r?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_usage_rows() {
        let v = json!({"billingCycleEnd": "1893456000000", "displayMessage": "You're at 40%",
            "planUsage": {"includedSpend": 800, "limit": 2000, "remaining": 1200, "autoPercentUsed": 10.5}});
        let s = parse(&v).unwrap();
        let rows: Vec<_> = s.windows.iter().map(|w| (w.label.as_str(), w.used_percent)).collect();
        assert_eq!(rows, [("cursorIncluded", 40.0), ("cursorAuto", 10.5)]);
        assert_eq!(s.windows[0].resets_at.as_deref(), Some("2030-01-01T00:00:00+00:00"));
        assert_eq!((s.message.as_deref(), s.remaining_usd), (Some("You're at 40%"), Some(12.0)));
        assert!(parse(&json!({"planUsage": {"limit": 0}})).is_none());
        assert_eq!(parse(&json!({"planUsage": {"totalPercentUsed": 7.0}})).unwrap().windows[0].used_percent, 7.0);
        assert_eq!(epoch(&json!("1893456000")).unwrap().timestamp(), 1_893_456_000);
        assert!(epoch(&json!("12345")).is_none());
    }

    #[test]
    fn bearer_strips_the_session_subject() {
        assert_eq!(bearer("user_1%3A%3Aeyj.abc"), "eyj.abc");
        assert_eq!(bearer("eyj.abc"), "eyj.abc");
    }
}
