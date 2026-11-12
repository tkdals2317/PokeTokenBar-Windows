//! Saved companion state and every growth rule that mutates it (companion.md §3.3-3.12, §4.2-4.3).
//! Pure: no network, no Tauri. `mod.rs` fetches lines/details and feeds them in.

use super::balance::{self, EGG_HATCH_THRESHOLD, MAX_TOKEN_VALUE, Rarity, phase_threshold, scaled};
use super::profile::{Details, Profile, fnv1a, uuid};
use super::{MAX_SPECIES_ID, NATURES, form, text};
use fastrand::Rng;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

pub const SHINY_DENOMINATOR: u64 = 64;
pub const SHINY_CHARM_DENOMINATOR: u64 = 48;
pub const DITTO_DISGUISE_DENOMINATOR: u64 = 128;
const MAX_EVOLUTIONS_PER_APPLY: usize = 50;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
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

    /// Number of forms on the longest path (`EvoLine.totalForms`).
    pub fn depth(&self) -> usize {
        1 + self.children.iter().map(EvoNode::depth).max().unwrap_or(0)
    }

    /// Drops species without an animated sprite, and everything after them.
    pub fn keep_animated(&self) -> Option<EvoNode> {
        (self.id <= MAX_SPECIES_ID).then(|| EvoNode {
            id: self.id,
            children: self.children.iter().filter_map(EvoNode::keep_animated).collect(),
        })
    }
}

/// species id → language code → name
pub type Names = HashMap<u32, HashMap<String, String>>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub base_id: u32,
    pub tree: EvoNode,
    pub rarity: Rarity,
    pub names: Names,
}

impl Default for Line {
    fn default() -> Self {
        Line { base_id: 0, tree: EvoNode::default(), rarity: Rarity::Common, names: Names::new() }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Prayers {
    pub count: u32,
}

/// The Pokémon being raised. While disguised, everything looks like the disguise species.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Mon {
    pub line: Line,
    /// Realized evolution path so far.
    pub path_ids: Vec<u32>,
    /// Full route chosen at hatch (branches resolved up front).
    pub planned_ids: Vec<u32>,
    pub stage_index: usize,
    pub used_at_stage: u64,
    pub total_forms: usize,
    pub is_shiny: bool,
    /// One of `NATURES`; empty for none.
    pub nature: String,
    pub form: Option<String>,
    pub profile: Option<Profile>,
    pub has_growth_boost: bool,
    pub ditto_disguise: Option<u32>,
    pub ditto_revealed: bool,
    pub prayers: Option<Prayers>,
}

impl Mon {
    pub fn base_id(&self) -> u32 {
        self.line.base_id
    }
    pub fn rarity(&self) -> Rarity {
        self.line.rarity
    }
    pub fn current_id(&self) -> u32 {
        self.path_ids.get(self.stage_index).or(self.path_ids.last()).copied().unwrap_or(self.line.base_id)
    }
    pub fn growth_multiplier(&self) -> u64 {
        if self.has_growth_boost { balance::REPEAT_GROWTH_MULTIPLIER } else { 1 }
    }
    /// Threshold before difficulty (rarity, forms and boost).
    pub fn phase_threshold(&self) -> u64 {
        phase_threshold(self.rarity(), self.total_forms, self.stage_index, self.growth_multiplier())
    }
    pub fn disguised(&self) -> bool {
        self.ditto_disguise.is_some() && !self.ditto_revealed
    }
    /// A disguised Ditto hides its shiny until the reveal. Every display path uses this.
    pub fn displays_shiny(&self) -> bool {
        self.is_shiny && (self.ditto_disguise.is_none() || self.ditto_revealed)
    }
    pub fn reached(&self) -> &[u32] {
        &self.path_ids[..(self.stage_index + 1).min(self.path_ids.len())]
    }
    pub fn is_final_stage(&self) -> bool {
        self.line.tree.find(self.current_id()).is_some_and(|n| n.children.is_empty())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DexEntry {
    pub id: String,
    pub base_id: u32,
    pub final_id: u32,
    pub chain: Vec<u32>,
    pub rarity: Rarity,
    pub is_shiny: bool,
    pub nature: String,
    pub names: Names,
    /// RFC 3339. For a released record this is the release time.
    pub caught_at: String,
    pub form: Option<String>,
    pub profile: Option<Profile>,
    pub prayers: Option<Prayers>,
    pub names_version: Option<u32>,
    /// Set when the Pokémon was released for a new egg (not graduated).
    pub released_at: Option<String>,
}

impl Default for Rarity {
    fn default() -> Self {
        Rarity::Common
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Event {
    // `text`: the localized event line, so the pet need not resolve names from a view it may not have yet.
    Hatched { id: u32, shiny: bool, text: String },
    Evolved { id: u32, text: String },
    Graduated { id: u32, text: String },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CelebrationKind {
    Hatch,
    Evolve,
    DittoReveal,
}

#[derive(Debug, Clone, Serialize)]
pub struct Celebration {
    pub seq: u64,
    pub kind: CelebrationKind,
    pub shiny: bool,
}

/// `latestEvent` for the floating pet: body text of the newest notification.
#[derive(Debug, Clone, Serialize)]
pub struct LatestEvent {
    pub seq: u64,
    pub text: String,
    pub at: String,
}

/// Display inputs from the latest usage refresh (companion.md §3.10).
#[derive(Debug, Clone, Default)]
pub struct Inputs {
    pub has_usage_data: bool,
    pub today_tokens: u64,
    pub tokens_per_minute: f64,
    pub limit_warning: bool,
    /// Seconds since the newest usage; `None` = unknown (never sulks).
    pub idle_secs: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DisplayState {
    Egg,
    Idle,
    Working,
    Focus,
    Tired,
    Sleep,
    Sulk,
    LevelUp,
}

/// In-memory state that is never saved: event windows, feedback, caches, outbox.
#[derive(Debug, Default)]
pub struct Runtime {
    pub lang: String,
    pub ditto_rolling: bool,
    pub hatch_retry_delayed: bool,
    pub ditto_pending: bool,
    pub event_until: Option<Instant>,
    pub just_evolved_to: Option<String>,
    pub just_graduated: Option<String>,
    pub celebration: Option<Celebration>,
    pub candy: Option<(u64, u64)>,
    pub mint_seq: u64,
    pub seq: u64,
    pub latest_event: Option<LatestEvent>,
    /// (title, body) system notifications to send after the lock is released.
    pub outbox: Vec<(String, String)>,
    pub events: Vec<Event>,
    pub details: HashMap<u32, Details>,
    /// `pokemon-form` id → localized names.
    pub form_labels: HashMap<u32, HashMap<String, String>>,
    pub inputs: Inputs,
    pub last_seen_today: Option<(String, u64)>,
    pub last_observed_usage: Option<Instant>,
}

impl Runtime {
    fn next_seq(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }
}

fn lenient_opt<'de, D: Deserializer<'de>, T: for<'a> Deserialize<'a>>(d: D) -> Result<Option<T>, D::Error> {
    Ok(serde_json::Value::deserialize(d).ok().and_then(|v| serde_json::from_value(v).ok()))
}

fn lenient_vec<'de, D: Deserializer<'de>, T: for<'a> Deserialize<'a>>(d: D) -> Result<Vec<T>, D::Error> {
    let v = serde_json::Value::deserialize(d)?;
    Ok(v.as_array().into_iter().flatten().filter_map(|e| serde_json::from_value(e.clone()).ok()).collect())
}

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct State {
    /// Usage before the first real observation isn't counted (no retroactive growth).
    pub baseline_set: bool,
    /// provider → today's tokens already credited. `None` (legacy) and empty mean different things.
    pub ledger: Option<HashMap<String, u64>>,
    pub last_date: String,
    pub used_since_install: u64,
    pub egg_usage: u64,
    pub egg_tier: Option<Rarity>,
    pub pending_hatch_id: Option<u32>,
    pub pending_form: Option<String>,
    pub egg_prayers: Option<Prayers>,
    #[serde(deserialize_with = "lenient_opt")]
    pub active: Option<Mon>,
    #[serde(deserialize_with = "lenient_vec")]
    pub dex: Vec<DexEntry>,
    /// "base:final" pairs, used to prefer new branches and down-weight repeats.
    pub collected_finals: HashSet<String>,
    /// Growth multiplier 0.1…2.0 (0 in v0.2 saves meant 1; migrated on load).
    pub growth_difficulty: f64,
    /// Wallet, bag and candy grants. Owned by the shop-bag-dex area (`economy`), saved in the same file.
    #[serde(flatten)]
    pub economy: crate::economy::EconomyState,
    #[serde(skip)]
    pub rt: Runtime,
}

fn now_rfc3339() -> String {
    chrono::Local::now().to_rfc3339()
}

impl State {
    pub fn difficulty(&self) -> f64 {
        balance::clamp_difficulty(self.growth_difficulty)
    }

    pub fn egg_threshold(&self) -> u64 {
        scaled(EGG_HATCH_THRESHOLD, self.difficulty())
    }

    /// Every threshold consumer goes through here (difficulty applied once).
    pub fn stage_threshold(&self, m: &Mon) -> u64 {
        scaled(m.phase_threshold(), self.difficulty())
    }

    pub fn threshold(&self) -> Option<u64> {
        self.active.as_ref().map(|m| self.stage_threshold(m))
    }

    pub fn has_collected_final(&self, base: u32) -> bool {
        let prefix = format!("{base}:");
        self.collected_finals.iter().any(|f| f.starts_with(&prefix))
    }

    pub fn shiny_denominator(&self) -> u64 {
        if self.economy.inventory.get("shinyCharm").copied().unwrap_or(0) > 0 {
            SHINY_CHARM_DENOMINATOR
        } else {
            SHINY_DENOMINATOR
        }
    }

    // ---- names ----

    fn names_for(&self, id: u32) -> Option<&HashMap<String, String>> {
        self.active
            .iter()
            .map(|m| &m.line.names)
            .chain(self.dex.iter().rev().map(|d| &d.names))
            .find_map(|n| n.get(&id))
    }

    /// Localized species name: app language, then en, then "#id".
    pub fn name_of(&self, id: u32) -> String {
        self.names_for(id).and_then(|n| text::resolve_name(n, &self.rt.lang)).unwrap_or_else(|| format!("#{id}"))
    }

    /// Species name with its form: "Unown [A]", "Shaymin [Sky Forme]".
    pub fn form_name(&self, id: u32, form_value: Option<&str>) -> String {
        let label = form::name_resource(id, form_value).map(|res| {
            self.rt
                .form_labels
                .get(&res)
                .and_then(|n| text::resolve_name(n, &self.rt.lang))
                .unwrap_or_else(|| text::identifier(&form::resolved(id, form_value).unwrap_or_default()))
        });
        form::display_name(&self.name_of(id), id, form_value, label.as_deref())
    }

    fn notify(&mut self, title: String, body: String) {
        let seq = self.rt.next_seq();
        self.rt.latest_event = Some(LatestEvent { seq, text: body.clone(), at: now_rfc3339() });
        self.rt.outbox.push((title, body));
    }

    fn celebrate(&mut self, kind: CelebrationKind, shiny: bool) {
        let seq = self.rt.next_seq();
        self.rt.celebration = Some(Celebration { seq, kind, shiny });
    }

    fn open_event_window(&mut self, secs: u64) {
        self.rt.event_until = Some(Instant::now() + Duration::from_secs(secs));
    }

    // ---- ledger (§3.11) ----

    /// Credits today's per-provider growth. `today` holds only providers with today-dated data.
    /// Returns the delta, already added to `used_since_install`.
    pub fn credit(&mut self, today: &HashMap<String, u64>, date: &str, has_usage_data: bool) -> u64 {
        let has_current = has_usage_data && !today.is_empty();
        if !has_current {
            return 0; // nothing observed: the baseline and the ledger stay put
        }
        if !self.baseline_set {
            self.baseline_set = true;
            self.ledger = Some(today.clone());
            self.last_date = date.to_string();
            return 0;
        }
        let Some(ledger) = self.ledger.as_mut() else {
            // Legacy save without per-provider ledger: seed, credit nothing.
            self.ledger = Some(today.clone());
            self.last_date = date.to_string();
            return 0;
        };
        // v0.2 keyed Claude Code as "claude"; providers now use the macOS id.
        if let Some(v) = ledger.remove("claude") {
            ledger.entry("claude_code".into()).or_insert(v);
        }
        let delta = if date != self.last_date {
            // A new day: known providers open at 0 so one reporting late is credited in full.
            self.last_date = date.to_string();
            ledger.values_mut().for_each(|v| *v = 0);
            ledger.extend(today.iter().map(|(k, v)| (k.clone(), *v)));
            today.values().sum()
        } else {
            let mut delta = 0;
            for (provider, &current) in today {
                match ledger.get(provider) {
                    None => {} // newly seen provider: seed, don't backfill its past logs
                    Some(&prev) if current < prev => {
                        eprintln!("companion usage regression provider={provider} date={date} previous={prev} current={current}");
                    }
                    Some(&prev) => delta += current - prev,
                }
                ledger.insert(provider.clone(), current);
            }
            delta
        };
        self.used_since_install = (self.used_since_install + delta).min(MAX_TOKEN_VALUE);
        delta
    }

    /// Routes growth to the egg or the active Pokémon.
    pub fn feed(&mut self, delta: u64, rng: &mut Rng) {
        if delta == 0 {
            return;
        }
        if self.active.is_none() {
            self.egg_usage = (self.egg_usage + delta).min(MAX_TOKEN_VALUE);
        } else {
            self.apply_usage(delta, rng);
        }
    }

    pub fn egg_ready(&self) -> bool {
        self.active.is_none() && self.baseline_set && self.egg_usage >= self.egg_threshold()
    }

    // ---- evolution (§3.7) ----

    fn pick_planned_child<'a>(&self, node: &'a EvoNode, base: u32, rng: &mut Rng) -> &'a EvoNode {
        let fresh: Vec<&EvoNode> = node
            .children
            .iter()
            .filter(|c| c.final_ids().iter().any(|f| !self.collected_finals.contains(&format!("{base}:{f}"))))
            .collect();
        let pool: Vec<&EvoNode> = if fresh.is_empty() { node.children.iter().collect() } else { fresh };
        pool[(rng.u64(..) % pool.len() as u64) as usize]
    }

    /// Full route from `root` to a leaf, one draw per branching node.
    pub fn make_plan(&self, root: &EvoNode, base: u32, rng: &mut Rng) -> Vec<u32> {
        let mut plan = vec![root.id];
        let mut node = root;
        while !node.children.is_empty() {
            node = self.pick_planned_child(node, base, rng);
            plan.push(node.id);
        }
        plan
    }

    /// Adds growth to the active Pokémon and evolves / graduates / starts the Ditto reveal.
    pub fn apply_usage(&mut self, delta: u64, rng: &mut Rng) {
        let Some(m) = self.active.as_mut() else { return };
        m.used_at_stage = (m.used_at_stage + delta).min(MAX_TOKEN_VALUE);
        self.reconcile_profile_growth();
        for _ in 0..MAX_EVOLUTIONS_PER_APPLY {
            let Some(a) = self.active.clone() else { break };
            let thr = self.stage_threshold(&a);
            if a.used_at_stage < thr {
                break;
            }
            let Some(node) = a.line.tree.find(a.current_id()).cloned() else { break };
            if a.disguised() {
                self.rt.ditto_pending = true; // mod.rs loads Ditto's line and calls `reveal_ditto`
                break;
            }
            if node.children.is_empty() {
                self.graduate();
                break;
            }
            let next_idx = a.stage_index + 1;
            let next = match a.planned_ids.get(next_idx) {
                Some(&p) if node.children.iter().any(|c| c.id == p) => p,
                _ => {
                    let child = self.pick_planned_child(&node, a.base_id(), rng).clone();
                    let route: Vec<u32> = std::iter::once(node.id).chain(self.make_plan(&child, a.base_id(), rng)).collect();
                    let repaired = repaired_plan(&a.path_ids, a.stage_index, &route);
                    eprintln!("evolve: repaired invalid planned path for base {}", a.base_id());
                    let m = self.active.as_mut().unwrap();
                    m.total_forms = repaired.len();
                    m.planned_ids = repaired;
                    child.id
                }
            };
            let old = self.form_name(a.current_id(), a.form.as_deref());
            let m = self.active.as_mut().unwrap();
            m.path_ids.truncate(next_idx);
            m.path_ids.push(next);
            m.stage_index = next_idx;
            m.used_at_stage -= thr; // overflow carries over
            if let (Some(p), Some(d)) = (m.profile.as_mut(), self.rt.details.get(&next)) {
                p.enrich(d);
            }
            let new = self.form_name(next, a.form.as_deref());
            self.rt.just_evolved_to = Some(new.clone());
            self.celebrate(CelebrationKind::Evolve, false);
            self.open_event_window(4);
            let lang = self.rt.lang.clone();
            let body = text::evolve_body(&lang, &old, &new);
            self.notify(text::evolve_title(&lang), body.clone());
            self.rt.events.push(Event::Evolved { id: next, text: body });
        }
        self.reconcile_profile_growth();
    }

    fn graduate(&mut self) {
        let Some(mut a) = self.active.take() else { return };
        let final_id = a.current_id();
        let rarity = a.rarity();
        if let Some(p) = a.profile.as_mut() {
            p.advance_growth(rarity.graduation_total(), rarity);
            if let Some(d) = self.rt.details.get(&final_id) {
                p.enrich(d);
            }
        }
        self.collected_finals.insert(format!("{}:{final_id}", a.base_id()));
        self.dex.push(dex_entry(&a, a.path_ids.clone(), a.is_shiny, None));
        let name = self.form_name(final_id, a.form.as_deref()); // resolves from the entry just saved
        self.rt.just_graduated = Some(name.clone());
        let lang = self.rt.lang.clone();
        let body = text::graduate_body(&lang, &name);
        self.notify(text::graduate_title(&lang), body.clone());
        self.open_event_window(6);
        self.rt.events.push(Event::Graduated { id: final_id, text: body });
        self.egg_usage = 0;
        self.egg_prayers = None;
        self.rt.hatch_retry_delayed = false;
    }

    // ---- hatch (§3.4-3.6) ----

    /// Capture-rate weighted pick; collected lines count half. A guaranteed tier filters first
    /// and never falls back to the full pool.
    pub fn choose_base(&self, index: &[super::pokeapi::BaseEntry], rng: &mut Rng) -> Option<u32> {
        let pool: Vec<_> = index.iter().filter(|e| self.egg_tier.is_none_or(|t| t.includes(e.capture_rate))).collect();
        let weights: Vec<u64> = pool
            .iter()
            .map(|e| {
                let w = e.capture_rate as u64;
                if self.has_collected_final(e.id) { (w / 2).max(1) } else { w.max(1) }
            })
            .collect();
        let total: u64 = weights.iter().sum();
        if total == 0 {
            return None;
        }
        let mut r = (rng.u64(..) % total) as i64;
        for (e, w) in pool.iter().zip(&weights) {
            r -= *w as i64;
            if r < 0 {
                return Some(e.id);
            }
        }
        pool.last().map(|e| e.id)
    }

    /// A form counts as collected only when every species of the line that has it owns it.
    pub fn collected_line_forms(&self, base: u32) -> Vec<String> {
        let species = form::form_species(base);
        form::line_forms(base)
            .into_iter()
            .filter(|f| {
                species.iter().all(|&s| !form::forms(s).contains(f) || self.collected_forms(s).contains(f))
            })
            .collect()
    }

    /// Forms of `species` owned in the dex or the active Pokémon's reached stages.
    pub fn collected_forms(&self, species: u32) -> Vec<String> {
        let mut out: Vec<String> = self
            .dex
            .iter()
            .filter(|d| d.chain.contains(&species))
            .filter_map(|d| form::resolved(species, d.form.as_deref()))
            .collect();
        if let Some(a) = self.active.as_ref().filter(|a| a.reached().contains(&species)) {
            out.extend(form::resolved(species, a.form.as_deref()));
        }
        out
    }

    /// Draws only for lines with forms, so other species keep their random sequence.
    pub fn roll_line_form(&self, base: u32, rng: &mut Rng) -> Option<String> {
        let forms = form::line_forms(base);
        if forms.is_empty() {
            return None;
        }
        Some(form::roll(rng.u64(..), &forms, &self.collected_line_forms(base)))
    }

    /// Hatches the egg into `line`. Returns false when the guaranteed tier rejects the line
    /// (the egg is kept and the pending roll discarded).
    pub fn hatch(&mut self, line: Line, rng: &mut Rng) -> bool {
        if let Some(tier) = self.egg_tier {
            if line.rarity.sort_rank() < tier.sort_rank() {
                self.pending_hatch_id = None;
                self.pending_form = None;
                self.rt.hatch_retry_delayed = self.egg_ready();
                return false;
            }
        }
        let pending_form = self.pending_form.take().and_then(|f| form::resolved_for_line(line.base_id, Some(&f)));
        self.pending_hatch_id = None;
        self.rt.hatch_retry_delayed = false;
        let overflow = self.egg_usage.saturating_sub(self.egg_threshold());
        self.egg_usage = 0;
        self.egg_tier = None;
        let prayers = self.egg_prayers.take();

        let odds = self.shiny_denominator();
        let is_shiny = rng.u64(..) % odds == 0;
        let nature = NATURES[(rng.u64(..) % NATURES.len() as u64) as usize].to_string();
        let ditto_disguise = (self.rt.ditto_rolling
            && line.rarity == Rarity::Common
            && line.tree.depth() >= 2
            && rng.u64(..) % DITTO_DISGUISE_DENOMINATOR == 0)
            .then_some(line.base_id);
        let plan = self.make_plan(&line.tree, line.base_id, rng);
        let mut profile = Profile::generate(rng.u64(..), 0, uuid());
        if let Some(d) = self.rt.details.get(&line.base_id) {
            profile.enrich(d);
        }
        let form_value = pending_form.or_else(|| self.roll_line_form(line.base_id, rng));
        let base = line.base_id;
        let rarity = line.rarity;
        self.active = Some(Mon {
            path_ids: vec![base],
            total_forms: plan.len(),
            planned_ids: plan,
            stage_index: 0,
            used_at_stage: 0,
            is_shiny,
            nature,
            form: form_value.clone(),
            profile: Some(profile),
            has_growth_boost: self.has_collected_final(base),
            ditto_disguise,
            ditto_revealed: false,
            prayers: prayers.clone(),
            line,
        });
        let show_shiny = is_shiny && ditto_disguise.is_none();
        let name = self.form_name(base, form_value.as_deref());
        let lang = self.rt.lang.clone();
        let body = match prayers.filter(|p| p.count > 0) {
            Some(p) => text::prayer_hatch_body(&lang, &name, p.count, show_shiny, rarity),
            None if show_shiny => text::shiny_hatch_body(&lang, &name, odds),
            None => text::hatch_body(&lang, &name),
        };
        self.notify(text::hatch_title(&lang, show_shiny), body);
        self.rt.events.push(Event::Hatched { id: base, shiny: show_shiny, text: text::hatch_body(&lang, &name) });
        self.rt.just_evolved_to = None;
        self.open_event_window(4);
        if overflow > 0 {
            self.apply_usage(overflow, rng);
        }
        // After the overflow evolutions, so a shiny burst is not overwritten.
        if self.active.is_some() {
            self.celebrate(CelebrationKind::Hatch, show_shiny);
        }
        true
    }

    // ---- Ditto (§3.8) ----

    pub fn reveal_ditto(&mut self, ditto: Line, rng: &mut Rng) {
        self.rt.ditto_pending = false;
        let Some(a) = self.active.clone().filter(|a| a.disguised()) else { return };
        let first = self.stage_threshold(&a);
        if a.used_at_stage < first {
            return;
        }
        let disguise_name = self.name_of(a.base_id());
        let plan = self.make_plan(&ditto.tree, ditto.base_id, rng);
        let old_rarity = a.rarity();
        let new_rarity = ditto.rarity;
        let mut m = a;
        m.used_at_stage = m.used_at_stage - first;
        m.path_ids = vec![ditto.base_id];
        m.total_forms = plan.len();
        m.planned_ids = plan;
        m.stage_index = 0;
        m.ditto_revealed = true;
        m.form = None;
        m.has_growth_boost = self.has_collected_final(ditto.base_id);
        if let Some(p) = m.profile.as_mut() {
            p.rebase_for_species_identity(old_rarity, new_rarity);
            if let Some(d) = self.rt.details.get(&ditto.base_id) {
                p.enrich(d);
            }
        }
        m.line = ditto;
        let shiny = m.is_shiny;
        self.active = Some(m);
        self.celebrate(CelebrationKind::DittoReveal, shiny);
        self.open_event_window(5);
        let lang = self.rt.lang.clone();
        self.notify(text::ditto_title(&lang, shiny), text::ditto_body(&lang, &disguise_name, shiny));
        self.apply_usage(0, rng);
    }

    // ---- shop hooks (§3.7 release, §3.3 mint) ----

    /// Keeps the active Pokémon's reached forms as a released record and restarts the egg.
    /// `collected_finals` is left untouched (release ≠ graduate).
    pub fn release_for_egg(&mut self, tier: Option<Rarity>) -> bool {
        // `FreshEgg.shopTiers`: plain, uncommon-or-better, rare-or-better.
        if matches!(tier, Some(Rarity::Common | Rarity::Legendary)) {
            return false;
        }
        let Some(a) = self.active.take() else { return false };
        let reached: Vec<u32> = a.path_ids.iter().take((a.stage_index + 1).max(1)).copied().collect();
        let chain = if reached.is_empty() { vec![a.base_id()] } else { reached };
        let now = now_rfc3339();
        self.dex.push(dex_entry(&a, chain, a.displays_shiny(), Some(now)));
        self.egg_usage = 0;
        self.egg_prayers = None;
        self.egg_tier = tier;
        self.pending_hatch_id = None;
        self.pending_form = None;
        self.rt.hatch_retry_delayed = false;
        self.rt.ditto_pending = false;
        self.rt.event_until = None;
        self.rt.just_evolved_to = None;
        self.rt.just_graduated = None;
        true
    }

    /// Picks a nature different from the current one (any of 25 when there is none).
    pub fn mint_nature(&mut self, rng: &mut Rng) -> bool {
        let Some(m) = self.active.as_mut() else { return false };
        let pool: Vec<&str> = NATURES.iter().copied().filter(|n| *n != m.nature).collect();
        m.nature = pool[(rng.u64(..) % pool.len() as u64) as usize].to_string();
        self.rt.mint_seq = self.rt.next_seq();
        true
    }

    /// Rare Candy XP: through `apply_usage` only (never `used_since_install`, never scaled).
    pub fn candy_xp(&mut self, xp: u64, rng: &mut Rng) {
        if self.active.is_none() {
            return;
        }
        let seq = self.rt.next_seq();
        self.rt.candy = Some((seq, xp));
        self.apply_usage(xp, rng);
    }

    // ---- difficulty (§3.1) ----

    /// Rescales banked progress; never evolves, hatches or graduates.
    pub fn set_difficulty(&mut self, value: f64) {
        let new = balance::clamp_difficulty(value);
        let old = self.difficulty();
        if new == old {
            return;
        }
        self.rescale_banked(old, new);
        self.growth_difficulty = new;
    }

    fn rescale_banked(&mut self, old: f64, new: f64) {
        match self.active.as_mut() {
            Some(m) => m.used_at_stage = balance::rescale(m.used_at_stage, m.phase_threshold(), old, new),
            None => self.egg_usage = balance::rescale(self.egg_usage, EGG_HATCH_THRESHOLD, old, new),
        }
    }

    /// v0.2 saved 0 for "default"; other out-of-range values rescale into 0.1…2.0.
    pub fn migrate_difficulty(&mut self) {
        let stored = self.growth_difficulty;
        let previous = if !stored.is_finite() || stored <= 0.0 { 1.0 } else { stored.clamp(0.0001, 20.0) };
        let new = balance::clamp_difficulty(previous);
        if previous != new {
            self.rescale_banked(previous, new);
        }
        self.growth_difficulty = new;
    }

    // ---- profiles (§3.9) ----

    /// Advances the active profile's growth from the realized stages (standard units).
    pub fn reconcile_profile_growth(&mut self) {
        let thr = self.active.as_ref().map(|a| self.stage_threshold(a)).unwrap_or(1);
        let Some(a) = self.active.as_mut() else { return };
        let completed = balance::reconstructed_growth(a.rarity(), a.total_forms, a.stage_index, 0);
        let standard = phase_threshold(a.rarity(), a.total_forms, a.stage_index, 1);
        let fraction = (a.used_at_stage as f64 / thr.max(1) as f64).clamp(0.0, 1.0);
        let candidate = (completed + (standard as f64 * fraction).floor() as u64).min(a.rarity().graduation_total());
        let current = a.current_id();
        if let Some(p) = a.profile.as_mut() {
            p.advance_growth(candidate, a.line.rarity);
            if let Some(d) = self.rt.details.get(&current) {
                p.enrich(d);
            }
        }
    }

    /// Enriches the active profile once the current species' details are known.
    pub fn add_details(&mut self, d: Details) {
        let id = d.species_id;
        self.rt.details.insert(id, d);
        if let Some(a) = self.active.as_mut().filter(|a| a.current_id() == id) {
            if let (Some(p), Some(d)) = (a.profile.as_mut(), self.rt.details.get(&id)) {
                p.enrich(d);
            }
        }
    }

    /// One-time profile creation for saves written before profiles. Returns true when it changed.
    pub fn migrate_profiles(&mut self) -> bool {
        let mut changed = false;
        let last_date = self.last_date.clone();
        if let Some(a) = self.active.as_mut().filter(|a| a.profile.is_none()) {
            let ids: Vec<String> = a.path_ids.iter().map(u32::to_string).collect();
            let key = format!("active:{}:{}:{last_date}", a.base_id(), ids.join(","));
            a.profile = Some(Profile::generate(fnv1a(&key), 0, uuid()));
            changed = true;
        }
        for d in self.dex.iter_mut().filter(|d| d.profile.is_none()) {
            if d.id.is_empty() {
                d.id = uuid();
            }
            let growth = if d.released_at.is_none() {
                d.rarity.graduation_total()
            } else {
                balance::reconstructed_growth(d.rarity, d.chain.len().max(1), d.chain.len().saturating_sub(1), 0)
            };
            let mut p = Profile::generate(fnv1a(&format!("dex:{}:{}", d.id, d.final_id)), growth, d.id.clone());
            p.advance_growth(growth, d.rarity);
            d.profile = Some(p);
            changed = true;
        }
        self.reconcile_profile_growth();
        changed
    }

    // ---- load-time repair (§3.7 normalization, §4.3 sanitize) ----

    /// Fits the saved path and plan to the line tree. A complete valid plan is reused (no RNG).
    pub fn normalize_evolution(&mut self, rng: &mut Rng) {
        let Some(a) = self.active.clone() else { return };
        let root = &a.line.tree;
        let realized = longest_valid_path(&a.path_ids, root);
        let candidate = longest_valid_path(&a.planned_ids, root);
        let reuse = candidate.0 == a.planned_ids
            && candidate.0.starts_with(&realized.0)
            && candidate.1.children.is_empty();
        let plan = if reuse {
            candidate.0
        } else {
            let suffix = self.make_plan(&realized.1, a.base_id(), rng);
            realized.0.iter().copied().chain(suffix.into_iter().skip(1)).collect()
        };
        let m = self.active.as_mut().unwrap();
        m.stage_index = realized.0.len() - 1;
        m.path_ids = realized.0;
        m.total_forms = plan.len();
        m.planned_ids = plan;
    }

    pub fn sanitize(&mut self) {
        let clamp = |v: &mut u64| *v = (*v).min(MAX_TOKEN_VALUE);
        clamp(&mut self.used_since_install);
        clamp(&mut self.egg_usage);
        clamp(&mut self.economy.spent_tokens);
        if self.active.is_some() {
            self.egg_tier = None;
            self.pending_hatch_id = None;
            self.egg_prayers = None;
        }
        if self.egg_tier == Some(Rarity::Legendary) {
            self.egg_tier = None;
        }
        if let Some(id) = self.pending_hatch_id {
            self.pending_form = form::resolved_for_line(id, self.pending_form.as_deref());
        } else {
            self.pending_form = None;
        }
        if let Some(a) = self.active.as_mut() {
            if a.path_ids.is_empty() {
                a.path_ids = vec![a.line.base_id];
            }
            let current = a.current_id();
            dedup(&mut a.path_ids);
            dedup(&mut a.planned_ids);
            a.stage_index = a.path_ids.iter().position(|id| *id == current).unwrap_or(a.path_ids.len() - 1);
            if a.planned_ids.is_empty() {
                a.planned_ids = a.path_ids.clone();
            }
            if a.total_forms == 0 {
                a.total_forms = a.planned_ids.len();
            }
            a.total_forms = a.total_forms.clamp(1, 12);
            clamp(&mut a.used_at_stage);
            a.form = form::resolved_for_line(a.line.base_id, a.form.as_deref());
            if let Some(p) = a.profile.as_mut() {
                p.sanitize();
            }
        }
        for d in &mut self.dex {
            dedup(&mut d.chain);
            if let Some(p) = d.profile.as_mut() {
                p.sanitize();
            }
        }
    }

    // ---- ownership / representative (§3.12) ----

    /// Dex chains or the active Pokémon's reached stages (never the plan), form-aware.
    pub fn owns_species(&self, id: u32, form_value: Option<&str>) -> bool {
        let want = form::resolved(id, form_value);
        self.dex.iter().any(|d| d.chain.contains(&id) && form::resolved(id, d.form.as_deref()) == want)
            || self
                .active
                .as_ref()
                .is_some_and(|a| a.reached().contains(&id) && form::resolved(id, a.form.as_deref()) == want)
    }

    pub fn owns_shiny_species(&self, id: u32, form_value: Option<&str>) -> bool {
        let want = form::resolved(id, form_value);
        self.dex.iter().any(|d| d.is_shiny && d.chain.contains(&id) && form::resolved(id, d.form.as_deref()) == want)
            || self.active.as_ref().is_some_and(|a| {
                a.reached().contains(&id) && form::resolved(id, a.form.as_deref()) == want && a.displays_shiny()
            })
    }

    // ---- display (§3.10, §5.6) ----

    /// Expires the levelUp window (and its texts) once it has passed.
    pub fn expire_events(&mut self, now: Instant) {
        if self.rt.event_until.is_some_and(|u| now > u) {
            self.rt.event_until = None;
            self.rt.just_evolved_to = None;
            self.rt.just_graduated = None;
        }
    }

    pub fn display_state(&self, now: Instant) -> DisplayState {
        let i = &self.rt.inputs;
        if self.active.is_none() {
            return DisplayState::Egg;
        }
        if self.rt.just_graduated.is_some() || self.rt.event_until.is_some_and(|u| now < u) {
            return DisplayState::LevelUp;
        }
        if i.limit_warning {
            return DisplayState::Tired;
        }
        if i.has_usage_data && i.idle_secs.is_some_and(|s| s >= balance::SULK_AFTER_IDLE_SECS) {
            return DisplayState::Sulk;
        }
        if !i.has_usage_data || i.today_tokens == 0 {
            return DisplayState::Sleep;
        }
        match i.tokens_per_minute {
            t if t <= 1000.0 => DisplayState::Idle,
            t if t < 100_000.0 => DisplayState::Working,
            _ => DisplayState::Focus,
        }
    }

    /// Caps the reported idle time by this app's own observation of today's total rising.
    pub fn effective_idle(&mut self, reported: Option<u64>, date: &str, today: u64, has_current: bool, now: Instant) -> Option<u64> {
        if has_current {
            if self.rt.last_seen_today.as_ref().is_some_and(|(d, t)| d == date && today > *t) {
                self.rt.last_observed_usage = Some(now);
            }
            self.rt.last_seen_today = Some((date.to_string(), today));
        }
        let reported = reported?;
        Some(match self.rt.last_observed_usage {
            Some(o) => reported.min(now.saturating_duration_since(o).as_secs()),
            None => reported,
        })
    }

    /// Realized path, then single-child future species, then one `?` at a branch.
    pub fn line_nodes(&self) -> Vec<(Option<u32>, &'static str)> {
        let Some(a) = &self.active else { return vec![] };
        let mut out: Vec<(Option<u32>, &'static str)> =
            a.path_ids.iter().enumerate().map(|(i, id)| (Some(*id), if i == a.stage_index { "current" } else { "done" })).collect();
        if let Some(mut node) = a.line.tree.find(a.current_id()) {
            while node.children.len() == 1 {
                node = &node.children[0];
                out.push((Some(node.id), "future"));
            }
            if node.children.len() > 1 {
                out.push((None, "future"));
            }
        }
        out
    }

    /// Integer-floor growth percent of the egg or the current stage.
    pub fn growth_percent(&self) -> (u64, bool) {
        let (credits, total, egg) = match &self.active {
            None => (self.egg_usage, self.egg_threshold(), true),
            Some(m) => (m.used_at_stage, self.stage_threshold(m), false),
        };
        (if total > 0 { credits.min(total) * 100 / total } else { 0 }, egg)
    }
}

fn dedup(v: &mut Vec<u32>) {
    let mut seen = HashSet::new();
    v.retain(|x| seen.insert(*x));
}

fn dex_entry(a: &Mon, chain: Vec<u32>, is_shiny: bool, released_at: Option<String>) -> DexEntry {
    let names: Names = chain.iter().filter_map(|id| a.line.names.get(id).map(|n| (*id, n.clone()))).collect();
    let names_version = (names.len() == chain.len()).then_some(1);
    DexEntry {
        id: a.profile.as_ref().map(|p| p.instance_id.clone()).unwrap_or_else(uuid),
        base_id: a.base_id(),
        final_id: *chain.last().unwrap_or(&a.base_id()),
        chain,
        rarity: a.rarity(),
        is_shiny,
        nature: a.nature.clone(),
        names,
        caught_at: released_at.clone().unwrap_or_else(now_rfc3339),
        form: a.form.clone(),
        profile: a.profile.clone(),
        prayers: a.prayers.clone(),
        names_version,
        released_at,
    }
}

/// `repairedPlan`: realized prefix up to the current stage, then the fallback route if it continues it.
pub fn repaired_plan(realized: &[u32], stage_index: usize, fallback: &[u32]) -> Vec<u32> {
    if realized.is_empty() {
        return fallback.to_vec();
    }
    let prefix = &realized[..=stage_index.min(realized.len() - 1)];
    if fallback.first() == prefix.last() {
        prefix.iter().chain(fallback.iter().skip(1)).copied().collect()
    } else {
        prefix.to_vec()
    }
}

/// Longest run of `ids` that follows the tree from its root, and the node it ends on.
fn longest_valid_path(ids: &[u32], root: &EvoNode) -> (Vec<u32>, EvoNode) {
    let mut path = vec![root.id];
    let mut node = root;
    if ids.first() != Some(&root.id) {
        return (path, node.clone());
    }
    for id in &ids[1..] {
        let Some(child) = node.children.iter().find(|c| c.id == *id) else { break };
        path.push(*id);
        node = child;
    }
    (path, node.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::companion::DITTO_ID;
    use crate::companion::pokeapi::BaseEntry;

    fn node(id: u32, children: Vec<EvoNode>) -> EvoNode {
        EvoNode { id, children }
    }

    fn line(base: u32, tree: EvoNode, rarity: Rarity) -> Line {
        Line { base_id: base, tree, rarity, names: Names::new() }
    }

    fn bulbasaur() -> Line {
        line(1, node(1, vec![node(2, vec![node(3, vec![])])]), Rarity::Common)
    }

    fn ready_state() -> State {
        let mut s = State { baseline_set: true, growth_difficulty: 1.0, ..Default::default() };
        s.rt.lang = "en".into();
        s
    }

    #[test]
    fn credit_follows_the_swift_ledger() {
        let mut s = State { growth_difficulty: 1.0, ..Default::default() };
        let day = |pairs: &[(&str, u64)]| pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect::<HashMap<_, _>>();
        // Cold start: no data → no baseline.
        assert_eq!(s.credit(&HashMap::new(), "d1", false), 0);
        assert!(!s.baseline_set);
        assert_eq!(s.credit(&day(&[("claude_code", 100)]), "d1", true), 0);
        assert!(s.baseline_set);
        assert_eq!(s.credit(&day(&[("claude_code", 150), ("codex", 10)]), "d1", true), 50); // codex seeded
        assert_eq!(s.credit(&day(&[("claude_code", 120)]), "d1", true), 0); // regression rebases
        assert_eq!(s.credit(&day(&[("claude_code", 130), ("codex", 15)]), "d1", true), 15);
        // New day: codex missing in the first refresh keeps a 0 baseline.
        assert_eq!(s.credit(&day(&[("claude_code", 5)]), "d2", true), 5);
        assert_eq!(s.ledger.as_ref().unwrap()["codex"], 0);
        assert_eq!(s.credit(&day(&[("claude_code", 5), ("codex", 40)]), "d2", true), 40); // late provider in full
        assert_eq!(s.used_since_install, 110);
        // Refresh without data leaves everything alone.
        assert_eq!(s.credit(&HashMap::new(), "d3", true), 0);
        assert_eq!(s.last_date, "d2");
    }

    #[test]
    fn legacy_ledger_seeds_without_credit() {
        let mut s = State { baseline_set: true, ledger: None, ..Default::default() };
        assert_eq!(s.credit(&HashMap::from([("codex".to_string(), 500)]), "d1", true), 0);
        assert_eq!(s.credit(&HashMap::from([("codex".to_string(), 600)]), "d1", true), 100);
    }

    #[test]
    fn hatch_evolve_graduate_with_overflow() {
        let mut rng = Rng::with_seed(7);
        let mut s = ready_state();
        s.feed(EGG_HATCH_THRESHOLD + 1_000, &mut rng);
        assert!(s.egg_ready());
        assert!(s.hatch(bulbasaur(), &mut rng));
        let a = s.active.as_ref().unwrap();
        assert_eq!((a.used_at_stage, a.total_forms), (1_000, 3));
        assert!(a.profile.is_some());
        assert_eq!(s.rt.outbox.len(), 1);

        s.feed(125_000_000 + 250_000_000, &mut rng);
        let a = s.active.as_ref().unwrap();
        assert_eq!((a.current_id(), a.used_at_stage), (3, 1_000));
        assert_eq!(s.rt.just_evolved_to.as_deref(), Some("#3"));
        assert_eq!(s.rt.outbox.last().unwrap().1, "#2 evolved into #3!");

        s.feed(375_000_000, &mut rng);
        assert!(s.active.is_none());
        assert_eq!(s.dex[0].chain, vec![1, 2, 3]);
        assert!(s.dex[0].released_at.is_none());
        assert_eq!(s.dex[0].profile.as_ref().unwrap().level, 100);
        assert!(s.has_collected_final(1));
        assert_eq!(s.display_state(Instant::now()), DisplayState::Egg);
    }

    #[test]
    fn repeat_line_gets_growth_boost() {
        let mut rng = Rng::with_seed(1);
        let mut s = ready_state();
        s.collected_finals.insert("1:3".into());
        s.egg_usage = EGG_HATCH_THRESHOLD;
        s.hatch(bulbasaur(), &mut rng);
        assert!(s.active.as_ref().unwrap().has_growth_boost);
        assert_eq!(s.threshold(), Some(62_500_000));
    }

    #[test]
    fn guaranteed_tier_rejects_lower_line_and_filters_index() {
        let mut rng = Rng::with_seed(2);
        let mut s = ready_state();
        s.egg_tier = Some(Rarity::Rare);
        s.egg_usage = EGG_HATCH_THRESHOLD;
        s.pending_hatch_id = Some(1);
        assert!(!s.hatch(bulbasaur(), &mut rng));
        assert!(s.active.is_none() && s.pending_hatch_id.is_none() && s.rt.hatch_retry_delayed);
        let index = [BaseEntry { id: 1, capture_rate: 45 }, BaseEntry { id: 4, capture_rate: 200 }];
        for _ in 0..50 {
            assert_eq!(s.choose_base(&index, &mut rng), Some(1));
        }
        s.egg_tier = Some(Rarity::Rare);
        assert_eq!(s.choose_base(&[BaseEntry { id: 4, capture_rate: 200 }], &mut rng), None);
    }

    #[test]
    fn choose_base_halves_collected_weight() {
        let mut rng = Rng::with_seed(3);
        let mut s = State::default();
        s.collected_finals.insert("1:3".into());
        let index = [BaseEntry { id: 1, capture_rate: 100 }, BaseEntry { id: 4, capture_rate: 100 }];
        let picks = (0..3000).filter(|_| s.choose_base(&index, &mut rng) == Some(1)).count();
        assert!((800..1200).contains(&picks), "{picks}"); // ~1/3
    }

    #[test]
    fn shiny_charm_lowers_odds() {
        let mut s = State::default();
        assert_eq!(s.shiny_denominator(), 64);
        s.economy.inventory.insert("shinyCharm".into(), 1);
        assert_eq!(s.shiny_denominator(), 48);
    }

    #[test]
    fn ditto_disguise_reveals_at_first_threshold() {
        let mut rng = Rng::with_seed(4);
        let mut s = ready_state();
        s.egg_usage = EGG_HATCH_THRESHOLD;
        s.hatch(bulbasaur(), &mut rng);
        {
            let a = s.active.as_mut().unwrap();
            a.ditto_disguise = Some(1);
            a.is_shiny = true;
        }
        assert!(!s.active.as_ref().unwrap().displays_shiny());
        s.feed(125_000_000 + 7, &mut rng);
        assert!(s.rt.ditto_pending);
        assert_eq!(s.active.as_ref().unwrap().current_id(), 1); // no evolution while disguised
        s.reveal_ditto(line(DITTO_ID, node(DITTO_ID, vec![]), Rarity::Rare), &mut rng);
        let a = s.active.as_ref().unwrap();
        assert_eq!((a.base_id(), a.used_at_stage, a.total_forms), (DITTO_ID, 7, 1));
        assert!(a.ditto_revealed && a.displays_shiny() && a.ditto_disguise == Some(1));
        assert_eq!(s.threshold(), Some(3_000_000_000));
        assert!(s.rt.outbox.last().unwrap().0.contains("Ditto"));
    }

    #[test]
    fn release_keeps_reached_forms_only() {
        let mut rng = Rng::with_seed(5);
        let mut s = ready_state();
        s.egg_usage = EGG_HATCH_THRESHOLD;
        s.hatch(bulbasaur(), &mut rng);
        s.feed(125_000_000, &mut rng);
        assert!(s.release_for_egg(Some(Rarity::Uncommon)));
        let d = &s.dex[0];
        assert_eq!((d.chain.clone(), d.final_id), (vec![1, 2], 2));
        assert!(d.released_at.is_some());
        assert!(s.collected_finals.is_empty());
        assert_eq!((s.egg_tier, s.egg_usage), (Some(Rarity::Uncommon), 0));
        assert!(!s.release_for_egg(None)); // needs an active Pokémon
    }

    #[test]
    fn mint_always_changes_nature() {
        let mut rng = Rng::with_seed(6);
        let mut s = ready_state();
        assert!(!s.reroll_nature(&mut rng));
        s.egg_usage = EGG_HATCH_THRESHOLD;
        s.hatch(bulbasaur(), &mut rng);
        for _ in 0..50 {
            let before = s.active.as_ref().unwrap().nature.clone();
            assert!(s.reroll_nature(&mut rng));
            assert_ne!(s.active.as_ref().unwrap().nature, before);
        }
    }

    #[test]
    fn difficulty_rescales_without_completing() {
        let mut s = ready_state();
        s.egg_usage = 4_999_999;
        s.set_difficulty(2.0);
        assert_eq!((s.egg_threshold(), s.egg_usage), (10_000_000, 9_999_998));
        s.set_difficulty(0.0); // clamps to 0.1
        assert_eq!(s.difficulty(), 0.1);
        assert!(s.egg_usage < s.egg_threshold());
    }

    #[test]
    fn difficulty_migration_from_v02() {
        let mut s = State { egg_usage: 1_000, growth_difficulty: 0.0, ..Default::default() };
        s.migrate_difficulty();
        assert_eq!((s.growth_difficulty, s.egg_usage), (1.0, 1_000));
        let mut s = State { egg_usage: 1_000_000, growth_difficulty: 4.0, ..Default::default() };
        s.migrate_difficulty();
        assert_eq!((s.growth_difficulty, s.egg_usage), (2.0, 500_000));
    }

    #[test]
    fn plans_prefer_uncollected_and_repair() {
        let mut rng = Rng::with_seed(1);
        let mut s = State::default();
        s.collected_finals.insert("133:134".into());
        let tree = node(133, vec![node(134, vec![]), node(135, vec![])]);
        for _ in 0..20 {
            assert_eq!(s.make_plan(&tree, 133, &mut rng), vec![133, 135]);
        }
        assert_eq!(repaired_plan(&[1, 2], 1, &[2, 3]), vec![1, 2, 3]);
        assert_eq!(repaired_plan(&[1, 2], 1, &[5, 6]), vec![1, 2]);
        assert_eq!(repaired_plan(&[], 0, &[5, 6]), vec![5, 6]);
    }

    #[test]
    fn normalization_and_sanitize_repair_saves() {
        let mut rng = Rng::with_seed(1);
        let mut s = ready_state();
        s.active = Some(Mon { line: bulbasaur(), path_ids: vec![1, 2, 2, 99], planned_ids: vec![1, 9], stage_index: 7, ..Default::default() });
        s.egg_tier = Some(Rarity::Rare);
        s.sanitize();
        s.normalize_evolution(&mut rng);
        let a = s.active.as_ref().unwrap();
        assert_eq!((a.path_ids.clone(), a.planned_ids.clone(), a.stage_index, a.total_forms), (vec![1, 2], vec![1, 2, 3], 1, 3));
        assert_eq!(s.egg_tier, None);
    }

    #[test]
    fn evolution_line_hides_branch_choices() {
        let mut s = ready_state();
        let tree = node(133, vec![node(134, vec![]), node(135, vec![])]);
        s.active = Some(Mon { line: line(133, tree, Rarity::Uncommon), path_ids: vec![133], planned_ids: vec![133, 135], total_forms: 2, ..Default::default() });
        assert_eq!(s.line_nodes(), vec![(Some(133), "current"), (None, "future")]);
        s.active.as_mut().unwrap().line = bulbasaur();
        s.active.as_mut().unwrap().path_ids = vec![1];
        assert_eq!(s.line_nodes(), vec![(Some(1), "current"), (Some(2), "future"), (Some(3), "future")]);
    }

    #[test]
    fn display_states() {
        let mut s = ready_state();
        let now = Instant::now();
        assert_eq!(s.display_state(now), DisplayState::Egg);
        s.active = Some(Mon { line: bulbasaur(), path_ids: vec![1], ..Default::default() });
        assert_eq!(s.display_state(now), DisplayState::Sleep);
        s.rt.inputs = Inputs { has_usage_data: true, today_tokens: 5, tokens_per_minute: 50_000.0, limit_warning: false, idle_secs: Some(10) };
        assert_eq!(s.display_state(now), DisplayState::Working);
        s.rt.inputs.tokens_per_minute = 500_000.0;
        assert_eq!(s.display_state(now), DisplayState::Focus);
        s.rt.inputs.idle_secs = Some(3 * 3600);
        assert_eq!(s.display_state(now), DisplayState::Sulk);
        s.rt.inputs.limit_warning = true;
        assert_eq!(s.display_state(now), DisplayState::Tired);
        s.rt.event_until = Some(now + Duration::from_secs(4));
        assert_eq!(s.display_state(now), DisplayState::LevelUp);
        s.expire_events(now + Duration::from_secs(5));
        assert_eq!(s.display_state(now + Duration::from_secs(5)), DisplayState::Tired);
    }

    #[test]
    fn effective_idle_caps_by_observed_rise() {
        let mut s = State::default();
        let t0 = Instant::now();
        assert_eq!(s.effective_idle(Some(9000), "d", 10, true, t0), Some(9000));
        assert_eq!(s.effective_idle(Some(9000), "d", 20, true, t0 + Duration::from_secs(60)), Some(0));
        assert_eq!(s.effective_idle(None, "d", 20, true, t0), None);
    }

    #[test]
    fn ownership_is_form_aware_and_hides_disguised_shiny() {
        let mut s = ready_state();
        s.active = Some(Mon {
            line: line(201, node(201, vec![]), Rarity::Uncommon),
            path_ids: vec![201],
            form: Some("b".into()),
            is_shiny: true,
            ditto_disguise: Some(201),
            ..Default::default()
        });
        assert!(s.owns_species(201, Some("b")) && !s.owns_species(201, Some("a")));
        assert!(!s.owns_shiny_species(201, Some("b")));
        assert_eq!(s.collected_line_forms(201), vec!["b".to_string()]);
        assert_eq!(s.form_name(201, Some("b")), "#201 [B]");
    }

    #[test]
    fn profile_migration_is_deterministic() {
        let mut s = ready_state();
        s.last_date = "2026-01-01".into();
        s.active = Some(Mon { line: bulbasaur(), path_ids: vec![1, 2], stage_index: 1, total_forms: 3, ..Default::default() });
        s.dex.push(DexEntry { id: "X".into(), final_id: 3, chain: vec![1, 2, 3], rarity: Rarity::Common, ..Default::default() });
        assert!(s.migrate_profiles());
        let seed = s.active.as_ref().unwrap().profile.as_ref().unwrap().seed;
        assert_eq!(seed, fnv1a("active:1:1,2:2026-01-01"));
        let p = s.dex[0].profile.as_ref().unwrap();
        assert_eq!((p.seed, p.level, p.instance_id.as_str()), (fnv1a("dex:X:3"), 100, "X"));
        assert_eq!(s.active.as_ref().unwrap().profile.as_ref().unwrap().level, 5 + 15); // 125M of 750M
        assert!(!s.migrate_profiles());
    }

    #[test]
    fn lenient_decoding_keeps_good_fields() {
        let json = r#"{"egg_usage": 42, "active": {"path_ids": "bad"}, "dex": [{"final_id": 3, "chain": [1,3]}, {"chain": "x"}], "egg_tier": "mythic"}"#;
        let s = super::super::decode_state(json.as_bytes()).unwrap();
        assert_eq!(s.egg_usage, 42);
        assert!(s.active.is_none());
        assert_eq!(s.dex.len(), 1);
        assert_eq!(s.egg_tier, None);
    }
}
