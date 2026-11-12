//! PokéAPI: hatch candidates (GraphQL, cached on disk) and evolution lines (REST).

use super::{DITTO_ID, EvoNode, Line, MAX_SPECIES_ID, Names, Rarity};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

const REST: &str = "https://pokeapi.co/api/v2";
const GRAPHQL: &str = "https://graphql.pokeapi.co/v1beta2";
const INDEX_TTL: Duration = Duration::from_secs(30 * 86400);
/// Name languages kept per species. ja-hrkt is kana, ja is kanji.
const LANGS: [&str; 4] = ["ko", "en", "ja-hrkt", "ja"];

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(15))
        .build()
}

fn get(url: &str) -> Result<Value, String> {
    agent()
        .get(url)
        .call()
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())
}

/// `(species id, capture rate)` for every line start up to #649, Ditto excluded.
/// Disk cache first; a stale cache still beats nothing when offline.
pub fn base_index(cache_dir: &Path) -> Result<Vec<(u32, u32)>, String> {
    let file = cache_dir.join("base-index.json");
    let cached: Option<Vec<(u32, u32)>> = fs::read(&file)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok());
    let fresh = fs::metadata(&file)
        .and_then(|m| m.modified())
        .is_ok_and(|m| SystemTime::now().duration_since(m).unwrap_or_default() < INDEX_TTL);
    if let (Some(c), true) = (&cached, fresh) {
        return Ok(c.clone());
    }
    match fetch_base_index() {
        Ok(index) => {
            let _ = fs::write(&file, serde_json::to_vec(&index).unwrap_or_default());
            Ok(index)
        }
        Err(e) => cached.ok_or(e),
    }
}

fn fetch_base_index() -> Result<Vec<(u32, u32)>, String> {
    let query = format!(
        "{{ pokemonspecies(where: {{evolves_from_species_id: {{_is_null: true}}, id: {{_lte: {MAX_SPECIES_ID}, _neq: {DITTO_ID}}}}}, order_by: {{id: asc}}) {{ id capture_rate }} }}"
    );
    let v: Value = agent()
        .post(GRAPHQL)
        .send_json(json!({ "query": query }))
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())?;
    let index: Vec<(u32, u32)> = v["data"]["pokemonspecies"]
        .as_array()
        .ok_or("bad graphql response")?
        .iter()
        .filter_map(|r| Some((r["id"].as_u64()? as u32, r["capture_rate"].as_u64()? as u32)))
        .collect();
    if index.is_empty() {
        return Err("empty base index".into());
    }
    Ok(index)
}

/// ".../pokemon-species/25/" → 25
fn id_from_url(url: &str) -> Option<u32> {
    url.trim_end_matches('/').rsplit('/').next()?.parse().ok()
}

fn chain_node(link: &Value) -> Option<EvoNode> {
    Some(EvoNode {
        id: id_from_url(link["species"]["url"].as_str()?)?,
        children: link["evolves_to"]
            .as_array()?
            .iter()
            .filter_map(chain_node)
            .collect(),
    })
}

fn names_of(species: &Value) -> HashMap<String, String> {
    species["names"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|n| {
            let lang = n["language"]["name"].as_str()?.to_lowercase();
            let name = n["name"].as_str()?.to_string();
            LANGS.contains(&lang.as_str()).then_some((lang, name))
        })
        .collect()
}

pub fn line(base_id: u32) -> Result<Line, String> {
    let species = get(&format!("{REST}/pokemon-species/{base_id}"))?;
    let rarity = Rarity::from(
        species["capture_rate"].as_u64().unwrap_or(255) as u32,
        species["is_legendary"].as_bool().unwrap_or(false)
            || species["is_mythical"].as_bool().unwrap_or(false),
    );
    let chain_url = species["evolution_chain"]["url"]
        .as_str()
        .ok_or("no evolution chain")?;
    if !chain_url.starts_with(REST) {
        return Err(format!("unexpected chain url {chain_url}"));
    }
    let chain = get(chain_url)?;
    let tree = chain_node(&chain["chain"])
        .and_then(|t| t.find(base_id).cloned())
        .and_then(|t| t.keep_animated())
        .ok_or("bad evolution chain")?;
    let mut names: Names = HashMap::from([(base_id, names_of(&species))]);
    for id in tree.all_ids().into_iter().filter(|id| *id != base_id) {
        names.insert(id, names_of(&get(&format!("{REST}/pokemon-species/{id}"))?));
    }
    Ok(Line {
        base_id,
        tree,
        rarity,
        names,
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
            { "language": { "name": "fr" }, "name": "Évoli" } ] });
        assert_eq!(
            names_of(&sp),
            HashMap::from([("ko".into(), "이브이".into())])
        );
    }

    /// `cargo test live_line -- --ignored` hits the real API.
    #[test]
    #[ignore]
    fn live_line() {
        let l = line(133).unwrap();
        println!(
            "{:?} {:?} {:?}",
            l.rarity,
            l.tree.all_ids(),
            l.names.get(&133)
        );
        let idx = fetch_base_index().unwrap();
        println!("{} bases", idx.len());
    }
}
