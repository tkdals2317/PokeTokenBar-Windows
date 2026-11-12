// Backend contract (mirrors src-tauri/src/model.rs and the area settings blocks) and app-wide hooks.
// Owner: foundation, except the `#region <area>` blocks, which belong to that area.
import { createContext, useContext, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { LogicalSize, PhysicalPosition, Window } from "@tauri-apps/api/window";
import { check, Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { LegacyDict, legacyDict } from "./i18n";
import type { Lang } from "./kit/i18n";
import type { CostCoverage } from "./kit/format";

// ---- usage model (model.rs) ----

export type Tokens = { input: number; output: number; cache_write: number; cache_read: number };
export type Usage = { tokens: Tokens; total: number; cost: number; coverage: CostCoverage };
export type ModelUsage = Usage & { model: string };
export type DailyUsage = Usage & { date: string; models: ModelUsage[] | null };
export type PeriodUsage = Usage & { from: string; to: string };
export type DayPoint = { date: string; total: number; cost: number };
export type ActiveBlock = { start: string; end: string; total: number; tokens_per_minute: number };
export type ProviderSnapshot = {
  provider_id: string;
  display_name: string;
  today: DailyUsage | null;
  active_block: ActiveBlock | null;
  week_total: PeriodUsage | null;
  month_total: PeriodUsage | null;
  month_daily: DayPoint[];
  recent_daily: DayPoint[];
  last_usage: string | null;
  reports_cost: boolean;
};
export type UsageSnapshot = { providers: ProviderSnapshot[]; fetched_at: string | null; error: string | null };
export type DailyLedger = { tokens_by_day: Record<string, number>; covered_since: string | null };

/** Local `yyyy-MM-dd`. */
export const todayKey = (d = new Date()) =>
  `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
export const todayTotal = (u: UsageSnapshot, day = todayKey()) =>
  u.providers.reduce((n, p) => n + (p.today?.date === day ? p.today.total : 0), 0);

/** `popover-opened` payload: where the popover lands. Default Home, Settings closed. */
export type PopoverOpened = { tab: Tab | null; settings: boolean };
export type Tab = "home" | "shop" | "bag" | "collection";

/** Props every tab body receives from the shell. */
export type TabProps = {
  usage: UsageSnapshot | null;
  companion: Companion | null;
  limits: ProviderLimits[];
  settings: Settings;
  save: (next: Settings) => void;
  goTo: (tab: Tab) => void;
};

// ---- settings (settings.json, one flat object; each block mirrors an area's Rust struct) ----

export type ShellSettings = {
  language: Lang;
  refresh_minutes: number;
  show_tokens_in_menu: boolean;
  show_cost_in_menu: boolean;
  show_growth_in_menu: boolean;
  show_limit_in_menu: boolean;
  menu_limit_color_mode: "gauge" | "attention" | "off";
  update_notifications_enabled: boolean;
  skipped_update_version: string | null;
};
// #region providers
export type ProviderSettings = {
  disabled_providers: string[];
  custom_scan_roots: Record<string, string>;
  additional_claude_config_dirs: string;
};
// #endregion
// #region limits
export type LimitSettings = {
  limit_display: "used" | "remaining";
  limit_notifications: boolean;
  warn_threshold: number;
  crit_threshold: number;
  status_checks_enabled: boolean;
  /** "automatic" | "default" | "highest" | "account:<id>" */
  claude_tracked_account: string;
};
// #endregion
// #region home-usage
export type HomeSettings = {
  daily_trend_window: "7d" | "14d" | "21d" | "month";
  activity_visualization: "monthlyBars" | "recentHeatmap";
};
// #endregion
// #region companion
export type CompanionSettings = {
  representative: number | null;
  representative_form: string | null;
  companion_notifications: boolean;
};
// #endregion
// #region shop-bag-dex
export type EconomySettings = { shop_difficulty: number };
// #endregion
// #region pet-settings-misc
export type PetSettings = {
  floating_pet_enabled: boolean;
  always_faces_back: boolean;
  floating_pet_size: number;
  floating_pet_bubble_alerts: boolean;
  floating_pet_agent_activity: boolean;
  animation_quality: "powerSaver" | "balanced" | "smooth";
};
// #endregion
export type Settings = ShellSettings &
  ProviderSettings &
  LimitSettings &
  HomeSettings &
  CompanionSettings &
  EconomySettings &
  PetSettings;

// #region companion
import { EGG_SPRITE, spriteUrl, type SpriteOpts } from "./areas/companion/forms";
import type { Profile } from "./areas/companion/profile";
export type { Profile, PokemonDetails } from "./areas/companion/profile";
export { computeStats, displayScaleMaximum } from "./areas/companion/profile";
export { spriteChain, spriteUrl, resolvedForm, formDisplayName, formLabel, formNameResource } from "./areas/companion/forms";

export type Rarity = "common" | "uncommon" | "rare" | "legendary";
export type Names = Record<string, Record<string, string>>;
export type EvoNode = { id: number; children: EvoNode[] };
export type Prayers = { count: number };
export type Mon = {
  line: { base_id: number; tree: EvoNode; rarity: Rarity; names: Names };
  path_ids: number[];
  planned_ids: number[];
  stage_index: number;
  used_at_stage: number;
  total_forms: number;
  is_shiny: boolean;
  nature: string;
  form: string | null;
  profile: Profile | null;
  has_growth_boost: boolean;
  ditto_disguise: number | null;
  ditto_revealed: boolean;
  prayers: Prayers | null;
};
export type DexEntry = {
  id: string;
  base_id: number;
  final_id: number;
  chain: number[];
  rarity: Rarity;
  is_shiny: boolean;
  nature: string;
  names: Names;
  /** RFC 3339; the release time for released records. */
  caught_at: string;
  form: string | null;
  profile: Profile | null;
  prayers: Prayers | null;
  names_version: number | null;
  /** Set for a Pokémon released for a new egg (not graduated). */
  released_at: string | null;
};
export type DisplayState = "egg" | "idle" | "working" | "focus" | "tired" | "sleep" | "sulk" | "levelUp";
export type RepresentativeSubject = { id: number; shiny: boolean; form: string | null; back: boolean };
export type Companion = {
  active: Mon | null;
  egg_usage: number;
  egg_threshold: number;
  threshold: number | null;
  difficulty: number;
  dex: DexEntry[];
  egg_tier: Rarity | null;
  hatch_retry_delayed: boolean;
  display_state: DisplayState;
  /** Back sprite on every surface: always-back setting or sulking. */
  faces_back: boolean;
  just_evolved_to: string | null;
  just_graduated: string | null;
  current_id: number | null;
  current_form: string | null;
  /** Shiny as shown: a disguised Ditto hides it until the reveal. */
  displays_shiny: boolean;
  /** Types of the current species once its details are cached. */
  current_types: string[] | null;
  is_final_stage: boolean;
  growth_multiplier: number | null;
  tokens_to_next: number;
  line_nodes: { id: number | null; state: "done" | "current" | "future" }[];
  /** pokemon-form id → {lang: name} */
  form_labels: Record<string, Record<string, string>>;
  celebration: { seq: number; kind: "hatch" | "evolve" | "ditto_reveal"; shiny: boolean } | null;
  candy: { seq: number; xp: number } | null;
  mint_seq: number;
  /** Newest notification body, for the floating pet bubble. */
  latest_event: { seq: number; text: string; at: string } | null;
  /** What the tray and pet show: the owned representative, else the raised Pokémon; null = egg. */
  representative_subject: RepresentativeSubject | null;
  shiny_denominator: number;
};
export type CompanionEvent =
  | { kind: "hatched"; id: number; shiny: boolean }
  | { kind: "evolved"; id: number }
  | { kind: "graduated"; id: number };

/** Sprite URL: animated GIF up to #649 (static PNG above), form-aware, optional back view. */
export const sprite = (id: number, shiny = false, opts: Omit<SpriteOpts, "shiny"> = {}) => spriteUrl(id, { ...opts, shiny });
export const EGG = EGG_SPRITE;

const API_CODES: Record<Lang, string[]> = {
  ko: ["ko"], en: ["en"], ja: ["ja-hrkt", "ja"], es: ["es"], fr: ["fr"], pt: ["pt-br", "pt"], de: ["de"], ru: ["ru"],
};

/** Localized name from a `{lang: name}` map: app language codes, then en. */
export function resolveName(n: Record<string, string> | undefined, lang: Lang): string | null {
  if (!n) return null;
  const norm: Record<string, string> = {};
  for (const [k, v] of Object.entries(n)) if (v.trim()) norm[k.trim().toLowerCase().replace(/_/g, "-")] = v.trim();
  for (const c of [...API_CODES[lang], "en"]) if (norm[c]) return norm[c];
  return null;
}

export function nameOf(names: Names, id: number, lang: Lang) {
  return resolveName(names[id], lang) ?? `#${id}`;
}

/** Species the user owns: every dex chain plus the stages the active Pokémon has reached. */
export type OwnedSpecies = {
  id: number;
  names: Names;
  rarity: Rarity;
  shiny: boolean;
  count: number;
  growing: boolean;
};

export function ownedSpecies(c: Companion): Map<number, OwnedSpecies> {
  const out = new Map<number, OwnedSpecies>();
  const add = (id: number, names: Names, rarity: Rarity, shiny: boolean, growing: boolean) => {
    const s = out.get(id) ?? { id, names, rarity, shiny: false, count: 0, growing: false };
    s.shiny ||= shiny;
    s.growing ||= growing;
    if (!growing) s.count += 1;
    out.set(id, s);
  };
  c.dex.forEach((d) => d.chain.forEach((id) => add(id, d.names, d.rarity, d.is_shiny, false)));
  const a = c.active;
  // displays_shiny: a disguised Ditto must not reveal its shiny early.
  a?.path_ids.slice(0, a.stage_index + 1).forEach((id) => add(id, a.line.names, a.line.rarity, c.displays_shiny, true));
  return out;
}

/** What the pet and tray show (Rust resolves ownership, shiny, form and back view). */
export function displayed(
  c: Companion,
  _s?: Settings | null,
): { id: number; shiny: boolean; names: Names; form: string | null; back: boolean } | null {
  const r = c.representative_subject;
  if (!r) return null;
  const names = ownedSpecies(c).get(r.id)?.names ?? {};
  return { ...r, names };
}
// #endregion

// ---- live backend state ----

/** Initial fetch plus the events the Rust side emits on every refresh. */
export function useBackend() {
  const [usage, setUsage] = useState<UsageSnapshot | null>(null);
  const [companion, setCompanion] = useState<Companion | null>(null);
  const [limits, setLimits] = useState<ProviderLimits[]>([]);
  const [settings, setSettings] = useState<Settings | null>(null);

  useEffect(() => {
    invoke<UsageSnapshot>("usage_snapshot").then(setUsage);
    invoke<Companion>("companion").then(setCompanion);
    invoke<ProviderLimits[]>("limits").then(setLimits);
    invoke<Settings>("get_settings").then(setSettings);
    const subs = [
      listen<UsageSnapshot>("usage", (e) => setUsage(e.payload)),
      listen<Companion>("companion", (e) => setCompanion(e.payload)),
      listen<ProviderLimits[]>("limits", (e) => setLimits(e.payload)),
      listen<Settings>("settings", (e) => setSettings(e.payload)),
    ];
    return () => subs.forEach((p) => p.then((f) => f()));
  }, []);

  const saveSettings = (next: Settings) => invoke<Settings>("set_settings", { value: next }).then(setSettings);
  return { usage, companion, limits, settings, saveSettings };
}

/** Runs `cb` every time the popover opens (reset per-open UI state here). */
export function usePopoverOpened(cb: (p: PopoverOpened) => void) {
  useEffect(() => {
    const sub = listen<PopoverOpened>("popover-opened", (e) => cb(e.payload));
    return () => {
      sub.then((f) => f());
    };
  }, [cb]);
}

// #region limits
/** One limit row (limits/mod.rs `Window`). `label` + `arg` name it; see areas/limits/model.ts. */
export type LimitWindow = {
  key: string;
  label: string;
  arg: string | null;
  group: string | null;
  used_percent: number;
  resets_at: string | null;
  span_secs: number | null;
  detail: string | null;
  not_started: boolean;
  expired: boolean;
};
export type ClaudeAccount = {
  id: string;
  key_prefix: string;
  folder: string;
  title: string;
  is_default: boolean;
  tracked: boolean;
  session_key: boolean;
};
export type ProviderLimits = {
  provider: string;
  account: ClaudeAccount | null;
  windows: LimitWindow[];
  error: string | null;
  updated_at: string | null;
  auth_expired: "oauth" | "sessionKey" | "token" | null;
  plan: string | null;
  account_label: string | null;
  limit_reached: boolean;
  message: string | null;
  remaining_usd: number | null;
  products: { name: string; percent: number }[];
};
// #endregion

/** LEGACY context for components still on the frozen `src/i18n.ts` dictionary. */
export const I18n = createContext<{ lang: Lang; t: LegacyDict }>({ lang: "en", t: legacyDict("en") });
export const useI18n = () => useContext(I18n);

// #region pet-settings-misc
export const PET_SIZE = { min: 48, max: 384, step: 8 };

async function petWindow() {
  return Window.getByLabel("pet");
}

export async function petSize() {
  const w = await petWindow();
  if (!w) return 120;
  const [size, scale] = await Promise.all([w.innerSize(), w.scaleFactor()]);
  return Math.round(size.width / scale);
}

/** Resizes the pet window, keeping its bottom-center (where the sprite stands) in place. */
export async function resizePet(px: number) {
  const w = await petWindow();
  if (!w) return;
  const [pos, size, scale] = await Promise.all([w.outerPosition(), w.outerSize(), w.scaleFactor()]);
  const next = Math.round(px * scale);
  await w.setSize(new LogicalSize(px, px));
  await w.setPosition(new PhysicalPosition(pos.x + Math.round((size.width - next) / 2), pos.y + size.height - next));
}
// #endregion

// ---- updater ----

export type UpdateState =
  | { status: "idle" | "checking" | "none" | "error" }
  | { status: "available"; update: Update }
  | { status: "downloading"; update: Update; progress: number };

/** In-app updater: checks GitHub Releases' latest.json once on open, installs on request. */
export function useUpdater() {
  const [state, setState] = useState<UpdateState>({ status: "idle" });

  async function checkNow() {
    setState({ status: "checking" });
    try {
      const update = await check();
      setState(update ? { status: "available", update } : { status: "none" });
    } catch {
      setState({ status: "error" });
    }
  }

  async function install() {
    if (state.status !== "available") return;
    const { update } = state;
    let total = 0;
    let received = 0;
    setState({ status: "downloading", update, progress: 0 });
    try {
      await update.downloadAndInstall((e) => {
        if (e.event === "Started") total = e.data.contentLength ?? 0;
        if (e.event === "Progress") {
          received += e.data.chunkLength;
          setState({ status: "downloading", update, progress: total ? received / total : 0 });
        }
      });
      // On Windows the installer usually closes the app itself; this covers the rest.
      await relaunch();
    } catch {
      setState({ status: "error" });
    }
  }

  useEffect(() => {
    checkNow();
  }, []);
  return { state, checkNow, install };
}
export type Updater = ReturnType<typeof useUpdater>;
