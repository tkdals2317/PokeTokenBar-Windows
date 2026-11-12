//! Wallet, shop, bag and candy grants. Owner: shop-bag-dex area.
//! State lives inside `companion.json` (flattened into `companion::State`) and is mutated only
//! through `companion::update`, so one lock covers growth and spending.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// This area's block of `companion::State`. Field names must not collide with companion's.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EconomyState {
    pub spent_tokens: u64,
    /// "rareCandy" | "mint" | "shinyCharm" → count
    pub inventory: BTreeMap<String, u32>,
}

/// Keys this area owns inside `settings.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EconomySettings {
    /// Price multiplier 0.1…2.0.
    pub shop_difficulty: f64,
}

impl Default for EconomySettings {
    fn default() -> Self {
        EconomySettings { shop_difficulty: 1.0 }
    }
}

impl EconomySettings {
    pub fn sanitize(&mut self) {
        self.shop_difficulty = crate::companion::clamp_difficulty(self.shop_difficulty);
    }
}

pub fn setup(_app: &tauri::App) -> tauri::Result<()> {
    Ok(())
}

/// Called by the shell after every limits refresh (free Rare Candy grants, shop-bag-dex.md §3.6).
pub fn on_limits(_app: &tauri::AppHandle, _limits: &[crate::limits::ProviderLimits]) {}

crate::area_commands![];
