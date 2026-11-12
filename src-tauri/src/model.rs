//! Shared data contract between areas and the frontend (`src/shared.ts` mirrors it).
//! Owner: foundation. Areas may add fields only through ARCHITECTURE.md's change process.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---- event names (Rust → webview) ----
pub const EV_USAGE: &str = "usage"; // UsageSnapshot
pub const EV_LIMITS: &str = "limits"; // Vec<limits::ProviderLimits>
pub const EV_COMPANION: &str = "companion"; // companion::view()
pub const EV_COMPANION_EVENT: &str = "companion-event"; // companion::Event
pub const EV_SETTINGS: &str = "settings"; // settings::Settings
pub const EV_POPOVER_OPENED: &str = "popover-opened"; // PopoverOpened

/// Where the popover should land when it opens. Default = Home with Settings closed.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PopoverOpened {
    /// "home" | "shop" | "bag" | "collection"
    pub tab: Option<String>,
    pub settings: bool,
}

/// Disjoint token buckets.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Tokens {
    pub input: u64,
    pub output: u64,
    pub cache_write: u64,
    pub cache_read: u64,
}

impl Tokens {
    pub fn total(&self) -> u64 {
        self.input + self.output + self.cache_write + self.cache_read
    }
    pub fn add(&mut self, o: &Tokens) {
        self.input += o.input;
        self.output += o.output;
        self.cache_write += o.cache_write;
        self.cache_read += o.cache_read;
    }
}

/// Where a cost number came from (macOS `CostCoverage`). Merging ORs the flags.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct CostCoverage {
    /// The tool logged the cost itself.
    pub reported: bool,
    /// Computed from the model price table.
    pub estimated: bool,
    /// Some tokens had no usable price.
    pub unknown: bool,
}

impl CostCoverage {
    pub fn merge(&mut self, o: CostCoverage) {
        self.reported |= o.reported;
        self.estimated |= o.estimated;
        self.unknown |= o.unknown;
    }
}

/// Tokens and cost for one model (or one bucket of a period).
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct Usage {
    pub tokens: Tokens,
    pub total: u64,
    pub cost: f64,
    pub coverage: CostCoverage,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelUsage {
    pub model: String,
    #[serde(flatten)]
    pub usage: Usage,
}

/// One local day (`yyyy-MM-dd`) for one provider.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct DailyUsage {
    pub date: String,
    #[serde(flatten)]
    pub usage: Usage,
    /// Per-model breakdown; `None` for providers that don't opt in (macOS: only pi/omp/omo/kimi_code).
    pub models: Option<Vec<ModelUsage>>,
}

/// Inclusive local-day range (`from`..=`to`).
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct PeriodUsage {
    pub from: String,
    pub to: String,
    #[serde(flatten)]
    pub usage: Usage,
}

/// One bar of a daily series. Zero days are explicit.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct DayPoint {
    pub date: String,
    pub total: u64,
    pub cost: f64,
}

/// Rolling 5-hour block that started with the oldest entry of the last 5 hours.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActiveBlock {
    pub start: String,
    pub end: String,
    pub total: u64,
    pub tokens_per_minute: f64,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderSnapshot {
    /// macOS ids: claude_code, codex, gemini, antigravity, opencode, hermes, cursor, grok, copilot,
    /// kiro, pi, omp, omo, aside, kimi_code, devin.
    pub provider_id: String,
    pub display_name: String,
    pub today: Option<DailyUsage>,
    pub active_block: Option<ActiveBlock>,
    pub week_total: Option<PeriodUsage>,
    pub month_total: Option<PeriodUsage>,
    pub month_daily: Vec<DayPoint>,
    pub recent_daily: Vec<DayPoint>,
    /// RFC 3339 of the newest entry ever seen.
    pub last_usage: Option<String>,
    pub reports_cost: bool,
}

/// Everything the providers area read in one refresh. Emitted as `usage`.
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct UsageSnapshot {
    pub providers: Vec<ProviderSnapshot>,
    /// RFC 3339; `None` before the first refresh (menu shows "—", footer hides "Updated").
    pub fetched_at: Option<String>,
    /// Set when at least one provider failed (footer error triangle).
    pub error: Option<String>,
}

impl UsageSnapshot {
    fn today_of<'a>(&'a self, day: &'a str) -> impl Iterator<Item = &'a DailyUsage> + 'a {
        self.providers.iter().filter_map(|p| p.today.as_ref()).filter(move |t| t.date == day)
    }

    pub fn today_total(&self, day: &str) -> u64 {
        self.today_of(day).map(|t| t.usage.total).sum()
    }

    pub fn today_cost(&self, day: &str) -> (f64, CostCoverage) {
        let mut cov = CostCoverage::default();
        let mut cost = 0.0;
        for t in self.today_of(day) {
            cost += t.usage.cost;
            cov.merge(t.usage.coverage);
        }
        (cost, cov)
    }

    /// Today's tokens per provider id, for companion crediting. Only providers whose snapshot is
    /// dated `day`: a stale or missing one must not move the ledger (companion.md §3.11).
    pub fn today_by_provider(&self, day: &str) -> HashMap<String, u64> {
        self.providers
            .iter()
            .filter_map(|p| p.today.as_ref().filter(|t| t.date == day).map(|t| (p.provider_id.clone(), t.usage.total)))
            .collect()
    }

    pub fn shows_cost(&self) -> bool {
        self.providers.iter().any(|p| p.reports_cost)
    }
}

/// Day → total tokens history kept by home-usage (`usage-ledger.json`).
#[allow(dead_code)] // contract for home-usage
#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct DailyLedger {
    pub tokens_by_day: std::collections::BTreeMap<String, u64>,
    pub covered_since: Option<String>,
}

pub fn today_key() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}
