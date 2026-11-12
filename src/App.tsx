// Popover shell (ui-shell.md §5.3): update banner, Home/Shop/Bag/Collection tabs, footer,
// Settings replacing the content. Owner: foundation. Tab bodies are owned by their areas.
import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./App.css";
import "./shell/shell.css";
import { legacyDict, LegacyDict } from "./i18n";
import { Segmented } from "./kit/components";
import { LangContext, Lang, pick } from "./kit/i18n";
import { Footer, UpdateBanner } from "./shell/chrome";
import { SettingsScreen } from "./shell/settings";
import { shellStrings } from "./shell/strings";
import { Companion, CompanionEvent, I18n, PopoverOpened, Tab, TabProps, nameOf, useBackend, usePopoverOpened, useUpdater } from "./shared";
import HomeTab from "./areas/home-usage/HomeTab";
import ShopTab from "./areas/shop-bag-dex/ShopTab";
import BagTab from "./areas/shop-bag-dex/BagTab";
import CollectionTab from "./areas/shop-bag-dex/CollectionTab";

const TABS: Record<Tab, (p: TabProps) => React.ReactNode> = {
  home: HomeTab,
  shop: ShopTab,
  bag: BagTab,
  collection: CollectionTab,
};

// ponytail: Windows-only hatch/evolve toast kept from v0.2 (ui-shell.md §8 Q4); companion area decides.
function eventText(e: CompanionEvent, c: Companion | null, t: LegacyDict, lang: Lang) {
  const names = c?.active?.line.names ?? c?.dex[c.dex.length - 1]?.names ?? {};
  const name = nameOf(names, e.id, lang);
  if (e.kind === "hatched") return e.shiny ? t.shinyHatched(name) : t.hatched(name);
  if (e.kind === "evolved") return t.evolved(name);
  return t.graduated(name);
}

export default function App() {
  const { usage, companion, limits, settings, saveSettings } = useBackend();
  const updater = useUpdater();
  const [tab, setTab] = useState<Tab>("home");
  const [showSettings, setShowSettings] = useState(false);
  const [toast, setToast] = useState<CompanionEvent | null>(null);

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
  useEffect(() => {
    const sub = listen<CompanionEvent>("companion-event", (e) => {
      setToast(e.payload);
      setTimeout(() => setToast(null), 6000);
    });
    return () => {
      sub.then((f) => f());
    };
  }, []);
  const closeSettings = useCallback(() => setShowSettings(false), []);

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
          <div className="popover">
            <UpdateBanner updater={updater} settings={settings} save={saveSettings} />
            <Segmented
              options={[["home", t.home], ["shop", t.shop], ["bag", t.bag], ["collection", t.collection]]}
              value={tab}
              onChange={setTab}
            />
            <main className="tab-body">
              {toast && <div className="toast">{eventText(toast, companion, legacy, lang)}</div>}
              <Body usage={usage} companion={companion} limits={limits} settings={settings} save={saveSettings} goTo={setTab} />
            </main>
            <Footer tab={tab} usage={usage} settings={settings} save={saveSettings} openSettings={() => setShowSettings(true)} />
          </div>
        )}
      </I18n.Provider>
    </LangContext.Provider>
  );
}
