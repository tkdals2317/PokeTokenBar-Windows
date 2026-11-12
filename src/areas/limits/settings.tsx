// Settings rows owned by limits (pet-settings-misc.md §3.3 rows 5, 7, 22-24, 26, 36-38, 40).
// Collected by src/shell/settings.tsx.
import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Segmented } from "../../kit/components";
import { useStrings } from "../../kit/i18n";
import { SettingRow, SettingsRow, SettingsRowProps, Toggle } from "../../shell/settingsKit";
import { SessionKeyView, takeSessionKeyFocus, useLimits, useLimitsExtras, useManualRefresh } from "./hooks";
import { friendlyError, setCrit, setWarn } from "./model";
import { limitsStrings } from "./strings";
import "./limits.css";

function LimitDisplay({ settings, set }: SettingsRowProps) {
  const t = useStrings(limitsStrings);
  return (
    <SettingRow label={t.limitDisplayModeLabel}>
      <Segmented
        options={[["used", t.limitDisplayUsed], ["remaining", t.limitDisplayRemaining]]}
        value={settings.limit_display}
        onChange={(v) => set("limit_display", v)}
      />
    </SettingRow>
  );
}

/** Shown only with more than one Claude account. */
function TrackedAccount({ settings, set }: SettingsRowProps) {
  const t = useStrings(limitsStrings);
  const accounts = useLimits().flatMap((p) => (p.account ? [p.account] : []));
  if (accounts.length < 2) return null;
  return (
    <div className="lim-set">
      <div>{t.trackedAccountLabel}</div>
      <div className="setting-hint">{t.trackedAccountHint}</div>
      <select value={settings.claude_tracked_account} onChange={(e) => set("claude_tracked_account", e.target.value)}>
        <option value="automatic">{t.trackedAccountAutomatic}</option>
        <option value="highest">{t.trackedAccountHighest}</option>
        {accounts.map((a) => (
          <option key={a.id} value={`account:${a.id}`}>{a.title}</option>
        ))}
      </select>
    </div>
  );
}

function Slider(p: { label: string; min: number; max: number; value: number; onChange: (v: number) => void }) {
  return (
    <SettingRow label={p.label}>
      <span className="lim-slider">
        <input type="range" min={p.min} max={p.max} step={5} value={p.value} onChange={(e) => p.onChange(Number(e.target.value))} />
        <span className="value">{p.value}%</span>
      </span>
    </SettingRow>
  );
}

function LimitAlerts({ settings: s, set, save }: SettingsRowProps) {
  const t = useStrings(limitsStrings);
  return (
    <>
      <SettingRow label={t.limitNotificationsLabel}>
        <Toggle on={s.limit_notifications} set={(v) => set("limit_notifications", v)} />
      </SettingRow>
      {s.limit_notifications && (
        <>
          <Slider label={t.warning} min={50} max={95} value={s.warn_threshold}
            onChange={(v) => save({ ...s, ...setWarn(v, s.crit_threshold) })} />
          <Slider label={t.critical} min={80} max={100} value={s.crit_threshold}
            onChange={(v) => save({ ...s, ...setCrit(s.warn_threshold, v) })} />
        </>
      )}
    </>
  );
}

function StatusChecks({ settings, set }: SettingsRowProps) {
  const t = useStrings(limitsStrings);
  return (
    <SettingRow label={t.statusChecksLabel} hint={t.statusChecksHint}>
      <Toggle on={settings.status_checks_enabled} set={(v) => set("status_checks_enabled", v)} />
    </SettingRow>
  );
}

/** One session key: field (never echoes the stored key), Save/Delete, organization picker, error. */
function SessionKeyEditor({ view, focus }: { view: SessionKeyView; focus?: boolean }) {
  const t = useStrings(limitsStrings);
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const field = useRef<HTMLInputElement>(null);
  const account = view.account === "default" ? null : view.account;

  useEffect(() => {
    if (focus && field.current) {
      field.current.scrollIntoView({ block: "center" });
      field.current.focus();
    }
  }, [focus]);
  useEffect(() => {
    // The organization list fills the first time Settings shows a saved key.
    if (view.configured && view.organizations.length === 0) {
      invoke("session_key_orgs", { account }).catch(() => {});
    }
  }, [view.configured]);

  const submit = async () => {
    if (!input || busy) return;
    setBusy(true);
    setError(null);
    try {
      await invoke("session_key_save", { account, key: input });
      setInput("");
    } catch (code) {
      setError(friendlyError(String(code), t));
    } finally {
      setBusy(false);
    }
  };
  return (
    <>
      <div className="lim-set-line">
        <input
          ref={field}
          type="password"
          className="lim-key-input"
          placeholder={view.configured ? "••••••••" : "sk-ant-sid…"}
          value={input}
          autoComplete="off"
          spellCheck={false}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && submit()}
        />
        <button className="lim-btn" disabled={!input || busy} onClick={submit}>
          {busy ? <span className="lim-spinner" /> : t.save}
        </button>
        {view.configured && (
          <button className="lim-btn" onClick={() => { setInput(""); setError(null); invoke("session_key_delete", { account }); }}>
            {t.delete}
          </button>
        )}
      </div>
      {view.organizations.length > 1 && (
        <div className="lim-set-line">
          <span>{t.sessionKeyOrganizationLabel}</span>
          <span className="lim-spacer" />
          <select value={view.organization_id ?? ""} onChange={(e) => invoke("session_key_select_org", { account, id: e.target.value })}>
            {view.organizations.map((o) => <option key={o.id} value={o.id}>{o.name}</option>)}
          </select>
        </div>
      )}
      {error ? <div className="lim-error">{error}</div> : view.configured && <div className="lim-tertiary-note">{t.sessionKeyStorageNote}</div>}
    </>
  );
}

function Badge({ view }: { view: SessionKeyView }) {
  const t = useStrings(limitsStrings);
  if (view.expired) return <span className="lim-badge-bad">{t.sessionKeyExpiredBadge}</span>;
  return view.configured ? <span className="lim-badge-ok">✓ {t.sessionKeySaved}</span> : null;
}

function SessionKeys() {
  const t = useStrings(limitsStrings);
  const extras = useLimitsExtras();
  const [focus] = useState(takeSessionKeyFocus);
  const keys = extras.session_keys;
  const main = keys.find((k) => k.account === "default") ?? {
    account: "default", folder: "~/.claude", title: "", configured: false, expired: false, organizations: [], organization_id: null,
  };
  const others = keys.filter((k) => k.account !== "default");
  return (
    <>
      <div className="lim-set">
        <div className="lim-set-line">
          <span>{t.sessionKeyLabel}</span>
          <Badge view={main} />
        </div>
        <div className="setting-hint">{others.length ? `${t.sessionKeyHint} ${t.sessionKeyPerAccountNote}` : t.sessionKeyHint}</div>
        <SessionKeyEditor view={main} focus={focus} />
      </div>
      {others.map((k) => (
        <div key={k.account} className="lim-set">
          <div className="lim-set-line">
            <span>{t.accountSessionKeyLabel(k.title)}</span>
            <Badge view={k} />
          </div>
          {k.title !== k.folder && <div className="setting-hint">{k.folder}</div>}
          <SessionKeyEditor view={k} />
        </div>
      ))}
    </>
  );
}

/** Windows: there is no Keychain; the button re-reads the credential file and skips the backoff. */
function RefreshLimitToken() {
  const t = useStrings(limitsStrings);
  const refresh = useManualRefresh();
  const message = refresh.busy ? null : friendlyError(refresh.error, t);
  return (
    <div className="lim-set">
      <div className="lim-set-line">
        <div className="setting-label">
          <div>{t.refreshLimitToken}</div>
          <div className="setting-hint">{t.refreshTokenHint}</div>
        </div>
        <span className="lim-spacer" />
        <button className="lim-btn" disabled={refresh.busy} onClick={refresh.run}>
          {refresh.busy ? <span className="lim-spinner" /> : t.refresh}
        </button>
      </div>
      {message && <div className="lim-error">{message}</div>}
    </div>
  );
}

export const settingsRows: SettingsRow[] = [
  { id: "limit-display", group: "general", order: 50, Row: LimitDisplay },
  { id: "limit-tracked-account", group: "general", order: 70, Row: TrackedAccount },
  { id: "limit-alerts", group: "notifications", order: 220, Row: LimitAlerts },
  { id: "limit-status-checks", group: "notifications", order: 260, Row: StatusChecks },
  { id: "limit-session-keys", group: "advanced", order: 360, Row: SessionKeys },
  { id: "limit-refresh-token", group: "advanced", order: 400, Row: RefreshLimitToken },
];
