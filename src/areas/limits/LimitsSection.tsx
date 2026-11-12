// Official limits on Home (limits.md §5). Owner: limits area. Home-usage renders it as a slot under
// the provider tabs; it shows the status banner and the selected provider's limits + a divider.
import { ReactNode, useState } from "react";
import { compact, cost, relativeDuration } from "../../kit/format";
import { CapsuleTabBar, LimitProgressBar, Tooltip } from "../../kit/components";
import { tierTextColor } from "../../kit/colors";
import { useLang, useStrings } from "../../kit/i18n";
import type { LimitWindow, ProviderLimits, TabProps } from "../../shared";
import {
  getClaudeAccount, openSessionKeySettings, setClaudeAccount, useLimitsExtras, useManualRefresh, useNow,
} from "./hooks";
import {
  DisplayMode, forecast, friendlyError, isStale, LimitsText, paceHelp, percentText, resetLabel,
  rowLook, rowName, sectionEntries, windowPace, windowReset,
} from "./model";
import { limitsStrings } from "./strings";
import "./limits.css";

type Ctx = { t: LimitsText; lang: string; now: number; mode: DisplayMode; warn: number; crit: number };

// ---- glyphs (SF Symbols → inline SVG) ----

const Glyph = ({ d, fill, size = 11 }: { d: string; fill?: boolean; size?: number }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden className="lim-glyph"
    fill={fill ? "currentColor" : "none"} stroke={fill ? "none" : "currentColor"} strokeWidth={2} strokeLinecap="round" strokeLinejoin="round">
    <path d={d} />
  </svg>
);
const TRIANGLE = "M10.3 3.6 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.6a2 2 0 0 0-3.4 0zM11 9h2v5h-2zm0 7h2v2h-2z";
const KEY_SLASH = "M15 3a6 6 0 0 0-5.6 8.2L3 17.6V21h3.4l1-1v-2h2v-2h2l1.6-1.6A6 6 0 1 0 15 3zm2.5 3a1.5 1.5 0 1 1 0 3 1.5 1.5 0 0 1 0-3zM2 2l20 20";
const QUESTION = "M12 22a10 10 0 1 1 0-20 10 10 0 0 1 0 20zM9.1 9a3 3 0 0 1 5.8 1c0 2-3 3-3 3M12 17h.01";
const CHECK = "M12 22a10 10 0 1 1 0-20 10 10 0 0 1 0 20zM8 12l3 3 5-6";

const Spinner = () => <span className="lim-spinner" aria-hidden />;

function SmallButton(p: { label: string; busy?: boolean; onClick: () => void; prominent?: boolean }) {
  return (
    <button className={p.prominent ? "lim-btn prominent" : "lim-btn"} disabled={p.busy} onClick={p.onClick}>
      {p.busy ? <Spinner /> : p.label}
    </button>
  );
}

// ---- rows ----

/** `quotaRow`: name · detail · reset | idle hint · percent, then the gauge; pace tooltip. */
function QuotaRow({ c, w, name }: { c: Ctx; w: LimitWindow; name: string }) {
  const reset = windowReset(w);
  const look = rowLook(w.used_percent, windowPace(w, c.now), c.warn, c.crit, c.mode);
  const help = paceHelp(look, w.used_percent, c.mode, c.t);
  const row = (
    <div className="lim-row">
      <div className="lim-row-top">
        <span className="lim-name">{name}</span>
        <span className="lim-spacer" />
        {w.detail && <span className="lim-detail num">{w.detail}</span>}
        {reset != null ? (
          <span className="lim-reset">{resetLabel(reset, c.now, c.lang)}</span>
        ) : (
          w.not_started && <span className="lim-reset">{c.t.fiveHourNotStarted}</span>
        )}
        <span className="lim-pct num" style={{ color: tierTextColor(look.gauge) }}>
          {percentText(w.used_percent, c.mode, c.t)}
        </span>
      </div>
      <div className="lim-bar-cell">
        <LimitProgressBar percent={c.mode === "remaining" ? Math.max(0, 100 - w.used_percent) : w.used_percent} tier={look.gauge} marker={look.marker} />
      </div>
    </div>
  );
  return help ? <Tooltip text={help}>{row}</Tooltip> : row;
}

function StaleBadge({ c, at }: { c: Ctx; at: string | null }) {
  if (!at) return null;
  return <span className="lim-stale">{`${c.t.staleLimits} · ${relativeDuration((c.now - Date.parse(at)) / 1000, c.lang)}`}</span>;
}

/** Generic auth-expired box (non-Claude): triangle + title, hint, optional prominent Retry. */
function ExpiredBox(p: { title: string; hint: string; retry?: ReactNode }) {
  return (
    <div className="lim-expired">
      <div className="lim-notice-head">
        <span className="lim-orange"><Glyph d={TRIANGLE} fill /></span>
        <span className="lim-notice-title lim-medium">{p.title}</span>
      </div>
      <div className="lim-hint">{p.hint}</div>
      {p.retry}
    </div>
  );
}

/** Claude notices: key-slash (session key) or triangle (OAuth) with a small trailing button. */
function ClaudeNotice(p: { icon: string; title: string; hint: string; button: ReactNode }) {
  return (
    <div className="lim-notice">
      <div className="lim-notice-head">
        <span className="lim-orange"><Glyph d={p.icon} fill={p.icon === TRIANGLE} /></span>
        <span className="lim-notice-title">{p.title}</span>
        <span className="lim-spacer" />
        {p.button}
      </div>
      <div className="lim-hint">{p.hint}</div>
    </div>
  );
}

function RefreshRow(p: { c: Ctx; staleAt: string | null; refresh: ReturnType<typeof useManualRefresh>; showError?: boolean }) {
  const { c, refresh } = p;
  const error = p.showError && !refresh.busy ? friendlyError(refresh.error, c.t) : null;
  return (
    <div className="lim-refresh">
      <div className="lim-refresh-row">
        {p.staleAt ? <StaleBadge c={c} at={p.staleAt} /> : <span className="lim-caption secondary">{c.t.limitsTapToLoad}</span>}
        <span className="lim-spacer" />
        <SmallButton label={c.t.refresh} busy={refresh.busy} onClick={refresh.run} />
      </div>
      {error && <div className="lim-error">{error}</div>}
    </div>
  );
}

// ---- providers ----

function HelpButton({ t }: { t: LimitsText }) {
  const [open, setOpen] = useState(false);
  return (
    <span className="lim-help">
      <Tooltip text={t.helpTitle}>
        <button className="lim-help-btn" aria-label={t.helpTitle} onClick={() => setOpen(!open)}>
          <Glyph d={QUESTION} size={11} />
        </button>
      </Tooltip>
      {open && (
        <div className="lim-help-card" role="dialog">
          <div className="lim-help-title">{t.helpTitle}</div>
          <div className="lim-hint">{t.helpBody}</div>
          <div className="lim-help-actions">
            <SmallButton label={t.registerSessionKey} onClick={() => { setOpen(false); openSessionKeySettings(); }} />
          </div>
        </div>
      )}
    </span>
  );
}

function ClaudeContent({ c, entries, props, refresh }: { c: Ctx; entries: ProviderLimits[]; props: TabProps; refresh: ReturnType<typeof useManualRefresh> }) {
  const extras = useLimitsExtras();
  const [selected, select] = useState(getClaudeAccount());
  const accounts = entries.filter((e) => e.account);
  const def = entries.find((e) => e.account?.is_default) ?? (accounts.length === 0 ? entries[0] : undefined);
  const missing = !def || def.windows.length === 0;
  const defStale = !!def && def.windows.length > 0 && isStale(def.updated_at, c.now);
  const otherStale = accounts.some((a) => !a.account!.is_default && a.windows.length > 0 && !a.auth_expired && isStale(a.updated_at, c.now));

  let notice: ReactNode = null;
  if (def?.auth_expired === "sessionKey") {
    notice = <ClaudeNotice icon={KEY_SLASH} title={c.t.sessionKeyExpiredTitle} hint={c.t.sessionKeyExpiredNoticeHint}
      button={<SmallButton label={c.t.settings} onClick={openSessionKeySettings} />} />;
  } else if (def?.auth_expired === "oauth") {
    notice = <ClaudeNotice icon={TRIANGLE} title={c.t.claudeAuthExpiredTitle} hint={c.t.claudeAuthExpiredHint}
      button={<SmallButton label={c.t.retry} busy={refresh.busy} onClick={refresh.run} />} />;
  } else if (missing || defStale || extras.claude_pending || otherStale) {
    notice = <RefreshRow c={c} staleAt={!missing && defStale ? def!.updated_at : null} refresh={refresh} showError />;
  }

  const account = accounts.find((a) => a.account!.id === selected) ?? accounts[0];
  return (
    <>
      {notice}
      {accounts.length > 1 && (
        <CapsuleTabBar
          tabs={accounts.map((a) => ({
            id: a.account!.id,
            title: a.account!.title,
            tooltip: a.account_label ?? undefined,
            dot: a.account!.tracked ? "currentColor" : undefined,
          }))}
          selected={account?.account?.id ?? ""}
          onSelect={(id) => { setClaudeAccount(id); select(id); }}
        />
      )}
      {account && <ClaudeAccountLimits c={c} p={account} single={accounts.length <= 1} props={props} refresh={refresh} />}
    </>
  );
}

function ClaudeAccountLimits({ c, p, single, props, refresh }: { c: Ctx; p: ProviderLimits; single: boolean; props: TabProps; refresh: ReturnType<typeof useManualRefresh> }) {
  const a = p.account!;
  const expired = p.auth_expired != null;
  const block = props.usage?.providers.find((s) => s.provider_id === "claude_code")?.active_block ?? null;
  const fiveHour = p.windows.find((w) => w.key === "5h");
  const fc = a.tracked ? forecast(fiveHour, block, c.now) : null;
  const clock = new Intl.DateTimeFormat(c.lang, { hour: "2-digit", minute: "2-digit", hourCycle: "h23" });
  return (
    <>
      {!a.is_default && expired && (p.auth_expired === "sessionKey" ? (
        <ClaudeNotice icon={KEY_SLASH} title={c.t.sessionKeyExpiredTitle} hint={c.t.sessionKeyExpiredNoticeHint}
          button={<SmallButton label={c.t.settings} onClick={openSessionKeySettings} />} />
      ) : (
        <ClaudeNotice icon={TRIANGLE} title={c.t.claudeAuthExpiredTitle} hint={c.t.additionalAccountExpiredHint(a.folder)}
          button={<SmallButton label={c.t.retry} busy={refresh.busy} onClick={refresh.run} />} />
      ))}
      {!a.is_default && !expired && p.windows.length > 0 && isStale(p.updated_at, c.now) && <StaleBadge c={c} at={p.updated_at} />}
      {p.plan && <div className="lim-meta">{c.t.plan(p.plan)}</div>}
      {p.account_label && <div className="lim-meta">{c.t.limitsAccount(p.account_label)}</div>}
      <div className="lim-rows" style={{ opacity: expired ? 0.5 : 1 }}>
        {p.windows.map((w) => (
          <div key={w.key} className="lim-rows-item">
            <QuotaRow c={c} w={w} name={rowName(w, c.t)} />
            {w.key === "5h" && fc && (
              <div className={fc.beforeReset ? "lim-forecast lim-orange" : "lim-forecast tertiary"}>
                <Glyph d={fc.beforeReset ? TRIANGLE : CHECK} fill={fc.beforeReset} size={10} />
                <span>{fc.beforeReset ? c.t.forecastReach(clock.format(new Date(fc.at))) : c.t.forecastNoReach}</span>
              </div>
            )}
          </div>
        ))}
        {single && block && (
          <div className="lim-block">
            <span className="secondary">{c.t.claudeCurrentBlock}</span>
            <span className="num">{compact(block.total)}</span>
            <span className="lim-spacer" />
            <span className="tertiary">{`${c.t.reset} ${resetLabel(Date.parse(block.end), c.now, c.lang)}`}</span>
          </div>
        )}
      </div>
    </>
  );
}

function CodexContent({ c, p }: { c: Ctx; p: ProviderLimits }) {
  const buckets: string[] = [];
  for (const w of p.windows) if (!buckets.includes(w.group ?? "")) buckets.push(w.group ?? "");
  const stale = isStale(p.updated_at, c.now);
  return (
    <>
      {(p.plan || p.limit_reached || stale) && (
        <div className="lim-meta-row">
          {p.plan && <span className="lim-meta">{c.t.plan(p.plan)}</span>}
          {p.limit_reached && <span className="lim-caption lim-red">{c.t.limitReached}</span>}
          {stale && <StaleBadge c={c} at={p.updated_at} />}
        </div>
      )}
      {buckets.map((b) => (
        <div key={b} className="lim-rows">
          {buckets.length > 1 && <div className="lim-caption secondary lim-bucket">{b}</div>}
          {p.windows.filter((w) => (w.group ?? "") === b).map((w) => (
            <QuotaRow key={w.key} c={c} w={w} name={rowName(w, c.t)} />
          ))}
        </div>
      ))}
    </>
  );
}

function SimpleContent({ c, p }: { c: Ctx; p: ProviderLimits }) {
  const t = c.t;
  const expired = p.auth_expired != null;
  const titles: Record<string, [string, string]> = {
    cursor: [t.cursorAuthExpiredTitle, t.cursorAuthExpiredHint],
    opencode: [t.openCodeGoAuthExpiredTitle, t.openCodeGoAuthExpiredHint],
    grok: [t.grokAuthExpiredTitle, t.grokAuthExpiredHint],
  };
  const [title, hint] = titles[p.provider] ?? ["", ""];
  const has = p.windows.length > 0;
  return (
    <>
      {expired && <ExpiredBox title={title} hint={hint} />}
      {has && isStale(p.updated_at, c.now) && <StaleBadge c={c} at={p.updated_at} />}
      {has && p.provider === "cursor" && p.message && <div className="lim-caption secondary">{p.message}</div>}
      {has && (
        <div className={p.provider === "grok" ? "lim-rows tight" : "lim-rows"} style={{ opacity: expired ? 0.5 : 1 }}>
          {p.windows.map((w) => (
            <div key={w.key} className="lim-rows-item">
              <QuotaRow c={c} w={w} name={rowName(w, t)} />
              {w.key === "included" && p.remaining_usd != null && (
                <div className="lim-meta">{t.cursorRemainingSpend(cost(p.remaining_usd))}</div>
              )}
            </div>
          ))}
          {p.products.length > 0 && (
            <div className="lim-meta">{p.products.map((x) => `${x.name} ${percentText(x.percent, "used", t)}`).join(" · ")}</div>
          )}
        </div>
      )}
    </>
  );
}

const STATUS_COLOR: Record<string, string> = {
  operational: "var(--green)", minor: "var(--yellow)", maintenance: "var(--yellow)",
  major: "var(--orange)", critical: "var(--red)", unknown: "var(--gray)",
};
const STATUS_KEY = {
  operational: "statusOperational", minor: "statusMinor", major: "statusMajor",
  critical: "statusCritical", maintenance: "statusMaintenance", unknown: "statusUnknown",
} as const;

/**
 * Home slot. Receives the full TabProps; `providerId` is the provider tab Home has selected
 * (falls back to the first provider with usage, then Claude).
 */
export default function LimitsSection(props: TabProps & { providerId?: string }) {
  const t = useStrings(limitsStrings);
  const lang = useLang();
  const now = useNow();
  const extras = useLimitsExtras();
  const refresh = useManualRefresh();
  const { settings, limits } = props;
  const provider = props.providerId ?? props.usage?.providers[0]?.provider_id ?? "claude_code";
  const c: Ctx = { t, lang, now, mode: settings.limit_display, warn: settings.warn_threshold, crit: settings.crit_threshold };
  const entries = sectionEntries(limits, provider);
  const status = settings.status_checks_enabled ? extras.statuses.find((s) => s.provider === provider) : undefined;
  const defaultKey = extras.session_keys.find((k) => k.account === "default");

  return (
    <>
      {status && status.indicator !== "operational" && (
        <div className="lim-status">
          <span className="lim-status-dot" style={{ background: STATUS_COLOR[status.indicator] }} />
          <span className="lim-caption lim-medium">{t[STATUS_KEY[status.indicator]]}</span>
          {status.description && <span className="lim-caption secondary lim-ellipsis">{status.description}</span>}
        </div>
      )}
      {entries.length > 0 && (
        <>
          <section className="lim">
            <div className="lim-head">
              <span className="lim-caption secondary">{t.limitsOfficial}</span>
              {provider === "claude_code" && !defaultKey?.configured && <HelpButton t={t} />}
            </div>
            {provider === "claude_code" && <ClaudeContent c={c} entries={entries} props={props} refresh={refresh} />}
            {provider === "codex" && <CodexContent c={c} p={entries[0]} />}
            {["cursor", "opencode", "grok"].includes(provider) && <SimpleContent c={c} p={entries[0]} />}
          </section>
          <hr className="divider" />
        </>
      )}
    </>
  );
}
