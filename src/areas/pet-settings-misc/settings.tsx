// Settings rows owned by pet-settings-misc (pet-settings-misc.md §3.3). Collected by src/shell/settings.tsx.
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { LANG_LABELS, LANGS, Lang } from "../../kit/i18n";
import { PET_SIZE, petSize, resizePet, useI18n } from "../../shared";
import { SettingRow, SettingsRow, SettingsRowProps, Toggle } from "../../shell/settingsKit";

const INTERVALS = [1, 2, 5, 10, 15, 0];

function Language({ settings, set }: SettingsRowProps) {
  const { t } = useI18n();
  return (
    <SettingRow label={t.language}>
      <select value={settings.language} onChange={(e) => set("language", e.target.value as Lang)}>
        {LANGS.map((l) => (
          <option key={l} value={l}>{LANG_LABELS[l]}</option>
        ))}
      </select>
    </SettingRow>
  );
}

function RefreshInterval({ settings, set }: SettingsRowProps) {
  const { t } = useI18n();
  return (
    <SettingRow label={t.refreshInterval}>
      <select value={settings.refresh_minutes} onChange={(e) => set("refresh_minutes", Number(e.target.value))}>
        {INTERVALS.map((m) => (
          <option key={m} value={m}>{m === 0 ? t.manual : t.everyMinutes(m)}</option>
        ))}
      </select>
    </SettingRow>
  );
}

function LaunchAtLogin() {
  const { t } = useI18n();
  const [on, setOn] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    invoke<boolean>("get_autostart").then(setOn);
  }, []);
  return (
    <SettingRow label={t.launchAtLogin} hint={error || undefined}>
      <Toggle
        on={on}
        set={(v) =>
          invoke<boolean>("set_autostart", { enabled: v })
            .then((r) => {
              setOn(r);
              setError("");
            })
            .catch((e) => setError(String(e)))
        }
      />
    </SettingRow>
  );
}

function MenuBarTokens({ settings, set }: SettingsRowProps) {
  const { t } = useI18n();
  return (
    <SettingRow label={t.trayTokens}>
      <Toggle on={settings.show_tokens_in_menu} set={(v) => set("show_tokens_in_menu", v)} />
    </SettingRow>
  );
}

function MenuBarLimit({ settings, set }: SettingsRowProps) {
  const { t } = useI18n();
  return (
    <SettingRow label={t.trayLimit} hint={t.trayHint}>
      <Toggle on={settings.show_limit_in_menu} set={(v) => set("show_limit_in_menu", v)} />
    </SettingRow>
  );
}

function FloatingPet({ settings, set }: SettingsRowProps) {
  const { t } = useI18n();
  const [size, setSize] = useState(120);
  useEffect(() => {
    petSize().then(setSize);
  }, []);
  return (
    <>
      <SettingRow label={t.petEnabled}>
        <Toggle on={settings.floating_pet_enabled} set={(v) => set("floating_pet_enabled", v)} />
      </SettingRow>
      {settings.floating_pet_enabled && (
        <SettingRow label={t.petSize}>
          <span className="slider">
            <input
              type="range"
              min={PET_SIZE.min}
              max={PET_SIZE.max}
              step={PET_SIZE.step}
              value={size}
              onChange={(e) => {
                const px = Number(e.target.value);
                setSize(px);
                resizePet(px);
              }}
            />
            <span className="value">{size}px</span>
          </span>
        </SettingRow>
      )}
    </>
  );
}

function CheckForUpdates({ updater }: SettingsRowProps) {
  const { t } = useI18n();
  const [version, setVersion] = useState("");
  useEffect(() => {
    getVersion().then(setVersion);
  }, []);
  const s = updater.state;
  const status =
    s.status === "checking" ? t.checking
    : s.status === "none" ? t.upToDate
    : s.status === "error" ? t.updateError
    : s.status === "available" ? t.updateAvailable(s.update.version)
    : s.status === "downloading" ? t.downloading(Math.round(s.progress * 100))
    : "";
  return (
    <SettingRow label={t.currentVersion(version)} hint={status}>
      {s.status === "available" ? (
        <button className="primary" onClick={updater.install}>{t.updateNow}</button>
      ) : (
        <button disabled={s.status === "checking" || s.status === "downloading"} onClick={updater.checkNow}>
          {t.checkUpdates}
        </button>
      )}
    </SettingRow>
  );
}

export const settingsRows: SettingsRow[] = [
  { id: "language", group: "general", order: 10, Row: Language },
  { id: "refresh-interval", group: "general", order: 30, Row: RefreshInterval },
  { id: "launch-at-login", group: "general", order: 80, Row: LaunchAtLogin },
  { id: "menu-tokens", group: "menuBar", order: 110, Row: MenuBarTokens },
  { id: "menu-limit", group: "menuBar", order: 140, Row: MenuBarLimit },
  { id: "floating-pet", group: "floatingPet", order: 170, Row: FloatingPet },
  { id: "check-updates", group: "updates", order: 280, Row: CheckForUpdates },
];
