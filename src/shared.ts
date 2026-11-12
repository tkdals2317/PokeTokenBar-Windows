// Backend contract (mirrors src-tauri/src/model.rs and the area settings blocks) and app-wide hooks.
// Owner: foundation, except the `#region <area>` blocks, which belong to that area.
import { createContext, useCallback, useContext, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { check, Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { LegacyDict, legacyDict, nameLang } from "./i18n";
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
export type CompanionSettings = { representative: number | null; companion_notifications: boolean };
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
export type Rarity = "common" | "uncommon" | "rare" | "legendary";
export type Names = Record<string, Record<string, string>>;
export type Mon = {
  line: { base_id: number; rarity: Rarity; names: Names };
  path_ids: number[];
  planned_ids: number[];
  stage_index: number;
  used_at_stage: number;
  is_shiny: boolean;
  nature: string;
};
export type DexEntry = {
  base_id: number;
  final_id: number;
  chain: number[];
  rarity: Rarity;
  is_shiny: boolean;
  nature: string;
  names: Names;
  caught_at: string;
};
export type Companion = {
  active: Mon | null;
  egg_usage: number;
  egg_threshold: number;
  threshold: number | null;
  difficulty: number;
  dex: DexEntry[];
};
export type CompanionEvent =
  | { kind: "hatched"; id: number; shiny: boolean }
  | { kind: "evolved"; id: number }
  | { kind: "graduated"; id: number };

const SPRITES = "https://raw.githubusercontent.com/PokeAPI/sprites/master/sprites/pokemon";
export const sprite = (id: number, shiny = false) =>
  `${SPRITES}/versions/generation-v/black-white/animated/${shiny ? "shiny/" : ""}${id}.gif`;
export const EGG = `${SPRITES}/egg.png`;

export function nameOf(names: Names, id: number, lang: Lang) {
  const n = names[id] ?? {};
  return n[nameLang(lang)] ?? n.en ?? `#${id}`;
}

/** Species the user owns: every graduated chain plus the stages the active Pokémon has reached. */
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
  a?.path_ids.slice(0, a.stage_index + 1).forEach((id) => add(id, a.line.names, a.line.rarity, a.is_shiny, true));
  return out;
}

/** What the pet and tray show: the chosen representative if still owned, else the active Pokémon. */
export function displayed(c: Companion, s: Settings | null): { id: number; shiny: boolean; names: Names } | null {
  const rep = s?.representative != null ? ownedSpecies(c).get(s.representative) : undefined;
  if (rep) return { id: rep.id, shiny: rep.shiny, names: rep.names };
  const a = c.active;
  return a ? { id: a.path_ids[a.stage_index], shiny: a.is_shiny, names: a.line.names } : null;
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
export type LimitWindow = { key: string; used_percent: number; resets_at: string | null };
export type ProviderLimits = { provider: string; windows: LimitWindow[]; error: string | null };
// #endregion

/** LEGACY context for components still on the frozen `src/i18n.ts` dictionary. */
export const I18n = createContext<{ lang: Lang; t: LegacyDict }>({ lang: "en", t: legacyDict("en") });
export const useI18n = () => useContext(I18n);

// #region pet-settings-misc
/** Update checks at launch and on popover open wait this long after the last successful one (UpdateChecker.swift). */
export const UPDATE_CHECK_COOLDOWN_SECS = 1800;
// #endregion

// ---- updater ----

export type UpdateState =
  | { status: "idle" | "checking" | "none" | "error" }
  | { status: "available"; update: Update }
  | { status: "downloading"; update: Update; progress: number };

/** In-app updater: checks GitHub Releases' latest.json at launch and on popover open (30 min cooldown
 *  after a success only), and manually with no cooldown. Installs on request. */
export function useUpdater() {
  const [state, setState] = useState<UpdateState>({ status: "idle" });
  const lastSuccess = useRef<number | null>(null);
  const inFlight = useRef(false);

  async function checkNow(minIntervalSecs = 0) {
    if (inFlight.current) return;
    if (lastSuccess.current != null && Date.now() - lastSuccess.current < minIntervalSecs * 1000) return;
    const manual = minIntervalSecs === 0;
    inFlight.current = true;
    if (manual) setState({ status: "checking" });
    try {
      const update = await check();
      lastSuccess.current = Date.now();
      setState((s) => (s.status === "downloading" ? s : update ? { status: "available", update } : { status: "none" }));
    } catch {
      if (manual) setState({ status: "error" }); // a background failure keeps what was known
    } finally {
      inFlight.current = false;
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
    checkNow(UPDATE_CHECK_COOLDOWN_SECS);
  }, []);
  usePopoverOpened(useCallback(() => void checkNow(UPDATE_CHECK_COOLDOWN_SECS), []));
  return { state, checkNow, install };
}
export type Updater = ReturnType<typeof useUpdater>;
