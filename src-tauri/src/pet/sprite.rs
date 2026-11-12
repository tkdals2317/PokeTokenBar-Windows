//! Animated tray sprite (ui-shell.md §3.5, SpriteAnimation.swift): Gen-V GIF frames, decimated to
//! the animation-quality frame floor, fitted nearest-neighbour into a 32 px icon, bottom-aligned.
//! Statics (egg, missing GIF) bob 1 px every 0.5 s. Paused while the popover is open.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tauri::image::Image;
use tauri::{AppHandle, Manager};

const BASE: &str = "https://raw.githubusercontent.com/PokeAPI/sprites/master/sprites/pokemon";
pub const ICON: usize = 32;
const BOB_DELAY: f64 = 0.5;
const ALPHA_MIN: u8 = 16;
const RETRY: Duration = Duration::from_secs(60);
const PAUSE_POLL: Duration = Duration::from_millis(300);

/// RGBA canvas + delay in seconds.
pub type Frame = (Vec<u8>, f64);

/// What the tray shows; any change rebuilds the frames.
#[derive(Debug, Clone, PartialEq)]
pub struct Key {
    pub species: Option<u32>,
    pub shiny: bool,
    pub back: bool,
    pub floor: f64,
}

/// `AnimationQuality.frameFloor`, raised to 0.4 s under Battery Saver.
pub fn frame_floor(quality: &str, low_power: bool) -> f64 {
    let f: f64 = match quality {
        "smooth" => 0.1,
        "balanced" => 0.2,
        _ => 0.4,
    };
    if low_power { f.max(0.4) } else { f }
}

/// Decimates to at most one frame per `floor` while keeping the loop length (never stretches).
pub fn cap_frame_rate<T: Clone>(frames: &[(T, f64)], floor: f64) -> Vec<(T, f64)> {
    if floor <= 0.0 || frames.len() <= 1 {
        return frames.to_vec();
    }
    let mut out: Vec<(T, f64)> = vec![];
    let mut held: Option<T> = None;
    let mut acc = 0.0;
    for (img, delay) in frames {
        if held.is_none() {
            held = Some(img.clone());
        }
        acc += delay;
        if acc + 1e-9 >= floor {
            out.push((held.take().unwrap(), acc));
            acc = 0.0;
        }
    }
    if let Some(tail) = held {
        if acc > 0.0 {
            match out.last_mut() {
                Some(last) => last.1 += acc,
                None => out.push((tail, acc)),
            }
        }
    }
    out
}

/// GIF delay in seconds: 0 or under 20 ms plays at 100 ms (browser convention).
pub fn gif_delay(centis: u16) -> f64 {
    let d = centis as f64 / 100.0;
    if d < 0.02 { 0.1 } else { d }
}

/// Fully composited frames (disposal honoured). A single-frame GIF yields one frame.
pub fn decode_gif(bytes: &[u8]) -> Option<(usize, usize, Vec<Frame>)> {
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::RGBA);
    let mut dec = opts.read_info(bytes).ok()?;
    let (w, h) = (dec.width() as usize, dec.height() as usize);
    let mut canvas = vec![0u8; w * h * 4];
    let mut out = vec![];
    while let Ok(Some(f)) = dec.read_next_frame() {
        let saved = (f.dispose == gif::DisposalMethod::Previous).then(|| canvas.clone());
        let (fl, ft, fw, fh) = (f.left as usize, f.top as usize, f.width as usize, f.height as usize);
        for y in 0..fh {
            for x in 0..fw {
                let (cx, cy) = (fl + x, ft + y);
                let src = (y * fw + x) * 4;
                if cx >= w || cy >= h || src + 3 >= f.buffer.len() || f.buffer[src + 3] == 0 {
                    continue;
                }
                let dst = (cy * w + cx) * 4;
                canvas[dst..dst + 4].copy_from_slice(&f.buffer[src..src + 4]);
            }
        }
        out.push((canvas.clone(), gif_delay(f.delay)));
        match f.dispose {
            gif::DisposalMethod::Background => {
                for y in ft..(ft + fh).min(h) {
                    for x in fl..(fl + fw).min(w) {
                        canvas[(y * w + x) * 4..(y * w + x) * 4 + 4].fill(0);
                    }
                }
            }
            gif::DisposalMethod::Previous => canvas = saved.unwrap_or(canvas),
            _ => {}
        }
    }
    (!out.is_empty()).then_some((w, h, out))
}

pub fn decode_png(bytes: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    let mut dec = png::Decoder::new(bytes);
    dec.set_transformations(png::Transformations::normalize_to_color8() | png::Transformations::ALPHA);
    let mut reader = dec.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    let rgba = match info.color_type {
        png::ColorType::Rgba => buf[..w * h * 4].to_vec(),
        png::ColorType::GrayscaleAlpha => buf[..w * h * 2].chunks(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        _ => return None,
    };
    Some((w, h, rgba))
}

/// Opaque bounds over every frame: (x0, y0, x1, y1), exclusive ends.
fn bounds(w: usize, h: usize, frames: &[&[u8]]) -> Option<(usize, usize, usize, usize)> {
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for f in frames {
        for y in 0..h {
            for x in 0..w {
                if f[(y * w + x) * 4 + 3] > ALPHA_MIN {
                    (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
                }
            }
        }
    }
    (x1 > x0).then_some((x0, y0, x1, y1))
}

/// Crops to `b`, scales nearest-neighbour to fit `ICON × (ICON − 1)`, centred, bottom at `ICON − lift`.
fn fit(w: usize, src: &[u8], b: (usize, usize, usize, usize), lift: usize) -> Vec<u8> {
    let (bw, bh) = (b.2 - b.0, b.3 - b.1);
    let scale = (ICON as f64 / bw as f64).min((ICON - 1) as f64 / bh as f64);
    let (dw, dh) = (((bw as f64 * scale).round() as usize).max(1), ((bh as f64 * scale).round() as usize).max(1));
    let (ox, oy) = ((ICON - dw.min(ICON)) / 2, ICON - lift - dh.min(ICON - lift));
    let mut out = vec![0u8; ICON * ICON * 4];
    for y in 0..dh.min(ICON - lift) {
        for x in 0..dw.min(ICON) {
            let sx = b.0 + ((x as f64 + 0.5) / scale) as usize;
            let sy = b.1 + ((y as f64 + 0.5) / scale) as usize;
            let s = (sy.min(b.3 - 1) * w + sx.min(b.2 - 1)) * 4;
            let d = ((oy + y) * ICON + ox + x) * 4;
            out[d..d + 4].copy_from_slice(&src[s..s + 4]);
        }
    }
    out
}

/// Icon frames for an animated GIF; a single frame becomes the static bob.
pub fn gif_frames(bytes: &[u8], floor: f64) -> Option<Vec<Frame>> {
    let (w, h, frames) = decode_gif(bytes)?;
    if frames.len() == 1 {
        return static_frames(w, h, &frames[0].0);
    }
    let b = bounds(w, h, &frames.iter().map(|f| f.0.as_slice()).collect::<Vec<_>>())?;
    let capped = cap_frame_rate(&frames, floor);
    Some(capped.into_iter().map(|(img, d)| (fit(w, &img, b, 0), d)).collect())
}

/// Two-frame bob: resting, then 1 px up.
pub fn static_frames(w: usize, h: usize, rgba: &[u8]) -> Option<Vec<Frame>> {
    let b = bounds(w, h, &[rgba])?;
    Some(vec![(fit(w, rgba, b, 0), BOB_DELAY), (fit(w, rgba, b, 1), BOB_DELAY)])
}

fn url(species: u32, animated: bool, shiny: bool, back: bool) -> String {
    let path = if animated { "versions/generation-v/black-white/animated/" } else { "" };
    let ext = if animated { "gif" } else { "png" };
    format!("{BASE}/{path}{}{}{species}.{ext}", if back { "back/" } else { "" }, if shiny { "shiny/" } else { "" })
}

enum Fetch {
    Ok(Vec<u8>),
    Missing,
    Failed,
}

/// Disk cache in `<app data>/sprites`, then the network.
fn fetch(dir: &Path, name: &str, url: &str) -> Fetch {
    let file = dir.join(name);
    if let Ok(b) = fs::read(&file) {
        return Fetch::Ok(b);
    }
    match ureq::get(url).timeout(Duration::from_secs(15)).call() {
        Ok(r) => {
            let mut b = vec![];
            if std::io::Read::read_to_end(&mut r.into_reader(), &mut b).is_err() || b.is_empty() {
                return Fetch::Failed;
            }
            let _ = fs::create_dir_all(dir);
            let _ = fs::write(&file, &b);
            Fetch::Ok(b)
        }
        Err(ureq::Error::Status(404, _)) => Fetch::Missing,
        Err(_) => Fetch::Failed,
    }
}

struct Loader {
    dir: PathBuf,
    /// Back sprites confirmed missing this session: fall back to the front.
    missing: HashSet<String>,
}

impl Loader {
    fn load(&mut self, key: &Key) -> Option<Vec<Frame>> {
        let Some(id) = key.species else {
            let Fetch::Ok(b) = fetch(&self.dir, "egg.png", &format!("{BASE}/egg.png")) else { return None };
            let (w, h, rgba) = decode_png(&b)?;
            return static_frames(w, h, &rgba);
        };
        let sh = if key.shiny { "sh" } else { "" };
        let mut back = key.back;
        let name = |back: bool, a: &str| format!("{id}-{}{sh}{a}", if back { "back-" } else { "" });
        if back && self.missing.contains(&name(true, "a")) {
            back = false;
        }
        match fetch(&self.dir, &format!("{}.gif", name(back, "a")), &url(id, true, key.shiny, back)) {
            Fetch::Ok(b) => {
                if let Some(f) = gif_frames(&b, key.floor) {
                    return Some(f);
                }
            }
            Fetch::Missing if back => {
                self.missing.insert(name(true, "a"));
                return self.load(&Key { back: false, ..key.clone() });
            }
            _ => {}
        }
        let Fetch::Ok(b) = fetch(&self.dir, &format!("{}.png", name(false, "s")), &url(id, false, key.shiny, false)) else {
            return None;
        };
        let (w, h, rgba) = decode_png(&b)?;
        static_frames(w, h, &rgba)
    }
}

/// Runs for the app's lifetime (called once after the tray exists).
pub fn run(app: AppHandle, key_of: impl Fn(&AppHandle) -> Key) {
    let dir = app.path().app_data_dir().unwrap_or_default().join("sprites");
    let mut loader = Loader { dir, missing: HashSet::new() };
    let mut current: Option<Key> = None;
    let mut frames: Vec<Image<'static>> = vec![];
    let mut delays: Vec<f64> = vec![];
    let mut failed_at: Option<Instant> = None;
    let mut i = 0;
    loop {
        let key = key_of(&app);
        let retry = failed_at.is_some_and(|t| t.elapsed() >= RETRY);
        if current.as_ref() != Some(&key) || retry {
            match loader.load(&key) {
                Some(f) => {
                    delays = f.iter().map(|(_, d)| *d).collect();
                    frames = f.into_iter().map(|(rgba, _)| Image::new_owned(rgba, ICON as u32, ICON as u32)).collect();
                    failed_at = None;
                }
                None => {
                    frames.clear();
                    failed_at = Some(Instant::now());
                    crate::shell::tray::set_icon(&app, None);
                }
            }
            current = Some(key);
            i = 0;
        }
        if frames.is_empty() || crate::shell::popover_visible(&app) {
            if !frames.is_empty() && i == 0 {
                crate::shell::tray::set_icon(&app, Some(frames[0].clone()));
                i = 1;
            }
            std::thread::sleep(PAUSE_POLL);
            continue;
        }
        let n = i % frames.len();
        crate::shell::tray::set_icon(&app, Some(frames[n].clone()));
        std::thread::sleep(Duration::from_secs_f64(delays[n].max(0.02)));
        i = n + 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_frame_rate_without_slowing_the_loop() {
        let frames: Vec<(u32, f64)> = (0..55).map(|i| (i, 0.05)).collect();
        let out = cap_frame_rate(&frames, 0.4);
        assert_eq!(out.iter().map(|f| f.0).collect::<Vec<_>>(), [0, 8, 16, 24, 32, 40]);
        let total: f64 = out.iter().map(|f| f.1).sum();
        assert!((total - 2.75).abs() < 1e-9);
        assert!((out.last().unwrap().1 - 0.75).abs() < 1e-9); // 7-frame tail merged into the last frame
        // 0.05 × 4 accumulates to 0.19999…: the epsilon keeps buckets aligned.
        assert_eq!(cap_frame_rate(&frames[..8], 0.2).len(), 2);
        assert_eq!(cap_frame_rate(&frames[..3], 0.4), vec![(0, 0.15000000000000002)]);
        assert_eq!(cap_frame_rate(&frames[..1], 0.4).len(), 1);
        assert_eq!((gif_delay(0), gif_delay(1), gif_delay(5)), (0.1, 0.1, 0.05));
        assert_eq!((frame_floor("smooth", false), frame_floor("smooth", true), frame_floor("x", false)), (0.1, 0.4, 0.4));
    }

    #[test]
    fn fits_bottom_aligned_and_bobs() {
        // 4×2 opaque block inside a 10×10 transparent canvas.
        let (w, h) = (10, 10);
        let mut rgba = vec![0u8; w * h * 4];
        for y in 6..8 {
            for x in 3..7 {
                rgba[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&[255, 0, 0, 255]);
            }
        }
        let f = static_frames(w, h, &rgba).unwrap();
        let alpha = |img: &[u8], x: usize, y: usize| img[(y * ICON + x) * 4 + 3];
        // Width-bound: 32×16, bottom row on the last line, then lifted by 1.
        assert_eq!((alpha(&f[0].0, 0, 31), alpha(&f[0].0, 31, 16), alpha(&f[0].0, 0, 15)), (255, 255, 0));
        assert_eq!((alpha(&f[1].0, 0, 31), alpha(&f[1].0, 0, 30)), (0, 255));
        assert_eq!((f[0].1, f[1].1), (0.5, 0.5));
        assert!(static_frames(w, h, &vec![0u8; w * h * 4]).is_none());
    }
}
