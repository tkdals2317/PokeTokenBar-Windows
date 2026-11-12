//! Windows system state the pet reacts to (pet-settings-misc.md §6): Battery Saver (≈ macOS Low Power
//! Mode), "Animation effects" off (≈ Reduce Motion), and WER crash restart (≈ LaunchAgent KeepAlive).

#[cfg(windows)]
mod win {
    #[repr(C)]
    #[derive(Default)]
    pub struct SystemPowerStatus {
        pub ac_line_status: u8,
        pub battery_flag: u8,
        pub battery_life_percent: u8,
        pub system_status_flag: u8,
        pub battery_life_time: u32,
        pub battery_full_life_time: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub fn GetSystemPowerStatus(status: *mut SystemPowerStatus) -> i32;
        pub fn RegisterApplicationRestart(command_line: *const u16, flags: u32) -> i32;
        pub fn UnregisterApplicationRestart() -> i32;
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn SystemParametersInfoW(action: u32, param: u32, pv: *mut core::ffi::c_void, win_ini: u32) -> i32;
    }

    pub const SPI_GETCLIENTAREAANIMATION: u32 = 0x1042;
    pub const RESTART_NO_REBOOT: u32 = 8;
}

/// Battery Saver is on.
pub fn low_power() -> bool {
    #[cfg(windows)]
    {
        let mut s = win::SystemPowerStatus::default();
        // SAFETY: plain out-parameter struct with the documented layout.
        unsafe { win::GetSystemPowerStatus(&mut s) != 0 && s.system_status_flag & 1 == 1 }
    }
    #[cfg(not(windows))]
    false
}

/// Settings › Accessibility › Visual effects › Animation effects is off.
pub fn reduce_motion() -> bool {
    #[cfg(windows)]
    {
        let mut on: i32 = 1;
        // SAFETY: SPI_GETCLIENTAREAANIMATION writes a BOOL into `on`.
        let ok = unsafe { win::SystemParametersInfoW(win::SPI_GETCLIENTAREAANIMATION, 0, (&mut on as *mut i32).cast(), 0) };
        ok != 0 && on == 0
    }
    #[cfg(not(windows))]
    false
}

/// Relaunch after a crash or hang (Windows Error Reporting, once the app has run 60 s). Shares the
/// launch-at-login toggle like the macOS KeepAlive agent; a clean exit never restarts.
pub fn set_crash_restart(on: bool) {
    #[cfg(windows)]
    // SAFETY: an empty, NUL-terminated command line; both calls take no other pointers.
    unsafe {
        if on {
            let empty = [0u16];
            win::RegisterApplicationRestart(empty.as_ptr(), win::RESTART_NO_REBOOT);
        } else {
            win::UnregisterApplicationRestart();
        }
    }
    #[cfg(not(windows))]
    let _ = on;
}
