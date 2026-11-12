// Popover chrome: update banner (ui-shell.md §5.6) and footer (§5.4). Owner: foundation.
import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { IconButton, Tooltip, Icon } from "../kit/components";
import { relativeDuration } from "../kit/format";
import { useLang, useStrings } from "../kit/i18n";
import { Settings, Tab, Updater, UsageSnapshot } from "../shared";
import { shellStrings } from "./strings";

export function UpdateBanner({ updater, settings, save }: { updater: Updater; settings: Settings; save: (s: Settings) => void }) {
  const t = useStrings(shellStrings);
  const [current, setCurrent] = useState("");
  useEffect(() => {
    getVersion().then(setCurrent);
  }, []);
  const s = updater.state;
  if (s.status !== "available" && s.status !== "downloading") return null;
  if (!settings.update_notifications_enabled || settings.skipped_update_version === s.update.version) return null;
  return (
    <div className="update-banner">
      <div className="caption">{t.updateAvailable(s.update.version, current)}</div>
      <div className="update-actions">
        {s.status === "downloading" ? (
          <span className="caption secondary">{t.updating}</span>
        ) : (
          <>
            <button className="link secondary" onClick={() => save({ ...settings, skipped_update_version: s.update.version })}>
              {t.skipThisVersion}
            </button>
            <button className="prominent" onClick={updater.install}>
              {t.update}
            </button>
          </>
        )}
      </div>
    </div>
  );
}

/** "Updated 2 min, 5 sec", re-rendered every second. */
function Updated({ at }: { at: string }) {
  const t = useStrings(shellStrings);
  const lang = useLang();
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, []);
  return (
    <span className="caption tertiary num">
      {t.updated} {relativeDuration((now - Date.parse(at)) / 1000, lang)}
    </span>
  );
}

export function Footer(props: {
  tab: Tab;
  usage: UsageSnapshot | null;
  settings: Settings;
  save: (s: Settings) => void;
  openSettings: () => void;
}) {
  const t = useStrings(shellStrings);
  const { usage, settings } = props;
  const petOn = settings.floating_pet_enabled;
  return (
    <footer className="popover-footer">
      {props.tab === "home" && (
        <>
          <IconButton icon="refresh" label={t.refreshNow} onClick={() => invoke("refresh_now")} />
          {usage?.fetched_at && <Updated at={usage.fetched_at} />}
          {usage?.error && (
            <Tooltip text={`${t.usageRefreshError}\n${usage.error}`}>
              <span style={{ color: "var(--orange)", display: "inline-flex" }}>
                <Icon name="warning" />
              </span>
            </Tooltip>
          )}
        </>
      )}
      <span style={{ flex: 1 }} />
      <IconButton
        icon={petOn ? "eye" : "eyeSlash"}
        label={petOn ? t.hidePet : t.showPet}
        onClick={() => props.save({ ...settings, floating_pet_enabled: !petOn })}
      />
      <IconButton icon="gear" label={t.settings} onClick={props.openSettings} />
      <IconButton icon="power" label={t.quit} onClick={() => invoke("quit")} />
    </footer>
  );
}
