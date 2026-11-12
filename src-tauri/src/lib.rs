//! Entry point. Owner: foundation. Module layout and ownership: docs/parity/ARCHITECTURE.md.

/// Declares an area's Tauri commands, once, in the area's `mod.rs`:
/// `crate::area_commands![cmd_a, cmd_b];` (commands must be in scope of that module).
/// `run()` routes every invoke to the area whose list contains the command name.
#[macro_export]
macro_rules! area_commands {
    ($($cmd:ident),* $(,)?) => {
        pub const COMMANDS: &[&str] = &[$(stringify!($cmd)),*];
        pub fn handler() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
            tauri::generate_handler![$($cmd),*]
        }
    };
}

mod format;
mod model;
mod settings;
mod store;

mod companion;
mod economy;
mod home;
mod limits;
mod pet;
mod providers;
mod shell;

/// Every area, in setup order. Adding an area = one entry here.
macro_rules! for_areas {
    ($m:ident) => {
        $m!(providers, limits, companion, home, economy, pet, shell)
    };
}

fn invoke(invoke: tauri::ipc::Invoke) -> bool {
    let cmd = invoke.message.command().to_string();
    macro_rules! route {
        ($($area:ident),*) => {
            $(if $area::COMMANDS.contains(&cmd.as_str()) {
                return $area::handler()(invoke);
            })*
        };
    }
    for_areas!(route);
    false
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use tauri::Manager;
    tauri::Builder::default()
        // Must be first: a second launch exits and opens the running copy's popover instead.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| shell::on_second_instance(app)))
        // The popover is anchored to the tray every time it opens, so only the pet keeps its place.
        .plugin(tauri_plugin_window_state::Builder::new().with_denylist(&[shell::popover::LABEL]).build())
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(invoke)
        .setup(|app| {
            // Menu bar app like the original: no Dock icon.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            shell::install_crash_log(dir.join("crash.log"));
            settings::setup(app, &dir);
            providers::setup(app)?;
            limits::setup(app)?;
            companion::setup(app, &dir)?;
            home::setup(app)?;
            economy::setup(app)?;
            pet::setup(app)?;
            shell::setup(app)?; // last: tray + scheduler need every area's state
            Ok(())
        })
        .on_window_event(shell::on_window_event)
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
