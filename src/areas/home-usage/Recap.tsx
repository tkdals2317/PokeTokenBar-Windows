// Usage recap screen (home-usage.md §5.3, macOS UsageRecapView). Covers the whole popover like macOS,
// where it replaces the content. Owner: home-usage area.
import { useEffect, useState } from "react";
import { compact, grouped, percent } from "../../kit/format";
import { Segmented } from "../../kit/components";
import { Lang, useLang, useStrings } from "../../kit/i18n";
import { Companion, DailyLedger, EGG, nameOf } from "../../shared";
import {
  addDays, dateOf, dayName, keyOf, litBlocks, makeRecap, METER_BLOCKS, monthName, period, Recap, RecapScope, shifted,
} from "./metrics";
import { homeStrings } from "./strings";

type DexEntry = Companion["dex"][number];
type Strings = typeof homeStrings.en;

const STATIC_SPRITES = "https://raw.githubusercontent.com/PokeAPI/sprites/master/sprites/pokemon";
const staticSprite = (id: number, shiny: boolean) => `${STATIC_SPRITES}/${shiny ? "shiny/" : ""}${id}.png`;

type RangeFormat = { formatRange(a: Date, b: Date): string };

function periodLabel(r: Recap<DexEntry>, lang: string): string {
  const { scope, start, end } = r.period;
  if (scope === "week") {
    const fmt = new Intl.DateTimeFormat(lang, { day: "numeric", month: "short", year: "numeric" }) as unknown as RangeFormat;
    return fmt.formatRange(start, addDays(end, -1));
  }
  if (scope === "month") return monthName(start, lang);
  return String(start.getFullYear());
}

function bucketLabel(key: string, scope: RecapScope, lang: string): string {
  const d = dateOf(key);
  if (scope === "week") return new Intl.DateTimeFormat(lang, { weekday: "short" }).format(d).slice(0, 2).toUpperCase();
  if (scope === "month") return [1, 8, 15, 22, 29].includes(d.getDate()) ? String(d.getDate()) : "";
  return new Intl.DateTimeFormat(lang, { month: "narrow" }).format(d).toUpperCase();
}

function RecapCard({ r, companion, t, lang }: { r: Recap<DexEntry>; companion: Companion | null; t: Strings; lang: Lang }) {
  const scope = r.period.scope;
  const peak = Math.max(1, ...r.buckets.map((b) => b.tokens));
  const active = companion?.active;
  const current = active
    ? { src: staticSprite(active.path_ids[active.stage_index], active.is_shiny), alt: nameOf(active.line.names, active.path_ids[active.stage_index], lang) }
    : { src: EGG, alt: "" };
  const up = r.delta != null && r.delta >= 0;
  return (
    <div className="hu-card">
      <div className="hu-card-head">
        <span className="hu-lens" />
        {["#F25959", "#FAD159", "#73E080"].map((c) => (
          <span key={c} className="hu-led" style={{ background: c }} />
        ))}
        <span className="hu-spacer" style={{ minWidth: 4 }} />
        <span className="hu-period">{periodLabel(r, lang).toUpperCase()}</span>
      </div>
      <div className="hu-screen">
        <div className="hu-total">
          <span className="hu-total-num">{compact(r.total)}</span>
          <span className="hu-total-unit">{t.recapTokensUnit.toUpperCase()}</span>
          <span className="hu-spacer" style={{ minWidth: 4 }} />
          {r.delta != null && (
            <span className={`hu-delta ${up ? "up" : "down"}`}>
              {up ? "▲" : "▼"}
              {percent(Math.abs(r.delta) * 100)}
            </span>
          )}
        </div>
        <div className={`hu-meters ${scope}`}>
          {r.buckets.map((b) => {
            const lit = litBlocks(b.tokens, peak);
            const name = scope === "year" ? monthName(dateOf(b.key), lang) : dayName(b.key, lang);
            const label = `${name}, ${b.hasData ? grouped(b.tokens) : t.recapNoData}`;
            return (
              <div key={b.key} className={`hu-meter${b.isCurrent ? " current" : ""}`} title={label} aria-label={label}>
                <div className="hu-blocks">
                  {Array.from({ length: METER_BLOCKS }, (_, i) => (
                    <i key={i} className={i >= METER_BLOCKS - lit ? "lit" : b.hasData ? "known" : ""} />
                  ))}
                </div>
                <span className="hu-meter-label">{bucketLabel(b.key, scope, lang)}</span>
              </div>
            );
          })}
        </div>
        <div className="hu-readouts">
          <div>
            <Readout label={t.recapBestDay} value={r.bestDay ? compact(r.bestDay.tokens) : "--"} />
            <Readout label={t.recapBestStreak} value={t.recapDays(r.bestStreak)} />
          </div>
          <div>
            <Readout label={t.recapActiveDays} value={`${r.activeDays}/${r.countedDays}`} />
            <Readout label={t.recapGraduates} value={String(r.graduated.length)} />
          </div>
        </div>
      </div>
      <div className="hu-strip">
        <img src={current.src} alt={current.alt} width={18} height={18} />
        <span className="hu-strip-divider" />
        {r.graduated.length === 0 ? (
          <span className="hu-no-grads">{t.recapNoGraduates}</span>
        ) : (
          r.graduated.slice(0, 4).map((e, i) => (
            <span key={`${e.final_id}-${e.caught_at}-${i}`} className="hu-chip">
              <span className="hu-chip-sprite">
                <img src={staticSprite(e.final_id, e.is_shiny)} alt="" width={18} height={18} />
                {e.is_shiny && <span className="hu-sparkle">✨</span>}
              </span>
              <span className="hu-chip-name">{nameOf(e.names, e.final_id, lang)}</span>
            </span>
          ))
        )}
      </div>
    </div>
  );
}

const Readout = ({ label, value }: { label: string; value: string }) => (
  <div className="hu-readout">
    <span>{label.toUpperCase()}</span>
    <b>{value}</b>
  </div>
);

const Triangle = ({ dir }: { dir: "left" | "right" }) => (
  <svg width="8" height="8" viewBox="0 0 10 10" aria-hidden>
    <path d={dir === "left" ? "M8 1v8L1 5z" : "M2 1v8l7-4z"} fill="currentColor" />
  </svg>
);

export default function RecapScreen(props: {
  ledger: DailyLedger;
  companion: Companion | null;
  firstDow: number;
  onBack: () => void;
}) {
  const t = useStrings(homeStrings);
  const lang = useLang();
  const [scope, setScope] = useState<RecapScope>("week");
  const [offset, setOffset] = useState(0);
  const { onBack } = props;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onBack();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onBack]);

  const now = new Date();
  const today = keyOf(now);
  const p = shifted(period(scope, now, props.firstDow), offset, props.firstDow);
  const r = makeRecap(props.ledger, props.companion?.dex ?? [], p, today, props.firstDow);
  const currentName = { week: t.thisWeek, month: t.thisMonth, year: t.thisYear }[scope];
  const compareName = { week: t.recapCompareWeek, month: t.recapCompareMonth, year: t.recapCompareYear }[scope];

  return (
    <div className="hu-recap">
      <div className="hu-recap-top">
        <button className="hu-borderless" onClick={onBack}>
          <svg width="14" height="14" viewBox="0 0 24 24" aria-hidden>
            <path d="M15 5l-7 7 7 7" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
          {t.back}
        </button>
        <span className="hu-spacer" style={{ minWidth: 4 }} />
        <div className="hu-scope" aria-label={t.recapOpen}>
          <Segmented<RecapScope>
            options={[["week", t.recapWeek], ["month", t.recapMonth], ["year", t.recapYear]]}
            value={scope}
            onChange={(s) => {
              setScope(s);
              setOffset(0);
            }}
          />
        </div>
      </div>
      <div className="hu-recap-nav caption">
        <button className="hu-borderless" disabled={!r.canGoBack} onClick={() => setOffset(offset - 1)}>
          <Triangle dir="left" />
          {t.recapPrevious}
        </button>
        <span className="hu-spacer" />
        <button className="hu-borderless" disabled={offset === 0} onClick={() => setOffset(0)}>
          {currentName}
        </button>
        <span className="hu-spacer" />
        <button className="hu-borderless" aria-label={t.recapNext} disabled={offset >= 0} onClick={() => setOffset(offset + 1)}>
          {t.recapNext}
          <Triangle dir="right" />
        </button>
      </div>
      <RecapCard r={r} companion={props.companion} t={t} lang={lang} />
      <div className="hu-recap-notes caption secondary">
        {r.bestDay && <div>{t.recapBestDayLine(dayName(r.bestDay.key, lang), grouped(r.bestDay.tokens))}</div>}
        {r.isInProgress && r.delta != null && <div>{compareName}</div>}
      </div>
    </div>
  );
}
