//! Floating pet window, animated tray sprite, autostart and other app services.
//! Owner: pet-settings-misc area.

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;

/// Keys this area owns inside `settings.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PetSettings {
    #[serde(alias = "pet_enabled")]
    pub floating_pet_enabled: bool,
    pub always_faces_back: bool,
    /// 48…384 step 8
    pub floating_pet_size: u32,
    pub floating_pet_bubble_alerts: bool,
    pub floating_pet_agent_activity: bool,
    /// "powerSaver" | "balanced" | "smooth" — tray sprite frame floor 0.4 / 0.2 / 0.1 s
    pub animation_quality: String,
}

impl Default for PetSettings {
    fn default() -> Self {
        PetSettings {
            floating_pet_enabled: false,
            always_faces_back: false,
            floating_pet_size: 96,
            floating_pet_bubble_alerts: true,
            floating_pet_agent_activity: true,
            animation_quality: "powerSaver".into(),
        }
    }
}

impl PetSettings {
    pub fn sanitize(&mut self) {
        self.floating_pet_size = self.floating_pet_size.clamp(48, 384);
        if !["powerSaver", "balanced", "smooth"].contains(&self.animation_quality.as_str()) {
            self.animation_quality = "powerSaver".into();
        }
    }
}

pub fn setup(_app: &tauri::App) -> tauri::Result<()> {
    Ok(())
}

/// Called by the shell whenever settings change (and once at launch).
pub fn on_settings(app: &AppHandle, s: &crate::settings::Settings) {
    if let Some(w) = app.get_webview_window("pet") {
        if s.pet.floating_pet_enabled {
            crate::shell::reveal(&w, false);
        } else {
            let _ = w.hide();
        }
    }
}

/// Tray sprite hook, called once after the tray exists. Stub: the tray keeps the app icon.
/// Owner fills it in (ui-shell.md §3.5/§6): a timer thread calling `crate::shell::tray::set_icon`,
/// pausing while `crate::shell::popover_visible(app)` is true.
pub fn start_tray_sprite(_app: &AppHandle) {}

#[tauri::command]
fn get_autostart(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn set_autostart(app: AppHandle, enabled: bool) -> Result<bool, String> {
    let launcher = app.autolaunch();
    let r = if enabled { launcher.enable() } else { launcher.disable() };
    r.map_err(|e| e.to_string())?;
    Ok(launcher.is_enabled().unwrap_or(enabled))
}

crate::area_commands![get_autostart, set_autostart];
