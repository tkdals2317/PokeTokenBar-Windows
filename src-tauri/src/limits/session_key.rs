//! claude.ai session key path (limits.md §3.4): cookie `sessionKey` → organizations → usage.
//! Stored in Windows Credential Manager (`PokeTokenBar` / `session-key[-hash8]`), never echoed back.

use super::claude::{self, Status};
use super::http::{self, Body, LimitError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

const BASE: &str = "https://claude.ai/api";
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Stored {
    pub key: String,
    #[serde(rename = "organizationID")]
    pub organization_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Org {
    pub id: String,
    pub name: String,
}

/// Trimmed; `sk-ant-` prefix, 40…2048 chars, no control characters.
pub fn normalize(raw: &str) -> Result<String> {
    let k = raw.trim();
    let ok = k.starts_with("sk-ant-") && (40..=2048).contains(&k.chars().count()) && !k.chars().any(char::is_control);
    if ok { Ok(k.to_string()) } else { Err(LimitError::SessionKeyMalformed) }
}

// ---- storage ----

/// `session-key` for the default login, `session-key-<pathKey>` for an extra folder.
pub fn account_name(account: Option<&str>) -> String {
    account.map_or("session-key".into(), |k| format!("session-key-{k}"))
}

#[cfg(windows)]
mod vault {
    fn entry(name: &str) -> Option<keyring::Entry> {
        keyring::Entry::new("PokeTokenBar", name).ok()
    }
    pub fn load(_dir: &std::path::Path, name: &str) -> Option<String> {
        entry(name)?.get_password().ok()
    }
    pub fn save(_dir: &std::path::Path, name: &str, value: &str) -> bool {
        entry(name).is_some_and(|e| e.set_password(value).is_ok())
    }
    pub fn delete(_dir: &std::path::Path, name: &str) {
        if let Some(e) = entry(name) {
            let _ = e.delete_credential();
        }
    }
}

/// Dev builds on other systems: an owner-only file in the app data folder (macOS behaviour).
#[cfg(not(windows))]
mod vault {
    use std::path::{Path, PathBuf};
    fn path(dir: &Path, name: &str) -> PathBuf {
        dir.join(format!("{name}.json"))
    }
    pub fn load(dir: &Path, name: &str) -> Option<String> {
        std::fs::read_to_string(path(dir, name)).ok()
    }
    pub fn save(dir: &Path, name: &str, value: &str) -> bool {
        use std::os::unix::fs::PermissionsExt;
        let p = path(dir, name);
        let ok = std::fs::write(&p, value).is_ok();
        let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
        ok
    }
    pub fn delete(dir: &Path, name: &str) {
        let _ = std::fs::remove_file(path(dir, name));
    }
}

pub fn load(dir: &Path, account: Option<&str>) -> Option<Stored> {
    serde_json::from_str::<Stored>(&vault::load(dir, &account_name(account))?).ok().filter(|s| !s.key.is_empty())
}

pub fn save(dir: &Path, account: Option<&str>, s: &Stored) -> bool {
    vault::save(dir, &account_name(account), &serde_json::to_string(s).unwrap_or_default())
}

pub fn delete(dir: &Path, account: Option<&str>) {
    vault::delete(dir, &account_name(account))
}

// ---- claude.ai ----

fn get(path: &str, key: &str) -> Result<http::Resp> {
    let req = http::agent(15)
        .get(&format!("{BASE}{path}"))
        .set("Cookie", &format!("sessionKey={key}"))
        .set("Accept", "application/json")
        .set("User-Agent", USER_AGENT)
        .set("Referer", "https://claude.ai")
        .set("Origin", "https://claude.ai");
    http::send(req, Body::Empty)
}

/// `mapFailure`: a Cloudflare challenge is reported as such, not as a dead key.
fn map(resp: http::Resp) -> Result<Value> {
    if resp.cf_challenge {
        return Err(LimitError::Cloudflare);
    }
    match resp.status {
        200 => resp.json(),
        401 => Err(LimitError::SessionKeyInvalid),
        429 => Err(LimitError::RateLimited(resp.retry_after)),
        s => Err(LimitError::Http(s)),
    }
}

pub fn parse_orgs(v: &Value) -> Result<Vec<Org>> {
    let rows = v.as_array().ok_or(LimitError::Format)?;
    let all: Vec<(Org, bool)> = rows
        .iter()
        .filter_map(|o| {
            let id = http::text(&o["uuid"])?;
            let chat = o["capabilities"].as_array().is_none_or(|c| c.iter().any(|c| c == "chat"));
            Some((Org { name: http::text(&o["name"]).unwrap_or_else(|| id.clone()), id }, chat))
        })
        .collect();
    if all.len() != rows.len() {
        return Err(LimitError::Format);
    }
    let chat: Vec<Org> = all.iter().filter(|(_, c)| *c).map(|(o, _)| o.clone()).collect();
    Ok(if chat.is_empty() { all.into_iter().map(|(o, _)| o).collect() } else { chat })
}

pub fn organizations(key: &str) -> Result<Vec<Org>> {
    let resp = get("/organizations", key)?;
    // A 403 on the list means the key is dead (observed 2026-08-30).
    if resp.status == 403 && !resp.cf_challenge {
        return Err(LimitError::SessionKeyInvalid);
    }
    parse_orgs(&map(resp)?)
}

fn org_usage(key: &str, org: &str) -> Result<Status> {
    map(get(&format!("/organizations/{org}/usage"), key)?).map(|v| claude::parse_usage(&v))
}

/// First org with usage data, else the first org.
fn pick(key: &str, orgs: &[Org]) -> Result<(String, Status)> {
    let results: Vec<(String, Result<Status>)> = std::thread::scope(|s| {
        let handles: Vec<_> = orgs.iter().map(|o| (o.id.clone(), s.spawn(|| org_usage(key, &o.id)))).collect();
        handles.into_iter().map(|(id, h)| (id, h.join().unwrap_or(Err(LimitError::Network)))).collect()
    });
    let mut ok = vec![];
    for (id, r) in results {
        match r {
            Ok(s) => ok.push((id, s)),
            Err(LimitError::Http(_)) => {}
            Err(e) => return Err(e),
        }
    }
    if ok.is_empty() {
        return Err(LimitError::SessionKeyNoOrg);
    }
    let best = ok.iter().position(|(_, s)| s.has_usage_data()).unwrap_or(0);
    Ok(ok.swap_remove(best))
}

/// `SessionKeyLimitsProvider.fetch`. Returns the status and the org to remember when it changed.
pub fn fetch(stored: &Stored) -> Result<(Status, Option<String>)> {
    if let Some(org) = &stored.organization_id {
        match org_usage(&stored.key, org) {
            Ok(s) => return Ok((s, None)),
            Err(LimitError::Http(403)) => {} // org gone: rediscover
            Err(e) => return Err(e),
        }
    }
    let orgs = organizations(&stored.key)?;
    let (id, status) = pick(&stored.key, &orgs)?;
    let changed = (stored.organization_id.as_deref() != Some(id.as_str())).then_some(id);
    Ok((status, changed))
}

/// Save flow: normalize, list orgs, pick (with usage, else first). Returns the stored value and the orgs.
pub fn validate(raw: &str) -> Result<(Stored, Vec<Org>)> {
    let key = normalize(raw)?;
    let orgs = organizations(&key)?;
    let (id, _) = pick(&key, &orgs)?;
    Ok((Stored { key, organization_id: Some(id) }, orgs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn normalizes_keys() {
        let good = format!("sk-ant-sid01-{}", "a".repeat(40));
        assert_eq!(normalize(&format!("  {good}\n")).unwrap(), good);
        assert_eq!(normalize("sk-ant-short"), Err(LimitError::SessionKeyMalformed));
        assert_eq!(normalize(&format!("xx{good}")), Err(LimitError::SessionKeyMalformed));
        assert_eq!(normalize(&format!("{good}\u{7}x")), Err(LimitError::SessionKeyMalformed));
    }

    #[test]
    fn keeps_chat_orgs_or_all() {
        let v = json!([
            {"uuid": "a", "name": "API", "capabilities": ["api"]},
            {"uuid": "b", "name": "Me", "capabilities": ["chat", "claude_pro"]},
            {"uuid": "c"}
        ]);
        let ids: Vec<_> = parse_orgs(&v).unwrap().into_iter().map(|o| o.id).collect();
        assert_eq!(ids, ["b", "c"]);
        let api_only = json!([{"uuid": "a", "capabilities": ["api"]}]);
        assert_eq!(parse_orgs(&api_only).unwrap()[0].name, "a");
        assert_eq!(parse_orgs(&json!({"x": 1})), Err(LimitError::Format));
    }

    #[test]
    fn stored_json_shape() {
        let s = Stored { key: "k".into(), organization_id: None };
        assert_eq!(serde_json::to_string(&s).unwrap(), r#"{"key":"k","organizationID":null}"#);
        assert_eq!(account_name(Some("ab12cd34")), "session-key-ab12cd34");
    }
}
