//! Home tab data beyond today's snapshot: daily ledger (`usage-ledger.json`), streak, recap.
//! Owner: home-usage area.

use serde::{Deserialize, Serialize};

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

pub fn setup(_app: &tauri::App) -> tauri::Result<()> {
    Ok(())
}

/// Called by the shell after every usage refresh (record the day ledger here).
pub fn on_usage(_app: &tauri::AppHandle, _snap: &crate::model::UsageSnapshot) {}

crate::area_commands![];
