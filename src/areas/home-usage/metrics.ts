// Pure Home-tab logic (home-usage.md §3.4-3.7): series merging, trend columns/axis/stacking, heatmap grid,
// streak and recap. Ports of macOS DailyTrendMetrics / DailyTrendStack / ActivityHeatmapMetrics /
// UsageLedger / UsageRecap. Owner: home-usage area. Tested by metrics.test.ts.
import type { CostCoverage } from "../../kit/format";
import type { DailyLedger, DayPoint, ProviderSnapshot } from "../../shared";

export type TrendWindow = "7d" | "14d" | "21d" | "month";
export const TREND_WINDOWS: TrendWindow[] = ["7d", "14d", "21d", "month"];
const WINDOW_DAYS: Record<TrendWindow, number | null> = { "7d": 7, "14d": 14, "21d": 21, month: null };

// ---- day keys (local calendar, "yyyy-MM-dd"; lexicographic order = chronological order) ----

const pad = (n: number) => String(n).padStart(2, "0");
export const keyOf = (d: Date) => `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
export function dateOf(key: string): Date {
  const [y, m, d] = key.split("-").map(Number);
  return new Date(y, m - 1, d);
}
/** Calendar-day arithmetic (never 86 400 s: DST days are 23/25 h long). */
export function addDays(d: Date, n: number): Date {
  return new Date(d.getFullYear(), d.getMonth(), d.getDate() + n);
}
export const shiftKey = (key: string, n: number) => keyOf(addDays(dateOf(key), n));
/** Whole days from `from` to `to` (positive when `to` is later). */
export const dayDiff = (from: string, to: string) =>
  Math.round((Date.UTC(...ymd(to)) - Date.UTC(...ymd(from))) / 86_400_000);
function ymd(key: string): [number, number, number] {
  const [y, m, d] = key.split("-").map(Number);
  return [y, m - 1, d];
}
export const daysInMonth = (year: number, month1: number) => new Date(Date.UTC(year, month1, 0)).getUTCDate();

// ---- week info (macOS Calendar.current.firstWeekday / isDateInWeekend) ----

/** `firstDow`: JS weekday (0 = Sun) the week starts on; `weekend`: JS weekdays. */
export type WeekInfo = { firstDow: number; weekend: number[] };
type RawWeekInfo = { firstDay: number; weekend: number[] };

/** System region week rules (WebView2 follows Windows' display language). Falls back to Sun / Sat-Sun. */
export function systemWeekInfo(locale = typeof navigator !== "undefined" ? navigator.language : "en-US"): WeekInfo {
  try {
    const loc = new Intl.Locale(locale) as unknown as { getWeekInfo?: () => RawWeekInfo; weekInfo?: RawWeekInfo };
    const info = loc.getWeekInfo?.() ?? loc.weekInfo;
    if (info) return { firstDow: info.firstDay % 7, weekend: info.weekend.map((d) => d % 7) };
  } catch {
    /* older engines */
  }
  return { firstDow: 0, weekend: [0, 6] };
}

export function startOfWeek(d: Date, firstDow: number): Date {
  return addDays(d, -((d.getDay() - firstDow + 7) % 7));
}

// ---- store series (UsageStore.monthDailyTotals / rollingDailyTotals / trendProviders) ----

export type Day = { date: string; total: number; cost: number; coverage: CostCoverage };
const NO_COVERAGE: CostCoverage = { reported: false, estimated: false, unknown: false };
// ponytail: DayPoint carries no coverage; a reported-cost day counts as known (no "Unavailable" per day).
const KNOWN: CostCoverage = { reported: false, estimated: true, unknown: false };

function union(parts: { days: DayPoint[]; countsCost: boolean }[]): Day[] {
  const byDay = new Map<string, Day>();
  for (const { days, countsCost } of parts) {
    for (const d of days) {
      const m = byDay.get(d.date) ?? { date: d.date, total: 0, cost: 0, coverage: NO_COVERAGE };
      m.total += d.total;
      if (countsCost) {
        m.cost += d.cost;
        m.coverage = KNOWN;
      }
      byDay.set(d.date, m);
    }
  }
  return [...byDay.values()].sort((a, b) => (a.date < b.date ? -1 : a.date > b.date ? 1 : 0));
}

const recentOf = (p: ProviderSnapshot) => (p.recent_daily.length ? p.recent_daily : p.month_daily);

export const monthDailyTotals = (ps: ProviderSnapshot[]) =>
  union(ps.map((p) => ({ days: p.month_daily, countsCost: p.reports_cost })));

export function dailyTrendTotals(ps: ProviderSnapshot[], w: TrendWindow): Day[] {
  const n = WINDOW_DAYS[w];
  if (n == null) return monthDailyTotals(ps);
  return union(ps.map((p) => ({ days: recentOf(p).slice(-n), countsCost: p.reports_cost })));
}

export type ProviderSeries = { id: string; name: string; days: Day[]; reportsCost: boolean };

export function trendProviders(ps: ProviderSnapshot[], w: TrendWindow): ProviderSeries[] {
  const n = WINDOW_DAYS[w];
  return ps
    .filter((p) => p.month_daily.length || p.recent_daily.length)
    .map((p) => {
      const days = n == null ? p.month_daily : recentOf(p).slice(-n);
      return {
        id: p.provider_id,
        name: p.display_name,
        days: union([{ days, countsCost: p.reports_cost }]),
        reportsCost: p.reports_cost,
      };
    });
}

export const hasDailyTrendHistory = (ps: ProviderSnapshot[]) =>
  ps.some((p) => p.month_daily.some((d) => d.total > 0) || p.recent_daily.some((d) => d.total > 0));

// ---- trend columns and geometry (DailyTrendMetrics) ----

export const TRACK = 26;
export const SPACING = 1.5;
export const BASELINE = 1.5;
export const TICK = 1.5;

export type Column = { date: string; tokens: number; isFuture: boolean };

export function columns(series: Day[], today: string, w: TrendWindow): Column[] {
  const tokens = new Map<string, number>();
  series.forEach((d) => tokens.set(d.date, (tokens.get(d.date) ?? 0) + d.total));
  const n = WINDOW_DAYS[w];
  if (n != null) {
    return Array.from({ length: n }, (_, i) => {
      const date = shiftKey(today, i - (n - 1));
      return { date, tokens: tokens.get(date) ?? 0, isFuture: false };
    });
  }
  const [y, m, todayOfMonth] = today.split("-").map(Number);
  return Array.from({ length: daysInMonth(y, m) }, (_, i) => {
    const date = `${today.slice(0, 7)}-${pad(i + 1)}`;
    return { date, tokens: tokens.get(date) ?? 0, isFuture: i + 1 > todayOfMonth };
  });
}

export function barHeight(tokens: number, peak: number): number {
  if (peak <= 0 || tokens <= 0) return BASELINE;
  return Math.max(BASELINE, Math.min(1, tokens / peak) * TRACK);
}

export const showsPeak = (cols: { tokens: number }[]) => cols.filter((c) => c.tokens > 0).length > 1;

export function isWeekend(key: string, week: WeekInfo) {
  return week.weekend.includes(dateOf(key).getDay());
}

/** Axis label for a column: the 1st, 7-day marks, and always today (home-usage.md §3.5). */
export function axisLabel(date: string, today: string, w: TrendWindow, interval = 7, minSep = 3): string | null {
  const day = Number(date.slice(8));
  if (date === today) return String(day);
  if (w === "month") {
    if (!(day === 1 || day % interval === 0)) return null;
    if (Math.abs(day - Number(today.slice(8))) < minSep) return null;
    return String(day);
  }
  const diff = dayDiff(date, today);
  if (!(diff > 0)) return null;
  if (w === "7d") {
    if (day === 1) return diff >= minSep && (diff === 3 || Math.abs(diff - 3) >= minSep) ? "1" : null;
    return diff === 3 && diff >= minSep ? String(day) : null;
  }
  if (day === 1) return diff >= minSep ? "1" : null;
  if (diff % 7 === 0 && diff >= minSep) {
    if (diff - (day - 1) >= minSep && Math.abs(day - 1) < minSep) return null;
    const toNextFirst = daysInMonth(Number(date.slice(0, 4)), Number(date.slice(5, 7))) - day + 1;
    if (diff - toNextFirst >= minSep && toNextFirst < minSep) return null;
    return String(day);
  }
  return null;
}

// ---- stacking (DailyTrendStack) ----

export const REGISTERED_PROVIDERS = [
  "claude_code", "codex", "gemini", "antigravity", "opencode", "hermes", "cursor", "grok",
  "copilot", "kiro", "pi", "omp", "omo", "aside", "kimi_code", "devin",
];
/** Same in light and dark (data colours). */
export const PROVIDER_PALETTE = [
  "#D97857", "#5C9EF2", "#73C785", "#C78CEB", "#F2C252", "#59C7CC", "#ED82AD", "#9EA3B2",
  "#E0CC99", "#B2D161", "#996B4C", "#4C998C", "#CC5966", "#667ACC", "#F59438", "#C29461",
];
export function providerColor(id: string): string {
  const slot = Math.max(0, REGISTERED_PROVIDERS.indexOf(id));
  return PROVIDER_PALETTE[slot % PROVIDER_PALETTE.length];
}

const sumOf = (days: Day[]) => days.reduce((n, d) => n + d.total, 0);

/** Providers used in the window, largest total first (bottom of the stack); ties by id. */
export function ordered(ps: ProviderSeries[]): ProviderSeries[] {
  return ps
    .map((p) => ({ p, t: sumOf(p.days) }))
    .filter((x) => x.t > 0)
    .sort((a, b) => (a.t !== b.t ? b.t - a.t : a.p.id < b.p.id ? -1 : a.p.id > b.p.id ? 1 : 0))
    .map((x) => x.p);
}
export const isStacked = (stack: ProviderSeries[]) => stack.length > 1;

export type Segment = { id: string; tokens: number; height: number };

/** Bottom → top segments whose cumulative heights end exactly at `barHeight`. */
export function segments(date: string, stack: ProviderSeries[], height: number): Segment[] {
  const parts = stack
    .map((p) => ({ id: p.id, tokens: p.days.find((d) => d.date === date)?.total ?? 0 }))
    .filter((p) => p.tokens > 0);
  const total = parts.reduce((n, p) => n + p.tokens, 0);
  if (total <= 0) return [];
  let running = 0;
  let placed = 0;
  return parts.map((p, i) => {
    running += p.tokens;
    const top = i === parts.length - 1 ? height : (height * running) / total;
    const seg = { id: p.id, tokens: p.tokens, height: top - placed };
    placed = top;
    return seg;
  });
}

// ---- heatmap (ActivityHeatmapMetrics) ----

export const HEATMAP_WEEKS = 16;
export type Cell = { date: string; tokens: number; isOutsideRange: boolean };

/** Ledger days with the live month series laid over (max of both). */
export function heatmapDays(ledger: DailyLedger, series: Day[]): Record<string, number> {
  const days = { ...ledger.tokens_by_day };
  for (const d of series) if (d.total > 0) days[d.date] = Math.max(days[d.date] ?? 0, d.total);
  return days;
}

export function recordedSince(ledger: DailyLedger, series: Day[]): string | null {
  const keys = [ledger.covered_since, ...series.map((d) => d.date)].filter((k): k is string => !!k);
  return keys.length ? keys.reduce((a, b) => (b < a ? b : a)) : null;
}

/** 16 whole-week columns ending with today's week; each column has 7 cells from `firstDow`. */
export function recentGrid(days: Record<string, number>, since: string | null, today: string, firstDow: number): Cell[][] {
  const thisWeek = startOfWeek(dateOf(today), firstDow);
  const firstWeek = addDays(thisWeek, -7 * (HEATMAP_WEEKS - 1));
  const windowStart = keyOf(firstWeek);
  const sinceKey = since ?? today;
  const first = windowStart > sinceKey ? windowStart : sinceKey;
  const grid: Cell[][] = [];
  for (let w = 0; w < HEATMAP_WEEKS; w++) {
    grid.push(
      Array.from({ length: 7 }, (_, i) => {
        const date = keyOf(addDays(firstWeek, w * 7 + i));
        return { date, tokens: days[date] ?? 0, isOutsideRange: date < first || date > today };
      }),
    );
  }
  return grid;
}

export function level(tokens: number, peak: number): number {
  if (tokens <= 0) return 0;
  const r = tokens / peak;
  return r <= 0.25 ? 1 : r <= 0.5 ? 2 : r <= 0.75 ? 3 : 4;
}

// ---- streak (UsageLedger.currentStreak) ----

export type Streak = { days: number; includesToday: boolean };
export const STREAK_MINIMUM_DAYS = 2;

export function currentStreak(ledger: DailyLedger, today: string): Streak {
  const active = (k: string) => (ledger.tokens_by_day[k] ?? 0) > 0;
  const includesToday = active(today);
  let cursor = includesToday ? today : shiftKey(today, -1);
  let days = 0;
  while (active(cursor)) {
    days += 1;
    cursor = shiftKey(cursor, -1);
  }
  return { days, includesToday };
}

// ---- recap (RecapPeriod / UsageRecap.make) ----

export type RecapScope = "week" | "month" | "year";
export type Period = { scope: RecapScope; start: Date; end: Date };

export function period(scope: RecapScope, containing: Date, firstDow: number): Period {
  const d = new Date(containing.getFullYear(), containing.getMonth(), containing.getDate());
  if (scope === "week") {
    const start = startOfWeek(d, firstDow);
    return { scope, start, end: addDays(start, 7) };
  }
  if (scope === "month") {
    return { scope, start: new Date(d.getFullYear(), d.getMonth(), 1), end: new Date(d.getFullYear(), d.getMonth() + 1, 1) };
  }
  return { scope, start: new Date(d.getFullYear(), 0, 1), end: new Date(d.getFullYear() + 1, 0, 1) };
}

export function shifted(p: Period, offset: number, firstDow: number): Period {
  const s = p.start;
  const moved =
    p.scope === "week"
      ? addDays(s, 7 * offset)
      : p.scope === "month"
        ? new Date(s.getFullYear(), s.getMonth() + offset, 1)
        : new Date(s.getFullYear() + offset, 0, 1);
  return period(p.scope, moved, firstDow);
}

function daysBetween(start: Date, end: Date): string[] {
  const out: string[] = [];
  for (let c = start; c < end; c = addDays(c, 1)) out.push(keyOf(c));
  return out;
}

export type Bucket = { key: string; tokens: number; isCurrent: boolean; hasData: boolean };
export type RecapDex = { caught_at: string; released_at?: string | null };
export type Recap<E> = {
  period: Period;
  buckets: Bucket[];
  total: number;
  isInProgress: boolean;
  previousTotal: number | null;
  delta: number | null;
  bestDay: { key: string; tokens: number } | null;
  activeDays: number;
  countedDays: number;
  bestStreak: number;
  graduated: E[];
  canGoBack: boolean;
};

export function makeRecap<E extends RecapDex>(ledger: DailyLedger, dex: E[], p: Period, today: string, firstDow: number): Recap<E> {
  const tokens = (k: string) => ledger.tokens_by_day[k] ?? 0;
  const covers = (k: string | undefined) => !!k && !!ledger.covered_since && k >= ledger.covered_since;
  const known = (k: string) => tokens(k) > 0 || covers(k);

  const periodDays = daysBetween(p.start, p.end);
  const elapsed = periodDays.filter((k) => k <= today).map((key) => ({ key, tokens: tokens(key) }));
  let bestStreak = 0;
  let run = 0;
  for (const d of elapsed) {
    run = d.tokens > 0 ? run + 1 : 0;
    bestStreak = Math.max(bestStreak, run);
  }

  const buckets: Bucket[] =
    p.scope === "year"
      ? Array.from({ length: 12 }, (_, i) => {
          const start = new Date(p.start.getFullYear(), i, 1);
          const end = new Date(p.start.getFullYear(), i + 1, 1);
          const keys = daysBetween(start, end).filter((k) => k <= today);
          const startKey = keyOf(start);
          return {
            key: startKey,
            tokens: keys.reduce((n, k) => n + tokens(k), 0),
            isCurrent: startKey <= today && today < keyOf(end),
            hasData: keys.some(known),
          };
        })
      : periodDays.map((key) => ({ key, tokens: tokens(key), isCurrent: key === today, hasData: key <= today && known(key) }));

  const isInProgress = keyOf(p.start) <= today && today < keyOf(p.end);
  const prev = shifted(p, -1, firstDow);
  const previousDays = daysBetween(prev.start, prev.end);
  const compared = isInProgress ? previousDays.slice(0, elapsed.length) : previousDays;
  const previousTotal = covers(compared[0]) ? compared.reduce((n, k) => n + tokens(k), 0) : null;
  const total = elapsed.reduce((n, d) => n + d.tokens, 0);

  // Swift `max(by:)` returns the last maximal element on ties.
  const bestDay = elapsed.filter((d) => d.tokens > 0).reduce<{ key: string; tokens: number } | null>(
    (best, d) => (!best || d.tokens >= best.tokens ? d : best),
    null,
  );
  const startMs = p.start.getTime();
  const endMs = p.end.getTime();
  const graduated = dex
    .filter((e) => {
      const t = Date.parse(e.caught_at);
      return !e.released_at && t >= startMs && t < endMs;
    })
    .sort((a, b) => Date.parse(b.caught_at) - Date.parse(a.caught_at));

  return {
    period: p,
    buckets,
    total,
    isInProgress,
    previousTotal,
    delta: previousTotal ? (total - previousTotal) / previousTotal : null,
    bestDay,
    activeDays: elapsed.filter((d) => d.tokens > 0).length,
    countedDays: elapsed.filter((d) => known(d.key)).length,
    bestStreak,
    graduated,
    canGoBack: covers(previousDays[previousDays.length - 1]),
  };
}

/** Lit meter blocks (of 8) for a recap bucket. */
export const METER_BLOCKS = 8;
export const litBlocks = (tokens: number, peak: number) =>
  tokens > 0 ? Math.max(1, Math.round((tokens / peak) * METER_BLOCKS)) : 0;

// ---- dates in the app language (macOS templates via Intl) ----

/** `MdE`: en "Thu, 11/5", ko "11. 5. (목)", ja "11/5(木)", fr "jeu. 05/11". */
export const dayStamp = (key: string, lang: string) =>
  new Intl.DateTimeFormat(lang, { month: "numeric", day: "numeric", weekday: "short" }).format(dateOf(key));

/** `EEEE d MMM`: "Sunday, Oct 11". */
export const dayName = (key: string, lang: string) =>
  new Intl.DateTimeFormat(lang, { weekday: "long", day: "numeric", month: "short" }).format(dateOf(key));

/** `MMMM y`: "October 2026". */
export const monthName = (d: Date, lang: string) =>
  new Intl.DateTimeFormat(lang, { month: "long", year: "numeric" }).format(d);

/** One-letter weekday names (veryShortStandaloneWeekdaySymbols) rotated to `firstDow`. */
export function weekdayLetters(lang: string, firstDow: number): string[] {
  const fmt = new Intl.DateTimeFormat(lang, { weekday: "narrow" });
  // 2023-01-01 is a Sunday.
  return Array.from({ length: 7 }, (_, i) => fmt.format(new Date(2023, 0, 1 + ((firstDow + i) % 7))));
}
