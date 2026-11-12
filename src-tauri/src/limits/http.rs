//! Small HTTP layer shared by every limit source: one error enum (macOS `LimitsError`),
//! status/Retry-After/Cloudflare handling, JSON and form bodies.

use serde_json::Value;
use std::time::Duration;

/// macOS `LimitsError`, minus the Keychain cases Windows does not have (limits.md §3.1, §6).
#[derive(Debug, Clone, PartialEq)]
pub enum LimitError {
    /// No credential file (`keychainUnavailable`, `credentialFormat`). A missing session key is not an
    /// error on Windows: the chain simply starts with OAuth.
    NoCredential,
    /// The credential file has no `claudeAiOauth` login (`credentialMissingAccountOAuth`).
    ReauthNeeded,
    /// The file token is past its expiry; Claude Code renews it on its next run.
    TokenExpired,
    Http(u16),
    RateLimited(Option<f64>),
    SessionKeyMalformed,
    SessionKeyInvalid,
    SessionKeyNoOrg,
    /// claude.ai answered with a Cloudflare challenge: the key may be fine, the client is not.
    Cloudflare,
    Format,
    Network,
}

impl LimitError {
    /// Stable code the webview maps to `friendlyLimitError` text.
    pub fn code(&self) -> String {
        match self {
            LimitError::NoCredential => "no_credential".into(),
            LimitError::ReauthNeeded => "reauth".into(),
            LimitError::TokenExpired => "token_expired".into(),
            LimitError::Http(s) => format!("http_{s}"),
            LimitError::RateLimited(_) => "rate_limited".into(),
            LimitError::SessionKeyMalformed => "session_key_malformed".into(),
            LimitError::SessionKeyInvalid => "session_key_invalid".into(),
            LimitError::SessionKeyNoOrg => "session_key_no_org".into(),
            LimitError::Cloudflare => "cloudflare".into(),
            LimitError::Format => "format".into(),
            LimitError::Network => "network".into(),
        }
    }

    pub fn is_auth_rejection(&self) -> bool {
        matches!(self, LimitError::Http(401 | 403))
    }
}

pub type Result<T> = std::result::Result<T, LimitError>;

/// One finished request, whatever its status.
pub struct Resp {
    pub status: u16,
    pub retry_after: Option<f64>,
    /// `cf-mitigated: challenge`
    pub cf_challenge: bool,
    pub body: String,
}

impl Resp {
    pub fn json(&self) -> Result<Value> {
        serde_json::from_str(&self.body).map_err(|_| LimitError::Format)
    }

    /// 200…299 → body JSON; 429 → `RateLimited`; anything else → `Http(status)`.
    pub fn ok_json(&self) -> Result<Value> {
        match self.status {
            200..=299 => self.json(),
            429 => Err(LimitError::RateLimited(self.retry_after)),
            s => Err(LimitError::Http(s)),
        }
    }
}

/// Only the delta-seconds form counts; it must be > 0 and is capped at an hour (`OAuthLimitsProvider.swift:103`).
pub fn parse_retry_after(raw: Option<&str>) -> Option<f64> {
    let v: f64 = raw?.trim().parse().ok()?;
    (v.is_finite() && v > 0.0).then(|| v.min(3600.0))
}

pub fn agent(timeout_secs: u64) -> ureq::Agent {
    let builder = ureq::AgentBuilder::new().timeout(Duration::from_secs(timeout_secs)).redirects(0);
    // SChannel (the Edge TLS stack) gets through Cloudflare where rustls' fingerprint may not (limits.md §6).
    #[cfg(windows)]
    let builder = match native_tls::TlsConnector::new() {
        Ok(tls) => builder.tls_connector(std::sync::Arc::new(tls)),
        Err(_) => builder,
    };
    builder.build()
}

pub enum Body<'a> {
    Empty,
    Json(&'a Value),
    Form(&'a [(&'a str, &'a str)]),
}

pub fn send(req: ureq::Request, body: Body) -> Result<Resp> {
    let res = match body {
        Body::Empty => req.call(),
        Body::Json(v) => req.send_json(v.clone()),
        Body::Form(f) => req.send_form(f),
    };
    let r = match res {
        Ok(r) => r,
        Err(ureq::Error::Status(_, r)) => r,
        Err(ureq::Error::Transport(_)) => return Err(LimitError::Network),
    };
    Ok(Resp {
        status: r.status(),
        retry_after: parse_retry_after(r.header("retry-after")),
        cf_challenge: r.header("cf-mitigated").is_some_and(|v| v.eq_ignore_ascii_case("challenge")),
        body: r.into_string().unwrap_or_default(),
    })
}

/// RFC 3339 / ISO 8601 with any fraction length → UTC RFC 3339 (`ISO8601Parser`).
pub fn iso(raw: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(raw.trim()).ok().map(|d| d.with_timezone(&chrono::Utc))
}

pub fn epoch_secs(secs: f64) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::from_timestamp_millis((secs * 1000.0) as i64)
}

/// Number or numeric string.
pub fn num(v: &Value) -> Option<f64> {
    v.as_f64().or_else(|| v.as_str().and_then(|s| s.trim().parse().ok()))
}

pub fn text(v: &Value) -> Option<String> {
    v.as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

pub fn home() -> std::path::PathBuf {
    std::env::home_dir().unwrap_or_default()
}

/// Process environment; empty counts as unset.
pub fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// `~` / `~/x` / `~\x` expansion.
pub fn expand(raw: &str) -> std::path::PathBuf {
    let t = raw.trim();
    if t == "~" {
        return home();
    }
    match t.strip_prefix("~/").or_else(|| t.strip_prefix("~\\")) {
        Some(rest) => home().join(rest),
        None => t.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_after_takes_positive_seconds_only() {
        assert_eq!(parse_retry_after(Some(" 120 ")), Some(120.0));
        assert_eq!(parse_retry_after(Some("1.5")), Some(1.5));
        assert_eq!(parse_retry_after(Some("99999")), Some(3600.0));
        assert_eq!(parse_retry_after(Some("0")), None);
        assert_eq!(parse_retry_after(Some("Wed, 21 Oct 2015 07:28:00 GMT")), None);
        assert_eq!(parse_retry_after(None), None);
    }

    #[test]
    fn iso_accepts_any_fraction() {
        for s in ["2026-10-10T11:10:00+00:00", "2026-10-10T11:10:00.123456+00:00", "2026-10-10T11:10:00.1Z"] {
            assert_eq!(iso(s).unwrap().timestamp(), 1_791_630_600, "{s}");
        }
    }
}
