//! App log with one-generation rotation, unclean-exit marker and crash.log merge
//! (pet-settings-misc.md §3.8, AppLog.swift, CrashReporter.swift).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub const MAX_BYTES: u64 = 2 * 1024 * 1024;
const FILE: &str = "PokeTokenBar.log";
const OLD: &str = "PokeTokenBar.old.log";
const MARKER: &str = "PokeTokenBar.running";

static DIR: OnceLock<PathBuf> = OnceLock::new();
static LOCK: Mutex<()> = Mutex::new(());

pub fn path() -> Option<PathBuf> {
    DIR.get().map(|d| d.join(FILE))
}

/// `[<ISO8601 UTC>] <message>`; rotates to `.old.log` once the file passes 2 MiB.
fn append(dir: &Path, message: &str) {
    let _guard = LOCK.lock();
    let file = dir.join(FILE);
    if fs::metadata(&file).is_ok_and(|m| m.len() > MAX_BYTES) {
        let _ = fs::remove_file(dir.join(OLD));
        let _ = fs::rename(&file, dir.join(OLD));
    }
    let line = format!("[{}] {message}\n", chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&file) {
        let _ = f.write_all(line.as_bytes());
    }
}

pub fn write(message: &str) {
    if let Some(dir) = DIR.get() {
        append(dir, message);
    }
}

/// Merges the previous session's crash.log, notes an unclean exit, then marks this session running.
pub fn init(log_dir: PathBuf, crash_log: &Path, version: &str) {
    let _ = fs::create_dir_all(&log_dir);
    let dir = DIR.get_or_init(|| log_dir);
    if let Ok(crash) = fs::read_to_string(crash_log) {
        if !crash.trim().is_empty() {
            append(dir, &format!("previous session crash record: {}", crash.trim()));
        }
        let _ = fs::remove_file(crash_log);
    }
    if dir.join(MARKER).exists() {
        append(dir, "previous session did not exit cleanly");
    }
    let _ = fs::write(dir.join(MARKER), std::process::id().to_string());
    append(dir, &format!("launch: PokeTokenBar {version} started"));
}

pub fn clean_exit() {
    if let Some(dir) = DIR.get() {
        let _ = fs::remove_file(dir.join(MARKER));
        append(dir, "clean shutdown");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotates_one_generation() {
        let dir = std::env::temp_dir().join(format!("ptb-log-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(FILE), vec![b'x'; MAX_BYTES as usize + 1]).unwrap();
        append(&dir, "hello");
        assert_eq!(fs::metadata(dir.join(OLD)).unwrap().len(), MAX_BYTES + 1);
        let text = fs::read_to_string(dir.join(FILE)).unwrap();
        assert!(text.starts_with('[') && text.ends_with("Z] hello\n"), "{text}");
        let _ = fs::remove_dir_all(&dir);
    }
}
