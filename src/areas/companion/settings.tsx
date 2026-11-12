// Settings rows owned by companion (pet-settings-misc.md §3.3 rows 2, 9, 25). Collected by src/shell/settings.tsx.
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { nameOf, ownedSpecies, useI18n } from "../../shared";
import { SettingRow, SettingsRow, SettingsRowProps, Toggle } from "../../shell/settingsKit";

// Growth multiplier slider is logarithmic like the original: equal ratios take equal distance.
const [D_MIN, D_MAX] = [0.1, 2.0];
const toPosition = (d: number) => Math.log(d / D_MIN) / Math.log(D_MAX / D_MIN);
function fromPosition(p: number) {
  if (Math.abs(p - toPosition(1)) < 0.01) return 1; // snap to default
  const v = D_MIN * Math.pow(D_MAX / D_MIN, p);
  const mag = Math.pow(10, Math.floor(Math.log10(v)) - 1);
  return Math.round(v / mag) * mag; // two significant digits
}

function Representative({ settings, set, companion }: SettingsRowProps) {
  const { t, lang } = useI18n();
  const rep = companion && settings.representative != null ? ownedSpecies(companion).get(settings.representative) : undefined;
  return (
    <SettingRow label={t.representative} hint={t.chooseInDex}>
      {rep ? (
        <button onClick={() => set("representative", null)} title={t.followCurrent}>
          ★ {nameOf(rep.names, rep.id, lang)} ✕
        </button>
      ) : (
        <span className="muted">{t.followCurrent}</span>
      )}
    </SettingRow>
  );
}

function GrowthDifficulty({ companion }: SettingsRowProps) {
  const { t } = useI18n();
  const saved = companion?.difficulty ?? 1;
  const [difficulty, setDifficulty] = useState(saved);
  useEffect(() => setDifficulty(saved), [saved]);
  return (
    <>
      <SettingRow label={t.growthDifficulty} hint={`${t.faster} ← → ${t.slower}`}>
        <span className="slider">
          <input
            type="range"
            min={0}
            max={1}
            step={0.001}
            value={toPosition(difficulty)}
            onChange={(e) => setDifficulty(fromPosition(Number(e.target.value)))}
          />
          <span className="value">×{difficulty.toFixed(difficulty < 1 ? 2 : 1)}</span>
        </span>
      </SettingRow>
      {difficulty !== saved && (
        <div className="actions">
          <button className="primary" onClick={() => invoke("set_difficulty", { value: difficulty })}>{t.save}</button>
        </div>
      )}
    </>
  );
}

function CompanionNotifications({ settings, set }: SettingsRowProps) {
  const { t } = useI18n();
  return (
    <SettingRow label={t.companionNotifications}>
      <Toggle on={settings.companion_notifications} set={(v) => set("companion_notifications", v)} />
    </SettingRow>
  );
}

export const settingsRows: SettingsRow[] = [
  { id: "representative", group: "general", order: 20, Row: Representative },
  { id: "growth-difficulty", group: "difficulty", order: 90, Row: GrowthDifficulty },
  { id: "companion-notifications", group: "notifications", order: 250, Row: CompanionNotifications },
];
