//! Claude per-account usage split (providers.md §4.1a, ClaudeAccountUsage.swift). Transcripts carry
//! no account; each login's own `history.jsonl` lists its prompts with session and time. A turn belongs
//! to the login that sent the latest prompt of its session before it. Consumed by the limits area.

use super::entry::*;
use crate::model::{ActiveBlock, CostCoverage};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Prompt times per session for one login, ascending.
pub type Prompts = HashMap<String, Vec<Ts>>;

pub struct Account {
    pub id: String,
    pub prompts: Prompts,
}

/// An account's official 5 h window, as its limits report it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FiveHourWindow {
    Running { reset: Ts },
    NotStarted,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct AccountUsage {
    pub today_tokens: u64,
    pub today_cost: f64,
    pub today_coverage: CostCoverage,
    pub month_tokens: u64,
    pub month_cost: f64,
    pub month_coverage: CostCoverage,
}

impl AccountUsage {
    pub fn add(&mut self, o: &AccountUsage) {
        self.today_tokens += o.today_tokens;
        self.today_cost += o.today_cost;
        self.today_coverage.merge(o.today_coverage);
        self.month_tokens += o.month_tokens;
        self.month_cost += o.month_cost;
        self.month_coverage.merge(o.month_coverage);
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Attribution {
    pub by_account: HashMap<String, AccountUsage>,
    pub unattributed: AccountUsage,
    pub active_blocks: HashMap<String, ActiveBlock>,
}

/// First 8 hex of SHA-256 of the folder path: the account's stable id (no path in save files).
pub fn path_key(root: &Path) -> String {
    let p: PathBuf = root.components().collect();
    format!("{:x}", Sha256::digest(p.to_string_lossy().as_bytes()))[..8].to_string()
}

/// Latest prompt at or before the turn across its sessions wins; otherwise the earliest first prompt.
pub fn owner(sessions: &[String], at: Ts, accounts: &[Account]) -> Option<String> {
    let sessions: BTreeSet<&String> = sessions.iter().collect();
    let mut latest: Option<(&str, Ts)> = None;
    let mut earliest: Option<(&str, Ts)> = None;
    for a in accounts {
        for s in &sessions {
            let Some(times) = a.prompts.get(*s).filter(|t| !t.is_empty()) else { continue };
            if let Some(before) = times.iter().rev().find(|t| **t <= at) {
                if latest.is_none_or(|(_, d)| *before > d) {
                    latest = Some((&a.id, *before));
                }
            }
            if earliest.is_none_or(|(_, d)| times[0] < d) {
                earliest = Some((&a.id, times[0]));
            }
        }
    }
    latest.or(earliest).map(|(id, _)| id.to_string())
}

pub fn attribute(entries: &[Entry], accounts: &[Account], now: Ts, windows: &HashMap<String, FiveHourWindow>) -> Attribution {
    let today = day_of(now);
    let month_start = day_of(start_of_month(now));
    let block_start = now - BLOCK_WINDOW;
    let mut todays: HashMap<String, Bucket> = HashMap::new();
    let mut months: HashMap<String, Bucket> = HashMap::new();
    let mut recent: HashMap<String, Vec<Entry>> = HashMap::new();
    for e in entries {
        let in_month = e.day >= month_start && e.day <= today;
        let in_block = e.date >= block_start;
        if !in_month && !in_block {
            continue;
        }
        let key = owner(&e.claude_session_ids(), e.date, accounts).unwrap_or_default();
        if in_block && !key.is_empty() {
            recent.entry(key.clone()).or_default().push(e.clone());
        }
        if !in_month {
            continue;
        }
        months.entry(key.clone()).or_default().add(e);
        if e.day == today {
            todays.entry(key).or_default().add(e);
        }
    }
    let usage = |k: &str| {
        let d = todays.get(k).cloned().unwrap_or_default();
        let m = months.get(k).cloned().unwrap_or_default();
        AccountUsage {
            today_tokens: d.total(),
            today_cost: d.cost,
            today_coverage: d.coverage,
            month_tokens: m.total(),
            month_cost: m.cost,
            month_coverage: m.coverage,
        }
    };
    let by_account = months.keys().filter(|k| !k.is_empty()).map(|k| (k.clone(), usage(k))).collect();
    let mut active_blocks = HashMap::new();
    for (id, turns) in recent {
        match windows.get(&id) {
            Some(FiveHourWindow::Running { reset }) if *reset > now => {
                let start = reset - BLOCK_WINDOW;
                let inside: Vec<Entry> = turns.into_iter().filter(|e| e.date >= start).collect();
                if let Some(mut b) = active_block(&inside, now) {
                    b.end = rfc3339(*reset);
                    active_blocks.insert(id, b);
                }
            }
            Some(_) => {} // the window is over or has not started
            None => {
                if let Some(b) = active_block(&turns, now) {
                    active_blocks.insert(id, b);
                }
            }
        }
    }
    Attribution { by_account, unattributed: usage(""), active_blocks }
}

fn prompt_date(v: Option<&Value>) -> Option<Ts> {
    match v? {
        Value::Number(n) => n.as_f64().filter(|d| *d > 0.0).map(|ms| ms / 1000.0),
        Value::String(s) => parse_iso(s),
        _ => None,
    }
}

pub fn parse_history(text: &str) -> Prompts {
    let mut out: Prompts = HashMap::new();
    for line in text.split('\n').filter(|l| l.contains("\"sessionId\"")) {
        let Ok(o) = serde_json::from_str::<Value>(line) else { continue };
        let Some(s) = str_of(&o, "sessionId").filter(|s| !s.is_empty()) else { continue };
        let Some(d) = prompt_date(o.get("timestamp")) else { continue };
        out.entry(s.to_string()).or_default().push(d);
    }
    out.values_mut().for_each(|v| v.sort_by(f64::total_cmp));
    out
}

struct Parsed {
    mtime: u64,
    size: u64,
    offset: u64,
    prompts: Prompts,
}

/// `<config dir>/history.jsonl`, append-only: resume only where the last read ended on `\n`.
pub fn prompts(config_dir: &Path) -> Prompts {
    static CACHE: Mutex<Option<HashMap<PathBuf, Parsed>>> = Mutex::new(None);
    let file = config_dir.join("history.jsonl");
    let Ok(meta) = fs::metadata(&file) else { return Prompts::new() };
    let (mtime, size) = (super::roots::mtime_ns(&meta), meta.len());
    let mut cache = CACHE.lock().unwrap();
    let map = cache.get_or_insert_with(HashMap::new);
    let hit = map.remove(&file);
    if let Some(h) = hit.as_ref().filter(|h| h.mtime == mtime && h.size == size) {
        let p = h.prompts.clone();
        map.insert(file, hit.unwrap());
        return p;
    }
    let Ok(mut f) = fs::File::open(&file) else { return Prompts::new() };
    let base = hit.filter(|h| {
        let mut b = [0u8; 1];
        h.offset > 0 && h.offset <= size && f.seek(SeekFrom::Start(h.offset - 1)).is_ok() && f.read_exact(&mut b).is_ok() && b[0] == b'\n'
    });
    let start = base.as_ref().map_or(0, |b| b.offset);
    let mut data = Vec::new();
    if f.seek(SeekFrom::Start(start)).is_err() || f.read_to_end(&mut data).is_err() {
        return base.map(|b| b.prompts).unwrap_or_default();
    }
    let complete = data.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
    let mut prompts = base.map(|b| b.prompts).unwrap_or_default();
    for (s, mut d) in parse_history(&String::from_utf8_lossy(&data[..complete])) {
        let v = prompts.entry(s).or_default();
        v.append(&mut d);
        v.sort_by(f64::total_cmp);
    }
    map.insert(file, Parsed { mtime, size, offset: start + complete as u64, prompts: prompts.clone() });
    prompts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acct(id: &str, s: &str, t: &[Ts]) -> Account {
        Account { id: id.into(), prompts: [(s.to_string(), t.to_vec())].into() }
    }

    #[test]
    fn owner_rules() {
        let a = [acct("a", "s", &[100.0, 300.0]), acct("b", "s", &[200.0])];
        assert_eq!(owner(&["s".into()], 250.0, &a).as_deref(), Some("b"));
        assert_eq!(owner(&["s".into()], 350.0, &a).as_deref(), Some("a"));
        assert_eq!(owner(&["s".into()], 50.0, &a).as_deref(), Some("a"), "before any prompt: earliest starter");
        assert_eq!(owner(&["x".into()], 50.0, &a), None);
        // ties keep the first account
        let t = [acct("a", "s", &[100.0]), acct("b", "s", &[100.0])];
        assert_eq!(owner(&["s".into()], 150.0, &t).as_deref(), Some("a"));
    }

    #[test]
    fn attribution_splits_today_month_and_blocks() {
        let now = now_ts();
        let mut e1 = Entry::new("1", now - 60.0, "m", [100, 0, 0, 0]);
        e1.session_id = Some("s".into());
        let mut e2 = Entry::new("2", now - 30.0, "m", [7, 0, 0, 0]);
        e2.session_id = Some("orphan".into());
        let accounts = [acct("a", "s", &[now - 120.0])];
        let r = attribute(&[e1.clone(), e2], &accounts, now, &HashMap::new());
        assert_eq!(r.by_account["a"].today_tokens, 100);
        assert_eq!(r.unattributed.month_tokens, 7);
        assert_eq!(r.active_blocks["a"].total, 100);
        let ended = attribute(&[e1.clone()], &accounts, now, &[("a".to_string(), FiveHourWindow::NotStarted)].into());
        assert!(ended.active_blocks.is_empty());
        let running = attribute(&[e1], &accounts, now, &[("a".to_string(), FiveHourWindow::Running { reset: now + 600.0 })].into());
        assert_eq!(running.active_blocks["a"].end, rfc3339(now + 600.0));
    }

    #[test]
    fn history_parse_and_incremental_read() {
        let d = std::env::temp_dir().join(format!("ptb-hist-{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        let f = d.join("history.jsonl");
        fs::write(&f, "{\"sessionId\":\"s\",\"timestamp\":2000}\n{\"sessionId\":\"s\",\"timestamp\":\"1970-01-01T00:00:01Z\"}\n{\"sessionId\":\"t\"").unwrap();
        assert_eq!(prompts(&d)["s"], vec![1.0, 2.0]);
        let mut h = fs::OpenOptions::new().append(true).open(&f).unwrap();
        std::io::Write::write_all(&mut h, b",\"timestamp\":5000}\n").unwrap();
        assert_eq!(prompts(&d)["t"], vec![5.0]);
        assert_eq!(path_key(Path::new("/Users/example/.claude-work")).len(), 8);
    }
}
