//! SQLite (and SQLite-adjacent) stores: OpenCode, Hermes, Cursor bubbles, Copilot, Devin, Kiro, Aside
//! (providers.md §4.5-4.7, §4.9-4.10, §4.14, §4.16-4.17). All opens are read-only.

use super::entry::*;
use super::roots::{ext_is, file_name, mtime_ns, read_json, walk_files};
use rusqlite::{types::ValueRef, Connection, OpenFlags, Row, ToSql};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

pub fn open_ro(path: &Path) -> Option<Connection> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_URI).ok()
}

/// Missing file ⇒ `Some([])`. Open/prepare/step failure (BUSY, interrupt) ⇒ `None`: partial rows never count.
pub fn query<T>(db: &Path, sql: &str, params: &[&dyn ToSql], mut row: impl FnMut(&Row) -> Option<T>) -> Option<Vec<T>> {
    if !db.exists() {
        return Some(Vec::new());
    }
    let conn = open_ro(db)?;
    let mut stmt = conn.prepare(sql).ok()?;
    let mut rows = stmt.query(params).ok()?;
    let mut out = Vec::new();
    loop {
        match rows.next() {
            Ok(Some(r)) => out.extend(row(r)),
            Ok(None) => return Some(out),
            Err(_) => return None,
        }
    }
}

pub fn col_text(r: &Row, i: usize) -> Option<String> {
    match r.get_ref(i).ok()? {
        ValueRef::Text(t) | ValueRef::Blob(t) => Some(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Integer(n) => Some(n.to_string()),
        ValueRef::Real(f) => Some(f.to_string()),
        ValueRef::Null => None,
    }
}

fn col_f64(r: &Row, i: usize) -> Option<f64> {
    match r.get_ref(i).ok()? {
        ValueRef::Integer(n) => Some(n as f64),
        ValueRef::Real(f) => Some(f),
        ValueRef::Text(t) => std::str::from_utf8(t).ok()?.trim().parse().ok(),
        _ => None,
    }
}

fn col_i64(r: &Row, i: usize) -> i64 {
    match r.get_ref(i) {
        Ok(ValueRef::Integer(n)) => n,
        Ok(ValueRef::Real(f)) => f as i64,
        Ok(ValueRef::Text(t)) => std::str::from_utf8(t).ok().and_then(|s| s.trim().parse::<f64>().ok()).map_or(0, |f| f as i64),
        _ => 0,
    }
}

/// Clamped `[0, 1e18]` token column.
fn col_tokens(r: &Row, i: usize) -> u64 {
    (col_i64(r, i).max(0) as u64).min(MAX_TOKEN)
}

fn trimmed(s: Option<String>) -> Option<String> {
    s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

fn json(text: &str) -> Option<Value> {
    serde_json::from_str::<Value>(text).ok().filter(Value::is_object)
}

/// `makeEntry` (LocalAdditionalUsageProvider.swift:1694-1722): an unexplained excess of `total` goes to output.
pub fn make_entry(id: String, date: Ts, model: &str, t: [u64; 4], total: u64, cost: Option<f64>, unavailable: bool) -> Option<Entry> {
    let parts = t.iter().sum::<u64>();
    let mut t = t;
    if total > parts {
        t[1] += total - parts;
    }
    if t.iter().sum::<u64>() == 0 {
        return None;
    }
    let mut e = Entry::new(id, date, model, t);
    e.explicit_cost = cost;
    if unavailable || total > parts {
        e.cost_unavailable = Some(true);
    }
    Some(e)
}

/// ISO string or epoch number (seconds / ms).
fn flexible_date(v: Option<&Value>) -> Option<Ts> {
    match v {
        Some(Value::String(s)) => parse_iso(s),
        other => date_value(other),
    }
}

// ---- OpenCode ----

fn opencode_db(root: &Path) -> Option<PathBuf> {
    if ext_is(root, "db") {
        return Some(root.to_path_buf());
    }
    let std = root.join("opencode.db");
    if std.exists() {
        return Some(std);
    }
    let mut cands: Vec<PathBuf> = fs::read_dir(root)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let n = file_name(p);
            n.strip_prefix("opencode-").and_then(|r| r.strip_suffix(".db")).is_some_and(|ch| {
                !ch.is_empty() && ch.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
            })
        })
        .collect();
    cands.sort();
    cands.into_iter().next()
}

pub fn parse_opencode_message(o: &Value, fallback: &str) -> Option<Entry> {
    let tokens = o.get("tokens").filter(|t| t.is_object())?;
    let date = date_value(o.get("time").and_then(|t| t.get("created")))?;
    let model = non_empty(o.get("modelID"))?;
    non_empty(o.get("providerID"))?;
    let cache = tokens.get("cache");
    let id = non_empty(o.get("id")).unwrap_or_else(|| fallback.to_string());
    make_entry(
        format!("opencode|{id}"),
        date,
        &model,
        [
            loose_int(tokens.get("input")),
            loose_int(tokens.get("output")),
            loose_int(cache.and_then(|c| c.get("write"))),
            loose_int(cache.and_then(|c| c.get("read"))),
        ],
        loose_int(tokens.get("total")),
        loose_double(o.get("cost")),
        false,
    )
}

pub fn parse_opencode_v2(o: &Value, fallback: &str, created: Option<f64>) -> Option<Entry> {
    let tokens = o.get("tokens").filter(|t| t.is_object())?;
    let model = o.get("model").filter(|m| m.is_object())?;
    let date = epoch_secs(created?)?;
    let model_id = non_empty(model.get("id"))?;
    non_empty(model.get("providerID"))?;
    let cache = tokens.get("cache");
    make_entry(
        format!("opencode|{fallback}"),
        date,
        &model_id,
        [
            loose_int(tokens.get("input")),
            loose_int(tokens.get("output")) + loose_int(tokens.get("reasoning")),
            loose_int(cache.and_then(|c| c.get("write"))),
            loose_int(cache.and_then(|c| c.get("read"))),
        ],
        0,
        loose_double(o.get("cost")),
        false,
    )
}

pub fn opencode_entries(roots: &[PathBuf], since: Ts) -> Vec<Entry> {
    let cutoff = (since * 1000.0) as i64;
    let mut out = Vec::new();
    for root in roots {
        if let Some(db) = opencode_db(root) {
            let v2 = query(&db, "SELECT id, time_created, data FROM session_message WHERE type = 'assistant' AND time_created >= ?1", &[&cutoff], |r| {
                parse_opencode_v2(&json(&col_text(r, 2)?)?, &col_text(r, 0)?, col_f64(r, 1))
            });
            out.extend(v2.unwrap_or_default());
            let v1 = |r: &Row| parse_opencode_message(&json(&col_text(r, 2)?)?, &col_text(r, 0)?);
            let rows = query(&db, "SELECT id, session_id, data FROM message WHERE time_created >= ?1", &[&cutoff], v1)
                .or_else(|| query(&db, "SELECT id, session_id, data FROM message", &[], v1));
            out.extend(rows.unwrap_or_default());
        }
        walk_files(&root.join("storage").join("message"), &mut |p, meta| {
            if ext_is(p, "json") && super::roots::ns_to_ts(mtime_ns(meta)) >= since {
                let stem = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                if let Some(e) = read_json(p).and_then(|o| parse_opencode_message(&o, &stem)) {
                    out.push(e);
                }
            }
        });
    }
    dedup_keep_max(out.into_iter().filter(|e| e.date >= since).collect())
}

// ---- Hermes ----

pub fn hermes_entries(roots: &[PathBuf], since: Ts) -> Vec<Entry> {
    let sql = "SELECT id, model, billing_provider, started_at, message_count, input_tokens, output_tokens, \
               cache_read_tokens, cache_write_tokens, reasoning_tokens, estimated_cost_usd, actual_cost_usd \
               FROM sessions WHERE model IS NOT NULL AND TRIM(model) != '' AND started_at >= ?1";
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for root in roots {
        let db = if ext_is(root, "db") { root.clone() } else { root.join("state.db") };
        let rows = query(&db, sql, &[&(since as i64)], |r| {
            let id = trimmed(col_text(r, 0))?;
            let model = trimmed(col_text(r, 1))?;
            let date = epoch_secs(col_f64(r, 3)?)?;
            let actual = col_f64(r, 11).filter(|_| !matches!(r.get_ref(11), Ok(ValueRef::Null)));
            let estimated = col_f64(r, 10);
            let mut e = make_entry(
                format!("hermes|{id}"),
                date,
                &model,
                [col_tokens(r, 5), col_tokens(r, 6) + col_tokens(r, 9), col_tokens(r, 8), col_tokens(r, 7)],
                0,
                actual.or(estimated),
                true,
            )?;
            e.cost_is_estimate = Some(actual.is_none());
            Some(e)
        });
        for e in rows.unwrap_or_default().into_iter().filter(|e| e.date >= since) {
            if seen.insert(e.id.clone()) {
                out.push(e);
            }
        }
    }
    out
}

// ---- incremental append-only stores (Cursor, Copilot, Devin) ----

pub struct IncrementalLoad {
    pub entries: Vec<Entry>,
    pub high_water: HashMap<String, i64>,
    pub did_reset: bool,
}

pub struct RowQuery {
    pub sql: String,
    pub bind: i64,
    pub bind_text: Option<String>,
}

pub struct Store<'a> {
    pub db: &'a dyn Fn(&Path) -> PathBuf,
    pub max_sql: &'a str,
    pub rows: &'a dyn Fn(i64, Ts) -> RowQuery,
    pub parse: &'a dyn Fn(&Row, &Path) -> Option<Entry>,
}

fn load_db(db: &Path, since: Ts, after: i64, s: &Store) -> (Vec<Entry>, i64, bool) {
    // A failed MAX is not a shrink: the writer may hold the file.
    let Some(max) = query(db, s.max_sql, &[], |r| Some(col_i64(r, 0))).map(|v| v.first().copied().unwrap_or(0)) else {
        return (Vec::new(), after, false);
    };
    let did_reset = after > 0 && max < after;
    let effective = if did_reset { 0 } else { after };
    let q = (s.rows)(effective, since);
    let rows = match &q.bind_text {
        Some(t) => query(db, &q.sql, &[&q.bind, t], |r| Some((col_i64(r, 0), (s.parse)(r, db)))),
        None if q.sql.contains("?1") => query(db, &q.sql, &[&q.bind], |r| Some((col_i64(r, 0), (s.parse)(r, db)))),
        None => query(db, &q.sql, &[], |r| Some((col_i64(r, 0), (s.parse)(r, db)))),
    };
    let Some(rows) = rows else { return (Vec::new(), after, false) };
    let mut high = 0;
    let mut entries = Vec::new();
    for (id, e) in rows {
        high = high.max(id);
        entries.extend(e);
    }
    (entries, if high == 0 { effective } else { high }, did_reset)
}

fn scan_roots(roots: &[PathBuf], since: Ts, marks: &HashMap<String, i64>, s: &Store) -> IncrementalLoad {
    let mut entries = Vec::new();
    let mut high_water = HashMap::new();
    let mut did_reset = false;
    for root in roots {
        let db = (s.db)(root);
        let key = db.to_string_lossy().into_owned();
        let (e, h, r) = load_db(&db, since, marks.get(&key).copied().unwrap_or(0), s);
        entries.extend(e);
        high_water.insert(key, h);
        did_reset |= r;
    }
    IncrementalLoad { entries: dedup_keep_max(entries.into_iter().filter(|e| e.date >= since).collect()), high_water, did_reset }
}

/// A reset on any root cold-rescans every root, so a partial payload never replaces the cache.
pub fn scan_incremental(roots: &[PathBuf], since: Ts, marks: &HashMap<String, i64>, s: &Store) -> IncrementalLoad {
    let first = scan_roots(roots, since, marks, s);
    if !first.did_reset {
        return first;
    }
    IncrementalLoad { did_reset: true, ..scan_roots(roots, since, &HashMap::new(), s) }
}

// Cursor local bubbles

pub fn cursor_db(root: &Path) -> PathBuf {
    if ext_is(root, "vscdb") { root.to_path_buf() } else { root.join("state.vscdb") }
}

pub fn parse_cursor_bubble(o: &Value, key: &str, since: Ts) -> Option<Entry> {
    let tc = o.get("tokenCount").filter(|t| t.is_object())?;
    let (i, out) = (loose_int(tc.get("inputTokens")), loose_int(tc.get("outputTokens")));
    if i + out == 0 {
        return None;
    }
    let date = flexible_date(o.get("createdAt")).filter(|d| *d >= since)?;
    let model = non_empty(o.get("modelType")).unwrap_or_else(|| "unknown".into());
    make_entry(format!("cursor|{key}"), date, &model, [i, out, 0, 0], 0, None, true)
}

pub fn cursor_local(roots: &[PathBuf], since: Ts, marks: &HashMap<String, i64>) -> IncrementalLoad {
    scan_incremental(roots, since, marks, &Store {
        db: &cursor_db,
        max_sql: "SELECT MAX(rowid) FROM cursorDiskKV",
        rows: &|after, _| RowQuery {
            sql: if after == 0 {
                "SELECT rowid, key, value FROM cursorDiskKV WHERE key GLOB 'bubbleId:*'".into()
            } else {
                "SELECT rowid, key, value FROM cursorDiskKV NOT INDEXED WHERE rowid > ?1 AND key GLOB 'bubbleId:*'".into()
            },
            bind: after,
            bind_text: None,
        },
        parse: &|r, _| parse_cursor_bubble(&json(&col_text(r, 2)?)?, &col_text(r, 1)?, since),
    })
}

/// `ItemTable` value in the first root that has it, with the 6-try BUSY backoff.
pub fn cursor_auth_value(key: &str, roots: &[PathBuf]) -> Option<String> {
    for root in roots {
        let db = root.join("state.vscdb");
        if !db.exists() {
            continue;
        }
        for attempt in 0..6 {
            let rows = query(&db, "SELECT value FROM ItemTable WHERE key = ?1 LIMIT 1", &[&key], |r| col_text(r, 0));
            if let Some(v) = rows.as_ref().and_then(|v| v.first()).map(|v| v.trim().to_string()).filter(|v| !v.is_empty()) {
                return Some(v);
            }
            if rows.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs_f64(0.05 * (attempt + 1) as f64));
        }
    }
    None
}

// Copilot

/// `"YYYY-MM-DD HH:MM:SS"` (UTC) or ISO.
pub fn copilot_date(raw: &str) -> Option<Ts> {
    let mut t = raw.trim().to_string();
    if t.chars().count() < 19 {
        return None;
    }
    if let Some(i) = t.find(' ') {
        t.replace_range(i..i + 1, "T");
    }
    let time = &t[11.min(t.len())..];
    if !time.contains(['Z', '+', '-']) {
        t.push('Z');
    }
    parse_iso(&t)
}

/// Start of the UTC day before `since`, in SQLite's default text shape.
pub fn copilot_cutoff(since: Ts) -> String {
    let d = chrono::DateTime::from_timestamp((since - 86_400.0) as i64, 0).unwrap_or_default();
    d.format("%Y-%m-%d 00:00:00").to_string()
}

pub fn copilot(roots: &[PathBuf], since: Ts, marks: &HashMap<String, i64>) -> IncrementalLoad {
    scan_incremental(roots, since, marks, &Store {
        db: &|root| if ext_is(root, "db") { root.to_path_buf() } else { root.join("session-store.db") },
        max_sql: "SELECT MAX(id) FROM assistant_usage_events",
        rows: &|after, since| RowQuery {
            sql: "SELECT id, model, input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, created_at \
                  FROM assistant_usage_events WHERE id > ?1 AND created_at >= ?2"
                .into(),
            bind: after,
            bind_text: Some(copilot_cutoff(since)),
        },
        parse: &|r, db| {
            let date = copilot_date(&col_text(r, 6)?)?;
            let (cr, cw) = (col_tokens(r, 4), col_tokens(r, 5));
            make_entry(
                format!("copilot|{}|{}", db.display(), col_i64(r, 0)),
                date,
                &trimmed(col_text(r, 1)).unwrap_or_else(|| "unknown".into()),
                [col_tokens(r, 2).saturating_sub(cr).saturating_sub(cw), col_tokens(r, 3), cw, cr],
                0,
                None,
                false,
            )
        },
    })
}

// Devin

pub fn devin_db(root: &Path) -> PathBuf {
    if ext_is(root, "db") {
        root.to_path_buf()
    } else if file_name(root) == "cli" {
        root.join("sessions.db")
    } else {
        root.join("cli").join("sessions.db")
    }
}

pub fn devin(roots: &[PathBuf], since: Ts, marks: &HashMap<String, i64>) -> IncrementalLoad {
    scan_incremental(roots, since, marks, &Store {
        db: &devin_db,
        max_sql: "SELECT MAX(row_id) FROM message_nodes",
        rows: &|after, since| RowQuery {
            sql: format!(
                "SELECT m.row_id, m.chat_message, m.session_id, m.created_at, s.model \
                 FROM message_nodes m LEFT JOIN sessions s ON s.id = m.session_id \
                 WHERE m.row_id > ?1 AND m.created_at >= {}",
                since as i64 - 86_400
            ),
            bind: after,
            bind_text: None,
        },
        parse: &|r, db| {
            let o = json(&col_text(r, 1)?)?;
            let m = o.get("metadata")?.get("metrics").filter(|m| m.is_object())?;
            let date = epoch_secs(col_f64(r, 3)?)?;
            let session = col_text(r, 2).unwrap_or_else(|| "nosession".into());
            let msg = non_empty(o.get("message_id")).unwrap_or_else(|| format!("row{}", col_i64(r, 0)));
            make_entry(
                format!("devin|{}|{session}|{msg}", db.display()),
                date,
                &trimmed(col_text(r, 4)).unwrap_or_else(|| "unknown".into()),
                [
                    loose_int(m.get("input_tokens")),
                    loose_int(m.get("output_tokens")),
                    loose_int(m.get("cache_creation_tokens")),
                    loose_int(m.get("cache_read_tokens")),
                ],
                0,
                None,
                true,
            )
        },
    })
}

// ---- Kiro (bytes/4 estimate) ----

const KIRO_BYTES_PER_TOKEN: u64 = 4;

/// `(newest mtime ns of {db, db-wal}, summed size)`; never `-shm`.
pub fn signature(db: &Path) -> Option<(u64, u64)> {
    let mut newest = None;
    let mut size = 0;
    for p in [db.to_path_buf(), PathBuf::from(format!("{}-wal", db.display()))] {
        if let Ok(m) = fs::metadata(&p) {
            newest = Some(newest.unwrap_or(0).max(mtime_ns(&m)));
            size += m.len();
        }
    }
    newest.map(|n| (n, size))
}

fn json_bytes(v: &Value) -> u64 {
    match v {
        Value::String(s) => s.len() as u64,
        Value::Number(n) => n.to_string().len() as u64,
        Value::Bool(_) => 1, // NSNumber "1"/"0"
        Value::Array(a) => a.iter().map(json_bytes).sum(),
        Value::Object(o) => o.values().map(json_bytes).sum(),
        Value::Null => 0,
    }
}

fn field_bytes(v: Option<&Value>) -> u64 {
    match v {
        Some(Value::Object(o)) => o.iter().filter(|(k, _)| *k != "images").map(|(_, v)| json_bytes(v)).sum(),
        Some(Value::String(s)) => s.len() as u64,
        _ => 0,
    }
}

fn kiro_turns(conversation: &str, o: &Value, since: Ts) -> Vec<Entry> {
    let Some(turns) = o.get("history").and_then(Value::as_array) else { return Vec::new() };
    let mut out = Vec::new();
    // Swift passes `latest_summary ?? 0`: an absent summary counts as the one byte of "0".
    let mut cumulative = o.get("latest_summary").map_or(1, json_bytes);
    for t in turns {
        let Some(t) = t.as_object() else { continue };
        let user = field_bytes(t.get("user"));
        let assistant = field_bytes(t.get("assistant"));
        let entry = (|| {
            let meta = t.get("request_metadata")?;
            let raw = loose_double(meta.get("request_start_timestamp_ms")).filter(|r| *r > 0.0)?;
            let date = date_value(meta.get("request_start_timestamp_ms")).filter(|d| *d >= since)?;
            make_entry(
                format!("kiro|{conversation}|{}", raw.trunc() as i64),
                date,
                &non_empty(meta.get("model_id")).unwrap_or_else(|| "unknown".into()),
                [(cumulative + user) / KIRO_BYTES_PER_TOKEN, loose_int(meta.get("response_size")) / KIRO_BYTES_PER_TOKEN, 0, 0],
                0,
                None,
                true,
            )
        })();
        out.extend(entry);
        cumulative += user + assistant;
    }
    out
}

fn kiro_text_bytes(v: Option<&Value>) -> u64 {
    match v {
        Some(Value::String(s)) => s.len() as u64,
        Some(Value::Array(a)) => a.iter().map(|x| kiro_text_bytes(Some(x))).sum(),
        Some(Value::Object(o)) => {
            if let Some(kind) = o.get("kind").and_then(Value::as_str) {
                let data = o.get("data");
                return match kind {
                    "text" => kiro_text_bytes(data),
                    "toolUse" => data.and_then(|d| d.get("input")).map_or(0, json_bytes),
                    "toolResult" => kiro_text_bytes(data.and_then(|d| d.get("content"))),
                    "thinking" => data.and_then(|d| d.get("text")).and_then(Value::as_str).map_or(0, |s| s.len() as u64),
                    "json" => data.map_or(0, json_bytes),
                    _ => 0,
                };
            }
            ["content", "text", "data"].iter().find_map(|k| o.get(*k)).map_or(0, |x| kiro_text_bytes(Some(x)))
        }
        _ => 0,
    }
}

fn kiro_ms(raw: Option<&Value>, date: Ts) -> i64 {
    match loose_double(raw).filter(|v| v.is_finite() && *v > 0.0) {
        Some(v) => (if v < 1e12 { v * 1000.0 } else { v }).trunc() as i64,
        None => (date * 1000.0).trunc() as i64,
    }
}

fn kiro_cli_jsonl(path: &Path, since: Ts) -> Vec<Entry> {
    let companion = read_json(&path.with_extension("json"));
    let session = companion.as_ref().and_then(|c| non_empty(c.get("session_id"))).unwrap_or_else(|| {
        path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
    });
    let model = companion
        .as_ref()
        .and_then(|c| c.pointer("/session_state/rts_model_state/model_info/model_id"))
        .and_then(|m| non_empty(Some(m)))
        .unwrap_or_else(|| "unknown".into());
    let Ok(text) = fs::read_to_string(path) else { return Vec::new() };
    let mut out = Vec::new();
    let (mut history, mut prompt, mut assistant, mut tool) = (0u64, 0u64, 0u64, 0u64);
    let mut prompt_raw: Option<Value> = None;
    let mut prompt_date: Option<Ts> = None;
    let mut started = false;
    let mut flush = |history: &mut u64, prompt: &mut u64, assistant: &mut u64, tool: &mut u64, raw: &mut Option<Value>, date: &mut Option<Ts>, started: bool| {
        if let (true, Some(d)) = (started, date.filter(|d| *d >= since)) {
            let id = format!("kiro|cli|{session}|{}", kiro_ms(raw.as_ref(), d));
            out.extend(make_entry(id, d, &model, [(*history + *prompt + *tool) / KIRO_BYTES_PER_TOKEN, *assistant / KIRO_BYTES_PER_TOKEN, 0, 0], 0, None, true));
        }
        *history += *prompt + *assistant + *tool;
        (*prompt, *assistant, *tool, *raw, *date) = (0, 0, 0, None, None);
    };
    for line in text.split('\n').filter(|l| !l.is_empty()) {
        let Some(o) = json(line) else { continue };
        let data = o.get("data");
        match str_of(&o, "kind").unwrap_or("") {
            "Prompt" => {
                if started {
                    flush(&mut history, &mut prompt, &mut assistant, &mut tool, &mut prompt_raw, &mut prompt_date, started);
                }
                started = true;
                prompt = kiro_text_bytes(data.and_then(|d| d.get("content")));
                prompt_raw = data.and_then(|d| d.get("meta")).and_then(|m| m.get("timestamp")).cloned();
                prompt_date = flexible_date(prompt_raw.as_ref());
                assistant = 0;
                tool = 0;
            }
            "AssistantMessage" => assistant += kiro_text_bytes(data.and_then(|d| d.get("content"))),
            "ToolResults" => tool += kiro_text_bytes(data.and_then(|d| d.get("content"))),
            "Clear" => {
                flush(&mut history, &mut prompt, &mut assistant, &mut tool, &mut prompt_raw, &mut prompt_date, started);
                started = false;
                history = 0;
            }
            _ => {}
        }
    }
    flush(&mut history, &mut prompt, &mut assistant, &mut tool, &mut prompt_raw, &mut prompt_date, started);
    out
}

fn kiro_v3_jsonl(path: &Path, since: Ts) -> Vec<Entry> {
    let dir = path.parent().unwrap_or(Path::new(""));
    let session = read_json(&dir.join("session.json"));
    let session_id = session.as_ref().and_then(|s| non_empty(s.get("id"))).unwrap_or_else(|| file_name(dir));
    let model = session.as_ref().and_then(|s| non_empty(s.get("modelId"))).unwrap_or_else(|| "unknown".into());
    let fallback = session.as_ref().and_then(|s| flexible_date(s.get("createdAt")).or_else(|| flexible_date(s.get("lastModifiedAt"))));
    let Ok(text) = fs::read_to_string(path) else { return Vec::new() };
    struct St {
        history: u64,
        prompt: u64,
        assistant: u64,
        date: Option<Ts>,
        started: bool,
        turn: u64,
    }
    let mut s = St { history: 0, prompt: 0, assistant: 0, date: None, started: false, turn: 0 };
    let mut out = Vec::new();
    let mut flush = |s: &mut St| {
        let had = s.started && s.prompt + s.assistant > 0;
        if had {
            if let Some(d) = s.date.or(fallback).filter(|d| *d >= since) {
                let id = format!("kiro|v3|{session_id}|{}", s.turn);
                out.extend(make_entry(id, d, &model, [(s.history + s.prompt) / KIRO_BYTES_PER_TOKEN, s.assistant / KIRO_BYTES_PER_TOKEN, 0, 0], 0, None, true));
            }
        }
        s.history += s.prompt + s.assistant;
        s.prompt = 0;
        s.date = None;
        s.assistant = 0;
        if had {
            s.turn += 1;
        }
    };
    for line in text.split('\n').filter(|l| !l.is_empty()) {
        let Some(o) = json(line) else { continue };
        let event_date = flexible_date(o.get("timestamp"));
        if let Some((p, kind)) = o.get("payload").and_then(|p| Some((p, str_of(p, "type")?))) {
            match kind {
                "user" => {
                    if s.started {
                        flush(&mut s);
                    }
                    s.started = true;
                    s.prompt = kiro_text_bytes(p.get("content"));
                    s.date = event_date;
                    s.assistant = 0;
                }
                "assistant" | "tool_call" => {
                    s.assistant += if kind == "assistant" {
                        kiro_text_bytes(p.get("content"))
                    } else {
                        p.get("args").map_or(0, |a| a.as_str().map_or_else(|| json_bytes(a), |t| t.len() as u64))
                    };
                    if !s.started && s.assistant > 0 {
                        s.started = true;
                    }
                    if kind == "assistant" && s.date.is_none() {
                        s.date = event_date;
                    }
                }
                "tool_result" => s.prompt += kiro_text_bytes(p.get("content")),
                "turn_end" => {
                    if s.date.is_none() {
                        s.date = event_date;
                    }
                    flush(&mut s);
                    s.started = false;
                }
                _ => {}
            }
        } else if let Some(role) = str_of(&o, "role") {
            match role {
                "user" | "human" | "prompt" => {
                    if s.started {
                        flush(&mut s);
                    }
                    s.started = true;
                    s.prompt = kiro_text_bytes(o.get("content"));
                    s.date = event_date;
                    s.assistant = 0;
                }
                "assistant" | "bot" => {
                    s.assistant += kiro_text_bytes(o.get("content"));
                    if !s.started && s.assistant > 0 {
                        s.started = true;
                    }
                    if s.date.is_none() {
                        s.date = event_date;
                    }
                }
                _ => {}
            }
        }
    }
    flush(&mut s);
    out
}

fn kiro_is_jsonl(p: &Path) -> bool {
    file_name(p) == "messages.jsonl" || (ext_is(p, "jsonl") && p.parent().is_some_and(|d| file_name(d) == "cli"))
}

pub type Signatures = HashMap<String, (u64, u64)>;

/// Unchanged files (same signature) yield nothing; the cache's sticky merge keeps what they had.
pub fn kiro(roots: &[PathBuf], since: Ts, known: &Signatures) -> (Vec<Entry>, Signatures) {
    let mut out = Vec::new();
    let mut sigs = Signatures::new();
    let mut seen = HashSet::new();
    let unchanged = |key: &str, sig: Option<(u64, u64)>| sig.is_some() && known.get(key).copied() == sig;
    for root in roots {
        let db = if ext_is(root, "sqlite3") { root.clone() } else { root.join("data.sqlite3") };
        let key = db.to_string_lossy().into_owned();
        if seen.insert(key.clone()) {
            let sig = signature(&db);
            if unchanged(&key, sig) {
                sigs.insert(key, sig.unwrap());
            } else {
                let v2 = query(&db, "SELECT conversation_id, value FROM conversations_v2", &[], |r| Some((col_text(r, 0), col_text(r, 1)?)));
                let v1 = query(&db, "SELECT value FROM conversations", &[], |r| col_text(r, 0));
                if v2.is_some() || v1.is_some() {
                    for (id, value) in v2.unwrap_or_default() {
                        let Some(o) = json(&value) else { continue };
                        let conv = id.or_else(|| non_empty(o.get("conversation_id"))).unwrap_or_else(|| key.clone());
                        out.extend(kiro_turns(&conv, &o, since));
                    }
                    for value in v1.unwrap_or_default() {
                        let Some(o) = json(&value) else { continue };
                        let Some(conv) = non_empty(o.get("conversation_id")) else { continue };
                        out.extend(kiro_turns(&conv, &o, since));
                    }
                    if let Some(sig) = sig {
                        sigs.insert(key, sig);
                    }
                }
            }
        }
        let mut files = Vec::new();
        if kiro_is_jsonl(root) {
            files.push(root.clone());
        } else {
            walk_files(root, &mut |p, _| {
                if kiro_is_jsonl(p) {
                    files.push(p.to_path_buf());
                }
            });
        }
        for f in files {
            let key = f.to_string_lossy().into_owned();
            if !seen.insert(key.clone()) {
                continue;
            }
            let sig = signature(&f);
            if unchanged(&key, sig) {
                sigs.insert(key, sig.unwrap());
                continue;
            }
            if !f.exists() {
                continue;
            }
            out.extend(if file_name(&f) == "messages.jsonl" { kiro_v3_jsonl(&f, since) } else { kiro_cli_jsonl(&f, since) });
            if let Some(sig) = sig {
                sigs.insert(key, sig);
            }
        }
    }
    (dedup_keep_max(out), sigs)
}

// ---- Aside ----

pub fn aside_dbs(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    for root in roots {
        let mut dirs = vec![root.clone()];
        if let Ok(rd) = fs::read_dir(root) {
            dirs.extend(rd.flatten().map(|e| e.path()));
        }
        for d in dirs {
            let db = super::roots::canonical(&d.join("state.db"));
            if db.is_file() && !found.contains(&db) {
                found.push(db);
            }
        }
    }
    found.sort();
    found
}

/// Stable store identity: a recreated `state.db` restarts its ids.
fn file_identity(p: &Path) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        fs::metadata(p).map_or(0, |m| m.ino())
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        fs::metadata(p).map_or(0, |m| m.creation_time())
    }
}

pub fn aside(roots: &[PathBuf], since: Ts) -> Vec<Entry> {
    let sql = "SELECT t.id, t.token_usage, COALESCE(t.finished_at, t.last_message_timestamp), s.model \
               FROM session_turns t LEFT JOIN sessions s ON s.id = t.session_id \
               WHERE COALESCE(t.finished_at, t.last_message_timestamp) >= ?1";
    let mut out = Vec::new();
    let mut skipped = Vec::new();
    for db in aside_dbs(roots) {
        let store = format!("aside|{}#{}", db.display(), file_identity(&db));
        let rows = (|| {
            let conn = open_ro(&db)?;
            conn.busy_timeout(std::time::Duration::from_millis(1000)).ok()?;
            let mut stmt = conn.prepare(sql).ok()?;
            let mut rows = stmt.query([since]).ok()?;
            let mut v = Vec::new();
            loop {
                let r = match rows.next() {
                    Ok(Some(r)) => r,
                    Ok(None) => return Some(v),
                    Err(_) => return None,
                };
                let Some(usage) = col_text(r, 1).and_then(|t| json(&t)) else { continue };
                let date = col_f64(r, 2).unwrap_or(0.0);
                let model = col_text(r, 3)
                    .and_then(|t| json(&t))
                    .and_then(|m| m.get("modelId").and_then(Value::as_str).map(String::from))
                    .unwrap_or_else(|| "aside".into());
                let num = |k: &str| usage.get(k).and_then(Value::as_f64).map_or(0, clamp_token);
                let buckets = ["input", "output", "cacheRead", "cacheWrite"].iter().any(|k| usage.get(*k).is_some_and(Value::is_number));
                let t = [if buckets { num("input") } else { num("totalTokens") }, num("output"), num("cacheWrite"), num("cacheRead")];
                if t.iter().sum::<u64>() == 0 {
                    continue;
                }
                let mut e = Entry::new(format!("{store}:{}", col_i64(r, 0)), date, model, t);
                e.explicit_cost = usage.get("cost").and_then(|c| c.get("total")).and_then(Value::as_f64);
                e.cost_unavailable = Some(true);
                v.push(e);
            }
        })();
        match rows {
            Some(v) => out.extend(v),
            None => skipped.push(db.display().to_string()),
        }
    }
    if !skipped.is_empty() {
        eprintln!("aside: skipped unreadable state.db: {}", skipped.join(", "));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ptb-sql-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn rw(p: &Path) -> Connection {
        Connection::open(p).unwrap()
    }

    #[test]
    fn make_entry_moves_excess_to_output() {
        let e = make_entry("x".into(), 1.0, "m", [1, 1, 0, 0], 5, None, false).unwrap();
        assert_eq!((e.output, e.cost_unavailable), (4, Some(true)));
        assert!(make_entry("x".into(), 1.0, "m", [0; 4], 0, None, false).is_none());
    }

    #[test]
    fn copilot_dates_and_cutoff() {
        assert_eq!(copilot_date("2026-01-03 20:00:00"), parse_iso("2026-01-03T20:00:00Z"));
        assert_eq!(copilot_date("2026-01-03T20:00:00-05:00"), parse_iso("2026-01-04T01:00:00Z"));
        assert!(copilot_date("2026-01-03").is_none());
        assert_eq!(copilot_cutoff(parse_iso("2026-01-03T05:00:00Z").unwrap()), "2026-01-02 00:00:00");
    }

    #[test]
    fn copilot_incremental_watermark_and_reset() {
        let d = tmp("copilot");
        let db = d.join("session-store.db");
        let c = rw(&db);
        c.execute_batch("CREATE TABLE assistant_usage_events(id INTEGER PRIMARY KEY, model TEXT, input_tokens INT, output_tokens INT, cache_read_tokens INT, cache_write_tokens INT, created_at TEXT);
            INSERT INTO assistant_usage_events VALUES (1,'gpt-5',100,10,60,0,'2026-01-05 10:00:00');").unwrap();
        let since = parse_iso("2026-01-01T00:00:00Z").unwrap();
        let first = copilot(&[d.clone()], since, &HashMap::new());
        assert_eq!(first.entries.len(), 1);
        assert_eq!((first.entries[0].input, first.entries[0].cache_read), (40, 60));
        let key = db.to_string_lossy().into_owned();
        assert_eq!(first.high_water[&key], 1);
        let again = copilot(&[d.clone()], since, &first.high_water);
        assert!(again.entries.is_empty() && again.high_water[&key] == 1 && !again.did_reset);
        // Store recreated with lower ids → reset and cold rescan.
        let reset = copilot(&[d.clone()], since, &[(key.clone(), 9)].into());
        assert!(reset.did_reset && reset.entries.len() == 1);
    }

    #[test]
    fn hermes_session_totals() {
        let d = tmp("hermes");
        let c = rw(&d.join("state.db"));
        c.execute_batch("CREATE TABLE sessions(id TEXT, model TEXT, billing_provider TEXT, started_at REAL, message_count INT, input_tokens INT, output_tokens INT, cache_read_tokens INT, cache_write_tokens INT, reasoning_tokens INT, estimated_cost_usd REAL, actual_cost_usd REAL);
            INSERT INTO sessions VALUES ('s1','claude-opus-5','x',1700000000,3,10,5,2,1,4,0.5,NULL), ('s2',' ','x',1700000000,1,1,1,1,1,1,NULL,NULL);").unwrap();
        let out = hermes_entries(&[d.clone()], 0.0);
        assert_eq!(out.len(), 1);
        let e = &out[0];
        assert_eq!((e.output, e.cache_read, e.cache_write), (9, 2, 1));
        assert_eq!((e.explicit_cost, e.cost_is_estimate, e.cost_unavailable), (Some(0.5), Some(true), Some(true)));
    }

    #[test]
    fn opencode_v1_and_v2() {
        let d = tmp("opencode");
        let c = rw(&d.join("opencode.db"));
        c.execute_batch(r#"CREATE TABLE session_message(id TEXT, time_created INT, type TEXT, data TEXT);
            INSERT INTO session_message VALUES ('m2', 1700000000000, 'assistant', '{"tokens":{"input":5,"output":1,"reasoning":2,"cache":{"read":3,"write":0}},"model":{"id":"gpt-5","providerID":"openai"},"cost":0.01}');
            CREATE TABLE message(id TEXT, session_id TEXT, data TEXT);
            INSERT INTO message VALUES ('r1','s','{"id":"m1","tokens":{"input":1,"output":1,"total":10,"cache":{"read":0,"write":0}},"time":{"created":1700000000000},"modelID":"x","providerID":"p"}');"#).unwrap();
        let out = opencode_entries(&[d.clone()], 1.0);
        let by: HashMap<&str, &Entry> = out.iter().map(|e| (e.id.as_str(), e)).collect();
        assert_eq!(by["opencode|m2"].output, 3);
        assert_eq!(by["opencode|m2"].explicit_cost, Some(0.01));
        assert_eq!((by["opencode|m1"].output, by["opencode|m1"].cost_unavailable), (9, Some(true)));
    }

    #[test]
    fn cursor_bubbles() {
        let o: Value = serde_json::from_str(r#"{"tokenCount":{"inputTokens":3,"outputTokens":4},"createdAt":"2026-01-01T00:00:00.5Z"}"#).unwrap();
        let e = parse_cursor_bubble(&o, "bubbleId:a", 0.0).unwrap();
        assert_eq!((e.id.as_str(), e.model.as_str(), e.cost_unavailable), ("cursor|bubbleId:a", "unknown", Some(true)));
        assert!(parse_cursor_bubble(&o, "k", 2e9).is_none());
    }

    #[test]
    fn kiro_sqlite_bytes_estimate_and_signature_skip() {
        let d = tmp("kiro");
        let db = d.join("data.sqlite3");
        let conv = r#"{"conversation_id":"c1","latest_summary":"abcd","history":[{"user":{"content":"12345678","images":"zzzzzzzzzzzz"},"assistant":{"text":"xxxx"},"request_metadata":{"request_start_timestamp_ms":1700000000000,"model_id":"claude","response_size":40}},{"user":{"content":"1234"},"assistant":"y","request_metadata":{"request_start_timestamp_ms":1700000001000,"response_size":8}}]}"#;
        rw(&db).execute("CREATE TABLE conversations(key TEXT, value TEXT)", []).unwrap();
        rw(&db).execute("INSERT INTO conversations VALUES ('k', ?1)", [conv]).unwrap();
        let (es, sigs) = kiro(&[d.clone()], 0.0, &Signatures::new());
        assert_eq!(es.len(), 2);
        let first = es.iter().find(|e| e.id == "kiro|c1|1700000000000").unwrap();
        assert_eq!((first.input, first.output), (3, 10)); // (4 + 8) / 4, 40 / 4
        let second = es.iter().find(|e| e.id == "kiro|c1|1700000001000").unwrap();
        assert_eq!(second.input, (4 + 8 + 4 + 4) / 4);
        let (again, _) = kiro(&[d.clone()], 0.0, &sigs);
        assert!(again.is_empty(), "unchanged store is skipped");
    }

    #[test]
    fn kiro_cli_jsonl_turns() {
        let d = tmp("kirocli").join("cli");
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("s1.json"), r#"{"session_id":"S","session_state":{"rts_model_state":{"model_info":{"model_id":"m"}}}}"#).unwrap();
        fs::write(d.join("s1.jsonl"), concat!(
            r#"{"kind":"Prompt","data":{"content":"abcdefgh","meta":{"timestamp":1700000000}}}"#, "\n",
            r#"{"kind":"AssistantMessage","data":{"content":[{"kind":"text","data":"1234"}]}}"#, "\n",
            r#"{"kind":"Prompt","data":{"content":"abcd","meta":{"timestamp":1700000100}}}"#, "\n",
        )).unwrap();
        let (es, _) = kiro(&[d.parent().unwrap().to_path_buf()], 0.0, &Signatures::new());
        let ids: Vec<&str> = es.iter().map(|e| e.id.as_str()).collect();
        assert!(ids.contains(&"kiro|cli|S|1700000000000") && ids.contains(&"kiro|cli|S|1700000100000"));
        let second = es.iter().find(|e| e.id.ends_with("1700000100000")).unwrap();
        assert_eq!(second.input, (8 + 4 + 4) / 4);
    }

    #[test]
    fn aside_turns() {
        let d = tmp("aside");
        let c = rw(&d.join("state.db"));
        c.execute_batch(r#"CREATE TABLE sessions(id TEXT, model TEXT);
            CREATE TABLE session_turns(id INTEGER, session_id TEXT, token_usage TEXT, finished_at REAL, last_message_timestamp REAL);
            INSERT INTO sessions VALUES ('s', '{"modelId":"claude-opus-5"}');
            INSERT INTO session_turns VALUES (1,'s','{"input":1,"output":2,"cost":{"total":0.3}}',1700000000,NULL),(2,'s','{"totalTokens":7}',NULL,1700000001),(3,'s','{"input":0}',1700000002,NULL);"#).unwrap();
        let out = aside(&[d.clone()], 0.0);
        assert_eq!(out.len(), 2);
        assert_eq!((out[0].model.as_str(), out[0].explicit_cost, out[0].cost_unavailable), ("claude-opus-5", Some(0.3), Some(true)));
        assert_eq!(out[1].input, 7);
    }

    #[test]
    fn devin_rows() {
        let d = tmp("devin").join("cli");
        fs::create_dir_all(&d).unwrap();
        let c = rw(&d.join("sessions.db"));
        c.execute_batch(r#"CREATE TABLE sessions(id TEXT, model TEXT);
            CREATE TABLE message_nodes(row_id INTEGER, chat_message TEXT, session_id TEXT, created_at REAL);
            INSERT INTO sessions VALUES ('s','swe-1');
            INSERT INTO message_nodes VALUES (5,'{"message_id":"m","metadata":{"metrics":{"input_tokens":1,"output_tokens":2,"cache_creation_tokens":3,"cache_read_tokens":4}}}','s',1700000000);"#).unwrap();
        let out = devin(&[d.clone()], 1_699_999_000.0, &HashMap::new());
        assert_eq!(out.entries.len(), 1);
        assert!(out.entries[0].id.ends_with("|s|m"));
        assert_eq!(out.entries[0].total(), 10);
    }
}
