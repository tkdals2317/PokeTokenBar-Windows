//! Pokémon companion: egg → hatch → evolve → graduate into the Pokédex.
//! Balance is ported from the original `CompanionModel.swift` / `CompanionStore.swift`.
//! Owner: companion area. See docs/parity/ARCHITECTURE.md.

pub mod api;
pub mod pokeapi;

use fastrand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use crate::store::{load_json, save_json};
use std::path::{Path, PathBuf};

pub const EGG_HATCH_THRESHOLD: u64 = 5_000_000;
pub const SHINY_DENOMINATOR: u64 = 64;
/// Gen-V animated sprites exist up to #649, so lines are cut there.
pub const MAX_SPECIES_ID: u32 = 649;
pub const DITTO_ID: u32 = 132;

pub const NATURES: [&str; 25] = [
    "hardy", "lonely", "brave", "adamant", "naughty", "bold", "docile", "relaxed", "impish", "lax",
    "timid", "hasty", "serious", "jolly", "naive", "modest", "mild", "quiet", "bashful", "rash",
    "calm", "gentle", "sassy", "careful", "quirky",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rarity {
    Common,
    Uncommon,
    Rare,
    Legendary,
}

impl Rarity {
    pub fn from(capture_rate: u32, legendary_or_mythical: bool) -> Self {
        match capture_rate {
            _ if legendary_or_mythical => Rarity::Legendary,
            0..=45 => Rarity::Rare,
            46..=120 => Rarity::Uncommon,
            _ => Rarity::Common,
        }
    }

    pub fn graduation_total(self) -> u64 {
        match self {
            Rarity::Common => 750_000_000,
            Rarity::Uncommon => 1_875_000_000,
            Rarity::Rare => 3_000_000_000,
            Rarity::Legendary => 6_000_000_000,
        }
    }
}

pub const DIFFICULTY_RANGE: (f64, f64) = (0.1, 2.0);

/// User growth multiplier: below 1 grows faster, above 1 slower. Bad saved values fall back to 1.
pub fn clamp_difficulty(d: f64) -> f64 {
    if d.is_finite() && d > 0.0 { d.clamp(DIFFICULTY_RANGE.0, DIFFICULTY_RANGE.1) } else { 1.0 }
}

pub fn scaled(base: u64, difficulty: f64) -> u64 {
    ((base as f64 * clamp_difficulty(difficulty)).round() as u64).max(1)
}

/// Tokens needed to leave stage `stage_index` (0-based) of a `k`-form line.
/// Stage i costs T·i / (k(k+1)/2), so every line of a rarity sums to the same total T.
pub fn phase_threshold(rarity: Rarity, k: usize, stage_index: usize) -> u64 {
    let k = k.max(1) as f64;
    let i = (stage_index + 1) as f64;
    let t = rarity.graduation_total() as f64;
    ((t * i / (k * (k + 1.0) / 2.0)).round() as u64).max(1)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvoNode {
    pub id: u32,
    pub children: Vec<EvoNode>,
}

impl EvoNode {
    pub fn find(&self, id: u32) -> Option<&EvoNode> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(id))
    }

    pub fn final_ids(&self) -> Vec<u32> {
        if self.children.is_empty() {
            return vec![self.id];
        }
        self.children.iter().flat_map(EvoNode::final_ids).collect()
    }

    pub fn all_ids(&self) -> Vec<u32> {
        let mut v = vec![self.id];
        self.children.iter().for_each(|c| v.extend(c.all_ids()));
        v
    }

    /// Drops species without an animated sprite, and everything after them.
    pub fn keep_animated(&self) -> Option<EvoNode> {
        (self.id <= MAX_SPECIES_ID).then(|| EvoNode {
            id: self.id,
            children: self
                .children
                .iter()
                .filter_map(EvoNode::keep_animated)
                .collect(),
        })
    }
}

/// species id → language code → name
pub type Names = HashMap<u32, HashMap<String, String>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Line {
    pub base_id: u32,
    pub tree: EvoNode,
    pub rarity: Rarity,
    pub names: Names,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mon {
    pub line: Line,
    /// Realized evolution path so far.
    pub path_ids: Vec<u32>,
    /// Full route chosen at hatch (branches resolved up front).
    pub planned_ids: Vec<u32>,
    pub stage_index: usize,
    pub used_at_stage: u64,
    pub is_shiny: bool,
    pub nature: String,
}

impl Mon {
    pub fn current_id(&self) -> u32 {
        self.path_ids[self.stage_index.min(self.path_ids.len() - 1)]
    }
    pub fn threshold(&self, difficulty: f64) -> u64 {
        scaled(phase_threshold(self.line.rarity, self.planned_ids.len(), self.stage_index), difficulty)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexEntry {
    pub base_id: u32,
    pub final_id: u32,
    pub chain: Vec<u32>,
    pub rarity: Rarity,
    pub is_shiny: bool,
    pub nature: String,
    pub names: Names,
    pub caught_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Event {
    Hatched { id: u32, shiny: bool },
    Evolved { id: u32 },
    Graduated { id: u32 },
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// Usage before the first observation isn't counted (no retroactive growth).
    pub baseline_set: bool,
    /// provider → today's tokens already credited.
    pub ledger: HashMap<String, u64>,
    pub last_date: String,
    pub used_since_install: u64,
    pub egg_usage: u64,
    pub active: Option<Mon>,
    pub dex: Vec<DexEntry>,
    /// "base:final" pairs, used to prefer new branches and down-weight repeats.
    pub collected_finals: HashSet<String>,
    /// Growth multiplier from Settings. 0 (older saves) means 1.
    pub growth_difficulty: f64,
    /// Wallet, bag and candy grants. Owned by the shop-bag-dex area (`economy`), saved in the same file.
    #[serde(flatten)]
    pub economy: crate::economy::EconomyState,
}

impl State {
    pub fn difficulty(&self) -> f64 {
        clamp_difficulty(self.growth_difficulty)
    }

    pub fn egg_threshold(&self) -> u64 {
        scaled(EGG_HATCH_THRESHOLD, self.difficulty())
    }

    /// Changes the multiplier and rescales banked progress so the progress bars don't jump.
    pub fn set_difficulty(&mut self, new: f64) {
        let (old, new) = (self.difficulty(), clamp_difficulty(new));
        let rescale = |v: u64| (v as f64 * new / old).round() as u64;
        self.egg_usage = rescale(self.egg_usage);
        if let Some(m) = self.active.as_mut() {
            m.used_at_stage = rescale(m.used_at_stage);
        }
        self.growth_difficulty = new;
    }

    /// Localized species name from the active line or the Pokédex.
    pub fn name_of(&self, id: u32, lang: &str) -> String {
        let names = self
            .active
            .iter()
            .map(|m| &m.line.names)
            .chain(self.dex.iter().rev().map(|d| &d.names))
            .find_map(|n| n.get(&id));
        let pick = |l: &str| names.and_then(|n| n.get(l)).cloned();
        let lang = if lang == "ja" { "ja-hrkt" } else { lang };
        pick(lang).or_else(|| pick("en")).unwrap_or_else(|| format!("#{id}"))
    }

    /// Credits today's per-provider growth since the last call. Returns the new tokens.
    pub fn credit(&mut self, today: &HashMap<String, u64>, date: &str) -> u64 {
        if !self.baseline_set {
            self.baseline_set = true;
            self.ledger = today.clone();
            self.last_date = date.to_string();
            return 0;
        }
        // Saves before v0.3 keyed Claude Code as "claude"; providers now use the macOS id.
        if let Some(v) = self.ledger.remove("claude") {
            self.ledger.entry("claude_code".into()).or_insert(v);
        }
        let delta = if date != self.last_date {
            // A new day's totals aren't comparable to yesterday's: count them all.
            self.last_date = date.to_string();
            self.ledger = today.clone();
            today.values().sum()
        } else {
            let mut delta = 0;
            for (provider, &current) in today {
                match self.ledger.get(provider) {
                    // Newly seen provider: seed, don't backfill its past logs.
                    None => {}
                    // Regression (logs rotated/deleted): rebase this provider only.
                    Some(&prev) if current < prev => {}
                    Some(&prev) => delta += current - prev,
                }
                self.ledger.insert(provider.clone(), current);
            }
            delta
        };
        self.used_since_install += delta;
        delta
    }

    pub fn ready_to_hatch(&self) -> bool {
        self.active.is_none() && self.baseline_set && self.egg_usage >= self.egg_threshold()
    }

    /// Feeds tokens to the egg or the active Pokémon.
    pub fn feed(&mut self, delta: u64, rng: &mut Rng) -> Vec<Event> {
        if self.active.is_none() {
            self.egg_usage += delta;
            return vec![];
        }
        self.active.as_mut().unwrap().used_at_stage += delta;
        let mut events = vec![];
        let difficulty = self.difficulty();
        while let Some(mon) = self.active.as_mut() {
            let thr = mon.threshold(difficulty);
            if mon.used_at_stage < thr {
                break;
            }
            let node = mon.line.tree.find(mon.current_id()).cloned();
            let Some(node) = node.filter(|n| !n.children.is_empty()) else {
                events.push(self.graduate());
                break;
            };
            let next_idx = mon.stage_index + 1;
            let next = match mon.planned_ids.get(next_idx) {
                Some(&p) if node.children.iter().any(|c| c.id == p) => p,
                _ => {
                    // Stale plan: re-plan from here.
                    let base = mon.line.base_id;
                    let route = plan(&node, base, &self.collected_finals, rng);
                    let mon = self.active.as_mut().unwrap();
                    mon.planned_ids.truncate(mon.stage_index);
                    mon.planned_ids.extend(route);
                    mon.planned_ids[next_idx]
                }
            };
            let mon = self.active.as_mut().unwrap();
            mon.path_ids.truncate(next_idx);
            mon.path_ids.push(next);
            mon.stage_index = next_idx;
            mon.used_at_stage -= thr; // overflow carries over
            events.push(Event::Evolved { id: next });
        }
        events
    }

    fn graduate(&mut self) -> Event {
        let mon = self.active.take().unwrap();
        let final_id = mon.current_id();
        self.collected_finals
            .insert(format!("{}:{}", mon.line.base_id, final_id));
        let names = mon
            .path_ids
            .iter()
            .filter_map(|id| mon.line.names.get(id).map(|n| (*id, n.clone())))
            .collect();
        self.dex.push(DexEntry {
            base_id: mon.line.base_id,
            final_id,
            chain: mon.path_ids,
            rarity: mon.line.rarity,
            is_shiny: mon.is_shiny,
            nature: mon.nature,
            names,
            caught_at: chrono::Local::now().to_rfc3339(),
        });
        self.egg_usage = 0;
        Event::Graduated { id: final_id }
    }

    pub fn hatch(&mut self, line: Line, rng: &mut Rng) -> Vec<Event> {
        let overflow = self.egg_usage.saturating_sub(self.egg_threshold());
        self.egg_usage = 0;
        let is_shiny = rng.u64(..) % SHINY_DENOMINATOR == 0;
        let nature = NATURES[rng.usize(..NATURES.len())].to_string();
        let planned_ids = plan(&line.tree, line.base_id, &self.collected_finals, rng);
        let base_id = line.base_id;
        self.active = Some(Mon {
            line,
            path_ids: vec![base_id],
            planned_ids,
            stage_index: 0,
            used_at_stage: 0,
            is_shiny,
            nature,
        });
        let mut events = vec![Event::Hatched {
            id: base_id,
            shiny: is_shiny,
        }];
        events.extend(self.feed(overflow, rng));
        events
    }

    pub fn has_collected_base(&self, base_id: u32) -> bool {
        let prefix = format!("{base_id}:");
        self.collected_finals.iter().any(|f| f.starts_with(&prefix))
    }
}

/// Prefer a child that still leads to an uncollected final form.
fn pick_child<'a>(
    node: &'a EvoNode,
    base: u32,
    collected: &HashSet<String>,
    rng: &mut Rng,
) -> &'a EvoNode {
    let fresh: Vec<&EvoNode> = node
        .children
        .iter()
        .filter(|c| {
            c.final_ids()
                .iter()
                .any(|f| !collected.contains(&format!("{base}:{f}")))
        })
        .collect();
    let pool: Vec<&EvoNode> = if fresh.is_empty() {
        node.children.iter().collect()
    } else {
        fresh
    };
    pool[rng.usize(..pool.len())]
}

/// Full route from `root` to a final form.
fn plan(root: &EvoNode, base: u32, collected: &HashSet<String>, rng: &mut Rng) -> Vec<u32> {
    let mut route = vec![root.id];
    let mut node = root;
    while !node.children.is_empty() {
        node = pick_child(node, base, collected, rng);
        route.push(node.id);
    }
    route
}

/// Capture-rate weighted pick from `(species id, capture rate)`; collected lines count half.
pub fn choose_base(index: &[(u32, u32)], state: &State, rng: &mut Rng) -> Option<u32> {
    let weights: Vec<u64> = index
        .iter()
        .map(|&(id, rate)| {
            let w = rate.max(1) as u64;
            if state.has_collected_base(id) {
                (w / 2).max(1)
            } else {
                w
            }
        })
        .collect();
    let total: u64 = weights.iter().sum();
    if total == 0 {
        return None;
    }
    let mut r = rng.u64(..total);
    for (i, w) in weights.iter().enumerate() {
        if r < *w {
            return Some(index[i].0);
        }
        r -= w;
    }
    None
}

pub struct Store {
    pub path: PathBuf,
    pub state: State,
}

impl Store {
    pub fn load(path: PathBuf) -> Self {
        Store { state: load_json(&path), path }
    }

    pub fn save(&self) {
        save_json(&self.path, &self.state);
    }

    pub fn dir(&self) -> &Path {
        self.path.parent().unwrap_or(Path::new("."))
    }
}


// ---- area wiring (see ARCHITECTURE.md) ----

use crate::model::{EV_COMPANION, EV_COMPANION_EVENT, UsageSnapshot};
use serde_json::{Value, json};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

/// Keys this area owns inside `settings.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CompanionSettings {
    /// Species shown by the floating pet and tray instead of the active Pokémon.
    pub representative: Option<u32>,
    pub companion_notifications: bool,
}

impl Default for CompanionSettings {
    fn default() -> Self {
        CompanionSettings { representative: None, companion_notifications: true }
    }
}

impl CompanionSettings {
    pub fn sanitize(&mut self) {}
}

/// The one lock for `companion.json`. Economy mutates its block through the same lock.
pub type SharedStore = Mutex<Store>;

pub fn setup(app: &tauri::App, dir: &Path) -> tauri::Result<()> {
    app.manage::<SharedStore>(Mutex::new(Store::load(dir.join("companion.json"))));
    Ok(())
}

/// Runs `f` on the saved state under the lock, saves, and emits `companion`.
pub fn update<T>(app: &AppHandle, f: impl FnOnce(&mut State) -> T) -> T {
    let store = app.state::<SharedStore>();
    let mut s = store.lock().unwrap();
    let out = f(&mut s.state);
    s.save();
    let _ = app.emit(EV_COMPANION, view(&s));
    out
}

/// What the UI needs: saved state plus the thresholds it can't derive.
pub fn view(store: &Store) -> Value {
    let s = &store.state;
    json!({
        "active": s.active,
        "egg_usage": s.egg_usage,
        "egg_threshold": s.egg_threshold(),
        "threshold": s.active.as_ref().map(|m| m.threshold(s.difficulty())),
        "difficulty": s.difficulty(),
        "dex": s.dex,
    })
}

/// Notification titles. es/fr/pt/de/ru fall back to English until this area ports them.
fn text(lang: &str, key: &str) -> &'static str {
    match (lang, key) {
        ("ko", "hatched") => "알이 부화했어요!",
        ("ko", "shiny") => "✨ 이로치가 태어났어요!",
        ("ko", "evolved") => "진화했어요!",
        ("ko", "graduated") => "졸업! 도감에 등록됐어요",
        ("ja", "hatched") => "タマゴがかえった！",
        ("ja", "shiny") => "✨ 色違いが生まれた！",
        ("ja", "evolved") => "進化した！",
        ("ja", "graduated") => "卒業！図鑑に登録されました",
        (_, "hatched") => "Your egg hatched!",
        (_, "shiny") => "✨ A shiny hatched!",
        (_, "evolved") => "Evolved!",
        (_, "graduated") => "Graduated! Added to the Pokédex",
        _ => "",
    }
}

/// Hatches the egg if it's ready. Network calls run without holding the store lock.
fn hatch_if_ready(app: &AppHandle, rng: &mut Rng) -> Vec<Event> {
    let store = app.state::<SharedStore>();
    let dir = {
        let s = store.lock().unwrap();
        if !s.state.ready_to_hatch() {
            return vec![];
        }
        s.dir().to_path_buf()
    };
    // Any failure keeps the egg; the next tick retries.
    let Ok(index) = pokeapi::base_index(&dir) else { return vec![] };
    let Some(base) = choose_base(&index, &store.lock().unwrap().state, rng) else {
        return vec![];
    };
    let Ok(line) = pokeapi::line(base) else { return vec![] };
    let mut s = store.lock().unwrap();
    if !s.state.ready_to_hatch() {
        return vec![];
    }
    s.state.hatch(line, rng)
}

/// Called by the shell after every usage refresh: credit growth, hatch, notify, emit.
pub fn on_usage(app: &AppHandle, snap: &UsageSnapshot) {
    let date = crate::model::today_key();
    let today = snap.today_by_provider(&date);
    let mut rng = Rng::new();
    let store = app.state::<SharedStore>();
    let mut events = {
        let mut s = store.lock().unwrap();
        let delta = s.state.credit(&today, &date);
        s.state.feed(delta, &mut rng)
    };
    events.extend(hatch_if_ready(app, &mut rng));

    let settings = crate::settings::current(app);
    let s = store.lock().unwrap();
    s.save();
    let _ = app.emit(EV_COMPANION, view(&s));
    for e in events {
        if settings.companion.companion_notifications {
            let (key, id) = match e {
                Event::Hatched { id, shiny: true } => ("shiny", id),
                Event::Hatched { id, .. } => ("hatched", id),
                Event::Evolved { id } => ("evolved", id),
                Event::Graduated { id } => ("graduated", id),
            };
            let lang = &settings.shell.language;
            crate::shell::notify(app, text(lang, key), &s.state.name_of(id, lang));
        }
        let _ = app.emit(EV_COMPANION_EVENT, e);
    }
}

/// "Growth 42%" / "Hatch 42%" for the tray tooltip (ui-shell.md §3.1, integer floor).
pub fn menu_growth_text(app: &AppHandle, lang: &str) -> String {
    let store = app.state::<SharedStore>();
    let s = &store.lock().unwrap().state;
    let (credits, total, egg) = match &s.active {
        None => (s.egg_usage, s.egg_threshold(), true),
        Some(m) => (m.used_at_stage, m.threshold(s.difficulty()), false),
    };
    let pct = if total > 0 { credits.min(total) * 100 / total } else { 0 };
    let word = match (lang, egg) {
        ("ko", true) => "부화",
        ("ja", true) => "孵化",
        ("es", true) => "Eclosión",
        ("fr", true) => "Éclosion",
        ("pt", true) => "Eclosão",
        ("de", true) => "Schlüpfen",
        ("ru", true) => "Вылупление",
        (_, true) => "Hatch",
        ("ko", false) => "성장",
        ("ja", false) => "成長",
        ("es", false) => "Crecimiento",
        ("fr", false) => "Croissance",
        ("pt", false) => "Crescimento",
        ("de", false) => "Wachstum",
        ("ru", false) => "Рост",
        (_, false) => "Growth",
    };
    format!("{word} {pct}%")
}

#[tauri::command]
fn companion(store: tauri::State<SharedStore>) -> Value {
    view(&store.lock().unwrap())
}

#[tauri::command]
fn set_difficulty(app: AppHandle, value: f64) -> Value {
    update(&app, |s| s.set_difficulty(value));
    view(&app.state::<SharedStore>().lock().unwrap())
}

/// Called by the shell on every settings change and once at launch (react to this area's keys here).
pub fn on_settings(_app: &tauri::AppHandle, _s: &crate::settings::Settings) {}

crate::area_commands![companion, set_difficulty];

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: u32, children: Vec<EvoNode>) -> EvoNode {
        EvoNode { id, children }
    }

    fn bulbasaur() -> Line {
        Line {
            base_id: 1,
            tree: node(1, vec![node(2, vec![node(3, vec![])])]),
            rarity: Rarity::Common,
            names: HashMap::new(),
        }
    }

    #[test]
    fn thresholds_sum_to_graduation_total() {
        let sum: u64 = (0..3).map(|i| phase_threshold(Rarity::Common, 3, i)).sum();
        assert_eq!(sum, 750_000_000);
        assert_eq!(phase_threshold(Rarity::Common, 3, 0), 125_000_000);
    }

    #[test]
    fn rarity_from_capture_rate() {
        assert_eq!(Rarity::from(45, false), Rarity::Rare);
        assert_eq!(Rarity::from(120, false), Rarity::Uncommon);
        assert_eq!(Rarity::from(255, false), Rarity::Common);
        assert_eq!(Rarity::from(3, true), Rarity::Legendary);
    }

    #[test]
    fn credit_seeds_then_counts_growth() {
        let mut s = State::default();
        let day = |c: u64, x: u64| HashMap::from([("claude_code".into(), c), ("codex".into(), x)]);
        assert_eq!(s.credit(&HashMap::from([("claude".into(), 100), ("codex".into(), 0)]), "d1"), 0); // old-id baseline
        assert_eq!(s.credit(&day(150, 10), "d1"), 60);
        assert_eq!(s.credit(&day(120, 10), "d1"), 0); // regression rebases
        assert_eq!(s.credit(&day(130, 10), "d1"), 10);
        assert_eq!(s.credit(&day(5, 0), "d2"), 5); // new day counts in full
        assert_eq!(s.used_since_install, 75);
    }

    #[test]
    fn hatch_evolve_graduate_with_overflow() {
        let mut rng = Rng::with_seed(7);
        let mut s = State {
            baseline_set: true,
            ..Default::default()
        };
        s.feed(EGG_HATCH_THRESHOLD + 1_000, &mut rng);
        assert!(s.ready_to_hatch());
        let ev = s.hatch(bulbasaur(), &mut rng);
        assert!(matches!(ev[0], Event::Hatched { id: 1, .. }));
        assert_eq!(s.active.as_ref().unwrap().used_at_stage, 1_000);

        // 125M finishes stage 0; the extra 1_000 + 250M jumps straight through stage 1 too.
        let ev = s.feed(125_000_000 + 250_000_000, &mut rng);
        assert_eq!(ev.len(), 2);
        assert_eq!(s.active.as_ref().unwrap().current_id(), 3);
        assert_eq!(s.active.as_ref().unwrap().used_at_stage, 1_000);

        let ev = s.feed(375_000_000, &mut rng);
        assert!(matches!(ev[0], Event::Graduated { id: 3 }));
        assert!(s.active.is_none());
        assert_eq!(s.dex[0].chain, vec![1, 2, 3]);
        assert!(s.has_collected_base(1));
    }

    #[test]
    fn difficulty_scales_and_rescales_progress() {
        let mut s = State { baseline_set: true, ..Default::default() };
        assert_eq!(s.egg_threshold(), EGG_HATCH_THRESHOLD); // 0 in old saves means 1
        s.egg_usage = 1_000_000;
        s.set_difficulty(2.0);
        assert_eq!((s.egg_threshold(), s.egg_usage), (10_000_000, 2_000_000));
        s.set_difficulty(50.0); // clamped
        assert_eq!(s.difficulty(), 2.0);
    }

    #[test]
    fn branches_prefer_uncollected_final() {
        let mut rng = Rng::with_seed(1);
        let tree = node(133, vec![node(134, vec![]), node(135, vec![])]);
        let collected = HashSet::from(["133:134".to_string()]);
        for _ in 0..20 {
            assert_eq!(plan(&tree, 133, &collected, &mut rng), vec![133, 135]);
        }
    }

    #[test]
    fn keep_animated_cuts_late_gen_evolutions() {
        let tree = node(439, vec![node(122, vec![node(866, vec![])])]);
        assert_eq!(
            tree.keep_animated().unwrap(),
            node(439, vec![node(122, vec![])])
        );
    }

    #[test]
    fn choose_base_halves_collected_weight() {
        let mut rng = Rng::with_seed(3);
        let mut s = State::default();
        s.collected_finals.insert("1:3".into());
        let index = [(1, 100), (4, 100)];
        let picks = (0..3000)
            .filter(|_| choose_base(&index, &s, &mut rng) == Some(1))
            .count();
        assert!((800..1200).contains(&picks), "{picks}"); // ~1/3
    }
}
