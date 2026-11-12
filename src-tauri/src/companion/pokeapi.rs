//! PokéAPI: hatch candidates, evolution lines, battle details and localized names
//! (companion.md §4.4). Everything is cached on disk under the app data folder; nothing is bundled.

use super::profile::{AbilityOption, Details, LearnMethod, LEARNSET_VERSION_GROUP, MoveOption};
use super::state::{EvoNode, Line, Names};
use super::{DITTO_ID, MAX_SPECIES_ID, text};
use super::balance::Rarity;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const REST: &str = "https://pokeapi.co/api/v2";
const GRAPHQL: &str = "https://graphql.pokeapi.co/v1beta2";
const TTL_SECS: u64 = 30 * 86400;

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout(Duration::from_secs(15)).build()
}

fn get(url: &str) -> Result<Value, String> {
    agent().get(url).call().map_err(|e| e.to_string())?.into_json().map_err(|e| e.to_string())
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// `{fetched_at, value}` disk snapshot with a 30-day TTL. A stale one still beats nothing offline.
#[derive(Serialize, Deserialize)]
struct Snapshot<T> {
    fetched_at: u64,
    #[serde(alias = "entries", alias = "details", alias = "names")]
    value: T,
}

fn cached<T: Serialize + for<'de> Deserialize<'de> + Clone>(file: &Path, fetch: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let disk: Option<Snapshot<T>> = fs::read(file).ok().and_then(|b| serde_json::from_slice(&b).ok());
    if let Some(d) = &disk {
        if now_secs().saturating_sub(d.fetched_at) < TTL_SECS {
            return Ok(d.value.clone());
        }
    }
    match fetch() {
        Ok(v) => {
            if let Some(dir) = file.parent() {
                let _ = fs::create_dir_all(dir);
            }
            crate::store::save_json(file, &Snapshot { fetched_at: now_secs(), value: v.clone() });
            Ok(v)
        }
        Err(e) => disk.map(|d| d.value).ok_or(e),
    }
}

// ---- base index (hatch candidates) ----

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BaseEntry {
    pub id: u32,
    pub capture_rate: u32,
}

static INDEX: LazyLock<Mutex<Option<Vec<BaseEntry>>>> = LazyLock::new(|| Mutex::new(None));

/// Line starts up to #649 with their capture rates, Ditto excluded. Memory → disk → GraphQL → stale disk.
pub fn base_index(dir: &Path) -> Result<Vec<BaseEntry>, String> {
    if let Some(i) = INDEX.lock().unwrap().clone() {
        return Ok(i);
    }
    let file = dir.join("base-index.json");
    let index = cached(&file, fetch_base_index).or_else(|e| {
        // v0.2 wrote a bare `[[id, rate], …]` list.
        let legacy: Vec<(u32, u32)> = fs::read(&file).ok().and_then(|b| serde_json::from_slice(&b).ok()).ok_or(e)?;
        Ok::<_, String>(legacy.into_iter().map(|(id, capture_rate)| BaseEntry { id, capture_rate }).collect())
    })?;
    *INDEX.lock().unwrap() = Some(index.clone());
    Ok(index)
}

fn fetch_base_index() -> Result<Vec<BaseEntry>, String> {
    let query = format!(
        "{{ pokemonspecies(where: {{evolves_from_species_id: {{_is_null: true}}, id: {{_lte: {MAX_SPECIES_ID}, _neq: {DITTO_ID}}}}}, order_by: {{id: asc}}) {{ id capture_rate }} }}"
    );
    let v: Value = agent()
        .post(GRAPHQL)
        .send_json(json!({ "query": query }))
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())?;
    let index: Vec<BaseEntry> = v["data"]["pokemonspecies"]
        .as_array()
        .ok_or("bad graphql response")?
        .iter()
        .filter_map(|r| Some(BaseEntry { id: r["id"].as_u64()? as u32, capture_rate: r["capture_rate"].as_u64()? as u32 }))
        .collect();
    if index.is_empty() {
        return Err("empty base index".into());
    }
    Ok(index)
}

// ---- species and lines ----

static SPECIES: LazyLock<Mutex<HashMap<u32, Value>>> = LazyLock::new(Default::default);
static LINES: LazyLock<Mutex<HashMap<u32, Line>>> = LazyLock::new(Default::default);

fn species(id: u32) -> Result<Value, String> {
    if let Some(v) = SPECIES.lock().unwrap().get(&id) {
        return Ok(v.clone());
    }
    let v = get(&format!("{REST}/pokemon-species/{id}"))?;
    SPECIES.lock().unwrap().insert(id, v.clone());
    Ok(v)
}

/// REST fallback probe: `Some(capture_rate)` when `id` starts a line (not Ditto), `None` otherwise.
pub fn base_species(id: u32) -> Result<Option<u32>, String> {
    let sp = species(id)?;
    if id == DITTO_ID || !sp["evolves_from_species"].is_null() {
        return Ok(None);
    }
    Ok(Some(sp["capture_rate"].as_u64().unwrap_or(255) as u32))
}

/// ".../pokemon-species/25/" → 25
fn id_from_url(url: &str) -> Option<u32> {
    url.trim_end_matches('/').rsplit('/').next()?.parse().ok()
}

fn chain_node(link: &Value) -> Option<EvoNode> {
    Some(EvoNode {
        id: id_from_url(link["species"]["url"].as_str()?)?,
        children: link["evolves_to"].as_array()?.iter().filter_map(chain_node).collect(),
    })
}

/// Every language PokéAPI has, codes normalized.
fn names_of(species: &Value) -> HashMap<String, String> {
    species["names"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|n| {
            let lang = text::lang_code(n["language"]["name"].as_str()?);
            let name = n["name"].as_str()?.trim().to_string();
            (!lang.is_empty() && !name.is_empty()).then_some((lang, name))
        })
        .collect()
}

/// The chain URL must be https on host pokeapi.co exactly.
fn valid_chain_url(url: &str) -> bool {
    url.strip_prefix("https://").and_then(|r| r.split('/').next()) == Some("pokeapi.co")
}

/// The evolution line starting at `base_id`, cut at #649. Cached per session.
pub fn line(base_id: u32) -> Result<Line, String> {
    if let Some(l) = LINES.lock().unwrap().get(&base_id) {
        return Ok(l.clone());
    }
    let species_v = species(base_id)?;
    let rarity = Rarity::from(
        species_v["capture_rate"].as_u64().unwrap_or(255) as u32,
        species_v["is_legendary"].as_bool().unwrap_or(false) || species_v["is_mythical"].as_bool().unwrap_or(false),
    );
    let chain_url = species_v["evolution_chain"]["url"].as_str().ok_or("no evolution chain")?;
    if !valid_chain_url(chain_url) {
        return Err(format!("unexpected chain url {chain_url}"));
    }
    let chain = get(chain_url)?;
    let tree = chain_node(&chain["chain"])
        .and_then(|t| t.find(base_id).cloned())
        .and_then(|t| t.keep_animated())
        .unwrap_or(EvoNode { id: base_id, children: vec![] });
    let mut names: Names = HashMap::from([(base_id, names_of(&species_v))]);
    for id in tree.all_ids().into_iter().filter(|id| *id != base_id) {
        names.insert(id, names_of(&species(id)?));
    }
    let line = Line { base_id, tree, rarity, names };
    LINES.lock().unwrap().insert(base_id, line.clone());
    Ok(line)
}

// ---- battle details ----

/// Default-form details. Memory (caller) → 30-day disk cache → REST → stale disk.
pub fn details(dir: &Path, id: u32) -> Result<Details, String> {
    cached(&dir.join("pokemon-details-v1").join(format!("{id}.json")), || {
        let p = get(&format!("{REST}/pokemon/{id}"))?;
        let sp = species(id)?;
        Ok(parse_details(id, &p, sp["gender_rate"].as_i64().unwrap_or(-1) as i32))
    })
}

fn parse_details(id: u32, p: &Value, gender_rate: i32) -> Details {
    let arr = |k: &str| p[k].as_array().cloned().unwrap_or_default();
    let mut types: Vec<(u64, String)> =
        arr("types").iter().filter_map(|t| Some((t["slot"].as_u64()?, t["type"]["name"].as_str()?.to_string()))).collect();
    types.sort();
    let base_stats: BTreeMap<String, u32> =
        arr("stats").iter().filter_map(|s| Some((s["stat"]["name"].as_str()?.to_string(), s["base_stat"].as_u64()? as u32))).collect();
    let mut abilities: Vec<AbilityOption> = arr("abilities")
        .iter()
        .filter_map(|a| {
            Some(AbilityOption {
                name: a["ability"]["name"].as_str()?.to_string(),
                slot: a["slot"].as_u64()? as u32,
                is_hidden: a["is_hidden"].as_bool().unwrap_or(false),
            })
        })
        .collect();
    abilities.sort_by_key(|a| a.slot);
    let mut moves: Vec<MoveOption> = arr("moves")
        .iter()
        .filter_map(|m| {
            let learn_methods: Vec<LearnMethod> = m["version_group_details"]
                .as_array()?
                .iter()
                .filter(|r| r["version_group"]["name"] == LEARNSET_VERSION_GROUP)
                .filter_map(|r| {
                    Some(LearnMethod {
                        method: r["move_learn_method"]["name"].as_str()?.to_string(),
                        level: r["level_learned_at"].as_u64()? as u32,
                    })
                })
                .collect();
            (!learn_methods.is_empty()).then(|| Some(MoveOption { name: m["move"]["name"].as_str()?.to_string(), learn_methods }))?
        })
        .collect();
    moves.sort_by(|a, b| a.name.cmp(&b.name));
    Details {
        species_id: id,
        name: p["name"].as_str().unwrap_or_default().to_string(),
        height: p["height"].as_u64().unwrap_or(0) as u32,
        weight: p["weight"].as_u64().unwrap_or(0) as u32,
        base_experience: p["base_experience"].as_u64().map(|v| v as u32),
        gender_rate,
        types: types.into_iter().map(|(_, t)| t).collect(),
        base_stats,
        abilities,
        moves,
    }
}

// ---- localized resource names (type, ability, move, pokemon-form) ----

pub const NAME_KINDS: [&str; 5] = ["type", "ability", "move", "pokemon-form", "pokemon-species"];

pub fn valid_resource(kind: &str, name: &str) -> bool {
    NAME_KINDS.contains(&kind) && !name.is_empty() && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// `{lang: name}` for one resource. 30-day disk cache in `pokemon-names-v1/<kind>-<name>.json`.
// ponytail: concurrent first requests for one resource may fetch twice; a per-key lock if that shows up.
pub fn names(dir: &Path, kind: &str, name: &str) -> Result<HashMap<String, String>, String> {
    if !valid_resource(kind, name) {
        return Err(format!("bad resource {kind}/{name}"));
    }
    cached(&dir.join("pokemon-names-v1").join(format!("{kind}-{name}.json")), || {
        let v = get(&format!("{REST}/{kind}/{name}/"))?;
        let key = if kind == "pokemon-form" { "form_names" } else { "names" };
        Ok(names_of(&json!({ "names": v[key] })))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_chain_and_names() {
        let chain = json!({ "species": { "url": "https://pokeapi.co/api/v2/pokemon-species/133/" },
            "evolves_to": [
                { "species": { "url": "https://pokeapi.co/api/v2/pokemon-species/134/" }, "evolves_to": [] },
                { "species": { "url": "https://pokeapi.co/api/v2/pokemon-species/700/" }, "evolves_to": [] }
            ] });
        let tree = chain_node(&chain).unwrap().keep_animated().unwrap();
        assert_eq!(tree.all_ids(), vec![133, 134]);

        let sp = json!({ "names": [
            { "language": { "name": "ko" }, "name": "이브이" },
            { "language": { "name": "ja-Hrkt" }, "name": "イーブイ" },
            { "language": { "name": "fr" }, "name": " Évoli " } ] });
        let n = names_of(&sp);
        assert_eq!(n.len(), 3);
        assert_eq!(n["ja-hrkt"], "イーブイ");
        assert_eq!(n["fr"], "Évoli");
    }

    #[test]
    fn validates_urls_and_resources() {
        assert!(valid_chain_url("https://pokeapi.co/api/v2/evolution-chain/1/"));
        assert!(!valid_chain_url("http://pokeapi.co/api/v2/evolution-chain/1/"));
        assert!(!valid_chain_url("https://pokeapi.co.evil.com/x"));
        assert!(valid_resource("pokemon-form", "10064"));
        assert!(!valid_resource("type", "../etc"));
        assert!(!valid_resource("berry", "x"));
    }

    #[test]
    fn parses_details_keeping_bw2_learnset() {
        let p = json!({
            "name": "pikachu", "height": 4, "weight": 60, "base_experience": 112,
            "types": [{ "slot": 1, "type": { "name": "electric" } }],
            "stats": [{ "base_stat": 35, "stat": { "name": "hp" } }],
            "abilities": [
                { "ability": { "name": "lightning-rod" }, "slot": 3, "is_hidden": true },
                { "ability": { "name": "static" }, "slot": 1, "is_hidden": false }],
            "moves": [
                { "move": { "name": "thunderbolt" }, "version_group_details": [
                    { "level_learned_at": 26, "move_learn_method": { "name": "level-up" }, "version_group": { "name": "black-2-white-2" } },
                    { "level_learned_at": 29, "move_learn_method": { "name": "level-up" }, "version_group": { "name": "x-y" } }] },
                { "move": { "name": "surf" }, "version_group_details": [
                    { "level_learned_at": 0, "move_learn_method": { "name": "tutor" }, "version_group": { "name": "x-y" } }] }]
        });
        let d = parse_details(25, &p, 4);
        assert_eq!(d.types, vec!["electric"]);
        assert_eq!(d.base_stats["hp"], 35);
        assert_eq!(d.abilities[0].name, "static");
        assert_eq!(d.moves.len(), 1);
        assert_eq!(d.moves[0].learn_methods, vec![LearnMethod { method: "level-up".into(), level: 26 }]);
    }

    /// `cargo test live_line -- --ignored` hits the real API.
    #[test]
    #[ignore]
    fn live_line() {
        let l = line(133).unwrap();
        println!("{:?} {:?} {:?}", l.rarity, l.tree.all_ids(), l.names.get(&133));
        println!("{} bases", fetch_base_index().unwrap().len());
    }
}
