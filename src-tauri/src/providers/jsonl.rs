//! JSONL session parsers: Claude Code, Gemini, Grok, Pi, omp, omo/senpi, Kimi Code
//! (providers.md §4.1, §4.3, §4.8, §4.11-4.13, §4.15). Codex lives in `codex.rs`.

use super::entry::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

fn uuid() -> String {
    format!("{:016x}{:016x}", fastrand::u64(..), fastrand::u64(..))
}

// ---- Claude ----

/// Last `type:"cost-state"` ledger of a transcript: base model → reported USD.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ClaudeCostState {
    pub cost_by_model: BTreeMap<String, f64>,
    pub has_unknown_model_cost: bool,
}

/// `claude-opus-5[1m]` → `claude-opus-5`.
pub fn claude_cost_model_key(model: &str) -> &str {
    model.split('[').next().unwrap_or(model)
}

pub fn parse_cost_state(line: &str) -> Option<ClaudeCostState> {
    let obj: Value = serde_json::from_str(line).ok()?;
    if str_of(&obj, "type")? != "cost-state" {
        return None;
    }
    let usage = obj.get("modelUsage")?.as_object()?;
    let mut s = ClaudeCostState { has_unknown_model_cost: bool_value(obj.get("hasUnknownModelCost")), ..Default::default() };
    for (model, raw) in usage {
        let Some(cost) = double_or_nil(raw.get("costUSD")).filter(|c| *c >= 0.0) else { continue };
        *s.cost_by_model.entry(claude_cost_model_key(model).to_string()).or_default() += cost;
    }
    (!s.cost_by_model.is_empty()).then_some(s)
}

/// Spreads each model's session total over that model's entries by tokens; the last absorbs the remainder.
pub fn apply_reported_cost(mut entries: Vec<Entry>, state: &ClaudeCostState) -> Vec<Entry> {
    let mut idx_by_model: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, e) in entries.iter().enumerate().filter(|(_, e)| e.total() > 0) {
        idx_by_model.entry(claude_cost_model_key(&e.model).to_string()).or_default().push(i);
    }
    for (model, cost) in &state.cost_by_model {
        let Some(idx) = idx_by_model.get(model) else { continue };
        let total: u64 = idx.iter().map(|&i| entries[i].total()).sum();
        if total == 0 {
            continue;
        }
        let mut remaining = *cost;
        for (n, &i) in idx.iter().enumerate() {
            let share = if n == idx.len() - 1 { remaining } else { cost * entries[i].total() as f64 / total as f64 };
            entries[i].explicit_cost = Some(share.max(0.0));
            entries[i].cost_is_estimate = Some(false);
            remaining -= share;
        }
    }
    entries
}

fn parse_claude_line(line: &str) -> Option<Entry> {
    let obj: Value = serde_json::from_str(line).ok()?;
    if str_of(&obj, "type")? != "assistant" {
        return None;
    }
    let msg = obj.get("message")?;
    let usage = msg.get("usage").filter(|u| u.is_object())?;
    let date = parse_iso(str_of(&obj, "timestamp")?)?;
    let id = format!("{}|{}", str_of(msg, "id").unwrap_or(""), str_of(&obj, "requestId").unwrap_or(""));
    Some(Entry::new(
        id,
        date,
        str_of(msg, "model").unwrap_or("unknown"),
        [
            int_value(usage.get("input_tokens")),
            int_value(usage.get("output_tokens")),
            int_value(usage.get("cache_creation_input_tokens")),
            int_value(usage.get("cache_read_input_tokens")),
        ],
    ))
}

pub struct ClaudeParsed {
    pub entries: Vec<Entry>,
    /// Bytes up to and including the last `\n`.
    pub consumed: u64,
    pub cost_state: Option<ClaudeCostState>,
}

/// Byte-level line scan. A trailing unterminated line is parsed but not consumed.
pub fn parse_claude_data(data: &[u8]) -> ClaudeParsed {
    let mut out = Vec::new();
    let mut cost_state = None;
    let mut consumed = 0;
    let mut parse = |line: &[u8]| {
        if contains(line, b"\"cost-state\"") {
            if let Some(s) = std::str::from_utf8(line).ok().and_then(parse_cost_state) {
                cost_state = Some(s);
            }
            return;
        }
        if line.is_empty() || !contains(line, b"\"usage\"") || !contains(line, b"\"assistant\"") {
            return;
        }
        if let Some(e) = std::str::from_utf8(line).ok().and_then(parse_claude_line) {
            out.push(e);
        }
    };
    let mut start = 0;
    while let Some(nl) = data[start..].iter().position(|&b| b == b'\n') {
        parse(&data[start..start + nl]);
        start += nl + 1;
        consumed = start;
    }
    if start < data.len() {
        parse(&data[start..]);
    }
    ClaudeParsed { entries: dedup_keep_max(out), consumed: consumed as u64, cost_state }
}

/// `<project>/<session>.jsonl` → session; `<project>/<session>/subagents/<a>.jsonl` → parent session.
pub fn claude_session_id(path: &Path) -> Option<String> {
    let parent = path.parent()?;
    let name = if parent.file_name().is_some_and(|n| n == "subagents") {
        parent.parent()?.file_name()?.to_string_lossy().into_owned()
    } else {
        path.file_stem()?.to_string_lossy().into_owned()
    };
    (!name.is_empty()).then_some(name)
}

/// Parses the bytes after `offset`. `None` = unreadable.
pub fn parse_claude_file(path: &Path, offset: u64) -> Option<ClaudeParsed> {
    let mut f = fs::File::open(path).ok()?;
    f.seek(SeekFrom::Start(offset)).ok()?;
    let mut data = Vec::new();
    f.read_to_end(&mut data).ok()?;
    let mut parsed = parse_claude_data(&data);
    let session = claude_session_id(path);
    parsed.entries.iter_mut().for_each(|e| e.session_id = session.clone());
    parsed.consumed += offset;
    Some(parsed)
}

// ---- Gemini ----

pub fn parse_gemini_file(path: &Path) -> Option<Vec<Entry>> {
    let data = fs::read(path).ok()?;
    let file = super::roots::file_name(path);
    let mut order: Vec<String> = Vec::new();
    let mut by_id: HashMap<String, Entry> = HashMap::new();
    let mut absorb = |obj: &Value, fallback: Option<Ts>| {
        let Some(tokens) = obj.get("tokens").filter(|t| t.is_object()) else { return };
        let id = str_of(obj, "id").map(String::from).unwrap_or_else(uuid);
        let Some(date) = str_of(obj, "timestamp").and_then(parse_iso).or(fallback) else { return };
        let input = int_value(tokens.get("input"));
        let cached = int_value(tokens.get("cached"));
        let e = Entry::new(
            format!("gemini|{file}|{id}"),
            date,
            str_of(obj, "model").unwrap_or("gemini"),
            [
                input.saturating_sub(cached) + int_value(tokens.get("tool")),
                int_value(tokens.get("output")) + int_value(tokens.get("thoughts")),
                0,
                cached,
            ],
        );
        if !by_id.contains_key(&id) {
            order.push(id.clone());
        }
        by_id.insert(id, e); // the update record is final
    };
    if super::roots::ext_is(path, "jsonl") {
        let text = std::str::from_utf8(&data).ok()?;
        let mut last_ts = None;
        for line in text.split('\n').filter(|l| !l.is_empty()) {
            if !line.contains("\"tokens\"") && !line.contains("\"timestamp\"") {
                continue;
            }
            let Ok(obj) = serde_json::from_str::<Value>(line) else { continue };
            if let Some(ts) = str_of(&obj, "timestamp").and_then(parse_iso) {
                last_ts = Some(ts);
            }
            absorb(&obj, last_ts);
        }
    } else {
        let Ok(obj) = serde_json::from_slice::<Value>(&data) else { return Some(Vec::new()) };
        let Some(messages) = obj.get("messages").and_then(Value::as_array) else { return Some(Vec::new()) };
        let start = str_of(&obj, "startTime").and_then(parse_iso);
        for m in messages {
            absorb(m, start);
        }
    }
    Some(order.into_iter().filter_map(|id| by_id.remove(&id)).collect())
}

// ---- Grok ----

/// `updates.jsonl` of a session whose `summary.json.session_kind` is not a subagent kind.
pub fn is_grok_usage_file(path: &Path) -> bool {
    if super::roots::file_name(path) != "updates.jsonl" {
        return false;
    }
    let kind = path
        .parent()
        .and_then(|d| super::roots::read_json(&d.join("summary.json")))
        .and_then(|j| str_of(&j, "session_kind").map(String::from));
    !kind.is_some_and(|k| k.starts_with("subagent"))
}

fn grok_date(envelope: &Value, meta: Option<&Value>) -> Option<Ts> {
    let ms = double_or_nil(meta.and_then(|m| m.get("agentTimestampMs"))).unwrap_or(0.0);
    if ms > 0.0 {
        return Some(ms / 1000.0);
    }
    let raw = double_or_nil(envelope.get("timestamp")).unwrap_or(0.0);
    if raw > 0.0 {
        return epoch_secs(raw);
    }
    str_of(envelope, "timestamp").and_then(parse_iso)
}

fn grok_model(usage: &Value) -> Option<String> {
    let by_model = usage.get("modelUsage").or_else(|| usage.get("model_usage"))?.as_object()?;
    let mut keys: Vec<&String> = by_model.keys().collect();
    keys.sort();
    let mut best: Option<(&String, u64)> = None;
    for k in keys {
        let f = &by_model[k];
        let t = int_or_nil(f.get("totalTokens")).or_else(|| int_or_nil(f.get("total_tokens"))).unwrap_or(0);
        if best.is_none_or(|(_, b)| t > b) {
            best = Some((k, t));
        }
    }
    best.map(|(m, _)| m.trim().to_string()).filter(|m| !m.is_empty())
}

fn grok_cost(usage: &Value) -> Option<f64> {
    let flag = |a: &str, b: &str| bool_value(usage.get(a)) || bool_value(usage.get(b));
    if flag("usageIsIncomplete", "usage_is_incomplete") || flag("costIsPartial", "cost_is_partial") {
        return None;
    }
    let ticks = double_or_nil(usage.get("costUsdTicks")).or_else(|| double_or_nil(usage.get("cost_usd_ticks")))?;
    (ticks >= 0.0).then(|| ticks / 1e10)
}

pub fn parse_grok_line(line: &str) -> Option<Entry> {
    let envelope: Value = serde_json::from_str(line).ok()?;
    let notification = envelope.get("params").filter(|p| p.is_object()).unwrap_or(&envelope);
    let update = notification.get("update")?;
    if str_of(update, "sessionUpdate")? != "turn_completed" {
        return None;
    }
    let usage = update.get("usage").filter(|u| u.is_object())?;
    let meta = notification.get("_meta");
    if bool_value(meta.and_then(|m| m.get("isReplay"))) {
        return None;
    }
    let turn = non_empty(update.get("prompt_id"))?;
    let date = grok_date(&envelope, meta)?;
    let pick = |a: &str, b: &str| int_or_nil(usage.get(a)).or_else(|| int_or_nil(usage.get(b)));
    let mut output = pick("outputTokens", "output_tokens").unwrap_or(0);
    let cr = pick("cachedReadTokens", "cached_read_tokens").unwrap_or(0);
    let (input, cache_read) = match int_or_nil(usage.get("inputTokens")) {
        Some(full) => {
            let c = cr.min(full);
            (full - c, c)
        }
        None => (int_or_nil(usage.get("input_tokens")).unwrap_or(0), cr),
    };
    if let Some(total) = pick("totalTokens", "total_tokens") {
        let parts = input + output + cache_read;
        if total > parts {
            output += total - parts;
        }
    }
    if input + output + cache_read == 0 {
        return None;
    }
    let mut e = Entry::new(format!("grok|{turn}"), date, grok_model(usage).unwrap_or_else(|| "grok".into()), [input, output, 0, cache_read]);
    e.explicit_cost = grok_cost(usage);
    Some(e)
}

pub fn parse_grok_file(path: &Path) -> Option<Vec<Entry>> {
    let text = fs::read_to_string(path).ok()?;
    Some(dedup_keep_max(text.split('\n').filter(|l| l.contains("turn_completed")).filter_map(parse_grok_line).collect()))
}

// ---- Pi / omp / omo (same session format) ----

#[derive(Clone, Copy, PartialEq)]
pub enum PiFlavor {
    Pi,
    Omp,
    Omo,
}

fn pi_envelope_date(env: &Value) -> Option<Ts> {
    str_of(env, "timestamp").and_then(parse_iso)
}

fn pi_message_date(msg: &Value, env: &Value) -> Option<Ts> {
    match double_or_nil(msg.get("timestamp")) {
        Some(ms) if ms > 0.0 => Some(ms / 1000.0),
        _ => pi_envelope_date(env),
    }
}

fn aborted(msg: &Value) -> bool {
    matches!(str_of(msg, "stopReason"), Some("aborted" | "error"))
}

fn pi_line(line: &str, flavor: PiFlavor, file: &str) -> Option<Entry> {
    let env: Value = serde_json::from_str(line).ok()?;
    let kind = str_of(&env, "type")?;
    let default_model = match flavor {
        PiFlavor::Pi => "pi",
        PiFlavor::Omp => "omp",
        PiFlavor::Omo => "omo",
    };
    let (usage, date, model) = match kind {
        "message" => {
            let msg = env.get("message")?;
            let role = str_of(msg, "role");
            let usage = msg.get("usage").filter(|u| u.is_object());
            let ok = match flavor {
                PiFlavor::Pi => !aborted(msg),
                PiFlavor::Omp => role == Some("assistant") && !aborted(msg),
                PiFlavor::Omo => (role == Some("assistant") && !aborted(msg)) || (role == Some("toolResult") && usage.is_some()),
            };
            if !ok {
                return None;
            }
            let model = match flavor {
                PiFlavor::Pi => str_of(&env, "model").or_else(|| str_of(msg, "model")),
                _ => str_of(msg, "model"),
            }
            .unwrap_or(default_model);
            (usage?.clone(), pi_message_date(msg, &env), model.to_string())
        }
        "compaction" | "branch_summary" => {
            let usage = env.get("usage").filter(|u| u.is_object()).cloned();
            (usage.unwrap_or(Value::Object(Default::default())), pi_envelope_date(&env), default_model.to_string())
        }
        _ => return None,
    };
    let date = date?;
    if flavor != PiFlavor::Pi && usage.as_object().is_some_and(|o| o.is_empty()) {
        return None;
    }
    let id = match flavor {
        PiFlavor::Pi => str_of(&env, "id").filter(|s| !s.is_empty())?.to_string(),
        PiFlavor::Omp => format!("omp|{file}|{}", str_of(&env, "id").map(String::from).unwrap_or_else(uuid)),
        PiFlavor::Omo => format!("omo|{file}|{}", str_of(&env, "id").map(String::from).unwrap_or_else(uuid)),
    };
    let cost = double_or_nil(usage.get("cost").and_then(|c| c.get("total")));
    let cost = if flavor == PiFlavor::Pi { cost } else { cost.filter(|c| *c >= 0.0) };
    let mut names = vec!["input", "output", "cacheWrite", "cacheRead"];
    if flavor == PiFlavor::Omo {
        names.push("cacheWrite1h");
    }
    let granular = names.iter().any(|n| int_or_nil(usage.get(*n)).is_some());
    let g = |n: &str| int_or_nil(usage.get(n)).unwrap_or(0);
    let mut e = if granular {
        let cw = g("cacheWrite") + if flavor == PiFlavor::Omo { g("cacheWrite1h") } else { 0 };
        Entry::new(id, date, model, [g("input"), g("output"), cw, g("cacheRead")])
    } else {
        let total = int_or_nil(usage.get("totalTokens"))?;
        let mut e = Entry::new(id, date, model, [total, 0, 0, 0]);
        e.cost_unavailable = Some(true);
        e
    };
    if flavor == PiFlavor::Pi {
        e.cost_unavailable = Some(!granular);
    }
    e.explicit_cost = cost;
    e.cost_is_estimate = Some(true);
    Some(e)
}

/// `bridge/` copies are conversions of other sessions (omp only).
pub fn is_omp_usage_file(path: &Path) -> bool {
    !path.components().any(|c| c.as_os_str() == "bridge")
}

pub fn parse_pi_file(path: &Path, flavor: PiFlavor) -> Option<Vec<Entry>> {
    let text = fs::read_to_string(path).ok()?;
    let file = match flavor {
        PiFlavor::Omo => match path.parent().and_then(Path::file_name) {
            Some(p) => format!("{}/{}", p.to_string_lossy(), super::roots::file_name(path)),
            None => super::roots::file_name(path),
        },
        _ => super::roots::file_name(path),
    };
    Some(dedup_keep_max(
        text.split('\n').filter(|l| !l.is_empty() && l.contains("\"usage\"")).filter_map(|l| pi_line(l, flavor, &file)).collect(),
    ))
}

// ---- Kimi Code ----

pub fn is_kimi_usage_file(path: &Path) -> bool {
    super::roots::file_name(path) == "wire.jsonl"
}

fn kimi_line(line: &str) -> Option<Entry> {
    let r: Value = serde_json::from_str(line).ok()?;
    if str_of(&r, "type")? != "usage.record" {
        return None;
    }
    let usage = r.get("usage").filter(|u| u.is_object())?;
    let ms = double_or_nil(r.get("time")).filter(|m| *m > 0.0)?;
    let scope = str_of(&r, "usageScope");
    if !matches!(scope, None | Some("turn" | "session")) {
        return None;
    }
    let g = |n: &str| int_or_nil(usage.get(n)).unwrap_or(0);
    let (i, o, cr, cw) = (g("inputOther"), g("output"), g("inputCacheRead"), g("inputCacheCreation"));
    let model = str_of(&r, "model").filter(|m| !m.is_empty()).unwrap_or("kimi-code");
    let id = format!("kimi|{}|{model}|{}|{i}|{o}|{cr}|{cw}", ms as i64, scope.unwrap_or("-"));
    Some(Entry::new(id, ms / 1000.0, model, [i, o, cw, cr]))
}

pub fn parse_kimi_file(path: &Path) -> Option<Vec<Entry>> {
    let text = fs::read_to_string(path).ok()?;
    Some(text.split('\n').filter(|l| l.contains("\"usage.record\"")).filter_map(kimi_line).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TS: &str = "2026-03-01T10:00:00.123456Z";

    fn claude(id: &str, req: &str, model: &str, out: u64) -> String {
        format!(
            r#"{{"type":"assistant","requestId":"{req}","timestamp":"{TS}","message":{{"id":"{id}","model":"{model}","usage":{{"input_tokens":1,"output_tokens":{out},"cache_creation_input_tokens":3,"cache_read_input_tokens":4}}}}}}"#
        )
    }

    #[test]
    fn claude_keeps_max_and_leaves_partial_line_unconsumed() {
        let data = format!("{}\n{}\n{{\"type\":\"user\"}}\n{}", claude("m", "r", "x", 2), claude("m", "r", "x", 20), claude("m2", "r", "x", 1));
        let p = parse_claude_data(data.as_bytes());
        assert_eq!(p.entries.len(), 2);
        assert_eq!(p.entries[0].output, 20);
        assert_eq!(p.entries[0].id, "m|r");
        assert_eq!(p.consumed as usize, data.rfind('\n').unwrap() + 1);
    }

    #[test]
    fn claude_cost_ledger_spreads_by_tokens() {
        let ledger = r#"{"type":"cost-state","modelUsage":{"claude-opus-5[1m]":{"costUSD":1.0},"claude-opus-5":{"costUSD":2.0},"ghost":{"costUSD":9},"bad":{"costUSD":-1}},"hasUnknownModelCost":true}"#;
        let s = parse_cost_state(ledger).unwrap();
        assert_eq!(s.cost_by_model.get("claude-opus-5"), Some(&3.0));
        assert!(s.has_unknown_model_cost && !s.cost_by_model.contains_key("bad"));
        let data = format!("{}\n{}\n{ledger}\n", claude("a", "1", "claude-opus-5", 2), claude("b", "1", "claude-opus-5", 12));
        let p = parse_claude_data(data.as_bytes());
        let out = apply_reported_cost(p.entries, p.cost_state.as_ref().unwrap());
        let costs: Vec<f64> = out.iter().map(|e| e.explicit_cost.unwrap()).collect();
        assert!((costs[0] - 1.0).abs() < 1e-9 && (costs[1] - 2.0).abs() < 1e-9);
        assert_eq!(out[0].cost_is_estimate, Some(false));
    }

    #[test]
    fn claude_session_from_path() {
        assert_eq!(claude_session_id(Path::new("/p/proj/abc.jsonl")).as_deref(), Some("abc"));
        assert_eq!(claude_session_id(Path::new("/p/proj/abc/subagents/x.jsonl")).as_deref(), Some("abc"));
    }

    #[test]
    fn grok_mapping() {
        let line = r#"{"timestamp":1700000000,"method":"_x.ai/session/update","params":{"sessionId":"s","update":{"sessionUpdate":"turn_completed","prompt_id":"p1","usage":{"inputTokens":100,"cachedReadTokens":150,"outputTokens":5,"totalTokens":120,"costUsdTicks":20000000000,"modelUsage":{"b":{"totalTokens":5},"a":{"totalTokens":5}}}},"_meta":{"agentTimestampMs":1600000000000}}}"#;
        let e = parse_grok_line(line).unwrap();
        assert_eq!((e.input, e.cache_read, e.output), (0, 100, 20));
        assert_eq!((e.id.as_str(), e.model.as_str(), e.date), ("grok|p1", "a", 1_600_000_000.0));
        assert_eq!(e.explicit_cost, Some(2.0));
        let snake = r#"{"update":{"sessionUpdate":"turn_completed","prompt_id":"p2","usage":{"input_tokens":10,"cached_read_tokens":4,"output_tokens":1,"usage_is_incomplete":true}},"timestamp":"2026-01-01T00:00:00Z"}"#;
        let e = parse_grok_line(snake).unwrap();
        assert_eq!((e.input, e.cache_read, e.explicit_cost), (10, 4, None));
        assert!(parse_grok_line(&snake.replace("\"usage\":{", "\"_meta\":{\"isReplay\":true},\"usage\":{")).is_some());
    }

    #[test]
    fn pi_family() {
        let pi = r#"{"type":"message","id":"e1","timestamp":"2026-01-01T00:00:00Z","model":"env-model","message":{"role":"assistant","timestamp":1700000000000,"usage":{"input":1,"output":2,"cacheRead":3,"cost":{"total":0.5}}}}"#;
        let e = pi_line(pi, PiFlavor::Pi, "f").unwrap();
        assert_eq!((e.id.as_str(), e.model.as_str(), e.total(), e.date), ("e1", "env-model", 6, 1_700_000_000.0));
        assert_eq!((e.explicit_cost, e.cost_is_estimate, e.cost_unavailable), (Some(0.5), Some(true), Some(false)));
        let total_only = r#"{"type":"compaction","id":"c","timestamp":"2026-01-01T00:00:00Z","usage":{"totalTokens":9}}"#;
        assert_eq!(pi_line(total_only, PiFlavor::Pi, "f").unwrap().cost_unavailable, Some(true));
        assert!(pi_line(&pi.replace("assistant\"", "assistant\",\"stopReason\":\"error\""), PiFlavor::Pi, "f").is_none());
        // omp: assistant only, model from message, file-scoped id.
        let e = pi_line(&pi.replace("\"role\":\"assistant\"", "\"role\":\"assistant\",\"model\":\"m\""), PiFlavor::Omp, "f.jsonl").unwrap();
        assert_eq!((e.id.as_str(), e.model.as_str()), ("omp|f.jsonl|e1", "m"));
        assert!(pi_line(&pi.replace("assistant", "toolResult"), PiFlavor::Omp, "f").is_none());
        let tool = pi_line(&pi.replace("assistant", "toolResult").replace("\"cacheRead\":3", "\"cacheWrite\":1,\"cacheWrite1h\":2"), PiFlavor::Omo, "d/f").unwrap();
        assert_eq!((tool.cache_write, tool.model.as_str()), (3, "omo"));
        assert!(!is_omp_usage_file(Path::new("/a/bridge/x.jsonl")));
    }

    #[test]
    fn kimi_content_id_and_scope() {
        let l = r#"{"type":"usage.record","time":1700000000000,"model":"k2","usageScope":"turn","usage":{"inputOther":1,"output":2,"inputCacheRead":3,"inputCacheCreation":4}}"#;
        let e = kimi_line(l).unwrap();
        assert_eq!(e.id, "kimi|1700000000000|k2|turn|1|2|3|4");
        assert_eq!((e.cache_write, e.cache_read), (4, 3));
        assert!(kimi_line(&l.replace("\"turn\"", "\"future\"")).is_none());
    }

    #[test]
    fn gemini_update_record_wins() {
        let dir = std::env::temp_dir().join(format!("ptb-gemini-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let f = dir.join("session-1.jsonl");
        fs::write(&f, concat!(
            r#"{"timestamp":"2026-01-01T00:00:00Z","type":"user"}"#, "\n",
            r#"{"id":"g1","type":"gemini","model":"gemini-2.5-pro","tokens":{"input":10,"cached":4,"output":1,"thoughts":2,"tool":1}}"#, "\n",
            r#"{"id":"g1","type":"message_update","tokens":{"input":20,"cached":4,"output":3,"thoughts":2,"tool":1}}"#, "\n",
        )).unwrap();
        let out = parse_gemini_file(&f).unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!((out[0].input, out[0].output, out[0].cache_read), (17, 5, 4));
        assert_eq!(out[0].model, "gemini", "update record without model resets to the default");
        assert_eq!(out[0].id, "gemini|session-1.jsonl|g1");
    }
}
