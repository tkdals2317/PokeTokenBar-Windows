//! Grok credit limits (limits.md §3.9): `cli-chat-proxy.grok.com/v1/billing?format=credits`
//! with the Grok CLI token from `$GROK_HOME/auth.json` (never refreshed by the app).

use super::http::{self, Body, Result};
use super::Window;
use serde::Serialize;
use serde_json::Value;

const URL: &str = "https://cli-chat-proxy.grok.com/v1/billing?format=credits";

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Product {
    pub name: String,
    pub percent: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Status {
    pub windows: Vec<Window>,
    pub products: Vec<Product>,
}

/// The `key` of the entry with the latest `expires_at` (missing or invalid dates sort first).
pub fn token(auth: &Value) -> Option<String> {
    auth.as_object()?
        .values()
        .filter_map(|e| Some((e["expires_at"].as_str().and_then(http::iso), http::text(&e["key"])?)))
        .max_by_key(|(d, _)| *d)
        .map(|(_, k)| k)
}

fn product_name(p: &str) -> String {
    match p {
        "GrokBot" => "Grok".into(),
        "GrokBuild" => "Grok Build".into(),
        "Api" => "API".into(),
        other => other.into(),
    }
}

/// After the period `end`, utilization is 0, no reset and no products.
pub fn parse(v: &Value, now: chrono::DateTime<chrono::Utc>) -> Option<Status> {
    let c = &v["config"];
    let pct = c["creditUsagePercent"].as_f64()?;
    let p = &c["currentPeriod"];
    let (start, end) = (p["start"].as_str().and_then(http::iso), p["end"].as_str().and_then(http::iso));
    let ended = end.is_some_and(|e| e <= now);
    let span = match (start, end) {
        (Some(s), Some(e)) if e > s => Some((e - s).num_seconds() as f64),
        _ => None,
    };
    let period = match p["type"].as_str() {
        Some("USAGE_PERIOD_TYPE_WEEKLY") => Some("weekly".to_string()),
        Some("USAGE_PERIOD_TYPE_MONTHLY") => Some("monthly".to_string()),
        _ => None,
    };
    let products = if ended {
        vec![]
    } else {
        c["productUsage"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|u| Some(Product { name: product_name(u["product"].as_str()?), percent: u["usagePercent"].as_f64()? }))
            .filter(|p| p.percent > 0.0)
            .collect()
    };
    Some(Status {
        windows: vec![Window {
            key: "credits".into(),
            label: "credits".into(),
            arg: period,
            used_percent: if ended { 0.0 } else { pct.max(0.0) },
            resets_at: end.filter(|_| !ended).map(|d| d.to_rfc3339()),
            span_secs: span,
            ..Default::default()
        }],
        products,
    })
}

/// `Ok(None)`: no Grok CLI login, the section hides.
pub fn fetch() -> Result<Option<Status>> {
    let home = http::env("GROK_HOME").map(|p| http::expand(&p)).unwrap_or_else(|| http::home().join(".grok"));
    let Some(key) = std::fs::read(home.join("auth.json")).ok().and_then(|b| token(&serde_json::from_slice(&b).ok()?)) else {
        return Ok(None);
    };
    let req = http::agent(10).get(URL).set("Authorization", &format!("Bearer {key}")).set("Accept", "application/json");
    let v = http::send(req, Body::Empty)?.ok_json()?;
    Ok(parse(&v, chrono::Utc::now()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn picks_the_latest_login() {
        let auth = json!({
            "a::1": {"key": "old", "expires_at": "2029-01-01T00:00:00Z"},
            "a::2": {"key": "new", "expires_at": "2030-01-01T00:00:00Z"},
            "a::3": {"key": "undated"}
        });
        assert_eq!(token(&auth).as_deref(), Some("new"));
    }

    #[test]
    fn credits_and_products() {
        let now = http::iso("2030-01-02T00:00:00Z").unwrap();
        let v = json!({"config": {
            "currentPeriod": {"type": "USAGE_PERIOD_TYPE_WEEKLY", "start": "2030-01-01T00:00:00Z", "end": "2030-01-08T00:00:00Z"},
            "creditUsagePercent": 33.0,
            "productUsage": [{"product": "GrokBuild", "usagePercent": 20}, {"product": "Api", "usagePercent": 0}, {"product": "GrokBot", "usagePercent": 13}]
        }});
        let s = parse(&v, now).unwrap();
        assert_eq!((s.windows[0].used_percent, s.windows[0].span_secs, s.windows[0].arg.as_deref()), (33.0, Some(604800.0), Some("weekly")));
        let names: Vec<_> = s.products.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, ["Grok Build", "Grok"]);
        let after = parse(&v, http::iso("2030-02-01T00:00:00Z").unwrap()).unwrap();
        assert_eq!((after.windows[0].used_percent, after.windows[0].resets_at.clone(), after.products.len()), (0.0, None, 0));
    }
}
