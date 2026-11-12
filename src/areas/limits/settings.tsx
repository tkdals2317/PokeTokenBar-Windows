// Settings rows owned by limits (pet-settings-misc.md §3.3 rows 5, 22-24). Collected by src/shell/settings.tsx.
import { Segmented } from "../../kit/components";
import { useI18n } from "../../shared";
import { SettingRow, SettingsRow, SettingsRowProps, Toggle } from "../../shell/settingsKit";

function LimitDisplay({ settings, set }: SettingsRowProps) {
  const { t } = useI18n();
  return (
    <SettingRow label={t.limitDisplay}>
      <Segmented
        options={[["used", t.used], ["remaining", t.remaining]]}
        value={settings.limit_display}
        onChange={(v) => set("limit_display", v)}
      />
    </SettingRow>
  );
}

function LimitAlerts({ settings: s, set }: SettingsRowProps) {
  const { t } = useI18n();
  return (
    <>
      <SettingRow label={t.limitNotifications}>
        <Toggle on={s.limit_notifications} set={(v) => set("limit_notifications", v)} />
      </SettingRow>
      {s.limit_notifications && (
        <>
          <SettingRow label={t.warning}>
            <span className="slider">
              <input type="range" min={50} max={95} step={5} value={s.warn_threshold}
                onChange={(e) => set("warn_threshold", Number(e.target.value))} />
              <span className="value">{s.warn_threshold}%</span>
            </span>
          </SettingRow>
          <SettingRow label={t.critical}>
            <span className="slider">
              <input type="range" min={80} max={100} step={5} value={s.crit_threshold}
                onChange={(e) => set("crit_threshold", Number(e.target.value))} />
              <span className="value">{s.crit_threshold}%</span>
            </span>
          </SettingRow>
        </>
      )}
    </>
  );
}

export const settingsRows: SettingsRow[] = [
  { id: "limit-display", group: "general", order: 50, Row: LimitDisplay },
  { id: "limit-alerts", group: "notifications", order: 220, Row: LimitAlerts },
];
