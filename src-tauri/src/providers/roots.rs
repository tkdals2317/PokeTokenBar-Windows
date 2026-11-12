//! Where each provider's logs live (providers.md §3.7-3.8, §4, §7): env overrides, Windows paths,
//! root normalization, custom scan roots (#177), hidden-skipping enumeration, Claude account folders.

use super::ProviderSettings;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

// ---- environment (UsageEnvironment.swift) ----

/// Location overrides users export. Resolved once per process.
pub const ENV_NAMES: [&str; 19] = [
    "CLAUDE_CONFIG_DIR", "OPENCODE_DATA_DIR", "HERMES_HOME", "COPILOT_HOME", "XDG_DATA_HOME", "GROK_HOME",
    "CLOUD_CODE_URL", "PI_CODING_AGENT_DIR", "PI_CODING_AGENT_SESSION_DIR", "CURSOR_SESSION_TOKEN",
    "CURSOR_USAGE_API", "KIRO_CLI_HOME", "KIRO_HOME", "CURSOR_DATA_DIR", "OMP_CODING_AGENT_DIR",
    "OMO_CODING_AGENT_DIR", "SENPI_CODING_AGENT_DIR", "ANTIGRAVITY_TOKEN_FILE", "KIMI_CODE_HOME",
];

/// Process env first; names still missing come from the registry (values `setx` wrote after the
/// app started are only there). Blank values count as unset.
pub fn resolve_env(process: impl Fn(&str) -> Option<String>, fallback: impl Fn(&[&str]) -> HashMap<String, String>) -> HashMap<String, String> {
    let blank = |v: &String| v.trim().is_empty();
    let mut out = HashMap::new();
    let mut missing = Vec::new();
    for name in ENV_NAMES {
        match process(name).filter(|v| !blank(v)) {
            Some(v) => {
                out.insert(name.to_string(), v);
            }
            None => missing.push(name),
        }
    }
    if !missing.is_empty() {
        out.extend(fallback(&missing).into_iter().filter(|(_, v)| !blank(v)));
    }
    out
}

#[cfg(windows)]
fn registry_env(names: &[&str]) -> HashMap<String, String> {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;
    let keys = [
        RegKey::predef(HKEY_CURRENT_USER).open_subkey("Environment").ok(),
        RegKey::predef(HKEY_LOCAL_MACHINE)
            .open_subkey(r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment")
            .ok(),
    ];
    let mut out = HashMap::new();
    for name in names {
        if let Some(v) = keys.iter().flatten().find_map(|k| k.get_value::<String, _>(name).ok()) {
            out.insert(name.to_string(), expand_percent(&v));
        }
    }
    out
}

#[cfg(not(windows))]
fn registry_env(_: &[&str]) -> HashMap<String, String> {
    HashMap::new()
}

#[cfg_attr(not(windows), allow(dead_code))]
/// `REG_EXPAND_SZ`: `%USERPROFILE%\x` → value of USERPROFILE. Unknown names stay as written.
pub fn expand_percent(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        match after.find('%') {
            Some(j) if j > 0 => {
                let name = &after[..j];
                match std::env::var(name) {
                    Ok(v) => out.push_str(&v),
                    Err(_) => out.push_str(&rest[i..i + j + 2]),
                }
                rest = &after[j + 1..];
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out + rest
}

pub fn env(name: &str) -> Option<String> {
    static RESOLVED: OnceLock<HashMap<String, String>> = OnceLock::new();
    RESOLVED.get_or_init(|| resolve_env(|n| std::env::var(n).ok(), registry_env)).get(name).cloned()
}

/// Comma list from an env override (no tilde expansion, like `environmentPaths`).
pub fn env_paths(name: &str) -> Option<Vec<PathBuf>> {
    let raw = env(name)?;
    Some(raw.split(',').map(str::trim).filter(|s| !s.is_empty()).map(PathBuf::from).collect())
}

/// Tilde-expanded env value, or `None` when blank.
fn env_dir(name: &str) -> Option<PathBuf> {
    env(name).map(|v| expand_tilde(v.trim()))
}

// ---- base folders ----

pub fn home() -> PathBuf {
    std::env::home_dir().unwrap_or_default()
}

/// `%APPDATA%` on Windows, `~/Library/Application Support` on macOS.
pub fn app_support() -> PathBuf {
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| home().join("AppData").join("Roaming"))
    } else if cfg!(target_os = "macos") {
        home().join("Library").join("Application Support")
    } else {
        home().join(".config")
    }
}

/// `%LOCALAPPDATA%` on Windows (Rust `dirs::data_local_dir`).
pub fn local_data() -> PathBuf {
    if cfg!(windows) {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| home().join("AppData").join("Local"))
    } else if cfg!(target_os = "macos") {
        app_support()
    } else {
        home().join(".local").join("share")
    }
}

/// `~`, `~/x`, `~\x` → home.
pub fn expand_tilde(s: &str) -> PathBuf {
    if s == "~" {
        home()
    } else if let Some(rest) = s.strip_prefix("~/").or_else(|| s.strip_prefix("~\\")) {
        home().join(rest)
    } else {
        PathBuf::from(s)
    }
}

/// Symlinks/junctions resolved, `\\?\` stripped; a missing path is kept as written.
pub fn canonical(p: &Path) -> PathBuf {
    match fs::canonicalize(p) {
        Ok(c) => {
            let s = c.to_string_lossy();
            match s.strip_prefix(r"\\?\UNC\") {
                Some(rest) => PathBuf::from(format!(r"\\{rest}")),
                None => PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()),
            }
        }
        Err(_) => p.components().collect(),
    }
}

/// Comparison key: case-insensitive, `/` separators, no trailing separator.
pub fn path_key(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/").to_lowercase();
    if s.len() > 1 && s.ends_with('/') { s[..s.len() - 1].to_string() } else { s }
}

fn is_under(p: &str, k: &str) -> bool {
    p == k || p.starts_with(&format!("{}/", k.trim_end_matches('/')))
}

/// Dedup (case-insensitive, after resolving links), drop roots nested under a kept one, keep order.
pub fn normalized_roots(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut unique: Vec<PathBuf> = Vec::new();
    for r in roots {
        let c = canonical(r);
        if seen.insert(path_key(&c)) {
            unique.push(c);
        }
    }
    let mut by_len: Vec<&PathBuf> = unique.iter().collect();
    by_len.sort_by_key(|p| p.as_os_str().len());
    let mut kept: Vec<String> = Vec::new();
    for p in by_len {
        let k = path_key(p);
        if !kept.iter().any(|x| is_under(&k, x)) {
            kept.push(k);
        }
    }
    unique.into_iter().filter(|p| kept.contains(&path_key(p))).collect()
}

// ---- custom scan roots (CustomScanRoots.swift) ----

/// Case-insensitive `*`, `?`, `[...]` (with `!`/`^` negation and ranges).
pub fn glob_match(pattern: &str, name: &str) -> bool {
    fn go(p: &[char], n: &[char]) -> bool {
        match p.first() {
            None => n.is_empty(),
            Some('*') => (0..=n.len()).any(|i| go(&p[1..], &n[i..])),
            Some('?') => !n.is_empty() && go(&p[1..], &n[1..]),
            Some('[') => {
                let Some(end) = p.iter().skip(1).position(|&c| c == ']').map(|i| i + 1) else {
                    return n.first() == Some(&'[') && go(&p[1..], &n[1..]);
                };
                let Some(&c) = n.first() else { return false };
                let mut set = &p[1..end];
                let negate = matches!(set.first(), Some('!') | Some('^'));
                if negate {
                    set = &set[1..];
                }
                let mut hit = false;
                let mut i = 0;
                while i < set.len() {
                    if i + 2 < set.len() && set[i + 1] == '-' {
                        hit |= set[i] <= c && c <= set[i + 2];
                        i += 3;
                    } else {
                        hit |= set[i] == c;
                        i += 1;
                    }
                }
                hit != negate && go(&p[end + 1..], &n[1..])
            }
            Some(&c) => n.first() == Some(&c) && go(&p[1..], &n[1..]),
        }
    }
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let n: Vec<char> = name.to_lowercase().chars().collect();
    go(&p, &n)
}

fn sorted_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    names.sort();
    names
}

/// Comma/newline list, tilde expansion, glob per segment; existing directories only.
pub fn expand_custom(raw: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for part in raw.split([',', '\n', '\r']) {
        let pattern = part.trim();
        if pattern.is_empty() {
            continue;
        }
        let abs = expand_tilde(pattern);
        if !abs.is_absolute() {
            continue;
        }
        let mut paths: Vec<PathBuf> = vec![PathBuf::new()];
        for comp in abs.components() {
            match comp {
                Component::Normal(seg) => {
                    let seg = seg.to_string_lossy();
                    if seg.contains(['*', '?', '[']) {
                        paths = paths
                            .iter()
                            .flat_map(|base| {
                                sorted_names(base)
                                    .into_iter()
                                    .filter(|n| glob_match(&seg, n))
                                    .map(|n| base.join(n))
                                    .collect::<Vec<_>>()
                            })
                            .collect();
                    } else {
                        paths.iter_mut().for_each(|p| p.push(seg.as_ref()));
                    }
                }
                other => paths.iter_mut().for_each(|p| p.push(other.as_os_str())),
            }
        }
        out.extend(paths.into_iter().filter(|p| p.is_dir()));
    }
    out
}

fn would_evict(extra: &Path, protected: &[PathBuf]) -> bool {
    let e = path_key(&canonical(extra));
    protected.iter().any(|d| {
        let d = path_key(&canonical(d));
        d != e && is_under(&d, &e)
    })
}

/// Curated defaults first; an extra that would fold a default away is dropped; then fold.
pub fn union(defaults: &[PathBuf], extra: Option<&str>) -> Vec<PathBuf> {
    let protected = normalized_roots(defaults);
    let Some(raw) = extra.filter(|r| !r.trim().is_empty()) else { return protected };
    let added: Vec<PathBuf> = expand_custom(raw).into_iter().filter(|x| !would_evict(x, &protected)).collect();
    normalized_roots(&[protected, added].concat())
}

/// Folded extras that are not curated defaults (the Settings match count).
pub fn surviving_extra_count(defaults: &[PathBuf], raw: &str) -> usize {
    let protected: HashSet<String> = normalized_roots(defaults).iter().map(|p| path_key(p)).collect();
    union(defaults, Some(raw)).iter().filter(|p| !protected.contains(&path_key(p))).count()
}

// ---- enumeration ----

pub fn mtime_ns(meta: &fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos() as u64)
}

pub fn ns_to_ts(ns: u64) -> f64 {
    ns as f64 / 1e9
}

/// Regular files under `root`, recursively, skipping dot entries below the root (the root itself
/// may be hidden: `.claude` is a root). Symlinked directories are not followed.
pub fn walk_files(root: &Path, f: &mut dyn FnMut(&Path, &fs::Metadata)) {
    let Ok(rd) = fs::read_dir(root) else { return };
    let mut entries: Vec<fs::DirEntry> = rd.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        if e.file_name().to_string_lossy().starts_with('.') || is_hidden_attr(&e) {
            continue;
        }
        let Ok(ft) = e.file_type() else { continue };
        let path = e.path();
        if ft.is_dir() {
            walk_files(&path, f);
        } else if ft.is_file() {
            // Not `e.metadata()`: on Windows that is the directory listing's copy, which keeps the old
            // size and mtime while another process still has the file open for appending (a live
            // Claude Code session), so new usage would not show until the session closed the file.
            if let Ok(meta) = fs::metadata(&path) {
                f(&path, &meta);
            }
        }
    }
}

#[cfg(windows)]
fn is_hidden_attr(e: &fs::DirEntry) -> bool {
    use std::os::windows::fs::MetadataExt;
    e.metadata().is_ok_and(|m| m.file_attributes() & 0x2 != 0)
}

#[cfg(not(windows))]
fn is_hidden_attr(_: &fs::DirEntry) -> bool {
    false
}

pub fn ext_is(p: &Path, ext: &str) -> bool {
    p.extension().is_some_and(|x| x.eq_ignore_ascii_case(ext))
}

pub fn file_name(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

pub fn read_json(p: &Path) -> Option<Value> {
    serde_json::from_slice(&fs::read(p).ok()?).ok()
}

// ---- Claude (LocalUsageReader.swift:101-129, ClaudeAccountRoots.swift) ----

const DESKTOP_SKIP: [&str; 4] = ["node_modules", ".git", "venv", ".venv"];

/// `.claude/projects` folders inside Claude Desktop session stores (depth ≤ 7, hidden not skipped).
pub fn embedded_claude_roots(base: &Path, max_depth: usize) -> Vec<PathBuf> {
    fn go(dir: &Path, level: usize, max: usize, out: &mut Vec<PathBuf>, pruned: &mut bool) {
        let Ok(rd) = fs::read_dir(dir) else { return };
        let mut entries: Vec<fs::DirEntry> = rd.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            if !e.file_type().is_ok_and(|t| t.is_dir()) {
                continue;
            }
            let name = e.file_name().to_string_lossy().into_owned();
            let path = e.path();
            if name == "projects" && file_name(dir) == ".claude" {
                out.push(path);
                continue;
            }
            if level + 1 >= max {
                *pruned = true;
            } else if !DESKTOP_SKIP.contains(&name.as_str()) {
                go(&path, level + 1, max, out, pruned);
            }
        }
    }
    let mut out = Vec::new();
    let mut pruned = false;
    if base.is_dir() {
        go(base, 0, max_depth, &mut out, &mut pruned);
    }
    if pruned {
        eprintln!(
            "claude desktop scan: depth {max_depth} reached under {}, found {} root(s) — deeper roots may be missed",
            file_name(base),
            out.len()
        );
    }
    out
}

pub fn claude_project_roots(config_dir: Option<&str>, accounts: &[PathBuf], home: &Path, custom: Option<&str>) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    for part in config_dir.unwrap_or("").split(',').map(str::trim).filter(|p| !p.is_empty()) {
        roots.push(expand_tilde(part).join("projects"));
    }
    roots.extend(accounts.iter().map(|a| a.join("projects")));
    roots.push(home.join(".config").join("claude").join("projects"));
    roots.push(home.join(".claude").join("projects"));
    let desktop = app_support().join("Claude");
    for store in ["local-agent-mode-sessions", "claude-code-sessions"] {
        roots.extend(embedded_claude_roots(&desktop.join(store), 7));
    }
    union(&roots, custom)
}

fn default_claude_roots(home: &Path) -> HashSet<String> {
    [home.join(".claude"), home.join(".config").join("claude")].iter().map(|p| path_key(p)).collect()
}

/// Settings extras: comma/newline, tilde, absolute, existing dir, not a default root, dedup.
pub fn account_roots_from(raw: &str, home: &Path) -> Vec<PathBuf> {
    let excluded = default_claude_roots(home);
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for part in raw.split([',', '\n', '\r']).map(str::trim).filter(|p| !p.is_empty()) {
        let p: PathBuf = expand_tilde(part).components().collect();
        if !p.is_absolute() || !p.is_dir() {
            continue;
        }
        let k = path_key(&p);
        if !excluded.contains(&k) && seen.insert(k) {
            out.push(p);
        }
    }
    out
}

/// The folder's `.claude.json` has an `oauthAccount` object (cached on mtime+size).
pub fn has_login(root: &Path) -> bool {
    saved_login(root).is_some()
}

#[derive(Clone, Debug, PartialEq)]
pub struct SavedLogin {
    pub email: Option<String>,
    pub organization: Option<String>,
    pub organization_id: Option<String>,
}

pub fn saved_login(root: &Path) -> Option<SavedLogin> {
    static CACHE: Mutex<Option<HashMap<PathBuf, (u64, u64, Option<SavedLogin>)>>> = Mutex::new(None);
    let file = root.join(".claude.json");
    let meta = fs::metadata(&file).ok()?;
    let sig = (mtime_ns(&meta), meta.len());
    let mut cache = CACHE.lock().unwrap();
    let map = cache.get_or_insert_with(HashMap::new);
    if let Some((m, s, v)) = map.get(&file) {
        if (*m, *s) == sig {
            return v.clone();
        }
    }
    let login = read_json(&file).and_then(|j| {
        let acc = j.get("oauthAccount")?.as_object()?;
        let get = |k: &str| acc.get(k).and_then(Value::as_str).filter(|s| !s.is_empty()).map(String::from);
        Some(SavedLogin {
            email: get("emailAddress"),
            organization: get("emailAddress").and(get("organizationName")),
            organization_id: get("emailAddress").and(get("organizationUuid")),
        })
    });
    map.insert(file, (sig.0, sig.1, login.clone()));
    login
}

/// `CLAUDE_CONFIG_DIR` entries, then sorted `~/.claude-*` / `~/.claude_*` with a login.
pub fn discovered_accounts(home: &Path, config_dir: Option<&str>) -> Vec<PathBuf> {
    let excluded = default_claude_roots(home);
    let mut seen = HashSet::new();
    let mut out: Vec<PathBuf> =
        account_roots_from(config_dir.unwrap_or(""), home).into_iter().filter(|p| seen.insert(path_key(p))).collect();
    for name in sorted_names(home) {
        if !(name.starts_with(".claude-") || name.starts_with(".claude_")) {
            continue;
        }
        let p = home.join(&name);
        let k = path_key(&p);
        if p.is_dir() && !excluded.contains(&k) && has_login(&p) && seen.insert(k) {
            out.push(p);
        }
    }
    out
}

/// Every additional Claude folder the running app follows: detected first, then Settings extras.
/// Detection never runs in tests (it would read the developer's own logins).
pub fn installed_account_roots(setting: &str) -> Vec<PathBuf> {
    let h = home();
    let detected = if cfg!(test) { Vec::new() } else { discovered_accounts(&h, env("CLAUDE_CONFIG_DIR").as_deref()) };
    let mut seen = HashSet::new();
    detected.into_iter().chain(account_roots_from(setting, &h)).filter(|p| seen.insert(path_key(p))).collect()
}

/// Claude roots are costly (Desktop walk) and change rarely: 300 s TTL, dropped on Settings save.
static CLAUDE_ROOTS: Mutex<Option<(Instant, String, Vec<PathBuf>)>> = Mutex::new(None);

pub fn invalidate_claude_roots() {
    *CLAUDE_ROOTS.lock().unwrap() = None;
}

fn cached_claude_roots(s: &ProviderSettings) -> Vec<PathBuf> {
    let key = format!("{}\u{0}{}", s.custom_scan_roots.get("claude_code").map_or("", String::as_str), s.additional_claude_config_dirs);
    if let Some((at, k, roots)) = CLAUDE_ROOTS.lock().unwrap().as_ref() {
        if *k == key && at.elapsed() < Duration::from_secs(300) {
            return roots.clone();
        }
    }
    let fresh = claude_project_roots(
        env("CLAUDE_CONFIG_DIR").as_deref(),
        &installed_account_roots(&s.additional_claude_config_dirs),
        &home(),
        custom_value(s, "claude_code"),
    );
    *CLAUDE_ROOTS.lock().unwrap() = Some((Instant::now(), key, fresh.clone()));
    fresh
}

// ---- curated roots per provider (§4) ----

pub fn custom_value<'a>(s: &'a ProviderSettings, id: &str) -> Option<&'a str> {
    s.custom_scan_roots.get(id).map(String::as_str).filter(|v| !v.trim().is_empty())
}

fn or_env(name: &str, default: Vec<PathBuf>) -> Vec<PathBuf> {
    env_paths(name).unwrap_or(default)
}

fn with_env_sessions(mut roots: Vec<PathBuf>, names: &[&str]) -> Vec<PathBuf> {
    roots.extend(names.iter().filter_map(|n| env_dir(n)).map(|p| p.join("sessions")));
    roots
}

/// Built-in locations for a provider, before custom roots.
pub fn curated(id: &str, s: &ProviderSettings) -> Vec<PathBuf> {
    let h = home();
    match id {
        "claude_code" => claude_project_roots(
            env("CLAUDE_CONFIG_DIR").as_deref(),
            &installed_account_roots(&s.additional_claude_config_dirs),
            &h,
            None,
        ),
        "codex" => normalized_roots(&[h.join(".codex").join("sessions"), h.join(".codex").join("archived_sessions")]),
        "gemini" => vec![h.join(".gemini").join("tmp")],
        "antigravity" => ["antigravity", "antigravity-cli", "antigravity-ide"]
            .iter()
            .map(|d| h.join(".gemini").join(d).join("conversations"))
            .collect(),
        "opencode" => or_env("OPENCODE_DATA_DIR", vec![h.join(".local").join("share").join("opencode")]),
        "hermes" => or_env("HERMES_HOME", vec![h.join(".hermes")]),
        "cursor" => or_env(
            "CURSOR_DATA_DIR",
            ["Cursor", "Cursor Nightly"].iter().map(|n| app_support().join(n).join("User").join("globalStorage")).collect(),
        ),
        "grok" => vec![env_dir("GROK_HOME").map_or_else(|| h.join(".grok").join("sessions"), |g| g.join("sessions"))],
        "copilot" => or_env("COPILOT_HOME", vec![h.join(".copilot")]),
        "kiro" => {
            let mut v = or_env("KIRO_CLI_HOME", vec![local_data().join("kiro-cli")]);
            v.extend(or_env("KIRO_HOME", vec![h.join(".kiro")]).into_iter().map(|k| k.join("sessions")));
            v
        }
        "pi" => {
            let mut v = with_env_sessions(vec![h.join(".pi").join("agent").join("sessions")], &["PI_CODING_AGENT_DIR"]);
            v.extend(env_dir("PI_CODING_AGENT_SESSION_DIR"));
            normalized_roots(&v)
        }
        "omp" => normalized_roots(&with_env_sessions(vec![h.join(".omp").join("agent").join("sessions")], &["OMP_CODING_AGENT_DIR"])),
        "omo" => normalized_roots(&with_env_sessions(
            vec![h.join(".omo").join("agent").join("sessions"), h.join(".senpi").join("agent").join("sessions")],
            &["OMO_CODING_AGENT_DIR", "SENPI_CODING_AGENT_DIR"],
        )),
        "aside" => vec![h.join(".aside").join("u")],
        "kimi_code" => normalized_roots(&with_env_sessions(
            vec![
                h.join(".kimi-code").join("sessions"),
                app_support().join("kimi-desktop/daimon-share/daimon/runtime/kimi-code/home/sessions"),
            ],
            &["KIMI_CODE_HOME"],
        )),
        "devin" => match env_paths("XDG_DATA_HOME") {
            Some(v) => v.into_iter().map(|p| p.join("devin")).collect(),
            None if cfg!(windows) => vec![local_data().join("devin"), h.join(".local").join("share").join("devin")],
            None => vec![h.join(".local").join("share").join("devin")],
        },
        _ => Vec::new(),
    }
}

/// What a provider scans: curated ∪ custom.
pub fn roots_for(id: &str, s: &ProviderSettings) -> Vec<PathBuf> {
    if id == "claude_code" {
        return cached_claude_roots(s);
    }
    union(&curated(id, s), custom_value(s, id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ptb-roots-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn env_resolution_prefers_process_and_asks_fallback_once() {
        let asked = std::cell::RefCell::new(Vec::new());
        let r = resolve_env(
            |n| match n {
                "GROK_HOME" => Some("/g".into()),
                "HERMES_HOME" => Some("  ".into()),
                _ => None,
            },
            |missing| {
                asked.borrow_mut().push(missing.len());
                [("HERMES_HOME".to_string(), "/h".to_string()), ("KIRO_HOME".to_string(), "".to_string())].into()
            },
        );
        assert_eq!(r.get("GROK_HOME").map(String::as_str), Some("/g"));
        assert_eq!(r.get("HERMES_HOME").map(String::as_str), Some("/h"));
        assert!(!r.contains_key("KIRO_HOME"));
        assert_eq!(*asked.borrow(), vec![18]);
    }

    #[test]
    fn percent_expansion() {
        unsafe { std::env::set_var("PTB_TEST_VAR", "X") };
        assert_eq!(expand_percent("%PTB_TEST_VAR%\\a %NOPE_PTB% 100%"), "X\\a %NOPE_PTB% 100%");
    }

    #[test]
    fn glob() {
        assert!(glob_match("*.jsonl", "A.JSONL"));
        assert!(glob_match(".claude-?", ".claude-w"));
        assert!(glob_match("[a-c]x", "bx") && !glob_match("[!a-c]x", "bx"));
        assert!(!glob_match("a*b", "acd"));
    }

    #[test]
    fn normalize_folds_dupes_and_nested_keeping_order() {
        let d = tmp("norm");
        let (a, b) = (d.join("a"), d.join("a").join("b"));
        fs::create_dir_all(&b).unwrap();
        let out = normalized_roots(&[b.clone(), d.join("other"), a.clone(), a.clone()]);
        assert_eq!(out, vec![d.join("other").components().collect::<PathBuf>(), canonical(&a)]);
    }

    #[test]
    fn custom_roots_expand_and_never_evict_defaults() {
        let d = tmp("custom");
        for p in ["logs/one", "logs/two", "def/.hidden"] {
            fs::create_dir_all(d.join(p)).unwrap();
        }
        let raw = format!("{}\n{}, relative/x", d.join("logs").join("*").display(), d.display());
        let found = expand_custom(&raw);
        assert_eq!(found.len(), 3); // one, two, d itself
        let defaults = vec![d.join("def").join(".hidden")];
        // `d` would fold the hidden default away, so it is dropped; the two globbed folders stay.
        assert_eq!(surviving_extra_count(&defaults, &raw), 2);
        assert_eq!(union(&defaults, Some(&raw)).len(), 3);
    }

    #[test]
    fn walk_skips_hidden_below_root() {
        let d = tmp("walk").join(".root");
        fs::create_dir_all(d.join(".git")).unwrap();
        fs::create_dir_all(d.join("p")).unwrap();
        fs::write(d.join(".git").join("x.jsonl"), "").unwrap();
        fs::write(d.join("p").join("y.jsonl"), "").unwrap();
        let mut seen = Vec::new();
        walk_files(&d, &mut |p, _| seen.push(file_name(p)));
        assert_eq!(seen, vec!["y.jsonl"]);
    }

    #[test]
    fn account_roots_and_discovery() {
        let h = tmp("acct");
        fs::create_dir_all(h.join(".claude")).unwrap();
        fs::create_dir_all(h.join(".claude-work")).unwrap();
        fs::create_dir_all(h.join(".claude_out")).unwrap();
        fs::write(h.join(".claude-work").join(".claude.json"), r#"{"oauthAccount":{"emailAddress":"a@b.c"}}"#).unwrap();
        fs::write(h.join(".claude_out").join(".claude.json"), r#"{"projects":{}}"#).unwrap();
        let raw = format!("{}, {}, {}", h.join(".claude").display(), h.join(".claude-work").display(), h.join("missing").display());
        assert_eq!(account_roots_from(&raw, &h), vec![h.join(".claude-work")]);
        assert_eq!(discovered_accounts(&h, None), vec![h.join(".claude-work")]);
        assert_eq!(saved_login(&h.join(".claude-work")).unwrap().email.as_deref(), Some("a@b.c"));
    }

    #[test]
    fn desktop_roots_found_and_not_descended() {
        let d = tmp("desk");
        let root = d.join("u1").join("u2").join("local_x").join(".claude").join("projects");
        fs::create_dir_all(root.join("deep").join(".claude").join("projects")).unwrap();
        fs::create_dir_all(d.join("node_modules").join(".claude").join("projects")).unwrap();
        assert_eq!(embedded_claude_roots(&d, 7), vec![root]);
    }
}
