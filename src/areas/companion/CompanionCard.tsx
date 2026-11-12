// Companion header on Home (companion.md §5, CompanionView.swift CompanionHeader). Owner: companion area.
// Home-usage renders it as a slot with the full TabProps.
import { CSSProperties, useEffect, useLayoutEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./companion.css";
import { Badge, Tooltip, TypeBadges } from "../../kit/components";
import { rarityColor } from "../../kit/colors";
import { compact } from "../../kit/format";
import { useLang, useStrings } from "../../kit/i18n";
import type { Companion, Rarity, TabProps } from "../../shared";
import { nameOf, resolveName } from "../../shared";
import { EGG_SPRITE, formDisplayName, formLabel, formNameResource, identifier, isDefaultForm, spriteChain } from "./forms";
import { companionStrings, natureLabel } from "./strings";

type S = typeof companionStrings.en;

export const rarityLabel = (t: S, r: Rarity) =>
  ({ common: t.rarityCommon, uncommon: t.rarityUncommon, rare: t.rarityRare, legendary: t.rarityLegendary })[r];

/** "X / Y · Z%": used is clamped to total, the percent floors. */
export function progressDetail(used: number, total: number) {
  const u = Math.min(used, total);
  const pct = total > 0 ? Math.floor((u * 100) / total) : 0;
  return `${compact(u)} / ${compact(total)} · ${pct}%`;
}

// ---- localized PokéAPI resource names (types, form names) ----

const nameCache = new Map<string, Promise<Record<string, string>>>();

/** `{lang: name}` of a type / ability / move / pokemon-form, through the Rust 30-day cache. */
export function useResourceNames(kind: string, name: string | null) {
  const [state, setState] = useState<{ key: string; names: Record<string, string> | null; failed: boolean } | null>(null);
  const key = name == null ? "" : `${kind}/${name}`;
  useEffect(() => {
    if (name == null) return;
    let p = nameCache.get(key);
    if (!p) {
      p = invoke<Record<string, string>>("pokemon_names", { kind, name });
      p.catch(() => nameCache.delete(key)); // retry on next mount
      nameCache.set(key, p);
    }
    let live = true;
    p.then(
      (names) => live && setState({ key, names, failed: false }),
      () => live && setState({ key, names: null, failed: true }),
    );
    return () => {
      live = false;
    };
  }, [key, kind, name]);
  return state?.key === key ? state : null;
}

function TypeBadge({ type }: { type: string }) {
  const lang = useLang();
  const r = useResourceNames("type", type);
  const label = r ? (r.names ? resolveName(r.names, lang) ?? identifier(type) : identifier(type)) : "…";
  return <TypeBadges types={[type]} label={() => label} />;
}

// ---- sprite ----

/**
 * One sprite with the macOS fallbacks: shiny → plain, back → front, animated → static, then 🥚.
 * A failed GIF keeps the static image and retries every 2 s, doubling to 30 s.
 */
export function Sprite(props: { id: number | null; size: number; shiny?: boolean; back?: boolean; form?: string | null; animated?: boolean; bob?: boolean }) {
  const { id, size } = props;
  const chain = id == null ? [EGG_SPRITE] : spriteChain(id, { shiny: props.shiny, back: props.back, form: props.form, animated: props.animated ?? true });
  const key = chain.join("|");
  const [state, setState] = useState({ key, idx: 0, retry: 2000 });
  const s = state.key === key ? state : { key, idx: 0, retry: 2000 };
  const src = chain[s.idx];
  const isStatic = src?.endsWith(".png");
  const hasGif = chain[0].endsWith(".gif");
  useEffect(() => {
    // Back on a static image while a GIF exists in the chain: try the GIF again later.
    if (!isStatic || !hasGif || s.idx === 0) return;
    const t = window.setTimeout(() => setState({ key, idx: 0, retry: Math.min(30000, s.retry * 2) }), s.retry);
    return () => window.clearTimeout(t);
  }, [key, s.idx, s.retry, isStatic, hasGif]);
  const style: CSSProperties = { width: size, height: size };
  if (!src) {
    return (
      <span className="cc-sprite" style={style}>
        <span className="cc-glyph" style={{ fontSize: size * 0.62 }}>🥚</span>
      </span>
    );
  }
  const onError = () => setState({ key, idx: s.idx + 1, retry: s.retry });
  const bob = props.bob && isStatic;
  if (id == null) {
    // The egg PNG has a 96 px canvas around 28×30 of content: scale the canvas so the content fills the box.
    const canvas = (size * 96) / 30;
    return (
      <span className={bob ? "cc-sprite bob" : "cc-sprite"} style={style}>
        <span className="cc-egg-crop" style={{ width: canvas, height: canvas }}>
          <img src={src} alt="" draggable={false} onError={onError} />
        </span>
      </span>
    );
  }
  return (
    <span className={bob ? "cc-sprite bob" : "cc-sprite"} style={style}>
      <img src={src} alt="" draggable={false} onError={onError} />
    </span>
  );
}

// ---- growth bar ----

/** 6 px capsule; hovering 300 ms grows it to 12 px and shows "X / Y · Z%" inside. Height is reserved. */
export function LabeledProgressBar({ value, label }: { value: number; label: string }) {
  const [expanded, setExpanded] = useState(false);
  const timer = useRef<number>(undefined);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  return (
    <div
      className={expanded ? "cc-bar expanded" : "cc-bar"}
      role="progressbar"
      aria-valuetext={label}
      onMouseEnter={() => {
        window.clearTimeout(timer.current);
        timer.current = window.setTimeout(() => setExpanded(true), 300);
      }}
      onMouseLeave={() => {
        window.clearTimeout(timer.current);
        setExpanded(false);
      }}
    >
      <div className="cc-bar-track">
        <div className="cc-bar-fill" style={{ width: `${Math.min(1, Math.max(0, value)) * 100}%` }} />
        <div className="cc-bar-label">{label}</div>
      </div>
    </div>
  );
}

// ---- dex links ----

/** Asks the Collection tab to open a species' entry (shop-bag-dex listens for this). */
export const OPEN_DEX_ENTRY = "companion:open-dex-entry";

function useDexLink(c: Companion, goTo: TabProps["goTo"]) {
  return (id: number | null, form: string | null) => {
    if (id == null || !c.dex.some((d) => d.chain.includes(id))) return undefined;
    return () => {
      goTo("collection");
      window.dispatchEvent(new CustomEvent(OPEN_DEX_ENTRY, { detail: { id, form } }));
    };
  };
}

function DexLink(props: { open?: () => void; hint: string; radius: number; children: React.ReactNode }) {
  if (!props.open) return <>{props.children}</>;
  return (
    <Tooltip text={props.hint}>
      <span
        className="cc-link"
        role="link"
        aria-label={props.hint}
        style={{ position: "relative", display: "inline-flex", borderRadius: props.radius }}
        onClick={props.open}
      >
        {props.children}
      </span>
    </Tooltip>
  );
}

// ---- evolution line ----

const THUMB = 40;
const STRIDE = THUMB + THUMB * 0.25 + 4;

function Arrow() {
  return (
    <span className="cc-evo-arrow" aria-hidden>
      <svg width={8} height={8} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={2.2} strokeLinecap="round" strokeLinejoin="round">
        <path d="M2 8h12M9 3l5 5-5 5" />
      </svg>
    </span>
  );
}

function Chevron({ dir }: { dir: "l" | "r" }) {
  return (
    <svg width={9} height={9} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={3} strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d={dir === "l" ? "M10 3 5 8l5 5" : "M6 3l5 5-5 5"} />
    </svg>
  );
}

function EvoLine({ c, t, link }: { c: Companion; t: S; link: ReturnType<typeof useDexLink> }) {
  const a = c.active!;
  const scroller = useRef<HTMLDivElement>(null);
  const [edges, setEdges] = useState({ l: false, r: false });
  const measure = () => {
    const el = scroller.current;
    if (!el) return;
    setEdges({ l: el.scrollLeft > 1, r: el.scrollLeft + el.clientWidth < el.scrollWidth - 1 });
  };
  useLayoutEffect(measure, [c.line_nodes.length]);
  const page = (dir: number) => {
    const el = scroller.current;
    if (!el) return;
    const cols = Math.max(1, Math.floor(el.clientWidth / STRIDE));
    el.scrollBy({ left: dir * cols * STRIDE });
  };
  return (
    <div className={`cc-evo${edges.l ? " fade-l" : ""}${edges.r ? " fade-r" : ""}`}>
      <button className="cc-chev l" style={{ opacity: edges.l ? 1 : 0, pointerEvents: edges.l ? "auto" : "none" }} aria-label={t.evolutionScrollPrevious} onClick={() => page(-1)}>
        <Chevron dir="l" />
      </button>
      <div className="cc-evo-scroll" ref={scroller} onScroll={measure}>
        {c.line_nodes.map((n, i) => (
          <span key={i} style={{ display: "contents" }}>
            {i > 0 && <Arrow />}
            <span className={`cc-evo-node ${n.state}`}>
              {n.id == null ? (
                <span className="cc-evo-mystery" aria-label={t.unknownNextEvolution}>?</span>
              ) : (
                <DexLink open={link(n.id, a.form)} hint={t.dexOpenEntryHint} radius={6}>
                  <Sprite id={n.id} size={THUMB} animated={false} shiny={c.displays_shiny} form={a.form} />
                </DexLink>
              )}
              {n.state === "current" && <span className="cc-evo-dot" />}
            </span>
          </span>
        ))}
      </div>
      <button className="cc-chev r" style={{ opacity: edges.r ? 1 : 0, pointerEvents: edges.r ? "auto" : "none" }} aria-label={t.evolutionScrollNext} onClick={() => page(1)}>
        <Chevron dir="r" />
      </button>
    </div>
  );
}

// ---- celebrations ----

/** Seqs already played. Module scope = consumed once, even if the card remounts (tab switch). */
const seen = { celebration: -1, candy: -1, mint: -1 };
const whenVisible = (f: () => void) => {
  if (document.visibilityState === "visible") return f();
  const on = () => {
    if (document.visibilityState !== "visible") return;
    document.removeEventListener("visibilitychange", on);
    f();
  };
  document.addEventListener("visibilitychange", on);
};

type Fx = { pop: boolean; shiny: "" | "in" | "out"; ditto: "" | "in" | "out"; candy: { xp: number; out: boolean } | null; mint: "" | "in" | "out" };

function useCelebrations(c: Companion) {
  const [fx, setFx] = useState<Fx>({ pop: false, shiny: "", ditto: "", candy: null, mint: "" });
  // Timers are not cleared on unmount: a seq is consumed once, so its fade-out must always run.
  const later = (ms: number, f: () => void) => window.setTimeout(f, ms);
  const set = (p: Partial<Fx>) => setFx((f) => ({ ...f, ...p }));

  useEffect(() => {
    const cel = c.celebration;
    if (!cel || cel.seq === seen.celebration) return;
    seen.celebration = cel.seq;
    whenVisible(() => {
      set({ pop: true });
      later(800, () => set({ pop: false }));
      if (cel.kind === "hatch" && cel.shiny) {
        later(300, () => set({ shiny: "in" }));
        later(2600, () => set({ shiny: "out" }));
        later(3100, () => set({ shiny: "" }));
      }
      if (cel.kind === "ditto_reveal") {
        later(250, () => set({ ditto: "in" }));
        if (cel.shiny) later(450, () => set({ shiny: "in" }));
        later(2600, () => set({ ditto: "out", shiny: cel.shiny ? "out" : "" }));
        later(3100, () => set({ ditto: "", shiny: "" }));
      }
    });
  }, [c.celebration?.seq]);

  useEffect(() => {
    const candy = c.candy;
    if (!candy || candy.seq === seen.candy) return;
    seen.candy = candy.seq;
    whenVisible(() => {
      set({ candy: { xp: candy.xp, out: false } });
      later(1300, () => set({ candy: { xp: candy.xp, out: true } }));
      later(1700, () => set({ candy: null }));
    });
  }, [c.candy?.seq]);

  useEffect(() => {
    if (!c.mint_seq || c.mint_seq === seen.mint) return;
    seen.mint = c.mint_seq;
    whenVisible(() => {
      set({ mint: "in" });
      later(900, () => set({ mint: "out" }));
      later(1300, () => set({ mint: "" }));
    });
  }, [c.mint_seq]);
  return fx;
}

// ---- card ----

/** Home slot: receives the full TabProps so its inputs can change without touching HomeTab. */
export default function CompanionCard({ companion: c, goTo }: TabProps) {
  const t = useStrings(companionStrings);
  const lang = useLang();
  const link = useDexLink(c ?? ({ dex: [] } as unknown as Companion), goTo);
  const fx = useCelebrations(c ?? ({ celebration: null, candy: null, mint_seq: 0 } as unknown as Companion));
  const a = c?.active ?? null;
  const id = c?.current_id ?? null;
  const formRes = a && id != null ? formNameResource(id, a.form) : null;
  const fetched = useResourceNames("pokemon-form", formRes != null && !c?.form_labels[formRes] ? String(formRes) : null);
  if (!c) return null;

  const isEgg = !a;
  const eggProgress = c.egg_threshold > 0 ? c.egg_usage / c.egg_threshold : 0;
  const imminent = isEgg && eggProgress >= 0.9;
  const localizedForm = formRes != null ? resolveName(c.form_labels[formRes] ?? fetched?.names ?? undefined, lang) : null;
  const name =
    a && id != null
      ? formDisplayName(nameOf(a.line.names, id, lang), id, a.form, formLabel(id, a.form, localizedForm, t.normalForm))
      : "Token Egg";

  const status = (() => {
    switch (c.display_state) {
      case "egg": return t.statusEgg;
      case "idle": return t.statusIdle;
      case "working": return t.statusWorking;
      case "focus": return t.statusFocus;
      case "tired": return t.statusTired;
      case "sleep": return t.statusSleep;
      case "sulk": return t.statusSulk;
      default: return c.just_evolved_to ? t.statusEvolved(c.just_evolved_to) : t.statusGrew;
    }
  })();

  const tileClass = ["cc-tile", imminent && "wiggle", fx.pop && "pop"].filter(Boolean).join(" ");
  const tile = (
    <span className={tileClass}>
      <span className="cc-clip">
        <Sprite id={id} size={76} bob shiny={c.displays_shiny} back={c.faces_back} form={a?.form} />
      </span>
      <span className="cc-flash" />
      {fx.shiny && <span className={`cc-burst shiny${fx.shiny === "out" ? " out" : ""}`}>✨</span>}
      {fx.ditto && <span className={`cc-burst ditto${fx.ditto === "out" ? " out" : ""}`}>🎭</span>}
      {fx.candy && <span className={`cc-candy${fx.candy.out ? " out" : ""}`}>+{compact(fx.candy.xp)} XP</span>}
      {fx.mint && (
        <span className={`cc-mint${fx.mint === "out" ? " out" : ""}`}>
          <span style={{ fontSize: 22, marginLeft: -11, marginTop: -9 }}>✨</span>
          <span style={{ fontSize: 15, marginLeft: 13, marginTop: 5 }}>✨</span>
          <span style={{ fontSize: 12, marginLeft: 1, marginTop: 13 }}>✨</span>
        </span>
      )}
    </span>
  );

  return (
    <section className="cc">
      <div className="cc-head">
        <DexLink open={link(id, a?.form ?? null)} hint={t.dexOpenEntryHint} radius={12}>
          {tile}
        </DexLink>
        <div className="cc-info">
          <div className="cc-title">
            <span className="cc-name">{name}</span>
            {c.displays_shiny && <span className="cc-sparkle">✨</span>}
            {a && <Badge text={rarityLabel(t, a.line.rarity).toUpperCase()} tint={rarityColor(a.line.rarity)} />}
          </div>
          {a && id != null && c.current_types && (
            <>
              <span className="kit-row" style={{ gap: 5 }}>
                {c.current_types.map((ty) => <TypeBadge key={ty} type={ty} />)}
              </span>
              {!isDefaultForm(id, a.form) && <span className="cc-cap cc-sec">{t.defaultFormDataNote}</span>}
            </>
          )}
          {a ? (
            <>
              <div className="cc-row5">
                <span className="cc-cap cc-sec">
                  {(c.is_final_stage ? t.finalForm : t.stage(a.stage_index + 1, a.total_forms)) + (a.nature ? ` · ${natureLabel(a.nature, lang)}` : "")}
                </span>
                {c.growth_multiplier != null && <Badge text={t.growthBoost(c.growth_multiplier)} tint="var(--orange)" variant="tinted" />}
              </div>
              <LabeledProgressBar value={c.threshold ? a.used_at_stage / c.threshold : 0} label={progressDetail(a.used_at_stage, c.threshold ?? 0)} />
              {c.tokens_to_next > 0 && (
                <span className="cc-cap cc-ter">
                  {c.is_final_stage ? t.toGraduation(compact(c.tokens_to_next)) : t.toNextEvolution(compact(c.tokens_to_next))}
                </span>
              )}
            </>
          ) : (
            <>
              <div className="cc-row6">
                <span className={imminent ? "cc-cap cc-orange" : "cc-cap cc-sec"}>{imminent ? t.eggImminent : t.eggIncubating}</span>
                {c.egg_tier && <Badge text={t.eggGuaranteeHint(rarityLabel(t, c.egg_tier))} tint={rarityColor(c.egg_tier)} />}
              </div>
              <LabeledProgressBar value={eggProgress} label={progressDetail(c.egg_usage, c.egg_threshold)} />
              {c.hatch_retry_delayed ? (
                <span className="cc-cap cc-orange">{t.eggHatchDelayed}</span>
              ) : (
                <span className="cc-cap cc-ter">{t.eggToHatch(compact(Math.max(0, c.egg_threshold - c.egg_usage)))}</span>
              )}
              {c.egg_usage === 0 && <span className="cc-cap cc-ter">{t.eggFirstRunHint(compact(c.egg_threshold))}</span>}
            </>
          )}
          <span className="cc-cap cc-sec">{status}</span>
        </div>
      </div>
      {a && c.line_nodes.length > 0 && <EvoLine c={c} t={t} link={link} />}
      {c.just_graduated && <span className="cc-cap cc-orange">{t.graduated(c.just_graduated)}</span>}
    </section>
  );
}
