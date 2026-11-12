//! Normalized usage record, number/time helpers, dedup and aggregation (providers.md §3).
//! macOS: `LocalUsageReader.Entry`, `Bucket`, `daily/period/dailySeries/activeBlock`.

use super::pricing;
use crate::model::{
    ActiveBlock, CostCoverage, DailyUsage, DayPoint, ModelUsage, PeriodUsage, ProviderSnapshot, Tokens, Usage,
};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, TimeZone};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeSet, HashMap};

/// Unix seconds.
pub type Ts = f64;

pub const BLOCK_WINDOW: f64 = 5.0 * 3600.0;
pub const RECENT_USE_WINDOW: f64 = 7.0 * 86_400.0;
pub const MAX_DAILY_TREND_WINDOW_DAYS: i64 = 21;
/// Parse ceiling: 1e18 leaves room for sums taken right after parsing.
pub const MAX_TOKEN: u64 = 1_000_000_000_000_000_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Entry {
    pub id: String,
    pub date: Ts,
    /// Local `yyyy-MM-dd` of `date`.
    pub day: String,
    pub model: String,
    pub input: u64,
    pub output: u64,
    pub cache_write: u64,
    pub cache_read: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explicit_cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_is_estimate: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_unavailable: Option<bool>,
    /// Claude only: the transcript session (account attribution).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_ids: Option<Vec<String>>,
}

impl Entry {
    pub fn new(id: impl Into<String>, date: Ts, model: impl Into<String>, t: [u64; 4]) -> Entry {
        Entry {
            id: id.into(),
            date,
            day: day_of(date),
            model: model.into(),
            input: t[0],
            output: t[1],
            cache_write: t[2],
            cache_read: t[3],
            ..Default::default()
        }
    }
    pub fn total(&self) -> u64 {
        self.input + self.output + self.cache_write + self.cache_read
    }
    pub fn claude_session_ids(&self) -> Vec<String> {
        let mut s: BTreeSet<String> = self.session_ids.clone().unwrap_or_default().into_iter().collect();
        s.extend(self.session_id.clone());
        s.into_iter().collect()
    }
}

// ---- numbers (LocalUsageReader.swift:2204-2238) ----

/// A JSON number that is finite. `null`, strings and absence are all `None`.
pub fn double_or_nil(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64).filter(|d| d.is_finite())
}

pub fn clamp_token(d: f64) -> u64 {
    if !d.is_finite() || d <= 0.0 {
        0
    } else if d >= MAX_TOKEN as f64 {
        MAX_TOKEN
    } else {
        d as u64
    }
}

pub fn int_or_nil(v: Option<&Value>) -> Option<u64> {
    double_or_nil(v).map(clamp_token)
}

pub fn int_value(v: Option<&Value>) -> u64 {
    int_or_nil(v).unwrap_or(0)
}

/// `LocalAdditionalUsageReader.intValue`: numbers or integer strings, clamped.
pub fn loose_int(v: Option<&Value>) -> u64 {
    match v {
        Some(Value::Number(n)) => n.as_f64().map_or(0, clamp_token),
        Some(Value::String(s)) => s.trim().parse::<i64>().map_or(0, |n| (n.max(0) as u64).min(MAX_TOKEN)),
        _ => 0,
    }
}

/// Numbers or numeric strings (not filtered for finiteness, like Swift's `doubleValue`).
pub fn loose_double(v: Option<&Value>) -> Option<f64> {
    match v {
        Some(Value::Number(n)) => n.as_f64(),
        Some(Value::String(s)) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// Epoch seconds or milliseconds (`>= 1e11` ⇒ ms).
pub fn date_value(v: Option<&Value>) -> Option<Ts> {
    epoch_secs(loose_double(v)?)
}

pub fn epoch_secs(raw: f64) -> Option<Ts> {
    if !raw.is_finite() || raw <= 0.0 {
        return None;
    }
    Some(if raw >= 100_000_000_000.0 { raw / 1000.0 } else { raw })
}

pub fn bool_value(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|d| d != 0.0),
        _ => false,
    }
}

pub fn str_of<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

/// Trimmed, non-empty string.
pub fn non_empty(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(String::from)
}

// ---- time ----

pub fn now_ts() -> Ts {
    Local::now().timestamp_millis() as f64 / 1000.0
}

fn local(ts: Ts) -> DateTime<Local> {
    let ms = (ts * 1000.0).round() as i64;
    Local.timestamp_millis_opt(ms).single().unwrap_or_else(Local::now)
}

pub fn day_of(ts: Ts) -> String {
    local(ts).format("%Y-%m-%d").to_string()
}

pub fn rfc3339(ts: Ts) -> String {
    local(ts).to_rfc3339()
}

/// ISO 8601 with any fractional width (macOS `ISO8601Parser`).
pub fn parse_iso(s: &str) -> Option<Ts> {
    DateTime::parse_from_rfc3339(s.trim()).ok().map(|d| d.timestamp_millis() as f64 / 1000.0)
}

fn midnight(d: NaiveDate) -> Ts {
    let naive = d.and_hms_opt(0, 0, 0).unwrap();
    match Local.from_local_datetime(&naive).earliest() {
        Some(t) => t.timestamp() as f64,
        // A DST gap at midnight: the day starts at the first valid instant after it.
        None => Local.from_local_datetime(&(naive + Duration::hours(1))).earliest().map_or(0.0, |t| t.timestamp() as f64),
    }
}

pub fn date_of(ts: Ts) -> NaiveDate {
    local(ts).date_naive()
}

pub fn start_of_month(ts: Ts) -> Ts {
    midnight(date_of(ts).with_day(1).unwrap())
}

/// Locale first weekday: Sunday in the regions that start weeks on Sunday, else Monday.
/// ponytail: region list approximates CLDR `firstDay`; read the OS calendar if users report wrong weeks.
fn week_starts_sunday() -> bool {
    let loc = sys_locale::get_locale().unwrap_or_default().to_uppercase();
    let region = loc.rsplit(['-', '_']).next().unwrap_or("");
    const SUNDAY: [&str; 18] =
        ["US", "CA", "JP", "KR", "TW", "HK", "BR", "MX", "IL", "PH", "IN", "ZA", "AU", "SA", "PE", "CO", "AR", "TH"];
    SUNDAY.contains(&region) || loc == "KO" || loc == "JA"
}

pub fn start_of_week(ts: Ts) -> Ts {
    let d = date_of(ts);
    let back = if week_starts_sunday() {
        d.weekday().num_days_from_sunday()
    } else {
        d.weekday().num_days_from_monday()
    };
    midnight(d - Duration::days(back as i64))
}

/// Calendar start of the first day of an N-day window ending today (DST-safe).
pub fn start_of_rolling(days: i64, ts: Ts) -> Ts {
    midnight(date_of(ts) - Duration::days(days.max(1) - 1))
}

/// Lower bound of the one scan every displayed window comes from.
pub fn enrichment_scan_start(now: Ts) -> Ts {
    [
        start_of_month(now),
        start_of_week(now),
        now - BLOCK_WINDOW,
        now - RECENT_USE_WINDOW,
        start_of_rolling(MAX_DAILY_TREND_WINDOW_DAYS, now),
    ]
    .into_iter()
    .fold(f64::INFINITY, f64::min)
}

pub fn month_key(ts: Ts) -> String {
    local(ts).format("%Y-%m").to_string()
}

// ---- dedup ----

/// Same id: keep the larger total (ties keep the first), date = earliest, union Claude sessions.
pub fn dedup_keep_max(entries: Vec<Entry>) -> Vec<Entry> {
    let mut order: Vec<String> = Vec::new();
    let mut by_id: HashMap<String, Entry> = HashMap::new();
    for e in entries {
        let Some(existing) = by_id.get(&e.id) else {
            order.push(e.id.clone());
            by_id.insert(e.id.clone(), e);
            continue;
        };
        let (date, day) = if e.date < existing.date {
            (e.date, e.day.clone())
        } else {
            (existing.date, existing.day.clone())
        };
        let mut sessions: BTreeSet<String> = existing.claude_session_ids().into_iter().collect();
        sessions.extend(e.claude_session_ids());
        let mut kept = if e.total() > existing.total() { e } else { existing.clone() };
        kept.date = date;
        kept.day = day;
        let sessions: Vec<String> = sessions.into_iter().collect();
        kept.session_id = sessions.first().cloned();
        kept.session_ids = (sessions.len() > 1).then_some(sessions);
        by_id.insert(kept.id.clone(), kept);
    }
    order.into_iter().filter_map(|id| by_id.remove(&id)).collect()
}

/// Codex canonical ids: keep the earliest date, first-seen order.
pub fn dedup_keep_earliest(entries: Vec<Entry>) -> Vec<Entry> {
    let mut order: Vec<String> = Vec::new();
    let mut by_id: HashMap<String, Entry> = HashMap::new();
    for e in entries {
        match by_id.get(&e.id) {
            Some(x) if e.date >= x.date => {}
            Some(_) => {
                by_id.insert(e.id.clone(), e);
            }
            None => {
                order.push(e.id.clone());
                by_id.insert(e.id.clone(), e);
            }
        }
    }
    order.into_iter().filter_map(|id| by_id.remove(&id)).collect()
}

// ---- aggregation ----

#[derive(Debug, Default, Clone)]
pub struct Bucket {
    pub tokens: Tokens,
    pub cost: f64,
    pub coverage: CostCoverage,
}

impl Bucket {
    pub fn total(&self) -> u64 {
        self.tokens.total()
    }

    /// Cost provenance per entry (LocalUsageReader.swift:57-73).
    pub fn add(&mut self, e: &Entry) {
        self.tokens.add(&Tokens { input: e.input, output: e.output, cache_write: e.cache_write, cache_read: e.cache_read });
        if e.total() == 0 {
            return;
        }
        if let Some(c) = e.explicit_cost.filter(|c| c.is_finite() && *c >= 0.0) {
            self.cost += c;
            if e.cost_is_estimate == Some(true) {
                self.coverage.estimated = true;
            } else {
                self.coverage.reported = true;
            }
        } else if let Some(c) = (e.cost_unavailable != Some(true))
            .then(|| pricing::estimated_cost(&e.model, e.input, e.output, e.cache_write, e.cache_read))
            .flatten()
        {
            self.cost += c;
            self.coverage.estimated = true;
        } else {
            if e.cost_unavailable != Some(true) {
                pricing::note_unpriced(&e.model);
            }
            self.coverage.unknown = true;
        }
    }

    pub fn usage(&self) -> Usage {
        Usage { tokens: self.tokens, total: self.total(), cost: self.cost, coverage: self.coverage }
    }
}

pub fn daily(entries: &[Entry], day: &str, include_models: bool) -> Option<DailyUsage> {
    let mut b = Bucket::default();
    let mut models: HashMap<&str, Bucket> = HashMap::new();
    for e in entries.iter().filter(|e| e.day == day) {
        b.add(e);
        if include_models {
            models.entry(e.model.as_str()).or_default().add(e);
        }
    }
    if b.total() == 0 {
        return None;
    }
    let models = include_models.then(|| {
        let mut v: Vec<ModelUsage> =
            models.into_iter().map(|(m, b)| ModelUsage { model: m.to_string(), usage: b.usage() }).collect();
        v.sort_by(|a, b| b.usage.total.cmp(&a.usage.total).then_with(|| a.model.cmp(&b.model)));
        v
    });
    Some(DailyUsage { date: day.to_string(), usage: b.usage(), models })
}

pub fn period(entries: &[Entry], from: &str, to: &str) -> PeriodUsage {
    let mut b = Bucket::default();
    for e in entries.iter().filter(|e| e.day.as_str() >= from && e.day.as_str() <= to) {
        b.add(e);
    }
    PeriodUsage { from: from.into(), to: to.into(), usage: b.usage() }
}

/// Day axis from the range, zero days explicit; entries outside the axis are dropped.
pub fn daily_series(entries: &[Entry], start: Ts, end: Ts) -> Vec<DayPoint> {
    let (mut d, last) = (date_of(start), date_of(end));
    let mut days = Vec::new();
    while d <= last {
        days.push(d.format("%Y-%m-%d").to_string());
        d = d.succ_opt().unwrap();
    }
    let in_range: BTreeSet<&str> = days.iter().map(String::as_str).collect();
    let mut buckets: HashMap<&str, Bucket> = HashMap::new();
    for e in entries {
        if in_range.contains(e.day.as_str()) {
            buckets.entry(e.day.as_str()).or_default().add(e);
        }
    }
    days.iter()
        .map(|day| {
            let b = buckets.get(day.as_str()).cloned().unwrap_or_default();
            DayPoint { date: day.clone(), total: b.total(), cost: b.cost }
        })
        .collect()
}

pub fn active_block(entries: &[Entry], now: Ts) -> Option<ActiveBlock> {
    let start = now - BLOCK_WINDOW;
    let mut recent: Vec<&Entry> = entries.iter().filter(|e| e.date >= start && e.total() > 0).collect();
    recent.sort_by(|a, b| a.date.total_cmp(&b.date));
    let first = recent.first()?.date;
    let mut b = Bucket::default();
    recent.iter().for_each(|e| b.add(e));
    let minutes = ((now - first) / 60.0).max(1.0);
    Some(ActiveBlock {
        start: rfc3339(first),
        end: rfc3339(first + BLOCK_WINDOW),
        total: b.total(),
        tokens_per_minute: b.total() as f64 / minutes,
    })
}

/// `ProviderEnrichment.local` + `fetchDaily`, from one scan.
pub fn snapshot(id: &str, name: &str, entries: &[Entry], now: Ts, include_models: bool) -> ProviderSnapshot {
    let today = day_of(now);
    let week_from = day_of(start_of_week(now));
    let month_from = day_of(start_of_month(now));
    ProviderSnapshot {
        provider_id: id.into(),
        display_name: name.into(),
        today: daily(entries, &today, include_models),
        active_block: active_block(entries, now),
        week_total: Some(period(entries, &week_from, &today)),
        month_total: Some(period(entries, &month_from, &today)),
        month_daily: daily_series(entries, start_of_month(now), now),
        recent_daily: daily_series(entries, start_of_rolling(MAX_DAILY_TREND_WINDOW_DAYS, now), now),
        last_usage: entries.iter().filter(|e| e.total() > 0).map(|e| e.date).reduce(f64::max).map(rfc3339),
        reports_cost: true,
    }
}

/// `hasDisplayableUsage` (UsageStore.swift:2306-2314).
pub fn displayable(p: &ProviderSnapshot, now: Ts) -> bool {
    p.today.is_some()
        || p.active_block.as_ref().is_some_and(|b| b.total > 0)
        || p.week_total.as_ref().is_some_and(|w| w.usage.total > 0)
        || p.month_total.as_ref().is_some_and(|m| m.usage.total > 0)
        || p.month_daily.iter().any(|d| d.total > 0)
        || p.recent_daily.iter().any(|d| d.total > 0)
        || p.last_usage
            .as_deref()
            .and_then(parse_iso)
            .is_some_and(|t| now - t <= RECENT_USE_WINDOW)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn e(id: &str, date: Ts, total: u64) -> Entry {
        Entry::new(id, date, "unknown-model", [total, 0, 0, 0])
    }

    #[test]
    fn numbers_clamp_and_distinguish_null() {
        let v = json!({"a": 12.0, "b": -3, "c": 1e30, "d": null, "e": "7", "f": " 42 "});
        assert_eq!(int_or_nil(v.get("a")), Some(12));
        assert_eq!(int_or_nil(v.get("b")), Some(0));
        assert_eq!(int_or_nil(v.get("c")), Some(MAX_TOKEN));
        assert_eq!(int_or_nil(v.get("d")), None);
        assert_eq!(int_or_nil(v.get("e")), None, "strings are not numbers here");
        assert_eq!(loose_int(v.get("f")), 42);
        assert_eq!(date_value(Some(&json!(1_700_000_000_000u64))), Some(1_700_000_000.0));
        assert_eq!(date_value(Some(&json!("1700000000"))), Some(1_700_000_000.0));
        assert_eq!(date_value(Some(&json!(0))), None);
    }

    #[test]
    fn iso_any_fraction_width() {
        let a = parse_iso("2026-01-02T03:04:05.123456789Z").unwrap();
        let b = parse_iso("2026-01-02T03:04:05Z").unwrap();
        assert!((a - b - 0.123).abs() < 1e-6);
        assert!(parse_iso("2026-01-02T03:04:05.034464+00:00").is_some());
    }

    #[test]
    fn keep_max_keeps_bigger_total_and_earliest_date() {
        let mut a = e("x", 200.0, 10);
        a.session_id = Some("s2".into());
        let mut b = e("x", 100.0, 5);
        b.session_id = Some("s1".into());
        let out = dedup_keep_max(vec![a, b, e("y", 1.0, 1)]);
        assert_eq!(out.len(), 2);
        assert_eq!((out[0].total(), out[0].date), (10, 100.0));
        assert_eq!(out[0].session_ids, Some(vec!["s1".to_string(), "s2".to_string()]));
        assert_eq!(out[0].session_id.as_deref(), Some("s1"));
        // tie keeps existing
        let mut c = e("z", 5.0, 3);
        c.model = "first".into();
        let mut d = e("z", 1.0, 3);
        d.model = "second".into();
        let out = dedup_keep_max(vec![c, d]);
        assert_eq!((out[0].model.as_str(), out[0].date), ("first", 1.0));
    }

    #[test]
    fn cost_provenance() {
        let mut b = Bucket::default();
        b.add(&e("zero", 1.0, 0)); // zero total: no coverage
        assert_eq!(b.coverage, CostCoverage::default());
        let mut src = e("a", 1.0, 10);
        src.explicit_cost = Some(0.0);
        b.add(&src);
        assert!(b.coverage.reported);
        let mut est = Entry::new("b", 1.0, "claude-sonnet-4-6", [1_000_000, 0, 0, 0]);
        b.add(&est);
        assert!(b.coverage.estimated && (b.cost - 3.0).abs() < 1e-9);
        est.cost_unavailable = Some(true);
        b.add(&est);
        assert!(b.coverage.unknown);
    }

    #[test]
    fn series_sum_matches_month_total() {
        let now = now_ts();
        let entries: Vec<Entry> = (0..40).map(|i| e(&i.to_string(), now - i as f64 * 86_400.0, 100)).collect();
        let series = daily_series(&entries, start_of_month(now), now);
        let month = period(&entries, &day_of(start_of_month(now)), &day_of(now));
        assert_eq!(series.iter().map(|d| d.total).sum::<u64>(), month.usage.total);
        assert_eq!(series.last().unwrap().date, day_of(now));
        let recent = daily_series(&entries, start_of_rolling(21, now), now);
        assert_eq!(recent.len(), 21);
    }

    #[test]
    fn block_ignores_zero_records() {
        let now = 10_000_000.0;
        assert!(active_block(&[e("syn", now - 60.0, 0)], now).is_none());
        let b = active_block(&[e("a", now - 120.0, 600), e("old", now - BLOCK_WINDOW - 1.0, 5)], now).unwrap();
        assert_eq!(b.total, 600);
        assert!((b.tokens_per_minute - 300.0).abs() < 1e-9);
    }

    #[test]
    fn scan_start_covers_every_window() {
        let now = now_ts();
        let s = enrichment_scan_start(now);
        assert!(s <= start_of_month(now) && s <= start_of_week(now) && s <= now - RECENT_USE_WINDOW);
        assert!(s <= start_of_rolling(21, now));
    }
}
