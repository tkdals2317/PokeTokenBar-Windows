// Individual stats from a profile and PokéAPI details (companion.md §3.9, Profile:225-274).
// Owner: companion area. The dex detail page (shop-bag-dex) renders these.

export type PokemonDetails = {
  species_id: number;
  name: string;
  height: number;
  weight: number;
  base_experience: number | null;
  gender_rate: number;
  types: string[];
  base_stats: Record<string, number>;
  abilities: { name: string; slot: number; is_hidden: boolean }[];
  moves: { name: string; learn_methods: { method: string; level: number }[] }[];
};

export type Ivs = { hp: number; attack: number; defense: number; special_attack: number; special_defense: number; speed: number };
export type Profile = {
  instance_id: string;
  seed: string;
  gender: "male" | "female" | "genderless" | null;
  ivs: Ivs;
  ability_slot: number | null;
  ability_name: string | null;
  ability_is_hidden: boolean;
  level: number;
  growth_tokens: number;
  moves: { name: string; learned_at_level: number }[];
};

export const STAT_ORDER = ["hp", "attack", "defense", "special-attack", "special-defense", "speed"] as const;
const SHEDINJA = 292;

/** [raised, lowered] stat per nature; neutral natures are absent. */
const NATURE_MODS: Record<string, [string, string]> = {
  lonely: ["attack", "defense"], brave: ["attack", "speed"], adamant: ["attack", "special-attack"],
  naughty: ["attack", "special-defense"], bold: ["defense", "attack"], relaxed: ["defense", "speed"],
  impish: ["defense", "special-attack"], lax: ["defense", "special-defense"], timid: ["speed", "attack"],
  hasty: ["speed", "defense"], jolly: ["speed", "special-attack"], naive: ["speed", "special-defense"],
  modest: ["special-attack", "attack"], mild: ["special-attack", "defense"], quiet: ["special-attack", "speed"],
  rash: ["special-attack", "special-defense"], calm: ["special-defense", "attack"], gentle: ["special-defense", "defense"],
  sassy: ["special-defense", "speed"], careful: ["special-defense", "special-attack"],
};

export function natureModifier(nature: string | null | undefined, stat: string) {
  const p = nature ? NATURE_MODS[nature] : undefined;
  return p?.[0] === stat ? 1.1 : p?.[1] === stat ? 0.9 : 1;
}

const ivOf = (ivs: Ivs, stat: string) => ivs[stat.replace("-", "_") as keyof Ivs] ?? 0;

export type ComputedStat = { name: string; base: number; iv: number; value: number };

/** No EVs. HP: ((2B+IV)·L)/100 + L + 10 (Shedinja 1); others floor((((2B+IV)·L)/100 + 5) · nature). */
export function computeStats(d: PokemonDetails, p: Profile, nature: string | null): ComputedStat[] {
  const L = p.level;
  return STAT_ORDER.flatMap((name) => {
    const base = d.base_stats[name];
    if (base == null) return [];
    const iv = ivOf(p.ivs, name);
    const raw = Math.floor(((2 * base + iv) * L) / 100);
    const value =
      name === "hp" ? (d.species_id === SHEDINJA ? 1 : raw + L + 10) : Math.floor((raw + 5) * natureModifier(nature, name));
    return [{ name, base, iv, value }];
  });
}

/** Bar scale: max(300, highest) rounded up to the next 100. */
export function displayScaleMaximum(values: number[]) {
  const hi = Math.max(300, ...values);
  return hi % 100 === 0 ? hi : hi + (100 - (hi % 100));
}
