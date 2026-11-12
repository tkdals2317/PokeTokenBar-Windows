// Settings rows owned by companion (pet-settings-misc.md §3.3 rows 2, 9, 25). Collected by src/shell/settings.tsx.
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useLang, useStrings } from "../../kit/i18n";
import { nameOf, ownedSpecies } from "../../shared";
import { SettingRow, SettingsRow, SettingsRowProps, Toggle } from "../../shell/settingsKit";
import { companionStrings } from "./strings";
import { difficultyAtPosition, difficultyPosition, difficultyValue } from "./difficulty";

function Representative({ settings, save, companion }: SettingsRowProps) {
  const t = useStrings(companionStrings);
  const lang = useLang();
  const id = settings.representative;
  const rep = companion && id != null ? ownedSpecies(companion).get(id) : undefined;
  const pinned = rep && companion?.representative_subject?.id === id ? companion.representative_subject : null;
  return (
    <SettingRow label={t.representativeLabel} hint={rep ? undefined : t.representativeChooseFromDex}>
      {rep ? (
        <select
          style={{ width: 150 }}
          value={String(id)}
          onChange={(e) => e.target.value === "" && save({ ...settings, representative: null, representative_form: null })}
        >
          <option value="">{t.representativeFollowCurrent}</option>
          <option value={String(id)}>{`#${id} ${nameOf(rep.names, rep.id, lang)}${pinned?.shiny ? " ✨" : ""}`}</option>
        </select>
      ) : (
        <span className="muted">✓ {t.representativeFollowCurrent}</span>
      )}
    </SettingRow>
  );
}

/** Growth multiplier on a log slider; saved (and banked progress rescaled) only on Save. */
function GrowthDifficulty({ companion }: SettingsRowProps) {
  const t = useStrings(companionStrings);
  const saved = companion?.difficulty ?? 1;
  const [draft, setDraft] = useState(saved);
  useEffect(() => setDraft(saved), [saved]);
  return (
    <>
      <SettingRow label={t.difficultyGrowthLabel} hint={`${t.difficultyGrowthLow} ← → ${t.difficultyGrowthHigh}`}>
        <span className="slider">
          <input
            type="range"
            min={0}
            max={1}
            step={0.001}
            aria-label={t.difficultyGrowthLabel}
            title={t.difficultyScaleHint(t.difficultyGrowthLow, t.difficultyGrowthHigh)}
            value={difficultyPosition(draft)}
            onChange={(e) => setDraft(difficultyAtPosition(Number(e.target.value)))}
          />
          <span className="value">{difficultyValue(draft)}</span>
        </span>
      </SettingRow>
      {draft !== saved && (
        <div className="actions">
          <button className="primary" onClick={() => invoke("set_difficulty", { value: draft })}>{t.save}</button>
        </div>
      )}
    </>
  );
}

function CompanionNotifications({ settings, set }: SettingsRowProps) {
  const t = useStrings(companionStrings);
  return (
    <SettingRow label={t.companionNotificationsLabel}>
      <Toggle on={settings.companion_notifications} set={(v) => set("companion_notifications", v)} label={t.companionNotificationsLabel} />
    </SettingRow>
  );
}

export const settingsRows: SettingsRow[] = [
  { id: "representative", group: "general", order: 20, Row: Representative },
  { id: "growth-difficulty", group: "difficulty", order: 90, Row: GrowthDifficulty },
  { id: "companion-notifications", group: "notifications", order: 250, Row: CompanionNotifications },
];
