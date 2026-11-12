// Colour maps (ui-shell.md §3.3, §5.7). Owner: foundation. Values resolve light/dark via tokens.css.
import type { PaceTier } from "./pace";

export const TIER_COLOR: Record<PaceTier, string> = {
  wayUnder: "var(--blue)",
  under: "var(--teal)",
  onPace: "var(--green)",
  slightlyOver: "var(--yellow)",
  over: "var(--orange)",
  wayOver: "var(--red)",
};

/** Popover % text: yellow is unreadable on light backgrounds, so it uses the primary text colour. */
export const tierTextColor = (t: PaceTier) => (t === "slightlyOver" ? "var(--text)" : TIER_COLOR[t]);

/** uncommon green, rare blue, legendary orange, common/unknown grey. */
export const rarityColor = (r: string | null | undefined) =>
  ({ uncommon: "var(--green)", rare: "var(--blue)", legendary: "var(--orange)" })[r ?? ""] ?? "var(--gray)";

export const TYPE_COLOR: Record<string, string> = {
  normal: "#AAAA99", fire: "#FF4422", water: "#3399FF", electric: "#FFCC33", grass: "#77CC55",
  ice: "#66CCFF", fighting: "#BB5544", poison: "#AA5599", ground: "#DDBB55", flying: "#8899FF",
  psychic: "#FF5599", bug: "#AABB22", rock: "#BBAA66", ghost: "#6666BB", dragon: "#7766EE",
  dark: "#775544", steel: "#AAAABB", fairy: "#EE99EE",
};

/** Unknown types (stellar, shadow, unknown) are grey. */
export const typeColor = (t: string) => TYPE_COLOR[t] ?? "var(--gray)";
