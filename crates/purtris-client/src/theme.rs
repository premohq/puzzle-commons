//! Themes (Voyage, Tournament, Phosphor), presets, palettes, and the cell sprite painter.
//! Original art only: gems, blobs, capsules with tiny faces, charm tiles, sleepy stones.

use crate::gfx::{darken, hex, lighten, mix, Canvas};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeId {
    Voyage,
    Tournament,
    Phosphor,
}
impl ThemeId {
    pub const ALL: [ThemeId; 3] = [ThemeId::Voyage, ThemeId::Tournament, ThemeId::Phosphor];
    pub fn name(self) -> &'static str {
        match self {
            ThemeId::Voyage => "Voyage",
            ThemeId::Tournament => "Tournament",
            ThemeId::Phosphor => "Phosphor",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Voyage,
    Balanced,
    Potato,
}
impl Preset {
    pub const ALL: [Preset; 3] = [Preset::Voyage, Preset::Balanced, Preset::Potato];
    pub fn name(self) -> &'static str {
        match self {
            Preset::Voyage => "Voyage",
            Preset::Balanced => "Balanced",
            Preset::Potato => "Potato",
        }
    }
    pub fn particle_cap(self) -> usize {
        match self {
            Preset::Voyage => 700,
            Preset::Balanced => 160,
            Preset::Potato => 0,
        }
    }
}

// Base palette (original): red, orange, yellow, green, cyan, blue, purple, pink, gray
pub const RED: u32 = 0xFFE8_475A;
pub const ORANGE: u32 = 0xFFF5_9A3B;
pub const YELLOW: u32 = 0xFFF5_D547;
pub const GREEN: u32 = 0xFF5B_D07A;
pub const CYAN: u32 = 0xFF47_D1E8;
pub const BLUE: u32 = 0xFF4C_7BF2;
pub const PURPLE: u32 = 0xFFA6_6CF0;
pub const PINK: u32 = 0xFFF0_6CC4;
pub const GRAY: u32 = 0xFF8C_93A8;

/// Colorblind pattern per base color, so color is never the only signal.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Pattern {
    None,
    Circle,
    Triangle,
    Square,
    Diamond,
    Star,
    Heart,
    Cross,
    Bar,
}
pub fn pattern_for(c: u32) -> Pattern {
    match c {
        RED => Pattern::Heart,
        ORANGE => Pattern::Cross,
        YELLOW => Pattern::Star,
        GREEN => Pattern::Triangle,
        CYAN => Pattern::Bar,
        BLUE => Pattern::Diamond,
        PURPLE => Pattern::Circle,
        PINK => Pattern::Square,
        _ => Pattern::None,
    }
}

pub fn quad_color(i: u8) -> u32 {
    match i {
        1 => CYAN,
        2 => YELLOW,
        3 => PURPLE,
        4 => GREEN,
        5 => RED,
        6 => BLUE,
        7 => ORANGE,
        _ => GRAY,
    }
}
pub fn chain_color(i: u8) -> u32 {
    match i {
        1 => RED,
        2 => GREEN,
        3 => BLUE,
        4 => YELLOW,
        5 => PURPLE,
        _ => 0xFFD8_DCE8,
    }
}
pub fn capsule_color(i: u8) -> u32 {
    match i {
        1 => YELLOW,
        2 => RED,
        3 => BLUE,
        _ => GRAY,
    }
}
pub fn panel_color(i: u8) -> u32 {
    match i {
        1 => RED,
        2 => GREEN,
        3 => CYAN,
        4 => YELLOW,
        5 => PURPLE,
        6 => BLUE,
        _ => GRAY,
    }
}

/// Phosphor tints each color toward a CRT palette but keeps the patterns.
pub fn theme_tint(theme: ThemeId, c: u32) -> u32 {
    match theme {
        ThemeId::Phosphor => mix(c, 0xFF40_FF80, 70),
        _ => c,
    }
}

pub struct Look {
    pub bg_top: u32,
    pub bg_bot: u32,
    pub well: u32,
    pub well_alpha: u32,
    pub grid: u32,
    pub text: u32,
    pub dim: u32,
    pub accent: u32,
    pub warn: u32,
    pub good: u32,
}
pub fn look(t: ThemeId) -> Look {
    match t {
        ThemeId::Voyage => Look {
            bg_top: hex(0x0B1033),
            bg_bot: hex(0x2A0F3D),
            well: hex(0x070A1C),
            well_alpha: 200,
            grid: hex(0x2A3366),
            text: hex(0xF7F5F0),
            dim: hex(0xC9D0E4),
            accent: hex(0xFFD27A),
            warn: hex(0xFF5A6E),
            good: hex(0x6CF0A8),
        },
        ThemeId::Tournament => Look {
            bg_top: hex(0x15171C),
            bg_bot: hex(0x15171C),
            well: hex(0x000000),
            well_alpha: 255,
            grid: hex(0x262A33),
            text: hex(0xFFFFFF),
            dim: hex(0xA0A6B4),
            accent: hex(0xFFD23F),
            warn: hex(0xFF3B3B),
            good: hex(0x3BFF8A),
        },
        ThemeId::Phosphor => Look {
            bg_top: hex(0x020A04),
            bg_bot: hex(0x031208),
            well: hex(0x000000),
            well_alpha: 235,
            grid: hex(0x0E3018),
            text: hex(0x9CFFB8),
            dim: hex(0x4FAF6F),
            accent: hex(0xFFE36E),
            warn: hex(0xFF6060),
            good: hex(0x8CFFAA),
        },
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[allow(dead_code)]
pub enum CellKind {
    Gem,
    GemFace,
    Blob,
    Nuisance,
    HalfL,
    HalfR,
    HalfT,
    HalfB,
    Single,
    Virus,
    Tile,
    Stone,
    Ghost,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SpriteKey {
    pub kind: CellKind,
    pub color: u32,
    pub size: i32,
    pub face: bool,
    pub pattern: bool,
    pub flat: bool,
    pub frame: u8,
}

/// Piece sprites, painted on demand at the screen's real pixel size and cached.
pub struct Sprites {
    cache: HashMap<SpriteKey, Canvas>,
    scale: f32,
}
impl Sprites {
    pub fn new() -> Self {
        Sprites { cache: HashMap::new(), scale: 1.0 }
    }
    pub fn clear(&mut self) {
        self.cache.clear();
    }
    /// Match the canvas scale (layout units -> pixels); a change repaints everything.
    pub fn set_scale(&mut self, k: f32) {
        if (k - self.scale).abs() > 1e-4 {
            self.scale = k;
            self.cache.clear();
        }
    }
    /// `k.size` is in layout units.
    pub fn get(&mut self, mut k: SpriteKey) -> &Canvas {
        if self.cache.len() > 3000 {
            self.cache.clear();
        }
        k.size = ((k.size as f32 * self.scale).round() as i32).max(2);
        self.cache.entry(k).or_insert_with(|| paint(k))
    }
}

fn eyes(cv: &mut Canvas, cx: f32, cy: f32, s: f32, sleepy: bool, angry: bool) {
    let dx = s * 0.17;
    let r = (s * 0.075).max(1.0);
    let dark = 0xFF1A_1530;
    if sleepy {
        for sx in [-1.0, 1.0] {
            cv.line(cx + sx * dx - r, cy, cx + sx * dx + r, cy, (s * 0.05).max(1.0), dark, 230);
        }
        return;
    }
    for sx in [-1.0f32, 1.0] {
        cv.ellipse(cx + sx * dx, cy, r, r * 1.3, dark, 240);
        cv.circle(cx + sx * dx - r * 0.35, cy - r * 0.45, (r * 0.4).max(0.6), 0xFFFF_FFFF, 230);
        if angry {
            let x0 = cx + sx * dx - r * 1.4;
            let x1 = cx + sx * dx + r * 1.4;
            let (y0, y1) = if sx < 0.0 { (cy - r * 2.2, cy - r * 1.3) } else { (cy - r * 1.3, cy - r * 2.2) };
            cv.line(x0, y0, x1, y1, (s * 0.05).max(1.0), dark, 230);
        }
    }
}
fn smile(cv: &mut Canvas, cx: f32, cy: f32, s: f32) {
    let w = s * 0.12;
    let steps = 7;
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let a = std::f32::consts::PI * (0.15 + 0.7 * t);
        cv.circle(cx + a.cos() * w, cy + a.sin() * w * 0.6 - w * 0.2, (s * 0.03).max(0.6), 0xFF1A_1530, 220);
    }
    cv.circle(cx - s * 0.27, cy - s * 0.02, s * 0.06, 0xFFFF_8FB0, 90);
    cv.circle(cx + s * 0.27, cy - s * 0.02, s * 0.06, 0xFFFF_8FB0, 90);
}

pub fn draw_pattern(cv: &mut Canvas, p: Pattern, cx: f32, cy: f32, r: f32, c: u32, a: u32) {
    match p {
        Pattern::None => {}
        Pattern::Circle => cv.ring(cx, cy, r * 0.7, (r * 0.35).max(1.0), c, a),
        Pattern::Triangle => cv.tri([(cx, cy - r), (cx + r * 0.95, cy + r * 0.75), (cx - r * 0.95, cy + r * 0.75)], c, a),
        Pattern::Square => cv.rect_a((cx - r * 0.7) as i32, (cy - r * 0.7) as i32, (r * 1.4) as i32, (r * 1.4) as i32, c, a),
        Pattern::Diamond => {
            cv.tri([(cx, cy - r), (cx + r, cy), (cx, cy + r)], c, a);
            cv.tri([(cx, cy - r), (cx - r, cy), (cx, cy + r)], c, a);
        }
        Pattern::Star => cv.star(cx, cy, r * 1.15, c, a),
        Pattern::Heart => {
            cv.circle(cx - r * 0.45, cy - r * 0.2, r * 0.5, c, a);
            cv.circle(cx + r * 0.45, cy - r * 0.2, r * 0.5, c, a);
            cv.tri([(cx - r * 0.93, cy), (cx + r * 0.93, cy), (cx, cy + r)], c, a);
        }
        Pattern::Cross => {
            cv.line(cx - r * 0.7, cy - r * 0.7, cx + r * 0.7, cy + r * 0.7, (r * 0.35).max(1.0), c, a);
            cv.line(cx + r * 0.7, cy - r * 0.7, cx - r * 0.7, cy + r * 0.7, (r * 0.35).max(1.0), c, a);
        }
        Pattern::Bar => {
            cv.rect_a((cx - r) as i32, (cy - r * 0.25) as i32, (r * 2.0) as i32, (r * 0.5).max(1.0) as i32, c, a);
            cv.rect_a((cx - r * 0.25) as i32, (cy - r) as i32, (r * 0.5).max(1.0) as i32, (r * 2.0) as i32, c, a);
        }
    }
}

fn color_name(c: u32) -> Option<&'static str> {
    Some(match c {
        RED => "red",
        ORANGE => "orange",
        YELLOW => "yellow",
        GREEN => "green",
        CYAN => "cyan",
        BLUE => "blue",
        PURPLE => "purple",
        GRAY => "gray",
        0xFFD8_DCE8 => "white",
        _ => return None,
    })
}

/// The painted piece sprite for this key (tools/sprites/pieces.py), with faces and patterns
/// layered on top.
fn art_sprite(k: SpriteKey) -> Option<Canvas> {
    let c = color_name(k.color)?;
    let name = match k.kind {
        CellKind::Gem | CellKind::GemFace => format!("gem_{}", c),
        CellKind::Blob => format!("blob_{}", c),
        CellKind::Nuisance => "blob_white".to_string(),
        CellKind::HalfL => format!("half_{}", c),
        CellKind::HalfR => format!("halfr_{}", c),
        CellKind::HalfT => format!("halft_{}", c),
        CellKind::HalfB => format!("halfb_{}", c),
        CellKind::Single => format!("single_{}", c),
        CellKind::Virus => format!("virus_{}_{}", c, k.frame % 2),
        CellKind::Tile => format!("tile_{}", c),
        CellKind::Stone => "stone".to_string(),
        CellKind::Ghost => return None,
    };
    let src = crate::assets::art().get(&name)?;
    let s = k.size.max(2);
    let mut cv = crate::assets::resample(src, s, s);
    ink_outline(&mut cv, darken(k.color, 150));
    let sf = s as f32;
    let pat = if k.pattern { pattern_for(k.color) } else { Pattern::None };
    match k.kind {
        CellKind::Gem | CellKind::GemFace => {
            if pat != Pattern::None {
                draw_pattern(&mut cv, pat, sf * 0.5, sf * 0.54, sf * 0.13, lighten(k.color, 200), 130);
            }
            if k.kind == CellKind::GemFace && k.face && s >= 14 {
                eyes(&mut cv, sf * 0.5, sf * 0.42, sf, false, false);
                smile(&mut cv, sf * 0.5, sf * 0.66, sf);
            }
        }
        CellKind::Blob => {
            if pat != Pattern::None {
                draw_pattern(&mut cv, pat, sf * 0.5, sf * 0.56, sf * 0.13, lighten(k.color, 150), 130);
            }
            if k.face && s >= 14 {
                eyes(&mut cv, sf * 0.5, sf * 0.48, sf, false, false);
            }
        }
        CellKind::HalfL | CellKind::HalfR | CellKind::HalfT | CellKind::HalfB | CellKind::Single => {
            if pat != Pattern::None {
                draw_pattern(&mut cv, pat, sf * 0.5, sf * 0.7, sf * 0.11, darken(k.color, 130), 190);
            }
            if k.face && s >= 16 {
                let d = 0xFF1A_1530;
                cv.circle(sf * 0.38, sf * 0.45, sf * 0.05, d, 220);
                cv.circle(sf * 0.62, sf * 0.45, sf * 0.05, d, 220);
                cv.circle(sf * 0.5, sf * 0.58, sf * 0.04, d, 160);
            }
        }
        CellKind::Virus => {
            if pat != Pattern::None && s >= 12 {
                draw_pattern(&mut cv, pat, sf * 0.5, sf * 0.74, sf * 0.1, darken(k.color, 140), 190);
            }
            if s >= 12 {
                eyes(&mut cv, sf * 0.5, sf * 0.46, sf * 0.9, false, true);
            }
        }
        CellKind::Tile => {
            let p = if pat == Pattern::None { Pattern::Circle } else { pat };
            draw_pattern(&mut cv, p, sf * 0.5, sf * 0.5, sf * 0.22, lighten(k.color, 210), 235);
            draw_pattern(&mut cv, p, sf * 0.5, sf * 0.5, sf * 0.14, k.color, 170);
            if k.face && s >= 18 {
                let d = 0xFF1A_1530;
                cv.circle(sf * 0.42, sf * 0.47, sf * 0.035, d, 220);
                cv.circle(sf * 0.58, sf * 0.47, sf * 0.035, d, 220);
            }
        }
        _ => {}
    }
    Some(cv)
}

/// A thin inked edge around the sprite silhouette so pieces read against any background.
fn ink_outline(cv: &mut Canvas, ink: u32) {
    let (w, h) = (cv.w, cv.h);
    let alpha: Vec<u32> = cv.px.iter().map(|p| p >> 24).collect();
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            if alpha[i] >= 200 {
                continue;
            }
            let mut m = 0;
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx >= 0 && ny >= 0 && nx < w && ny < h {
                    m = m.max(alpha[(ny * w + nx) as usize]);
                }
            }
            if m > alpha[i] + 60 {
                let a = (m - alpha[i]).min(220);
                let under = cv.px[i];
                let mixed = crate::gfx::mix(if alpha[i] > 0 { under } else { ink }, ink, a);
                cv.px[i] = ((alpha[i].max(a)) << 24) | (mixed & 0xFF_FFFF);
            }
        }
    }
}

fn paint(k: SpriteKey) -> Canvas {
    if !k.flat {
        if let Some(cv) = art_sprite(k) {
            return cv;
        }
    }
    let s = k.size.max(2);
    let mut cv = Canvas::transparent(s, s);
    let sf = s as f32;
    let c = k.color;
    let pat = if k.pattern { pattern_for(c) } else { Pattern::None };
    let light = lighten(c, 110);
    let dark = darken(c, 90);
    let pad = (s / 14).max(1);
    match k.kind {
        CellKind::Gem | CellKind::GemFace => {
            if k.flat {
                cv.rect(0, 0, s, s, darken(c, 60));
                cv.rect(1, 1, s - 2, s - 2, c);
            } else {
                let r = s / 5;
                cv.round_rect(pad, pad, s - 2 * pad, s - 2 * pad, r, dark, 255);
                cv.round_grad(pad + 1, pad + 1, s - 2 * pad - 2, s - 2 * pad - 2, r - 1, light, c, 255);
                cv.round_rect(pad + s / 6, pad + s / 8, s / 3, s / 7, s / 14, 0xFFFF_FFFF, 110);
            }
            if pat != Pattern::None {
                draw_pattern(&mut cv, pat, sf * 0.5, sf * 0.52, sf * 0.16, if k.flat { darken(c, 120) } else { lighten(c, 190) }, 170);
            }
            if k.kind == CellKind::GemFace && k.face && s >= 14 {
                eyes(&mut cv, sf * 0.5, sf * 0.42, sf, false, false);
                smile(&mut cv, sf * 0.5, sf * 0.66, sf);
            }
        }
        CellKind::Ghost => {
            let th = (s / 14).max(1);
            cv.round_rect(pad, pad, s - 2 * pad, s - 2 * pad, s / 5, c, 38);
            cv.round_ring(pad, pad, s - 2 * pad, s - 2 * pad, s / 5, th, lighten(c, 60), c, 190);
        }
        CellKind::Blob | CellKind::Nuisance => {
            let r = sf * 0.46;
            if k.flat {
                cv.circle(sf / 2.0, sf / 2.0, r, c, 255);
            } else {
                cv.ellipse(sf / 2.0, sf * 0.53, r, r * 0.94, dark, 255);
                cv.ellipse(sf / 2.0, sf * 0.5, r * 0.9, r * 0.86, c, 255);
                cv.ellipse(sf * 0.5, sf * 0.42, r * 0.7, r * 0.55, light, 120);
                cv.ellipse(sf * 0.36, sf * 0.3, r * 0.22, r * 0.14, 0xFFFF_FFFF, 170);
            }
            if k.kind == CellKind::Nuisance {
                cv.ellipse(sf / 2.0, sf * 0.5, r * 0.5, r * 0.5, darken(c, 40), 120);
            } else {
                if pat != Pattern::None {
                    draw_pattern(&mut cv, pat, sf * 0.78, sf * 0.78, sf * 0.12, lighten(c, 200), 190);
                }
                if k.face && s >= 14 {
                    eyes(&mut cv, sf * 0.5, sf * 0.48, sf, false, false);
                }
            }
        }
        CellKind::HalfL | CellKind::HalfR | CellKind::HalfT | CellKind::HalfB | CellKind::Single => {
            let r = s / 2 - pad;
            // half-capsule: rounded on the outer side, flat on the partner side
            cv.round_rect(pad, pad, s - 2 * pad, s - 2 * pad, r, dark, 255);
            cv.round_rect(pad + 1, pad + 1, s - 2 * pad - 2, s - 2 * pad - 2, r - 1, c, 255);
            match k.kind {
                CellKind::HalfL => {
                    cv.rect(s / 2, pad, s - s / 2, s - 2 * pad, dark);
                    cv.rect(s / 2, pad + 1, s - s / 2, s - 2 * pad - 2, c);
                }
                CellKind::HalfR => {
                    cv.rect(0, pad, s / 2, s - 2 * pad, dark);
                    cv.rect(0, pad + 1, s / 2, s - 2 * pad - 2, c);
                }
                CellKind::HalfT => {
                    cv.rect(pad, s / 2, s - 2 * pad, s - s / 2, dark);
                    cv.rect(pad + 1, s / 2, s - 2 * pad - 2, s - s / 2, c);
                }
                CellKind::HalfB => {
                    cv.rect(pad, 0, s - 2 * pad, s / 2, dark);
                    cv.rect(pad + 1, 0, s - 2 * pad - 2, s / 2, c);
                }
                _ => {}
            }
            if !k.flat {
                cv.round_rect(s / 4, s / 5, s / 3, s / 7, s / 14, 0xFFFF_FFFF, 120);
            }
            if pat != Pattern::None {
                draw_pattern(&mut cv, pat, sf * 0.5, sf * 0.72, sf * 0.12, darken(c, 130), 170);
            }
            if k.face && s >= 16 {
                let d = 0xFF1A_1530;
                cv.circle(sf * 0.38, sf * 0.45, sf * 0.05, d, 220);
                cv.circle(sf * 0.62, sf * 0.45, sf * 0.05, d, 220);
                cv.circle(sf * 0.5, sf * 0.58, sf * 0.04, d, 160);
            }
        }
        CellKind::Virus => {
            let r = sf * 0.36;
            let wob = if k.frame % 2 == 0 { 0.0 } else { 0.05 };
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::PI / 4.0 + wob;
                cv.circle(sf / 2.0 + a.cos() * r * 1.1, sf / 2.0 + a.sin() * r * 1.1, sf * 0.08, dark, 255);
            }
            cv.circle(sf / 2.0, sf / 2.0, r, dark, 255);
            cv.circle(sf / 2.0, sf / 2.0, r * 0.9, c, 255);
            cv.circle(sf * 0.42, sf * 0.38, r * 0.3, light, 150);
            if pat != Pattern::None && s >= 12 {
                draw_pattern(&mut cv, pat, sf * 0.5, sf * 0.72, sf * 0.1, darken(c, 140), 180);
            }
            if s >= 12 {
                eyes(&mut cv, sf * 0.5, sf * 0.47, sf * 0.9, false, true);
            }
        }
        CellKind::Tile => {
            if k.flat {
                cv.rect(0, 0, s, s, darken(c, 70));
                cv.rect(1, 1, s - 2, s - 2, c);
            } else {
                let r = s / 6;
                cv.round_rect(pad, pad, s - 2 * pad, s - 2 * pad, r, darken(c, 110), 255);
                cv.round_grad(pad + 2, pad + 2, s - 2 * pad - 4, s - 2 * pad - 4, r, lighten(c, 70), darken(c, 20), 255);
            }
            // Panel charm symbol (doubles as the colorblind pattern)
            let p = if pat == Pattern::None { Pattern::Circle } else { pat };
            draw_pattern(&mut cv, p, sf * 0.5, sf * 0.5, sf * 0.24, lighten(c, 200), 220);
            draw_pattern(&mut cv, p, sf * 0.5, sf * 0.5, sf * 0.16, c, 160);
            if k.face && s >= 18 {
                let d = 0xFF1A_1530;
                cv.circle(sf * 0.42, sf * 0.47, sf * 0.035, d, 220);
                cv.circle(sf * 0.58, sf * 0.47, sf * 0.035, d, 220);
            }
        }
        CellKind::Stone => {
            cv.round_rect(pad, pad, s - 2 * pad, s - 2 * pad, s / 6, hex(0x50566A), 255);
            cv.round_rect(pad + 2, pad + 2, s - 2 * pad - 4, s - 2 * pad - 4, s / 7, mix(GRAY, c, 60), 255);
            cv.circle(sf * 0.3, sf * 0.35, sf * 0.06, hex(0x6A7086), 200);
            cv.circle(sf * 0.7, sf * 0.66, sf * 0.08, hex(0x6A7086), 200);
        }
    }
    cv
}

