// PaceTier (ui-shell.md §3.3, limits.md §3.12). Owner: foundation.

export type PaceTier = "wayUnder" | "under" | "onPace" | "slightlyOver" | "over" | "wayOver";
const ORDER: PaceTier[] = ["wayUnder", "under", "onPace", "slightlyOver", "over", "wayOver"];
export const MINIMUM_PACE = 0.1;

/** `pace` is the elapsed fraction of the window (0…1); `u` the used percent. */
export function tier(u: number, pace: number | null, crit: number): PaceTier | null {
  if (pace == null || pace < MINIMUM_PACE) return null;
  if (u >= crit) return "wayOver";
  const d = Math.round(u - pace * 100);
  if (d < -25) return "wayUnder";
  if (d < -10) return "under";
  if (d < 5) return "onPace";
  if (d < 15) return "slightlyOver";
  if (d < 30) return "over";
  return "wayOver";
}

export function gauge(u: number, pace: number | null, warn: number, crit: number): PaceTier {
  return tier(u, pace, crit) ?? (u >= crit ? "wayOver" : u >= warn ? "over" : "onPace");
}

export const isCalm = (t: PaceTier) => ORDER.indexOf(t) <= ORDER.indexOf("onPace");

/** Elapsed fraction of a window, `null` outside 0…1 (no clamp). Times in ms. */
export function paceFraction(resetsAt: number, spanMs: number, now = Date.now()): number | null {
  if (spanMs <= 0) return null;
  const f = (spanMs - (resetsAt - now)) / spanMs;
  return f >= 0 && f <= 1 ? f : null;
}
