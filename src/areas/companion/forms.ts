// Alternate forms and sprite URLs (companion.md §3.5, §4.5). Owner: companion area.
// The Rust side has the same catalog in src-tauri/src/companion/form.rs; keep both in sync.

/** [form, pokemon-form id (names), variety id holding the static PNG (or null)]. Default first. */
type Entry = [string, number, number | null];

const UNOWN = 201;
const LETTERS = [..."abcdefghijklmnopqrstuvwxyz", "exclamation", "question"];

const CATALOG: Record<number, Entry[]> = {
  [UNOWN]: LETTERS.map((l, i): Entry => [l, i === 0 ? UNOWN : 10000 + i, null]),
  351: [["normal", 351, null], ["sunny", 10028, 10013], ["rainy", 10029, 10014], ["snowy", 10030, 10015]],
  386: [["normal", 386, null], ["attack", 10031, 10001], ["defense", 10032, 10002], ["speed", 10033, 10003]],
  412: [["plant", 412, null], ["sandy", 10034, null], ["trash", 10035, null]],
  413: [["plant", 413, null], ["sandy", 10036, 10004], ["trash", 10037, 10005]],
  421: [["overcast", 421, null], ["sunshine", 10038, null]],
  422: [["west", 422, null], ["east", 10039, null]],
  423: [["west", 423, null], ["east", 10040, null]],
  479: [["normal", 479, null], ["heat", 10058, 10008], ["wash", 10059, 10009], ["frost", 10060, 10010], ["fan", 10061, 10011], ["mow", 10062, 10012]],
  487: [["altered", 487, null], ["origin", 10063, 10007]],
  492: [["land", 492, null], ["sky", 10064, 10006]],
  493: (
    [["normal", 493], ["fighting", 10045], ["flying", 10047], ["poison", 10052], ["ground", 10050], ["rock", 10054],
     ["bug", 10041], ["ghost", 10048], ["steel", 10055], ["fire", 10046], ["water", 10056], ["grass", 10049],
     ["electric", 10044], ["psychic", 10053], ["ice", 10051], ["dragon", 10043], ["dark", 10042], ["fairy", 10085]] as [string, number][]
  ).map(([f, id]): Entry => [f, id, null]),
  550: [["red-striped", 550, null], ["blue-striped", 10066, 10016]],
  555: [["standard", 555, null], ["zen", 10067, 10017]],
  585: [["spring", 585, null], ["summer", 10068, null], ["autumn", 10069, null], ["winter", 10070, null]],
  586: [["spring", 586, null], ["summer", 10071, null], ["autumn", 10072, null], ["winter", 10073, null]],
  641: [["incarnate", 641, null], ["therian", 10079, 10019]],
  642: [["incarnate", 642, null], ["therian", 10080, 10020]],
  645: [["incarnate", 645, null], ["therian", 10081, 10021]],
  646: [["normal", 646, null], ["black", 10082, 10022], ["white", 10083, 10023]],
  647: [["ordinary", 647, null], ["resolute", 10084, 10024]],
  648: [["aria", 648, null], ["pirouette", 10074, 10018]],
  649: [["normal", 649, null], ["douse", 10075, null], ["shock", 10076, null], ["burn", 10077, null], ["chill", 10078, null]],
};

const UNNAMED_DEFAULT = new Set([351, 479, 646, 649]);

export const forms = (species: number) => (CATALOG[species] ?? []).map((e) => e[0]);

/** The look at `species`: its own form when that species has it, else the default. Null without forms. */
export function resolvedForm(species: number, form: string | null | undefined): string | null {
  const all = forms(species);
  if (all.length === 0) return null;
  return form && all.includes(form) ? form : all[0];
}

export const isDefaultForm = (species: number, form: string | null | undefined) =>
  (forms(species)[0] ?? null) === resolvedForm(species, form);

const entry = (species: number, form: string | null | undefined) => {
  const f = resolvedForm(species, form);
  return CATALOG[species]?.find((e) => e[0] === f);
};

/** `<id>` for the default look, `<id>-<form>` otherwise (animated GIFs). */
export function assetName(species: number, form?: string | null) {
  const f = resolvedForm(species, form);
  return f && !isDefaultForm(species, f) ? `${species}-${f}` : String(species);
}

/** Static PNGs of separate varieties live under the variety id (`10006.png` for Sky Shaymin). */
export const staticAssetName = (species: number, form?: string | null) =>
  String(entry(species, form)?.[2] ?? assetName(species, form));

/** `pokemon-form` resource holding the localized form name, or null (Unown, unnamed defaults). */
export function formNameResource(species: number, form?: string | null): number | null {
  if (species === UNOWN || (UNNAMED_DEFAULT.has(species) && isDefaultForm(species, form))) return null;
  return entry(species, form)?.[1] ?? null;
}

export const unownSymbol = (f: string) => (f === "exclamation" ? "!" : f === "question" ? "?" : f.toUpperCase());

/** "sky-forme" → "Sky Forme": offline fallback for resource names. */
export const identifier = (raw: string) =>
  raw.split("-").map((w) => w.charAt(0).toUpperCase() + w.slice(1)).join(" ");

/**
 * Form label for the title: Unown's letter, "Normal Form" for unnamed defaults, the localized
 * PokéAPI name, or the identifier until it loads. Null for species without forms.
 */
export function formLabel(species: number, form: string | null | undefined, localized: string | null, normalForm: string) {
  const f = resolvedForm(species, form);
  if (!f) return null;
  if (species === UNOWN) return unownSymbol(f);
  if (UNNAMED_DEFAULT.has(species) && isDefaultForm(species, f)) return normalForm;
  return localized ?? identifier(f);
}

/** Unown always "Name [A]"; others "Name [Label]" only for a labelled non-default form. */
export function formDisplayName(name: string, species: number, form: string | null | undefined, label: string | null) {
  const f = resolvedForm(species, form);
  if (!f) return name;
  if (species === UNOWN) return `${name} [${unownSymbol(f)}]`;
  return label && !isDefaultForm(species, f) ? `${name} [${label}]` : name;
}

// ---- sprites ----

const BASE = "https://raw.githubusercontent.com/PokeAPI/sprites/master/sprites/pokemon";
export const EGG_SPRITE = `${BASE}/egg.png`;
export const ANIMATED_MAX = 649;

export type SpriteOpts = { shiny?: boolean; back?: boolean; animated?: boolean; form?: string | null };

export function spriteUrl(id: number, o: SpriteOpts = {}) {
  const animated = (o.animated ?? true) && id >= 1 && id <= ANIMATED_MAX;
  const name = animated ? assetName(id, o.form) : staticAssetName(id, o.form);
  return `${BASE}/${animated ? "versions/generation-v/black-white/animated/" : ""}${o.back ? "back/" : ""}${o.shiny ? "shiny/" : ""}${name}.${animated ? "gif" : "png"}`;
}

/**
 * Fallback chain for one sprite: missing shiny → plain, missing back → front, missing animated →
 * static (companion.md §4.5). The <img> walks it on each load error.
 */
export function spriteChain(id: number, o: SpriteOpts = {}): string[] {
  const out: string[] = [];
  const add = (x: SpriteOpts) => {
    const u = spriteUrl(id, x);
    if (!out.includes(u)) out.push(u);
  };
  for (const animated of [o.animated ?? true, false]) {
    for (const back of o.back ? [true, false] : [false]) {
      for (const shiny of o.shiny ? [true, false] : [false]) add({ ...o, animated, back, shiny });
    }
  }
  return out;
}
