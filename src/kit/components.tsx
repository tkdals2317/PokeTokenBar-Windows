// Shared visual components (ui-shell.md §5.7). Owner: foundation.
import { CSSProperties, ReactNode, useLayoutEffect, useRef, useState } from "react";
import { rarityColor, TIER_COLOR, typeColor } from "./colors";
import type { PaceTier } from "./pace";

// ---- Badge ----

/** Capsule label. `solid`: white text on tint; `tinted`: tint text on tint×0.14. */
export function Badge(props: { text: string; tint: string; variant?: "solid" | "tinted"; size?: number; shadow?: boolean }) {
  const solid = (props.variant ?? "solid") === "solid";
  const style: CSSProperties = {
    fontSize: props.size ?? 8,
    color: solid ? "#fff" : props.tint,
    background: solid ? props.tint : `color-mix(in srgb, ${props.tint} 14%, transparent)`,
    textShadow: props.shadow ? "0 0.5px 0.5px rgba(0,0,0,0.35)" : undefined,
  };
  return (
    <span className="kit-badge" style={style}>
      {props.text}
    </span>
  );
}

/** Type pills; `label` maps a type id to its localized name. */
export function TypeBadges({ types, label }: { types: string[]; label: (t: string) => string }) {
  return (
    <span className="kit-row" style={{ gap: 5 }}>
      {types.map((t) => (
        <Badge key={t} text={label(t).toUpperCase()} tint={typeColor(t)} shadow />
      ))}
    </span>
  );
}

export const RarityBadge = ({ rarity, text }: { rarity: string; text: string }) => (
  <Badge text={text} tint={rarityColor(rarity)} />
);

// ---- CapsuleTabBar ----

export type CapsuleTab = { id: string; title: string; dot?: string; tooltip?: string };

/** Provider / Claude account tabs: horizontally scrolling capsules. */
export function CapsuleTabBar(props: { tabs: CapsuleTab[]; selected: string; onSelect: (id: string) => void }) {
  return (
    <div className="kit-capsules" role="tablist">
      {props.tabs.map((t) => {
        const on = t.id === props.selected;
        const button = (
          <button
            key={t.id}
            role="tab"
            aria-selected={on}
            className={on ? "kit-capsule on" : "kit-capsule"}
            onClick={() => props.onSelect(t.id)}
          >
            {t.dot && <span className="kit-dot" style={{ background: t.dot }} />}
            {t.title}
          </button>
        );
        return t.tooltip ? (
          <Tooltip key={t.id} text={t.tooltip}>
            {button}
          </Tooltip>
        ) : (
          button
        );
      })}
    </div>
  );
}

// ---- LimitProgressBar ----

/**
 * 6 px track filled to `percent` (0…100, already in used/remaining display terms) in the tier colour.
 * `marker` (0…1, same display terms) draws the 2.5×10 pace marker.
 */
export function LimitProgressBar(props: { percent: number; tier: PaceTier; marker?: number | null }) {
  const p = Math.min(100, Math.max(0, props.percent));
  return (
    <div className="kit-limit-bar">
      <div className="kit-limit-fill" style={{ width: `${p}%`, background: TIER_COLOR[props.tier] }} />
      {props.marker != null && (
        <div className="kit-limit-marker" style={{ left: `calc((100% - 2.5px) * ${Math.min(1, Math.max(0, props.marker))})` }} />
      )}
    </div>
  );
}

// ---- Tooltip ----

const TOOLTIP_DELAY_MS = 600;

/** Hover tooltip after 600 ms (macOS NSInitialToolTipDelay), clamped inside the window. */
export function Tooltip({ text, children }: { text: string; children: ReactNode }) {
  const [anchor, setAnchor] = useState<DOMRect | null>(null);
  const timer = useRef<number>(undefined);
  const bubble = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState<{ left: number; top: number } | null>(null);

  useLayoutEffect(() => {
    const b = bubble.current;
    if (!anchor || !b) return setPos(null);
    const w = b.offsetWidth;
    const h = b.offsetHeight;
    const left = Math.min(window.innerWidth - w - 4, Math.max(4, anchor.left + anchor.width / 2 - w / 2));
    const below = anchor.bottom + 4 + h <= window.innerHeight;
    setPos({ left, top: below ? anchor.bottom + 4 : anchor.top - h - 4 });
  }, [anchor]);

  const hide = () => {
    window.clearTimeout(timer.current);
    setAnchor(null);
  };
  return (
    <span
      className="kit-tooltip-host"
      onMouseEnter={(e) => {
        const target = e.currentTarget;
        timer.current = window.setTimeout(() => setAnchor(target.getBoundingClientRect()), TOOLTIP_DELAY_MS);
      }}
      onMouseLeave={hide}
      onMouseDown={hide}
    >
      {children}
      {anchor && (
        <div ref={bubble} className="kit-tooltip" role="tooltip" style={pos ?? { visibility: "hidden" }}>
          {text}
        </div>
      )}
    </span>
  );
}

// ---- Icons (SF Symbols → inline SVG) ----

const paths = {
  refresh: "M20 12a8 8 0 1 1-2.34-5.66M20 4v4h-4",
  warning: "M12 3 2 20h20L12 3zm0 6v5m0 3v.5",
  eye: "M2 12s3.5-6.5 10-6.5S22 12 22 12s-3.5 6.5-10 6.5S2 12 2 12zm10 3a3 3 0 1 0 0-6 3 3 0 0 0 0 6z",
  eyeSlash: "M2 12s3.5-6.5 10-6.5S22 12 22 12s-3.5 6.5-10 6.5S2 12 2 12zM4 4l16 16",
  gear: "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zm7.4-3a7.4 7.4 0 0 0-.1-1.2l2-1.6-2-3.4-2.4 1a7.5 7.5 0 0 0-2-1.2L14.5 3h-5l-.4 2.6a7.5 7.5 0 0 0-2 1.2l-2.4-1-2 3.4 2 1.6a7.4 7.4 0 0 0 0 2.4l-2 1.6 2 3.4 2.4-1a7.5 7.5 0 0 0 2 1.2l.4 2.6h5l.4-2.6a7.5 7.5 0 0 0 2-1.2l2.4 1 2-3.4-2-1.6c.1-.4.1-.8.1-1.2z",
  power: "M12 3v8M6.3 6.3a8 8 0 1 0 11.4 0",
  chevronLeft: "M15 5l-7 7 7 7",
  barChart: "M4 20h16M7 16v-5m5 5V6m5 10v-8",
};
export type IconName = keyof typeof paths;

export const Icon = ({ name, size = 14 }: { name: IconName; size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" aria-hidden>
    <path d={paths[name]} />
  </svg>
);

/** Borderless icon button with tooltip + accessible label (macOS `.borderless`). */
export const IconButton = (p: { icon: IconName; label: string; onClick: () => void; color?: string }) => (
  <Tooltip text={p.label}>
    <button className="kit-icon-button" aria-label={p.label} onClick={p.onClick} style={{ color: p.color }}>
      <Icon name={p.icon} />
    </button>
  </Tooltip>
);

/** macOS segmented picker: equal-width segments, 13 px labels. */
export function Segmented<T extends string>(props: { options: [T, string][]; value: T; onChange: (v: T) => void }) {
  return (
    <div className="kit-segmented" role="tablist">
      {props.options.map(([v, label]) => (
        <button key={v} role="tab" aria-selected={v === props.value} className={v === props.value ? "on" : ""} onClick={() => props.onChange(v)}>
          {label}
        </button>
      ))}
    </div>
  );
}
