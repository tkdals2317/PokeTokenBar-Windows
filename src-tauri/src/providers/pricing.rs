//! Model price table (macOS `ModelPricing.swift`, ported verbatim). USD per million tokens:
//! input, output, 5-minute cache write, cache read. A 0 cache-write rate means "no supported
//! write rate", not free writes: any cache-write tokens make the estimate unavailable.

use std::collections::HashSet;
use std::sync::Mutex;

const TABLE: &[(&str, [f64; 4])] = &[
    ("claude-opus-5", [5.0, 25.0, 6.25, 0.5]),
    ("claude-opus-5-5", [4.0, 20.0, 5.0, 0.2]),
    ("claude-sonnet-5", [2.0, 10.0, 2.5, 0.2]),
    ("claude-sonnet-5-5", [2.0, 10.0, 2.5, 0.1]),
    ("claude-opus-4-20250514", [15.0, 75.0, 18.75, 1.5]),
    ("claude-sonnet-4-20250514", [3.0, 15.0, 3.75, 0.3]),
    ("claude-sonnet-4-5-20250929", [3.0, 15.0, 3.75, 0.3]),
    ("claude-opus-4-8", [5.0, 25.0, 6.25, 0.5]),
    ("claude-opus-4-7", [5.0, 25.0, 6.25, 0.5]),
    ("claude-sonnet-4-6", [3.0, 15.0, 3.75, 0.3]),
    ("claude-haiku-4-5-20251001", [1.0, 5.0, 1.25, 0.1]),
    ("claude-fable-5", [10.0, 50.0, 12.5, 1.0]),
    ("claude-fable-5-1", [10.0, 50.0, 12.5, 0.25]),
    ("gpt-6-astra", [10.0, 50.0, 12.5, 1.0]),
    ("gpt-6-sol", [2.0, 10.0, 2.5, 0.2]),
    ("gpt-6.1-sol", [2.0, 10.0, 2.5, 0.1]),
    ("gpt-6-luna", [0.1, 0.5, 0.125, 0.01]),
    ("gpt-5.6-sol", [4.0, 20.0, 5.0, 0.4]),
    ("gpt-5.6-terra", [2.0, 12.0, 2.5, 0.2]),
    ("gpt-5.6-luna", [0.2, 1.2, 0.25, 0.02]),
    ("gpt-5", [1.25, 10.0, 0.0, 0.125]),
    ("gpt-5-codex", [1.25, 10.0, 0.0, 0.125]),
    ("gpt-5.1", [1.25, 10.0, 0.0, 0.125]),
    ("gpt-5.1-codex", [1.25, 10.0, 0.0, 0.125]),
    ("gpt-5.2", [1.75, 14.0, 0.0, 0.175]),
    ("gpt-5.2-codex", [1.75, 14.0, 0.0, 0.175]),
    ("gpt-5.3-codex", [1.75, 14.0, 0.0, 0.175]),
    ("gpt-5.4", [2.5, 15.0, 0.0, 0.25]),
    ("gpt-5.5", [5.0, 30.0, 0.0, 0.5]),
    ("gemini-2.5-pro", [1.25, 10.0, 0.0, 0.125]),
    ("gemini-2.5-flash", [0.30, 2.5, 0.0, 0.03]),
    ("gemini-2.5-flash-lite", [0.10, 0.40, 0.0, 0.01]),
    ("gemini-2.0-flash", [0.10, 0.4, 0.0, 0.025]),
];

/// Documented model identities only; no family or substring fallback.
const ALIASES: &[(&str, &str)] = &[
    ("gpt-5.6", "gpt-5.6-sol"),
    ("claude-sonnet-4", "claude-sonnet-4-20250514"),
    ("claude-opus-4", "claude-opus-4-20250514"),
    ("claude-sonnet-4-5", "claude-sonnet-4-5-20250929"),
    ("claude-haiku-4-5", "claude-haiku-4-5-20251001"),
    ("gpt-5-2025-08-07", "gpt-5"),
    ("gpt-5.1-2025-11-13", "gpt-5.1"),
    ("gpt-5.2-2025-12-11", "gpt-5.2"),
    ("gpt-5.4-2026-03-05", "gpt-5.4"),
    ("gpt-5.5-2026-04-23", "gpt-5.5"),
];

const LONG_CONTEXT_272K: &[&str] = &[
    "gpt-6-astra", "gpt-6-sol", "gpt-6.1-sol", "gpt-6-luna", "gpt-5.6-sol", "gpt-5.6-terra", "gpt-5.6-luna",
    "gpt-5.5", "gpt-5.4",
];

pub fn model_key(model: &str) -> String {
    let mut key = model.trim().to_lowercase();
    if let Some(p) = ["openai/", "anthropic/", "google/", "models/"].iter().find(|p| key.starts_with(*p)) {
        key = key[p.len()..].to_string();
    }
    ALIASES.iter().find(|(a, _)| *a == key).map_or(key, |(_, to)| to.to_string())
}

/// One request's buckets. Long context is judged per request, never on aggregates.
pub fn estimated_cost(model: &str, input: u64, output: u64, cache_write: u64, cache_read: u64) -> Option<f64> {
    let key = model_key(model);
    let r = TABLE.iter().find(|(k, _)| *k == key)?.1;
    if cache_write > 0 && r[2] <= 0.0 {
        return None;
    }
    let prompt = (input + cache_read + cache_write) as f64;
    let long = LONG_CONTEXT_272K.contains(&key.as_str()) && prompt > 272_000.0
        || key == "gemini-2.5-pro" && prompt > 200_000.0;
    let (in_mul, out_mul) = if long { (2.0, 1.5) } else { (1.0, 1.0) };
    let m = 1_000_000.0;
    Some(
        (input as f64 * r[0] / m + cache_write as f64 * r[2] / m + cache_read as f64 * r[3] / m) * in_mul
            + output as f64 * r[1] / m * out_mul,
    )
}

static UNPRICED: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// Logs each unpriced model once per process.
pub fn note_unpriced(model: &str) {
    let key = model_key(model);
    let mut seen = UNPRICED.lock().unwrap();
    if seen.get_or_insert_with(HashSet::new).insert(key) {
        eprintln!("Unpriced model: {model} — its cost shows as Unavailable until ModelPricing.table has a row");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Option<f64>, b: f64) -> bool {
        a.is_some_and(|a| (a - b).abs() < 1e-9)
    }

    #[test]
    fn normalizes_prefix_and_alias() {
        assert_eq!(model_key("  Anthropic/claude-sonnet-4-5 "), "claude-sonnet-4-5-20250929");
        assert_eq!(model_key("openai/models/gpt-5"), "models/gpt-5", "only one prefix is stripped");
        assert!(estimated_cost("claude-opus-5[1m]", 1, 0, 0, 0).is_none(), "no substring fallback");
    }

    #[test]
    fn formula_and_long_context() {
        assert!(close(estimated_cost("claude-opus-5", 1_000_000, 1_000_000, 1_000_000, 1_000_000), 36.75));
        // gpt-5 has no write rate.
        assert!(estimated_cost("gpt-5", 10, 10, 1, 0).is_none());
        assert!(close(estimated_cost("gpt-5.4", 272_000, 0, 0, 0), 0.68));
        assert!(close(estimated_cost("gpt-5.4", 272_001, 1_000_000, 0, 0), 272_001.0 * 2.5 / 1e6 * 2.0 + 22.5));
        assert!(close(estimated_cost("gemini-2.5-pro", 200_001, 0, 0, 0), 200_001.0 * 1.25 / 1e6 * 2.0));
        assert!(close(estimated_cost("gpt-5.6", 100_000, 0, 0, 0), 0.4), "alias to gpt-5.6-sol");
    }
}
