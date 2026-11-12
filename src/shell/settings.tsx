// Settings screen container (ui-shell.md §5.5, pet-settings-misc.md §5.4). Owner: foundation.
// Areas add rows by exporting `settingsRows: SettingsRow[]` from `src/areas/<area>/settings.tsx`;
// they are collected here with import.meta.glob, so no shared file changes.
import { useEffect, useMemo, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Icon } from "../kit/components";
import { useStrings } from "../kit/i18n";
import { GROUPS, SettingsRow, SettingsRowProps } from "./settingsKit";
import { shellStrings } from "./strings";
import { petStrings } from "../areas/pet-settings-misc/strings";

const modules = import.meta.glob<{ settingsRows?: SettingsRow[] }>("../areas/*/settings.tsx", { eager: true });
const ROWS = Object.values(modules)
  .flatMap((m) => m.settingsRows ?? [])
  .sort((a, b) => a.order - b.order);


export function SettingsScreen(props: Omit<SettingsRowProps, "set"> & { onClose: () => void }) {
  const t = useStrings(shellStrings);
  const p = useStrings(petStrings);
  const links: [string, string][] = [
    ["GitHub", "https://github.com/tkdals2317/PokeTokenBar-Windows"],
    [p.website, "https://chattymin.github.io/PokeTokenBar/"],
    [`♥ ${p.sponsor}`, "https://github.com/sponsors/chattymin"],
  ];
  const [version, setVersion] = useState("");
  useEffect(() => {
    getVersion().then(setVersion);
    const esc = (e: KeyboardEvent) => e.key === "Escape" && props.onClose();
    window.addEventListener("keydown", esc);
    return () => window.removeEventListener("keydown", esc);
  }, [props.onClose]);

  const rowProps: SettingsRowProps = {
    ...props,
    set: (k, v) => props.save({ ...props.settings, [k]: v }),
  };
  const groups = useMemo(() => GROUPS.map((g) => [g, ROWS.filter((r) => r.group === g)] as const), []);

  return (
    <div className="settings-screen">
      <header className="settings-header">
        <button className="back" onClick={props.onClose}>
          <Icon name="chevronLeft" size={12} /> {t.back}
        </button>
        <strong>{t.settings}</strong>
        <span className="back" aria-hidden style={{ visibility: "hidden" }}>
          <Icon name="chevronLeft" size={12} /> {t.back}
        </span>
      </header>
      <hr className="divider" />
      <div className="settings-body">
        {groups.map(
          ([g, rows]) =>
            rows.length > 0 && (
              <section key={g}>
                <h3>{t[g]}</h3>
                <div className="settings-card">
                  {rows.filter((r) => !r.footer).map(({ id, Row }) => (
                    <Row key={id} {...rowProps} />
                  ))}
                </div>
                {rows.filter((r) => r.footer).map(({ id, Row }) => (
                  <Row key={id} {...rowProps} />
                ))}
              </section>
            ),
        )}
      </div>
      <hr className="divider" />
      <footer className="settings-footer">
        <span>v{version}</span>
        {links.map(([label, url]) => (
          <span key={url}>
            {" · "}
            <button className="link" title={url} onClick={() => openUrl(url)}>
              {label}
            </button>
          </span>
        ))}
      </footer>
    </div>
  );
}
