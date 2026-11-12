// Pokédex model: species, catch log, search, filters, sorts, regions, badges, paging
// (shop-bag-dex.md §3.7-3.11, CompanionStore.swift:452-800, PokedexRegion.swift). Owner: shop-bag-dex area.
import type { Lang } from "../../kit/i18n";
import type { Companion, DexEntry, Names, Rarity } from "../../shared";
import { resolveName } from "../../shared";
import { formDisplayName, forms, resolvedForm } from "../companion/forms";

export const PAGE_SIZE = 16; // 4 × 4
export const RARITY_DISPLAY_ORDER: Rarity[] = ["legendary", "rare", "uncommon", "common"];
export const SORT_RANK: Record<Rarity, number> = { common: 0, uncommon: 1, rare: 2, legendary: 3 };

export type DexSpecies = {
  id: number;
  form: string | null;
  /** `"<id>"` or `"<id>-<form>"`. */
  collectionID: string;
  name: string;
  names: Record<string, string> | null;
  rarity: Rarity;
  isShiny: boolean;
  hasNormal: boolean;
  isRaising: boolean;
};

export const collectionID = (id: number, form: string | null) => (form ? `${id}-${form}` : String(id));

/**
 * Owned species: every dex chain plus the active Pokémon's reached stages (never the plan).
 * `groupForms` splits species by form (detail page); the grid keeps one cell per species.
 * `label(id, form)` gives the form label for grouped names.
 */
export function dexSpecies(
  c: Companion,
  lang: Lang,
  groupForms = false,
  label: (id: number, form: string | null) => string | null = () => null,
): DexSpecies[] {
  type Acc = { id: number; form: string | null; rarity: Rarity; names: Record<string, string> | null; isShiny: boolean; hasNormal: boolean };
  const acc = new Map<string, Acc>();
  const add = (id: number, form: string | null, rarity: Rarity, names: Names, shiny: boolean) => {
    const f = groupForms ? resolvedForm(id, form) : null;
    const key = collectionID(id, f);
    const a = acc.get(key) ?? { id, form: f, rarity, names: null, isShiny: false, hasNormal: false };
    if (names[id]) a.names = names[id];
    if (shiny) a.isShiny = true;
    else a.hasNormal = true;
    acc.set(key, a);
  };
  for (const d of c.dex) for (const id of d.chain) add(id, d.form, d.rarity, d.names ?? {}, d.is_shiny);
  const a = c.active;
  if (a) for (const id of a.path_ids.slice(0, a.stage_index + 1)) add(id, a.form, a.line.rarity, a.line.names, c.displays_shiny);
  const currentForm = a && c.current_id != null ? resolvedForm(c.current_id, a.form) : null;
  const order = (x: Acc) => (x.form ? forms(x.id).indexOf(x.form) : -1);
  return [...acc.values()]
    .sort((x, y) => x.id - y.id || order(x) - order(y))
    .map((x) => {
      const plain = resolveName(x.names ?? undefined, lang) ?? `#${x.id}`;
      return {
        ...x,
        collectionID: collectionID(x.id, x.form),
        name: groupForms ? formDisplayName(plain, x.id, x.form, label(x.id, x.form)) : plain,
        isRaising: x.id === c.current_id && (!groupForms || x.form === currentForm),
      };
    });
}

/** A catch-log row: a dex record or the synthesized active individual (`caught_at` null). */
export type LogEntry = Omit<DexEntry, "caught_at"> & { caught_at: string | null; active: boolean };

export function logEntries(c: Companion): LogEntry[] {
  const out: LogEntry[] = c.dex.map((d) => ({ ...d, active: false }));
  const a = c.active;
  if (a && c.current_id != null) {
    const names: Names = {};
    for (const id of a.path_ids) if (a.line.names[id]) names[id] = a.line.names[id];
    out.push({
      id: `active-${a.line.base_id}-${c.current_id}`,
      base_id: a.line.base_id,
      final_id: c.current_id,
      chain: a.path_ids,
      rarity: a.line.rarity,
      is_shiny: c.displays_shiny,
      nature: a.nature,
      names,
      caught_at: null,
      form: a.form,
      profile: a.profile,
      prayers: a.prayers,
      names_version: 1,
      released_at: null,
      active: true,
    });
  }
  return out;
}

/** Final species name, else the last named chain species, else `#final`. */
export function entryName(e: LogEntry, lang: Lang) {
  for (const id of [e.final_id, ...[...e.chain].reverse()]) {
    const n = resolveName(e.names?.[id], lang);
    if (n) return n;
  }
  return `#${e.final_id}`;
}

// ---- search (DexSearchMatcher) ----

const fold = (s: string) => s.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase();

/** `null` for a blank query. Exact number, `#id` substring ("25" finds #125), or any stored name. */
export function matcher(raw: string) {
  const query = raw.trim();
  if (!query) return null;
  const digits = query.startsWith("#") ? query.slice(1) : query;
  const number = /^[+-]?\d+$/.test(digits) ? Number(digits) : null;
  const q = fold(query);
  return (ids: number[], names: string[]) =>
    (number != null && ids.includes(number)) ||
    ids.some((id) => `#${id}`.includes(query)) ||
    names.some((n) => fold(n).includes(q));
}

const speciesMatch = (m: NonNullable<ReturnType<typeof matcher>>, s: DexSpecies) =>
  m([s.id], [s.name, ...Object.values(s.names ?? {})]);

const entryMatch = (m: NonNullable<ReturnType<typeof matcher>>, e: LogEntry) =>
  m([e.base_id, e.final_id, ...e.chain], Object.values(e.names ?? {}).flatMap((n) => Object.values(n)));

// ---- regions ----

export const REGIONS = [
  { id: "kanto", lo: 1, hi: 151 },
  { id: "johto", lo: 152, hi: 251 },
  { id: "hoenn", lo: 252, hi: 386 },
  { id: "sinnoh", lo: 387, hi: 493 },
  { id: "unova", lo: 494, hi: 649 },
] as const;
export type Region = (typeof REGIONS)[number]["id"];
export const NATIONAL_TOTAL = 649;
export const UNOWN = 201;
export const regionOf = (r: Region) => REGIONS.find((x) => x.id === r)!;
export const inRegion = (r: Region, id: number) => id >= regionOf(r).lo && id <= regionOf(r).hi;

export type RegionCompletion = { region: Region; owned: number; shinyOwned: number; total: number };

function shinyByID(species: { id: number; isShiny: boolean }[]) {
  const m = new Map<number, boolean>();
  for (const s of species) m.set(s.id, (m.get(s.id) ?? false) || s.isShiny);
  return m;
}

export function regionCompletions(species: { id: number; isShiny: boolean }[]): RegionCompletion[] {
  const m = shinyByID(species);
  return REGIONS.map(({ id, lo, hi }) => {
    const ids = [...m.keys()].filter((x) => x >= lo && x <= hi);
    return { region: id, owned: ids.length, shinyOwned: ids.filter((x) => m.get(x)).length, total: hi - lo + 1 };
  });
}

/** Distinct Unown letters owned in the dex or the active Pokémon's reached stages. */
export function unownFormsCollected(c: Companion) {
  const set = new Set<string>();
  for (const d of c.dex) if (d.chain.includes(UNOWN)) set.add(resolvedForm(UNOWN, d.form) ?? "");
  const a = c.active;
  if (a && a.path_ids.slice(0, a.stage_index + 1).includes(UNOWN)) set.add(resolvedForm(UNOWN, a.form) ?? "");
  set.delete("");
  return set.size;
}

/** Badge keys in `earnedBadges` order (regions, unown, national, national shiny). */
export function earnedBadges(species: { id: number; isShiny: boolean }[], unownForms: number): string[] {
  const out: string[] = [];
  for (const r of regionCompletions(species)) {
    if (r.owned >= r.total) out.push(`region.${r.region}`);
    if (r.shinyOwned >= r.total) out.push(`regionShiny.${r.region}`);
  }
  if (unownForms >= forms(UNOWN).length) out.push("unown");
  const m = shinyByID(species.filter((s) => s.id >= 1 && s.id <= NATIONAL_TOTAL));
  if (m.size >= NATIONAL_TOTAL) out.push("national");
  if ([...m.values()].filter(Boolean).length >= NATIONAL_TOTAL) out.push("nationalShiny");
  return out;
}

// ---- filter + sort ----

export type DexSort = "numberAsc" | "numberDesc" | "nameAsc" | "nameDesc" | "rarityDesc";
export type LogSort = "recentFirst" | "oldestFirst" | "numberAsc" | "numberDesc" | "nameAsc" | "nameDesc" | "rarityDesc";
export const DEX_SORTS: DexSort[] = ["numberAsc", "numberDesc", "nameAsc", "nameDesc", "rarityDesc"];
export const LOG_SORTS: LogSort[] = ["recentFirst", "oldestFirst", "numberAsc", "numberDesc", "nameAsc", "nameDesc", "rarityDesc"];

export type Filters = { query: string; rarity: Rarity | null; shinyOnly: boolean };

export function filterSpecies(all: DexSpecies[], f: Filters & { region: Region | null; sort: DexSort }, lang: Lang) {
  const m = matcher(f.query);
  const out = all.filter(
    (s) =>
      (!f.rarity || s.rarity === f.rarity) &&
      (!f.region || inRegion(f.region, s.id)) &&
      (!f.shinyOnly || s.isShiny) &&
      (!m || speciesMatch(m, s)),
  );
  const coll = new Intl.Collator(lang);
  const sorters: Record<DexSort, (a: DexSpecies, b: DexSpecies) => number> = {
    numberAsc: (a, b) => a.id - b.id,
    numberDesc: (a, b) => b.id - a.id,
    nameAsc: (a, b) => coll.compare(a.name, b.name) || a.id - b.id,
    nameDesc: (a, b) => coll.compare(b.name, a.name) || a.id - b.id,
    rarityDesc: (a, b) => SORT_RANK[b.rarity] - SORT_RANK[a.rarity] || a.id - b.id,
  };
  return out.sort(sorters[f.sort]);
}

const caughtMs = (e: LogEntry) => (e.caught_at ? Date.parse(e.caught_at) || -Infinity : -Infinity);
const idCmp = (a: LogEntry, b: LogEntry) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0);
const dateDesc = (a: LogEntry, b: LogEntry) => {
  const d = caughtMs(b) - caughtMs(a);
  return Number.isNaN(d) ? 0 : d;
};
/** caughtAt desc (nil = distant past), then id asc. */
const recent = (a: LogEntry, b: LogEntry) => dateDesc(a, b) || idCmp(a, b);

export function filterLog(all: LogEntry[], f: Filters & { sort: LogSort }, lang: Lang) {
  const m = matcher(f.query);
  const out = all.filter((e) => (!f.rarity || e.rarity === f.rarity) && (!f.shinyOnly || e.is_shiny) && (!m || entryMatch(m, e)));
  const coll = new Intl.Collator(lang);
  const active = out.filter((e) => e.active);
  const rest = out.filter((e) => !e.active);
  switch (f.sort) {
    case "recentFirst":
      return [...active, ...rest.sort(recent)];
    case "oldestFirst":
      return [...rest.sort((a, b) => dateDesc(b, a) || idCmp(a, b)), ...active];
    case "numberAsc":
      return out.sort((a, b) => a.final_id - b.final_id || recent(a, b));
    case "numberDesc":
      return out.sort((a, b) => b.final_id - a.final_id || recent(a, b));
    case "nameAsc":
      return out.sort((a, b) => coll.compare(entryName(a, lang), entryName(b, lang)) || recent(a, b));
    case "nameDesc":
      return out.sort((a, b) => coll.compare(entryName(b, lang), entryName(a, lang)) || recent(a, b));
    case "rarityDesc":
      return out.sort((a, b) => SORT_RANK[b.rarity] - SORT_RANK[a.rarity] || recent(a, b));
  }
}

// ---- paging ----

export const pageCount = (n: number) => Math.max(1, Math.ceil(n / PAGE_SIZE));
/** The page showing `id`, 0 when absent (Back from a detail page lands there). */
export const pageContaining = (id: string | null, ids: string[]) => {
  const i = id == null ? -1 : ids.indexOf(id);
  return i < 0 ? 0 : Math.floor(i / PAGE_SIZE);
};

/**
 * Wheel → page steps (DexScrollPager): a line-mode or ≥100 px event is one notch = one page;
 * touchpad pixels accumulate to 24 px, once per gesture (a 150 ms pause ends a gesture).
 * Returns +1 next, −1 previous, 0 nothing. Negative delta (content up) = next, like macOS.
 */
export class WheelPager {
  static THRESHOLD = 24;
  static IDLE_MS = 150;
  private acc = 0;
  private fired = false;
  private last = -Infinity;

  step(deltaY: number, deltaMode: number, now: number): number {
    // DOM deltaY is positive when scrolling down (content moves up): flip to the AppKit sign.
    const d = -deltaY;
    if (deltaMode === 1 || Math.abs(deltaY) >= 100) return Math.sign(d) < 0 ? 1 : Math.sign(d) > 0 ? -1 : 0;
    if (now - this.last > WheelPager.IDLE_MS) {
      this.acc = 0;
      this.fired = false;
    }
    this.last = now;
    if (this.fired) return 0;
    this.acc += d;
    if (Math.abs(this.acc) < WheelPager.THRESHOLD) return 0;
    const r = this.acc < 0 ? 1 : -1;
    this.acc = 0;
    this.fired = true;
    return r;
  }
}

/** "2 yrs, 10 mths": the two largest units of the elapsed time, no "ago". */
export function relativeAge(fromMs: number, nowMs: number, lang: string) {
  let s = Math.max(0, Math.floor((nowMs - fromMs) / 1000));
  const units: [Intl.NumberFormatOptions["unit"], number][] = [
    ["year", 365 * 86400], ["month", 30 * 86400], ["day", 86400], ["hour", 3600], ["minute", 60], ["second", 1],
  ];
  const parts: string[] = [];
  for (const [unit, size] of units) {
    const n = Math.floor(s / size);
    s -= n * size;
    if (n > 0 && parts.length < 2) parts.push(new Intl.NumberFormat(lang, { style: "unit", unit, unitDisplay: "short" }).format(n));
  }
  if (!parts.length) parts.push(new Intl.NumberFormat(lang, { style: "unit", unit: "second", unitDisplay: "short" }).format(0));
  // Intl.ListFormat is ES2021 (WebView2 and Node have it; the tsconfig lib predates it).
  type LF = new (l: string, o: object) => { format(p: string[]): string };
  const ListFormat = (Intl as unknown as { ListFormat?: LF }).ListFormat;
  return ListFormat ? new ListFormat(lang, { style: "short", type: "unit" }).format(parts) : parts.join(", ");
}
