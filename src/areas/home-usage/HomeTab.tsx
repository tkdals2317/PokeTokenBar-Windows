// Home tab body (home-usage.md §5.2). Owner: home-usage area.
// Slots owned by other areas: <CompanionCard> (companion), <LimitsSection> (limits).
import { useCallback, useState } from "react";
import CompanionCard from "../companion/CompanionCard";
import LimitsSection from "../limits/LimitsSection";
import { CapsuleTabBar, Icon } from "../../kit/components";
import { compact, CostCoverage, costText, grouped } from "../../kit/format";
import { useStrings } from "../../kit/i18n";
import { DailyUsage, ProviderSnapshot, TabProps, todayKey, todayTotal, usePopoverOpened } from "../../shared";
import ActivityPanel from "./ActivityPanel";
import {
  currentStreak, dailyTrendTotals, hasDailyTrendHistory, monthDailyTotals, Streak, STREAK_MINIMUM_DAYS, systemWeekInfo,
  trendProviders,
} from "./metrics";
import RecapScreen from "./Recap";
import { selectProvider, useLedger, useSelectedProviderId } from "./state";
import { homeStrings } from "./strings";
import "./home.css";

type Strings = typeof homeStrings.en;
const WEEK = systemWeekInfo();

type Cost = { usd: number; coverage: CostCoverage };
function sumCost(items: ({ cost: number; coverage: CostCoverage } | null | undefined)[]): Cost {
  const out: Cost = { usd: 0, coverage: { reported: false, estimated: false, unknown: false } };
  for (const u of items) {
    if (!u) continue;
    out.usd += u.cost;
    out.coverage = {
      reported: out.coverage.reported || u.coverage.reported,
      estimated: out.coverage.estimated || u.coverage.estimated,
      unknown: out.coverage.unknown || u.coverage.unknown,
    };
  }
  return out;
}
const costLabel = (c: Cost, t: Strings) => costText(c.usd, c.coverage, false, t.costUnavailable);

function Flame() {
  return (
    <svg width="10" height="11" viewBox="0 0 20 22" aria-hidden>
      <path
        fill="currentColor"
        d="M10 0c.6 3.4 2.6 5.3 4.4 7.2C16.4 9.3 18 11.4 18 14.4 18 18.6 14.4 22 10 22S2 18.6 2 14.4c0-2.6 1.2-4.6 2.8-6.1.2 1.7 1 3 2.3 3.6C6.6 7.4 7.8 3.2 10 0zm.3 12.2c-.3 1.9-2.6 3-2.6 5.5A2.4 2.4 0 0 0 10 20a2.4 2.4 0 0 0 2.4-2.3c0-2.2-1.4-3.4-2.1-5.5z"
      />
    </svg>
  );
}

/** Flame + day count from 2 days; orange once today counts, grey while today has no usage yet. */
function StreakBadge({ streak, t }: { streak: Streak; t: Strings }) {
  if (streak.days < STREAK_MINIMUM_DAYS) return null;
  const label = streak.includesToday ? t.streakDays(streak.days) : t.streakDaysPending(streak.days);
  return (
    <span className="hu-streak" title={label} aria-label={label} role="img">
      <span style={{ color: streak.includesToday ? "var(--orange)" : "var(--text-2)" }}>
        <Flame />
      </span>
      <span className="secondary num">{streak.days}</span>
    </span>
  );
}

function PeriodLabel({ name, tokens, cost, t }: { name: string; tokens: number; cost: Cost | null; t: Strings }) {
  return (
    <span className="hu-period-label">
      <span className="tertiary">{name}</span>
      <span className="num hu-strong">{compact(tokens)}</span>
      {cost && <span className="secondary">{costLabel(cost, t)}</span>}
    </span>
  );
}

function TokenType({ name, value }: { name: string; value: number }) {
  return (
    <span className="hu-token-type">
      <span className="tertiary">{name}</span>
      <span className="secondary num">{compact(value)}</span>
    </span>
  );
}

function ProviderRow({ snap, today, t }: { snap: ProviderSnapshot; today: DailyUsage; t: Strings }) {
  const models = today.models && today.models.length > 1 ? [...today.models].sort((a, b) => b.total - a.total) : [];
  return (
    <div className="hu-provider">
      <div className="hu-provider-head">
        <span className="hu-provider-name">{snap.display_name}</span>
        <span className="hu-spacer" />
        <span className="num">{compact(today.total)}</span>
        {snap.reports_cost && <span className="caption secondary">{costLabel({ usd: today.cost, coverage: today.coverage }, t)}</span>}
      </div>
      <div className="hu-token-types">
        <div>
          <TokenType name={t.tokenInput} value={today.tokens.input} />
          <TokenType name={t.tokenOutput} value={today.tokens.output} />
        </div>
        <div>
          <TokenType name={t.tokenCacheWrite} value={today.tokens.cache_write} />
          <TokenType name={t.tokenCacheRead} value={today.tokens.cache_read} />
        </div>
      </div>
      {models.map((m) => (
        <div key={m.model} className="hu-model">
          <span className="secondary hu-middle-ellipsis">{m.model.split("/").pop() || m.model}</span>
          <span className="hu-spacer" />
          <span className="tertiary num">{compact(m.total)}</span>
        </div>
      ))}
    </div>
  );
}

export default function HomeTab(props: TabProps) {
  const { usage, settings, save } = props;
  const t = useStrings(homeStrings);
  const ledger = useLedger();
  const selectedId = useSelectedProviderId(usage);
  const [recap, setRecap] = useState(false);
  usePopoverOpened(useCallback(() => setRecap(false), []));

  const day = todayKey();
  const ps = usage?.providers ?? [];
  const showsCost = ps.some((p) => p.reports_cost);
  const total = usage ? todayTotal(usage, day) : 0;
  const todayCost = sumCost(ps.map((p) => (p.today?.date === day ? p.today : null)));
  const week = ps.reduce((n, p) => n + (p.week_total?.total ?? 0), 0);
  const month = ps.reduce((n, p) => n + (p.month_total?.total ?? 0), 0);
  const streak = currentStreak(ledger, day);
  const trendWindow = settings.daily_trend_window;
  const selected = ps.find((p) => p.provider_id === selectedId) ?? null;

  const periodLabels = (
    <span className="hu-period-totals">
      <PeriodLabel name={t.thisWeek} tokens={week} cost={showsCost ? sumCost(ps.map((p) => p.week_total)) : null} t={t} />
      <PeriodLabel name={t.thisMonth} tokens={month} cost={showsCost ? sumCost(ps.map((p) => p.month_total)) : null} t={t} />
    </span>
  );

  return (
    <div className="hu-home">
      <CompanionCard {...props} />
      <hr className="divider" />
      <div className="hu-header">
        <div className="caption secondary">{t.todayTokens}</div>
        <div className="hu-today">
          <span className="hu-big num">{compact(total)}</span>
          {total >= 1_000 && <span className="caption secondary num">{grouped(total)}</span>}
          <span className="hu-spacer" />
          {showsCost && <span className="hu-callout secondary">{costLabel(todayCost, t)}</span>}
        </div>
        <div className="hu-period-row">
          {periodLabels}
          <span className="hu-period-actions">
            <StreakBadge streak={streak} t={t} />
            <button className="hu-recap-button" title={t.recapOpen} aria-label={t.recapOpen} onClick={() => setRecap(true)}>
              <Icon name="barChart" size={12} />
              <span>{t.recapDetails}</span>
            </button>
          </span>
        </div>
        <ActivityPanel
          visualization={settings.activity_visualization}
          onCycle={() =>
            save({
              ...settings,
              activity_visualization: settings.activity_visualization === "monthlyBars" ? "recentHeatmap" : "monthlyBars",
            })
          }
          barsSeries={dailyTrendTotals(ps, trendWindow)}
          heatmapSeries={monthDailyTotals(ps)}
          providers={trendProviders(ps, trendWindow)}
          ledger={ledger}
          showsCost={showsCost}
          window={trendWindow}
          setWindow={(w) => save({ ...settings, daily_trend_window: w })}
          hasHistory={hasDailyTrendHistory(ps)}
          today={day}
          week={WEEK}
        />
        {ps.length > 1 && (
          <div className="hu-tabs">
            <CapsuleTabBar
              tabs={ps.map((p) => ({ id: p.provider_id, title: p.display_name }))}
              selected={selectedId ?? ""}
              onSelect={selectProvider}
            />
          </div>
        )}
        {selected?.today && <ProviderRow snap={selected} today={selected.today} t={t} />}
      </div>
      <hr className="divider" />
      {/* Limits owns the status banner, its section and the trailing divider, for the selected provider. */}
      <LimitsSection {...props} />
      {recap && <RecapScreen ledger={ledger} companion={props.companion} firstDow={WEEK.firstDow} onBack={() => setRecap(false)} />}
    </div>
  );
}
