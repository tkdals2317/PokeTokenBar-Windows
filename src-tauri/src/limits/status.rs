//! Provider status banner (limits.md §3.10): statuspage.io components "Claude Code" and "Codex API".

use super::http::{self, Body};
use serde::Serialize;
use serde_json::Value;

const SOURCES: [(&str, &str, &str); 2] = [
    ("claude_code", "https://status.claude.com/api/v2/summary.json", "Claude Code"),
    ("codex", "https://status.openai.com/api/v2/summary.json", "Codex API"),
];

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProviderStatus {
    pub provider: String,
    /// operational | minor | major | critical | maintenance | unknown
    pub indicator: String,
    /// The component name as the status page spells it.
    pub description: String,
}

pub fn indicator(status: &str) -> &'static str {
    match status {
        "operational" => "operational",
        "degraded_performance" => "minor",
        "partial_outage" => "major",
        "major_outage" => "critical",
        "under_maintenance" => "maintenance",
        _ => "unknown",
    }
}

/// The named component (case-insensitive) of a statuspage summary.
pub fn parse(v: &Value, provider: &str, component: &str) -> Option<ProviderStatus> {
    let c = v["components"].as_array()?.iter().find(|c| c["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(component)))?;
    Some(ProviderStatus {
        provider: provider.into(),
        indicator: indicator(c["status"].as_str().unwrap_or("")).into(),
        description: http::text(&c["name"]).unwrap_or_default(),
    })
}

/// Both pages in parallel; a failed fetch is left out (the caller keeps the previous value).
pub fn fetch() -> Vec<ProviderStatus> {
    std::thread::scope(|s| {
        let handles: Vec<_> = SOURCES
            .iter()
            .map(|(id, url, name)| {
                s.spawn(move || {
                    let req = http::agent(10).get(url).set("User-Agent", "PokeTokenBar");
                    parse(&http::send(req, Body::Empty).ok()?.ok_json().ok()?, id, name)
                })
            })
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok().flatten()).collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn maps_component_status() {
        let v = json!({"components": [{"name": "claude code", "status": "partial_outage"}]});
        let s = parse(&v, "claude_code", "Claude Code").unwrap();
        assert_eq!((s.indicator.as_str(), s.description.as_str()), ("major", "claude code"));
        assert_eq!(indicator("degraded_performance"), "minor");
        assert_eq!(indicator("weird"), "unknown");
        assert!(parse(&v, "codex", "Codex API").is_none());
    }
}
