// Literal port of macOS TokenFormatter (ui-shell.md §3.2, home-usage.md §3.1). Owner: foundation.
// Rust has the same rules in src-tauri/src/format.rs; keep both in sync.

export type CostCoverage = { reported: boolean; estimated: boolean; unknown: boolean };

const trim = (s: string) => (s.includes(".") ? s.replace(/0+$/, "").replace(/\.$/, "") : s);

/** 987, 12.3K, 1K, 190.6M, 1.24B. 999_950 promotes to the next unit so "1000.0K" never shows. */
export function compact(value: number): string {
  const v = Math.abs(value);
  const sign = value < 0 ? "-" : "";
  if (v < 1_000) return String(Math.trunc(value));
  if (v < 999_950) return `${sign}${trim((v / 1e3).toFixed(1))}K`;
  if (v < 999_950_000) return `${sign}${trim((v / 1e6).toFixed(1))}M`;
  return `${sign}${trim((v / 1e9).toFixed(2))}B`;
}

/** Thousands grouping in the system region locale (not the app language). */
export const grouped = (v: number) => new Intl.NumberFormat(undefined).format(Math.trunc(v));

/** $2.76 */
export const cost = (usd: number) => `$${usd.toFixed(2)}`;

/** $9.5, $311, $1.2K. Thresholds are judged on the rounded text. */
export function costCompact(usd: number): string {
  const tenths = usd.toFixed(1);
  if (Number(tenths) < 100) return `$${tenths}`;
  const whole = usd.toFixed(0);
  if (Number(whole) < 10_000) return `$${whole}`;
  return `$${(usd / 1000).toFixed(1)}K`;
}

/** 80%, 88.3% */
export const percent = (v: number) => `${v.toFixed(1).replace(/\.0$/, "")}%`;

/** `UsageCost.text`: unknown-only coverage → `$—` (compact) or the localized "Unavailable". */
export function costText(usd: number, c: CostCoverage, compactForm: boolean, unavailable: string): string {
  if (c.unknown && !c.reported && !c.estimated) return compactForm ? "$—" : unavailable;
  return compactForm ? costCompact(usd) : cost(usd);
}

const UNITS = [
  ["days", 86400],
  ["hours", 3600],
  ["minutes", 60],
  ["seconds", 1],
] as const;

type DurationFormatCtor = new (lang: string, o: object) => { format(d: object): string };

/** "2 min, 5 sec": the two largest non-zero units, like SwiftUI `Text(date, style: .relative)`. */
export function relativeDuration(totalSeconds: number, lang: string): string {
  let rest = Math.max(0, Math.floor(totalSeconds));
  const parts: Record<string, number> = {};
  for (const [unit, size] of UNITS) {
    const n = Math.floor(rest / size);
    rest -= n * size;
    if (n > 0 && Object.keys(parts).length < 2) parts[unit] = n;
  }
  if (Object.keys(parts).length === 0) parts.seconds = 0;
  // Intl.DurationFormat ships in WebView2 (Chromium 129+); the fallback is plain English.
  const DF = (Intl as unknown as { DurationFormat?: DurationFormatCtor }).DurationFormat;
  if (DF) return new DF(lang, { style: "short" }).format(parts);
  const short: Record<string, string> = { days: "days", hours: "hr", minutes: "min", seconds: "sec" };
  return Object.entries(parts)
    .map(([u, n]) => `${n} ${short[u]}`)
    .join(", ");
}
