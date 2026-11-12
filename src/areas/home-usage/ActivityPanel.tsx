// Activity panel (home-usage.md §3.5-3.7): daily trend bars or the 16-week heatmap; a click cycles them.
// Owner: home-usage area.
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { compact, costText } from "../../kit/format";
import { useLang, useStrings } from "../../kit/i18n";
import type { DailyLedger, Settings } from "../../shared";
import {
  axisLabel, barHeight, columns, Day, dayStamp, heatmapDays, isStacked, isWeekend, level, ordered, ProviderSeries,
  providerColor, recentGrid, recordedSince, segments, showsPeak, TREND_WINDOWS, TrendWindow, WeekInfo, weekdayLetters,
} from "./metrics";
import { homeStrings } from "./strings";

type Visualization = Settings["activity_visualization"];
const VISUALIZATIONS: Visualization[] = ["monthlyBars", "recentHeatmap"];

type Strings = typeof homeStrings.en;

/** "Thu, 11/5 1.1M $2.76" for `day`, or "" when the day is not in the series. */
function readout(series: Day[], day: string, showsCost: boolean, t: Strings, lang: string): string {
  const d = series.find((x) => x.date === day);
  if (!d) return "";
  const head = `${dayStamp(d.date, lang)} ${compact(d.total)}`;
  return showsCost ? `${head} ${costText(d.cost, d.coverage, false, t.costUnavailable)}` : head;
}

function PageDots({ current }: { current: Visualization }) {
  return (
    <span className="hu-dots">
      {VISUALIZATIONS.map((v) => (
        <span key={v} className={v === current ? "on" : ""} />
      ))}
    </span>
  );
}

function Peak({ peak, t }: { peak: number; t: Strings }) {
  return (
    <>
      <span className="caption tertiary">{t.peakDay}</span>
      <span className="caption secondary num hu-strong">{compact(peak)}</span>
    </>
  );
}

const windowTitle = (w: TrendWindow, t: Strings) =>
  ({ "7d": t.dailyTrend7d, "14d": t.dailyTrend14d, "21d": t.dailyTrend21d, month: t.dailyTrendMonth })[w];

/** Borderless menu: the window title with a chevron; the active window carries a checkmark. */
function WindowMenu({ value, onChange, t }: { value: TrendWindow; onChange: (w: TrendWindow) => void; t: Strings }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent) => !ref.current?.contains(e.target as Node) && setOpen(false);
    window.addEventListener("mousedown", close);
    return () => window.removeEventListener("mousedown", close);
  }, [open]);
  return (
    <span ref={ref} className="hu-menu" onClick={(e) => e.stopPropagation()}>
      <button className="hu-menu-button caption tertiary" aria-haspopup="menu" aria-expanded={open} onClick={() => setOpen(!open)}>
        {windowTitle(value, t)}
        <svg width="8" height="8" viewBox="0 0 10 10" aria-hidden>
          <path d="M2 3.5 5 6.5 8 3.5" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </button>
      {open && (
        <div className="hu-menu-list" role="menu">
          {TREND_WINDOWS.map((w) => (
            <button
              key={w}
              role="menuitemradio"
              aria-checked={w === value}
              onClick={() => {
                setOpen(false);
                onChange(w);
              }}
            >
              <span className="hu-check">{w === value ? "✓" : ""}</span>
              {windowTitle(w, t)}
            </button>
          ))}
        </div>
      )}
    </span>
  );
}

type BarsProps = {
  series: Day[];
  providers: ProviderSeries[];
  showsCost: boolean;
  window: TrendWindow;
  setWindow: (w: TrendWindow) => void;
  hasHistory: boolean;
  today: string;
  week: WeekInfo;
};

function DailyBars(p: BarsProps) {
  const t = useStrings(homeStrings);
  const lang = useLang();
  const [hovered, setHovered] = useState<string | null>(null);
  const cols = columns(p.series, p.today, p.window);
  const peak = Math.max(0, ...cols.map((c) => c.tokens));

  const caption = (withPeak: boolean) => (
    <div className="hu-caption">
      <WindowMenu value={p.window} onChange={p.setWindow} t={t} />
      <span className="caption secondary num hu-medium">{readout(p.series, p.today, p.showsCost, t, lang)}</span>
      <span className="hu-spacer" />
      {withPeak && <Peak peak={peak} t={t} />}
      <PageDots current="monthlyBars" />
    </div>
  );

  if (peak <= 0) return p.hasHistory ? <div className="hu-panel-body">{caption(false)}</div> : null;

  const stack = ordered(p.providers);
  const stacked = isStacked(stack);
  return (
    <div className="hu-panel-body">
      {caption(showsPeak(cols))}
      <div className="hu-bars-wrap">
        <div className="hu-bars">
          {cols.map((c) => {
            if (c.isFuture) return <div key={c.date} className="hu-col" />;
            const isToday = c.date === p.today;
            const isHovered = hovered === c.date;
            const h = barHeight(c.tokens, peak);
            const segs = stacked && c.tokens > 0 ? segments(c.date, stack, h) : [];
            return (
              <div
                key={c.date}
                className="hu-col"
                onMouseEnter={() => setHovered(c.date)}
                onMouseLeave={() => setHovered((x) => (x === c.date ? null : x))}
              >
                {segs.length === 0 ? (
                  <div
                    className={`hu-bar ${isToday ? "today" : isHovered ? "hover" : c.tokens === 0 ? "empty" : ""}`}
                    style={{ height: h }}
                  />
                ) : (
                  <div className="hu-bar stacked" style={{ height: h, opacity: isToday || isHovered ? 1 : 0.55 }}>
                    {[...segs].reverse().map((s) => (
                      <div key={s.id} style={{ height: s.height, background: providerColor(s.id) }} />
                    ))}
                  </div>
                )}
              </div>
            );
          })}
        </div>
        {hovered && (
          <HoverCard
            day={hovered}
            index={cols.findIndex((c) => c.date === hovered)}
            count={cols.length}
            series={p.series}
            stack={stacked ? stack : []}
            showsCost={p.showsCost}
            t={t}
            lang={lang}
          />
        )}
      </div>
      <div className="hu-ticks">
        {cols.map((c) => (
          <div key={c.date} className={!c.isFuture && isWeekend(c.date, p.week) ? "on" : ""} />
        ))}
      </div>
      <div className="hu-axis">
        {cols.map((c) => {
          const label = axisLabel(c.date, p.today, p.window);
          const tone = c.date === p.today ? "accent" : c.isFuture ? "tertiary" : "secondary";
          return (
            <div key={c.date}>
              {label && <span className={`num ${tone}`}>{label}</span>}
            </div>
          );
        })}
      </div>
      {stacked && (
        <div className="hu-legend">
          {stack.map((s) => (
            <span key={s.id}>
              <i style={{ background: providerColor(s.id) }} />
              {s.name}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

/** Floating card 4 px above the bars, centred on the column and clamped to the row. No delay. */
function HoverCard(props: {
  day: string;
  index: number;
  count: number;
  series: Day[];
  stack: ProviderSeries[];
  showsCost: boolean;
  t: Strings;
  lang: string;
}) {
  const { day, series, stack, showsCost, t } = props;
  const ref = useRef<HTMLDivElement>(null);
  const [left, setLeft] = useState<number | null>(null);
  useLayoutEffect(() => {
    const card = ref.current;
    const row = card?.parentElement;
    if (!card || !row) return;
    const width = row.clientWidth;
    const center = ((width + 1.5) / props.count) * (props.index + 0.5);
    setLeft(Math.min(Math.max(center - card.offsetWidth / 2, 0), Math.max(0, width - card.offsetWidth)));
  }, [props.index, props.count, day]);

  const total = series.find((d) => d.date === day);
  const unavailable = t.costUnavailable;
  const rows = stack.flatMap((s) => {
    const d = s.days.find((x) => x.date === day);
    return d && d.total > 0 ? [{ s, d }] : [];
  });
  return (
    <div ref={ref} className="hu-hover" style={{ left: left ?? 0, visibility: left == null ? "hidden" : undefined }}>
      <div className="hu-hover-head">
        <b>{dayStamp(day, props.lang)}</b>
        <span>{compact(total?.total ?? 0)}</span>
        {showsCost && <span className="secondary">{costText(total?.cost ?? 0, total?.coverage ?? { reported: false, estimated: false, unknown: false }, false, unavailable)}</span>}
      </div>
      {rows.map(({ s, d }) => (
        <div key={s.id} className="hu-hover-row">
          <i style={{ background: providerColor(s.id) }} />
          <span className="secondary">{s.name}</span>
          <span className="hu-spacer" style={{ minWidth: 8 }} />
          <span>{compact(d.total)}</span>
          {showsCost && s.reportsCost && <span className="secondary">{costText(d.cost, d.coverage, false, unavailable)}</span>}
        </div>
      ))}
    </div>
  );
}

function Heatmap(p: { series: Day[]; ledger: DailyLedger; showsCost: boolean; today: string; week: WeekInfo }) {
  const t = useStrings(homeStrings);
  const lang = useLang();
  const [hovered, setHovered] = useState<string | null>(null);
  const grid = recentGrid(heatmapDays(p.ledger, p.series), recordedSince(p.ledger, p.series), p.today, p.week.firstDow);
  const used = grid.flat().filter((c) => !c.isOutsideRange && c.tokens > 0);
  const peak = Math.max(0, ...used.map((c) => c.tokens));
  if (peak <= 0) return null;

  const day = hovered ?? p.today;
  const cell = grid.flat().find((c) => c.date === day);
  const text = p.series.some((d) => d.date === day)
    ? readout(p.series, day, p.showsCost, t, lang)
    : cell
      ? `${dayStamp(day, lang)} ${compact(cell.tokens)}`
      : "";
  return (
    <div className="hu-panel-body">
      <div className="hu-caption">
        <span className="caption tertiary hu-ellipsis">{t.recentHeatmap}</span>
        <span className="caption secondary num hu-medium hu-nowrap">{text}</span>
        <span className="hu-spacer" />
        {used.length > 1 && <Peak peak={peak} t={t} />}
        <PageDots current="recentHeatmap" />
      </div>
      <div className="hu-heatmap">
        <div className="hu-weekdays">
          {weekdayLetters(lang, p.week.firstDow).map((l, i) => (
            <span key={i}>{l}</span>
          ))}
        </div>
        <div className="hu-grid">
          {grid.map((col, i) => (
            <div key={i}>
              {col.map((c) => (
                <div
                  key={c.date}
                  className={`hu-cell${c.date === p.today ? " today" : ""}`}
                  data-level={c.isOutsideRange ? undefined : level(c.tokens, peak)}
                  onMouseEnter={() => !c.isOutsideRange && setHovered(c.date)}
                  onMouseLeave={() => !c.isOutsideRange && setHovered(null)}
                />
              ))}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

export default function ActivityPanel(p: {
  visualization: Visualization;
  onCycle: () => void;
  barsSeries: Day[];
  heatmapSeries: Day[];
  providers: ProviderSeries[];
  ledger: DailyLedger;
  showsCost: boolean;
  window: TrendWindow;
  setWindow: (w: TrendWindow) => void;
  hasHistory: boolean;
  today: string;
  week: WeekInfo;
}) {
  const t = useStrings(homeStrings);
  const chart =
    p.visualization === "recentHeatmap" ? (
      <Heatmap series={p.heatmapSeries} ledger={p.ledger} showsCost={p.showsCost} today={p.today} week={p.week} />
    ) : (
      <DailyBars
        series={p.barsSeries}
        providers={p.providers}
        showsCost={p.showsCost}
        window={p.window}
        setWindow={p.setWindow}
        hasHistory={p.hasHistory}
        today={p.today}
        week={p.week}
      />
    );
  return (
    <div className="hu-panel" title={t.activitySwitchHint} onClick={p.onCycle}>
      {chart}
    </div>
  );
}
