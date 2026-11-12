//! Antigravity: one SQLite store per conversation, token ledger inside protobuf blobs
//! (providers.md §4.4, LocalAntigravityUsageReader.swift).

use super::entry::*;
use super::sqlite::signature;
use rusqlite::{Connection, OpenFlags};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const TOKEN_CEILING: u64 = 1_000_000_000;
const NAMED_LOSS_LIMIT: usize = 5;

// ---- protobuf wire format ----

fn varint_at(d: &[u8], mut i: usize) -> Option<(u64, usize)> {
    let mut v = 0u64;
    let mut shift = 0;
    while i < d.len() {
        let b = d[i];
        i += 1;
        v |= ((b & 0x7f) as u64) << shift;
        if b & 0x80 == 0 {
            return Some((v, i));
        }
        shift += 7;
        if shift > 63 {
            return None;
        }
    }
    None
}

/// Visits fields until `visit` returns false or the bytes stop making sense.
fn walk<'a>(d: &'a [u8], mut visit: impl FnMut(u64, u64, Option<&'a [u8]>) -> bool) {
    let mut i = 0;
    while i < d.len() {
        let Some((key, next)) = varint_at(d, i) else { return };
        i = next;
        let field = key >> 3;
        if field == 0 {
            return;
        }
        match key & 7 {
            0 => {
                let Some((v, next)) = varint_at(d, i) else { return };
                i = next;
                if !visit(field, v, None) {
                    return;
                }
            }
            1 => {
                if d.len() - i < 8 {
                    return;
                }
                i += 8;
            }
            2 => {
                let Some((len, next)) = varint_at(d, i) else { return };
                if len > (d.len() - next) as u64 {
                    return;
                }
                let end = next + len as usize;
                if !visit(field, 0, Some(&d[next..end])) {
                    return;
                }
                i = end;
            }
            5 => {
                if d.len() - i < 4 {
                    return;
                }
                i += 4;
            }
            _ => return,
        }
    }
}

pub fn varint(d: &[u8], field: u64) -> Option<u64> {
    let mut out = None;
    walk(d, |f, v, p| {
        if f == field && p.is_none() {
            out = Some(v);
            false
        } else {
            true
        }
    });
    out
}

pub fn message(d: &[u8], field: u64) -> Option<&[u8]> {
    let mut out = None;
    walk(d, |f, _, p| match p {
        Some(p) if f == field => {
            out = Some(p);
            false
        }
        _ => true,
    });
    out
}

pub fn string(d: &[u8], field: u64) -> Option<String> {
    let p = message(d, field).filter(|p| !p.is_empty())?;
    std::str::from_utf8(p).ok().filter(|s| !s.is_empty()).map(String::from)
}

/// `Some(0)` when absent; `None` when present but too large to be a count.
pub fn token_count(d: &[u8], field: u64) -> Option<u64> {
    match varint(d, field) {
        None => Some(0),
        Some(v) if v <= TOKEN_CEILING => Some(v),
        Some(_) => None,
    }
}

fn plausible(seconds: u64) -> bool {
    (1_000_000_000..=4_102_444_800).contains(&seconds)
}

/// A `google.protobuf.Timestamp` at `field`, within the plausibility window.
pub fn timestamp(d: &[u8], field: u64) -> Option<Ts> {
    let stamp = message(d, field)?;
    let s = varint(stamp, 1).filter(|s| plausible(*s))?;
    let nanos = varint(stamp, 2).map_or(0.0, |n| if n < 1_000_000_000 { n as f64 } else { 0.0 });
    Some(s as f64 + nanos / 1e9)
}

enum Created {
    Valid(Ts),
    Absent,
    Invalid,
}

fn created_at(chat: &[u8]) -> Created {
    let Some(stamp) = message(chat, 9).and_then(|start| message(start, 4)) else { return Created::Absent };
    match varint(stamp, 1).filter(|s| plausible(*s)) {
        None => Created::Invalid,
        Some(s) => {
            let nanos = varint(stamp, 2).map_or(0.0, |n| if n < 1_000_000_000 { n as f64 } else { 0.0 });
            Created::Valid(s as f64 + nanos / 1e9)
        }
    }
}

pub struct Record {
    pub entry: Option<Entry>,
    pub discarded: usize,
}

pub fn parse_generation(
    blob: &[u8],
    conversation: &str,
    index: i64,
    fallback: &mut dyn FnMut(Option<&str>, Option<&str>) -> Option<Ts>,
) -> Record {
    let none = Record { entry: None, discarded: 0 };
    let Some(chat) = message(blob, 1) else { return none };
    let Some(usage) = message(chat, 4) else { return none };
    let response = string(usage, 11);
    let execution = string(blob, 4);
    let date = match created_at(chat) {
        Created::Valid(d) => d,
        Created::Absent => match fallback(response.as_deref(), execution.as_deref()) {
            Some(d) => d,
            None => return none,
        },
        Created::Invalid => return none,
    };
    let id = response.as_ref().map_or_else(|| format!("antigravity|{conversation}|{index}"), |r| format!("antigravity|{r}"));
    let model = string(chat, 19).unwrap_or_else(|| "unknown".into());
    let counts = [token_count(usage, 2), token_count(usage, 3), token_count(usage, 4), token_count(usage, 5)];
    let discarded = counts.iter().filter(|c| c.is_none()).count();
    let [i, o, cw, cr] = counts.map(|c| c.unwrap_or(0));
    let entry = (i + o + cw + cr > 0).then(|| Entry::new(id, date, format!("antigravity/{model}"), [i, o, cw, cr]));
    Record { entry, discarded }
}

// ---- one conversation store ----

pub enum Read {
    Complete(Vec<Entry>, usize),
    Incomplete(i32, usize),
    Unreadable(Option<i32>),
    NotAConversation,
}

fn uri(path: &Path, params: &str) -> String {
    let mut p = path.to_string_lossy().replace('\\', "/");
    if p.as_bytes().get(1) == Some(&b':') {
        p = format!("/{p}");
    }
    let mut enc = String::new();
    for c in p.chars() {
        match c {
            '%' => enc.push_str("%25"),
            '?' => enc.push_str("%3F"),
            '#' => enc.push_str("%23"),
            ' ' => enc.push_str("%20"),
            c => enc.push(c),
        }
    }
    format!("file:{enc}?{params}")
}

/// `mode=ro` first (sees the WAL tail), then `immutable=1` (no `-wal` yet). Each must read.
fn open(path: &Path) -> Option<Connection> {
    if !path.exists() {
        return None;
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    ["mode=ro", "immutable=1"].iter().find_map(|p| {
        let c = Connection::open_with_flags(uri(path, p), flags).ok()?;
        c.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get::<_, i64>(0)).ok()?;
        Some(c)
    })
}

fn sqlite_code(e: &rusqlite::Error) -> i32 {
    match e {
        rusqlite::Error::SqliteFailure(f, _) => f.extended_code & 0xff,
        _ => 1,
    }
}

type StepDates = (HashMap<String, Ts>, HashMap<String, Vec<Ts>>);

fn step_dates(c: &Connection) -> Result<StepDates, i32> {
    let mut stmt = match c.prepare("SELECT metadata FROM steps WHERE metadata IS NOT NULL ORDER BY idx") {
        Ok(s) => s,
        Err(e) if sqlite_code(&e) == 1 => return Ok(Default::default()),
        Err(e) => return Err(sqlite_code(&e)),
    };
    let mut rows = stmt.query([]).map_err(|e| sqlite_code(&e))?;
    let (mut by_response, mut by_execution): StepDates = Default::default();
    loop {
        let r = match rows.next() {
            Ok(Some(r)) => r,
            Ok(None) => return Ok((by_response, by_execution)),
            Err(e) => return Err(sqlite_code(&e)),
        };
        let Ok(b) = r.get::<_, Vec<u8>>(0) else { continue };
        let Some(date) = timestamp(&b, 8).or_else(|| timestamp(&b, 1)) else { continue };
        if let Some(rid) = message(&b, 9).and_then(|m| string(m, 11)) {
            by_response.insert(rid, date);
        }
        if let Some(eid) = string(&b, 12) {
            by_execution.entry(eid).or_default().push(date);
        }
    }
}

pub fn conversation(db: &Path) -> Read {
    let Some(c) = open(db) else { return Read::Unreadable(None) };
    let mut stmt = match c.prepare("SELECT idx, data FROM gen_metadata WHERE data IS NOT NULL ORDER BY idx") {
        Ok(s) => s,
        Err(e) if sqlite_code(&e) == 1 => return Read::NotAConversation,
        Err(e) => return Read::Unreadable(Some(sqlite_code(&e))),
    };
    let (by_response, by_execution) = match step_dates(&c) {
        Ok(d) => d,
        Err(code) => return Read::Incomplete(code, 0),
    };
    let store_date = signature(db).map_or_else(now_ts, |(m, _)| super::roots::ns_to_ts(m));
    let conv = db.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let mut taken: HashMap<String, usize> = HashMap::new();
    let mut fallback = |rid: Option<&str>, eid: Option<&str>| -> Option<Ts> {
        if let Some(d) = rid.and_then(|r| by_response.get(r)) {
            return Some(*d);
        }
        if let Some(list) = eid.and_then(|e| by_execution.get(e)).filter(|l| !l.is_empty()) {
            let n = taken.entry(eid.unwrap().to_string()).or_default();
            let d = list[(*n).min(list.len() - 1)];
            *n += 1;
            return Some(d);
        }
        Some(store_date)
    };
    let mut rows = match stmt.query([]) {
        Ok(r) => r,
        Err(e) => return Read::Unreadable(Some(sqlite_code(&e))),
    };
    let (mut entries, mut discarded, mut count) = (Vec::new(), 0, 0);
    loop {
        let r = match rows.next() {
            Ok(Some(r)) => r,
            Ok(None) => break,
            Err(e) => return Read::Incomplete(sqlite_code(&e), count),
        };
        count += 1;
        let idx: i64 = r.get(0).unwrap_or(0);
        let Ok(blob) = r.get::<_, Vec<u8>>(1) else { continue };
        if blob.is_empty() {
            continue;
        }
        let rec = parse_generation(&blob, &conv, idx, &mut fallback);
        discarded += rec.discarded;
        entries.extend(rec.entry);
    }
    Read::Complete(entries, discarded)
}

// ---- scan with signature-keyed blobs ----

#[derive(Clone)]
pub struct Blob {
    pub mtime: u64,
    pub size: u64,
    pub entries: Vec<Entry>,
}

fn databases(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in roots {
        let Ok(rd) = fs::read_dir(root) else { continue };
        let mut names: Vec<String> =
            rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.ends_with(".db")).collect();
        names.sort();
        out.extend(names.into_iter().map(|n| root.join(n)));
    }
    out
}

fn capped(lines: Vec<String>, summary: impl Fn(usize, usize) -> String) -> Vec<String> {
    if lines.len() <= NAMED_LOSS_LIMIT {
        return lines;
    }
    let total = lines.len();
    let mut v: Vec<String> = lines.into_iter().take(NAMED_LOSS_LIMIT).collect();
    v.push(summary(total, total - NAMED_LOSS_LIMIT));
    v
}

/// Reads every store the window admits, reusing blobs whose signature still matches. A failed
/// read keeps the previous blob under its *old* signature so the next sweep retries.
pub fn scan(roots: &[PathBuf], since: Ts, known: &HashMap<String, Blob>) -> (HashMap<String, Blob>, Vec<String>) {
    let mut blobs = HashMap::new();
    let (mut loss, mut discards) = (Vec::new(), Vec::new());
    for db in databases(roots) {
        let Some((mtime, size)) = signature(&db).filter(|(m, _)| super::roots::ns_to_ts(*m) >= since) else { continue };
        let key = db.to_string_lossy().into_owned();
        if let Some(b) = known.get(&key).filter(|b| b.mtime == mtime && b.size == size) {
            blobs.insert(key, b.clone());
            continue;
        }
        let conv = db.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        match conversation(&db) {
            Read::Complete(entries, d) => {
                if d > 0 {
                    discards.push(format!("antigravity: conversation={conv} discarded={d} token counter(s) over {TOKEN_CEILING}"));
                }
                blobs.insert(key, Blob { mtime, size, entries });
            }
            Read::NotAConversation => {
                blobs.insert(key, Blob { mtime, size, entries: Vec::new() });
            }
            other => {
                loss.push(match other {
                    Read::Incomplete(s, rows) => format!("antigravity: lost conversation={conv} reason=scan-incomplete status={s} rows={rows}"),
                    Read::Unreadable(s) => format!(
                        "antigravity: lost conversation={conv} reason=unreadable{}",
                        s.map_or(String::new(), |s| format!(" status={s}"))
                    ),
                    _ => unreachable!(),
                });
                if let Some(stale) = known.get(&key) {
                    blobs.insert(key, stale.clone());
                }
            }
        }
    }
    let mut log = capped(loss, |t, h| format!("antigravity: lost {t} conversation(s) this scan ({h} not named)"));
    log.extend(capped(discards, |t, h| format!("antigravity: discarded token counters in {t} conversation(s) ({h} not named)")));
    (blobs, log)
}

pub fn assemble(blobs: &HashMap<String, Blob>, since: Ts) -> Vec<Entry> {
    dedup_keep_max(blobs.values().flat_map(|b| b.entries.iter().filter(|e| e.date >= since).cloned()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(field: u64, wire: u64) -> Vec<u8> {
        enc(field << 3 | wire)
    }
    fn enc(mut v: u64) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                out.push(b);
                return out;
            }
            out.push(b | 0x80);
        }
    }
    fn vi(field: u64, v: u64) -> Vec<u8> {
        [tag(field, 0), enc(v)].concat()
    }
    fn msg(field: u64, body: &[u8]) -> Vec<u8> {
        [tag(field, 2), enc(body.len() as u64), body.to_vec()].concat()
    }

    fn generation(created: Option<u64>, input: u64) -> Vec<u8> {
        let usage = [vi(2, input), vi(3, 5), vi(5, 7), msg(11, b"resp-1")].concat();
        let mut chat = [msg(4, &usage), msg(19, b"gemini-3.6-flash")].concat();
        if let Some(s) = created {
            chat.extend(msg(9, &msg(4, &vi(1, s))));
        }
        [msg(1, &chat), msg(4, b"exec-1")].concat()
    }

    #[test]
    fn parses_usage_and_ids() {
        let mut never = |_: Option<&str>, _: Option<&str>| None;
        let r = parse_generation(&generation(Some(1_700_000_000), 10), "conv", 3, &mut never);
        let e = r.entry.unwrap();
        assert_eq!((e.input, e.output, e.cache_read, e.total()), (10, 5, 7, 22));
        assert_eq!((e.id.as_str(), e.model.as_str(), e.date), ("antigravity|resp-1", "antigravity/gemini-3.6-flash", 1_700_000_000.0));
        // Implausible created_at drops the record; absent asks the fallback.
        assert!(parse_generation(&generation(Some(5), 10), "c", 0, &mut never).entry.is_none());
        let mut fb = |r: Option<&str>, x: Option<&str>| {
            assert_eq!((r, x), (Some("resp-1"), Some("exec-1")));
            Some(1_800_000_000.0)
        };
        assert_eq!(parse_generation(&generation(None, 10), "c", 0, &mut fb).entry.unwrap().date, 1_800_000_000.0);
        // Over-ceiling counter is discarded, not clamped.
        let r = parse_generation(&generation(Some(1_700_000_000), TOKEN_CEILING + 1), "c", 0, &mut never);
        assert_eq!((r.discarded, r.entry.unwrap().input), (1, 0));
    }

    #[test]
    fn malformed_bytes_stop_safely() {
        assert_eq!(varint(&[0x08, 0xff, 0xff], 1), None);
        assert!(message(&[0x0a, 0x7f, 0x01], 1).is_none());
        assert_eq!(token_count(&[], 2), Some(0));
    }

    #[test]
    fn store_scan_and_step_dates() {
        let d = std::env::temp_dir().join(format!("ptb-ag-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let db = d.join("conv1.db");
        let c = Connection::open(&db).unwrap();
        c.execute_batch("CREATE TABLE gen_metadata(idx INTEGER, data BLOB); CREATE TABLE steps(idx INTEGER, metadata BLOB);").unwrap();
        c.execute("INSERT INTO gen_metadata VALUES (0, ?1)", [generation(None, 10)]).unwrap();
        let step = [msg(8, &vi(1, 1_750_000_000)), msg(12, b"exec-1")].concat();
        c.execute("INSERT INTO steps VALUES (0, ?1)", [step]).unwrap();
        drop(c);
        fs::write(d.join("other.db"), b"").unwrap();
        let (blobs, log) = scan(&[d.clone()], 0.0, &HashMap::new());
        let es = assemble(&blobs, 0.0);
        assert_eq!(es.len(), 1);
        assert_eq!(es[0].date, 1_750_000_000.0);
        assert!(log.is_empty(), "{log:?}");
        assert!(uri(Path::new(r"C:\a b\x.db"), "mode=ro").starts_with("file:/C:/a%20b/x.db?"));
    }
}
