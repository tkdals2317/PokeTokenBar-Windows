//! JSON files in the app data folder. Every area persists through these two functions.

use serde::Serialize;
use std::fs;
use std::path::Path;

/// Reads a JSON file. One that fails to parse is kept as `.corrupt` instead of being overwritten.
pub fn load_json<T: serde::de::DeserializeOwned + Default>(path: &Path) -> T {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|_| {
            let _ = fs::rename(path, path.with_extension("json.corrupt"));
            T::default()
        }),
        Err(_) => T::default(),
    }
}

/// Atomic write (temp file + rename) so a crash never leaves a half-written file.
pub fn save_json<T: Serialize>(path: &Path, value: &T) {
    let tmp = path.with_extension("json.tmp");
    if let Ok(json) = serde_json::to_vec_pretty(value) {
        if fs::write(&tmp, json).is_ok() {
            let _ = fs::rename(&tmp, path);
        }
    }
}
