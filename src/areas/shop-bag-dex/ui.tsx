// Small shared pieces for Shop, Bag and Collection: icons (SF Symbols → SVG), item icon, stepper,
// menu, rarity labels. Owner: shop-bag-dex area.
import { CSSProperties, ReactNode, useEffect, useLayoutEffect, useRef, useState } from "react";
import { compact } from "../../kit/format";
import type { Companion, Rarity } from "../../shared";
import { nameOf, resolveName } from "../../shared";
import type { Lang } from "../../kit/i18n";
import { formDisplayName, formLabel, formNameResource } from "../companion/forms";
import { ITEM_EMOJI, ITEM_SPRITE, ItemKey, itemSpriteUrl } from "./economy";
import type { sbdStrings } from "./strings";

export type S = typeof sbdStrings.en;

const paths = {
  search: "M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13zM15.5 15.5 21 21",
  sort: "M7 20V4M3.5 7.5 7 4l3.5 3.5M17 4v16m-3.5-3.5L17 20l3.5-3.5",
  check: "M5 12.5l4.5 4.5L19 7",
  chevronLeft: "M15 5l-7 7 7 7",
  chevronRight: "M9 5l7 7-7 7",
  chevronUp: "M6 15l6-6 6 6",
  chevronDown: "M6 9l6 6 6-6",
  star: "M12 3.5l2.6 5.4 5.9.8-4.3 4.1 1 5.9L12 16.9l-5.2 2.8 1-5.9L3.5 9.7l5.9-.8z",
  follow: "M4 12a8 8 0 0 1 13.7-5.6M20 12a8 8 0 0 1-13.7 5.6M17.5 3v3.6h-3.6M6.5 21v-3.6h3.6",
};
export type IconName = keyof typeof paths | "xmarkFill" | "sparkle" | "sparkles" | "starFill" | "sealCheck";

/** Inline SVG glyphs standing in for the SF Symbols the macOS views use. */
export function Glyph({ name, size = 13, weight = 2, style }: { name: IconName; size?: number; weight?: number; style?: CSSProperties }) {
  const svg = (children: ReactNode, fill = false) => (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden style={style}
      fill={fill ? "currentColor" : "none"} stroke={fill ? "none" : "currentColor"} strokeWidth={weight} strokeLinecap="round" strokeLinejoin="round">
      {children}
    </svg>
  );
  switch (name) {
    case "xmarkFill":
      return svg(<path fillRule="evenodd" d="M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20zM8.3 8.3a1 1 0 0 1 1.4 0L12 10.6l2.3-2.3a1 1 0 1 1 1.4 1.4L13.4 12l2.3 2.3a1 1 0 0 1-1.4 1.4L12 13.4l-2.3 2.3a1 1 0 0 1-1.4-1.4l2.3-2.3-2.3-2.3a1 1 0 0 1 0-1.4z" />, true);
    case "sparkle":
      return svg(<path d="M12 3c.6 4.6 2.4 6.4 7 7-4.6.6-6.4 2.4-7 7-.6-4.6-2.4-6.4-7-7 4.6-.6 6.4-2.4 7-7z" />);
    case "sparkles":
      return svg(<path d="M10 4c.6 4.4 2.2 6 6.6 6.6C12.2 11.2 10.6 12.8 10 17.2 9.4 12.8 7.8 11.2 3.4 10.6 7.8 10 9.4 8.4 10 4zM18 13c.3 2.1 1 2.8 3 3-2 .3-2.7 1-3 3-.3-2-1-2.7-3-3 2-.2 2.7-.9 3-3zM18 2.5c.2 1.4.7 1.9 2 2-1.3.2-1.8.7-2 2-.2-1.3-.7-1.8-2-2 1.3-.1 1.8-.6 2-2z" />, true);
    case "starFill":
      return svg(<path d={paths.star} />, true);
    case "sealCheck":
      return svg(<path fillRule="evenodd" d="M12 1.8l2.4 1.8 3-.2.9 2.9 2.5 1.7-1 2.8 1 2.8-2.5 1.7-.9 2.9-3-.2L12 20.2l-2.4-1.8-3 .2-.9-2.9-2.5-1.7 1-2.8-1-2.8 2.5-1.7.9-2.9 3 .2zm4.2 7.3a1 1 0 0 0-1.4-1.4L10.8 11.7 9.2 10.1a1 1 0 1 0-1.4 1.4l2.3 2.3a1 1 0 0 0 1.4 0z" />, true);
    default:
      return svg(<path d={paths[name]} />);
  }
}

export const rarityLabel = (t: S, r: Rarity) =>
  ({ common: t.rarityCommon, uncommon: t.rarityUncommon, rare: t.rarityRare, legendary: t.rarityLegendary })[r];

export const itemName = (t: S, i: ItemKey) => ({ rareCandy: t.itemRareCandy, mint: t.itemMint, shinyCharm: t.itemShinyCharm })[i];

export const itemDescription = (t: S, i: ItemKey, candyXP: number) =>
  ({ rareCandy: t.descRareCandy(compact(candyXP)), mint: t.descMint, shinyCharm: t.descShinyCharm })[i];

export const eggName = (t: S, tier: Rarity | null) =>
  tier === "uncommon" ? t.eggNameUncommon : tier === "rare" ? t.eggNameRare : t.eggNamePlain;

/** The raised Pokémon's display name with its form ("Rotom [Heat Rotom]"), or "Token Egg". */
export function activeName(c: Companion, lang: Lang, normalForm: string) {
  const a = c.active;
  const id = c.current_id;
  if (!a || id == null) return "Token Egg";
  const res = formNameResource(id, a.form);
  const localized = res != null ? resolveName(c.form_labels[res], lang) : null;
  return formDisplayName(nameOf(a.line.names, id, lang), id, a.form, formLabel(id, a.form, localized, normalForm));
}

/** Item sprite (nearest-neighbour, aspect-fit) with the emoji while loading or on failure. */
export function ItemIcon({ item, size = 30 }: { item: ItemKey; size?: number }) {
  const sprite = ITEM_SPRITE[item];
  const [ok, setOk] = useState<boolean | null>(null);
  return (
    <span className="sbd-item-icon" style={{ width: size, height: size }}>
      {sprite && ok !== false && (
        <img src={itemSpriteUrl(sprite)} alt="" draggable={false} onLoad={() => setOk(true)} onError={() => setOk(false)} style={{ opacity: ok ? 1 : 0 }} />
      )}
      {ok !== true && <span className="sbd-item-emoji" style={{ fontSize: size * 0.8 }}>{ITEM_EMOJI[item]}</span>}
    </span>
  );
}

/** `×n` + macOS-style vertical stepper, clamped to 1…max. */
export function Stepper({ value, max, onChange, label }: { value: number; max: number; onChange: (v: number) => void; label: string }) {
  return (
    <span className="sbd-stepper-row">
      <span className="sbd-qty num">×{value}</span>
      <span className="sbd-stepper" role="spinbutton" aria-label={label} aria-valuenow={value} aria-valuemin={1} aria-valuemax={max}>
        <button aria-label="+" disabled={value >= max} onClick={() => onChange(Math.min(max, value + 1))}>
          <Glyph name="chevronUp" size={10} weight={2.6} />
        </button>
        <button aria-label="−" disabled={value <= 1} onClick={() => onChange(Math.max(1, value - 1))}>
          <Glyph name="chevronDown" size={10} weight={2.6} />
        </button>
      </span>
    </span>
  );
}

export type MenuItem = { label: string; checked?: boolean; icon?: IconName; onSelect: () => void };

/** Absolute-positioned menu (macOS `Menu` / `.contextMenu`): closes on outside click or Escape. */
export function PopupMenu({ items, at, onClose }: { items: MenuItem[]; at: { x: number; y: number }; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [pos, setPos] = useState(at);
  const close = useRef(onClose);
  close.current = onClose;
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    setPos({
      x: Math.max(4, Math.min(at.x, window.innerWidth - el.offsetWidth - 4)),
      y: Math.max(4, Math.min(at.y, window.innerHeight - el.offsetHeight - 4)),
    });
  }, [at.x, at.y]);
  useEffect(() => {
    const down = (e: MouseEvent) => !ref.current?.contains(e.target as Node) && close.current();
    const key = (e: KeyboardEvent) => e.key === "Escape" && close.current();
    const blur = () => close.current();
    window.addEventListener("mousedown", down, true);
    window.addEventListener("keydown", key);
    window.addEventListener("blur", blur);
    return () => {
      window.removeEventListener("mousedown", down, true);
      window.removeEventListener("keydown", key);
      window.removeEventListener("blur", blur);
    };
  }, []);
  return (
    <div ref={ref} className="sbd-menu" role="menu" style={{ left: pos.x, top: pos.y }}>
      {items.map((it) => (
        <button key={it.label} role="menuitemcheckbox" aria-checked={!!it.checked} onClick={() => { it.onSelect(); onClose(); }}>
          <span className="sbd-menu-check">{it.checked && <Glyph name="check" size={11} weight={2.6} />}</span>
          {it.icon && <Glyph name={it.icon} size={12} />}
          <span>{it.label}</span>
        </button>
      ))}
    </div>
  );
}
