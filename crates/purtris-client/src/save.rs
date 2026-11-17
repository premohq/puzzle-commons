//! Settings, story progress, lesson bests, the player card, and replay files.
//! Plain key=value text in the per-user config folder. None of this enters a match header.

use crate::input::{default_bindings, key_from_name, Handling};
use crate::theme::{Preset, ThemeId};
use minifb::Key;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub fn data_dir() -> PathBuf {
    #[cfg(windows)]
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    #[cfg(target_os = "macos")]
    let base = std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support")).unwrap_or_else(|| PathBuf::from("."));
    #[cfg(not(any(windows, target_os = "macos")))]
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    let d = base.join("Puzzle Commons");
    let _ = std::fs::create_dir_all(d.join("replays"));
    d
}

fn read_kv(name: &str) -> BTreeMap<String, String> {
    let mut m = BTreeMap::new();
    if let Ok(s) = std::fs::read_to_string(data_dir().join(name)) {
        for line in s.lines() {
            if let Some((k, v)) = line.split_once('=') {
                m.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }
    m
}
fn write_kv(name: &str, m: &BTreeMap<String, String>) {
    let mut s = String::new();
    for (k, v) in m {
        s.push_str(k);
        s.push('=');
        s.push_str(v);
        s.push('\n');
    }
    let _ = std::fs::write(data_dir().join(name), s);
}

/// How the game window is shown.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum WindowMode {
    Windowed,
    /// a borderless window covering the primary monitor (and the WM's fullscreen state)
    Fullscreen,
}

/// Windowed sizes offered (16:9); larger than the monitor ones are hidden.
pub const WINDOW_SIZES: [(u32, u32); 6] = [(1280, 720), (1600, 900), (1920, 1080), (2240, 1260), (2560, 1440), (3840, 2160)];
/// Render widths: 0 = native (match the window). Others render at that size and scale.
pub const RENDER_WIDTHS: [u32; 6] = [0, 1280, 1600, 1920, 2560, 3840];
/// Frame caps: 1 = match the display's refresh rate, 0 = unlimited.
pub const FPS_CAPS: [u32; 7] = [1, 60, 120, 144, 165, 240, 0];

#[derive(Clone)]
pub struct Settings {
    pub theme: ThemeId,
    pub preset: Preset,
    pub benched: bool,
    pub colorblind: bool,
    pub decorations: bool,
    pub characters: bool,
    /// show the characters beside the boards in versus (always on in the story)
    pub chars_versus: bool,
    pub reduce_motion: bool,
    pub shake: bool,
    pub music_vol: u32,
    pub sfx_vol: u32,
    pub handling: Handling,
    pub socd_neutral: bool,
    pub analog: bool,
    pub input_display: bool,
    pub window_mode: WindowMode,
    pub window_size: (u32, u32),
    pub render_w: u32,
    /// scaling filter when the render size differs from the window: smooth (bilinear) or sharp
    pub smooth: bool,
    pub fps_cap: u32,
    pub show_fps: bool,
    pub bindings: Vec<Vec<Vec<Key>>>,
    pub first_boot: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: ThemeId::Voyage,
            preset: Preset::Voyage,
            benched: false,
            colorblind: true,
            decorations: false,
            characters: true,
            chars_versus: true,
            reduce_motion: false,
            shake: false,
            music_vol: 6,
            sfx_vol: 8,
            handling: Handling::default(),
            socd_neutral: true,
            analog: false,
            input_display: false,
            window_mode: WindowMode::Fullscreen,
            window_size: (1600, 900),
            render_w: 0,
            smooth: true,
            fps_cap: 1,
            show_fps: false,
            bindings: default_bindings(),
            first_boot: true,
        }
    }
}

impl Settings {
    pub fn load() -> Self {
        let m = read_kv("settings.txt");
        let mut s = Settings::default();
        if m.is_empty() {
            return s;
        }
        s.first_boot = false;
        // v2: the old F1 hotkey silently saved the flat Tournament theme, and the old renderer
        // was slow enough to fail the boot bench; start everyone once from the full look
        let version: u32 = m.get("settings_version").and_then(|v| v.parse().ok()).unwrap_or(1);
        let b = |k: &str, d: bool| m.get(k).map(|v| v == "1").unwrap_or(d);
        let n = |k: &str, d: u32| m.get(k).and_then(|v| v.parse().ok()).unwrap_or(d);
        s.theme = ThemeId::ALL[n("theme", 0).min(2) as usize];
        s.preset = Preset::ALL[n("preset", 0).min(2) as usize];
        s.benched = b("benched", false);
        s.colorblind = b("colorblind", true);
        s.decorations = b("decorations2", false);
        s.characters = b("characters", true);
        s.chars_versus = b("chars_versus", true);
        s.reduce_motion = b("reduce_motion", false);
        s.shake = b("shake", false);
        s.music_vol = n("music_vol", 6).min(10);
        s.sfx_vol = n("sfx_vol", 8).min(10);
        s.handling.das = n("das", 10).clamp(1, 30);
        s.handling.arr = n("arr", 2).min(10);
        s.handling.sdf = n("sdf", 1).min(10);
        s.socd_neutral = b("socd_neutral", true);
        s.analog = b("analog", false);
        s.input_display = b("input_display", false);
        s.window_mode = if m.get("window_mode").map(|v| v == "windowed").unwrap_or(false) { WindowMode::Windowed } else { WindowMode::Fullscreen };
        if let Some((w, h)) = m.get("window_size").and_then(|v| v.split_once('x')).and_then(|(a, b)| Some((a.parse().ok()?, b.parse().ok()?))) {
            if WINDOW_SIZES.contains(&(w, h)) {
                s.window_size = (w, h);
            }
        }
        // any width the monitor offered (or a preset); 0 = native
        s.render_w = n("render_w", 0);
        if s.render_w != 0 && !(320..=7680).contains(&s.render_w) {
            s.render_w = 0;
        }
        s.smooth = b("smooth", true);
        // 0 unlimited, 1 match display, otherwise a refresh rate in Hz
        s.fps_cap = n("fps_cap", 1);
        if s.fps_cap > 1 && !(24..=500).contains(&s.fps_cap) {
            s.fps_cap = 1;
        }
        s.show_fps = b("show_fps", false);
        if version < 2 {
            s.theme = ThemeId::Voyage;
            s.preset = Preset::Voyage;
            s.benched = true;
            // the game changed a lot since: show Mira's setup and tour once
            s.first_boot = true;
        }
        for (li, layout) in s.bindings.iter_mut().enumerate() {
            for (ai, keys) in layout.iter_mut().enumerate() {
                if let Some(v) = m.get(&format!("bind.{}.{}", li, ai)) {
                    let ks: Vec<Key> = v.split(',').filter_map(key_from_name).collect();
                    if !ks.is_empty() {
                        *keys = ks;
                    }
                }
            }
        }
        s
    }
    pub fn save(&self) {
        let mut m = BTreeMap::new();
        let mut put = |k: &str, v: String| {
            m.insert(k.to_string(), v);
        };
        put("settings_version", "2".to_string());
        put("theme", (ThemeId::ALL.iter().position(|&t| t == self.theme).unwrap()).to_string());
        put("preset", (Preset::ALL.iter().position(|&t| t == self.preset).unwrap()).to_string());
        put("benched", (self.benched as u8).to_string());
        put("colorblind", (self.colorblind as u8).to_string());
        put("decorations2", (self.decorations as u8).to_string());
        put("characters", (self.characters as u8).to_string());
        put("chars_versus", (self.chars_versus as u8).to_string());
        put("reduce_motion", (self.reduce_motion as u8).to_string());
        put("shake", (self.shake as u8).to_string());
        put("music_vol", self.music_vol.to_string());
        put("sfx_vol", self.sfx_vol.to_string());
        put("das", self.handling.das.to_string());
        put("arr", self.handling.arr.to_string());
        put("sdf", self.handling.sdf.to_string());
        put("socd_neutral", (self.socd_neutral as u8).to_string());
        put("analog", (self.analog as u8).to_string());
        put("input_display", (self.input_display as u8).to_string());
        put("window_mode", if self.window_mode == WindowMode::Windowed { "windowed" } else { "fullscreen" }.to_string());
        put("window_size", format!("{}x{}", self.window_size.0, self.window_size.1));
        put("render_w", self.render_w.to_string());
        put("smooth", (self.smooth as u8).to_string());
        put("fps_cap", self.fps_cap.to_string());
        put("show_fps", (self.show_fps as u8).to_string());
        for (li, layout) in self.bindings.iter().enumerate() {
            for (ai, keys) in layout.iter().enumerate() {
                let v: Vec<String> = keys.iter().map(|k| format!("{:?}", k)).collect();
                put(&format!("bind.{}.{}", li, ai), v.join(","));
            }
        }
        write_kv("settings.txt", &m);
    }
}

/// Story progress: per node, three bits for difficulties cleared and a bit per objective.
#[derive(Clone, Default)]
pub struct Progress {
    pub nodes: BTreeMap<String, u32>,
    pub lessons: BTreeMap<String, u32>,
    pub records: BTreeMap<String, u64>,
}
impl Progress {
    pub fn load() -> Self {
        let mut p = Progress::default();
        for (k, v) in read_kv("progress.txt") {
            if let Some(n) = k.strip_prefix("node.") {
                p.nodes.insert(n.to_string(), v.parse().unwrap_or(0));
            } else if let Some(n) = k.strip_prefix("lesson.") {
                p.lessons.insert(n.to_string(), v.parse().unwrap_or(0));
            } else if let Some(n) = k.strip_prefix("record.") {
                p.records.insert(n.to_string(), v.parse().unwrap_or(0));
            }
        }
        p
    }
    pub fn save(&self) {
        let mut m = BTreeMap::new();
        for (k, v) in &self.nodes {
            m.insert(format!("node.{}", k), v.to_string());
        }
        for (k, v) in &self.lessons {
            m.insert(format!("lesson.{}", k), v.to_string());
        }
        for (k, v) in &self.records {
            m.insert(format!("record.{}", k), v.to_string());
        }
        write_kv("progress.txt", &m);
    }
    pub fn node(&self, id: &str) -> u32 {
        self.nodes.get(id).copied().unwrap_or(0)
    }
}

#[derive(Clone)]
pub struct Card {
    /// the cast member you play as outside the story (cast id)
    pub character: String,
    pub name: String,
    pub bio: String,
    pub country: String,
    pub favorite: usize,
    /// the story's protagonist, as the player named her (cast::hero); empty until asked
    pub hero: String,
    /// the courier's model: false = girl (Mira's), true = boy
    pub hero_boy: bool,
}
impl Card {
    pub fn character(&self) -> &str {
        &self.character
    }
    pub fn load() -> Self {
        let m = read_kv("card.txt");
        let card = Card {
            character: m.get("character").filter(|c| crate::cast::CAST.iter().any(|x| x.id == c.as_str())).cloned().unwrap_or_else(|| "mira".into()),
            name: m.get("name").cloned().unwrap_or_else(|| "PLAYER".into()),
            bio: m.get("bio").cloned().unwrap_or_default(),
            country: m.get("country").cloned().unwrap_or_default(),
            favorite: m.get("favorite").and_then(|v| v.parse().ok()).unwrap_or(0),
            hero: m.get("hero").map(|h| crate::cast::clean_hero(h)).unwrap_or_default(),
            hero_boy: m.get("hero_model").map(|v| v == "boy").unwrap_or(false),
        };
        crate::cast::set_hero(&card.hero);
        crate::cast::set_hero_boy(card.hero_boy);
        card
    }
    pub fn save(&self) {
        let mut m = BTreeMap::new();
        m.insert("name".into(), self.name.clone());
        m.insert("character".into(), self.character.clone());
        m.insert("bio".into(), self.bio.clone());
        m.insert("country".into(), self.country.clone());
        m.insert("favorite".into(), self.favorite.to_string());
        if !self.hero.is_empty() {
            m.insert("hero".into(), self.hero.clone());
        }
        m.insert("hero_model".into(), if self.hero_boy { "boy" } else { "girl" }.into());
        crate::cast::set_hero(&self.hero);
        crate::cast::set_hero_boy(self.hero_boy);
        write_kv("card.txt", &m);
    }
}

pub fn save_replay(bytes: &[u8], label: &str) -> Option<PathBuf> {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let clean: String = label.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').take(24).collect();
    let p = data_dir().join("replays").join(format!("{}-{}.purtris", secs, clean));
    std::fs::write(&p, bytes).ok().map(|_| p)
}
pub fn list_replays() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(data_dir().join("replays"))
        .map(|r| r.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.extension().map(|x| x == "purtris").unwrap_or(false)).collect())
        .unwrap_or_default();
    v.sort();
    v.reverse();
    v
}
