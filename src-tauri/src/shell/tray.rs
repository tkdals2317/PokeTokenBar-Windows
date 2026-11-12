//! Tray icon: left click toggles the popover, right click shows the status menu,
//! tooltip carries the macOS menu bar text. ui-shell.md §3.1, §3.6, §6.

use super::i18n::{self, tr};
use super::{popover, TRAY_ID};
use crate::format;
use crate::model::PopoverOpened;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::AppHandle;

/// Windows `NOTIFYICONDATA.szTip` holds 127 characters.
const TOOLTIP_MAX: usize = 127;

/// `UsageStore.menuLines`: at most two lines, the limit line always last and on its own.
pub fn menu_lines(
    refreshed: bool,
    tokens: Option<String>,
    cost: Option<String>,
    growth: Option<String>,
    limit: Option<String>,
) -> Vec<String> {
    if !refreshed {
        return vec![growth.unwrap_or_else(|| "—".into())];
    }
    let usage: Vec<String> = [tokens, cost, growth].into_iter().flatten().collect();
    if limit.is_some() && usage.len() >= 2 {
        return vec![usage.join(" · "), limit.unwrap()];
    }
    if usage.len() > 2 {
        return vec![usage[..usage.len() - 1].join(" · "), usage[usage.len() - 1].clone()];
    }
    usage.into_iter().chain(limit).collect()
}

fn tooltip(app: &AppHandle) -> String {
    let s = crate::settings::current(app);
    let snap = crate::providers::current(app);
    let day = crate::model::today_key();
    let lang = s.shell.language.as_str();
    let (cost, coverage) = snap.today_cost(&day);
    let lines = menu_lines(
        snap.fetched_at.is_some(),
        s.shell.show_tokens_in_menu.then(|| format::compact(snap.today_total(&day) as i64)),
        (s.shell.show_cost_in_menu && snap.shows_cost())
            .then(|| format::cost_text(cost, coverage, true, tr(&i18n::COST_UNAVAILABLE, lang))),
        s.shell.show_growth_in_menu.then(|| crate::companion::menu_growth_text(app, lang)),
        crate::limits::menu_limit_line(app, &s),
    );
    let text: String = std::iter::once("PokeTokenBar".to_string()).chain(lines).collect::<Vec<_>>().join("\n");
    text.chars().take(TOOLTIP_MAX).collect()
}

fn build_menu(app: &AppHandle) -> tauri::Result<Menu<tauri::Wry>> {
    let s = crate::settings::current(app);
    let lang = s.shell.language.as_str();
    let snap = crate::providers::current(app);
    let day = crate::model::today_key();
    let mut summary = format!("{} {}", tr(&i18n::TODAY_TOKENS, lang), format::compact(snap.today_total(&day) as i64));
    if snap.shows_cost() {
        let (cost, coverage) = snap.today_cost(&day);
        summary += &format!(" · {}", format::cost_text(cost, coverage, false, tr(&i18n::COST_UNAVAILABLE, lang)));
    }
    let pet = if s.pet.floating_pet_enabled { &i18n::HIDE_PET } else { &i18n::SHOW_PET };
    Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "summary", summary, false, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "refresh", tr(&i18n::REFRESH_NOW, lang), true, Some("CmdOrCtrl+R"))?,
            &MenuItem::with_id(app, "dex", tr(&i18n::DEX, lang), true, Some("CmdOrCtrl+D"))?,
            &MenuItem::with_id(app, "settings", format!("{}…", tr(&i18n::SETTINGS, lang)), true, Some("CmdOrCtrl+,"))?,
            &MenuItem::with_id(app, "pet", tr(pet, lang), true, None::<&str>)?,
            &CheckMenuItem::with_id(app, "back", tr(&i18n::BACK_VIEW, lang), true, s.pet.always_faces_back, None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", tr(&i18n::QUIT, lang), true, Some("CmdOrCtrl+Q"))?,
        ],
    )
}

fn on_menu(app: &AppHandle, id: &str) {
    let mut s = crate::settings::current(app);
    match id {
        "refresh" => super::refresh_now(app.clone()),
        "dex" => popover::show(app, PopoverOpened { tab: Some("collection".into()), settings: false }),
        "settings" => popover::show(app, PopoverOpened { tab: None, settings: true }),
        "pet" => {
            s.pet.floating_pet_enabled = !s.pet.floating_pet_enabled;
            super::apply_settings(app, s);
        }
        "back" => {
            s.pet.always_faces_back = !s.pet.always_faces_back;
            super::apply_settings(app, s);
        }
        "quit" => app.exit(0),
        _ => {}
    }
}

pub fn create(app: &tauri::App) -> tauri::Result<()> {
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&build_menu(app.handle())?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, e| on_menu(app, e.id.as_ref()))
        .on_tray_icon_event(|tray, e| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                popover::toggle_from_tray(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Refreshes tooltip and menu from current settings, usage, limits and companion.
pub fn update(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    let _ = tray.set_tooltip(Some(tooltip(app)));
    if let Ok(menu) = build_menu(app) {
        let _ = tray.set_menu(Some(menu));
    }
}

/// Extension point for the animated tray sprite (pet-settings-misc). `None` restores the app icon.
#[allow(dead_code)] // called by pet::start_tray_sprite once it lands
pub fn set_icon(app: &AppHandle, frame: Option<Image<'static>>) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    let icon = frame.or_else(|| app.default_window_icon().cloned());
    let _ = tray.set_icon(icon);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn o(s: &str) -> Option<String> {
        Some(s.into())
    }

    #[test]
    fn menu_lines_follow_usage_store() {
        assert_eq!(menu_lines(false, o("1M"), None, None, None), ["—"]);
        assert_eq!(menu_lines(false, None, None, o("Growth 4%"), None), ["Growth 4%"]);
        assert_eq!(menu_lines(true, o("1.2M"), None, o("Growth 42%"), None), ["1.2M", "Growth 42%"]);
        assert_eq!(menu_lines(true, o("1M"), o("$2"), None, o("Claude 80%")), ["1M · $2", "Claude 80%"]);
        assert_eq!(menu_lines(true, o("1M"), o("$2"), o("G 1%"), None), ["1M · $2", "G 1%"]);
        assert_eq!(menu_lines(true, o("1M"), None, None, o("Codex 5%")), ["1M", "Codex 5%"]);
        assert!(menu_lines(true, None, None, None, None).is_empty());
    }
}
