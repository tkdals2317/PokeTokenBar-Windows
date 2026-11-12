//! Codex rollouts with fork/subagent replay resolution (providers.md §4.2,
//! LocalUsageReader.swift:676-1403). A fork replays its parent's `token_count` events; those are
//! matched against the parent's usage-state history and dropped, so each turn is counted once.

use super::entry::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

pub const PROBE_BYTE_LIMIT: usize = 1 << 20;
const PROBE_CHUNK: usize = 64 * 1024;
const FORK_REPLAY_MAX_GAP: f64 = 1.0;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vector {
    pub input: u64,
    pub cached_input: u64,
    pub cache_write_input: u64,
    pub output: u64,
    pub reasoning_output: u64,
    pub total: u64,
}

impl Vector {
    fn of(v: &Value) -> Vector {
        let g = |k: &str| int_or_nil(v.get(k)).unwrap_or(0);
        Vector {
            input: g("input_tokens"),
            cached_input: g("cached_input_tokens"),
            cache_write_input: g("cache_write_input_tokens"),
            output: g("output_tokens"),
            reasoning_output: g("reasoning_output_tokens"),
            total: g("total_tokens"),
        }
    }
    fn fingerprint(&self) -> String {
        format!(
            "{},{},{},{},{},{}",
            self.input, self.cached_input, self.cache_write_input, self.output, self.reasoning_output, self.total
        )
    }
    fn billable(&self) -> u64 {
        self.input.saturating_sub(self.cached_input) + self.cached_input + self.output
    }
    fn is_lower(&self, p: &Vector) -> bool {
        self.input < p.input
            || self.cached_input < p.cached_input
            || self.cache_write_input < p.cache_write_input
            || self.output < p.output
            || self.reasoning_output < p.reasoning_output
            || self.total < p.total
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageState {
    pub cumulative: Vector,
    pub last: Vector,
}

impl UsageState {
    fn fingerprint(&self) -> String {
        format!("{}|{}", self.cumulative.fingerprint(), self.last.fingerprint())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UsageEvent {
    pub entry: Entry,
    pub usage_state: Option<UsageState>,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedRollout {
    pub path: String,
    pub session_id: Option<String>,
    pub parent_session_id: Option<String>,
    pub forked_at: Option<Ts>,
    pub is_subagent: bool,
    pub events: Vec<UsageEvent>,
}

#[derive(Debug, Clone)]
pub struct RolloutFile {
    pub path: PathBuf,
    pub mtime: u64,
    pub size: u64,
}

impl RolloutFile {
    pub fn key(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }
}

pub enum Knowledge {
    Unknown,
    Known(Option<String>),
}

struct Meta {
    id: Option<String>,
    parent: Option<String>,
    date: Option<Ts>,
    subagent: bool,
}

fn non_empty_str(v: &Value, k: &str) -> Option<String> {
    str_of(v, k).filter(|s| !s.is_empty()).map(String::from)
}

fn session_meta(line: &[u8]) -> Option<Meta> {
    let obj: Value = serde_json::from_slice(line).ok()?;
    if str_of(&obj, "type")? != "session_meta" {
        return None;
    }
    let p = obj.get("payload").filter(|p| p.is_object())?;
    Some(Meta {
        id: non_empty_str(p, "id").or_else(|| non_empty_str(p, "session_id")),
        parent: non_empty_str(p, "forked_from_id").or_else(|| non_empty_str(p, "parent_thread_id")),
        date: str_of(&obj, "timestamp").and_then(parse_iso),
        subagent: str_of(p, "thread_source") == Some("subagent")
            || p.get("source").and_then(|s| s.get("subagent")).is_some_and(|s| !s.is_null()),
    })
}

fn codex_model(line: &[u8]) -> Option<String> {
    let obj: Value = serde_json::from_slice(line).ok()?;
    let p = obj.get("payload")?;
    str_of(p, "model").or_else(|| p.get("turn_context").and_then(|t| str_of(t, "model"))).map(String::from)
}

fn should_trust_total_only(last_total: u64, cumulative: Option<&Vector>) -> bool {
    match cumulative {
        None => true,
        Some(c) if c.billable() == 0 && c.total > 0 => true,
        Some(c) => c.total == last_total,
    }
}

fn parse_token_line(line: &[u8], file: &str, turn: usize, model: &str) -> Option<(Entry, Option<UsageState>)> {
    let obj: Value = serde_json::from_slice(line).ok()?;
    let p = obj.get("payload")?;
    if str_of(p, "type")? != "token_count" {
        return None;
    }
    let info = p.get("info").filter(|i| i.is_object())?;
    let last = info.get("last_token_usage").filter(|l| l.is_object())?;
    let date = parse_iso(str_of(&obj, "timestamp")?)?;
    let input_total = int_value(last.get("input_tokens"));
    let cached = int_value(last.get("cached_input_tokens"));
    let output = int_value(last.get("output_tokens"));
    let non_cached = input_total.saturating_sub(cached);
    let last_total = int_value(last.get("total_tokens"));
    let cum = info.get("total_token_usage").filter(|c| c.is_object()).map(Vector::of);
    let zero_components = non_cached + cached + output == 0;
    let t = if zero_components && last_total > 0 && should_trust_total_only(last_total, cum.as_ref()) {
        [last_total, 0, 0, 0]
    } else {
        [non_cached, output, 0, cached]
    };
    let mut e = Entry::new(format!("codex|{file}|{turn}"), date, model, t);
    if zero_components && last_total > 0 {
        e.cost_unavailable = Some(true);
    }
    Some((e, cum.map(|cumulative| UsageState { cumulative, last: Vector::of(last) })))
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// Complete, non-empty lines; a final unterminated line is emitted too.
fn for_each_line(path: &Path, mut body: impl FnMut(&[u8])) -> io::Result<()> {
    let mut r = BufReader::with_capacity(1 << 20, fs::File::open(path)?);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        if r.read_until(b'\n', &mut buf)? == 0 {
            return Ok(());
        }
        let line = buf.strip_suffix(b"\n").unwrap_or(&buf);
        if !line.is_empty() {
            body(line);
        }
    }
}

pub fn parse_rollout(path: &Path) -> ParsedRollout {
    let file = super::roots::file_name(path);
    let mut r = ParsedRollout {
        path: path.to_string_lossy().into_owned(),
        session_id: None,
        parent_session_id: None,
        forked_at: None,
        is_subagent: false,
        events: Vec::new(),
    };
    let mut events = Vec::new();
    let mut turn = 0usize;
    let mut current: Option<String> = None;
    let mut previous: Option<(String, UsageState)> = None;
    let mut model = "codex".to_string();
    let mut first_meta: Option<Meta> = None;
    let read = for_each_line(path, |line| {
        if contains(line, b"session_meta") {
            if let Some(meta) = session_meta(line) {
                if let Some(id) = meta.id.clone().filter(|id| Some(id) != current.as_ref()) {
                    current = Some(id);
                    previous = None;
                }
                first_meta.get_or_insert(meta);
            }
        }
        if contains(line, b"\"model\"") {
            if let Some(m) = codex_model(line) {
                model = m;
            }
        }
        if !contains(line, b"token_count") {
            return;
        }
        let Some((entry, state)) = parse_token_line(line, &file, turn, &model) else { return };
        turn += 1; // advances even when the identical state is dropped below
        match (&state, &current) {
            (Some(s), Some(sid)) => {
                if previous.as_ref().is_some_and(|(ps, pst)| ps == sid && pst == s) {
                    return;
                }
                previous = Some((sid.clone(), s.clone()));
            }
            _ => previous = None,
        }
        events.push(UsageEvent { entry, usage_state: state, session_id: current.clone() });
    });
    if read.is_err() {
        return r;
    }
    if let Some(m) = first_meta {
        r.session_id = m.id;
        r.parent_session_id = m.parent;
        r.forked_at = m.date;
        r.is_subagent = m.subagent;
    }
    r.events = events;
    r
}

enum Probe {
    Id(Option<String>),
    Stop,
    Keep,
}

fn probe_line(line: &[u8]) -> Probe {
    if line.is_empty() {
        return Probe::Keep;
    }
    if std::str::from_utf8(line).is_err() {
        return Probe::Stop;
    }
    if contains(line, b"session_meta") {
        if let Some(m) = session_meta(line) {
            return Probe::Id(m.id);
        }
    }
    if contains(line, b"token_count") { Probe::Stop } else { Probe::Keep }
}

/// Session id from the first `session_meta` within `limit` bytes, complete lines only.
/// `Ok(None)` is a property of the file; `Err` is I/O and must not be remembered.
pub fn probe_session_id(path: &Path, limit: usize) -> io::Result<Option<String>> {
    let mut f = fs::File::open(path)?;
    let mut buf: Vec<u8> = Vec::new();
    let mut read = 0usize;
    let mut chunk = vec![0u8; PROBE_CHUNK];
    while read < limit {
        let want = PROBE_CHUNK.min(limit - read);
        let n = f.read(&mut chunk[..want])?;
        if n == 0 {
            return Ok(match probe_line(&buf) {
                Probe::Id(id) => id,
                _ => None,
            });
        }
        read += n;
        buf.extend_from_slice(&chunk[..n]);
        let mut start = 0;
        while let Some(nl) = buf[start..].iter().position(|&b| b == b'\n') {
            match probe_line(&buf[start..start + nl]) {
                Probe::Id(id) => return Ok(id),
                Probe::Stop => return Ok(None),
                Probe::Keep => start += nl + 1,
            }
        }
        buf.drain(..start);
    }
    Ok(match probe_line(&buf) {
        Probe::Id(id) => id,
        _ => None,
    })
}

/// Every `.jsonl` under the roots (window files and parent candidates), sorted by path.
pub fn rollout_files(roots: &[PathBuf]) -> Vec<RolloutFile> {
    let mut out = Vec::new();
    for root in super::roots::normalized_roots(roots) {
        super::roots::walk_files(&root, &mut |p, meta| {
            if super::roots::ext_is(p, "jsonl") {
                out.push(RolloutFile { path: p.to_path_buf(), mtime: super::roots::mtime_ns(meta), size: meta.len() });
            }
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

pub fn usable_filename_hint(id: &str) -> bool {
    id.chars().count() >= 4 && id.chars().any(|c| c.is_alphanumeric())
}

/// What the closure expansion needs from its caller (reader or cache).
pub trait Source {
    fn load(&mut self, f: &RolloutFile) -> ParsedRollout;
    fn knowledge(&self, f: &RolloutFile) -> Knowledge;
    fn probe(&mut self, f: &RolloutFile) -> Option<String>;
}

/// Window rollouts plus every ancestor needed for replay matching.
pub fn expand_parent_closure(
    window: &[RolloutFile],
    all: &[RolloutFile],
    src: &mut dyn Source,
) -> (Vec<ParsedRollout>, BTreeSet<String>) {
    let mut by_path: BTreeMap<String, ParsedRollout> = window.iter().map(|f| (f.key(), src.load(f))).collect();
    let included: BTreeSet<String> = window.iter().map(RolloutFile::key).collect();
    let mut pending: BTreeSet<String> = by_path.values().filter_map(|r| r.parent_session_id.clone()).collect();
    let mut searched: HashSet<String> = HashSet::new();
    while let Some(parent) = pending.iter().find(|p| !searched.contains(*p)).cloned() {
        searched.insert(parent.clone());
        if by_path.values().any(|r| r.session_id.as_ref() == Some(&parent)) {
            continue;
        }
        let unresolved: Vec<&RolloutFile> = all.iter().filter(|f| !by_path.contains_key(&f.key())).collect();
        let hinted: Vec<&RolloutFile> = unresolved
            .iter()
            .copied()
            .filter(|f| match src.knowledge(f) {
                Knowledge::Known(id) => id.as_ref() == Some(&parent),
                Knowledge::Unknown => {
                    usable_filename_hint(&parent) && super::roots::file_name(&f.path).contains(parent.as_str())
                }
            })
            .collect();
        let mut adopt = |cands: &[&RolloutFile], src: &mut dyn Source, by_path: &mut BTreeMap<String, ParsedRollout>| {
            let mut ok = false;
            for c in cands {
                let r = src.load(c);
                if r.session_id.as_ref() != Some(&parent) {
                    continue;
                }
                if let Some(a) = r.parent_session_id.clone() {
                    pending.insert(a);
                }
                by_path.insert(r.path.clone(), r);
                ok = true;
            }
            ok
        };
        if adopt(&hinted, src, &mut by_path) {
            continue;
        }
        let hinted_paths: HashSet<String> = hinted.iter().map(|f| f.key()).collect();
        let mut probed = Vec::new();
        for f in unresolved {
            if hinted_paths.contains(&f.key()) || !matches!(src.knowledge(f), Knowledge::Unknown) {
                continue;
            }
            if src.probe(f).as_ref() == Some(&parent) {
                probed.push(f);
            }
        }
        adopt(&probed, src, &mut by_path);
    }
    (by_path.into_values().collect(), included)
}

#[derive(Clone)]
struct Resolved {
    history: Vec<(Entry, Option<UsageState>)>,
    owned: Vec<Entry>,
}

fn prefix_count(child: &[UsageEvent], parent: &[(Entry, Option<UsageState>)]) -> Option<usize> {
    if child.is_empty() {
        return Some(0);
    }
    if parent.is_empty() {
        return None;
    }
    let mut n = 0;
    while n < child.len() && n < parent.len() {
        let (Some(c), Some(p)) = (&child[n].usage_state, &parent[n].1) else { return None };
        if c != p {
            break;
        }
        n += 1;
    }
    Some(n)
}

fn fallback_replay_count(r: &ParsedRollout) -> usize {
    if r.is_subagent {
        return 0;
    }
    let ev = &r.events;
    if ev.len() <= 1 {
        return ev.len();
    }
    let mut n = 1;
    while n < ev.len() && ev[n].entry.date - ev[n - 1].entry.date < FORK_REPLAY_MAX_GAP {
        n += 1;
    }
    n
}

fn resolve_owned(r: &ParsedRollout, replay: usize, inherited: Vec<(Entry, Option<UsageState>)>) -> Resolved {
    let mut history = inherited;
    let mut owned = Vec::new();
    let mut epoch = 0u64;
    let mut prev_cum: Option<Vector> = None;
    let mut prev_owner: Option<Option<String>> = None;
    for ev in r.events.iter().skip(replay) {
        let owner = if r.parent_session_id.is_none() {
            ev.session_id.clone().or_else(|| r.session_id.clone())
        } else {
            r.session_id.clone()
        };
        if prev_owner.as_ref() != Some(&owner) {
            epoch = 0;
            prev_cum = None;
            prev_owner = Some(owner.clone());
        }
        let prior = prev_cum.clone();
        match &ev.usage_state {
            Some(s) => {
                if prior.as_ref().is_some_and(|p| s.cumulative.is_lower(p)) {
                    epoch += 1;
                }
                prev_cum = Some(s.cumulative.clone());
            }
            None => prev_cum = None,
        }
        let mut entry = ev.entry.clone();
        if let (Some(owner), Some(state)) = (&owner, &ev.usage_state) {
            // #278 mid-session fill: zero components but cumulative grew.
            if entry.total() == 0
                && state.last.billable() == 0
                && state.last.total > 0
                && prior.as_ref().is_some_and(|p| state.cumulative.total > p.total)
            {
                entry.input = state.last.total;
                entry.output = 0;
                entry.cache_write = 0;
                entry.cache_read = 0;
                entry.cost_unavailable = Some(true);
            }
            entry.id = format!("codex|{owner}|{epoch}|{}", state.fingerprint());
        }
        owned.push(entry.clone());
        history.push((entry, ev.usage_state.clone()));
    }
    Resolved { history, owned }
}

pub fn resolve_rollouts(rollouts: &[ParsedRollout], included: &BTreeSet<String>) -> Vec<Entry> {
    let mut by_session: HashMap<&str, Vec<&ParsedRollout>> = HashMap::new();
    for r in rollouts {
        if let Some(s) = &r.session_id {
            by_session.entry(s.as_str()).or_default().push(r);
        }
    }
    by_session.values_mut().for_each(|v| v.sort_by(|a, b| a.path.cmp(&b.path)));
    let by_path: HashMap<&str, &ParsedRollout> = rollouts.iter().map(|r| (r.path.as_str(), r)).collect();
    let mut memo: HashMap<String, Resolved> = HashMap::new();

    fn resolve(
        r: &ParsedRollout,
        visiting: &mut HashSet<String>,
        memo: &mut HashMap<String, Resolved>,
        by_session: &HashMap<&str, Vec<&ParsedRollout>>,
    ) -> Resolved {
        if let Some(m) = memo.get(&r.path) {
            return m.clone();
        }
        if !visiting.insert(r.path.clone()) {
            return resolve_owned(r, fallback_replay_count(r), Vec::new());
        }
        let mut best: Option<(usize, Vec<(Entry, Option<UsageState>)>)> = None;
        if let Some(parent) = &r.parent_session_id {
            for cand in by_session.get(parent.as_str()).into_iter().flatten().filter(|c| c.path != r.path) {
                let rp = resolve(cand, visiting, memo, by_session);
                let Some(n) = prefix_count(&r.events, &rp.history).filter(|n| *n > 0) else { continue };
                if best.as_ref().is_none_or(|(b, _)| n > *b) {
                    best = Some((n, rp.history));
                }
            }
        }
        visiting.remove(&r.path);
        let out = match best {
            Some((n, h)) => resolve_owned(r, n, h.into_iter().take(n).collect()),
            None if r.parent_session_id.is_some() => resolve_owned(r, fallback_replay_count(r), Vec::new()),
            None => resolve_owned(r, 0, Vec::new()),
        };
        memo.insert(r.path.clone(), out.clone());
        out
    }

    let mut result = Vec::new();
    for path in included {
        let Some(r) = by_path.get(path.as_str()) else { continue };
        let mut visiting = HashSet::new();
        result.extend(resolve(r, &mut visiting, &mut memo, &by_session).owned);
    }
    dedup_keep_earliest(result)
}

/// Uncached read (tests).
#[cfg(test)]
pub fn entries(roots: &[PathBuf], since: Ts) -> Vec<Entry> {
    struct Direct;
    impl Source for Direct {
        fn load(&mut self, f: &RolloutFile) -> ParsedRollout {
            parse_rollout(&f.path)
        }
        fn knowledge(&self, _: &RolloutFile) -> Knowledge {
            Knowledge::Unknown
        }
        fn probe(&mut self, f: &RolloutFile) -> Option<String> {
            probe_session_id(&f.path, PROBE_BYTE_LIMIT).ok().flatten()
        }
    }
    let all = rollout_files(roots);
    let window: Vec<RolloutFile> = all.iter().filter(|f| super::roots::ns_to_ts(f.mtime) >= since).cloned().collect();
    let (rollouts, included) = expand_parent_closure(&window, &all, &mut Direct);
    resolve_rollouts(&rollouts, &included)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(id: &str, parent: Option<&str>, subagent: bool) -> String {
        let parent = parent.map_or(String::new(), |p| format!(r#","forked_from_id":"{p}""#));
        let src = if subagent { r#","thread_source":"subagent""# } else { "" };
        format!(r#"{{"timestamp":"2026-01-01T00:00:00Z","type":"session_meta","payload":{{"id":"{id}"{parent}{src}}}}}"#)
    }

    fn tc(sec: u32, cum: u64, last: u64) -> String {
        format!(
            r#"{{"timestamp":"2026-01-01T00:00:{sec:02}.000Z","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":{cum},"cached_input_tokens":0,"output_tokens":0,"total_tokens":{cum}}},"last_token_usage":{{"input_tokens":{last},"cached_input_tokens":0,"output_tokens":0,"total_tokens":{last}}}}}}}}}"#
        )
    }

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ptb-codex-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn write(d: &Path, name: &str, lines: &[String]) {
        fs::write(d.join(name), lines.join("\n") + "\n").unwrap();
    }

    fn total(es: &[Entry]) -> u64 {
        es.iter().map(Entry::total).sum()
    }

    #[test]
    fn last_usage_mapping_and_model() {
        let line = br#"{"timestamp":"2026-01-01T00:00:00Z","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":100,"cached_input_tokens":60,"output_tokens":5,"reasoning_output_tokens":3}}}}"#;
        let (e, s) = parse_token_line(line, "f", 0, "gpt-5").unwrap();
        assert_eq!((e.input, e.output, e.cache_write, e.cache_read), (40, 5, 0, 60));
        assert!(s.is_none() && e.cost_unavailable.is_none());
        // #278 total-only, no cumulative → trusted, cost unavailable.
        let only = br#"{"timestamp":"2026-01-01T00:00:00Z","payload":{"type":"token_count","info":{"last_token_usage":{"total_tokens":7}}}}"#;
        let (e, _) = parse_token_line(only, "f", 0, "m").unwrap();
        assert_eq!((e.input, e.cost_unavailable), (7, Some(true)));
        assert_eq!(codex_model(br#"{"payload":{"turn_context":{"model":"gpt-5.5"}}}"#).as_deref(), Some("gpt-5.5"));
    }

    #[test]
    fn fork_replay_is_dropped_by_state_prefix() {
        let d = dir("fork");
        write(&d, "rollout-parent-aaaa1111.jsonl", &[meta("aaaa1111", None, false), tc(1, 10, 10), tc(5, 30, 20)]);
        // The fork embeds its own meta, then the parent's meta and replays both parent turns, then its own turn.
        write(
            &d,
            "rollout-child.jsonl",
            &[meta("bbbb2222", Some("aaaa1111"), false), meta("aaaa1111", None, false), tc(1, 10, 10), tc(5, 30, 20), tc(9, 70, 40)],
        );
        let all = entries(&[d.clone()], 0.0);
        assert_eq!(total(&all), 70, "10 + 20 from the parent, 40 from the fork");
    }

    #[test]
    fn orphan_fork_uses_timing_fallback_but_subagent_keeps_all() {
        let d = dir("orphan");
        write(&d, "rollout-x.jsonl", &[meta("cccc3333", Some("gone"), false), tc(1, 10, 10), tc(1, 15, 5), tc(9, 30, 15)]);
        assert_eq!(total(&entries(&[d.clone()], 0.0)), 15);
        let d2 = dir("sub");
        write(&d2, "rollout-y.jsonl", &[meta("dddd4444", Some("gone"), true), tc(1, 10, 10), tc(1, 15, 5)]);
        assert_eq!(total(&entries(&[d2], 0.0)), 15);
    }

    #[test]
    fn identical_state_is_dropped_but_turn_advances() {
        let d = dir("same");
        write(&d, "r.jsonl", &[meta("eeee5555", None, false), tc(1, 10, 10), tc(2, 10, 10), tc(3, 25, 15)]);
        let r = parse_rollout(&d.join("r.jsonl"));
        assert_eq!(r.events.len(), 2);
        assert_eq!(r.events[1].entry.id, "codex|r.jsonl|2");
        let es = resolve_rollouts(&[r.clone()], &[r.path.clone()].into());
        assert!(es[0].id.starts_with("codex|eeee5555|0|"));
        assert_eq!(total(&es), 25);
    }

    #[test]
    fn archived_copy_is_counted_once() {
        let a = dir("act");
        let b = dir("arch");
        let lines = [meta("ffff6666", None, false), tc(1, 10, 10)];
        write(&a, "rollout-1.jsonl", &lines);
        write(&b, "rollout-1.jsonl", &lines);
        assert_eq!(total(&entries(&[a, b], 0.0)), 10);
    }

    #[test]
    fn parent_outside_window_found_by_probe() {
        let d = dir("probe");
        write(&d, "old.jsonl", &[meta("abcd7777", None, false), tc(1, 10, 10)]);
        let old = fs::File::options().write(true).open(d.join("old.jsonl")).unwrap();
        old.set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000)).unwrap();
        write(&d, "new.jsonl", &[meta("zzzz8888", Some("abcd7777"), false), meta("abcd7777", None, false), tc(1, 10, 10), tc(9, 30, 20)]);
        assert_eq!(total(&entries(&[d.clone()], 2_000.0)), 20, "only the fork's own turn is in the window");
        assert_eq!(probe_session_id(&d.join("old.jsonl"), PROBE_BYTE_LIMIT).unwrap().as_deref(), Some("abcd7777"));
        assert_eq!(probe_session_id(&d.join("old.jsonl"), 10).unwrap(), None, "byte limit");
    }

    #[test]
    fn cumulative_drop_starts_new_epoch() {
        let d = dir("epoch");
        write(&d, "r.jsonl", &[meta("gggg9999", None, false), tc(1, 50, 50), tc(9, 20, 20)]);
        let es = entries(&[d], 0.0);
        assert_eq!(es.len(), 2);
        assert!(es[1].id.starts_with("codex|gggg9999|1|"));
    }

    #[test]
    fn hint_needs_letters_and_length() {
        assert!(usable_filename_hint("ab12") && !usable_filename_hint("---") && !usable_filename_hint("a1"));
    }
}
