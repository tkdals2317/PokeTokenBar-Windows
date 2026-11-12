//! `poketokenbar://agent?state=<working|waiting|done|idle>[&message=<text>][&label=<name>]`
//! (pet-settings-misc.md §3.2, AgentActivity.swift). Pure parser + tracker; time comes in as arguments.

use serde::Serialize;
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub const SCHEME: &str = "poketokenbar";
const HOST: &str = "agent";
const MAX_MESSAGE: usize = 80;
const MAX_LABEL: usize = 24;
pub const WORKING_TIMEOUT: Duration = Duration::from_secs(600);
pub const DONE_DURATION: Duration = Duration::from_secs(6);
const MAX_AGENTS: usize = 8;

#[derive(Debug, Clone, PartialEq)]
pub enum Activity {
    Idle,
    Working,
    Waiting(Option<String>),
    Done(Option<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub activity: Activity,
    pub label: Option<String>,
}

fn hex(b: u8) -> Option<u8> {
    (b as char).to_digit(16).map(|d| d as u8)
}

/// `URLComponents.queryItems` decoding: `%XX` only (a `+` stays a plus).
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Nil for any other scheme, host or state: unknown input is ignored, never an error.
pub fn parse(url: &str) -> Option<Event> {
    let url = tauri::Url::parse(url.trim()).ok()?;
    if !url.scheme().eq_ignore_ascii_case(SCHEME) || !url.host_str()?.eq_ignore_ascii_case(HOST) {
        return None;
    }
    let items: Vec<(String, String)> = url
        .query()?
        .split('&')
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            (percent_decode(k), percent_decode(v))
        })
        .collect();
    let first = |name: &str| items.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str());
    let state = first("state")?.to_lowercase();
    let message = sanitize(first("message"), MAX_MESSAGE);
    let label = sanitize(first("label"), MAX_LABEL);
    let activity = match state.as_str() {
        "idle" => Activity::Idle,
        "working" => Activity::Working,
        "waiting" => Activity::Waiting(message),
        "done" => Activity::Done(message),
        _ => return None,
    };
    Some(Event { activity, label })
}

/// Unicode general category Cf (format), the code points that occur in practice.
fn is_format(c: char) -> bool {
    matches!(c as u32,
        0x00AD | 0x0600..=0x0605 | 0x061C | 0x06DD | 0x070F | 0x0890..=0x0891 | 0x08E2 | 0x180E
        | 0x200B..=0x200F | 0x202A..=0x202E | 0x2060..=0x2064 | 0x2066..=0x206F | 0xFEFF
        | 0xFFF9..=0xFFFB | 0x110BD | 0x110CD | 0x13430..=0x1343F | 0x1BCA0..=0x1BCA3
        | 0x1D173..=0x1D17A | 0xE0001 | 0xE0020..=0xE007F)
}

/// Scalars that attach to the previous one (Swift counts them as one `Character`).
// ponytail: approximates extended grapheme clusters; add unicode-segmentation if exotic scripts matter.
fn extends(prev: Option<char>, c: char) -> bool {
    prev == Some('\u{200D}')
        || matches!(c as u32,
            0x200D | 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF
            | 0xFE00..=0xFE0F | 0xFE20..=0xFE2F | 0x1F3FB..=0x1F3FF | 0xE0020..=0xE007F)
}

/// Byte offsets where each user-perceived character starts.
fn char_starts(s: &str) -> Vec<usize> {
    let mut out = vec![];
    let mut prev = None;
    for (i, c) in s.char_indices() {
        if !extends(prev, c) {
            out.push(i);
        }
        prev = Some(c);
    }
    out
}

/// Controls become spaces, format characters (except ZWJ) are dropped, whitespace runs collapse,
/// and anything past 4×max scalars or max characters is cut with an ellipsis. Empty → None.
pub fn sanitize(raw: Option<&str>, max: usize) -> Option<String> {
    let cleaned: String = raw?
        .chars()
        .filter_map(|c| if c.is_control() { Some(' ') } else if is_format(c) && c != '\u{200D}' { None } else { Some(c) })
        .collect();
    let mut text = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return None;
    }
    if text.chars().count() > max * 4 {
        text = text.chars().take(max * 4 - 1).collect::<String>().trim().to_string() + "…";
    }
    let starts = char_starts(&text);
    if starts.len() <= max {
        return Some(text);
    }
    Some(text[..starts[max - 1]].trim().to_string() + "…")
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Note {
    pub label: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Summary {
    pub done: Option<Note>,
    pub waiting: Option<Note>,
    pub other_waiting: usize,
    pub working: bool,
}

#[derive(Debug, Clone)]
struct Entry {
    activity: Activity,
    deadline: Option<Instant>,
    sequence: u64,
}

/// One entry per label (`""` = the unlabeled agent), at most 8.
#[derive(Debug, Default)]
pub struct Tracker {
    agents: HashMap<String, Entry>,
    sequence: u64,
}

impl Tracker {
    pub fn deadline(&self) -> Option<Instant> {
        self.agents.values().filter_map(|e| e.deadline).min()
    }

    pub fn summary(&self) -> Summary {
        let note = |k: &str, m: &Option<String>| Note { label: (!k.is_empty()).then(|| k.to_string()), message: m.clone() };
        let mut s = Summary::default();
        let (mut done, mut waiting) = (vec![], vec![]);
        for (k, e) in &self.agents {
            match &e.activity {
                Activity::Working => s.working = true,
                Activity::Waiting(m) => waiting.push((e.sequence, k, m)),
                Activity::Done(m) => done.push((e.sequence, k, m)),
                Activity::Idle => {}
            }
        }
        s.done = done.iter().max_by_key(|d| d.0).map(|d| note(d.1, d.2));
        if let Some(w) = waiting.iter().max_by_key(|w| w.0) {
            s.waiting = Some(note(w.1, w.2));
            s.other_waiting = waiting.len() - 1;
        }
        s
    }

    pub fn apply(&mut self, e: Event, now: Instant) {
        let key = e.label.unwrap_or_default();
        self.expire(now);
        let deadline = match e.activity {
            Activity::Idle => {
                self.agents.remove(&key);
                return;
            }
            Activity::Working => Some(now + WORKING_TIMEOUT),
            Activity::Done(_) => Some(now + DONE_DURATION),
            Activity::Waiting(_) => None,
        };
        if !self.agents.contains_key(&key) && self.agents.len() >= MAX_AGENTS {
            if let Some(oldest) = self.agents.iter().min_by_key(|(_, v)| v.sequence).map(|(k, _)| k.clone()) {
                self.agents.remove(&oldest);
            }
        }
        self.sequence += 1;
        self.agents.insert(key, Entry { activity: e.activity, deadline, sequence: self.sequence });
    }

    /// Whether any agent changed.
    pub fn expire(&mut self, now: Instant) -> bool {
        let before = self.agents.len();
        self.agents.retain(|_, e| e.deadline.is_none_or(|d| now < d));
        before != self.agents.len()
    }

    /// A click answers every question and acknowledges results; it does not stop work.
    pub fn dismiss_bubble(&mut self) -> bool {
        let before = self.agents.len();
        self.agents.retain(|_, e| e.activity == Activity::Working);
        before != self.agents.len()
    }

    pub fn reset(&mut self) {
        self.agents.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_states_and_ignores_unknown_input() {
        let e = parse("poketokenbar://agent?state=DONE&message=Build%20ok&label=web").unwrap();
        assert_eq!(e, Event { activity: Activity::Done(Some("Build ok".into())), label: Some("web".into()) });
        assert_eq!(parse("PokeTokenBar://Agent?state=working&message=x").unwrap().activity, Activity::Working);
        assert_eq!(parse("poketokenbar://agent?state=waiting").unwrap().activity, Activity::Waiting(None));
        assert_eq!(parse("poketokenbar://agent?state=idle&state=done").unwrap().activity, Activity::Idle);
        assert!(parse("poketokenbar://agent?state=sleeping").is_none());
        assert!(parse("poketokenbar://other?state=done").is_none());
        assert!(parse("https://agent?state=done").is_none());
        assert!(parse("poketokenbar://agent").is_none());
        assert_eq!(parse("poketokenbar://agent?state=done&message=a+b").unwrap().activity, Activity::Done(Some("a+b".into())));
    }

    #[test]
    fn sanitizes_untrusted_text() {
        assert_eq!(sanitize(Some(" a\n\tb \u{202E}c\u{200B} "), 80).as_deref(), Some("a b c"));
        assert_eq!(sanitize(Some("  \u{200B} "), 80), None);
        assert_eq!(sanitize(None, 80), None);
        let long = "x".repeat(30);
        assert_eq!(sanitize(Some(&long), 24).unwrap(), format!("{}…", "x".repeat(23)));
        assert_eq!(sanitize(Some("abcdefghij klmnopqrstuv xyz"), 24).unwrap(), "abcdefghij klmnopqrstuv…");
        // The cut is trimmed before the ellipsis.
        assert_eq!(sanitize(Some("abcdefghij klmnopqrstu vwxyz"), 24).unwrap(), "abcdefghij klmnopqrstu…");
        // ZWJ family emoji stays one character.
        let family = "👨\u{200D}👩\u{200D}👧";
        assert_eq!(sanitize(Some(family), 24).as_deref(), Some(family));
        // Scalar cap: 4 × max.
        let marks = format!("a{}", "\u{0301}".repeat(200));
        assert_eq!(sanitize(Some(&marks), 24).unwrap().chars().count(), 4 * 24 - 1 + 1);
    }

    #[test]
    fn tracker_follows_labels_deadlines_and_eviction() {
        let t0 = Instant::now();
        let ev = |a: Activity, l: Option<&str>| Event { activity: a, label: l.map(String::from) };
        let mut t = Tracker::default();
        t.apply(ev(Activity::Working, Some("a")), t0);
        t.apply(ev(Activity::Waiting(Some("q1".into())), Some("b")), t0);
        t.apply(ev(Activity::Waiting(None), Some("c")), t0);
        let s = t.summary();
        assert!(s.working);
        assert_eq!(s.waiting, Some(Note { label: Some("c".into()), message: None }));
        assert_eq!(s.other_waiting, 1);
        t.apply(ev(Activity::Done(None), None), t0);
        assert_eq!(t.summary().done, Some(Note::default()));
        assert_eq!(t.deadline(), Some(t0 + DONE_DURATION));
        assert!(t.expire(t0 + DONE_DURATION));
        assert!(t.summary().done.is_none());
        assert!(t.dismiss_bubble());
        assert_eq!(t.summary(), Summary { working: true, ..Default::default() });
        assert!(!t.expire(t0 + WORKING_TIMEOUT - Duration::from_secs(1)));
        assert!(t.expire(t0 + WORKING_TIMEOUT));
        for i in 0..9 {
            t.apply(ev(Activity::Waiting(None), Some(&i.to_string())), t0);
        }
        let s = t.summary();
        assert_eq!((s.waiting.unwrap().label.unwrap(), s.other_waiting), ("8".to_string(), 7));
        t.apply(ev(Activity::Idle, Some("8")), t0);
        assert_eq!(t.summary().other_waiting, 6);
    }
}
