import { useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Menu } from "@tauri-apps/api/menu";
import { getCurrentWindow } from "@tauri-apps/api/window";
// Floating pet window. Owner: pet-settings-misc area.
import { compact } from "../../kit/format";
import { legacyDict } from "../../i18n";
import { EGG, displayed, nameOf, petSize, resizePet, sprite, todayTotal, useBackend } from "../../shared";

const DRAG_SLOP = 4;
const MENU_SIZES = [48, 72, 96, 120, 160, 224, 288, 384];
const toggle = (label: string) => invoke("toggle_window", { label });
/** Pet click opens the popover (never toggles it off). */
const openPopover = () => invoke("show_popover");

const MENU = {
  ko: { open: "창 열기", size: "크기", hide: "펫 숨기기", quit: "종료" },
  en: { open: "Open window", size: "Size", hide: "Hide pet", quit: "Quit" },
  ja: { open: "ウィンドウを開く", size: "サイズ", hide: "ペットを隠す", quit: "終了" },
};

/** Floating Pokémon: drag to move, click to open the popover, right-click for a menu. */
export default function Pet() {
  const { companion, usage, settings } = useBackend();
  const press = useRef<{ x: number; y: number } | null>(null);
  const lang = settings?.language ?? "en";
  const t = legacyDict(lang);

  const shown = companion ? displayed(companion, settings) : null;
  const src = shown ? sprite(shown.id, shown.shiny) : EGG;
  const name = shown ? nameOf(shown.names, shown.id, lang) : t.egg;
  const title = usage ? `${name} · ${t.today} ${compact(todayTotal(usage))}` : name;

  async function showMenu(e: React.MouseEvent) {
    e.preventDefault();
    const current = await petSize();
    const m = MENU[lang as keyof typeof MENU] ?? MENU.en;
    const menu = await Menu.new({
      items: [
        { id: "open", text: m.open, action: openPopover },
        {
          text: m.size,
          items: MENU_SIZES.map((px) => ({
            id: `size-${px}`,
            text: `${px}px`,
            checked: px === current,
            action: () => resizePet(px),
          })),
        },
        { id: "hide", text: m.hide, action: () => toggle("pet") },
        { id: "quit", text: m.quit, action: () => invoke("quit") },
      ],
    });
    await menu.popup();
  }

  return (
    <div
      className="pet"
      onContextMenu={showMenu}
      onMouseDown={(e) => {
        if (e.button === 0) press.current = { x: e.screenX, y: e.screenY };
      }}
      onMouseMove={(e) => {
        const p = press.current;
        if (p && Math.hypot(e.screenX - p.x, e.screenY - p.y) > DRAG_SLOP) {
          press.current = null;
          getCurrentWindow().startDragging();
        }
      }}
      onMouseUp={() => {
        if (press.current) openPopover();
        press.current = null;
      }}
    >
      <img src={src} alt={name} title={title} draggable={false} />
    </div>
  );
}
