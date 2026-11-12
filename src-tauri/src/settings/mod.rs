//! User settings, saved as one flat `settings.json` in the app data folder.
//! Owner: foundation. Each area owns its own flattened block (its `*Settings` struct lives in the
//! area module), so adding a key never touches this file. Key names must be unique across blocks.

use crate::store::{load_json, save_json};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

pub const LANGUAGES: [&str; 8] = ["ko", "en", "ja", "es", "fr", "pt", "de", "ru"];

/// Keys the shell reads: language, refresh cadence, what the tray tooltip shows, update banner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShellSettings {
    /// ko | en | ja | es | fr | pt | de | ru
    pub language: String,
    /// 0 = manual refresh only. Options 0, 1, 2, 5, 15.
    pub refresh_minutes: u32,
    #[serde(alias = "tray_tokens")]
    pub show_tokens_in_menu: bool,
    pub show_cost_in_menu: bool,
    pub show_growth_in_menu: bool,
    #[serde(alias = "tray_limit")]
    pub show_limit_in_menu: bool,
    /// "gauge" | "attention" | "off" (kept for parity; the Windows tooltip has no colours)
    pub menu_limit_color_mode: String,
    pub update_notifications_enabled: bool,
    pub skipped_update_version: Option<String>,
}

impl Default for ShellSettings {
    fn default() -> Self {
        ShellSettings {
            language: system_language(),
            refresh_minutes: 2,
            show_tokens_in_menu: true,
            show_cost_in_menu: false,
            show_growth_in_menu: false,
            show_limit_in_menu: false,
            menu_limit_color_mode: "gauge".into(),
            update_notifications_enabled: true,
            skipped_update_version: None,
        }
    }
}

impl ShellSettings {
    fn sanitize(&mut self) {
        if !LANGUAGES.contains(&self.language.as_str()) {
            self.language = "en".into();
        }
        self.refresh_minutes = self.refresh_minutes.min(15);
        if !["gauge", "attention", "off"].contains(&self.menu_limit_color_mode.as_str()) {
            self.menu_limit_color_mode = "gauge".into();
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    #[serde(flatten)]
    pub shell: ShellSettings,
    #[serde(flatten)]
    pub providers: crate::providers::ProviderSettings,
    #[serde(flatten)]
    pub limits: crate::limits::LimitSettings,
    #[serde(flatten)]
    pub home: crate::home::HomeSettings,
    #[serde(flatten)]
    pub companion: crate::companion::CompanionSettings,
    #[serde(flatten)]
    pub economy: crate::economy::EconomySettings,
    #[serde(flatten)]
    pub pet: crate::pet::PetSettings,
}

impl Settings {
    /// Pulls hand-edited or out-of-range values back into what the UI can show.
    pub fn sanitized(mut self) -> Self {
        self.shell.sanitize();
        self.providers.sanitize();
        self.limits.sanitize();
        self.home.sanitize();
        self.companion.sanitize();
        self.economy.sanitize();
        self.pet.sanitize();
        self
    }
}

fn system_language() -> String {
    let locale = sys_locale::get_locale().unwrap_or_default().to_lowercase();
    LANGUAGES.iter().find(|l| locale.starts_with(*l)).unwrap_or(&"en").to_string()
}

struct SettingsStore {
    path: PathBuf,
    value: Settings,
}

pub fn setup(app: &tauri::App, dir: &std::path::Path) {
    let path = dir.join("settings.json");
    let value = load_json::<Settings>(&path).sanitized();
    app.manage(Mutex::new(SettingsStore { path, value }));
}

pub fn current(app: &AppHandle) -> Settings {
    app.state::<Mutex<SettingsStore>>().lock().unwrap().value.clone()
}

/// Sanitizes, stores and saves. Callers go through `shell::apply_settings` for side effects.
pub fn store(app: &AppHandle, value: Settings) -> Settings {
    let value = value.sanitized();
    let state = app.state::<Mutex<SettingsStore>>();
    let mut s = state.lock().unwrap();
    s.value = value.clone();
    save_json(&s.path, &s.value);
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_bad_values() {
        let mut s = Settings::default();
        s.shell.language = "xx".into();
        s.shell.refresh_minutes = 99;
        let s = s.sanitized();
        assert_eq!((s.shell.language.as_str(), s.shell.refresh_minutes), ("en", 15));
    }

    #[test]
    fn reads_v02_files_and_writes_flat_keys() {
        let old = r#"{"language":"ja","tray_tokens":false,"tray_limit":true,"pet_enabled":true,"warn_threshold":70}"#;
        let s: Settings = serde_json::from_str(old).unwrap();
        assert!(!s.shell.show_tokens_in_menu && s.shell.show_limit_in_menu && s.pet.floating_pet_enabled);
        assert_eq!((s.shell.language.as_str(), s.limits.warn_threshold), ("ja", 70.0));
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["warn_threshold"], 70.0);
        assert_eq!(v["show_limit_in_menu"], true);
    }
}
