//! Save transfer (envelope export/import, pre-import backups ×5) and local snapshots ×10
//! (pet-settings-misc.md §3.4-3.5, SaveTransfer.swift, SaveSnapshots.swift).
//!
//! The state inside the envelope is the Windows `companion.json` object, handled as JSON so this area
//! never depends on companion's field layout beyond the few keys named here.

use chrono::{DateTime, Local, NaiveDateTime, SecondsFormat, TimeZone, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Distinct from macOS's `poketokenbar.save`: the Windows state schema differs, so a cross-platform
/// file is refused as "not a save file" instead of silently importing an empty state (§8 Q1).
pub const FORMAT: &str = "poketokenbar.windows.save";
pub const SCHEMA: u64 = 2;
pub const MAX_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_BACKUPS: usize = 5;
pub const MAX_SNAPSHOTS: usize = 10;
pub const AUTO_SNAPSHOT_AGE_SECS: i64 = 12 * 3600;
const TOKEN_CAP: u64 = 1_000_000_000_000_000_000;
const SNAPSHOT_PREFIX: &str = "companion-snapshot-";
const STAMP: &str = "%Y-%m-%d-%H%M%S";

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SaveError {
    NotASaveFile,
    NewerSchema,
    FileTooLarge,
    BackupFailed,
    Io,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Envelope {
    pub app_version: String,
    pub exported_at: DateTime<Utc>,
    pub source_device: String,
    pub state: Value,
}

/// Pretty-printed with sorted keys (serde_json's default map is ordered).
pub fn encode(state: &Value, app_version: &str, device: &str, now: DateTime<Utc>) -> Vec<u8> {
    let v = json!({
        "format": FORMAT,
        "schema": SCHEMA,
        "appVersion": app_version,
        "exportedAt": now.to_rfc3339_opts(SecondsFormat::Secs, true),
        "sourceDevice": device,
        "state": state,
    });
    serde_json::to_vec_pretty(&v).unwrap_or_default()
}

/// `valid` checks that the state decodes as the app's state type.
pub fn decode(bytes: &[u8], valid: impl Fn(&Value) -> bool) -> Result<Envelope, SaveError> {
    if bytes.len() > MAX_BYTES {
        return Err(SaveError::FileTooLarge);
    }
    let v: Value = serde_json::from_slice(bytes).map_err(|_| SaveError::NotASaveFile)?;
    if v.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(SaveError::NotASaveFile);
    }
    let schema = v.get("schema").and_then(Value::as_u64).ok_or(SaveError::NotASaveFile)?;
    if schema > SCHEMA {
        return Err(SaveError::NewerSchema);
    }
    let text = |k: &str| v.get(k).and_then(Value::as_str).map(String::from);
    let exported_at = text("exportedAt")
        .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
        .ok_or(SaveError::NotASaveFile)?
        .with_timezone(&Utc);
    let mut state = v.get("state").filter(|s| s.is_object() && valid(s)).cloned().ok_or(SaveError::NotASaveFile)?;
    sanitize(&mut state);
    Ok(Envelope {
        app_version: text("appVersion").unwrap_or_default(),
        exported_at,
        source_device: text("sourceDevice").unwrap_or_default(),
        state,
    })
}

fn clamp_u64(v: &mut Value) {
    if let Some(n) = v.as_f64() {
        *v = json!((n.max(0.0) as u64).min(TOKEN_CAP));
    }
}

fn dedupe(v: Option<&mut Value>) {
    if let Some(Value::Array(a)) = v {
        let mut seen = vec![];
        a.retain(|x| if seen.contains(x) { false } else { seen.push(x.clone()); true });
    }
}

/// Clamps token counters, keeps the active Pokémon's path consistent and dedupes dex chains.
pub fn sanitize(state: &mut Value) {
    let Some(s) = state.as_object_mut() else { return };
    for k in ["used_since_install", "spent_tokens", "egg_usage"] {
        if let Some(v) = s.get_mut(k) {
            clamp_u64(v);
        }
    }
    if let Some(Value::Object(ledger)) = s.get_mut("ledger") {
        ledger.values_mut().for_each(clamp_u64);
    }
    if let Some(Value::Object(m)) = s.get_mut("active") {
        if let Some(v) = m.get_mut("used_at_stage") {
            clamp_u64(v);
        }
        let current = m.get("path_ids").and_then(Value::as_array).and_then(|p| {
            let i = m.get("stage_index").and_then(Value::as_u64).unwrap_or(0) as usize;
            p.get(i.min(p.len().saturating_sub(1))).cloned()
        });
        dedupe(m.get_mut("path_ids"));
        dedupe(m.get_mut("planned_ids"));
        let path = m.get("path_ids").and_then(Value::as_array).cloned().unwrap_or_default();
        let index = current.and_then(|c| path.iter().position(|p| *p == c)).unwrap_or(0);
        m.insert("stage_index".into(), json!(index));
    }
    if let Some(Value::Array(dex)) = s.get_mut("dex") {
        dex.iter_mut().for_each(|d| dedupe(d.get_mut("chain")));
    }
}

/// Crediting restarts from today's usage so imported progress never back-fills (SaveTransfer :211).
/// The growth multiplier is a setting of this PC, so the current one is kept.
pub fn rebase(incoming: &mut Value, current: &Value, today: &HashMap<String, u64>, date: &str, has_usage: bool) {
    let Some(s) = incoming.as_object_mut() else { return };
    if has_usage && !today.is_empty() {
        s.insert("baseline_set".into(), json!(true));
        s.insert("ledger".into(), json!(today));
        s.insert("last_date".into(), json!(date));
    } else {
        s.insert("baseline_set".into(), json!(false));
        s.insert("ledger".into(), json!({}));
        s.insert("last_date".into(), json!(""));
    }
    match current.get("growth_difficulty") {
        Some(d) => s.insert("growth_difficulty".into(), d.clone()),
        None => s.remove("growth_difficulty"),
    };
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Summary {
    pub dex: usize,
    pub lifetime: u64,
    /// Current species of the active Pokémon; `None` = egg.
    pub species: Option<u64>,
    pub shiny: bool,
}

pub fn summary(state: &Value) -> Summary {
    let active = state.get("active").filter(|a| a.is_object());
    Summary {
        dex: state.get("dex").and_then(Value::as_array).map_or(0, Vec::len),
        lifetime: state.get("used_since_install").and_then(Value::as_u64).unwrap_or(0),
        species: active.and_then(|a| {
            let i = a.get("stage_index")?.as_u64()? as usize;
            a.get("path_ids")?.as_array()?.get(i)?.as_u64()
        }),
        shiny: active.and_then(|a| a.get("is_shiny")?.as_bool()).unwrap_or(false),
    }
}

/// Worth snapshotting: some growth, a dex entry or a Pokémon.
pub fn has_progress(state: &Value) -> bool {
    let s = summary(state);
    s.lifetime > 0 || s.dex > 0 || s.species.is_some()
}

fn stamp(now: DateTime<Local>) -> String {
    now.format(STAMP).to_string()
}

fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, bytes)?;
    fs::rename(&tmp, path)
}

/// Sorted file names in `dir` with this prefix and a `.json` suffix.
fn matching(dir: &Path, prefix: &str) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.starts_with(prefix) && n.ends_with(".json"))
        .collect();
    names.sort();
    names
}

/// `<stem>.pre-import-<stamp>.json` next to the state file; the 5 newest are kept.
pub fn backup(state_file: &Path, bytes: &[u8], now: DateTime<Local>) -> io::Result<PathBuf> {
    let dir = state_file.parent().unwrap_or(Path::new("."));
    let stem = state_file.file_stem().and_then(|s| s.to_str()).unwrap_or("companion");
    let prefix = format!("{stem}.pre-import-");
    let path = dir.join(format!("{prefix}{}.json", stamp(now)));
    write_atomic(&path, bytes)?;
    let names = matching(dir, &prefix);
    for old in &names[..names.len().saturating_sub(MAX_BACKUPS)] {
        let _ = fs::remove_file(dir.join(old));
    }
    Ok(path)
}

/// `<stateDir>/.snapshots/<state file name>/`
pub fn snapshot_dir(state_file: &Path) -> PathBuf {
    let dir = state_file.parent().unwrap_or(Path::new("."));
    dir.join(".snapshots").join(state_file.file_name().unwrap_or_default())
}

/// Writes `companion-snapshot-<stamp>[-n].json`, then keeps the 10 newest.
pub fn create_snapshot(dir: &Path, envelope: &[u8], now: DateTime<Local>) -> io::Result<PathBuf> {
    fs::create_dir_all(dir)?;
    let base = format!("{SNAPSHOT_PREFIX}{}", stamp(now));
    let mut path = dir.join(format!("{base}.json"));
    let mut n = 0;
    while path.exists() {
        n += 1;
        path = dir.join(format!("{base}-{n}.json"));
    }
    write_atomic(&path, envelope)?;
    for s in list_snapshots(dir, |_| true).iter().skip(MAX_SNAPSHOTS) {
        let _ = fs::remove_file(dir.join(&s.id));
    }
    Ok(path)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Snapshot {
    /// File name.
    pub id: String,
    /// RFC 3339.
    pub date: String,
    #[serde(flatten)]
    pub summary: Summary,
    #[serde(skip)]
    pub at: DateTime<Utc>,
}

/// Date from `companion-snapshot-yyyy-MM-dd-HHmmss[-n].json` (local time).
fn stamp_date(name: &str) -> Option<DateTime<Utc>> {
    let s = name.strip_prefix(SNAPSHOT_PREFIX)?.strip_suffix(".json")?;
    let s = s.get(..17)?;
    let naive = NaiveDateTime::parse_from_str(s, STAMP).ok()?;
    Local.from_local_datetime(&naive).single().map(|d| d.with_timezone(&Utc))
}

/// Envelopes and legacy raw states, newest first (date, then file name). Unreadable files are skipped.
pub fn list_snapshots(dir: &Path, valid: impl Fn(&Value) -> bool) -> Vec<Snapshot> {
    let mut out: Vec<Snapshot> = matching(dir, SNAPSHOT_PREFIX)
        .into_iter()
        .filter_map(|id| {
            let bytes = fs::read(dir.join(&id)).ok()?;
            let (state, at) = match decode(&bytes, &valid) {
                Ok(e) => (e.state, e.exported_at),
                Err(_) => {
                    let v: Value = serde_json::from_slice(&bytes).ok()?;
                    if !v.is_object() || v.get("format").is_some() || !valid(&v) {
                        return None;
                    }
                    let created = fs::metadata(dir.join(&id)).and_then(|m| m.created()).ok().map(DateTime::<Utc>::from);
                    (v, stamp_date(&id).or(created).unwrap_or_else(Utc::now))
                }
            };
            Some(Snapshot { summary: summary(&state), date: at.to_rfc3339(), at, id })
        })
        .collect();
    out.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| b.id.cmp(&a.id)));
    out
}

/// Loads one snapshot's state (validated), by file name inside `dir`.
pub fn load_snapshot(dir: &Path, id: &str, valid: impl Fn(&Value) -> bool) -> Result<Value, SaveError> {
    if id.contains(['/', '\\']) || !id.starts_with(SNAPSHOT_PREFIX) {
        return Err(SaveError::NotASaveFile);
    }
    let bytes = fs::read(dir.join(id)).map_err(|_| SaveError::Io)?;
    match decode(&bytes, &valid) {
        Ok(e) => Ok(e.state),
        Err(SaveError::NotASaveFile) => {
            let mut v: Value = serde_json::from_slice(&bytes).map_err(|_| SaveError::NotASaveFile)?;
            if !v.is_object() || !valid(&v) {
                return Err(SaveError::NotASaveFile);
            }
            sanitize(&mut v);
            Ok(v)
        }
        Err(e) => Err(e),
    }
}

/// Export file name: `PokeTokenBar-Save-yyyy-MM-dd.json`.
pub fn export_name(now: DateTime<Local>) -> String {
    format!("PokeTokenBar-Save-{}.json", now.format("%Y-%m-%d"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> Value {
        json!({
            "baseline_set": true, "ledger": {"codex": 5}, "last_date": "2026-01-01",
            "used_since_install": 1234, "egg_usage": -5, "growth_difficulty": 0.5,
            "active": {"path_ids": [1, 1, 2], "planned_ids": [1, 2, 2], "stage_index": 2, "is_shiny": true, "used_at_stage": 9},
            "dex": [{"chain": [4, 5, 5]}],
        })
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ptb-save-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn round_trips_and_rejects_bad_files() {
        let now = Utc.with_ymd_and_hms(2026, 11, 12, 8, 0, 0).unwrap();
        let bytes = encode(&state(), "0.3.0", "PC", now);
        let text = String::from_utf8(bytes.clone()).unwrap();
        assert!(text.contains("\"exportedAt\": \"2026-11-12T08:00:00Z\""));
        assert!(text.find("\"appVersion\"").unwrap() < text.find("\"exportedAt\"").unwrap());
        let e = decode(&bytes, |_| true).unwrap();
        assert_eq!((e.app_version.as_str(), e.source_device.as_str(), e.exported_at), ("0.3.0", "PC", now));
        // sanitized: clamped, deduped, stage relocated to the current id (2).
        assert_eq!(e.state["egg_usage"], 0);
        assert_eq!(e.state["active"]["path_ids"], json!([1, 2]));
        assert_eq!(e.state["active"]["planned_ids"], json!([1, 2]));
        assert_eq!(e.state["active"]["stage_index"], 1);
        assert_eq!(e.state["dex"][0]["chain"], json!([4, 5]));

        assert_eq!(decode(b"{}", |_| true), Err(SaveError::NotASaveFile));
        assert_eq!(decode(b"nope", |_| true), Err(SaveError::NotASaveFile));
        let newer = text.replace("\"schema\": 2", "\"schema\": 3");
        assert_eq!(decode(newer.as_bytes(), |_| true), Err(SaveError::NewerSchema));
        let mac = text.replace(FORMAT, "poketokenbar.save");
        assert_eq!(decode(mac.as_bytes(), |_| true), Err(SaveError::NotASaveFile));
        assert_eq!(decode(&bytes, |_| false), Err(SaveError::NotASaveFile));
        assert_eq!(decode(&vec![b' '; MAX_BYTES + 1], |_| true), Err(SaveError::FileTooLarge));
    }

    #[test]
    fn rebase_restarts_crediting_and_keeps_local_difficulty() {
        let mut s = state();
        let today = HashMap::from([("claude_code".to_string(), 77u64)]);
        rebase(&mut s, &json!({"growth_difficulty": 1.5}), &today, "2026-11-12", true);
        assert_eq!((s["baseline_set"].clone(), s["ledger"]["claude_code"].clone()), (json!(true), json!(77)));
        assert_eq!((s["last_date"].clone(), s["growth_difficulty"].clone()), (json!("2026-11-12"), json!(1.5)));
        rebase(&mut s, &json!({}), &today, "2026-11-12", false);
        assert_eq!((s["baseline_set"].clone(), s["ledger"].clone(), s["last_date"].clone()), (json!(false), json!({}), json!("")));
        assert!(s.get("growth_difficulty").is_none());
        let sum = summary(&state());
        assert_eq!(sum, Summary { dex: 1, lifetime: 1234, species: Some(2), shiny: true });
        assert!(!has_progress(&json!({})));
    }

    #[test]
    fn backups_keep_five_and_snapshots_keep_ten() {
        let dir = tmp("files");
        let file = dir.join("companion.json");
        let t = |s: u32| Local.with_ymd_and_hms(2026, 1, 1, 0, 0, s).unwrap();
        for s in 0..7 {
            backup(&file, b"{}", t(s)).unwrap();
        }
        let names = matching(&dir, "companion.pre-import-");
        assert_eq!(names.len(), 5);
        assert_eq!(names[0], "companion.pre-import-2026-01-01-000002.json");

        let snaps = snapshot_dir(&file);
        assert!(snaps.ends_with(".snapshots/companion.json"));
        let env = encode(&state(), "1", "PC", Utc.with_ymd_and_hms(2026, 1, 2, 0, 0, 0).unwrap());
        let a = create_snapshot(&snaps, &env, t(0)).unwrap();
        let b = create_snapshot(&snaps, &env, t(0)).unwrap();
        assert!(a.ends_with("companion-snapshot-2026-01-01-000000.json"));
        assert!(b.ends_with("companion-snapshot-2026-01-01-000000-1.json"));
        // A legacy raw state is listed with its stamp date; junk is skipped.
        fs::write(snaps.join("companion-snapshot-2020-01-01-000000.json"), serde_json::to_vec(&state()).unwrap()).unwrap();
        fs::write(snaps.join("companion-snapshot-junk.json"), b"[]").unwrap();
        let list = list_snapshots(&snaps, |_| true);
        assert_eq!(list.len(), 3);
        // Same date: file name descending ("…000000.json" sorts after "…000000-1.json").
        assert_eq!(list[0].id, "companion-snapshot-2026-01-01-000000.json");
        assert_eq!(list[2].id, "companion-snapshot-2020-01-01-000000.json");
        assert_eq!(list[2].at, stamp_date(&list[2].id).unwrap());
        for s in 1..12 {
            create_snapshot(&snaps, &env, t(s)).unwrap();
        }
        assert_eq!(list_snapshots(&snaps, |_| true).len(), MAX_SNAPSHOTS);
        assert!(load_snapshot(&snaps, "../companion.json", |_| true).is_err());
        let newest = &list_snapshots(&snaps, |_| true)[0];
        assert_eq!(summary(&load_snapshot(&snaps, &newest.id, |_| true).unwrap()).dex, 1);
        assert_eq!(export_name(t(0)), "PokeTokenBar-Save-2026-01-01.json");
        let _ = fs::remove_dir_all(&dir);
    }
}
