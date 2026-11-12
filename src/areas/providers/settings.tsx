// Settings rows owned by providers (pet-settings-misc.md §3.3 rows 16, 41, 42; providers.md §6).
// Collected by src/shell/settings.tsx.
import { KeyboardEvent, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useStrings } from "../../kit/i18n";
import { SettingRow, SettingsRow, SettingsRowProps, Toggle } from "../../shell/settingsKit";
import { PROVIDERS, withProviderEnabled, withScanRoots } from "./registry";
import { providerStrings } from "./strings";
import "./providers.css";

function TrackedProviders({ settings, save }: SettingsRowProps) {
  const disabled = settings.disabled_providers;
  return (
    <>
      {PROVIDERS.map(([id, name]) => (
        <SettingRow key={id} label={name}>
          <Toggle
            label={name}
            on={!disabled.includes(id)}
            set={(on) => save({ ...settings, disabled_providers: withProviderEnabled(disabled, id, on) })}
          />
        </SettingRow>
      ))}
    </>
  );
}

/** Below the Providers card, like the macOS group footer. */
function TrackedProvidersHint() {
  return <div className="providers-footnote below">{useStrings(providerStrings).trackedProvidersHint}</div>;
}

/** Latest value for effects that run on unmount. */
function useLatest<T>(value: T) {
  const ref = useRef(value);
  ref.current = value;
  return ref;
}

/** Vertical text field: Enter commits, Shift+Enter adds a line, blur commits. */
function DraftField(props: { value: string; placeholder: string; onChange: (v: string) => void; onCommit: () => void }) {
  const lines = Math.max(1, props.value.split("\n").length);
  const onKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      props.onCommit();
    }
  };
  return (
    <textarea
      className="providers-field"
      rows={lines}
      spellCheck={false}
      placeholder={props.placeholder}
      value={props.value}
      onChange={(e) => props.onChange(e.target.value)}
      onKeyDown={onKeyDown}
      onBlur={props.onCommit}
    />
  );
}

/** Debounced (250 ms) value from the backend; a stale answer is dropped. */
function useDebounced<T>(fetch: () => Promise<T>, deps: unknown[], initial: T): T {
  const [value, setValue] = useState(initial);
  const generation = useRef(0);
  useEffect(() => {
    const mine = ++generation.current;
    const timer = setTimeout(() => {
      fetch().then((v) => mine === generation.current && setValue(v)).catch(() => {});
    }, 250);
    return () => clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, deps);
  return value;
}

function AdditionalAccounts(props: SettingsRowProps) {
  const t = useStrings(providerStrings);
  const [draft, setDraft] = useState(props.settings.additional_claude_config_dirs);
  const latest = useLatest({ draft, props });
  const commit = () => {
    const { draft, props } = latest.current;
    if (draft !== props.settings.additional_claude_config_dirs) {
      props.save({ ...props.settings, additional_claude_config_dirs: draft });
    }
  };
  useEffect(() => commit, []); // eslint-disable-line react-hooks/exhaustive-deps
  const folders = useDebounced(
    () => invoke<{ detected: string[]; found: number }>("claude_account_folders", { raw: draft }),
    [draft],
    { detected: [], found: 0 },
  );
  return (
    <div className="providers-group">
      <div>{t.additionalClaudeAccountsLabel}</div>
      <div className="providers-caption tertiary">{t.additionalClaudeAccountsHint}</div>
      {folders.detected.length > 0 && (
        <div className="providers-caption secondary">{t.additionalClaudeAccountsDetected(folders.detected.join(", "))}</div>
      )}
      <DraftField value={draft} placeholder={t.additionalClaudeAccountsPlaceholder} onChange={setDraft} onCommit={commit} />
      {draft.trim() !== "" && <div className="providers-caption tertiary">{t.additionalClaudeAccountsFound(folders.found)}</div>}
    </div>
  );
}

function ScanFolders(props: SettingsRowProps) {
  const t = useStrings(providerStrings);
  // The picker selection is not persisted: every Settings visit starts at Claude Code.
  const [owner, setOwner] = useState("claude_code");
  const [draft, setDraft] = useState(props.settings.custom_scan_roots[owner] ?? "");
  const latest = useLatest({ owner, draft, props });
  const commit = () => {
    const { owner, draft, props } = latest.current;
    const roots = props.settings.custom_scan_roots;
    if (draft !== (roots[owner] ?? "")) {
      props.save({ ...props.settings, custom_scan_roots: withScanRoots(roots, owner, draft) });
    }
  };
  useEffect(() => commit, []); // eslint-disable-line react-hooks/exhaustive-deps
  const switchTo = (id: string) => {
    commit(); // the old draft belongs to the old provider
    setOwner(id);
    setDraft(latest.current.props.settings.custom_scan_roots[id] ?? "");
  };
  const matches = useDebounced(() => invoke<number>("custom_scan_match_count", { provider: owner, raw: draft }), [owner, draft], 0);
  return (
    <>
      <div className="providers-group">
        <div>{t.customScanRootsLabel}</div>
        <div className="providers-caption tertiary">{t.customScanRootsHint}</div>
        <div className="providers-picker">
          <span className="providers-caption-1">{t.customScanProviderLabel}</span>
          <select aria-label={t.customScanProviderLabel} value={owner} onChange={(e) => switchTo(e.target.value)}>
            {PROVIDERS.map(([id, name]) => (
              <option key={id} value={id}>
                {name}
              </option>
            ))}
          </select>
        </div>
        <DraftField value={draft} placeholder={t.customScanRootsPlaceholder} onChange={setDraft} onCommit={commit} />
        {draft.trim() !== "" && <div className="providers-caption tertiary">{t.customScanRootsMatches(matches)}</div>}
      </div>
      <div className="providers-footnote">{t.aggregationNote}</div>
    </>
  );
}

export const settingsRows: SettingsRow[] = [
  { id: "tracked-providers", group: "providers", order: 160, Row: TrackedProviders },
  { id: "tracked-providers-hint", group: "providers", order: 161, Row: TrackedProvidersHint, footer: true },
  { id: "additional-claude-accounts", group: "advanced", order: 410, Row: AdditionalAccounts },
  { id: "custom-scan-roots", group: "advanced", order: 420, Row: ScanFolders },
];
