//! OpenCode Go limits (limits.md §3.8): `opencode.ai/zen/go/v1/usage` with the `/connect` API key.

use super::http::{self, Body, Result};
use super::Window;
use serde_json::Value;
use std::path::{Path, PathBuf};

const URL: &str = "https://opencode.ai/zen/go/v1/usage";
const INTEGRATION: &str = "opencode-go";

pub fn root() -> PathBuf {
    http::env("OPENCODE_DATA_DIR").map(|p| http::expand(&p)).unwrap_or_else(|| http::home().join(".local").join("share").join("opencode"))
}

/// `opencode.db`, else the alphabetically first `opencode-<channel>.db`.
fn database(root: &Path) -> Option<PathBuf> {
    let main = root.join("opencode.db");
    if main.is_file() {
        return Some(main);
    }
    let mut dbs: Vec<PathBuf> = std::fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("opencode-") && n.ends_with(".db")))
        .collect();
    dbs.sort();
    dbs.into_iter().next()
}

/// `{"type":"key"|"api","key":…}`; OAuth credentials carry no API key.
fn key_of(v: &Value) -> Option<String> {
    matches!(v["type"].as_str(), Some("key" | "api")).then(|| http::text(&v["key"])).flatten()
}

pub fn api_key(root: &Path) -> Option<String> {
    if let Some(db) = database(root) {
        let rows = rusqlite::Connection::open_with_flags(&db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).and_then(|c| {
            let mut st = c.prepare("SELECT value FROM credential WHERE integration_id = ?1 AND active = 1 ORDER BY time_updated DESC")?;
            let rows = st.query_map([INTEGRATION], |r| r.get::<_, String>(0))?.flatten().collect::<Vec<_>>();
            Ok(rows)
        });
        let found = rows.unwrap_or_default().iter().find_map(|v| key_of(&serde_json::from_str(v).ok()?));
        if found.is_some() {
            return found;
        }
    }
    let auth: Value = serde_json::from_slice(&std::fs::read(root.join("auth.json")).ok()?).ok()?;
    key_of(&auth[INTEGRATION])
}

/// A reset in the past means the window ended: 0% with no reset (rolling shows "Starts with…").
pub fn parse(v: &Value, now: chrono::DateTime<chrono::Utc>) -> Vec<Window> {
    let u = &v["usage"];
    let rows = [
        ("rolling", "fiveHour", Some(5.0 * 3600.0)),
        ("weekly", "weekly", Some(7.0 * 24.0 * 3600.0)),
        ("monthly", "monthly", None),
    ];
    rows.iter()
        .filter_map(|(key, label, span)| {
            let w = &u[*key];
            let pct = w["percent"].as_f64()?;
            let reset = w["resetsAt"].as_str().and_then(http::iso);
            let ended = reset.is_some_and(|r| r <= now);
            Some(Window {
                key: key.to_string(),
                label: label.to_string(),
                used_percent: if ended { 0.0 } else { pct.max(0.0) },
                resets_at: reset.filter(|_| !ended).map(|d| d.to_rfc3339()),
                span_secs: *span,
                not_started: ended && *key == "rolling",
                ..Default::default()
            })
        })
        .collect()
}

/// `Ok(None)`: no OpenCode Go key (not a subscriber), the section hides.
pub fn fetch() -> Result<Option<Vec<Window>>> {
    let Some(key) = api_key(&root()) else { return Ok(None) };
    let req = http::agent(10).get(URL).set("Authorization", &format!("Bearer {key}")).set("Accept", "application/json");
    let v = http::send(req, Body::Empty)?.ok_json()?;
    Ok(Some(parse(&v, chrono::Utc::now())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn windows_and_ended_resets() {
        let now = http::iso("2030-01-01T00:00:00Z").unwrap();
        let v = json!({"usage": {
            "rolling": {"status": "ok", "percent": 40, "resetsAt": "2029-12-31T23:00:00Z"},
            "weekly": {"percent": 12.5, "resetsAt": "2030-01-03T00:00:00Z"},
            "monthly": "garbage"
        }});
        let w = parse(&v, now);
        assert_eq!(w.len(), 2);
        assert_eq!((w[0].used_percent, w[0].resets_at.clone(), w[0].not_started), (0.0, None, true));
        assert_eq!((w[1].label.as_str(), w[1].used_percent), ("weekly", 12.5));
    }

    #[test]
    fn auth_json_key_types() {
        assert_eq!(key_of(&json!({"type": "api", "key": "k"})).as_deref(), Some("k"));
        assert_eq!(key_of(&json!({"type": "oauth", "key": "k"})), None);
    }
}
