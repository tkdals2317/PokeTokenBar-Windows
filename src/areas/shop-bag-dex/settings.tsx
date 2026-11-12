// Settings row owned by shop-bag-dex (pet-settings-misc.md §3.3 row 10). Collected by src/shell/settings.tsx.
import { useEffect, useState } from "react";
import { useStrings } from "../../kit/i18n";
import { SettingRow, SettingsRow, SettingsRowProps } from "../../shell/settingsKit";
import { difficultyAtPosition, difficultyPosition, difficultyValue } from "../companion/difficulty";
import { sbdStrings } from "./strings";

/** Price multiplier on the same log slider as Growth; saved only on Save (a pure multiplier). */
function ShopDifficulty({ settings, save }: SettingsRowProps) {
  const t = useStrings(sbdStrings);
  const saved = settings.shop_difficulty;
  const [draft, setDraft] = useState(saved);
  useEffect(() => setDraft(saved), [saved]);
  return (
    <>
      <SettingRow label={t.difficultyShopLabel} hint={`${t.difficultyShopLow} ← → ${t.difficultyShopHigh}`}>
        <span className="slider">
          <input
            type="range"
            min={0}
            max={1}
            step={0.001}
            aria-label={t.difficultyShopLabel}
            title={t.difficultyScaleHint(t.difficultyShopLow, t.difficultyShopHigh)}
            value={difficultyPosition(draft)}
            onChange={(e) => setDraft(difficultyAtPosition(Number(e.target.value)))}
          />
          <span className="value">{difficultyValue(draft)}</span>
        </span>
      </SettingRow>
      {draft !== saved && (
        <div className="actions">
          <button className="primary" onClick={() => save({ ...settings, shop_difficulty: draft })}>{t.save}</button>
        </div>
      )}
    </>
  );
}

export const settingsRows: SettingsRow[] = [{ id: "shop-difficulty", group: "difficulty", order: 100, Row: ShopDifficulty }];
