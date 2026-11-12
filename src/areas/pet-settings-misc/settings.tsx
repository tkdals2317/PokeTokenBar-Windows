// Settings rows owned by pet-settings-misc (pet-settings-misc.md §3.3 rows 1, 3, 4, 8, 11-15, 17-21,
// 27-34, 44-46 plus the menu-bar footer). Collected by src/shell/settings.tsx.
import { ReactNode, useCallback, useEffect, useState } from "react";
import { createPortal } from "react-dom";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import { compact } from "../../kit/format";
import { LANG_LABELS, LANGS, Lang, useLang, useStrings } from "../../kit/i18n";
import { EGG, Settings, usePopoverOpened } from "../../shared";
import { SettingRow, SettingsRow, SettingsRowProps, Toggle } from "../../shell/settingsKit";
import { petStrings } from "./strings";
import "./settings.css";

const SUPPORT_MAIL = "parkdongmin123@gmail.com";
const INTERVALS = [0, 1, 2, 5, 15];
const SPRITES = "https://raw.githubusercontent.com/PokeAPI/sprites/master/sprites/pokemon";

type SaveError = "notASaveFile" | "newerSchema" | "fileTooLarge" | "backupFailed" | "io";
type Summary = { dex: number; lifetime: number; species: number | null; shiny: boolean };
type Snapshot = Summary & { id: string; date: string };
type ImportPreview = { path: string; incoming: Summary; exported_at: string; source_device: string; current: Summary };

const useL = () => useStrings(petStrings);

function errorText(l: ReturnType<typeof useL>, e: unknown): string {
  switch (e as SaveError) {
    case "notASaveFile": return l.importErrorNotSaveFile;
    case "newerSchema": return l.importErrorNewerSchema;
    case "fileTooLarge": return l.importErrorTooLarge;
    case "backupFailed": return l.importErrorBackupFailed;
    default: return l.errorGeneric;
  }
}

/** Medium date + short time in the app language. */
const dateText = (iso: string, lang: Lang) =>
  new Intl.DateTimeFormat(lang, { dateStyle: "medium", timeStyle: "short" }).format(new Date(iso));

/** The popover hides while a native file dialog has focus; bring Settings back for the result. */
const reopenSettings = () => invoke("show_popover", { opened: { tab: null, settings: true } });

// ---- in-app alert / confirm (NSAlert: Return = Cancel on confirms) ----

type Dialog = { title: string; message: string; confirm?: { label: string; run: () => void } };

function DialogView({ d, close }: { d: Dialog; close: () => void }) {
  const l = useL();
  return (
    <div
      className="ptb-dialog-scrim"
      onKeyDown={(e) => {
        e.stopPropagation(); // Esc closes the dialog, not Settings
        if (e.key === "Escape") close();
      }}
    >
      <div className="ptb-dialog" role="alertdialog" aria-labelledby="ptb-dialog-title">
        <div id="ptb-dialog-title" className="ptb-dialog-title">{d.title}</div>
        <div className="ptb-dialog-message">{d.message}</div>
        <div className="ptb-dialog-buttons">
          {d.confirm && (
            <button className="ptb-dialog-destructive" onClick={() => { close(); d.confirm!.run(); }}>
              {d.confirm.label}
            </button>
          )}
          <button autoFocus onClick={close}>{d.confirm ? l.cancel : l.close}</button>
        </div>
      </div>
    </div>
  );
}

function useDialog(): [ReactNode, (d: Dialog) => void] {
  const [d, setD] = useState<Dialog | null>(null);
  return [d ? createPortal(<DialogView d={d} close={() => setD(null)} />, document.body) : null, setD];
}

const ToggleRow = ({ label, hint, on, set }: { label: string; hint?: string; on: boolean; set: (v: boolean) => void }) => (
  <SettingRow label={label} hint={hint}>
    <Toggle on={on} set={set} label={label} />
  </SettingRow>
);

// ---- General ----

function Language({ settings, set }: SettingsRowProps) {
  const l = useL();
  return (
    <SettingRow label={l.language}>
      <select value={settings.language} aria-label={l.language} onChange={(e) => set("language", e.target.value as Lang)}>
        {LANGS.map((x) => <option key={x} value={x}>{LANG_LABELS[x]}</option>)}
      </select>
    </SettingRow>
  );
}

function RefreshInterval({ settings, set }: SettingsRowProps) {
  const l = useL();
  return (
    <SettingRow label={l.refreshInterval}>
      <select value={settings.refresh_minutes} aria-label={l.refreshInterval} onChange={(e) => set("refresh_minutes", Number(e.target.value))}>
        {INTERVALS.map((m) => <option key={m} value={m}>{m === 0 ? l.intervalManual : l.intervalMinutes(m)}</option>)}
      </select>
    </SettingRow>
  );
}

/** Applies to both the tray sprite and the floating pet, so it lives in General. */
function Animation({ settings, set }: SettingsRowProps) {
  const l = useL();
  return (
    <SettingRow label={l.animationQualityLabel} hint={l.animationQualityHint}>
      <select value={settings.animation_quality} aria-label={l.animationQualityLabel}
        onChange={(e) => set("animation_quality", e.target.value as Settings["animation_quality"])}>
        <option value="powerSaver">{l.animationPowerSaver}</option>
        <option value="balanced">{l.animationBalanced}</option>
        <option value="smooth">{l.animationSmooth}</option>
      </select>
    </SettingRow>
  );
}

function LaunchAtLogin() {
  const l = useL();
  const [on, setOn] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    invoke<boolean>("get_autostart").then(setOn);
  }, []);
  return (
    <SettingRow label={l.launchAtLogin} hint={error ? <span className="ptb-red">{error}</span> : undefined}>
      <Toggle
        on={on}
        label={l.launchAtLogin}
        set={(v) =>
          invoke<boolean>("set_autostart", { enabled: v })
            .then((r) => {
              setOn(r);
              setError("");
            })
            .catch(() => {
              setError(l.errorGeneric);
              invoke<boolean>("get_autostart").then(setOn);
            })
        }
      />
    </SettingRow>
  );
}

// ---- Show in menu bar (the tray tooltip on Windows) ----

function MenuBar({ settings: s, set }: SettingsRowProps) {
  const l = useL();
  return (
    <>
      <ToggleRow label={l.todayTokensShort} on={s.show_tokens_in_menu} set={(v) => set("show_tokens_in_menu", v)} />
      <ToggleRow label={l.todayCost} on={s.show_cost_in_menu} set={(v) => set("show_cost_in_menu", v)} />
      <ToggleRow label={l.pokemonGrowthPercent} hint={l.pokemonGrowthPercentHint} on={s.show_growth_in_menu} set={(v) => set("show_growth_in_menu", v)} />
      <ToggleRow label={l.limitPercent} on={s.show_limit_in_menu} set={(v) => set("show_limit_in_menu", v)} />
      {s.show_limit_in_menu && (
        <SettingRow label={l.menuLimitColor} hint={l.menuLimitColorHint}>
          <select value={s.menu_limit_color_mode} aria-label={l.menuLimitColor}
            onChange={(e) => set("menu_limit_color_mode", e.target.value as Settings["menu_limit_color_mode"])}>
            <option value="gauge">{l.colorGauge}</option>
            <option value="attention">{l.colorAttention}</option>
            <option value="off">{l.colorOff}</option>
          </select>
        </SettingRow>
      )}
    </>
  );
}

function MenuBarFooter() {
  return <div className="ptb-footnote">{useL().allOffHint}</div>;
}

// ---- Floating Pet ----

function FloatingPet({ settings: s, set }: SettingsRowProps) {
  const l = useL();
  return (
    <>
      <ToggleRow label={l.floatingPetEnableLabel} hint={l.floatingPetHint} on={s.floating_pet_enabled} set={(v) => set("floating_pet_enabled", v)} />
      <ToggleRow label={l.backViewLabel} hint={l.backViewHint} on={s.always_faces_back} set={(v) => set("always_faces_back", v)} />
      {s.floating_pet_enabled && (
        <>
          <div className="setting-row">
            <span className="ptb-callout">{l.floatingPetSizeLabel}</span>
            <input
              className="ptb-slider"
              type="range"
              min={48}
              max={384}
              step={8}
              value={s.floating_pet_size}
              aria-label={l.floatingPetSizeLabel}
              onChange={(e) => set("floating_pet_size", Number(e.target.value))}
            />
            <span className="ptb-value num">{s.floating_pet_size}px</span>
          </div>
          <ToggleRow label={l.floatingPetBubbleAlertsLabel} on={s.floating_pet_bubble_alerts} set={(v) => set("floating_pet_bubble_alerts", v)} />
          <ToggleRow label={l.floatingPetAgentActivityLabel} hint={l.floatingPetAgentActivityHint}
            on={s.floating_pet_agent_activity} set={(v) => set("floating_pet_agent_activity", v)} />
        </>
      )}
    </>
  );
}

// ---- Updates ----

function Updates({ settings: s, set, save, updater }: SettingsRowProps) {
  const l = useL();
  const [version, setVersion] = useState("");
  const [checked, setChecked] = useState(false);
  useEffect(() => {
    getVersion().then(setVersion);
  }, []);
  const st = updater.state;
  const checking = st.status === "checking";
  const available = st.status === "available" || st.status === "downloading" ? st.update.version : null;
  const skipped = available != null && s.skipped_update_version === available;
  return (
    <>
      <ToggleRow label={l.updateNotificationsLabel} on={s.update_notifications_enabled} set={(v) => set("update_notifications_enabled", v)} />
      <SettingRow label={l.checkForUpdatesLabel}>
        <button
          disabled={checking}
          onClick={async () => {
            await updater.checkNow();
            setChecked(true);
          }}
        >
          {checking ? <span className="ptb-spinner" aria-label={l.checkNowButton} /> : l.checkNowButton}
        </button>
      </SettingRow>
      {checked && !checking && (
        <div className="setting-row">
          {available && skipped ? (
            <div className="ptb-stack">
              <span className="ptb-orange caption">{l.skippedVersion(available)}</span>
              <span className="ptb-buttons">
                <button className="small" onClick={() => save({ ...s, skipped_update_version: null })}>{l.showSkippedAgain}</button>
                <button className="small" disabled={st.status === "downloading"} onClick={updater.install}>{l.updateButton}</button>
              </span>
            </div>
          ) : available ? (
            <>
              <span className="ptb-orange caption">{l.updateFound(available)}</span>
              <button className="small" disabled={st.status === "downloading"} onClick={updater.install}>{l.updateButton}</button>
            </>
          ) : (
            <span className="secondary caption">{l.upToDate(version)}</span>
          )}
        </div>
      )}
    </>
  );
}

// ---- Backup & Transfer ----

function ExportSave() {
  const l = useL();
  const [dialog, show] = useDialog();
  return (
    <SettingRow label={l.exportSaveLabel} hint={l.exportSaveHint}>
      <button
        onClick={() =>
          invoke<boolean>("export_save", { title: l.exportSaveLabel }).catch((e) => {
            reopenSettings();
            show({ title: l.exportSaveLabel, message: errorText(l, e) });
          })
        }
      >
        {l.exportSaveButton}
      </button>
      {dialog}
    </SettingRow>
  );
}

function ImportSave() {
  const l = useL();
  const lang = useLang();
  const [dialog, show] = useDialog();
  async function pick() {
    let preview: ImportPreview | null;
    try {
      preview = await invoke<ImportPreview | null>("pick_import_save", { title: l.importSaveLabel });
    } catch (e) {
      reopenSettings();
      return show({ title: l.importSaveLabel, message: errorText(l, e) });
    }
    if (!preview) return;
    const p = preview;
    reopenSettings();
    show({
      title: l.importConfirmTitle,
      message: l.importConfirmBody(p.incoming.dex, compact(p.incoming.lifetime), dateText(p.exported_at, lang), p.source_device,
        p.current.dex, compact(p.current.lifetime)),
      confirm: {
        label: l.importConfirmReplace,
        run: () =>
          invoke<Summary>("apply_import", { path: p.path })
            .then((r) => show({ title: l.importSaveLabel, message: l.importSaveDone(r.dex, compact(r.lifetime)) }))
            .catch((e) => show({ title: l.importSaveLabel, message: errorText(l, e) })),
      },
    });
  }
  return (
    <SettingRow label={l.importSaveLabel} hint={l.importSaveHint}>
      <button onClick={pick}>{l.importSaveButton}</button>
      {dialog}
    </SettingRow>
  );
}

const staticSprite = (id: number, shiny: boolean) => `${SPRITES}/${shiny ? "shiny/" : ""}${id}.png`;

function Snapshots() {
  const l = useL();
  const lang = useLang();
  const [dialog, show] = useDialog();
  const [expanded, setExpanded] = useState(false);
  const [list, setList] = useState<Snapshot[]>([]);
  const refresh = useCallback(() => {
    invoke<Snapshot[]>("list_snapshots").then(setList);
  }, []);
  // Collapsed and refreshed every time Settings appears.
  useEffect(refresh, [refresh]);
  usePopoverOpened(
    useCallback(() => {
      setExpanded(false);
      refresh();
    }, [refresh]),
  );

  function restore(s: Snapshot) {
    invoke<Summary>("transfer_summary").then((cur) =>
      show({
        title: l.restoreConfirmTitle,
        message: l.restoreConfirmBody(dateText(s.date, lang), s.dex, compact(s.lifetime), cur.dex, compact(cur.lifetime)),
        confirm: {
          label: l.restoreSnapshotButton,
          run: () =>
            invoke<Summary>("restore_snapshot", { id: s.id })
              .then((r) => show({ title: l.restoreSnapshotButton, message: l.restoreDoneMessage(r.dex, compact(r.lifetime)) }))
              .catch((e) => show({ title: l.restoreSnapshotButton, message: errorText(l, e) }))
              .finally(refresh),
        },
      }),
    );
  }

  return (
    <>
      <div className="setting-row">
        <span className="setting-hint">{l.createSnapshotHint}</span>
        <button
          onClick={() =>
            invoke("create_snapshot")
              .then(() => show({ title: l.createSnapshotButton, message: l.snapshotCreatedToast }))
              .catch((e) => show({ title: l.createSnapshotButton, message: errorText(l, e) }))
              .finally(refresh)
          }
        >
          {l.createSnapshotButton}
        </button>
      </div>
      <button
        className="ptb-disclosure"
        aria-expanded={expanded}
        aria-label={`${l.snapshotsSectionTitle}, ${expanded ? l.historyExpanded : l.historyCollapsed}`}
        onClick={() => setExpanded((e) => !e)}
      >
        <svg width={9} height={9} viewBox="0 0 10 10" aria-hidden>
          <path d={expanded ? "M1 3 L5 7 L9 3" : "M3 1 L7 5 L3 9"} fill="none" stroke="currentColor" strokeWidth={1.6} />
        </svg>
        {l.snapshotsSectionTitle}
      </button>
      {expanded &&
        (list.length === 0 ? (
          <div className="setting-row">
            <span className="setting-hint">{l.noSnapshotsYet}</span>
          </div>
        ) : (
          list.map((s) => (
            <div key={s.id} className="setting-row">
              <span className="ptb-snapshot">
                <img src={s.species == null ? EGG : staticSprite(s.species, s.shiny)} width={28} height={28} alt="" />
                <span className="ptb-stack">
                  <span className="caption">{dateText(s.date, lang)}</span>
                  <span className="caption secondary">{l.snapshotDexAndTokens(s.dex, compact(s.lifetime))}</span>
                </span>
              </span>
              <button className="small" onClick={() => restore(s)}>{l.restoreSnapshotButton}</button>
            </div>
          ))
        ))}
      {dialog}
    </>
  );
}

// ---- About & Support ----

function AboutSupport() {
  const l = useL();
  const [failed, setFailed] = useState(false);
  async function report() {
    const [version, info] = await Promise.all([getVersion(), invoke<{ path: string; os: string }>("log_info")]);
    const url = `mailto:${SUPPORT_MAIL}?subject=${encodeURIComponent(l.reportMailSubject(version))}&body=${encodeURIComponent(
      l.reportMailBody(version, info.os, info.path),
    )}`;
    openUrl(url).then(() => setFailed(false), () => setFailed(true));
  }
  return (
    <>
      <SettingRow label={l.reportProblem} hint={l.reportAttachHint}>
        <button onClick={report}>{l.reportProblem}</button>
      </SettingRow>
      <SettingRow label={l.showLogFile}>
        <button onClick={() => invoke("reveal_log")}>Explorer</button>
      </SettingRow>
      {failed && <div className="ptb-mail-fallback">{l.reportMailFallback(SUPPORT_MAIL)}</div>}
    </>
  );
}

export const settingsRows: SettingsRow[] = [
  { id: "language", group: "general", order: 10, Row: Language },
  { id: "refresh-interval", group: "general", order: 30, Row: RefreshInterval },
  { id: "animation-quality", group: "general", order: 40, Row: Animation },
  { id: "launch-at-login", group: "general", order: 80, Row: LaunchAtLogin },
  { id: "menu-bar", group: "menuBar", order: 110, Row: MenuBar },
  { id: "menu-bar-footer", group: "menuBar", order: 160, Row: MenuBarFooter, footer: true },
  { id: "floating-pet", group: "floatingPet", order: 170, Row: FloatingPet },
  { id: "updates", group: "updates", order: 270, Row: Updates },
  { id: "export-save", group: "backup", order: 300, Row: ExportSave },
  { id: "import-save", group: "backup", order: 310, Row: ImportSave },
  { id: "snapshots", group: "backup", order: 320, Row: Snapshots },
  { id: "about-support", group: "about", order: 440, Row: AboutSupport },
];
