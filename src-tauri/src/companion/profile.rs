//! Individual profile: IVs, gender, ability, moves, level (companion.md §3.9, Profile:92-296).
//! Stats are computed in the frontend (`src/areas/companion/profile.ts`) from these values.

use super::balance::{MAX_TOKEN_VALUE, Rarity};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const LEARNSET_VERSION_GROUP: &str = "black-2-white-2";

/// PokéAPI battle metadata of a default form, cached in `pokemon-details-v1/<id>.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Details {
    pub species_id: u32,
    pub name: String,
    pub height: u32,
    pub weight: u32,
    pub base_experience: Option<u32>,
    /// Female eighths; −1 = genderless.
    pub gender_rate: i32,
    pub types: Vec<String>,
    pub base_stats: BTreeMap<String, u32>,
    pub abilities: Vec<AbilityOption>,
    pub moves: Vec<MoveOption>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AbilityOption {
    pub name: String,
    pub slot: u32,
    pub is_hidden: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MoveOption {
    pub name: String,
    pub learn_methods: Vec<LearnMethod>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LearnMethod {
    pub method: String,
    pub level: u32,
}

impl Details {
    /// Level-up moves known by `level`, each at its lowest learn level, sorted by (level, name).
    pub fn level_up_moves(&self, level: u32) -> Vec<KnownMove> {
        let mut out: Vec<KnownMove> = self
            .moves
            .iter()
            .filter_map(|m| {
                let at = m.learn_methods.iter().filter(|l| l.method == "level-up" && l.level <= level).map(|l| l.level).min()?;
                Some(KnownMove { name: m.name.clone(), learned_at_level: at })
            })
            .collect();
        out.sort_by(|a, b| (a.learned_at_level, &a.name).cmp(&(b.learned_at_level, &b.name)));
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Gender {
    Male,
    Female,
    Genderless,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ivs {
    pub hp: u32,
    pub attack: u32,
    pub defense: u32,
    pub special_attack: u32,
    pub special_defense: u32,
    pub speed: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KnownMove {
    pub name: String,
    pub learned_at_level: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    pub instance_id: String,
    /// Serialized as a string: u64 seeds exceed JavaScript's 2^53.
    #[serde(with = "u64_string")]
    pub seed: u64,
    pub gender: Option<Gender>,
    pub ivs: Ivs,
    pub ability_slot: Option<u32>,
    pub ability_name: Option<String>,
    pub ability_is_hidden: bool,
    pub level: u32,
    /// Earned growth in standard-balance units (no difficulty, no boost).
    pub growth_tokens: u64,
    pub moves: Vec<KnownMove>,
}

impl Default for Profile {
    fn default() -> Self {
        Profile::generate(0, 0, String::new())
    }
}

mod u64_string {
    use serde::{Deserialize, Deserializer, Serializer, de::Error};
    pub fn serialize<S: Serializer>(v: &u64, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&v.to_string())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
        match serde_json::Value::deserialize(d)? {
            serde_json::Value::String(s) => s.parse().map_err(D::Error::custom),
            serde_json::Value::Number(n) => n.as_u64().ok_or_else(|| D::Error::custom("bad seed")),
            _ => Err(D::Error::custom("bad seed")),
        }
    }
}

/// SplitMix64, `ProfileRNG`.
pub struct ProfileRng(u64);

impl ProfileRng {
    pub fn new(seed: u64) -> Self {
        ProfileRng(if seed == 0 { 0x9E37_79B9_7F4A_7C15 } else { seed })
    }
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// FNV-1a 64 over UTF-8, the profile migration seed.
pub fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x0000_0100_0000_01B3))
}

/// Random RFC 4122 v4 UUID (uppercase, like Foundation's `UUID().uuidString`).
pub fn uuid() -> String {
    let mut b = [0u8; 16];
    b.iter_mut().for_each(|x| *x = fastrand::u8(..));
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02X}")).collect();
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

impl Profile {
    pub fn generate(seed: u64, growth_tokens: u64, instance_id: String) -> Profile {
        let mut r = ProfileRng::new(seed);
        let mut iv = || (r.next() % 32) as u32;
        let ivs = Ivs { hp: iv(), attack: iv(), defense: iv(), special_attack: iv(), special_defense: iv(), speed: iv() };
        Profile {
            instance_id,
            seed,
            gender: None,
            ivs,
            ability_slot: None,
            ability_name: None,
            ability_is_hidden: false,
            level: 5,
            growth_tokens,
            moves: vec![],
        }
    }

    /// Ditto's reveal: keep the individual values, re-roll species-dependent ones later.
    pub fn rebase_for_species_identity(&mut self, old: Rarity, new: Rarity) {
        let fraction = (self.growth_tokens as f64 / old.graduation_total() as f64).min(1.0);
        let rebased = (fraction * new.graduation_total() as f64).floor() as u64;
        self.gender = None;
        self.ability_slot = None;
        self.ability_name = None;
        self.ability_is_hidden = false;
        self.moves.clear();
        self.growth_tokens = rebased;
        self.advance_growth(rebased, new);
    }

    pub fn enrich(&mut self, d: &Details) {
        let mut r = ProfileRng::new(self.seed ^ 0xA11B_1E5D_9EED);
        if self.gender.is_none() {
            self.gender = Some(if d.gender_rate < 0 {
                Gender::Genderless
            } else if ((r.next() % 8) as i32) < d.gender_rate {
                Gender::Female
            } else {
                Gender::Male
            });
        }
        let mut normal: Vec<&AbilityOption> = d.abilities.iter().filter(|a| !a.is_hidden).collect();
        let mut hidden: Vec<&AbilityOption> = d.abilities.iter().filter(|a| a.is_hidden).collect();
        normal.sort_by_key(|a| a.slot);
        hidden.sort_by_key(|a| a.slot);
        if self.ability_slot.is_none() {
            if !hidden.is_empty() && r.next() % 128 == 0 {
                self.ability_slot = Some(hidden[(r.next() % hidden.len() as u64) as usize].slot);
                self.ability_is_hidden = true;
            } else if !normal.is_empty() {
                self.ability_slot = Some(normal[(r.next() % normal.len() as u64) as usize].slot);
                self.ability_is_hidden = false;
            } else if let Some(f) = d.abilities.iter().min_by_key(|a| a.slot) {
                self.ability_slot = Some(f.slot);
                self.ability_is_hidden = f.is_hidden;
            }
        }
        let chosen = d
            .abilities
            .iter()
            .find(|a| Some(a.slot) == self.ability_slot && a.is_hidden == self.ability_is_hidden)
            .or_else(|| d.abilities.iter().find(|a| Some(a.slot) == self.ability_slot))
            .or(normal.first().copied())
            .or(hidden.first().copied());
        self.ability_name = chosen.map(|a| a.name.clone());
        if let Some(a) = chosen {
            self.ability_slot = Some(a.slot);
            self.ability_is_hidden = a.is_hidden;
        }
        let moves = d.level_up_moves(self.level);
        self.moves = moves[moves.len().saturating_sub(4)..].to_vec();
    }

    /// High-water mark: difficulty changes and imports never lower the level.
    pub fn advance_growth(&mut self, candidate: u64, rarity: Rarity) {
        self.growth_tokens = self.growth_tokens.max(candidate).min(MAX_TOKEN_VALUE);
        let total = rarity.graduation_total().max(1);
        let progress = (self.growth_tokens as f64 / total as f64).min(1.0);
        self.level = self.level.max(5 + (progress * 95.0).floor() as u32).min(100);
    }

    pub fn sanitize(&mut self) {
        self.level = self.level.clamp(5, 100);
        self.growth_tokens = self.growth_tokens.min(MAX_TOKEN_VALUE);
        let iv = &mut self.ivs;
        for v in [&mut iv.hp, &mut iv.attack, &mut iv.defense, &mut iv.special_attack, &mut iv.special_defense, &mut iv.speed] {
            *v = (*v).min(31);
        }
        self.moves.truncate(4);
        for m in &mut self.moves {
            m.name = m.name.chars().take(80).collect();
            m.learned_at_level = m.learned_at_level.min(100);
        }
        if self.instance_id.is_empty() {
            self.instance_id = uuid();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn details() -> Details {
        Details {
            species_id: 25,
            name: "pikachu".into(),
            height: 4,
            weight: 60,
            base_experience: Some(112),
            gender_rate: 4,
            types: vec!["electric".into()],
            base_stats: BTreeMap::new(),
            abilities: vec![
                AbilityOption { name: "static".into(), slot: 1, is_hidden: false },
                AbilityOption { name: "lightning-rod".into(), slot: 3, is_hidden: true },
            ],
            moves: ["growl:1", "thunder-shock:1", "tail-whip:5", "quick-attack:10", "thunderbolt:26", "agility:21", "spark:13"]
                .iter()
                .map(|s| {
                    let (n, l) = s.split_once(':').unwrap();
                    MoveOption { name: n.into(), learn_methods: vec![LearnMethod { method: "level-up".into(), level: l.parse().unwrap() }] }
                })
                .collect(),
        }
    }

    #[test]
    fn splitmix_and_fnv_are_stable() {
        // Seed 0 starts at the golden ratio; SplitMix64's first output from state 0 is the reference.
        let mut r = ProfileRng(0);
        assert_eq!(r.next(), 0xE220_A839_7B1D_CDAF);
        assert_eq!(ProfileRng::new(0).next(), ProfileRng(0x9E37_79B9_7F4A_7C15).next());
        assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn generate_draws_ivs_in_order() {
        let p = Profile::generate(42, 0, "x".into());
        let mut r = ProfileRng::new(42);
        assert_eq!(p.ivs.hp, (r.next() % 32) as u32);
        assert_eq!(p.ivs.attack, (r.next() % 32) as u32);
        assert_eq!((p.level, p.gender), (5, None));
    }

    #[test]
    fn enrich_picks_last_four_moves_and_ability() {
        let mut p = Profile::generate(7, 0, "x".into());
        p.level = 21;
        p.enrich(&details());
        let names: Vec<_> = p.moves.iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, vec!["tail-whip", "quick-attack", "spark", "agility"]);
        assert!(p.gender.is_some() && p.ability_name.is_some());
        let again = p.clone();
        p.enrich(&details());
        assert_eq!(p, again); // deterministic
    }

    #[test]
    fn growth_is_a_high_water_mark() {
        let mut p = Profile::generate(1, 0, "x".into());
        p.advance_growth(375_000_000, Rarity::Common);
        assert_eq!(p.level, 5 + 47);
        p.advance_growth(0, Rarity::Common);
        assert_eq!(p.level, 52);
        p.rebase_for_species_identity(Rarity::Common, Rarity::Rare);
        assert_eq!(p.growth_tokens, 1_500_000_000);
        assert_eq!(p.level, 52);
    }

    #[test]
    fn seed_round_trips_as_string() {
        let p = Profile::generate(u64::MAX, 0, "x".into());
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["seed"], "18446744073709551615");
        let back: Profile = serde_json::from_value(v).unwrap();
        assert_eq!(back.seed, u64::MAX);
    }
}
