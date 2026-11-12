// Types and primitives for settings rows. Owner: foundation. Areas import from here.
import { ComponentType, ReactNode } from "react";
import { Companion, Settings, Updater } from "../shared";

/** Groups in render order. */
export const GROUPS = [
  "general", "difficulty", "menuBar", "providers", "floatingPet", "notifications", "updates", "backup", "advanced", "about",
] as const;
export type SettingsGroup = (typeof GROUPS)[number];

export type SettingsRowProps = {
  settings: Settings;
  set: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
  save: (next: Settings) => void;
  companion: Companion | null;
  updater: Updater;
};

/** One row (or a few rows that belong together). `order` sorts within the group; use pet-settings-misc.md §3.3 # × 10. */
/** `footer` rows render under the group's card (caption hints like "All off shows only the character"). */
export type SettingsRow = { id: string; group: SettingsGroup; order: number; Row: ComponentType<SettingsRowProps>; footer?: boolean };

/** Row primitive: label + optional hint on the left, control on the right (padding 12/8, min height 38). */
export function SettingRow({ label, hint, children }: { label: ReactNode; hint?: ReactNode; children?: ReactNode }) {
  return (
    <div className="setting-row">
      <div className="setting-label">
        <div>{label}</div>
        {hint && <div className="setting-hint">{hint}</div>}
      </div>
      {children}
    </div>
  );
}

export const Toggle = ({ on, set, label }: { on: boolean; set: (v: boolean) => void; label?: string }) => (
  <input type="checkbox" role="switch" aria-label={label} className="switch" checked={on} onChange={(e) => set(e.target.checked)} />
);

