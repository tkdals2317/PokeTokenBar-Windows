//! Codex limits (limits.md §3.5): `codex app-server --stdio` JSON-RPC `account/rateLimits/read`,
//! with the rollout-log snapshot as the fallback when no binary is installed.

use super::http::{self, LimitError, Result};
use super::Window;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(20);
/// Newest rollout files scanned for a fallback snapshot.
const ROLLOUTS_TO_SCAN: usize = 5;

/// What the Codex tab shows.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Status {
    pub windows: Vec<Window>,
    pub plan: Option<String>,
    pub limit_reached: bool,
}

fn window_span(minutes: Option<u64>) -> Option<f64> {
    minutes.filter(|m| *m > 0).map(|m| m as f64 * 60.0)
}

/// `limitName ?? limitId ?? "codex"`, `_` → space, first letter capitalized.
pub fn bucket_name(s: &Value) -> String {
    let raw = http::text(&s["limitName"]).or_else(|| http::text(&s["limitId"])).unwrap_or_else(|| "codex".into());
    let spaced = raw.replace('_', " ");
    let mut c = spaced.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

fn visible(s: &Value) -> bool {
    ["primary", "secondary", "individualLimit"].iter().any(|k| s[k].is_object())
}

/// `rateLimits` plus `rateLimitsByLimitId` sorted by key, minus duplicates of the top-level bucket.
pub fn snapshots(result: &Value) -> Vec<Value> {
    let top = &result["rateLimits"];
    let mut out = vec![top.clone()];
    let top_id = http::text(&top["limitId"]);
    let primary_key = top_id.clone().unwrap_or_else(|| "codex".into());
    if let Some(by) = result["rateLimitsByLimitId"].as_object() {
        let mut keys: Vec<&String> = by.keys().collect();
        keys.sort();
        for k in keys {
            let s = &by[k];
            if *k == primary_key || (top_id.is_some() && http::text(&s["limitId"]) == top_id) {
                continue;
            }
            out.push(s.clone());
        }
    }
    out
}

pub fn parse(result: &Value) -> Status {
    let snaps: Vec<Value> = snapshots(result).into_iter().filter(visible).collect();
    let mut windows = vec![];
    for s in &snaps {
        let bucket = http::text(&s["limitId"]).or_else(|| http::text(&s["limitName"])).unwrap_or_else(|| "codex".into());
        let name = bucket_name(s);
        for slot in ["primary", "secondary"] {
            let w = &s[slot];
            let Some(used) = w["usedPercent"].as_f64() else { continue };
            let minutes = w["windowDurationMins"].as_u64();
            windows.push(Window {
                key: format!("{bucket}.{slot}"),
                label: "codex".into(),
                arg: minutes.map(|m| m.to_string()),
                group: Some(name.clone()),
                used_percent: used,
                resets_at: w["resetsAt"].as_f64().and_then(http::epoch_secs).map(|d| d.to_rfc3339()),
                span_secs: window_span(minutes),
                ..Default::default()
            });
        }
        let ind = &s["individualLimit"];
        if let Some(remaining) = ind["remainingPercent"].as_f64() {
            windows.push(Window {
                key: format!("{bucket}.individual"),
                label: "personal".into(),
                group: Some(name.clone()),
                used_percent: (100.0 - remaining).clamp(0.0, 100.0),
                resets_at: ind["resetsAt"].as_f64().and_then(http::epoch_secs).map(|d| d.to_rfc3339()),
                detail: Some(format!(
                    "{} / {}",
                    http::text(&ind["used"]).unwrap_or_default(),
                    http::text(&ind["limit"]).unwrap_or_default()
                )),
                ..Default::default()
            });
        }
    }
    Status {
        plan: http::text(&result["rateLimits"]["planType"]).or_else(|| snaps.first().and_then(|s| http::text(&s["planType"]))),
        limit_reached: snaps.iter().any(|s| !s["rateLimitReachedType"].is_null()),
        windows,
    }
}

// ---- binary ----

fn candidates() -> Vec<PathBuf> {
    let home = http::home();
    let mut out: Vec<PathBuf> = vec![];
    let finder = if cfg!(windows) { "where" } else { "which" };
    if let Ok(o) = hidden(Command::new(finder).arg("codex")).output() {
        out.extend(String::from_utf8_lossy(&o.stdout).lines().map(|l| PathBuf::from(l.trim())).filter(|p| p.is_file()));
    }
    if cfg!(windows) {
        if let Some(appdata) = http::env("APPDATA") {
            out.push(Path::new(&appdata).join("npm").join("codex.cmd"));
        }
        if let Some(local) = http::env("LOCALAPPDATA") {
            out.push(Path::new(&local).join("Programs").join("codex").join("codex.exe"));
        }
        out.push(home.join(".codex").join("bin").join("codex.exe"));
    } else {
        out.push(home.join(".codex/bin/codex"));
        out.push("/opt/homebrew/bin/codex".into());
        out.push("/usr/local/bin/codex".into());
    }
    out
}

/// A runnable binary: `.exe` before `.cmd` shims, and never the extensionless npm shell script.
pub fn binary() -> Option<PathBuf> {
    let found: Vec<PathBuf> = candidates().into_iter().filter(|p| p.is_file()).collect();
    if !cfg!(windows) {
        return found.into_iter().next();
    }
    let ext = |p: &PathBuf, e: &str| p.extension().is_some_and(|x| x.eq_ignore_ascii_case(e));
    found.iter().find(|p| ext(p, "exe")).or_else(|| found.iter().find(|p| ext(p, "cmd"))).cloned()
}

fn hidden(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

fn request_lines() -> String {
    let msgs = [
        json!({"method": "initialize", "id": 0, "params": {
            "clientInfo": {"name": "token_mac", "title": "PokeTokenBar", "version": env!("CARGO_PKG_VERSION")},
            "capabilities": {"experimentalApi": true}}}),
        json!({"method": "initialized", "params": {}}),
        json!({"method": "account/rateLimits/read", "id": 1, "params": {}}),
    ];
    msgs.iter().map(|m| m.to_string() + "\n").collect()
}

fn kill_tree(child: &mut std::process::Child) {
    #[cfg(windows)]
    {
        let _ = hidden(Command::new("taskkill").args(["/T", "/F", "/PID", &child.id().to_string()])).output();
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// Runs `codex app-server --stdio` and returns the `result` of the request with `id: 1`.
fn rpc(bin: &Path) -> Result<Value> {
    let is_cmd = bin.extension().is_some_and(|x| x.eq_ignore_ascii_case("cmd"));
    let mut cmd = if is_cmd {
        let mut c = Command::new("cmd");
        c.arg("/C").arg(bin);
        c
    } else {
        Command::new(bin)
    };
    let mut child = hidden(cmd.args(["app-server", "--stdio"]))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| LimitError::Network)?;
    let _ = child.stdin.as_mut().map(|s| s.write_all(request_lines().as_bytes()));
    let stdout = child.stdout.take().ok_or(LimitError::Network)?;
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(std::result::Result::ok) {
            let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
            if v["id"] == 1 && tx.send(v).is_err() {
                return;
            }
        }
    });
    let reply = rx.recv_timeout(TIMEOUT);
    kill_tree(&mut child);
    let v = reply.map_err(|_| LimitError::Network)?;
    if v["result"].is_object() { Ok(v["result"].clone()) } else { Err(LimitError::Format) }
}

/// `Ok(None)`: no Codex installed and no rollout snapshot, so the section hides.
pub fn fetch() -> Result<Option<Status>> {
    match binary() {
        Some(bin) => rpc(&bin).map(|r| Some(parse(&r))),
        None => Ok(from_logs(&http::home().join(".codex").join("sessions"), chrono::Utc::now())),
    }
}

// ---- rollout-log fallback ----

fn newest_rollouts(root: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<(std::time::SystemTime, PathBuf)>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let p = e.path();
            match e.metadata() {
                Ok(m) if m.is_dir() => walk(&p, out),
                Ok(m) if p.extension().is_some_and(|x| x == "jsonl") => {
                    out.push((m.modified().unwrap_or(std::time::UNIX_EPOCH), p))
                }
                _ => {}
            }
        }
    }
    let mut files = vec![];
    walk(root, &mut files);
    files.sort_by(|a, b| b.0.cmp(&a.0));
    files.into_iter().take(ROLLOUTS_TO_SCAN).map(|(_, p)| p).collect()
}

pub fn from_logs(root: &Path, now: chrono::DateTime<chrono::Utc>) -> Option<Status> {
    for file in newest_rollouts(root) {
        let Ok(text) = std::fs::read_to_string(&file) else { continue };
        let latest = text
            .lines()
            .rev()
            .filter(|l| l.contains("\"rate_limits\""))
            .find_map(|l| parse_log_line(l, now).filter(|w| !w.is_empty()));
        if let Some(windows) = latest {
            return Some(Status { windows, ..Default::default() });
        }
    }
    None
}

/// A logged window whose reset already passed shows 0% (the snapshot predates the reset) and is
/// marked `expired`, so candy never reads that synthetic 0% as a real sub-100% sample (#434).
pub fn parse_log_line(line: &str, now: chrono::DateTime<chrono::Utc>) -> Option<Vec<Window>> {
    let v: Value = serde_json::from_str(line).ok()?;
    let limits = &v["payload"]["rate_limits"];
    let windows = ["primary", "secondary"]
        .iter()
        .filter_map(|slot| {
            let w = limits.get(*slot).filter(|w| w.is_object())?;
            let minutes = w["window_minutes"].as_u64()?;
            let resets = w["resets_at"].as_f64().and_then(http::epoch_secs);
            let expired = resets.is_some_and(|r| r <= now);
            Some(Window {
                key: format!("codex.{slot}"),
                label: "codex".into(),
                arg: Some(minutes.to_string()),
                group: Some("Codex".into()),
                used_percent: if expired { 0.0 } else { w["used_percent"].as_f64()? },
                resets_at: resets.filter(|_| !expired).map(|r| r.to_rfc3339()),
                span_secs: window_span(Some(minutes)),
                expired,
                ..Default::default()
            })
        })
        .collect();
    Some(windows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_buckets_and_personal_limit() {
        let r = json!({
            "rateLimits": {"limitId": "codex", "planType": "pro",
                "primary": {"usedPercent": 30, "windowDurationMins": 300, "resetsAt": 2000},
                "secondary": {"usedPercent": 12, "windowDurationMins": 10080}},
            "rateLimitsByLimitId": {
                "codex": {"limitId": "codex", "primary": {"usedPercent": 99}},
                "gpt_5_codex_other": {"limitId": "gpt_5_codex_other", "rateLimitReachedType": "primary",
                    "primary": {"usedPercent": 100, "windowDurationMins": 120}},
                "empty": {"limitId": "empty"},
                "spend": {"limitName": "team_spend", "individualLimit": {"limit": "$50", "remainingPercent": 120, "resetsAt": 3000, "used": "$0"}}
            }
        });
        let s = parse(&r);
        let rows: Vec<_> = s.windows.iter().map(|w| (w.key.as_str(), w.used_percent, w.arg.as_deref(), w.group.as_deref())).collect();
        assert_eq!(rows, [
            ("codex.primary", 30.0, Some("300"), Some("Codex")),
            ("codex.secondary", 12.0, Some("10080"), Some("Codex")),
            ("gpt_5_codex_other.primary", 100.0, Some("120"), Some("Gpt 5 codex other")),
            ("team_spend.individual", 0.0, None, Some("Team spend")),
        ]);
        assert_eq!(s.windows[0].span_secs, Some(18000.0));
        assert_eq!(s.windows[3].detail.as_deref(), Some("$0 / $50"));
        assert_eq!(s.windows[3].span_secs, None);
        assert_eq!(s.plan.as_deref(), Some("pro"));
        assert!(s.limit_reached);
    }

    #[test]
    fn log_fallback_marks_expired_windows() {
        let line = r#"{"type":"event_msg","payload":{"type":"token_count","rate_limits":{"primary":{"used_percent":30.0,"window_minutes":300,"resets_at":2000},"secondary":{"used_percent":100.0,"window_minutes":10080,"resets_at":1000}}}}"#;
        let now = chrono::DateTime::from_timestamp(1500, 0).unwrap();
        let w = parse_log_line(line, now).unwrap();
        assert_eq!((w[0].key.as_str(), w[0].used_percent, w[0].expired), ("codex.primary", 30.0, false));
        assert_eq!((w[1].used_percent, w[1].resets_at.clone(), w[1].expired), (0.0, None, true));
        let empty = r#"{"payload":{"rate_limits":{"primary":null,"secondary":null}}}"#;
        assert_eq!(parse_log_line(empty, now), Some(vec![]));
    }
}
