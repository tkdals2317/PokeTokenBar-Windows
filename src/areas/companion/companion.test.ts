import assert from "node:assert/strict";
import { assetName, formDisplayName, formLabel, formNameResource, resolvedForm, spriteChain, spriteUrl, staticAssetName } from "./forms";
import { computeStats, displayScaleMaximum, type PokemonDetails, type Profile } from "./profile";
import { difficultyAtPosition, difficultyPosition, difficultyValue, snap } from "./difficulty";
import { companionStrings, natureLabel, particle } from "./strings";

// forms and sprite names
assert.equal(assetName(201, "b"), "201-b");
assert.equal(assetName(201, null), "201");
assert.equal(staticAssetName(492, "sky"), "10006");
assert.equal(staticAssetName(412, "sandy"), "412-sandy");
assert.equal(resolvedForm(414, "sandy"), null);
assert.equal(resolvedForm(413, "zz"), "plant");
assert.equal(formNameResource(201, "b"), null);
assert.equal(formNameResource(479, null), null);
assert.equal(formNameResource(479, "heat"), 10058);
assert.equal(formLabel(479, null, null, "Normal Form"), "Normal Form");
assert.equal(formLabel(492, "sky", null, "Normal Form"), "Sky");
assert.equal(formDisplayName("Unown", 201, "question", null), "Unown [?]");
assert.equal(formDisplayName("Shaymin", 492, "land", "Land Forme"), "Shaymin");
assert.equal(
  spriteUrl(25, { shiny: true, back: true }),
  "https://raw.githubusercontent.com/PokeAPI/sprites/master/sprites/pokemon/versions/generation-v/black-white/animated/back/shiny/25.gif",
);
assert.equal(spriteUrl(700), "https://raw.githubusercontent.com/PokeAPI/sprites/master/sprites/pokemon/700.png");
const chain = spriteChain(25, { shiny: true, back: true });
assert.equal(chain.length, 8);
assert.ok(chain[1].endsWith("animated/back/25.gif") && chain[2].endsWith("animated/shiny/25.gif") && chain[7].endsWith("/25.png"));

// stats: Garchomp-like bases, adamant, level 50
const details: PokemonDetails = {
  species_id: 445, name: "garchomp", height: 19, weight: 950, base_experience: 270, gender_rate: 4, types: ["dragon", "ground"],
  base_stats: { hp: 108, attack: 130, defense: 95, "special-attack": 80, "special-defense": 85, speed: 102 },
  abilities: [], moves: [],
};
const profile: Profile = {
  instance_id: "x", seed: "1", gender: null, ability_slot: null, ability_name: null, ability_is_hidden: false,
  level: 50, growth_tokens: 0, moves: [],
  ivs: { hp: 31, attack: 31, defense: 31, special_attack: 31, special_defense: 31, speed: 31 },
};
const stats = computeStats(details, profile, "adamant").map((s) => s.value);
assert.deepEqual(stats, [183, 165, 115, 90, 105, 122]);
assert.equal(computeStats({ ...details, species_id: 292 }, profile, null)[0].value, 1); // Shedinja
assert.equal(displayScaleMaximum([183, 165]), 300);
assert.equal(displayScaleMaximum([301]), 400);

// difficulty slider
assert.equal(difficultyAtPosition(difficultyPosition(1) + 0.009), 1);
assert.equal(difficultyAtPosition(0), 0.1);
assert.ok(Math.abs(difficultyAtPosition(1) - 2) < 1e-9);
assert.ok(Math.abs(snap(1.467) - 1.5) < 1e-9);
assert.equal(difficultyValue(0.1), "10%");
assert.equal(difficultyValue(1), "100%");

// strings
assert.equal(particle("리자몽", "direction"), "리자몽으로");
assert.equal(particle("피카츄", "subject"), "피카츄가");
assert.equal(particle("Unown [A]", "direction"), "Unown [A](으)로");
assert.equal(companionStrings.ko!.statusEvolved!("리자드"), "리자드로 진화했어요!");
assert.equal(natureLabel("quirky", "ru"), "Ловкий");
for (const lang of ["ko", "ja", "es", "fr", "pt", "de", "ru"] as const) {
  assert.deepEqual(Object.keys(companionStrings[lang]!).sort(), Object.keys(companionStrings.en).sort(), lang);
}
