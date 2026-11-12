//! Parse caches (providers.md §3.5, §4.2, §5; LocalUsageCache.swift, LocalAdditionalUsageCache).
//! - `JsonlCache`: per-file blobs keyed by `(path, mtime, size)`, persisted to `usage-cache.json`.
//!   Claude transcripts are tail-parsed from a byte offset (#338). Codex keeps parsed rollouts and a
//!   session-id index that outlives the 40-day prune.
//! - `Additional`: 30 s in-memory cache for the SQLite sources, with watermarks and sticky merges.

use super::codex::{self, Knowledge, ParsedRollout, RolloutFile, Source};
use super::entry::*;
use super::jsonl::{self, ClaudeCostState};
use super::roots::{ext_is, mtime_ns, ns_to_ts, walk_files};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const CLAUDE_V: u32 = 1;
const CODEX_V: u32 = 6;
const CODEX_INDEX_V: u32 = 2;
const GROK_V: u32 = 2;
const PI_V: u32 = 3;
const OMP_V: u32 = 2;
const OMO_V: u32 = 1;
const KIMI_V: u32 = 1;
const PRUNE_AGE: f64 = 40.0 * 86_400.0;
const SAVE_THROTTLE: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Blob {
    pub mtime: u64,
    pub size: u64,
    pub entries: Vec<Entry>,
    /// Bytes parsed so far (append-only logs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    /// Latest Claude cost ledger; entries stay raw so a later ledger can replace it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_state: Option<ClaudeCostState>,
}

impl Blob {
    fn attributed(&self) -> Vec<Entry> {
        match &self.cost_state {
            Some(s) => jsonl::apply_reported_cost(self.entries.clone(), s),
            None => self.entries.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexBlob {
    pub mtime: u64,
    pub size: u64,
    pub rollout: ParsedRollout,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Probe {
    pub mtime: u64,
    pub size: u64,
    pub session_id: Option<String>,
}

/// A map that fails to decode is dropped alone (missing ⇒ empty).
fn lenient<'de, D: Deserializer<'de>, T: DeserializeOwned + Default>(d: D) -> Result<T, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(serde_json::from_value(v).unwrap_or_default())
}

type Blobs = HashMap<String, Blob>;

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct Snapshot {
    #[serde(deserialize_with = "lenient")]
    claude: Blobs,
    #[serde(deserialize_with = "lenient")]
    codex: HashMap<String, CodexBlob>,
    #[serde(deserialize_with = "lenient")]
    codex_session_ids: HashMap<String, Probe>,
    #[serde(deserialize_with = "lenient")]
    gemini: Blobs,
    #[serde(deserialize_with = "lenient")]
    grok: Blobs,
    #[serde(deserialize_with = "lenient")]
    pi: Blobs,
    #[serde(deserialize_with = "lenient")]
    omp: Blobs,
    #[serde(deserialize_with = "lenient")]
    omo: Blobs,
    #[serde(deserialize_with = "lenient")]
    kimi: Blobs,
    claude_parser_version: u32,
    codex_parser_version: u32,
    codex_session_index_version: u32,
    grok_parser_version: u32,
    pi_parser_version: u32,
    omp_parser_version: u32,
    omo_parser_version: u32,
    kimi_parser_version: u32,
}

#[derive(Clone, Copy)]
pub enum Kind {
    Gemini,
    Grok,
    Pi,
    Omp,
    Omo,
    Kimi,
}

#[derive(Default)]
pub struct JsonlCache {
    s: Snapshot,
    /// `None` in tests: never touch the user's real cache.
    path: Option<PathBuf>,
    loaded: bool,
    dirty: bool,
    last_save: Option<Instant>,
}

impl JsonlCache {
    pub fn new(path: Option<PathBuf>) -> Self {
        JsonlCache { path, ..Default::default() }
    }

    fn ensure_loaded(&mut self) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        let Some(raw) = self.path.as_ref().and_then(|p| std::fs::read(p).ok()) else { return };
        let Ok(s) = serde_json::from_slice::<Snapshot>(&raw) else { return };
        self.s = s;
        let s = &mut self.s;
        let mut drop_if = |ok: bool, clear: &mut dyn FnMut()| {
            if !ok {
                clear();
                self.dirty = true;
            }
        };
        drop_if(s.claude_parser_version == CLAUDE_V, &mut || s.claude.clear());
        drop_if(s.codex_parser_version == CODEX_V, &mut || s.codex.clear());
        drop_if(s.codex_session_index_version == CODEX_INDEX_V, &mut || s.codex_session_ids.clear());
        drop_if(s.grok_parser_version == GROK_V, &mut || s.grok.clear());
        drop_if(s.pi_parser_version == PI_V, &mut || s.pi.clear());
        drop_if(s.omp_parser_version == OMP_V, &mut || s.omp.clear());
        drop_if(s.omo_parser_version == OMO_V, &mut || s.omo.clear());
        drop_if(s.kimi_parser_version == KIMI_V, &mut || s.kimi.clear());
    }

    /// Throttled (60 s) atomic save; prunes blobs older than 40 days first.
    pub fn save_if_needed(&mut self, now: Ts) {
        if !self.dirty {
            return;
        }
        let Some(path) = self.path.clone() else {
            self.dirty = false;
            return;
        };
        if self.last_save.is_some_and(|t| t.elapsed() < SAVE_THROTTLE) {
            return;
        }
        let cutoff = now - PRUNE_AGE;
        let s = &mut self.s;
        for m in [&mut s.claude, &mut s.gemini, &mut s.grok, &mut s.pi, &mut s.omp, &mut s.omo, &mut s.kimi] {
            m.retain(|_, b| ns_to_ts(b.mtime) >= cutoff);
        }
        s.codex.retain(|_, b| ns_to_ts(b.mtime) >= cutoff);
        (s.claude_parser_version, s.codex_parser_version, s.codex_session_index_version) = (CLAUDE_V, CODEX_V, CODEX_INDEX_V);
        (s.grok_parser_version, s.pi_parser_version, s.omp_parser_version) = (GROK_V, PI_V, OMP_V);
        (s.omo_parser_version, s.kimi_parser_version) = (OMO_V, KIMI_V);
        let Ok(json) = serde_json::to_vec(&self.s) else { return };
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() && std::fs::rename(&tmp, &path).is_ok() {
            self.dirty = false;
            self.last_save = Some(Instant::now());
        }
    }

    pub fn claude(&mut self, roots: &[PathBuf], since: Ts) -> Vec<Entry> {
        self.ensure_loaded();
        let mut all = Vec::new();
        for root in roots {
            walk_files(root, &mut |p, meta| {
                if !ext_is(p, "jsonl") {
                    return;
                }
                let (mtime, size) = (mtime_ns(meta), meta.len());
                if ns_to_ts(mtime) < since {
                    return;
                }
                let key = p.to_string_lossy().into_owned();
                if let Some(b) = self.s.claude.get(&key).filter(|b| b.mtime == mtime && b.size == size) {
                    all.extend(b.attributed());
                    return;
                }
                let prior = self.s.claude.get(&key).cloned();
                let prior = prior.as_ref();
                // Tail-parse only when the file strictly grew; a rewrite or truncation re-reads it all.
                let from = prior.filter(|b| size > b.size).and_then(|b| b.offset).unwrap_or(0);
                match jsonl::parse_claude_file(p, from) {
                    Some(tail) => {
                        let (entries, cost_state) = match prior.filter(|_| from > 0) {
                            Some(b) => (
                                dedup_keep_max([b.entries.clone(), tail.entries].concat()),
                                tail.cost_state.or_else(|| b.cost_state.clone()),
                            ),
                            None => (tail.entries, tail.cost_state),
                        };
                        let blob = Blob { mtime, size, entries, offset: Some(tail.consumed), cost_state };
                        all.extend(blob.attributed());
                        self.s.claude.insert(key, blob);
                        self.dirty = true;
                    }
                    // Read failure: keep the old blob under its old signature (retried next time).
                    None => all.extend(prior.map(Blob::attributed).unwrap_or_default()),
                }
            });
        }
        dedup_keep_max(all)
    }

    pub fn generic(&mut self, kind: Kind, roots: &[PathBuf], since: Ts) -> Vec<Entry> {
        self.ensure_loaded();
        let (map, allow_json, include, parse): (&mut Blobs, bool, fn(&Path) -> bool, fn(&Path) -> Option<Vec<Entry>>) = match kind {
            Kind::Gemini => (&mut self.s.gemini, true, |_| true, jsonl::parse_gemini_file),
            Kind::Grok => (&mut self.s.grok, false, jsonl::is_grok_usage_file, jsonl::parse_grok_file),
            Kind::Pi => (&mut self.s.pi, false, |_| true, |p| jsonl::parse_pi_file(p, jsonl::PiFlavor::Pi)),
            Kind::Omp => (&mut self.s.omp, false, jsonl::is_omp_usage_file, |p| jsonl::parse_pi_file(p, jsonl::PiFlavor::Omp)),
            Kind::Omo => (&mut self.s.omo, false, |_| true, |p| jsonl::parse_pi_file(p, jsonl::PiFlavor::Omo)),
            Kind::Kimi => (&mut self.s.kimi, false, jsonl::is_kimi_usage_file, jsonl::parse_kimi_file),
        };
        let mut dirty = false;
        let mut all = Vec::new();
        for root in roots {
            walk_files(root, &mut |p, meta| {
                if !(ext_is(p, "jsonl") || allow_json && ext_is(p, "json")) || !include(p) {
                    return;
                }
                let (mtime, size) = (mtime_ns(meta), meta.len());
                if ns_to_ts(mtime) < since {
                    return;
                }
                let key = p.to_string_lossy().into_owned();
                if let Some(b) = map.get(&key).filter(|b| b.mtime == mtime && b.size == size) {
                    all.extend(b.entries.iter().cloned());
                } else if let Some(entries) = parse(p) {
                    all.extend(entries.iter().cloned());
                    map.insert(key, Blob { mtime, size, entries, offset: None, cost_state: None });
                    dirty = true;
                } else if let Some(b) = map.get(&key) {
                    all.extend(b.entries.iter().cloned());
                }
            });
        }
        self.dirty |= dirty;
        dedup_keep_max(all)
    }

    pub fn codex(&mut self, roots: &[PathBuf], since: Ts) -> Vec<Entry> {
        self.ensure_loaded();
        let files = codex::rollout_files(roots);
        let window: Vec<RolloutFile> = files.iter().filter(|f| ns_to_ts(f.mtime) >= since).cloned().collect();
        let mut src = CodexSource { cache: self, failed: HashSet::new() };
        let (rollouts, included): (Vec<ParsedRollout>, BTreeSet<String>) = codex::expand_parent_closure(&window, &files, &mut src);
        // Index entries of deleted rollouts go; an empty enumeration is not "all deleted".
        if !files.is_empty() {
            let existing: HashSet<String> = files.iter().map(RolloutFile::key).collect();
            let before = self.s.codex_session_ids.len();
            self.s.codex_session_ids.retain(|k, _| existing.contains(k));
            self.dirty |= before != self.s.codex_session_ids.len();
        }
        codex::resolve_rollouts(&rollouts, &included)
    }
}

struct CodexSource<'a> {
    cache: &'a mut JsonlCache,
    /// Probe I/O failures, remembered for this call only.
    failed: HashSet<String>,
}

impl CodexSource<'_> {
    fn remember(&mut self, f: &RolloutFile, id: Option<String>) {
        self.cache.s.codex_session_ids.insert(f.key(), Probe { mtime: f.mtime, size: f.size, session_id: id });
        self.cache.dirty = true;
    }
}

impl Source for CodexSource<'_> {
    fn load(&mut self, f: &RolloutFile) -> ParsedRollout {
        let key = f.key();
        if let Some(b) = self.cache.s.codex.get(&key).filter(|b| b.mtime == f.mtime && b.size == f.size) {
            let r = b.rollout.clone();
            let indexed = self.cache.s.codex_session_ids.get(&key).is_some_and(|p| p.mtime == f.mtime && p.size == f.size);
            if !indexed {
                self.remember(f, r.session_id.clone());
            }
            return r;
        }
        let r = codex::parse_rollout(&f.path);
        self.cache.s.codex.insert(key, CodexBlob { mtime: f.mtime, size: f.size, rollout: r.clone() });
        self.remember(f, r.session_id.clone());
        r
    }

    fn knowledge(&self, f: &RolloutFile) -> Knowledge {
        let key = f.key();
        if let Some(b) = self.cache.s.codex.get(&key).filter(|b| b.mtime == f.mtime && b.size == f.size) {
            return Knowledge::Known(b.rollout.session_id.clone());
        }
        match self.cache.s.codex_session_ids.get(&key).filter(|p| p.mtime == f.mtime && p.size == f.size) {
            Some(p) => Knowledge::Known(p.session_id.clone()),
            None => Knowledge::Unknown,
        }
    }

    fn probe(&mut self, f: &RolloutFile) -> Option<String> {
        if self.failed.contains(&f.key()) {
            return None;
        }
        match codex::probe_session_id(&f.path, codex::PROBE_BYTE_LIMIT) {
            Ok(id) => {
                self.remember(f, id.clone());
                id
            }
            Err(_) => {
                self.failed.insert(f.key());
                None
            }
        }
    }
}

// ---- additional (SQLite) sources ----

pub struct Additional {
    pub loaded_at: Ts,
    pub month: String,
    pub entries: Vec<Entry>,
    pub high_water: HashMap<String, i64>,
    pub kiro_signatures: super::sqlite::Signatures,
}

pub const ADDITIONAL_FRESHNESS: f64 = 30.0;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn line(id: &str, out: u64) -> String {
        format!(
            r#"{{"type":"assistant","requestId":"r","timestamp":"2026-01-01T00:00:00Z","message":{{"id":"{id}","model":"claude-opus-5","usage":{{"input_tokens":1,"output_tokens":{out}}}}}}}"#
        ) + "\n"
    }

    #[test]
    fn claude_tail_parse_and_persist() {
        let d = std::env::temp_dir().join(format!("ptb-cache-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        let proj = d.join("projects").join("p");
        fs::create_dir_all(&proj).unwrap();
        let f = proj.join("sess.jsonl");
        fs::write(&f, line("a", 1) + &line("b", 1)).unwrap();
        let cache_file = d.join("usage-cache.json");
        let mut c = JsonlCache::new(Some(cache_file.clone()));
        let roots = vec![d.join("projects")];
        assert_eq!(c.claude(&roots, 0.0).len(), 2);
        let off = c.s.claude.values().next().unwrap().offset.unwrap();
        assert_eq!(off, fs::metadata(&f).unwrap().len());
        // Append a grown copy of `b` plus a ledger: tail only, keep-max, ledger applied.
        let mut h = fs::OpenOptions::new().append(true).open(&f).unwrap();
        h.write_all(line("b", 9).as_bytes()).unwrap();
        h.write_all(b"{\"type\":\"cost-state\",\"modelUsage\":{\"claude-opus-5\":{\"costUSD\":1.2}}}\n").unwrap();
        let es = c.claude(&roots, 0.0);
        assert_eq!(es.iter().map(Entry::total).sum::<u64>(), 2 + 10);
        assert!((es.iter().filter_map(|e| e.explicit_cost).sum::<f64>() - 1.2).abs() < 1e-9);
        assert_eq!(es[0].session_id.as_deref(), Some("sess"));
        c.save_if_needed(now_ts());
        let mut reloaded = JsonlCache::new(Some(cache_file));
        reloaded.ensure_loaded();
        assert_eq!(reloaded.s.claude.len(), 1);
    }
}
