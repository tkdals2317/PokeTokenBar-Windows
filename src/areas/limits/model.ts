// Pure limits rules for the popover and settings (limits.md §3.11-3.14, §5.3). Owner: limits area.
import { percent, relativeDuration } from "../../kit/format";
import { gauge, PaceTier, paceFraction, tier } from "../../kit/pace";
import type { ActiveBlock, LimitWindow, ProviderLimits } from "../../shared";
import type { limitsStrings } from "./strings";

export type LimitsText = (typeof limitsStrings)["en"];
export type DisplayMode = "used" | "remaining";

/** Values older than 15 minutes show "Stale · N min". */
export const STALE_MS = 15 * 60 * 1000;
/** The reset clock drops weekday/day when the reset is today or within 6 h. */
const RESET_CLOCK_WINDOW_MS = 6 * 3600 * 1000;
const GAP = 5;

export const displayPercent = (u: number, mode: DisplayMode) => (mode === "remaining" ? Math.max(0, 100 - u) : u);

/** "62%", or "62% left" / "62% 남음" / "残り62%" in remaining mode. */
export const percentText = (u: number, mode: DisplayMode, t: LimitsText) => {
  const text = percent(displayPercent(u, mode));
  return mode === "remaining" ? t.percentRemaining(text) : text;
};

export const isStale = (updatedAt: string | null, now: number) =>
  updatedAt != null && now - Date.parse(updatedAt) > STALE_MS;

/** " (16:57)" today or within 6 h, else " (Thursday 1, 14:57)", 24-hour, app language. */
export function resetClock(reset: number, now: number, lang: string): string {
  const d = new Date(reset);
  const today = new Date(now).toDateString() === d.toDateString();
  const near = today || reset - now <= RESET_CLOCK_WINDOW_MS;
  const time: Intl.DateTimeFormatOptions = { hour: "2-digit", minute: "2-digit", hourCycle: "h23" };
  if (near) return ` (${new Intl.DateTimeFormat(lang, time).format(d)})`;
  const f = new Intl.DateTimeFormat(lang, { ...time, weekday: "long", day: "numeric" });
  if (lang !== "en") return ` (${f.format(d)})`;
  // ICU's English skeleton reads "8 Thursday"; macOS `EEEEdHHmm` reads "Thursday 8, 14:57".
  const part = (type: string) => f.formatToParts(d).find((p) => p.type === type)?.value ?? "";
  return ` (${part("weekday")} ${part("day")}, ${new Intl.DateTimeFormat(lang, time).format(d)})`;
}

/** "1 hr, 59 min (16:57)" */
export const resetLabel = (reset: number, now: number, lang: string) =>
  relativeDuration((reset - now) / 1000, lang) + resetClock(reset, now, lang);

export const windowReset = (w: LimitWindow) => (w.resets_at ? Date.parse(w.resets_at) : null);

/** Elapsed fraction of the window, `null` without a reset or span (no marker). */
export function windowPace(w: LimitWindow, now: number): number | null {
  const reset = windowReset(w);
  return reset == null || !w.span_secs ? null : paceFraction(reset, w.span_secs * 1000, now);
}

export type RowLook = { gauge: PaceTier; tier: PaceTier | null; pace: number | null; marker: number | null };

export function rowLook(u: number, pace: number | null, warn: number, crit: number, mode: DisplayMode): RowLook {
  return {
    gauge: gauge(u, pace, warn, crit),
    tier: tier(u, pace, crit),
    pace,
    marker: pace == null ? null : displayPercent(pace * 100, mode) / 100,
  };
}

const TIER_KEY: Record<PaceTier, keyof LimitsText> = {
  wayUnder: "tierWayUnder", under: "tierUnder", onPace: "tierOnPace",
  slightlyOver: "tierSlightlyOver", over: "tierOver", wayOver: "tierWayOver",
};

/** Swift `.rounded()`: half away from zero. */
const roundAway = (v: number) => Math.sign(v) * Math.round(Math.abs(v));

/** Row tooltip: "<tier> · <delta>\n<pace hint>", or only the hint before the 0.1 hold ends. */
export function paceHelp(look: RowLook, u: number, mode: DisplayMode, t: LimitsText): string | null {
  if (look.pace == null) return null;
  const hint = t.paceHint(percent(displayPercent(look.pace * 100, mode)));
  if (!look.tier) return hint;
  const d = roundAway(u - look.pace * 100);
  const head = t[TIER_KEY[look.tier]] as string;
  const delta = d > 0 ? t.paceMore(d) : d < 0 ? t.paceLess(-d) : null;
  return `${delta ? `${head} · ${delta}` : head}\n${hint}`;
}

/** `codexWindow(minutes)` */
export function codexWindowName(minutes: string | null, t: LimitsText): string {
  const m = minutes == null ? null : Number(minutes);
  if (m == null || !Number.isFinite(m)) return t.limit;
  if (m === 300) return t.fiveHourSession;
  if (m === 10080) return t.weekly;
  return m >= 60 && m % 60 === 0 ? t.hours(m / 60) : t.minutes(m);
}

export function rowName(w: LimitWindow, t: LimitsText): string {
  switch (w.label) {
    case "fiveHour": return t.fiveHourSession;
    case "weekly": return t.weekly;
    case "weeklyOpus": return t.weeklyOpus;
    case "weeklySonnet": return t.weeklySonnet;
    case "scoped": return w.arg ? t.weeklyModel(w.arg) : t.weeklyScoped;
    case "raw": return w.arg ?? "";
    case "codex": return codexWindowName(w.arg, t);
    case "personal": return t.personalSpendLimit;
    case "cursorIncluded": return t.cursorMonthlyIncluded;
    case "cursorAuto": return t.cursorAutoUsage;
    case "cursorApi": return t.cursorApiUsage;
    case "monthly": return t.openCodeGoMonthly;
    case "credits": return w.arg === "weekly" ? t.grokWeekly : w.arg === "monthly" ? t.grokMonthly : t.grokCredits;
    default: return t.limit;
  }
}

/** `friendlyLimitError`: Rust error code → sentence. */
export function friendlyError(code: string | null, t: LimitsText): string | null {
  if (!code) return null;
  const http = code.match(/^http_(\d+)$/);
  if (http) {
    const n = Number(http[1]);
    return n === 401 || n === 403 ? t.limitRefreshHTTPError(n) : t.limitRefreshHTTPFailed(n);
  }
  switch (code) {
    case "rate_limited": return t.limitRefreshRateLimited;
    case "no_credential": case "format": case "token_expired": return t.limitRefreshNoCredential;
    case "reauth": return t.limitRefreshReauthNeeded;
    case "session_key_malformed": return t.sessionKeyMalformedError;
    case "session_key_invalid": return t.sessionKeyExpiredError;
    case "session_key_no_org": return t.sessionKeyNoOrgError;
    case "cloudflare": return t.cloudflareBlocked;
    default: return t.limitRefreshGeneric;
  }
}

/** `selectedProviderHasLimits`: which provider tabs get a limits section. */
export function sectionEntries(all: ProviderLimits[], provider: string): ProviderLimits[] {
  const mine = all.filter((p) => p.provider === provider);
  switch (provider) {
    case "claude_code":
      return mine;
    case "codex":
      return mine.filter((p) => p.windows.length > 0);
    default:
      return mine.filter((p) => p.windows.length > 0 || p.auth_expired != null);
  }
}

/** 5-hour forecast for the tracked account: depletion time and whether it beats the reset. */
export function forecast(w: LimitWindow | undefined, block: ActiveBlock | null | undefined, now: number) {
  const reset = w ? windowReset(w) : null;
  if (!w || reset == null) return null;
  const u = w.used_percent;
  if (u >= 100) return { at: now, beforeReset: true };
  if (!block || u < 5 || block.total <= 0 || block.tokens_per_minute < 10_000) return null;
  const minutes = ((100 - u) * (block.total / u)) / block.tokens_per_minute;
  if (!Number.isFinite(minutes) || minutes >= 1440) return null;
  const at = now + minutes * 60_000;
  return { at, beforeReset: at < reset };
}

/** Settings sliders keep a 5-point gap: warn ≥ crit pushes crit up, crit ≤ warn pushes warn down. */
export function setWarn(warn: number, crit: number) {
  return { warn_threshold: warn, crit_threshold: warn >= crit ? Math.min(100, warn + GAP) : crit };
}
export function setCrit(warn: number, crit: number) {
  return { warn_threshold: crit <= warn ? Math.max(50, crit - GAP) : warn, crit_threshold: crit };
}
