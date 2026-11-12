//! Pure economy rules (shop-bag-dex.md §3): items, prices, wallet, eggs, Rare Candy and Mint use,
//! free candy grants from 100% limits, regional Pokédex progress and badges. No Tauri here.

use crate::companion::{Rarity, State, clamp_difficulty};
use crate::limits::ProviderLimits;
use fastrand::Rng;
use serde::Serialize;
use std::collections::BTreeMap;

pub const RARE_CANDY_XP: u64 = 100_000_000;
pub const RARE_CANDY_WEEKLY_GRANT: u32 = 5;
pub const EGG_PRICE: u64 = 1_000_000_000;
/// `FreshEgg.shopTiers`: legendary is never sold.
pub const EGG_TIERS: [Option<Rarity>; 3] = [None, Some(Rarity::Uncommon), Some(Rarity::Rare)];
const UNOWN: u32 = 201;
const UNOWN_FORMS: usize = 28;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    RareCandy,
    Mint,
    ShinyCharm,
}

impl Item {
    /// `ItemKind.allCases`: the bag's order.
    pub const ALL: [Item; 3] = [Item::RareCandy, Item::Mint, Item::ShinyCharm];

    pub fn key(self) -> &'static str {
        match self {
            Item::RareCandy => "rareCandy",
            Item::Mint => "mint",
            Item::ShinyCharm => "shinyCharm",
        }
    }

    pub fn from_key(key: &str) -> Option<Item> {
        Item::ALL.into_iter().find(|i| i.key() == key)
    }

    pub fn base_price(self) -> u64 {
        match self {
            Item::RareCandy => 500_000_000,
            Item::Mint => 100_000_000,
            Item::ShinyCharm => 3_000_000_000,
        }
    }

    /// One-time purchase with a permanent effect (no count, no use button).
    pub fn passive(self) -> bool {
        self == Item::ShinyCharm
    }
}

/// `round(base × clamp(shopDifficulty))`.
pub fn price(base: u64, shop_difficulty: f64) -> u64 {
    (base as f64 * clamp_difficulty(shop_difficulty)).round() as u64
}

/// 1B scaled by the tier's graduation total: 1B / 2.5B / 4B.
pub fn egg_base_price(tier: Option<Rarity>) -> u64 {
    let t = tier.unwrap_or(Rarity::Common).graduation_total() as f64;
    (EGG_PRICE as f64 * t / Rarity::Common.graduation_total() as f64).round() as u64
}

pub fn count(s: &State, item: Item) -> u32 {
    s.economy.inventory.get(item.key()).copied().unwrap_or(0)
}

/// Spendable = lifetime growth tokens − spent.
pub fn available(s: &State) -> u64 {
    s.lifetime_tokens().saturating_sub(s.economy.spent_tokens)
}

pub fn max_buy(s: &State, item: Item, diff: f64) -> u64 {
    let p = price(item.base_price(), diff);
    if (item.passive() && count(s, item) > 0) || available(s) < p {
        return 0;
    }
    if item.passive() { 1 } else { available(s) / p.max(1) }
}

/// No partial buys; items can be stockpiled in the egg stage.
pub fn buy(s: &mut State, item: Item, n: u64, diff: f64) -> bool {
    if n == 0 || n > max_buy(s, item, diff) {
        return false;
    }
    s.economy.spent_tokens += price(item.base_price(), diff) * n;
    *s.economy.inventory.entry(item.key().into()).or_insert(0) += n as u32;
    true
}

pub fn can_buy_egg(s: &State, tier: Option<Rarity>, diff: f64) -> bool {
    EGG_TIERS.contains(&tier) && s.active.is_some() && available(s) >= price(egg_base_price(tier), diff)
}

/// Releases the active Pokémon (kept as a released dex record) and restarts the egg.
pub fn buy_egg(s: &mut State, tier: Option<Rarity>, diff: f64) -> bool {
    if !can_buy_egg(s, tier, diff) || !s.release_active_for_egg(tier) {
        return false;
    }
    s.economy.spent_tokens += price(egg_base_price(tier), diff);
    true
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShopEntry {
    /// "item" | "egg"
    pub kind: &'static str,
    pub item: Option<&'static str>,
    pub tier: Option<Rarity>,
    pub price: u64,
}

/// Items + the three egg tiers by effective price; a purchased passive item sinks to the bottom.
pub fn shop_entries(s: &State, diff: f64) -> Vec<ShopEntry> {
    let mut v: Vec<(bool, ShopEntry)> = Item::ALL
        .iter()
        .map(|&i| {
            let e = ShopEntry { kind: "item", item: Some(i.key()), tier: None, price: price(i.base_price(), diff) };
            (i.passive() && count(s, i) > 0, e)
        })
        .chain(EGG_TIERS.iter().map(|&t| (false, ShopEntry { kind: "egg", item: None, tier: t, price: price(egg_base_price(t), diff) })))
        .collect();
    v.sort_by_key(|(sunk, e)| (*sunk, e.price));
    v.into_iter().map(|(_, e)| e).collect()
}

// ---- bag ----

/// Difficulty-scaled thresholds of the stages still ahead (the apparent line while disguised).
/// Empty while a Ditto reveal is pending: the batch already accepted must land first.
pub fn candy_stage_costs(s: &State) -> Vec<u64> {
    let Some(m) = s.active.as_ref() else { return vec![] };
    if m.disguised() && m.used_at_stage >= s.stage_threshold(m) {
        return vec![];
    }
    (m.stage_index..m.total_forms)
        .map(|stage| {
            let mut c = m.clone();
            c.stage_index = stage;
            s.stage_threshold(&c)
        })
        .collect()
}

fn candies_for(xp: u64) -> u64 {
    xp.div_ceil(RARE_CANDY_XP)
}

/// Never more than needed to graduate.
pub fn max_candy_use(s: &State) -> u64 {
    let used = s.active.as_ref().map_or(0, |m| m.used_at_stage);
    let remaining = candy_stage_costs(s).iter().sum::<u64>().saturating_sub(used);
    (count(s, Item::RareCandy) as u64).min(candies_for(remaining))
}

/// "evolved" | "graduated" | "progressed"; `None` when no candy could be used.
pub fn use_candy(s: &mut State, n: u64, rng: &mut Rng) -> Option<&'static str> {
    let mut consumed = n.min(max_candy_use(s));
    let m = s.active.as_ref()?;
    if consumed == 0 {
        return None;
    }
    if m.disguised() {
        // Keep whole candies beyond the surprise reveal.
        consumed = consumed.min(candies_for(s.stage_threshold(m).saturating_sub(m.used_at_stage)));
    }
    let before = m.stage_index;
    let left = count(s, Item::RareCandy) - consumed as u32;
    s.economy.inventory.insert(Item::RareCandy.key().into(), left);
    s.apply_candy_xp(consumed * RARE_CANDY_XP, rng);
    Some(match &s.active {
        None => "graduated",
        Some(a) if a.stage_index > before => "evolved",
        Some(_) => "progressed",
    })
}

pub fn use_mint(s: &mut State, rng: &mut Rng) -> bool {
    if count(s, Item::Mint) == 0 || !s.reroll_nature(rng) {
        return false;
    }
    *s.economy.inventory.get_mut(Item::Mint.key()).unwrap() -= 1;
    true
}

// ---- free Rare Candy from 100% limits (§3.6) ----

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowKind {
    Session,
    Weekly,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CandyWindow {
    pub key: String,
    pub name: String,
    pub kind: WindowKind,
    pub utilization: f64,
    /// Reset time; `None` for Codex, which rearms only after a sub-100% sample (#434).
    pub epoch: Option<String>,
}

/// The limits area's eligible windows (claude[.<id>].fiveHour/sevenDay, codex.<bucket>.<slot>,
/// antigravity.<g>.<bucket>) in this module's shape.
pub fn from_limits(ws: Vec<crate::limits::CandyWindow>) -> Vec<CandyWindow> {
    ws.into_iter()
        .map(|w| CandyWindow {
            kind: if w.kind == "weekly" { WindowKind::Weekly } else { WindowKind::Session },
            key: w.key,
            name: w.name,
            utilization: w.utilization,
            epoch: w.epoch,
        })
        .collect()
}

/// Any provider limit loaded: until then nothing is seeded or granted.
pub fn limits_ready(limits: &[ProviderLimits]) -> bool {
    limits.iter().any(|p| !p.windows.is_empty())
}

/// Numeric > 1e12 → ms, > 1e8 → s, else ISO 8601. Seconds since the Unix epoch.
pub fn parse_epoch_date(s: &str) -> Option<f64> {
    if let Ok(ts) = s.trim().parse::<f64>() {
        if ts > 1e12 {
            return Some(ts / 1000.0);
        }
        if ts > 1e8 {
            return Some(ts);
        }
    }
    chrono::DateTime::parse_from_rfc3339(s.trim()).ok().map(|d| d.timestamp_micros() as f64 / 1e6)
}

/// A reset only when the epoch moves forward by ≥ 30 min (session) / 24 h (weekly) (#375);
/// jitter (Codex ±1 s, Claude microseconds) is not a new epoch. Unparsable → string inequality.
pub fn is_new_epoch(previous: &str, current: &str, kind: WindowKind) -> bool {
    if previous == current {
        return false;
    }
    let (Some(p), Some(c)) = (parse_epoch_date(previous), parse_epoch_date(current)) else { return true };
    c - p >= if kind == WindowKind::Weekly { 86400.0 } else { 1800.0 }
}

/// (window name, candy count) for each window that newly reached 100%.
pub fn evaluate_grants(
    windows: &[CandyWindow],
    tier: &mut BTreeMap<String, u8>,
    epochs: &mut BTreeMap<String, String>,
) -> Vec<(String, u32)> {
    let mut grants = vec![];
    for w in windows {
        if let Some(epoch) = &w.epoch {
            match epochs.get(&w.key) {
                Some(prev) if is_new_epoch(prev, epoch, w.kind) => {
                    tier.remove(&w.key);
                    epochs.insert(w.key.clone(), epoch.clone());
                }
                Some(_) => {}
                None => {
                    epochs.insert(w.key.clone(), epoch.clone());
                }
            }
        }
        if w.utilization < 100.0 {
            tier.remove(&w.key);
            if let Some(e) = &w.epoch {
                epochs.insert(w.key.clone(), e.clone());
            }
            continue;
        }
        if tier.get(&w.key).copied().unwrap_or(0) >= 1 {
            continue;
        }
        tier.insert(w.key.clone(), 1);
        if let Some(e) = &w.epoch {
            epochs.insert(w.key.clone(), e.clone());
        }
        grants.push((w.name.clone(), if w.kind == WindowKind::Weekly { RARE_CANDY_WEEKLY_GRANT } else { 1 }));
    }
    grants
}

/// First run seeds windows already at 100% without granting; later runs add the candies.
pub fn grant_candies(s: &mut State, windows: &[CandyWindow], ready: bool) -> Vec<(String, u32)> {
    if !ready {
        return vec![];
    }
    let e = &mut s.economy;
    if !e.candy_feature_seeded {
        for w in windows.iter().filter(|w| w.utilization >= 100.0) {
            e.candy_grant_tier.insert(w.key.clone(), 1);
            if let Some(epoch) = &w.epoch {
                e.candy_window_epoch.insert(w.key.clone(), epoch.clone());
            }
        }
        e.candy_feature_seeded = true;
        return vec![];
    }
    let grants = evaluate_grants(windows, &mut e.candy_grant_tier, &mut e.candy_window_epoch);
    for (_, n) in &grants {
        *e.inventory.entry(Item::RareCandy.key().into()).or_insert(0) += n;
    }
    grants
}

// ---- regional Pokédex (§3.7) ----

/// (id, national range): Gen 1–5.
pub const REGIONS: [(&str, u32, u32); 5] =
    [("kanto", 1, 151), ("johto", 152, 251), ("hoenn", 252, 386), ("sinnoh", 387, 493), ("unova", 494, 649)];
pub const NATIONAL: (u32, u32) = (1, 649);

/// Species id → any owned individual shiny: dex chains plus the active Pokémon's reached stages.
pub fn owned_species(s: &State) -> BTreeMap<u32, bool> {
    let mut out = BTreeMap::new();
    for d in &s.dex {
        for &id in &d.chain {
            *out.entry(id).or_insert(false) |= d.is_shiny;
        }
    }
    if let Some(a) = &s.active {
        for &id in a.reached() {
            *out.entry(id).or_insert(false) |= a.displays_shiny();
        }
    }
    out
}

/// Badge keys (persisted: never change) in `earnedBadges` order.
pub fn earned_badges(s: &State) -> Vec<String> {
    let owned = owned_species(s);
    let tally = |lo: u32, hi: u32| {
        let ids: Vec<bool> = owned.range(lo..=hi).map(|(_, sh)| *sh).collect();
        (ids.len() as u32, ids.iter().filter(|x| **x).count() as u32, hi - lo + 1)
    };
    let mut out = vec![];
    for (r, lo, hi) in REGIONS {
        let (n, shiny, total) = tally(lo, hi);
        if n >= total {
            out.push(format!("region.{r}"));
        }
        if shiny >= total {
            out.push(format!("regionShiny.{r}"));
        }
    }
    let unown: std::collections::HashSet<String> = s.collected_forms(UNOWN).into_iter().collect();
    if unown.len() >= UNOWN_FORMS {
        out.push("unown".into());
    }
    let (n, shiny, total) = tally(NATIONAL.0, NATIONAL.1);
    if n >= total {
        out.push("national".into());
    }
    if shiny >= total {
        out.push("nationalShiny".into());
    }
    out
}

/// First run marks what is already earned as announced. Returns true when keys were added.
pub fn seed_badges(s: &mut State) -> bool {
    if s.economy.dex_completions_seeded {
        return false;
    }
    s.economy.dex_completions_seeded = true;
    let keys = earned_badges(s);
    let added = !keys.is_empty();
    s.economy.notified_dex_completions.extend(keys);
    added
}

/// Badges earned since the last check, recorded as announced (in badge order).
pub fn new_badges(s: &mut State) -> Vec<String> {
    if !s.economy.dex_completions_seeded {
        return vec![];
    }
    let new: Vec<String> = earned_badges(s).into_iter().filter(|k| !s.economy.notified_dex_completions.contains(k)).collect();
    s.economy.notified_dex_completions.extend(new.iter().cloned());
    new
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::companion::form;
    use crate::companion::state::{DexEntry, EvoNode, Line, Mon, Names};

    fn mon(rarity: Rarity, forms: usize, stage: usize, used: u64) -> Mon {
        let ids: Vec<u32> = (1..=forms as u32).collect();
        let mut tree = EvoNode { id: forms as u32, children: vec![] };
        for id in (1..forms as u32).rev() {
            tree = EvoNode { id, children: vec![tree] };
        }
        Mon {
            line: Line { base_id: 1, tree, rarity, names: Names::new() },
            path_ids: ids[..=stage].to_vec(),
            planned_ids: ids.clone(),
            stage_index: stage,
            used_at_stage: used,
            total_forms: forms,
            nature: "hardy".into(),
            ..Default::default()
        }
    }

    fn state(spendable: u64) -> State {
        let mut s = State { growth_difficulty: 1.0, used_since_install: spendable, ..Default::default() };
        s.rt.lang = "en".into();
        s
    }

    #[test]
    fn prices_follow_difficulty_and_tiers() {
        assert_eq!([None, Some(Rarity::Uncommon), Some(Rarity::Rare)].map(egg_base_price), [1_000_000_000, 2_500_000_000, 4_000_000_000]);
        assert_eq!(price(500_000_000, 0.5), 250_000_000);
        assert_eq!(price(500_000_000, f64::NAN), 500_000_000);
        assert_eq!(price(100_000_000, 9.0), 200_000_000);
        let s = state(0);
        let order: Vec<u64> = shop_entries(&s, 1.0).iter().map(|e| e.price).collect();
        assert_eq!(order, [100_000_000, 500_000_000, 1_000_000_000, 2_500_000_000, 3_000_000_000, 4_000_000_000]);
        let mut s = state(0);
        s.economy.inventory.insert("shinyCharm".into(), 1);
        assert_eq!(shop_entries(&s, 1.0).last().unwrap().item, Some("shinyCharm"));
    }

    #[test]
    fn buying_spends_the_wallet_only() {
        let mut s = state(1_250_000_000);
        assert_eq!(max_buy(&s, Item::RareCandy, 1.0), 2);
        assert!(!buy(&mut s, Item::RareCandy, 3, 1.0));
        assert!(buy(&mut s, Item::RareCandy, 2, 1.0));
        assert_eq!((available(&s), count(&s, Item::RareCandy), s.used_since_install), (250_000_000, 2, 1_250_000_000));
        assert_eq!(max_buy(&s, Item::ShinyCharm, 0.05), 0); // clamps to 0.1 → 300M > 250M
        let mut s = state(10_000_000_000);
        assert!(buy(&mut s, Item::ShinyCharm, 1, 1.0));
        assert_eq!(max_buy(&s, Item::ShinyCharm, 1.0), 0); // one-time
        assert!(!buy(&mut s, Item::Mint, 0, 1.0));
    }

    #[test]
    fn egg_purchase_releases_and_guarantees() {
        let mut s = state(3_000_000_000);
        assert!(!buy_egg(&mut s, None, 1.0)); // no active Pokémon
        s.active = Some(mon(Rarity::Common, 3, 1, 5));
        assert!(!buy_egg(&mut s, Some(Rarity::Legendary), 1.0));
        assert!(!buy_egg(&mut s, Some(Rarity::Rare), 1.0)); // 4B > 3B
        assert!(buy_egg(&mut s, Some(Rarity::Uncommon), 1.0));
        assert_eq!((s.economy.spent_tokens, s.egg_tier), (2_500_000_000, Some(Rarity::Uncommon)));
        assert!(s.active.is_none() && s.dex[0].released_at.is_some() && s.dex[0].chain == vec![1, 2]);
    }

    #[test]
    fn candy_max_and_use() {
        let mut s = state(0);
        s.economy.inventory.insert("rareCandy".into(), 20);
        assert_eq!(max_candy_use(&s), 0);
        // Common 2-form line: stage costs 250M, 500M.
        s.active = Some(mon(Rarity::Common, 2, 0, 10_000_000));
        assert_eq!(candy_stage_costs(&s), vec![250_000_000, 500_000_000]);
        assert_eq!(max_candy_use(&s), 8); // ceil(740M / 100M)
        let mut rng = Rng::with_seed(1);
        assert_eq!(use_candy(&mut s, 3, &mut rng), Some("evolved"));
        assert_eq!((count(&s, Item::RareCandy), s.active.as_ref().unwrap().used_at_stage), (17, 60_000_000));
        assert_eq!(use_candy(&mut s, 99, &mut rng), Some("graduated"));
        assert_eq!(count(&s, Item::RareCandy), 12); // only 5 needed
        assert_eq!(use_candy(&mut s, 1, &mut rng), None);
    }

    #[test]
    fn candy_caps_at_ditto_reveal() {
        let mut s = state(0);
        s.economy.inventory.insert("rareCandy".into(), 9);
        let mut m = mon(Rarity::Common, 2, 0, 0);
        m.ditto_disguise = Some(1);
        s.active = Some(m);
        let mut rng = Rng::with_seed(2);
        assert_eq!(use_candy(&mut s, 5, &mut rng), Some("progressed"));
        assert_eq!(count(&s, Item::RareCandy), 6); // 250M needs 3
        assert_eq!(max_candy_use(&s), 0); // reveal pending
    }

    #[test]
    fn mint_needs_stock_and_a_pokemon() {
        let mut s = state(0);
        let mut rng = Rng::with_seed(3);
        s.economy.inventory.insert("mint".into(), 1);
        assert!(!use_mint(&mut s, &mut rng));
        s.active = Some(mon(Rarity::Common, 1, 0, 0));
        assert!(use_mint(&mut s, &mut rng));
        assert_ne!(s.active.as_ref().unwrap().nature, "hardy");
        assert!(!use_mint(&mut s, &mut rng));
    }

    #[test]
    fn epochs_ignore_jitter() {
        let w = WindowKind::Session;
        assert!(!is_new_epoch("2026-01-01T05:00:00Z", "2026-01-01T05:00:00Z", w));
        assert!(!is_new_epoch("2026-01-01T05:00:00.123456+00:00", "2026-01-01T05:00:00.987+00:00", w));
        assert!(!is_new_epoch("1767243600", "1767243601", w)); // Codex ±1 s
        assert!(!is_new_epoch("2026-01-01T05:00:00Z", "2026-01-01T04:00:00Z", w)); // backwards
        assert!(is_new_epoch("2026-01-01T05:00:00Z", "2026-01-01T10:00:00Z", w));
        assert!(!is_new_epoch("2026-01-01T00:00:00Z", "2026-01-01T10:00:00Z", WindowKind::Weekly));
        assert!(is_new_epoch("2026-01-01T00:00:00Z", "2026-01-08T00:00:00Z", WindowKind::Weekly));
        assert!(is_new_epoch("a", "b", w));
        assert_eq!(parse_epoch_date("1767243600000"), Some(1767243600.0));
    }

    fn win(key: &str, kind: WindowKind, u: f64, epoch: Option<&str>) -> CandyWindow {
        CandyWindow { key: key.into(), name: key.into(), kind, utilization: u, epoch: epoch.map(String::from) }
    }

    #[test]
    fn grants_are_edge_triggered_and_rearm() {
        let (mut tier, mut ep) = (BTreeMap::new(), BTreeMap::new());
        let e1 = Some("2026-01-01T05:00:00Z");
        let e2 = Some("2026-01-01T10:00:00Z");
        let s = WindowKind::Session;
        assert!(evaluate_grants(&[win("c", s, 99.0, e1)], &mut tier, &mut ep).is_empty());
        assert_eq!(evaluate_grants(&[win("c", s, 100.0, e1)], &mut tier, &mut ep), vec![("c".into(), 1)]);
        assert!(evaluate_grants(&[win("c", s, 100.0, e1)], &mut tier, &mut ep).is_empty());
        // #326: a new epoch rearms without a sub-100 sample.
        assert_eq!(evaluate_grants(&[win("c", s, 100.0, e2)], &mut tier, &mut ep).len(), 1);
        // #375: jitter does not.
        assert!(evaluate_grants(&[win("c", s, 100.0, Some("2026-01-01T10:00:01Z"))], &mut tier, &mut ep).is_empty());
        // #434: no epoch → only a sub-100 sample rearms.
        let w = WindowKind::Weekly;
        assert_eq!(evaluate_grants(&[win("x", w, 100.0, None)], &mut tier, &mut ep), vec![("x".into(), 5)]);
        assert!(evaluate_grants(&[win("x", w, 100.0, None)], &mut tier, &mut ep).is_empty());
        evaluate_grants(&[win("x", w, 40.0, None)], &mut tier, &mut ep);
        assert_eq!(evaluate_grants(&[win("x", w, 100.0, None)], &mut tier, &mut ep).len(), 1);
    }

    #[test]
    fn first_run_seeds_without_granting() {
        let mut s = state(0);
        let ws = [win("a", WindowKind::Session, 100.0, Some("1767243600")), win("b", WindowKind::Session, 10.0, None)];
        assert!(grant_candies(&mut s, &ws, false).is_empty());
        assert!(!s.economy.candy_feature_seeded);
        assert!(grant_candies(&mut s, &ws, true).is_empty());
        assert!(s.economy.candy_feature_seeded && s.economy.candy_grant_tier["a"] == 1);
        assert!(grant_candies(&mut s, &ws, true).is_empty());
        let ws = [win("b", WindowKind::Weekly, 100.0, None)];
        assert_eq!(grant_candies(&mut s, &ws, true).len(), 1);
        assert_eq!(count(&s, Item::RareCandy), 5);
    }

    #[test]
    fn windows_from_limits() {
        let lw = |kind: &'static str| crate::limits::CandyWindow { key: "k".into(), name: "n".into(), kind, utilization: 1.0, epoch: None };
        let ws = from_limits(vec![lw("session"), lw("weekly")]);
        assert_eq!(ws.iter().map(|w| w.kind).collect::<Vec<_>>(), [WindowKind::Session, WindowKind::Weekly]);
        assert!(!limits_ready(&[]));
    }

    fn entry(chain: Vec<u32>, shiny: bool) -> DexEntry {
        DexEntry { final_id: *chain.last().unwrap(), chain, is_shiny: shiny, ..Default::default() }
    }

    #[test]
    fn badges_seed_then_notify_once() {
        let mut s = state(0);
        s.dex.push(entry((1..=150).collect(), true));
        assert!(!seed_badges(&mut s)); // nothing earned yet
        assert!(s.economy.dex_completions_seeded);
        s.dex.push(entry(vec![151], false));
        assert_eq!(new_badges(&mut s), vec!["region.kanto".to_string()]);
        assert!(new_badges(&mut s).is_empty());
        s.dex.push(entry(vec![151], true));
        assert_eq!(new_badges(&mut s), vec!["regionShiny.kanto".to_string()]);
        let mut fresh = state(0);
        fresh.dex = s.dex.clone();
        assert!(seed_badges(&mut fresh)); // already earned: recorded silently
        assert!(new_badges(&mut fresh).is_empty());
    }

    #[test]
    fn unown_badge_needs_all_28_forms() {
        let mut s = state(0);
        s.economy.dex_completions_seeded = true;
        for f in form::forms(UNOWN).into_iter().take(27) {
            s.dex.push(DexEntry { form: Some(f), ..entry(vec![UNOWN], false) });
        }
        s.dex.push(DexEntry { form: Some("a".into()), ..entry(vec![UNOWN], false) });
        assert!(new_badges(&mut s).is_empty());
        s.dex.push(DexEntry { form: Some("question".into()), ..entry(vec![UNOWN], false) });
        assert_eq!(new_badges(&mut s), vec!["unown".to_string()]);
    }
}
