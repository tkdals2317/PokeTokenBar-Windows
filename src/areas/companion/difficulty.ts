// Difficulty slider mapping (companion.md §3.1, Model:142-178). Owner: companion area.
// Rust: src-tauri/src/companion/balance.rs. Shop prices use the same scale.

export const clampDifficulty = (d: number) => (Number.isFinite(d) ? Math.min(2, Math.max(0.1, d)) : 1);

/** Two significant figures; 0.1 for v ≤ 0. */
export function snap(v: number) {
  if (v <= 0) return 0.1;
  const m = Math.pow(10, Math.floor(Math.log10(v)) - 1);
  return Math.round(v / m) * m;
}

/** value → slider position (log scale, 0.1…2.0 ↦ 0…1). */
export const difficultyPosition = (v: number) => Math.log(clampDifficulty(v) / 0.1) / Math.log(20);

/** position → value, snapping to 1.0 within ±0.01 of its position. */
export function difficultyAtPosition(position: number) {
  const p = Math.min(1, Math.max(0, position));
  if (Math.abs(p - difficultyPosition(1)) < 0.01) return 1;
  return snap(0.1 * Math.pow(20, p));
}

/** "10%" … "200%": 0 decimals from 10%, 1 from 1%, else 2. */
export function difficultyValue(v: number) {
  const pct = v * 100;
  return pct >= 10 ? `${pct.toFixed(0)}%` : pct >= 1 ? `${pct.toFixed(1)}%` : `${pct.toFixed(2)}%`;
}
