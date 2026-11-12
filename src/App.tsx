// Popover shell (ui-shell.md §5.3): update banner, Home/Shop/Bag/Collection tabs, footer,
// Settings replacing the content. Owner: foundation. Tab bodies are owned by their areas.
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import "./App.css";
import "./shell/shell.css";
import { legacyDict } from "./i18n";
import { Segmented } from "./kit/components";
import { LangContext, pick } from "./kit/i18n";
import { Footer, UpdateBanner } from "./shell/chrome";
import { SettingsScreen } from "./shell/settings";
import { shellStrings } from "./shell/strings";
import { I18n, PopoverOpened, Tab, TabProps, useBackend, usePopoverOpened, useUpdater } from "./shared";
import HomeTab from "./areas/home-usage/HomeTab";
import ShopTab from "./areas/shop-bag-dex/ShopTab";
import BagTab from "./areas/shop-bag-dex/BagTab";
import CollectionTab from "./areas/shop-bag-dex/CollectionTab";

/** The popover has no title bar: pressing on an element marked `data-drag` itself (not on a
 * control inside it) moves the window, and the shell then stops re-anchoring it to the tray. */
function useWindowDrag() {
  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      if (e.button !== 0 || !(e.target instanceof HTMLElement) || !e.target.hasAttribute("data-drag")) return;
      e.preventDefault();
      invoke("popover_drag_started");
      getCurrentWindow().startDragging();
    };
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, []);
}

const TABS: Record<Tab, (p: TabProps) => React.ReactNode> = {
  home: HomeTab,
  shop: ShopTab,
  bag: BagTab,
  collection: CollectionTab,
};

export default function App() {
  const { usage, companion, limits, settings, saveSettings } = useBackend();
  const updater = useUpdater();
  const [tab, setTab] = useState<Tab>("home");
  const [showSettings, setShowSettings] = useState(false);

  // Every open lands on Home with Settings closed, unless the tray menu asked for Pokédex/Settings.
  usePopoverOpened(
    useCallback((p: PopoverOpened) => {
      setTab(p.tab ?? "home");
      setShowSettings(p.settings);
    }, []),
  );
  useEffect(() => {
    invoke("set_popover_mode", { settings: showSettings });
  }, [showSettings]);
  const closeSettings = useCallback(() => setShowSettings(false), []);
  useWindowDrag();

  if (!settings) return null;
  const lang = settings.language;
  const t = pick(shellStrings, lang);
  const legacy = legacyDict(lang);
  const Body = TABS[tab];

  return (
    <LangContext.Provider value={lang}>
      <I18n.Provider value={{ lang, t: legacy }}>
        {showSettings ? (
          <SettingsScreen settings={settings} save={saveSettings} companion={companion} updater={updater} onClose={closeSettings} />
        ) : (
          <div className="popover" data-drag>
            <div className="drag-handle" data-drag title={t.dragToMove} />
            <UpdateBanner updater={updater} settings={settings} save={saveSettings} />
            <Segmented
              options={[["home", t.home], ["shop", t.shop], ["bag", t.bag], ["collection", t.collection]]}
              value={tab}
              onChange={setTab}
            />
            <main className="tab-body">
              <Body usage={usage} companion={companion} limits={limits} settings={settings} save={saveSettings} goTo={setTab} />
            </main>
            <Footer tab={tab} usage={usage} settings={settings} save={saveSettings} openSettings={() => setShowSettings(true)} />
          </div>
        )}
      </I18n.Provider>
    </LangContext.Provider>
  );
}
