import assert from "node:assert/strict";
import type { Companion, DexEntry } from "../../shared";
import {
  dexSpecies, earnedBadges, entryName, filterLog, filterSpecies, logEntries, matcher, pageContaining, pageCount,
  regionCompletions, relativeAge, unownFormsCollected, WheelPager,
} from "./dex";
import { planCandy } from "./economy";

const entry = (p: Partial<DexEntry> & { chain: number[] }): DexEntry => ({
  id: p.id ?? `e${p.chain.join("-")}`, base_id: p.chain[0], final_id: p.chain[p.chain.length - 1], rarity: "common",
  is_shiny: false, nature: "bold", names: {}, caught_at: "2026-01-01T00:00:00Z", form: null, profile: null, prayers: null,
  names_version: 1, released_at: null, ...p,
});

const c = {
  dex: [
    entry({ id: "a", chain: [25, 26], names: { 25: { en: "Pikachu" }, 26: { en: "Raichu" } }, caught_at: "2026-01-02T00:00:00Z" }),
    entry({ id: "b", chain: [669], rarity: "uncommon", is_shiny: true, names: { 669: { en: "Flabébé" } }, caught_at: "2026-01-03T00:00:00Z" }),
    entry({ id: "c", chain: [125], rarity: "rare", names: { 125: { en: "Electabuzz" } } }),
    entry({ id: "d", chain: [201], form: "b", rarity: "uncommon" }),
  ],
  active: {
    line: { base_id: 250, tree: { id: 250, children: [] }, rarity: "legendary", names: { 250: { en: "Ho-Oh" } } },
    path_ids: [250], planned_ids: [250], stage_index: 0, used_at_stage: 0, total_forms: 1, is_shiny: false,
    nature: "calm", form: null, profile: null, has_growth_boost: false, ditto_disguise: null, ditto_revealed: false, prayers: null,
  },
  current_id: 250,
  displays_shiny: false,
} as unknown as Companion;

// species model
const all = dexSpecies(c, "en");
assert.deepEqual(all.map((s) => s.id), [25, 26, 125, 201, 250, 669]);
assert.equal(all.find((s) => s.id === 250)!.isRaising, true);
assert.equal(all.find((s) => s.id === 669)!.isShiny, true);
assert.equal(dexSpecies(c, "en", true).find((s) => s.id === 201)!.collectionID, "201-b");

// search: exact number, "#id" substring, diacritics
const m = matcher("25")!;
assert.ok(m([25], []) && m([125], []) && m([250], []));
const h = matcher("#25")!;
assert.ok(h([25], []) && h([250], []) && !h([125], []));
assert.ok(matcher("flabebe")!([669], ["Flabébé"]));
assert.equal(matcher("   "), null);
const base = { query: "", rarity: null, shinyOnly: false };
assert.deepEqual(filterSpecies(all, { ...base, query: "#25", region: null, sort: "numberAsc" }, "en").map((s) => s.id), [25, 250]);

// filters and sorts
assert.deepEqual(filterSpecies(all, { ...base, region: "kanto", sort: "numberDesc" }, "en").map((s) => s.id), [125, 26, 25]);
assert.deepEqual(filterSpecies(all, { ...base, region: null, sort: "rarityDesc" }, "en").map((s) => s.id), [250, 125, 201, 669, 25, 26]);
assert.deepEqual(filterSpecies(all, { ...base, shinyOnly: true, region: null, sort: "nameAsc" }, "en").map((s) => s.id), [669]);

const log = logEntries(c);
assert.equal(log.length, 5);
assert.equal(log[4].id, "active-250-250");
assert.deepEqual(filterLog(log, { ...base, sort: "recentFirst" }, "en").map((e) => e.id), ["active-250-250", "b", "a", "c", "d"]);
assert.deepEqual(filterLog(log, { ...base, sort: "oldestFirst" }, "en").map((e) => e.id), ["c", "d", "a", "b", "active-250-250"]);
assert.deepEqual(filterLog(log, { ...base, sort: "numberAsc" }, "en").map((e) => e.final_id), [26, 125, 201, 250, 669]);
assert.deepEqual(filterLog(log, { ...base, query: "pikachu", sort: "recentFirst" }, "en").map((e) => e.id), ["a"]); // chain names
assert.equal(entryName(log[3], "en"), "#201");

// regions and badges
const kanto = Array.from({ length: 151 }, (_, i) => ({ id: i + 1, isShiny: i < 10 }));
assert.deepEqual(regionCompletions(kanto)[0], { region: "kanto", owned: 151, shinyOwned: 10, total: 151 });
assert.deepEqual(earnedBadges(kanto, 3), ["region.kanto"]);
assert.deepEqual(earnedBadges([], 28), ["unown"]);
assert.equal(unownFormsCollected(c), 1);

// paging
assert.equal(pageCount(0), 1);
assert.equal(pageCount(17), 2);
assert.equal(pageContaining("20", Array.from({ length: 30 }, (_, i) => String(i))), 1);
assert.equal(pageContaining("x", ["a"]), 0);
const p = new WheelPager();
assert.equal(p.step(3, 1, 0), 1); // one line-mode notch down = next page
assert.equal(p.step(-120, 0, 10), -1);
assert.equal(p.step(10, 0, 1000), 0);
assert.equal(p.step(15, 0, 1010), 1); // 25 px accumulated
assert.equal(p.step(40, 0, 1020), 0); // once per gesture
assert.equal(p.step(30, 0, 1300), 1); // pause = new gesture

// Rare Candy preview (2-form common line: 250M, 500M)
const e = { candy_costs: [250e6, 500e6], max_candy_use: 8, candy_xp: 100e6 };
assert.deepEqual(planCandy(1, e, 10e6), { count: 1, evolves: false, graduates: false, carryoverXP: 0, discardedXP: 0 });
assert.deepEqual(planCandy(3, e, 10e6), { count: 3, evolves: true, graduates: false, carryoverXP: 60e6, discardedXP: 0 });
assert.deepEqual(planCandy(99, e, 10e6), { count: 8, evolves: true, graduates: true, carryoverXP: 0, discardedXP: 60e6 });
assert.equal(planCandy(1, { ...e, max_candy_use: 0 }, 0), null);

// relative age: two largest units, no "ago"
assert.equal(relativeAge(0, (2 * 365 + 300) * 86400e3, "en"), "2 yrs, 10 mths");
