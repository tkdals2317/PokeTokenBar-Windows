//! Wallet, shop, bag, free Rare Candy grants and dex completion badges (docs/parity/shop-bag-dex.md).
//! Owner: shop-bag-dex area. State lives inside `companion.json` (flattened into `companion::State`)
//! and is mutated only through `companion::update`, so one lock covers growth and spending.

pub mod rules;
pub mod text;

use crate::companion::state::LatestEvent;
use crate::companion::{Rarity, SharedStore, State};
use crate::settings::Settings;
use rules::Item;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use tauri::{AppHandle, Emitter, Listener, Manager};

/// Economy view for the Shop and Bag tabs (`useEconomy` in the frontend).
pub const EV_ECONOMY: &str = "economy-changed";

/// This area's block of `companion::State`. Field names must not collide with companion's.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct EconomyState {
    pub spent_tokens: u64,
    /// "rareCandy" | "mint" | "shinyCharm" → count
    pub inventory: BTreeMap<String, u32>,
    /// Limit window key → 1 once its candy was granted (cleared on rearm).
    pub candy_grant_tier: BTreeMap<String, u8>,
    /// Limit window key → last seen reset time.
    pub candy_window_epoch: BTreeMap<String, String>,
    pub candy_feature_seeded: bool,
    /// Announced badge keys (`region.kanto`, `unown`, …). Keys never change.
    pub notified_dex_completions: BTreeSet<String>,
    pub dex_completions_seeded: bool,
}

impl EconomyState {
    /// Shiny Charm owned: hatches roll 1/48 instead of 1/64.
    #[allow(dead_code)] // companion reads the inventory directly today
    pub fn owns_shiny_charm(&self) -> bool {
        self.inventory.get(Item::ShinyCharm.key()).copied().unwrap_or(0) > 0
    }
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

/// Windows toasts need the installed AUMID; dev builds have none (same gate as companion).
fn notify_all(app: &AppHandle, settings: &Settings, notes: &[(String, String)]) {
    if settings.companion.companion_notifications && !cfg!(debug_assertions) {
        for (title, body) in notes {
            crate::shell::notify(app, title, body);
        }
    }
}

pub fn setup(app: &tauri::App) -> tauri::Result<()> {
    {
        let store = app.state::<SharedStore>();
        let mut s = store.lock().unwrap();
        // Badges earned before this feature are recorded silently; an empty seed is not worth a write.
        if rules::seed_badges(&mut s.state) {
            s.save();
        }
    }
    // Every companion change (growth, hatch, purchase, settings) can move the wallet or finish a dex.
    let handle = app.handle().clone();
    app.listen(crate::model::EV_COMPANION, move |_| on_companion_changed(&handle));
    Ok(())
}

/// Announces newly completed dexes and refreshes the economy view.
fn on_companion_changed(app: &AppHandle) {
    let settings = crate::settings::current(app);
    let lang = settings.shell.language.clone();
    let (economy, notes, companion) = {
        let store = app.state::<SharedStore>();
        let mut s = store.lock().unwrap();
        let notes: Vec<(String, String)> =
            rules::new_badges(&mut s.state).iter().map(|k| text::badge_notification(&lang, k)).collect();
        let companion = notes.last().map(|(_, body)| {
            let st = &mut s.state;
            st.rt.seq += 1;
            // Badges show on the floating pet (`showsOnPet: true`); candy does not.
            st.rt.latest_event = Some(LatestEvent { seq: st.rt.seq, text: body.clone(), at: chrono::Local::now().to_rfc3339() });
            crate::companion::view(st, &settings)
        });
        if companion.is_some() {
            s.save();
        }
        (view(&s.state, &settings), notes, companion)
    };
    notify_all(app, &settings, &notes);
    let _ = app.emit(EV_ECONOMY, economy);
    if let Some(v) = companion {
        let _ = app.emit(crate::model::EV_COMPANION, v);
    }
}

/// Called by the shell after every limits refresh (free Rare Candy grants, shop-bag-dex.md §3.6).
pub fn on_limits(app: &AppHandle, limits: &[crate::limits::ProviderLimits]) {
    let ready = rules::limits_ready(limits);
    if !ready {
        return;
    }
    let settings = crate::settings::current(app);
    let lang = settings.shell.language.clone();
    let windows = rules::from_limits(crate::limits::candy_windows(app));
    let grants = crate::companion::update(app, |s| rules::grant_candies(s, &windows, ready));
    let notes: Vec<(String, String)> = grants.iter().map(|(name, n)| (text::candy_title(&lang, *n), text::candy_body(&lang, name))).collect();
    notify_all(app, &settings, &notes);
}

/// Called by the shell on every settings change and once at launch: prices follow `shop_difficulty`.
pub fn on_settings(app: &AppHandle, s: &Settings) {
    let v = view(&app.state::<SharedStore>().lock().unwrap().state, s);
    let _ = app.emit(EV_ECONOMY, v);
}

/// What the Shop and Bag need (shop-bag-dex.md §5.1-5.2).
pub fn view(s: &State, settings: &Settings) -> Value {
    let diff = settings.economy.shop_difficulty;
    let shop: Vec<Value> = rules::shop_entries(s, diff)
        .into_iter()
        .map(|e| {
            let (max_buy, can_buy) = match e.item.and_then(Item::from_key) {
                Some(i) => {
                    let m = rules::max_buy(s, i, diff);
                    (m, m > 0)
                }
                None => (0, rules::can_buy_egg(s, e.tier, diff)),
            };
            json!({ "kind": e.kind, "item": e.item, "tier": e.tier, "price": e.price, "max_buy": max_buy, "can_buy": can_buy })
        })
        .collect();
    let owned: Vec<Value> = Item::ALL
        .iter()
        .map(|&i| (i, rules::count(s, i)))
        .filter(|(_, n)| *n > 0)
        .map(|(i, n)| json!({ "item": i.key(), "count": n, "passive": i.passive() }))
        .collect();
    json!({
        "available": rules::available(s),
        "spent": s.economy.spent_tokens,
        "shop": shop,
        "owned": owned,
        "candy_xp": rules::RARE_CANDY_XP,
        "candy_costs": rules::candy_stage_costs(s),
        "max_candy_use": rules::max_candy_use(s),
        "can_use_mint": s.active.is_some() && rules::count(s, Item::Mint) > 0,
    })
}

#[tauri::command]
fn economy(app: AppHandle) -> Value {
    let settings = crate::settings::current(&app);
    view(&app.state::<SharedStore>().lock().unwrap().state, &settings)
}

#[tauri::command]
fn buy_item(app: AppHandle, item: String, count: u64) -> bool {
    let Some(item) = Item::from_key(&item) else { return false };
    let diff = crate::settings::current(&app).economy.shop_difficulty;
    crate::companion::update(&app, |s| rules::buy(s, item, count, diff))
}

/// `tier`: null (no guarantee), "uncommon" or "rare".
#[tauri::command]
fn buy_egg(app: AppHandle, tier: Option<Rarity>) -> bool {
    let diff = crate::settings::current(&app).economy.shop_difficulty;
    crate::companion::update(&app, |s| rules::buy_egg(s, tier, diff))
}

/// Rare Candy → "evolved" | "graduated" | "progressed"; Mint → "changed"; null when unusable.
#[tauri::command]
fn use_item(app: AppHandle, item: String, count: u64) -> Option<String> {
    let mut rng = fastrand::Rng::new();
    match Item::from_key(&item)? {
        Item::RareCandy => crate::companion::update(&app, |s| rules::use_candy(s, count, &mut rng)).map(String::from),
        Item::Mint => crate::companion::update(&app, |s| rules::use_mint(s, &mut rng)).then(|| "changed".into()),
        Item::ShinyCharm => None,
    }
}

crate::area_commands![economy, buy_item, buy_egg, use_item];
