//! Home tab data beyond today's snapshot: daily ledger (`usage-ledger.json`), streak, recap.
//! Owner: home-usage area. Port of macOS `UsageLedger` (home-usage.md §3.4). Streak, recap and the
//! heatmap are computed in the webview from this ledger (`src/areas/home-usage/metrics.ts`).

use crate::model::{DailyLedger, UsageSnapshot};
use crate::store::{load_json, save_json};
use chrono::{Datelike, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

/// Emitted with the whole `DailyLedger` whenever it changes.
pub const EV_LEDGER: &str = "home-ledger";
const FILE: &str = "usage-ledger.json";

/// Keys this area owns inside `settings.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HomeSettings {
    /// "7d" | "14d" | "21d" | "month"
    pub daily_trend_window: String,
    /// "monthlyBars" | "recentHeatmap"
    pub activity_visualization: String,
}

impl Default for HomeSettings {
    fn default() -> Self {
        HomeSettings { daily_trend_window: "14d".into(), activity_visualization: "monthlyBars".into() }
    }
}

impl HomeSettings {
    pub fn sanitize(&mut self) {
        if !["7d", "14d", "21d", "month"].contains(&self.daily_trend_window.as_str()) {
            self.daily_trend_window = "14d".into();
        }
        if self.activity_visualization != "recentHeatmap" {
            self.activity_visualization = "monthlyBars".into();
        }
    }
}

struct Ledger {
    path: PathBuf,
    data: Mutex<DailyLedger>,
}

fn is_day(key: &str) -> bool {
    NaiveDate::parse_from_str(key, "%Y-%m-%d").is_ok()
}

/// `UsageLedger.load`: hand-edited files are data, not truth. Keep day keys with a value > 0.
fn sanitized(mut l: DailyLedger) -> DailyLedger {
    l.tokens_by_day.retain(|k, v| is_day(k) && *v > 0);
    if l.covered_since.as_deref().is_some_and(|s| !is_day(s)) {
        l.covered_since = None;
    }
    l
}

/// `UsageLedger.merge`: days never decrease; the series' first day (zeros included) starts coverage.
fn merge(l: &mut DailyLedger, series: &BTreeMap<String, u64>) {
    for (day, &tokens) in series {
        if tokens > 0 {
            let slot = l.tokens_by_day.entry(day.clone()).or_insert(0);
            *slot = (*slot).max(tokens);
        }
    }
    if let Some(first) = series.keys().next() {
        if l.covered_since.as_ref().is_none_or(|since| first < since) {
            l.covered_since = Some(first.clone());
        }
    }
}

/// `UsageLedger.prune`: dropped days are unknown again, not empty.
fn prune(l: &mut DailyLedger, cutoff: &str) {
    l.tokens_by_day.retain(|k, _| k.as_str() >= cutoff);
    if l.covered_since.as_deref().is_some_and(|s| s < cutoff) {
        l.covered_since = Some(cutoff.to_string());
    }
}

/// `UsageStore.monthDailyTotals` reduced to tokens: union by date of every provider's month series.
fn month_daily_totals(snap: &UsageSnapshot) -> BTreeMap<String, u64> {
    let mut by_day = BTreeMap::new();
    for p in &snap.providers {
        for d in &p.month_daily {
            *by_day.entry(d.date.clone()).or_insert(0) += d.total;
        }
    }
    by_day
}

/// Jan 1 of last year: this year and the one before it are kept (the year recap compares against it).
fn cutoff(today: NaiveDate) -> String {
    format!("{:04}-01-01", today.year() - 1)
}

/// Merge + prune; returns whether anything changed.
fn record(l: &mut DailyLedger, snap: &UsageSnapshot, today: NaiveDate) -> bool {
    let before = l.clone();
    merge(l, &month_daily_totals(snap));
    prune(l, &cutoff(today));
    *l != before
}

pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    let path = app.path().app_data_dir()?.join(FILE);
    let data = sanitized(load_json(&path));
    app.manage(Ledger { path, data: Mutex::new(data) });
    Ok(())
}

/// Called by the shell after every usage refresh: record the day ledger, save only if changed.
pub fn on_usage(app: &AppHandle, snap: &UsageSnapshot) {
    let Some(ledger) = app.try_state::<Ledger>() else { return };
    let mut l = ledger.data.lock().unwrap_or_else(|e| e.into_inner());
    if record(&mut l, snap, Local::now().date_naive()) {
        save_json(&ledger.path, &*l);
        let _ = app.emit(EV_LEDGER, &*l);
    }
}

/// Called by the shell on every settings change and once at launch (react to this area's keys here).
pub fn on_settings(_app: &AppHandle, _s: &crate::settings::Settings) {}

#[tauri::command]
fn home_ledger(app: AppHandle) -> DailyLedger {
    app.try_state::<Ledger>()
        .map(|l| l.data.lock().unwrap_or_else(|e| e.into_inner()).clone())
        .unwrap_or_default()
}

crate::area_commands![home_ledger];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DayPoint, ProviderSnapshot};

    fn series(days: &[(&str, u64)]) -> BTreeMap<String, u64> {
        days.iter().map(|(d, t)| (d.to_string(), *t)).collect()
    }

    #[test]
    fn merge_keeps_max_and_earliest_coverage() {
        let mut l = DailyLedger::default();
        merge(&mut l, &series(&[("2026-11-01", 0), ("2026-11-02", 500), ("2026-11-03", 10)]));
        assert_eq!(l.covered_since.as_deref(), Some("2026-11-01"));
        assert!(!l.tokens_by_day.contains_key("2026-11-01"), "zero days are not stored");
        merge(&mut l, &series(&[("2026-11-02", 100), ("2026-11-03", 20)]));
        assert_eq!(l.tokens_by_day["2026-11-02"], 500, "days never decrease");
        assert_eq!(l.tokens_by_day["2026-11-03"], 20);
        assert_eq!(l.covered_since.as_deref(), Some("2026-11-01"), "coverage only moves back");
        merge(&mut l, &series(&[("2026-10-30", 0)]));
        assert_eq!(l.covered_since.as_deref(), Some("2026-10-30"));
    }

    #[test]
    fn prune_drops_old_days_and_clamps_coverage() {
        let mut l = DailyLedger::default();
        merge(&mut l, &series(&[("2024-12-31", 5), ("2025-01-01", 7)]));
        prune(&mut l, "2025-01-01");
        assert_eq!(l.tokens_by_day.len(), 1);
        assert_eq!(l.covered_since.as_deref(), Some("2025-01-01"));
    }

    #[test]
    fn load_sanitizes_junk() {
        let mut l = DailyLedger::default();
        l.tokens_by_day.insert("junk".into(), 9);
        l.tokens_by_day.insert("2026-11-02".into(), 0);
        l.tokens_by_day.insert("2026-11-03".into(), 4);
        l.covered_since = Some("nope".into());
        let l = sanitized(l);
        assert_eq!(l.tokens_by_day.len(), 1);
        assert_eq!(l.covered_since, None);
    }

    #[test]
    fn record_unions_providers_and_reports_change() {
        let p = |id: &str, pts: &[(&str, u64)]| ProviderSnapshot {
            provider_id: id.into(),
            month_daily: pts.iter().map(|(d, t)| DayPoint { date: d.to_string(), total: *t, cost: 0.0 }).collect(),
            ..Default::default()
        };
        let snap = UsageSnapshot {
            providers: vec![p("claude_code", &[("2026-11-01", 3), ("2026-11-02", 4)]), p("codex", &[("2026-11-02", 6)])],
            ..Default::default()
        };
        let today = NaiveDate::from_ymd_opt(2026, 11, 2).unwrap();
        let mut l = DailyLedger::default();
        assert!(record(&mut l, &snap, today));
        assert_eq!(l.tokens_by_day["2026-11-02"], 10);
        assert!(!record(&mut l, &snap, today), "unchanged ledger is not saved again");
        assert_eq!(cutoff(today), "2025-01-01");
    }
}
