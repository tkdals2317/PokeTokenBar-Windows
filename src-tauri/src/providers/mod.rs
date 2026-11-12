//! Usage providers: local CLI logs → `model::UsageSnapshot`. Owner: providers area.
//! Today's token usage read straight from local CLI logs.
//! - Claude: `~/.claude/projects/**/*.jsonl`, `type:"assistant"` lines with `message.usage`.
//!   Resumed sessions/sidechains log the same turn again, so dedup by `(message.id, requestId)`
//!   and keep the largest total (streaming logs partial output first).
//! - Codex: `~/.codex/sessions/**/rollout-*.jsonl`, `payload.type:"token_count"` → `info.last_token_usage`.

use chrono::{DateTime, Local, NaiveDate};
use crate::model::{DailyUsage, ProviderSnapshot, Tokens, Usage, UsageSnapshot};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

fn n(v: &Value, key: &str) -> u64 {
    v.get(key).and_then(Value::as_u64).unwrap_or(0)
}

fn local_day(ts: &str) -> Option<NaiveDate> {
    DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|d| d.with_timezone(&Local).date_naive())
}

/// Returns `(dedup key, tokens)` for a Claude assistant line logged on `day`.
pub fn parse_claude_line(line: &str, day: NaiveDate) -> Option<(String, Tokens)> {
    let obj: Value = serde_json::from_str(line).ok()?;
    if obj.get("type")?.as_str()? != "assistant" {
        return None;
    }
    if local_day(obj.get("timestamp")?.as_str()?)? != day {
        return None;
    }
    let msg = obj.get("message")?;
    let usage = msg.get("usage")?;
    let id = format!(
        "{}|{}",
        msg.get("id").and_then(Value::as_str).unwrap_or(""),
        obj.get("requestId").and_then(Value::as_str).unwrap_or("")
    );
    Some((
        id,
        Tokens {
            input: n(usage, "input_tokens"),
            output: n(usage, "output_tokens"),
            cache_write: n(usage, "cache_creation_input_tokens"),
            cache_read: n(usage, "cache_read_input_tokens"),
        },
    ))
}

/// Codex `last_token_usage` is the turn delta. input_tokens includes cached input.
pub fn parse_codex_line(line: &str, day: NaiveDate) -> Option<Tokens> {
    if !line.contains("token_count") {
        return None;
    }
    let obj: Value = serde_json::from_str(line).ok()?;
    let payload = obj.get("payload")?;
    if payload.get("type")?.as_str()? != "token_count" {
        return None;
    }
    if local_day(obj.get("timestamp")?.as_str()?)? != day {
        return None;
    }
    let last = payload.get("info")?.get("last_token_usage")?;
    let cached = n(last, "cached_input_tokens");
    Some(Tokens {
        input: n(last, "input_tokens").saturating_sub(cached),
        output: n(last, "output_tokens"),
        cache_write: 0,
        cache_read: cached,
    })
}

/// `*.jsonl` under `root`, modified on/after `since`.
fn jsonl_files(root: &Path, since: SystemTime, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        let Ok(meta) = e.metadata() else { continue };
        if meta.is_dir() {
            jsonl_files(&path, since, out);
        } else if path.extension().is_some_and(|x| x == "jsonl")
            && meta.modified().is_ok_and(|m| m >= since)
        {
            out.push(path);
        }
    }
}

fn lines(path: &Path) -> impl Iterator<Item = String> {
    fs::File::open(path)
        .ok()
        .into_iter()
        .flat_map(|f| BufReader::new(f).lines().map_while(Result::ok))
}

fn start_of_today() -> SystemTime {
    let midnight = Local::now().date_naive().and_hms_opt(0, 0, 0).unwrap();
    midnight
        .and_local_timezone(Local)
        .earliest()
        .map(SystemTime::from)
        .unwrap_or(SystemTime::UNIX_EPOCH)
}

// ponytail: full rescan each refresh. Add per-file (mtime, offset) cache when logs get large.
fn read_today() -> (Tokens, Tokens) {
    let Some(home) = std::env::home_dir() else {
        return Default::default();
    };
    let day = Local::now().date_naive();
    let since = start_of_today();

    let mut files = Vec::new();
    jsonl_files(&home.join(".claude/projects"), since, &mut files);
    let mut by_id: HashMap<String, Tokens> = HashMap::new();
    for f in &files {
        for line in lines(f) {
            if let Some((id, t)) = parse_claude_line(&line, day) {
                let e = by_id.entry(id).or_default();
                if t.total() > e.total() {
                    *e = t;
                }
            }
        }
    }
    let mut claude = Tokens::default();
    by_id.values().for_each(|t| claude.add(t));

    // ponytail: sums every token_count line. Forked/subagent rollouts replay parent turns
    // (original handles that in resolveCodexRollouts); port it if Codex totals look inflated.
    let mut files = Vec::new();
    jsonl_files(&home.join(".codex/sessions"), since, &mut files);
    let mut codex = Tokens::default();
    for f in &files {
        for line in lines(f) {
            if let Some(t) = parse_codex_line(&line, day) {
                codex.add(&t);
            }
        }
    }

    (claude, codex)
}


// ---- snapshot ----

fn snapshot_of(id: &str, name: &str, t: Tokens, day: &str) -> ProviderSnapshot {
    ProviderSnapshot {
        provider_id: id.into(),
        display_name: name.into(),
        today: Some(DailyUsage {
            date: day.into(),
            usage: Usage { tokens: t, total: t.total(), ..Default::default() },
            models: None,
        }),
        // ponytail: no price table yet, so cost stays hidden. Flip when ModelPricing is ported.
        reports_cost: false,
        ..Default::default()
    }
}

/// One full read. Today only for now; week/month/daily series are the providers area's job.
pub fn read_snapshot() -> UsageSnapshot {
    let day = crate::model::today_key();
    let (claude, codex) = read_today();
    UsageSnapshot {
        providers: vec![
            snapshot_of("claude_code", "Claude Code", claude, &day),
            snapshot_of("codex", "Codex", codex, &day),
        ],
        fetched_at: Some(Local::now().to_rfc3339()),
        error: None,
    }
}

// ---- area wiring (see ARCHITECTURE.md) ----

/// Keys this area owns inside `settings.json` (flattened into `settings::Settings`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProviderSettings {
    /// Opt-out list of provider ids, sorted.
    pub disabled_providers: Vec<String>,
    /// provider id → raw comma/newline separated folders.
    pub custom_scan_roots: std::collections::BTreeMap<String, String>,
    pub additional_claude_config_dirs: String,
}

impl ProviderSettings {
    pub fn sanitize(&mut self) {
        self.disabled_providers.sort();
        self.disabled_providers.dedup();
    }
}

type Shared = std::sync::Mutex<UsageSnapshot>;

pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    use tauri::Manager;
    app.manage::<Shared>(Default::default());
    Ok(())
}

/// Latest snapshot (empty before the first refresh).
pub fn current(app: &tauri::AppHandle) -> UsageSnapshot {
    use tauri::Manager;
    app.state::<Shared>().lock().unwrap().clone()
}

/// Called by the shell scheduler. Reads, stores and returns the new snapshot; the shell emits it.
pub fn refresh(app: &tauri::AppHandle) -> UsageSnapshot {
    use tauri::Manager;
    let snap = read_snapshot();
    *app.state::<Shared>().lock().unwrap() = snap.clone();
    snap
}

#[tauri::command]
fn usage_snapshot(app: tauri::AppHandle) -> UsageSnapshot {
    current(&app)
}

crate::area_commands![usage_snapshot];

#[cfg(test)]
mod tests {
    use super::*;

    fn today_ts() -> String {
        Local::now().to_rfc3339()
    }

    #[test]
    fn claude_line_parses_and_filters() {
        let day = Local::now().date_naive();
        let line = format!(
            r#"{{"type":"assistant","requestId":"r1","timestamp":"{}","message":{{"id":"m1","usage":{{"input_tokens":1,"output_tokens":2,"cache_creation_input_tokens":3,"cache_read_input_tokens":4}}}}}}"#,
            today_ts()
        );
        let (id, t) = parse_claude_line(&line, day).unwrap();
        assert_eq!(id, "m1|r1");
        assert_eq!(t.total(), 10);
        assert!(parse_claude_line(&line.replace("assistant", "user"), day).is_none());
        assert!(parse_claude_line(&line, day.succ_opt().unwrap()).is_none());
    }

    #[test]
    fn codex_line_splits_cached_input() {
        let day = Local::now().date_naive();
        let line = format!(
            r#"{{"timestamp":"{}","type":"event_msg","payload":{{"type":"token_count","info":{{"last_token_usage":{{"input_tokens":100,"cached_input_tokens":60,"output_tokens":5}}}}}}}}"#,
            today_ts()
        );
        let t = parse_codex_line(&line, day).unwrap();
        assert_eq!(
            t,
            Tokens {
                input: 40,
                output: 5,
                cache_write: 0,
                cache_read: 60
            }
        );
    }
}

/// `cargo test real_logs -- --ignored --nocapture` prints today's usage from this machine.
#[cfg(test)]
#[test]
#[ignore]
fn real_logs() {
    println!("{:?}", read_snapshot());
}
