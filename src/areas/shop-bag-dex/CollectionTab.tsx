// Collection tab: Pokédex grid | Catch log, shared filter toolbar, entry detail page
// (shop-bag-dex.md §5.3, CompanionView.swift CollectionView). Owner: shop-bag-dex area.
import { CSSProperties, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import "./shop-bag-dex.css";
import { Badge } from "../../kit/components";
import { rarityColor } from "../../kit/colors";
import { Lang, useLang, useStrings } from "../../kit/i18n";
import type { Companion, Rarity, Settings, TabProps } from "../../shared";
import { resolveName } from "../../shared";
import { OPEN_DEX_ENTRY, Sprite } from "../companion/CompanionCard";
import { forms } from "../companion/forms";
import { natureLabel } from "../companion/strings";
import { DexDetail, RepButton } from "./DexDetail";
import {
  DEX_SORTS, DexSort, DexSpecies, dexSpecies, earnedBadges, filterLog, filterSpecies, LOG_SORTS, LogEntry, logEntries,
  LogSort, PAGE_SIZE, pageContaining, pageCount, RARITY_DISPLAY_ORDER, Region, RegionCompletion, regionCompletions,
  relativeAge, unownFormsCollected, WheelPager,
} from "./dex";
import { sbdStrings } from "./strings";
import { Glyph, MenuItem, PopupMenu, rarityLabel, S } from "./ui";

// ---- popover-lifetime navigation (PopoverNavigation) ----

/** Segment survives tab switches while the popover is open; each open starts on the Pokédex. */
let lastSegment: "dex" | "log" = "dex";
/** A Home sprite asked for an entry before this tab mounted (`openDexEntry`). */
let pendingEntry: { id: number; form: string | null } | null = null;
if (typeof window !== "undefined") {
  window.addEventListener(OPEN_DEX_ENTRY, (e) => (pendingEntry = (e as CustomEvent).detail));
  listen("popover-opened", () => (lastSegment = "dex")).catch(() => {});
}

const regionName = (t: S, r: Region) =>
  ({ kanto: t.regionKanto, johto: t.regionJohto, hoenn: t.regionHoenn, sinnoh: t.regionSinnoh, unova: t.regionUnova })[r];

function badgeLabel(t: S, key: string) {
  const [kind, r] = key.split(".") as [string, Region | undefined];
  if (kind === "region") return t.badgeRegion(regionName(t, r!));
  if (kind === "regionShiny") return t.badgeRegionShiny(regionName(t, r!));
  return kind === "unown" ? t.badgeUnown : kind === "national" ? t.badgeNational : t.badgeNationalShiny;
}
const MEDAL: Record<string, string> = { unown: "🔤", national: "🌍", nationalShiny: "✨" };

const sortLabel = (t: S, s: DexSort | LogSort) =>
  ({
    numberAsc: t.sortNumberAsc, numberDesc: t.sortNumberDesc, nameAsc: t.sortNameAsc, nameDesc: t.sortNameDesc,
    rarityDesc: t.sortRarity, recentFirst: t.sortRecent, oldestFirst: t.sortOldest,
  })[s];

// ---- chips ----

function RarityChips({ t, counts, selected, onSelect }: { t: S; counts: (r: Rarity) => number; selected: Rarity | null; onSelect: (r: Rarity) => void }) {
  return (
    <div className="sbd-chips">
      {RARITY_DISPLAY_ORDER.map((r) => {
        const n = counts(r);
        const on = selected === r;
        return (
          <button key={r} className={`sbd-chip${on ? " on" : ""}`} style={{ "--chip": rarityColor(r) } as CSSProperties}
            disabled={n === 0} title={t.dexFilterHint} onClick={() => onSelect(r)}>
            <span className="sbd-dot" />
            <span className="sbd-chip-label">{rarityLabel(t, r)}</span>
            <span className="sbd-chip-count num">{n}</span>
            {on && <Glyph name="check" size={9} weight={3} style={{ color: "var(--chip)" }} />}
          </button>
        );
      })}
    </div>
  );
}

function RegionChips({ t, completions, selected, onSelect }: { t: S; completions: RegionCompletion[]; selected: Region | null; onSelect: (r: Region) => void }) {
  return (
    <div className="sbd-chips">
      {completions.map((c) => {
        const complete = c.owned >= c.total;
        const on = selected === c.region;
        const label = regionName(t, c.region);
        return (
          <button key={c.region} className={`sbd-chip region${on ? " on" : ""}${complete ? " complete" : ""}`}
            style={{ "--chip": complete ? "var(--green)" : "var(--text-2)" } as CSSProperties}
            disabled={c.owned === 0} title={`${label} ${c.owned}/${c.total}`} onClick={() => onSelect(c.region)}>
            {c.shinyOwned >= c.total ? <span className="sbd-chip-emoji">✨</span> : complete && <span className="sbd-chip-emoji">🏅</span>}
            <span className="sbd-chip-label">{label}</span>
            <span className="sbd-chip-count num" style={{ color: complete ? "var(--green)" : "var(--text-2)" }}>{c.owned}/{c.total}</span>
            {on && <Glyph name="check" size={9} weight={3} style={{ color: "var(--chip)" }} />}
          </button>
        );
      })}
    </div>
  );
}

function NoResults({ t, onReset }: { t: S; onReset: () => void }) {
  return (
    <div className="sbd-noresults">
      <Glyph name="search" size={24} style={{ color: "var(--text-3)" }} />
      <span className="sbd-callout sbd-medium sbd-sec">{t.noSearchResults}</span>
      <button className="sbd-btn" onClick={onReset}>{t.clearFilters}</button>
    </div>
  );
}

// ---- grid ----

type Shared = { query: string; shinyOnly: boolean; rarity: Rarity | null };

function DexGrid(props: {
  c: Companion; all: DexSpecies[]; t: S; f: Shared; sort: DexSort; region: Region | null;
  setRarity: (r: Rarity | null) => void; setRegion: (r: Region | null) => void;
  returnAnchor: string | null; onOpen: (s: DexSpecies) => void; onReset: () => void;
  isRep: (id: number) => boolean; setRep: (id: number | null) => void;
}) {
  const { c, all, t, f, sort, region } = props;
  const lang = useLang();
  const visible = useMemo(() => filterSpecies(all, { ...f, region, sort }, lang), [all, f, region, sort, lang]);
  const [page, setPage] = useState<number | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; s: DexSpecies } | null>(null);
  // The anchor only picks the first page; a later rebuild starts on page 1.
  const [anchor] = useState(props.returnAnchor);
  const pages = pageCount(visible.length);
  const current = Math.min(page ?? pageContaining(anchor, visible.map((s) => s.collectionID)), pages - 1);
  const slice = visible.slice(current * PAGE_SIZE, (current + 1) * PAGE_SIZE);

  const filterKey = `${f.query}|${f.shinyOnly}|${sort}|${f.rarity}|${region}`;
  const firstKey = useRef(filterKey);
  useEffect(() => {
    if (firstKey.current === filterKey) return;
    firstKey.current = filterKey;
    setPage(0);
    setSelected(null);
  }, [filterKey]);

  const gridRef = useRef<HTMLDivElement>(null);
  const pager = useRef(new WheelPager());
  const pageRef = useRef({ current, pages });
  pageRef.current = { current, pages };
  useEffect(() => {
    const el = gridRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const step = pager.current.step(e.deltaY, e.deltaMode, performance.now());
      const { current, pages } = pageRef.current;
      const next = Math.min(Math.max(0, current + step), pages - 1);
      if (next !== current) {
        setPage(next);
        setSelected(null);
      }
    };
    el.addEventListener("wheel", onWheel, { passive: false });
    return () => el.removeEventListener("wheel", onWheel);
  }, [visible.length > 0]);

  const completions = useMemo(() => regionCompletions(all), [all]);
  const medals = earnedBadges(all, unownFormsCollected(c)).filter((k) => MEDAL[k]);
  const sel = slice.find((s) => s.collectionID === selected);
  const formCount = (s: DexSpecies) => {
    if (forms(s.id).length === 0) return null;
    const set = new Set<string>();
    for (const d of c.dex) if (d.chain.includes(s.id)) set.add(d.form ?? forms(s.id)[0]);
    const a = c.active;
    if (a && a.path_ids.slice(0, a.stage_index + 1).includes(s.id)) set.add(a.form ?? forms(s.id)[0]);
    return [...set].filter((x) => forms(s.id).includes(x)).length;
  };

  return (
    <div className="sbd-grid-wrap">
      <div className="sbd-header">
        <div className="sbd-header-row">
          <span className="sbd-callout-semi">{t.dexTitle}</span>
          <span className="sbd-caption sbd-sec">{t.dexSpeciesTotal(all.length)}</span>
          <span className="sbd-spacer" />
          {medals.map((k) => (
            <span key={k} className="sbd-medal" title={badgeLabel(t, k)} aria-label={badgeLabel(t, k)}>{MEDAL[k]}</span>
          ))}
        </div>
        <RegionChips t={t} completions={completions} selected={region} onSelect={(r) => props.setRegion(region === r ? null : r)} />
        <RarityChips t={t} counts={(r) => all.filter((s) => s.rarity === r).length} selected={f.rarity}
          onSelect={(r) => props.setRarity(f.rarity === r ? null : r)} />
      </div>
      {visible.length === 0 ? (
        <NoResults t={t} onReset={props.onReset} />
      ) : (
        <>
          <div className="sbd-grid" ref={gridRef}>
            {Array.from({ length: PAGE_SIZE }, (_, i) => {
              const s = slice[i];
              if (!s) return <span key={`blank-${i}`} />;
              const fc = formCount(s);
              const rep = props.isRep(s.id);
              const tip = [
                `#${s.id} ${s.name}`, rarityLabel(t, s.rarity),
                ...(fc != null ? [t.formsCollected(fc, forms(s.id).length)] : []),
                ...(s.isShiny ? [t.dexShinyLabel] : []), ...(s.isRaising ? [t.dexRaising] : []), ...(rep ? [t.representativeBadge] : []),
              ].join(" · ");
              return (
                <button key={s.collectionID}
                  className={`sbd-cell${selected === s.collectionID ? " selected" : ""}${rep ? " rep" : ""}`}
                  title={tip} aria-label={tip}
                  onClick={() => { setSelected(s.collectionID); props.onOpen(s); }}
                  onContextMenu={(e) => { e.preventDefault(); setMenu({ x: e.clientX, y: e.clientY, s }); }}>
                  <span className="sbd-cell-sprite">
                    <Sprite id={s.id} size={56} animated={false} shiny={fc == null && s.isShiny} />
                    {s.isRaising && (
                      <span className="sbd-raising"><Badge text={t.dexRaising.toUpperCase()} tint="var(--accent)" variant="tinted" size={10} /></span>
                    )}
                  </span>
                  <span className="sbd-cell-name">{fc != null ? `${s.name} ${fc}/${forms(s.id).length}` : s.name}</span>
                  <span className="sbd-num-tag" style={{ color: rarityColor(s.rarity) }}>
                    #{s.id}
                    {rep && <Glyph name="starFill" size={8} style={{ color: "var(--accent)" }} />}
                  </span>
                  {s.isShiny && <span className="sbd-shiny-tag" aria-label={t.dexShinyLabel}>✨</span>}
                </button>
              );
            })}
          </div>
          <div className="sbd-footer">
            {sel && (
              <>
                <span className="sbd-footer-sel sbd-sec">{`#${sel.id} ${sel.name} · ${rarityLabel(t, sel.rarity)}`}</span>
                {forms(sel.id).length === 0 && (
                  <RepButton t={t} rep={props.isRep(sel.id)} onClick={() => props.setRep(props.isRep(sel.id) ? null : sel.id)} />
                )}
              </>
            )}
            <span className="sbd-spacer" />
            {pages > 1 && (
              <span className="sbd-pager">
                <button aria-label={t.dexPagePrev} disabled={current === 0} onClick={() => { setPage(Math.max(0, current - 1)); setSelected(null); }}>
                  <Glyph name="chevronLeft" size={13} weight={2.4} />
                </button>
                <span className="num" aria-label={t.dexPageLabel(current + 1, pages)}>{current + 1} / {pages}</span>
                <button aria-label={t.dexPageNext} disabled={current === pages - 1} onClick={() => { setPage(Math.min(pages - 1, current + 1)); setSelected(null); }}>
                  <Glyph name="chevronRight" size={13} weight={2.4} />
                </button>
              </span>
            )}
          </div>
        </>
      )}
      {menu && (
        <PopupMenu at={menu} onClose={() => setMenu(null)}
          items={
            forms(menu.s.id).length > 0
              ? [{ label: t.chooseForm, onSelect: () => { setSelected(menu.s.collectionID); props.onOpen(menu.s); } }]
              : props.isRep(menu.s.id)
                ? [{ label: t.representativeFollowCurrent, icon: "follow", onSelect: () => props.setRep(null) }]
                : [{ label: t.representativeSet, icon: "star", onSelect: () => props.setRep(menu.s.id) }]
          } />
      )}
    </div>
  );
}

// ---- catch log ----

const THUMB = 68;
const LOG_LINE_WIDTH = 296; // 320 − 2 × 8

function LogRow({ e, t, lang, dexIDs, onOpen }: { e: LogEntry; t: S; lang: Lang; dexIDs: Set<number>; onOpen: (id: number) => void }) {
  return (
    <div className="sbd-card sbd-log-row" data-entry={e.id}>
      <div className="sbd-log-head">
        <Badge text={rarityLabel(t, e.rarity).toUpperCase()} tint={rarityColor(e.rarity)} size={10} />
        {e.active ? (
          <Badge text={t.dexRaising.toUpperCase()} tint="var(--accent)" variant="tinted" size={10} />
        ) : (
          e.released_at && <Badge text={t.dexReleased.toUpperCase()} tint="var(--text-2)" variant="tinted" size={10} />
        )}
        {e.is_shiny && <span className="sbd-caption" aria-label={t.dexShinyLabel}>✨</span>}
        <span className="sbd-spacer" />
        {e.nature && <span className="sbd-small sbd-sec">{natureLabel(e.nature, lang)}</span>}
      </div>
      <div className="sbd-log-line" style={{ maxWidth: LOG_LINE_WIDTH }}>
        {e.chain.map((id, i) => (
          <span key={`${id}-${i}`} className="sbd-log-node">
            {i > 0 && (
              <span className="sbd-log-arrow" style={{ width: THUMB * 0.25 }}>
                <svg width={14} height={14} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth={1.6} strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                  <path d="M2 8h12M9 3l5 5-5 5" />
                </svg>
              </span>
            )}
            <span className="sbd-log-species">
              {dexIDs.has(id) ? (
                <span className="sbd-log-link" role="link" title={t.dexOpenEntryHint} aria-label={t.dexOpenEntryHint} onClick={() => onOpen(id)}>
                  <Sprite id={id} size={THUMB} animated={false} shiny={e.is_shiny} form={e.form} />
                </span>
              ) : (
                <Sprite id={id} size={THUMB} animated={false} shiny={e.is_shiny} form={e.form} />
              )}
              <span className="sbd-log-name sbd-sec">{resolveName(e.names?.[id], lang) ?? `#${id}`}</span>
            </span>
          </span>
        ))}
      </div>
      {e.caught_at && <span className="sbd-small sbd-sec">{relativeAge(Date.parse(e.caught_at), Date.now(), lang)}</span>}
    </div>
  );
}

function CatchLog(props: { c: Companion; t: S; f: Shared; sort: LogSort; setRarity: (r: Rarity | null) => void; dexIDs: Set<number>;
  returnAnchor: string | null; onOpen: (entry: LogEntry, id: number) => void; onReset: () => void }) {
  const { c, t, f, sort } = props;
  const lang = useLang();
  const all = useMemo(() => logEntries(c), [c]);
  const visible = useMemo(() => filterLog(all, { ...f, sort }, lang), [all, f, sort, lang]);
  const scroller = useRef<HTMLDivElement>(null);
  const [anchor] = useState(props.returnAnchor);

  useLayoutEffect(() => {
    if (!anchor) return;
    const center = () => scroller.current?.querySelector(`[data-entry="${CSS.escape(anchor)}"]`)?.scrollIntoView({ block: "center" });
    center();
    const timer = window.setTimeout(center, 50); // again once sprites have laid out
    return () => window.clearTimeout(timer);
  }, [anchor]);

  const filterKey = `${f.query}|${f.shinyOnly}|${sort}|${f.rarity}`;
  const firstKey = useRef(filterKey);
  useEffect(() => {
    if (firstKey.current === filterKey) return;
    firstKey.current = filterKey;
    scroller.current?.scrollTo({ top: 0, behavior: "smooth" });
  }, [filterKey]);

  return (
    <div className="sbd-grid-wrap">
      <div className="sbd-header">
        <div className="sbd-header-row">
          <span className="sbd-callout-semi">{t.catchLogTitle}</span>
          <span className="sbd-caption sbd-sec">{t.dexTotal(all.length)}</span>
        </div>
        <RarityChips t={t} counts={(r) => all.filter((e) => e.rarity === r).length} selected={f.rarity}
          onSelect={(r) => props.setRarity(f.rarity === r ? null : r)} />
      </div>
      {visible.length === 0 ? (
        <NoResults t={t} onReset={props.onReset} />
      ) : (
        <div className="sbd-log" ref={scroller}>
          {visible.map((e) => (
            <LogRow key={e.id} e={e} t={t} lang={lang} dexIDs={props.dexIDs} onOpen={(id) => props.onOpen(e, id)} />
          ))}
        </div>
      )}
    </div>
  );
}

// ---- tab ----

export default function CollectionTab({ companion, settings, save }: TabProps) {
  const t = useStrings(sbdStrings);
  return companion && <Collection c={companion} settings={settings} save={save} t={t} />;
}

function Collection({ c, settings, save, t }: { c: Companion; settings: Settings; save: (s: Settings) => void; t: S }) {
  const lang = useLang();
  const [segment, setSegmentState] = useState(lastSegment);
  const [query, setQuery] = useState("");
  const [shinyOnly, setShinyOnly] = useState(false);
  const [rarity, setRarity] = useState<Rarity | null>(null);
  const [region, setRegion] = useState<Region | null>(null);
  const [dexSort, setDexSort] = useState<DexSort>("numberAsc");
  const [logSort, setLogSort] = useState<LogSort>("recentFirst");
  const [detail, setDetail] = useState<{ id: string; form: string | null } | null>(null);
  const [dexAnchor, setDexAnchor] = useState<string | null>(null);
  const [logAnchor, setLogAnchor] = useState<string | null>(null);
  const [sortMenu, setSortMenu] = useState<{ x: number; y: number } | null>(null);

  const setSegment = (s: "dex" | "log") => {
    lastSegment = s;
    setDetail(null);
    setDexAnchor(null);
    setLogAnchor(null);
    setSegmentState(s);
  };
  const open = useCallback((e: { id: number; form: string | null }) => setDetail({ id: String(e.id), form: e.form }), []);
  useEffect(() => {
    if (pendingEntry) {
      open(pendingEntry);
      pendingEntry = null;
    }
    const on = (e: Event) => {
      pendingEntry = null;
      open((e as CustomEvent).detail);
    };
    window.addEventListener(OPEN_DEX_ENTRY, on);
    return () => window.removeEventListener(OPEN_DEX_ENTRY, on);
  }, [open]);

  const all = useMemo(() => dexSpecies(c, lang), [c, lang]);
  const dexIDs = useMemo(() => new Set(all.map((s) => s.id)), [all]);
  const shared = useMemo(() => ({ query, shinyOnly, rarity }), [query, shinyOnly, rarity]);
  const isRep = (id: number) => settings.representative === id;
  const setRep = (id: number | null) => save({ ...settings, representative: id, representative_form: null });
  const reset = () => {
    setQuery("");
    setShinyOnly(false);
    setRarity(null);
    setRegion(null);
    setDexSort("numberAsc");
    setLogSort("recentFirst");
  };

  if (c.dex.length === 0 && !c.active) {
    return (
      <div className="sbd-empty">
        <Sprite id={25} size={96} />
        <span className="sbd-callout-semi">{t.dexEmptyTitle}</span>
        <span className="sbd-caption sbd-sec sbd-center">{t.dexEmptyHint}</span>
      </div>
    );
  }

  const species = detail && all.find((s) => s.collectionID === detail.id);
  if (detail && species) {
    return (
      <div className="sbd-collection">
        <DexDetail key={species.collectionID} c={c} species={species} initialForm={detail.form} settings={settings} save={save} t={t}
          onBack={() => {
            if (segment === "dex") setDexAnchor(detail.id);
            setDetail(null);
          }} />
      </div>
    );
  }

  const customSort = segment === "log" ? logSort !== "recentFirst" : dexSort !== "numberAsc";
  const sortItems: MenuItem[] =
    segment === "log"
      ? LOG_SORTS.map((s) => ({ label: sortLabel(t, s), checked: logSort === s, onSelect: () => setLogSort(s) }))
      : DEX_SORTS.map((s) => ({ label: sortLabel(t, s), checked: dexSort === s, onSelect: () => setDexSort(s) }));

  return (
    <div className="sbd-collection">
      <div className="kit-segmented" role="tablist">
        <button className={segment === "dex" ? "on" : ""} onClick={() => setSegment("dex")}>{t.dexTitle}</button>
        <button className={segment === "log" ? "on" : ""} onClick={() => setSegment("log")}>{t.catchLogTitle}</button>
      </div>
      <div className="sbd-toolbar">
        <label className="sbd-search">
          <Glyph name="search" size={13} style={{ color: "var(--text-2)", flex: "none" }} />
          <input value={query} placeholder={t.dexSearchPlaceholder} onChange={(e) => setQuery(e.target.value)} spellCheck={false} />
          {query && (
            <button className="sbd-clear" aria-label={t.clearFilters} onClick={() => setQuery("")}>
              <Glyph name="xmarkFill" size={13} />
            </button>
          )}
        </label>
        <button className={`sbd-tool${shinyOnly ? " shiny" : ""}`} title={t.filterShinyOnly} aria-label={t.filterShinyOnly}
          aria-pressed={shinyOnly} onClick={() => setShinyOnly(!shinyOnly)}>
          <Glyph name={shinyOnly ? "sparkles" : "sparkle"} size={13} weight={shinyOnly ? 2.4 : 1.6} />
        </button>
        <button className={`sbd-tool${customSort ? " sorted" : ""}`} title={t.sortTitle} aria-label={t.sortTitle} aria-haspopup="menu"
          onClick={(e) => {
            const r = e.currentTarget.getBoundingClientRect();
            setSortMenu(sortMenu ? null : { x: r.right - 180, y: r.bottom + 4 });
          }}>
          <Glyph name="sort" size={13} />
        </button>
      </div>
      {segment === "dex" ? (
        <DexGrid c={c} all={all} t={t} f={shared} sort={dexSort} region={region} setRarity={setRarity} setRegion={setRegion}
          returnAnchor={dexAnchor} onReset={reset} isRep={isRep} setRep={setRep}
          onOpen={(s) => { setDexAnchor(null); setDetail({ id: s.collectionID, form: null }); }} />
      ) : (
        <CatchLog c={c} t={t} f={shared} sort={logSort} setRarity={setRarity} dexIDs={dexIDs} returnAnchor={logAnchor} onReset={reset}
          onOpen={(entry, id) => { setLogAnchor(entry.id); setDetail({ id: String(id), form: entry.form }); }} />
      )}
      {sortMenu && <PopupMenu at={sortMenu} items={sortItems} onClose={() => setSortMenu(null)} />}
    </div>
  );
}
