// Pure rules behind the floating pet (pet-settings-misc.md §3.1, FloatingPetPanel.swift). Owner: pet-settings-misc.
import { grouped, percent } from "../../kit/format";
import { petStrings } from "./strings";

type L = (typeof petStrings)["en"];

export const PRAYERS_PER_QUIP = 30;
const QUIP_COUNT = 11;
export const SULK_DAYS_FROM_HOURS = 48;

/** Quip index for `n` prayers: one step per 30, capped at the last quip. */
export const prayerQuipIndex = (n: number) => (n > 0 ? Math.min(Math.floor(n / PRAYERS_PER_QUIP), QUIP_COUNT - 1) : null);

/** "{n} prayers · {quip}" ("1 prayer" singular); empty for none. */
export function prayerHoverLine(l: L, n: number): string {
  const i = prayerQuipIndex(n);
  if (i == null) return "";
  const quip = l[`prayerQuip${i}` as keyof L] as string;
  return `${n === 1 ? l.prayerOne : l.prayerMany(n)} · ${quip}`;
}

export type HoverInput = {
  todayTokens: number;
  /** Highest limit utilization 0…100, or null when no limits are known. */
  limitUtilization: number | null;
  mode: "used" | "remaining";
  /** Seconds idle while sulking (0 / null = not sulking). */
  sulkIdle: number | null;
  eventLine: string | null;
  /** Prayers over the egg (0 when the subject is a Pokémon). */
  prayers: number;
};

/** `FloatingPetView.hoverTooltip`. */
export function hoverText(l: L, h: HoverInput): string {
  const usage = grouped(h.todayTokens);
  let base: string;
  if (h.limitUtilization != null) {
    const shown = h.mode === "remaining" ? l.percentRemaining(percent(Math.max(0, 100 - h.limitUtilization))) : percent(h.limitUtilization);
    base = l.floatingPetHoverWithLimit(usage, shown);
  } else {
    base = l.floatingPetHoverTokensOnly(usage);
  }
  if (h.sulkIdle && h.sulkIdle > 0) {
    const hours = Math.floor(h.sulkIdle / 3600);
    base += " · " + (hours < SULK_DAYS_FROM_HOURS ? l.floatingPetHoverSulkHours(hours) : l.floatingPetHoverSulkDays(Math.floor(hours / 24)));
  }
  if (h.eventLine) base += "\n" + h.eventLine;
  if (h.prayers > 0) base += "\n" + prayerHoverLine(l, h.prayers);
  return base;
}

/** Three horizontal direction reversals inside 1.2 s, each at least `minDistance` (screen x). */
export class PettingDetector {
  static readonly requiredReversals = 3;
  static readonly window = 1.2;
  private last: number | null = null;
  private extreme = 0;
  private dir = 0;
  private reversals: number[] = [];

  constructor(public minDistance: number) {}

  reset() {
    this.last = null;
    this.dir = 0;
    this.reversals = [];
  }

  /** `t` in seconds. True when this move completes a stroke. */
  feed(x: number, t: number): boolean {
    if (this.last == null || t - this.last > PettingDetector.window) {
      this.reset();
      this.last = t;
      this.extreme = x;
      return false;
    }
    this.last = t;
    this.reversals = this.reversals.filter((r) => t - r <= PettingDetector.window);
    if (this.dir === 0) {
      if (Math.abs(x - this.extreme) >= this.minDistance) {
        this.dir = Math.sign(x - this.extreme);
        this.extreme = x;
      }
      return false;
    }
    if ((x - this.extreme) * this.dir > 0) {
      this.extreme = x;
      return false;
    }
    if ((this.extreme - x) * this.dir >= this.minDistance) {
      this.dir = -this.dir;
      this.extreme = x;
      this.reversals.push(t);
      return this.reversals.length >= PettingDetector.requiredReversals;
    }
    return false;
  }
}

/** Stroke distance: max(10, size × 0.15). */
export const strokeDistance = (petSize: number) => Math.max(10, petSize * 0.15);

const ELLIPSIS = "…";

/** Longest prefix (by binary search) for which `ok(prefix.trim() + "…" + suffix)` holds. */
function longestFit(chars: string[], ok: (n: number) => boolean): number {
  let low = 0;
  let high = chars.length;
  while (high - low > 1) {
    const mid = (low + high) >> 1;
    if (ok(mid)) low = mid;
    else high = mid;
  }
  return low;
}

/** "<label> · <status> [+N]" or the plain title, cut with an ellipsis to fit one line. */
export function agentTitle(label: string | null, unlabeled: string, status: string, count: string | null, fits: (t: string) => boolean): string {
  const lead = label ?? unlabeled;
  let suffix = label == null ? "" : " · " + status;
  if (count) suffix += " " + count;
  if (fits(lead + suffix)) return lead + suffix;
  const chars = Array.from(lead);
  const cut = (n: number) => chars.slice(0, n).join("").trim() + ELLIPSIS + suffix;
  const low = longestFit(chars, (n) => fits(cut(n)));
  // Prefer a word boundary; folder names and CJK have none.
  const space = chars.slice(0, low).lastIndexOf(" ");
  if (space >= 0 && space >= low / 2) return cut(space);
  return cut(low);
}

/** Cuts `text` with an ellipsis until it wraps to at most two lines (`fits`). */
export function fittedBody(text: string, fits: (t: string) => boolean): string {
  if (fits(text)) return text;
  const chars = Array.from(text);
  const cut = (n: number) => chars.slice(0, n).join("").trim() + ELLIPSIS;
  return cut(longestFit(chars, (n) => fits(cut(n))));
}

export type AgentNote = { label: string | null; message: string | null };
export type AgentSummary = { done: AgentNote | null; waiting: AgentNote | null; other_waiting: number; working: boolean };
export type LimitAlert = { critical: boolean; window: string; utilization: number; account: string | null };

export type Bubble =
  | { kind: "limit"; alert: LimitAlert }
  | { kind: "done"; note: AgentNote }
  | { kind: "waiting"; note: AgentNote; others: number };

/** A limit alert outranks agents; among agents a fresh `done` outranks waiting. */
export function pickBubble(alert: LimitAlert | null, agent: AgentSummary | null): Bubble | null {
  if (alert) return { kind: "limit", alert };
  if (agent?.done) return { kind: "done", note: agent.done };
  if (agent?.waiting) return { kind: "waiting", note: agent.waiting, others: agent.other_waiting };
  return null;
}

export type BubbleContent = { title: string; body: string; critical: boolean };

export function bubbleContent(
  b: Bubble,
  l: L,
  fitsTitle: (t: string) => boolean,
  fitsBody: (title: string, body: string) => boolean,
): BubbleContent {
  if (b.kind === "limit") {
    const title = b.alert.critical ? l.notifCritical : l.notifWarning;
    const plain = l.notifBody(b.alert.window, percent(b.alert.utilization));
    const full = b.alert.account ? `${plain} · ${b.alert.account}` : plain;
    // The account comes last: a bubble cut short loses it, not the percentage.
    const body = !b.alert.account || fitsBody(title, full) ? full : plain;
    return { title, body, critical: b.alert.critical };
  }
  const title =
    b.kind === "waiting"
      ? agentTitle(b.note.label, l.agentWaitingTitle, l.agentWaitingStatus, b.others > 0 ? `+${b.others}` : null, fitsTitle)
      : agentTitle(b.note.label, l.agentDoneTitle, l.agentDoneStatus, null, fitsTitle);
  const message = b.note.message ?? (b.kind === "waiting" ? l.agentWaitingDefaultBody : l.agentDoneDefaultBody);
  return { title, body: fittedBody(message, (t) => fitsBody(title, t)), critical: false };
}

/** Decimates GIF frames to at most one per `floor` s while keeping the loop length (SpriteAnimation.swift). */
export function capFrameRate<T>(frames: { image: T; delay: number }[], floor: number): { image: T; delay: number }[] {
  if (floor <= 0 || frames.length <= 1) return frames;
  const out: { image: T; delay: number }[] = [];
  let held: T | null = null;
  let acc = 0;
  for (const f of frames) {
    if (held == null) held = f.image;
    acc += f.delay;
    if (acc + 1e-9 >= floor) {
      out.push({ image: held, delay: acc });
      held = null;
      acc = 0;
    }
  }
  if (held != null && acc > 0) {
    if (out.length === 0) out.push({ image: held, delay: acc });
    else out[out.length - 1].delay += acc;
  }
  return out;
}

/** Animation quality → frame floor (s); Battery Saver forces ≥ 0.4. */
export function frameFloor(q: "powerSaver" | "balanced" | "smooth", lowPower: boolean): number {
  const f = q === "smooth" ? 0.1 : q === "balanced" ? 0.2 : 0.4;
  return lowPower ? Math.max(f, 0.4) : f;
}

/** Bubble/hearts panel: (max(size, 180), size + 72), else the bare sprite. */
export const panelSize = (size: number, showingBubble: boolean) =>
  showingBubble ? { w: Math.max(size, 180), h: size + 72 } : { w: size, h: size };

/** Prayer emoji edge: clamp(size × 0.25, 14, 32). */
export const prayerEmojiSize = (size: number) => Math.min(32, Math.max(14, size * 0.25));

/** Heart bubble top inside the sprite box: its tail overlaps the visible top by 3 pt. */
export const heartTop = (visibleTop: number) => Math.round(visibleTop + 3 - 32);
