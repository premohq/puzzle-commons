//! Match effects: what a clear, a chain, an attack or a KO looks like.
//!
//! Everything here is client-side. It reads the sim's public events after each tick and never
//! feeds anything back, so replays and determinism are untouched. The look is toon, to match
//! the boards: crisp cel shards and droplets with ink outlines, solid shockwave rings that thin
//! out as they grow, four-point sparkles with white-hot cores, and sticker-style labels (two-tone
//! fill, dark ink line, white rim) that slam in. Every sprite is rasterized here at the screen's
//! real pixel size, so nothing is a scaled bitmap and it all stays sharp from 720p to 4K.
//!
//! Intensity scales with a tier (0..4) worked out from the move: a single is a few shards, a
//! 7-chain or a KO is rings, rays, a board flash, a jolt, hit-stop and (with the setting on) a
//! screen flash and shake. Reduce motion drops the movement-heavy parts (slams, rays, jolts,
//! shake, hit-stop); the Tournament theme gets plain text callouts only.

use crate::font::{self, Weight};
use crate::fxdraw::{self as fd, Mode};
use crate::gfx::{darken, lighten, mix, Canvas};
use crate::render::Geom;
use crate::save::Settings;
use crate::theme::*;
use purtris_sim::{Ev, Match, Outcome, SeatKind, Style};
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};
use std::sync::Arc;

const INK: u32 = 0xFF16_1030;

// ------------------------------------------------------------------ quality from settings

#[derive(Clone, Copy)]
struct Q {
    /// anything beyond plain callouts (off in Tournament)
    on: bool,
    cap: usize,
    /// slams, rays, jolts, hit-stop
    motion: bool,
    /// whole-screen flashes and shake ("Screen shake and flashes")
    flashes: bool,
    /// the expensive extras (Voyage and Balanced)
    rays: bool,
}
fn quality(st: &Settings) -> Q {
    let on = st.theme != ThemeId::Tournament;
    Q {
        on,
        cap: if on { st.preset.particle_cap() } else { 0 },
        motion: on && !st.reduce_motion,
        flashes: on && st.shake && !st.reduce_motion,
        rays: on && !st.reduce_motion && st.preset != Preset::Potato,
    }
}

// ------------------------------------------------------------------ palettes

/// Label palettes. Chain tiers climb white, yellow, orange, red, violet, blue, then rainbow.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Pal {
    Plain,
    Tier(u8),
    Spin,
    Quad,
    Gold,
    Ko,
    Rainbow,
    Good,
}
/// (upper band, lower band, rim, ink)
fn pal(p: Pal) -> (u32, u32, u32, u32) {
    match p {
        Pal::Plain => (0xFFFF_FFFF, 0xFFC4_DDF5, 0xFF2A_3A66, INK),
        Pal::Tier(n) => match n {
            0 | 1 => (0xFFFF_FFFF, 0xFFBF_E6FF, 0xFFFF_FFFF, INK),
            2 => (0xFFFF_FBD8, 0xFFFF_D23A, 0xFFFF_FFFF, INK),
            3 => (0xFFFF_EBC4, 0xFFFF_8A24, 0xFFFF_FFFF, 0xFF2A_1010),
            4 => (0xFFFF_DCE4, 0xFFFF_3D6E, 0xFFFF_FFFF, 0xFF2A_0A1A),
            5 => (0xFFF5_E2FF, 0xFFB0_4DFF, 0xFFFF_FFFF, 0xFF1C_0A30),
            6 => (0xFFDC_F6FF, 0xFF26_B0FF, 0xFFFF_FFFF, 0xFF06_1A33),
            _ => (0xFFFF_FFFF, 0xFFFF_FFFF, 0xFFFF_E066, 0xFF1A_0A2A),
        },
        Pal::Spin => (0xFFFB_E3FF, 0xFFC0_4DFF, 0xFFFF_FFFF, 0xFF1C_0A30),
        Pal::Quad => (0xFFFF_FFFF, 0xFF3F_E0FF, 0xFFFF_FFFF, 0xFF06_1A33),
        Pal::Gold => (0xFFFF_F7CF, 0xFFFF_B400, 0xFFFF_FFFF, 0xFF2A_1A06),
        Pal::Ko => (0xFFFF_F2C2, 0xFFFF_3B3B, 0xFFFF_D54A, 0xFF2A_0A10),
        Pal::Rainbow => (0xFFFF_FFFF, 0xFFFF_FFFF, 0xFFFF_FFFF, INK),
        Pal::Good => (0xFFE6_FFF0, 0xFF4C_E08A, 0xFFFF_FFFF, 0xFF06_2A16),
    }
}
fn rainbow(p: Pal) -> bool {
    p == Pal::Rainbow || p == Pal::Tier(7) || matches!(p, Pal::Tier(n) if n > 7)
}
/// Hue in 0..1 to a saturated, toon-friendly color.
pub fn hue(h: f32) -> u32 {
    let h = h.rem_euclid(1.0) * 6.0;
    let i = h.floor() as i32;
    let f = h - i as f32;
    let (r, g, b) = match i {
        0 => (1.0, f, 0.0),
        1 => (1.0 - f, 1.0, 0.0),
        2 => (0.0, 1.0, f),
        3 => (0.0, 1.0 - f, 1.0),
        4 => (f, 0.0, 1.0),
        _ => (1.0, 0.0, 1.0 - f),
    };
    let c = |v: f32| ((0.30 + 0.70 * v) * 255.0) as u32;
    0xFF00_0000 | (c(r) << 16) | (c(g) << 8) | c(b)
}
/// The accent color of a move's tier (rings, rays, sparks).
fn tier_color(p: Pal, t: f32) -> u32 {
    if rainbow(p) {
        return hue(t * 0.8);
    }
    let (_, b, _, _) = pal(p);
    if p == Pal::Tier(0) || p == Pal::Tier(1) || p == Pal::Plain {
        0xFFCF_F0FF
    } else {
        b
    }
}
fn chain_pal(n: u32) -> Pal {
    Pal::Tier(n.min(7) as u8)
}
fn style_accent(s: Style) -> u32 {
    match s {
        Style::Quad => CYAN,
        Style::Chain => GREEN,
        Style::Capsule => PINK,
        Style::Panel => YELLOW,
    }
}

// ------------------------------------------------------------------ sprites

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum PK {
    /// cel crystal shard (Quad, Panel)
    Shard,
    /// gummy droplet (Chain, Capsule)
    Drop,
    /// four-point sparkle, added as light
    Spark,
    /// soft light (glows), added as light
    Dot,
    /// a projectile head's light: soft glow with a four-point glint, in one mask
    Halo,
    /// toon smoke puff
    Puff,
    /// five-point confetti star
    Star,
}

/// Supersampled sprite: `f(x, y)` gives a straight color and coverage for a point relative to
/// the sprite's centre (pixels). Output is premultiplied.
fn build(w: i32, h: i32, f: impl Fn(f32, f32) -> (u32, f32)) -> Canvas {
    let mut c = Canvas::new(w, h);
    let (hx, hy) = (w as f32 * 0.5, h as f32 * 0.5);
    for y in 0..h {
        for x in 0..w {
            let (mut r, mut g, mut b, mut a) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for sy in 0..3 {
                for sx in 0..3 {
                    let px = x as f32 + (sx as f32 + 0.5) / 3.0 - hx;
                    let py = y as f32 + (sy as f32 + 0.5) / 3.0 - hy;
                    let (col, al) = f(px, py);
                    if al > 0.0 {
                        r += ((col >> 16) & 0xFF) as f32 * al;
                        g += ((col >> 8) & 0xFF) as f32 * al;
                        b += (col & 0xFF) as f32 * al;
                        a += al;
                    }
                }
            }
            if a > 0.0 {
                let n = 1.0 / 9.0;
                c.px[(y * w + x) as usize] = (((a * n * 255.0) as u32).min(255) << 24) | (((r * n) as u32).min(255) << 16) | (((g * n) as u32).min(255) << 8) | ((b * n) as u32).min(255);
            }
        }
    }
    c
}
/// A light mask: `f` gives intensity 0..1, stored in the alpha byte.
fn build_mask(w: i32, h: i32, f: impl Fn(f32, f32) -> f32) -> Canvas {
    build(w, h, |x, y| (0, f(x, y).clamp(0.0, 1.0)))
}

fn star_points(r: f32, inner: f32, rot: f32) -> [(f32, f32); 10] {
    let mut p = [(0.0, 0.0); 10];
    for (i, q) in p.iter_mut().enumerate() {
        let a = -PI / 2.0 + rot + i as f32 * PI / 5.0;
        let rr = if i % 2 == 0 { r } else { r * inner };
        *q = (a.cos() * rr, a.sin() * rr);
    }
    p
}
fn in_poly(p: &[(f32, f32)], x: f32, y: f32) -> bool {
    let mut inside = false;
    let n = p.len();
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = p[i];
        let (xj, yj) = p[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn make_sprite(kind: PK, color: u32, s: i32, rot: u8) -> Canvas {
    let sf = s as f32;
    let ink = darken(color, 150);
    let o = (sf * 0.09).clamp(1.0, 2.6);
    match kind {
        PK::Shard => {
            let th = rot as f32 / 8.0 * PI;
            let (l, wd) = (sf * 0.5, sf * 0.25);
            let d = (sf + 2.0 * o).ceil() as i32 + 2;
            let (cs, sn) = (th.cos(), th.sin());
            let light = lighten(color, 110);
            build(d, d, move |x, y| {
                let xr = x * cs + y * sn;
                let yr = -x * sn + y * cs;
                let dd = xr.abs() / l + yr.abs() / wd;
                if dd <= 1.0 {
                    // a lit facet, a shaded facet and a glint near the tip
                    if yr < 0.0 && xr < -l * 0.25 && xr > -l * 0.62 && yr > -wd * 0.45 {
                        (0xFFFF_FFFF, 1.0)
                    } else if yr < 0.0 {
                        (light, 1.0)
                    } else {
                        (color, 1.0)
                    }
                } else if xr.abs() / (l + o * 1.8) + yr.abs() / (wd + o) <= 1.0 {
                    (ink, 1.0)
                } else {
                    (0, 0.0)
                }
            })
        }
        PK::Drop => {
            let r = sf * 0.5;
            let d = (sf + 2.0).ceil() as i32 + 1;
            let shade = mix(darken(color, 55), 0xFF30_2060, 40);
            build(d, d, move |x, y| {
                let rr = (x * x + y * y).sqrt();
                if rr > r {
                    return (0, 0.0);
                }
                if rr > r - o {
                    return (ink, 1.0);
                }
                let hx = (x + r * 0.32) / (r * 0.30);
                let hy = (y + r * 0.40) / (r * 0.18);
                if hx * hx + hy * hy <= 1.0 {
                    return (0xFFFF_FFFF, 1.0);
                }
                let sx = x + r * 0.24;
                let sy = y + r * 0.24;
                if (sx * sx + sy * sy).sqrt() > r * 0.94 {
                    (shade, 1.0)
                } else {
                    (color, 1.0)
                }
            })
        }
        PK::Star => {
            let r = sf * 0.5;
            let rot = rot as f32 / 8.0 * (TAU / 5.0);
            let pts = star_points(r - o, 0.46, rot);
            let outer = star_points(r, 0.52, rot);
            let d = (sf + 2.0).ceil() as i32 + 1;
            let light = lighten(color, 120);
            build(d, d, move |x, y| {
                if in_poly(&pts, x, y) {
                    (if x * 0.6 + y < 0.0 { light } else { color }, 1.0)
                } else if in_poly(&outer, x, y) {
                    (ink, 1.0)
                } else {
                    (0, 0.0)
                }
            })
        }
        PK::Puff => {
            let r = sf * 0.5;
            let lobes = [(-0.36f32, 0.14f32, 0.50f32), (0.34, 0.18, 0.46), (0.0, -0.16, 0.60), (0.06, 0.28, 0.42)];
            let shade = mix(darken(color, 50), 0xFF40_4878, 60);
            let d = (sf * 1.25 + 2.0 * o).ceil() as i32 + 2;
            let inside = move |x: f32, y: f32, grow: f32| lobes.iter().any(|&(cx, cy, rr)| {
                let (dx, dy) = (x - cx * r, y - cy * r);
                dx * dx + dy * dy <= (rr * r + grow) * (rr * r + grow)
            });
            build(d, d, move |x, y| {
                if inside(x, y, 0.0) {
                    if inside(x + r * 0.16, y + r * 0.16, 0.0) {
                        (color, 1.0)
                    } else {
                        (shade, 1.0)
                    }
                } else if inside(x, y, o) {
                    (darken(color, 140), 1.0)
                } else {
                    (0, 0.0)
                }
            })
        }
        PK::Dot => {
            let r = sf * 0.5;
            let d = s.max(3) | 1;
            build_mask(d, d, move |x, y| {
                let t = 1.0 - (x * x + y * y) / (r * r);
                if t > 0.0 {
                    t * t
                } else {
                    0.0
                }
            })
        }
        PK::Halo => {
            let r = sf * 0.5;
            let d = s.max(3) | 1;
            build_mask(d, d, move |x, y| {
                let t = (1.0 - (x * x + y * y) / (r * r)).max(0.0);
                let a = x.abs() / (r * 0.75) + y.abs() / (r * 0.08);
                let b = x.abs() / (r * 0.08) + y.abs() / (r * 0.75);
                let arms = if a <= 1.0 || b <= 1.0 { 1.0 - a.min(b) * 0.5 } else { 0.0 };
                (t * t * 0.75).max(arms)
            })
        }
        PK::Spark => {
            let r = sf * 0.5;
            let d = s.max(3) | 1;
            build_mask(d, d, move |x, y| {
                let a = x.abs() / r + y.abs() / (r * 0.14);
                let b = x.abs() / (r * 0.14) + y.abs() / r;
                let rr = (x * x + y * y).sqrt();
                let core = (1.0 - rr / (r * 0.42)).max(0.0);
                let halo = (1.0 - rr / r).max(0.0);
                let arms = if a <= 1.0 || b <= 1.0 { 1.0 - a.min(b) * 0.35 } else { 0.0 };
                arms.max(core * core * 1.2).max(halo * halo * 0.35)
            })
        }
    }
}

/// Sprite sizes snap to steps about 10% apart, so particles that grow and shrink reuse a few
/// cached sprites instead of rasterizing a new one every frame.
fn bucket(px: f32) -> i32 {
    let s = px.max(2.0);
    if s < 10.0 {
        return s.round() as i32;
    }
    let step = (s / 10.0).ln() / 1.1f32.ln();
    (10.0 * 1.1f32.powf(step.round())).round() as i32
}

// ------------------------------------------------------------------ sticker labels

/// Bake a label: two-tone fill, ink line, white rim and a hard drop shadow. Leading digits are
/// drawn bigger ("5 Chain"). `size` is the em size in real pixels.
fn make_label(text: &str, size: f32, p: Pal, big_digits: bool) -> Canvas {
    let (top_c, bot_c, rim_c, ink_c) = pal(p);
    let digits = if big_digits { text.chars().take_while(|c| c.is_ascii_digit()).count() } else { 0 };
    let (head, tail) = text.split_at(digits);
    let big = size * 1.5;
    let wh = font::measure(head, big, Weight::Black);
    let wt = font::measure(tail, size, Weight::Black);
    let ro = (size * 0.12).max(2.5);
    let ri = (size * 0.07).max(1.6);
    let sh = (size * 0.07).max(2.0);
    let pad = (ro + sh + 3.0).ceil() as i32;
    let tall = if digits > 0 { big } else { size };
    let w = (wh + wt).ceil() as i32 + pad * 2;
    let h = (tall * 1.02).ceil() as i32 + pad * 2;
    let base = pad as f32 + tall * 0.80;
    let mut m = Canvas::new(w, h);
    font::draw(&mut m, pad as f32, base, head, big, Weight::Black, 0xFFFF_FFFF, 255);
    font::draw(&mut m, pad as f32 + wh, base, tail, size, Weight::Black, 0xFFFF_FFFF, 255);
    let cov: Vec<f32> = m.px.iter().map(|&v| (v >> 24) as f32 / 255.0).collect();
    // chamfer distance to the glyphs (1, sqrt 2), good to a few percent at these radii
    let mut d: Vec<f32> = cov.iter().map(|&c| if c >= 0.5 { 0.0 } else if c > 0.0 { 0.5 - c } else { 1e6 }).collect();
    let (wu, hu) = (w as usize, h as usize);
    let s2 = std::f32::consts::SQRT_2;
    for y in 0..hu {
        for x in 0..wu {
            let i = y * wu + x;
            let mut v = d[i];
            if x > 0 {
                v = v.min(d[i - 1] + 1.0);
            }
            if y > 0 {
                v = v.min(d[i - wu] + 1.0);
                if x > 0 {
                    v = v.min(d[i - wu - 1] + s2);
                }
                if x + 1 < wu {
                    v = v.min(d[i - wu + 1] + s2);
                }
            }
            d[i] = v;
        }
    }
    for y in (0..hu).rev() {
        for x in (0..wu).rev() {
            let i = y * wu + x;
            let mut v = d[i];
            if x + 1 < wu {
                v = v.min(d[i + 1] + 1.0);
            }
            if y + 1 < hu {
                v = v.min(d[i + wu] + 1.0);
                if x + 1 < wu {
                    v = v.min(d[i + wu + 1] + s2);
                }
                if x > 0 {
                    v = v.min(d[i + wu - 1] + s2);
                }
            }
            d[i] = v;
        }
    }
    let shx = (sh * 0.35).round() as i32;
    let shy = sh.round() as i32;
    let rb = rainbow(p);
    let cap_big = big * 0.72;
    let cap_small = size * 0.72;
    let split = pad as f32 + wh;
    let mut out = Canvas::new(w, h);
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            // layers, bottom up, composited premultiplied in f32
            let mut acc = [0.0f32; 4];
            let mut over = |c: u32, a: f32| {
                if a <= 0.0 {
                    return;
                }
                let ia = 1.0 - a;
                acc[0] = ((c >> 16) & 0xFF) as f32 * a + acc[0] * ia;
                acc[1] = ((c >> 8) & 0xFF) as f32 * a + acc[1] * ia;
                acc[2] = (c & 0xFF) as f32 * a + acc[2] * ia;
                acc[3] = a + acc[3] * ia;
            };
            let (sx, sy) = (x - shx, y - shy);
            if sx >= 0 && sy >= 0 && sx < w && sy < h {
                let ds = d[(sy * w + sx) as usize];
                over(ink_c, (ro - ds + 0.5).clamp(0.0, 1.0) * 0.55);
            }
            let dv = d[i];
            over(rim_c, (ro - dv + 0.5).clamp(0.0, 1.0));
            over(ink_c, (ri - dv + 0.5).clamp(0.0, 1.0));
            let a = cov[i];
            if a > 0.0 {
                let cap = if (x as f32) < split { cap_big } else { cap_small };
                let up = ((base - y as f32) / cap).clamp(0.0, 1.2);
                let (tc, bc) = if rb {
                    let hc = hue(x as f32 / w as f32 * 0.85 + 0.02);
                    (lighten(hc, 150), hc)
                } else {
                    (top_c, bot_c)
                };
                // cel two-tone: a lit upper band, the base color below, a soft seam, a bounce
                // line at the foot and a gloss streak near the top
                let mut c = if up > 0.56 {
                    tc
                } else if up > 0.48 {
                    mix(bc, tc, ((up - 0.48) / 0.08 * 255.0) as u32)
                } else if up < 0.10 {
                    darken(bc, 40)
                } else {
                    bc
                };
                if (0.80..0.90).contains(&up) {
                    c = lighten(c, 150);
                }
                over(c, a);
            }
            if acc[3] > 0.0 {
                out.px[i] = (((acc[3] * 255.0) as u32).min(255) << 24) | ((acc[0] as u32).min(255) << 16) | ((acc[1] as u32).min(255) << 8) | (acc[2] as u32).min(255);
            }
        }
    }
    out
}

/// "T-SPIN DOUBLE" -> "T-Spin Double", "BACK-TO-BACK" -> "Back-to-Back", "K.O.!" stays.
fn sticker_case(s: &str) -> String {
    if s.chars().filter(|c| c.is_alphabetic()).count() < 3 {
        return s.to_string();
    }
    let mut out = String::new();
    let mut start = true;
    let mut word = String::new();
    let flush = |w: &mut String, out: &mut String, first: bool| {
        let lw = w.to_lowercase();
        if !first && ["to", "of", "and", "the"].contains(&lw.as_str()) {
            out.push_str(&lw);
        } else {
            let mut c = lw.chars();
            if let Some(f) = c.next() {
                out.extend(f.to_uppercase());
                out.push_str(c.as_str());
            }
        }
        w.clear();
    };
    for ch in s.chars() {
        if ch.is_alphabetic() {
            word.push(ch);
        } else {
            if !word.is_empty() {
                flush(&mut word, &mut out, start);
                start = false;
            }
            out.push(ch);
        }
    }
    if !word.is_empty() {
        flush(&mut word, &mut out, start);
    }
    out
}

// ------------------------------------------------------------------ effect records

pub struct Particle {
    k: PK,
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,
    max: f32,
    size: f32,
    color: u32,
    rot: f32,
    vr: f32,
    grav: f32,
    drag: f32,
    delay: f32,
}

#[derive(Clone, Copy, PartialEq)]
enum RingStyle {
    /// solid toon band that thins as it grows, white inner edge
    Crisp,
    /// additive glow band
    Soft,
}
struct Ring {
    /// clipped to this seat's slot; 6 = the whole screen
    seat: usize,
    x: f32,
    y: f32,
    r0: f32,
    r1: f32,
    th: f32,
    t: f32,
    dur: f32,
    delay: f32,
    color: u32,
    style: RingStyle,
}
struct Rays {
    seat: usize,
    x: f32,
    y: f32,
    n: u32,
    r0: f32,
    len: f32,
    width: f32,
    t: f32,
    dur: f32,
    delay: f32,
    color: u32,
    seed: u32,
}
/// A light beam along a cleared Quad row.
struct Beam {
    seat: usize,
    row: i32,
    t: f32,
    dur: f32,
    color: u32,
}
/// T-spin: crescents whirling round the piece.
struct Spin {
    x: f32,
    y: f32,
    r: f32,
    t: f32,
    dur: f32,
    color: u32,
}
/// All clear: a band of light sweeping across the well.
struct Sweep {
    seat: usize,
    t: f32,
    dur: f32,
}

#[derive(Clone, Copy, PartialEq)]
enum LabelKind {
    /// headline: slams in from big, settles, lifts away
    Slam,
    /// chain/combo counter at the clear: pops, floats up
    Counter,
    /// a small tag over a headline (Back-to-Back)
    Chip,
}
#[derive(Clone, Copy, PartialEq)]
enum Persist {
    No,
    /// stays while the seat is out (K.O.)
    WhileDead,
    /// stays once the match is over (Win)
    WhileOver,
}
struct Label {
    seat: usize,
    spr: Arc<Canvas>,
    /// position relative to the well's top-left (layout units), so jolts carry it along
    dx: f32,
    dy: f32,
    t: f32,
    dur: f32,
    delay: f32,
    kind: LabelKind,
    persist: Persist,
    /// sparkle on landing
    flare: u32,
}

#[derive(Clone, Copy, PartialEq)]
enum ProjKind {
    Attack,
    /// the gold star a KO sends to the winner's nameplate
    Star,
}
#[derive(Clone, Copy)]
struct Proj {
    kind: ProjKind,
    from: (f32, f32),
    dst: usize,
    t: f32,
    dur: f32,
    delay: f32,
    color: u32,
    size: f32,
    /// where it is headed, refreshed every frame (the target board can jolt)
    end: (f32, f32),
    seed: u32,
}

/// Plain text callouts, for the Tournament theme.
pub struct Callout {
    seat: usize,
    text: String,
    t: f32,
    color: u32,
    scale: i32,
}

#[derive(Default, Clone, Copy)]
struct Spring {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
}

#[derive(Default)]
struct TickInfo {
    bursts: Vec<(f32, f32, u32)>,
    rows: Vec<i32>,
    lock: Option<(i32, i32)>,
    chain: u32,
    clear: Option<(u32, String)>,
    all_clear: bool,
    level_clear: bool,
    garbage: Option<u32>,
    attack: bool,
    ko: bool,
}

pub struct Fx {
    particles: Vec<Particle>,
    rings: Vec<Ring>,
    rays: Vec<Rays>,
    beams: Vec<Beam>,
    spins: Vec<Spin>,
    sweeps: Vec<Sweep>,
    labels: Vec<Label>,
    projs: Vec<Proj>,
    pub callouts: Vec<Callout>,
    pub shake: f32,
    /// garbage-hit flash per seat (also read for the "angry" portrait)
    pub flash: [f32; 6],
    /// white flash over each well
    glare: [f32; 6],
    glare_color: [u32; 6],
    /// garbage meter flash when an attack lands
    meter: [f32; 6],
    screen: f32,
    jolt: [Spring; 6],
    pub energy: f32,
    /// seconds of hit-stop left; the match clock holds while it runs
    pub hitstop: f32,
    /// the screen's pixel scale; labels are baked at it
    pub k: f32,
    /// big movement allowed (not Reduce motion)
    motion: bool,
    seed: std::cell::Cell<u32>,
    clock: f32,
    ended: bool,
    sprites: HashMap<(PK, u32, i32, u8), Canvas>,
    label_cache: HashMap<(String, i32, Pal, bool), Arc<Canvas>>,
}

fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t) * (1.0 - t) * (1.0 - t)
}
fn ease_out_back(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0) - 1.0;
    1.0 + t * t * (2.7 * t + 1.7)
}

/// Board corners and helpers.
fn well(g: &Geom) -> (f32, f32, f32, f32) {
    (g.fx as f32, g.fy as f32, (g.cols * g.cell) as f32, (g.rows * g.cell) as f32)
}
fn well_center(g: &Geom) -> (f32, f32) {
    let (x, y, w, h) = well(g);
    (x + w / 2.0, y + h / 2.0)
}
/// The top of the garbage stack in a seat's meter (where attacks land).
fn meter_point(m: &Match, i: usize, g: &Geom) -> (f32, f32) {
    let style = m.seats[i].cfg.profile.style();
    let (trav, tray) = m.incoming(i);
    let per_row = match style {
        Style::Quad => 1,
        Style::Chain | Style::Panel => 6,
        Style::Capsule => 2,
    };
    let rows = ((trav + tray + per_row - 1) / per_row).min(g.rows as u32) as i32;
    let mw = (g.cell / 4).clamp(4, 8);
    let mx = g.fx - 12 - mw / 2;
    let fh = g.rows * g.cell;
    (mx as f32, (g.fy + fh - rows * g.cell).min(g.fy + fh - g.cell / 2) as f32)
}
/// Where the KO stars sit on a seat's nameplate.
fn plate_point(m: &Match, i: usize, g: &Geom) -> (f32, f32) {
    let style = m.seats[i].cfg.profile.style();
    let fw = g.cols * g.cell;
    let plate_h = if fw >= 200 { 50 } else { 34 };
    let lift = if style == Style::Quad { g.cell * 2 + 4 } else { 12 };
    ((g.fx + fw / 2) as f32, (g.fy - lift - plate_h / 2) as f32)
}

impl Fx {
    pub fn new() -> Self {
        Fx {
            particles: Vec::new(),
            rings: Vec::new(),
            rays: Vec::new(),
            beams: Vec::new(),
            spins: Vec::new(),
            sweeps: Vec::new(),
            labels: Vec::new(),
            projs: Vec::new(),
            callouts: Vec::new(),
            shake: 0.0,
            flash: [0.0; 6],
            glare: [0.0; 6],
            glare_color: [0xFFFF_FFFF; 6],
            meter: [0.0; 6],
            screen: 0.0,
            jolt: [Spring::default(); 6],
            energy: 0.0,
            hitstop: 0.0,
            k: 1.0,
            motion: true,
            seed: std::cell::Cell::new(77),
            clock: 0.0,
            ended: false,
            sprites: HashMap::new(),
            label_cache: HashMap::new(),
        }
    }
    fn rnd(&self) -> f32 {
        let s = self.seed.get().wrapping_mul(1664525).wrapping_add(1013904223);
        self.seed.set(s);
        (s >> 8) as f32 / 16777216.0
    }
    fn rr(&self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.rnd()
    }

    /// The seat has a K.O. or Win label of its own (render skips its plain text then).
    pub fn has_banner(&self, seat: usize) -> bool {
        self.labels.iter().any(|l| l.seat == seat && l.persist != Persist::No)
    }
    /// Per-seat board offset (layout units) from jolts.
    pub fn jolt(&self, seat: usize) -> (i32, i32) {
        (self.jolt[seat].x.round() as i32, self.jolt[seat].y.round() as i32)
    }
    /// Screen shake offset (layout units).
    pub fn shake_offset(&self, t: f32) -> (i32, i32) {
        if self.shake < 0.3 {
            return (0, 0);
        }
        ((self.shake * (t * 53.0).sin()) as i32, (self.shake * 0.6 * (t * 41.0 + 1.3).sin()) as i32)
    }

    fn label(&mut self, text: &str, size_px: f32, p: Pal, big_digits: bool) -> Arc<Canvas> {
        let size = ((size_px / 2.0).round() * 2.0).max(8.0) as i32;
        let key = (text.to_string(), size, p, big_digits);
        if let Some(s) = self.label_cache.get(&key) {
            return s.clone();
        }
        if self.label_cache.len() > 96 {
            self.label_cache.clear();
        }
        let s = Arc::new(make_label(text, size as f32, p, big_digits));
        self.label_cache.insert(key, s.clone());
        s
    }
    /// A label `size` layout units tall, shrunk until it fits `max_w` layout units.
    #[allow(clippy::too_many_arguments)]
    fn add_label(&mut self, k: f32, seat: usize, text: &str, size: f32, max_w: f32, p: Pal, big: bool, kind: LabelKind, dx: f32, dy: f32, dur: f32, persist: Persist) {
        let text = sticker_case(text);
        let mut size = size;
        let digits = if big { text.chars().take_while(|c| c.is_ascii_digit()).count() } else { 0 };
        let width = |s: f32| font::measure(&text[..digits], s * 1.5, Weight::Black) + font::measure(&text[digits..], s, Weight::Black) + s * 0.4;
        while size > 10.0 && width(size) > max_w {
            size *= 0.92;
        }
        let spr = self.label(&text, size * k, p, big);
        if kind == LabelKind::Slam {
            self.labels.retain(|l| !(l.seat == seat && l.kind != LabelKind::Counter && l.persist == Persist::No));
        }
        let flare = if kind == LabelKind::Counter { 0 } else { tier_color(p, 0.3) };
        self.labels.push(Label { seat, spr, dx, dy, t: 0.0, dur, delay: 0.0, kind, persist, flare });
    }

    #[allow(clippy::too_many_arguments)]
    fn part(&mut self, cap: usize, k: PK, x: f32, y: f32, vx: f32, vy: f32, life: f32, size: f32, color: u32, grav: f32, drag: f32, delay: f32) {
        if self.particles.len() >= cap {
            return;
        }
        let rot = self.rnd() * TAU;
        let vr = self.rr(-14.0, 14.0);
        self.particles.push(Particle { k, x, y, vx, vy, life, max: life, size, color, rot, vr, grav, drag, delay });
    }
    #[allow(clippy::too_many_arguments)]
    fn ring(&mut self, seat: usize, x: f32, y: f32, r0: f32, r1: f32, th: f32, dur: f32, delay: f32, color: u32, style: RingStyle) {
        // Reduce motion keeps the small pop flashes, not the big shockwaves
        if self.rings.len() < 64 && (self.motion || r1 - r0 < 40.0) {
            self.rings.push(Ring { seat, x, y, r0, r1, th, t: 0.0, dur, delay, color, style });
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn rays(&mut self, seat: usize, x: f32, y: f32, n: u32, r0: f32, len: f32, width: f32, dur: f32, delay: f32, color: u32) {
        if self.rays.len() < 16 {
            let seed = (self.rnd() * 1e6) as u32;
            self.rays.push(Rays { seat, x, y, n, r0, len, width, t: 0.0, dur, delay, color, seed });
        }
    }

    /// One cell popping: shards or droplets in its color plus a sparkle.
    #[allow(clippy::too_many_arguments)]
    fn pop(&mut self, q: &Q, seat: usize, style: Style, x: f32, y: f32, cell: f32, color: u32, n: usize, delay: f32, tier: u32) {
        let cap = q.cap;
        let pw = cell * (5.0 + tier as f32 * 1.4);
        match style {
            Style::Quad | Style::Panel => {
                for _ in 0..n {
                    let a = self.rr(-PI, 0.0) + self.rr(-0.6, 0.6);
                    let sp = self.rr(0.5, 1.0) * pw;
                    let (vx, vy) = (a.cos() * sp, a.sin() * sp - cell * 3.0);
                    let s = cell * self.rr(0.38, 0.62);
                    let life = self.rr(0.45, 0.8);
                    self.part(cap, PK::Shard, x, y, vx, vy, life, s, color, cell * 40.0, 2.2, delay);
                }
            }
            Style::Chain | Style::Capsule => {
                for _ in 0..n {
                    let a = self.rr(0.0, TAU);
                    let sp = self.rr(0.4, 1.0) * pw;
                    let (vx, vy) = (a.cos() * sp, a.sin() * sp - cell * 3.5);
                    let s = cell * self.rr(0.26, 0.46);
                    let life = self.rr(0.4, 0.7);
                    self.part(cap, PK::Drop, x, y, vx, vy, life, s, color, cell * 45.0, 2.6, delay);
                }
                if q.cap > 0 {
                    // a quick toon pop: a fat disc that opens into a ring and is gone
                    self.ring(seat, x, y, cell * 0.15, cell * 0.75, cell * 0.55, 0.16, delay, lighten(color, 120), RingStyle::Crisp);
                }
            }
        }
        let s = cell * self.rr(1.1, 1.6);
        self.part(cap, PK::Spark, x, y, 0.0, 0.0, 0.32, s, lighten(color, 60), 0.0, 0.0, delay);
    }

    /// Feed one tick of sim events.
    pub fn on_events(&mut self, m: &Match, geoms: &[Geom; 6], st: &Settings) {
        let q = quality(st);
        self.motion = q.motion;
        let n_active = m.active_count();
        let mut info: [TickInfo; 6] = Default::default();
        for (seat, e) in &m.events {
            let s = *seat as usize;
            let g = geoms[s];
            if g.cell == 0 {
                continue;
            }
            let style = m.seats[s].cfg.profile.style();
            let ti = &mut info[s];
            match e {
                Ev::Burst { x, y, color } => {
                    let (px, py) = crate::render::cell_pos(style, &g, *x, *y, m.seats[s].board.as_ref());
                    let c = theme_tint(st.theme, crate::render::color_for(style, *color));
                    ti.bursts.push((px + g.cell as f32 / 2.0, py + g.cell as f32 / 2.0, c));
                    if style == Style::Quad && !ti.rows.contains(y) {
                        ti.rows.push(*y);
                    }
                }
                Ev::Lock { x, y } => ti.lock = Some((*x, *y)),
                Ev::ChainStep { n } => ti.chain = *n,
                Ev::Clear { power, text } => ti.clear = Some((*power, text.clone())),
                Ev::AllClear => ti.all_clear = true,
                Ev::LevelClear => ti.level_clear = true,
                Ev::GarbageLand { n } => ti.garbage = Some(*n),
                Ev::AttackSent { .. } => ti.attack = true,
                Ev::Ko { .. } => ti.ko = true,
                _ => {}
            }
        }
        for s in 0..6 {
            let ti = std::mem::take(&mut info[s]);
            let g = geoms[s];
            if g.cell == 0 || m.seats[s].board.is_none() {
                continue;
            }
            self.seat_tick(m, s, &g, geoms, ti, &q, st, n_active);
        }
        // the match just ended: the winners celebrate
        if m.outcome != Outcome::Running && !self.ended {
            self.ended = true;
            if q.on && !m.header.mode.is_solo() {
                for s in 0..6 {
                    let g = geoms[s];
                    if g.cell == 0 || !m.is_winner(s) {
                        continue;
                    }
                    self.win(s, &g, &q);
                }
            }
        } else if m.outcome == Outcome::Running {
            self.ended = false;
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn seat_tick(&mut self, m: &Match, s: usize, g: &Geom, geoms: &[Geom; 6], ti: TickInfo, q: &Q, st: &Settings, n_active: usize) {
        let style = m.seats[s].cfg.profile.style();
        let cell = g.cell as f32;
        let (wx, wy, ww, wh) = well(g);
        let human = m.seats[s].cfg.kind == SeatKind::Human;
        let crowd = n_active > 2 && !human;
        let l = look(st.theme);
        // where this tick's clear happened
        let centroid = if ti.bursts.is_empty() {
            None
        } else {
            let n = ti.bursts.len() as f32;
            Some((ti.bursts.iter().map(|b| b.0).sum::<f32>() / n, ti.bursts.iter().map(|b| b.1).sum::<f32>() / n))
        };
        let (cx, cy) = centroid.unwrap_or(well_center(g));

        if !q.on {
            // Tournament: the plain callouts the original showed, nothing else
            if let Some((power, text)) = &ti.clear {
                let scale = ((g.cell / 10).max(1) + if *power >= 6 { 1 } else { 0 }).min(5);
                self.callout(s, text.clone(), if *power >= 4 { l.accent } else { l.text }, scale);
            }
            if ti.all_clear {
                self.callout(s, "ALL CLEAR!".into(), l.good, (g.cell / 9).max(2));
            }
            if ti.level_clear {
                self.callout(s, "BOTTLE CLEAR!".into(), l.good, (g.cell / 9).max(2));
            }
            if ti.garbage.is_some() {
                self.flash[s] = 1.0;
            }
            return;
        }

        // ---- the move and its tier
        let mut tier = 0u32;
        let mut pal_main = Pal::Plain;
        let mut headline: Option<String> = None;
        let mut chip: Option<&str> = None;
        let mut counters: Vec<(String, Pal)> = Vec::new();
        let mut spin = false;
        if let Some((power, text)) = &ti.clear {
            self.energy = (self.energy + *power as f32 * 0.06).min(1.0);
            match style {
                Style::Quad => {
                    let b2b = text.starts_with("B2B");
                    let mini = text.contains("MINI");
                    let ts = text.contains("T-SPIN");
                    let lines = ["SINGLE", "DOUBLE", "TRIPLE", "QUAD"].iter().position(|w| text.contains(w)).map(|p| p as u32 + 1).unwrap_or(0);
                    let combo: u32 = text.split("  ").nth(1).and_then(|c| c.split_whitespace().next()).and_then(|n| n.parse().ok()).unwrap_or(0);
                    tier = match (ts, mini, lines) {
                        (true, true, _) => 1,
                        (true, false, 1) => 1,
                        (true, false, 2) => 2,
                        (true, false, _) if lines >= 3 => 3,
                        (true, false, _) => 1,
                        (false, _, 4) => 2,
                        (false, _, 2) | (false, _, 3) => 1,
                        _ => 0,
                    };
                    if b2b && tier >= 2 {
                        tier += 1;
                    }
                    if combo >= 7 {
                        tier = tier.max(3);
                    } else if combo >= 4 {
                        tier = tier.max(2);
                    }
                    spin = ts;
                    pal_main = if ts {
                        Pal::Spin
                    } else if lines == 4 {
                        Pal::Quad
                    } else {
                        Pal::Tier(lines.min(2) as u8)
                    };
                    let head = text.split("  ").next().unwrap_or("").trim_start_matches("B2B ").trim();
                    if !head.is_empty() {
                        headline = Some(head.to_string());
                    }
                    if b2b {
                        chip = Some("BACK-TO-BACK");
                    }
                    if combo >= 1 {
                        counters.push((format!("{} COMBO", combo), chain_pal(combo + 1)));
                    }
                }
                Style::Chain | Style::Capsule | Style::Panel => {
                    let n = ti.chain.max(1);
                    let combo: u32 = text.split("  ").find(|p| p.ends_with("COMBO")).and_then(|c| c.split_whitespace().next()).and_then(|n| n.parse().ok()).unwrap_or(0);
                    tier = match n {
                        0 | 1 => 0,
                        2 => 1,
                        3 => 2,
                        4 | 5 => 3,
                        _ => 4,
                    };
                    if combo >= 9 {
                        tier = tier.max(3);
                    } else if combo >= 6 {
                        tier = tier.max(2);
                    } else if combo >= 4 {
                        tier = tier.max(1);
                    }
                    pal_main = chain_pal(n);
                    if ti.chain >= 2 {
                        counters.push((format!("{} CHAIN", n), chain_pal(n)));
                    }
                    if combo >= 4 {
                        counters.push((format!("{} COMBO", combo), Pal::Tier((combo as u8 / 2).clamp(2, 6))));
                    }
                    if style == Style::Capsule && ti.chain < 2 && text.contains("LINES") {
                        counters.push((text.clone(), Pal::Tier(2)));
                        tier = tier.max(1);
                    }
                }
            }
        }
        if ti.all_clear || ti.level_clear {
            tier = 4;
        }
        let accent = tier_color(pal_main, self.clock);

        // ---- pops
        if !ti.bursts.is_empty() {
            let per = if crowd { 1 } else if ti.bursts.len() > 20 { 2 } else { 3 + tier as usize / 2 };
            // when the board shows the pop: Panel pops one tile after another after its flash,
            // Chain blobs pulse first (the sim's timing, mirrored here)
            for (j, &(x, y, c)) in ti.bursts.iter().enumerate() {
                let delay = match style {
                    Style::Panel => (44.0 + 9.0 * j as f32) / 60.0,
                    Style::Chain => 16.0 / 60.0,
                    _ => 0.0,
                };
                if crowd && j % 2 == 1 {
                    // background seats in a crowded match: half the pops get the full treatment
                    self.part(q.cap, if matches!(style, Style::Chain | Style::Capsule) { PK::Drop } else { PK::Shard }, x, y, 0.0, -cell * 4.0, 0.4, cell * 0.45, c, cell * 40.0, 2.0, delay);
                    continue;
                }
                self.pop(q, s, style, x, y, cell, c, per, delay, tier);
            }
        }
        if style == Style::Quad && !ti.rows.is_empty() {
            let col = if spin { 0xFFE0_8CFF } else if ti.rows.len() >= 4 { 0xFF8C_F4FF } else { 0xFFFF_FFFF };
            for &r in &ti.rows {
                self.beams.push(Beam { seat: s, row: r, t: 0.0, dur: 0.34, color: col });
            }
        }
        if spin {
            let (lx, ly) = ti.lock.map(|(x, y)| crate::render::cell_pos(style, g, x, y, None)).map(|(x, y)| (x + cell / 2.0, y + cell / 2.0)).unwrap_or((cx, cy));
            self.spins.push(Spin { x: lx, y: ly, r: cell * 1.5, t: 0.0, dur: 0.55, color: 0xFFC0_6CFF });
        }

        // ---- shock: rings, rays, glare, jolt, shake, hit-stop, scaled by tier
        if ti.clear.is_some() && tier >= 1 {
            let reach = cell * (1.6 + 0.9 * tier as f32);
            self.ring(s, cx, cy, cell * 0.4, reach, cell * (0.35 + 0.08 * tier as f32), 0.38, 0.0, accent, RingStyle::Crisp);
            if tier >= 2 {
                if !crowd {
                    self.ring(s, cx, cy, cell * 0.6, reach * 1.4, cell * 0.7, 0.45, 0.04, accent, RingStyle::Soft);
                }
                if !crowd {
                    self.glare[s] = self.glare[s].max(0.2 + 0.1 * tier as f32);
                    self.glare_color[s] = accent;
                }
            }
            if tier >= 2 && q.rays && !crowd {
                let n = [0, 0, 8, 12, 16][tier.min(4) as usize];
                self.rays(s, cx, cy, n, cell * 0.9, cell * (1.6 + 0.8 * tier as f32), cell * (0.26 + 0.04 * tier as f32), 0.30, 0.0, accent);
            }
            if tier >= 3 && !crowd {
                let (bx, by) = well_center(g);
                self.ring(s, bx, by, cell * 2.0, ww.max(wh) * 0.7, cell * 0.6, 0.5, 0.08, lighten(accent, 60), RingStyle::Crisp);
                if q.flashes {
                    self.shake = self.shake.max(2.0 + 1.5 * tier as f32);
                }
            }
            if tier >= 2 && q.motion {
                self.jolt[s].vy += 60.0 + 40.0 * tier as f32;
            }
            if tier >= 4 && q.motion {
                self.hitstop = self.hitstop.max(0.07);
            }
        }

        // ---- words
        let fw = ww;
        let base = cell.clamp(14.0, 34.0);
        if let Some(h) = &headline {
            let small = tier == 0;
            let size = if small { base * 1.0 } else { base * (1.45 + 0.18 * tier as f32) };
            // background seats in a crowded match pop their words in instead of slamming them
            let kind = if crowd { LabelKind::Chip } else { LabelKind::Slam };
            self.add_label(self.k, s, h, size, fw * if small { 0.8 } else { 1.12 }, pal_main, false, kind, fw / 2.0, wh * 0.30, if small { 0.9 } else { 1.35 }, Persist::No);
            if let (Some(c), false) = (chip, crowd) {
                let cs = (base * 0.95).max(16.0);
                self.add_label(self.k, s, c, cs, fw * 0.95, Pal::Gold, false, LabelKind::Chip, fw / 2.0, wh * 0.30 - size * 0.62 - cs * 0.55, 1.35, Persist::No);
            }
        }
        for (j, (txt, p)) in counters.iter().enumerate() {
            let n: f32 = txt.split_whitespace().next().and_then(|v| v.parse().ok()).unwrap_or(1.0);
            let size = base * (0.85 + 0.09 * n.min(10.0));
            let dy = if headline.is_some() { wh * 0.30 + base * 1.7 } else { (cy - wy - cell * 0.6 - j as f32 * size * 1.5).clamp(size, wh - size) };
            let dx = if headline.is_some() { fw / 2.0 } else { (cx - wx).clamp(fw * 0.3, fw * 0.7) };
            let kind = if headline.is_some() { LabelKind::Chip } else { LabelKind::Counter };
            self.labels.retain(|l| !(l.seat == s && l.kind == LabelKind::Counter && l.t > 0.25));
            self.add_label(self.k, s, txt, size, fw * 1.05, *p, true, kind, dx, dy, 1.15, Persist::No);
        }

        // ---- all clear / bottle clear
        if ti.all_clear || ti.level_clear {
            let (bx, by) = well_center(g);
            let (text, p) = if ti.all_clear { ("ALL CLEAR!", Pal::Rainbow) } else { ("BOTTLE CLEAR!", Pal::Good) };
            self.add_label(self.k, s, text, base * 2.0, fw * 1.15, p, false, LabelKind::Slam, fw / 2.0, wh * 0.42, 1.8, Persist::No);
            self.sweeps.push(Sweep { seat: s, t: 0.0, dur: 0.55 });
            self.ring(s, bx, by, cell, ww.max(wh) * 0.75, cell * 0.7, 0.6, 0.0, 0xFFFF_E066, RingStyle::Crisp);
            self.ring(s, bx, by, cell, ww.max(wh), cell * 2.0, 0.8, 0.1, 0xFFFF_9AD8, RingStyle::Soft);
            for j in 0..28 {
                let x = wx + self.rr(0.1, 0.9) * ww;
                let y = wy + self.rr(0.2, 0.95) * wh;
                let c = hue(j as f32 / 28.0);
                let vx = self.rr(-1.0, 1.0) * cell * 3.0;
                let vy = -self.rr(4.0, 10.0) * cell;
                let size = cell * self.rr(0.5, 0.8);
                let d = self.rr(0.0, 0.35);
                self.part(q.cap, PK::Star, x, y, vx, vy, self.rr(0.9, 1.4), size, c, cell * 18.0, 1.4, d);
                self.part(q.cap, PK::Spark, x, y, 0.0, 0.0, 0.35, cell * 1.6, c, 0.0, 0.0, d);
            }
            self.glare[s] = 1.0;
            self.glare_color[s] = 0xFFFF_F0C0;
            if q.motion {
                self.hitstop = self.hitstop.max(0.1);
            }
            if q.flashes {
                self.screen = self.screen.max(0.6);
                self.shake = self.shake.max(6.0);
            }
        }

        // ---- garbage lands
        if let Some(n) = ti.garbage {
            self.flash[s] = 1.0;
            let up = style == Style::Quad;
            if q.motion {
                let kick = 70.0 + (n.min(12) as f32) * 14.0;
                self.jolt[s].vy += if up { -kick } else { kick };
            }
            let y = if up { wy + wh } else { wy };
            let puffs = if crowd { 3 } else { 6 };
            for _ in 0..puffs {
                let x = wx + self.rr(0.05, 0.95) * ww;
                let vx = self.rr(-1.0, 1.0) * cell * 2.0;
                let vy = if up { -cell * self.rr(1.0, 2.0) } else { -cell * self.rr(0.3, 1.2) };
                let size = cell * self.rr(0.8, 1.3);
                self.part(q.cap, PK::Puff, x, y, vx, vy, self.rr(0.45, 0.7), size, 0xFFE4_E8F4, -cell * 2.0, 2.0, 0.0);
            }
        }

        // ---- attacks fly to their targets
        if ti.attack {
            let from = centroid.unwrap_or((wx + ww / 2.0, wy + wh * 0.35));
            for tr in m.travel.iter().filter(|t| t.src as usize == s && t.tick == m.tick) {
                let d = tr.dst as usize;
                if geoms[d].cell == 0 {
                    continue;
                }
                let u = tr.native.units() as f32;
                let size = (9.0 + 2.6 * u.sqrt()).min(24.0) * if crowd { 0.7 } else { 1.0 };
                let seed = (self.rnd() * 1e6) as u32;
                let end = meter_point(m, d, &geoms[d]);
                self.projs.push(Proj {
                    kind: ProjKind::Attack,
                    from,
                    dst: d,
                    t: 0.0,
                    dur: purtris_sim::CROSS_DELAY as f32 / 60.0,
                    delay: 0.0,
                    color: style_accent(style),
                    size,
                    end,
                    seed,
                });
                if !crowd {
                    self.ring(s, from.0, from.1, size * 0.5, size * 3.2, size * 0.45, 0.25, 0.0, lighten(style_accent(style), 80), RingStyle::Crisp);
                }
            }
        }

        // ---- K.O.
        if ti.ko {
            self.ko(m, s, g, geoms, q, st);
        }
    }

    fn ko(&mut self, m: &Match, s: usize, g: &Geom, geoms: &[Geom; 6], q: &Q, st: &Settings) {
        let cell = g.cell as f32;
        let (wx, wy, ww, wh) = well(g);
        let (bx, by) = well_center(g);
        let big = ww.max(wh);
        self.ring(6, bx, by, cell, big * 0.7, cell * 1.1, 0.5, 0.0, 0xFFFF_5A5A, RingStyle::Crisp);
        self.ring(6, bx, by, cell, big * 0.9, cell * 1.4, 0.6, 0.06, 0xFFFF_4A4A, RingStyle::Soft);
        self.ring(6, bx, by, cell * 2.0, big * 1.1, cell * 0.6, 0.7, 0.14, 0xFFFF_D54A, RingStyle::Crisp);
        if q.rays {
            self.rays(6, bx, by, 14, cell * 2.5, big * 0.55, cell * 0.55, 0.45, 0.0, 0xFFFF_9A5A);
        }
        // the stack bursts apart into shards, and smoke rolls up
        let style = m.seats[s].cfg.profile.style();
        let cols = [RED, ORANGE, YELLOW, GREEN, CYAN, BLUE, PURPLE, PINK];
        for j in 0..56 {
            let x = wx + self.rr(0.05, 0.95) * ww;
            let y = wy + self.rr(0.3, 1.0) * wh;
            let a = (y - by).atan2(x - bx) + self.rr(-0.4, 0.4);
            let sp = cell * self.rr(6.0, 16.0);
            let c = theme_tint(st.theme, cols[j % cols.len()]);
            let kind = if matches!(style, Style::Chain | Style::Capsule) { PK::Drop } else { PK::Shard };
            self.part(q.cap, kind, x, y, a.cos() * sp, a.sin() * sp - cell * 6.0, self.rr(0.7, 1.2), cell * self.rr(0.45, 0.8), c, cell * 30.0, 1.6, 0.0);
        }
        for _ in 0..8 {
            let x = wx + self.rr(0.1, 0.9) * ww;
            let y = wy + wh * self.rr(0.75, 1.0);
            let sz = cell * self.rr(1.2, 2.0);
            self.part(q.cap, PK::Puff, x, y, self.rr(-1.0, 1.0) * cell * 2.0, -cell * self.rr(5.0, 9.0), self.rr(0.6, 0.9), sz, 0xFFEC_E4F4, -cell * 4.0, 2.0, 0.05);
        }
        let base = cell.clamp(14.0, 34.0);
        self.add_label(self.k, s, "K.O.!", base * 2.3, ww + cell * 2.0, Pal::Ko, false, LabelKind::Slam, ww / 2.0, wh * 0.42, 1.2, Persist::WhileDead);
        self.glare[s] = 1.0;
        self.glare_color[s] = 0xFFFF_7070;
        if q.motion {
            self.hitstop = self.hitstop.max(0.13);
            self.jolt[s].vy += 260.0;
        }
        if q.flashes {
            self.screen = self.screen.max(1.0);
            self.shake = self.shake.max(12.0);
        }
        // a gold star flies to whoever scored the KO
        if let Some(killer) = m.seats[s].ko_by {
            let kk = killer as usize;
            if geoms[kk].cell > 0 {
                let end = plate_point(m, kk, &geoms[kk]);
                let seed = (self.rnd() * 1e6) as u32;
                self.projs.push(Proj { kind: ProjKind::Star, from: (bx, by), dst: kk, t: 0.0, dur: 0.7, delay: 0.25, color: 0xFFFF_D54A, size: 16.0, end, seed });
            }
        }
    }

    fn win(&mut self, s: usize, g: &Geom, q: &Q) {
        let cell = g.cell as f32;
        let (wx, wy, ww, wh) = well(g);
        let base = cell.clamp(14.0, 34.0);
        let d0 = 0.7;
        self.add_label(self.k, s, "WIN!", base * 2.3, ww + cell * 2.0, Pal::Gold, false, LabelKind::Slam, ww / 2.0, wh * 0.40, 1.2, Persist::WhileOver);
        if let Some(l) = self.labels.last_mut() {
            l.delay = d0;
        }
        let (bx, by) = well_center(g);
        self.ring(s, bx, wy + wh * 0.4, cell, ww.max(wh) * 0.7, cell * 0.8, 0.6, d0 + 0.08, 0xFFFF_E066, RingStyle::Crisp);
        if q.rays {
            self.rays(s, bx, wy + wh * 0.4, 16, cell * 2.5, ww.max(wh) * 0.5, cell * 0.5, 0.9, d0 + 0.08, 0xFFFF_D070);
        }
        let _ = by;
        for j in 0..40 {
            let x = wx + self.rr(0.0, 1.0) * ww;
            let y = wy - cell * self.rr(0.0, 3.0);
            let c = hue(j as f32 / 40.0 + 0.1);
            let d = 0.75 + self.rr(0.0, 0.8);
            self.part(q.cap, PK::Star, x, y, self.rr(-1.0, 1.0) * cell * 2.0, cell * self.rr(1.0, 4.0), self.rr(1.4, 2.2), cell * self.rr(0.45, 0.75), c, cell * 6.0, 0.8, d);
        }
    }

    pub fn callout(&mut self, seat: usize, text: String, color: u32, scale: i32) {
        if text.is_empty() {
            return;
        }
        self.callouts.retain(|c| c.seat != seat);
        self.callouts.push(Callout { seat, text, t: 0.0, color, scale });
    }

    pub fn update(&mut self, dt: f32) {
        // flashes and shake fade in real time; during hit-stop everything that moves
        // crawls (the impact frame), then lets go
        let real = dt;
        let f = (-real * 9.6).exp();
        self.shake *= f;
        for v in self.flash.iter_mut() {
            *v *= (-real * 6.3).exp();
        }
        for v in self.glare.iter_mut() {
            *v *= (-real * 12.0).exp();
        }
        for v in self.meter.iter_mut() {
            *v *= (-real * 5.0).exp();
        }
        self.screen *= (-real * 16.0).exp();
        let dt = if self.hitstop > 0.0 {
            self.hitstop -= real;
            dt * 0.25
        } else {
            dt
        };
        self.clock += dt;
        for p in self.particles.iter_mut() {
            if p.delay > 0.0 {
                p.delay -= dt;
                continue;
            }
            let damp = (-p.drag * dt).exp();
            p.vx *= damp;
            p.vy = p.vy * damp + p.grav * dt;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.rot += p.vr * dt;
            p.life -= dt;
        }
        self.particles.retain(|p| p.life > 0.0);
        for r in self.rings.iter_mut() {
            if r.delay > 0.0 {
                r.delay -= dt;
            } else {
                r.t += dt;
            }
        }
        self.rings.retain(|r| r.t < r.dur);
        for r in self.rays.iter_mut() {
            if r.delay > 0.0 {
                r.delay -= dt;
            } else {
                r.t += dt;
            }
        }
        self.rays.retain(|r| r.t < r.dur);
        for b in self.beams.iter_mut() {
            b.t += dt;
        }
        self.beams.retain(|b| b.t < b.dur);
        for b in self.spins.iter_mut() {
            b.t += dt;
        }
        self.spins.retain(|b| b.t < b.dur);
        for b in self.sweeps.iter_mut() {
            b.t += dt;
        }
        self.sweeps.retain(|b| b.t < b.dur);
        // words slam in at full speed even in hit-stop: that is the impact frame
        for l in self.labels.iter_mut() {
            if l.delay > 0.0 {
                l.delay -= real;
            } else {
                l.t += real;
            }
        }
        self.labels.retain(|l| l.persist != Persist::No || l.t < l.dur);
        // projectiles: arrival bursts at the target
        let mut arrived: Vec<(ProjKind, usize, (f32, f32), u32, f32)> = Vec::new();
        for p in self.projs.iter_mut() {
            if p.delay > 0.0 {
                p.delay -= dt;
                continue;
            }
            p.t += dt;
            if p.t >= p.dur {
                arrived.push((p.kind, p.dst, p.end, p.color, p.size));
            }
        }
        self.projs.retain(|p| p.t < p.dur);
        for (kind, dst, (x, y), color, size) in arrived {
            match kind {
                ProjKind::Attack => {
                    self.meter[dst] = 1.0;
                    self.ring(6, x, y, size * 0.4, size * 3.4, size * 0.5, 0.3, 0.0, lighten(color, 90), RingStyle::Crisp);
                    for _ in 0..6 {
                        let a = self.rr(-1.2, 1.2);
                        let sp = self.rr(150.0, 320.0);
                        self.part(700, PK::Spark, x, y, a.cos() * sp, a.sin() * sp, 0.25, size * 1.1, lighten(color, 60), 0.0, 5.0, 0.0);
                    }
                    self.jolt[dst].vx += 40.0 + size * 4.0;
                }
                ProjKind::Star => {
                    self.ring(6, x, y, 6.0, 46.0, 7.0, 0.35, 0.0, 0xFFFF_E066, RingStyle::Crisp);
                    for j in 0..10 {
                        let a = j as f32 / 10.0 * TAU;
                        self.part(700, PK::Star, x, y, a.cos() * 160.0, a.sin() * 160.0 - 60.0, 0.6, 12.0, hue(j as f32 / 10.0), 300.0, 2.5, 0.0);
                    }
                    self.part(700, PK::Spark, x, y, 0.0, 0.0, 0.4, 60.0, 0xFFFF_E066, 0.0, 0.0, 0.0);
                }
            }
        }
        for c in self.callouts.iter_mut() {
            c.t += dt;
        }
        self.callouts.retain(|c| c.t < 1.4);
        // springs: board jolts settle fast with a little overshoot
        for j in self.jolt.iter_mut() {
            let (kk, damp) = (700.0, 18.0);
            j.vx += (-kk * j.x - damp * j.vx) * dt;
            j.vy += (-kk * j.y - damp * j.vy) * dt;
            j.x += j.vx * dt;
            j.y += j.vy * dt;
            j.x = j.x.clamp(-18.0, 18.0);
            j.y = j.y.clamp(-18.0, 18.0);
        }
        self.energy *= (-dt * 0.3).exp();
    }

    /// Draw everything over the boards. `m` gives the live target points and persistence.
    pub fn draw(&mut self, cv: &mut Canvas, m: &Match, geoms: &[Geom; 6], st: &Settings) {
        let _p = crate::prof::scope("fx");
        let q = quality(st);
        if !q.on {
            self.draw_callouts(cv, geoms);
            return;
        }
        let k = cv.k;
        // drop K.O./Win labels whose condition ended (rewind, a new match)
        self.labels.retain(|l| match l.persist {
            Persist::No => true,
            Persist::WhileDead => m.seats[l.seat].board.is_some() && !m.seats[l.seat].alive,
            Persist::WhileOver => m.outcome != Outcome::Running,
        });
        let _pg = crate::prof::scope("fx: glare");
        // glare over each well, and the meter flash
        for s in 0..6 {
            let g = geoms[s];
            if g.cell == 0 {
                continue;
            }
            let (wx, wy, ww, wh) = well(&g);
            if self.glare[s] > 0.05 {
                let a = (self.glare[s] * if q.flashes { 90.0 } else { 45.0 }) as u32;
                fd::rect(cv, wx as i32, wy as i32, ww as i32, wh as i32, self.glare_color[s], a, Mode::Add);
            }
            if self.meter[s] > 0.03 {
                let (mx, my) = meter_point(m, s, &g);
                let mw = (g.cell / 4).clamp(4, 8) as f32;
                let a = (self.meter[s] * 200.0) as u32;
                let spr = self.sprite(PK::Dot, 0, (mw * 8.0 * k) as i32, 0);
                fd::light_px(cv, spr, mx * k, my * k, 0xFFFF_C070, a, false);
                fd::rect(cv, (mx - mw / 2.0) as i32, (my) as i32, mw as i32, (wy + wh - my) as i32, 0xFFFF_FFFF, (self.meter[s] * 160.0) as u32, Mode::Add);
            }
        }
        drop(_pg);
        // all-clear sweeps: a slanted band of light crossing the well
        for sw in &self.sweeps {
            let g = geoms[sw.seat];
            if g.cell == 0 {
                continue;
            }
            let (wx, wy, ww, wh) = well(&g);
            let e = ease_out(sw.t / sw.dur);
            let x = wx - ww * 0.6 + e * ww * 2.0;
            let band = ww * 0.28;
            let a = ((1.0 - e) * 230.0) as u32;
            cv.set_clip(wx as i32, wy as i32, ww as i32, wh as i32);
            fd::poly(cv, &[(x, wy), (x + band, wy), (x + band - wh * 0.35, wy + wh), (x - wh * 0.35, wy + wh)], 0xFFFF_FFFF, a, Mode::Add);
            fd::poly(cv, &[(x + band * 1.25, wy), (x + band * 1.45, wy), (x + band * 1.45 - wh * 0.35, wy + wh), (x + band * 1.25 - wh * 0.35, wy + wh)], 0xFFFF_F0B0, a, Mode::Add);
            cv.reset_clip();
        }
        // Quad row beams: a flare across the row that reaches past the walls, then pinches shut
        for b in &self.beams {
            let g = geoms[b.seat];
            if g.cell == 0 {
                continue;
            }
            let (wx, _, ww, _) = well(&g);
            let cell = g.cell as f32;
            let y = g.fy as f32 + (b.row - purtris_sim::quad::VIS0) as f32 * cell + cell / 2.0;
            let e = b.t / b.dur;
            let h = cell * if e < 0.25 { 0.5 + 1.6 * e / 0.25 * 0.5 } else { 0.9 * (1.0 - (e - 0.25) / 0.75).powf(1.5) };
            let reach = cell * (1.0 + 4.0 * ease_out(e));
            let a = ((1.0 - e * 0.6) * 255.0) as u32;
            fd::poly(cv, &[(wx - reach, y), (wx, y - h * 0.5), (wx + ww, y - h * 0.5), (wx + ww + reach, y), (wx + ww, y + h * 0.5), (wx, y + h * 0.5)], b.color, a * 3 / 4, Mode::Add);
            fd::poly(cv, &[(wx - reach * 0.6, y), (wx, y - h * 0.18), (wx + ww, y - h * 0.18), (wx + ww + reach * 0.6, y), (wx + ww, y + h * 0.18), (wx, y + h * 0.18)], 0xFFFF_FFFF, a, Mode::Add);
        }
        let clip_to = |cv: &mut Canvas, seat: usize| {
            if seat < 6 && geoms[seat].cell > 0 {
                let g = geoms[seat];
                cv.set_clip(g.x - 10, g.y - 10, g.w + 20, g.h + 20);
            } else {
                cv.reset_clip();
            }
        };
        let _pr = crate::prof::scope("fx: rings");
        // rings
        for r in &self.rings {
            if r.delay > 0.0 {
                continue;
            }
            clip_to(cv, r.seat);
            let e = ease_out(r.t / r.dur);
            let rad = r.r0 + (r.r1 - r.r0) * e;
            match r.style {
                RingStyle::Crisp => {
                    let th = r.th * (1.0 - e).powf(1.3);
                    if th * k < 1.5 {
                        continue;
                    }
                    let a = ((1.0 - e).sqrt() * 255.0) as u32;
                    fd::ring(cv, r.x, r.y, rad, th, r.color, a, Mode::Over, false);
                    if th * k >= 4.0 {
                        fd::ring(cv, r.x, r.y, rad - th * 0.18, th * 0.45, 0xFFFF_FFFF, a * 3 / 4, Mode::Add, false);
                    }
                }
                RingStyle::Soft => {
                    fd::ring(cv, r.x, r.y, rad, r.th * (1.0 - 0.5 * e), r.color, ((1.0 - e) * 170.0) as u32, Mode::Add, true);
                }
            }
        }
        cv.reset_clip();
        drop(_pr);
        let _pr = crate::prof::scope("fx: rays");
        // rays: tapered spikes thrown outward
        for r in &self.rays {
            if r.delay > 0.0 {
                continue;
            }
            clip_to(cv, r.seat);
            let e = r.t / r.dur;
            let mut sd = r.seed;
            for i in 0..r.n {
                sd = sd.wrapping_mul(1664525).wrapping_add(1013904223);
                let jit = (sd >> 8) as f32 / 16777216.0;
                let a = i as f32 / r.n as f32 * TAU + jit * 0.4;
                let len = r.len * (0.55 + 0.45 * jit) * (1.0 - e * 0.3);
                let r0 = r.r0 + r.len * 0.6 * ease_out(e);
                let w = r.width * (0.6 + 0.8 * jit) * (1.0 - e);
                let (ca, sa) = (a.cos(), a.sin());
                let (x0, y0) = (r.x + ca * r0, r.y + sa * r0);
                let (x1, y1) = (r.x + ca * (r0 + len), r.y + sa * (r0 + len));
                let al = ((1.0 - e) * 170.0) as u32;
                fd::streak(cv, x0, y0, x1, y1, w, 0.0, r.color, al, Mode::Add);
                fd::streak(cv, x0, y0, x0 + (x1 - x0) * 0.5, y0 + (y1 - y0) * 0.5, w * 0.35, 0.0, 0xFFFF_FFFF, al * 2 / 3, Mode::Add);
            }
        }
        cv.reset_clip();
        drop(_pr);
        // T-spin crescents
        for sp in &self.spins {
            let e = sp.t / sp.dur;
            let rad = sp.r * (0.7 + 0.8 * ease_out(e));
            let al = ((1.0 - e) * 255.0) as u32;
            for j in 0..3 {
                let a0 = j as f32 / 3.0 * TAU + e * TAU * 1.4;
                let segs = 8;
                for sgi in 0..segs {
                    let f0 = sgi as f32 / segs as f32;
                    let f1 = (sgi + 1) as f32 / segs as f32;
                    let (b0, b1) = (a0 + f0 * 2.0, a0 + f1 * 2.0);
                    let w0 = sp.r * 0.32 * (1.0 - f0) * (1.0 - e * 0.5);
                    let w1 = sp.r * 0.32 * (1.0 - f1) * (1.0 - e * 0.5);
                    let (x0, y0) = (sp.x + b0.cos() * rad, sp.y + b0.sin() * rad);
                    let (x1, y1) = (sp.x + b1.cos() * rad, sp.y + b1.sin() * rad);
                    fd::streak(cv, x0, y0, x1, y1, w0, w1, sp.color, al, Mode::Add);
                    fd::streak(cv, x0, y0, x1, y1, w0 * 0.4, w1 * 0.4, 0xFFFF_FFFF, al, Mode::Add);
                }
            }
        }
        let _pp = crate::prof::scope("fx: particles");
        // particles
        let parts = std::mem::take(&mut self.particles);
        for p in &parts {
            if p.delay > 0.0 {
                continue;
            }
            let life = p.life / p.max;
            let fade = if life < 0.3 { life / 0.3 } else { 1.0 };
            let (px, py) = (p.x * k, p.y * k);
            match p.k {
                PK::Spark => {
                    let e = 1.0 - life;
                    let s = p.size * (e * 5.0).min(1.0) * (1.0 - e).powf(0.7);
                    let sz = (s * k).round() as i32;
                    if sz >= 3 {
                        let spr = self.sprite(PK::Spark, 0, sz, 0);
                        fd::light_px(cv, spr, px, py, p.color, 255, true);
                    }
                }
                PK::Dot | PK::Halo => {
                    let sz = (p.size * k).round() as i32;
                    if sz >= 3 {
                        let spr = self.sprite(PK::Dot, 0, sz, 0);
                        fd::light_px(cv, spr, px, py, p.color, (fade * 255.0) as u32, true);
                    }
                }
                PK::Puff => {
                    let s = p.size * (0.6 + 0.7 * (1.0 - life));
                    let sz = (s * k).round() as i32;
                    if sz >= 3 {
                        let spr = self.sprite(PK::Puff, p.color, sz, 0);
                        fd::sprite_px(cv, spr, px, py, 1.0, (life.min(0.6) / 0.6 * 230.0) as u32);
                    }
                }
                PK::Drop => {
                    let s = p.size * (0.45 + 0.55 * life);
                    let sz = (s * k).round() as i32;
                    if sz >= 2 {
                        let spr = self.sprite(PK::Drop, p.color, sz, 0);
                        fd::sprite_px(cv, spr, px, py, 1.0, (fade * 255.0) as u32);
                    }
                }
                PK::Shard | PK::Star => {
                    let sz = (p.size * k).round() as i32;
                    if sz >= 2 {
                        let span = if p.k == PK::Shard { PI } else { TAU / 5.0 };
                        let rb = ((p.rot.rem_euclid(span) / span) * 8.0) as u8 % 8;
                        let spr = self.sprite(p.k, p.color, sz, rb);
                        fd::sprite_px(cv, spr, px, py, 1.0, (fade * 255.0) as u32);
                    }
                }
            }
        }
        self.particles = parts;
        drop(_pp);
        let _pj = crate::prof::scope("fx: projectiles");
        // projectiles: a bright head on an arcing path with a tapering tail of light
        for i in 0..self.projs.len() {
            let (kind, dst) = (self.projs[i].kind, self.projs[i].dst);
            if geoms[dst].cell > 0 {
                self.projs[i].end = match kind {
                    ProjKind::Attack => meter_point(m, dst, &geoms[dst]),
                    ProjKind::Star => plate_point(m, dst, &geoms[dst]),
                };
            }
            let p = self.projs[i];
            if p.delay > 0.0 {
                continue;
            }
            let pos = |t: f32| -> (f32, f32) {
                let t = t.clamp(0.0, 1.0);
                let e = t * t * (3.0 - 2.0 * t) * 0.35 + t * t * 0.65;
                let (x0, y0) = p.from;
                let (x1, y1) = p.end;
                let dist = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt();
                let side = if p.seed & 1 == 0 { 1.0 } else { -1.0 };
                let (cx, cy) = ((x0 + x1) / 2.0 + side * dist * 0.08, y0.min(y1) - 60.0 - dist * 0.25);
                let u = 1.0 - e;
                (u * u * x0 + 2.0 * u * e * cx + e * e * x1, u * u * y0 + 2.0 * u * e * cy + e * e * y1)
            };
            let tn = p.t / p.dur;
            let (hx, hy) = pos(tn);
            let col = p.color;
            let sz = p.size;
            let tail = 6;
            let mut prev = (hx, hy);
            for j in 1..=tail {
                let tj = tn - j as f32 * 0.04;
                if tj < 0.0 {
                    break;
                }
                let pt = pos(tj);
                let f0 = 1.0 - (j - 1) as f32 / tail as f32;
                let f1 = 1.0 - j as f32 / tail as f32;
                fd::streak(cv, prev.0, prev.1, pt.0, pt.1, sz * 1.1 * f0, sz * 1.1 * f1, col, (f0 * 200.0) as u32, Mode::Add);
                if j <= 2 {
                    fd::streak(cv, prev.0, prev.1, pt.0, pt.1, sz * 0.5 * f0, sz * 0.5 * f1, 0xFFFF_FFFF, (f0 * 220.0) as u32, Mode::Add);
                }
                prev = pt;
            }
            let gl = (sz * 3.2 * k) as i32;
            let spr = self.sprite(PK::Halo, 0, gl, 0);
            fd::light_px(cv, spr, hx * k, hy * k, lighten(col, 40), 200, true);
            match kind {
                ProjKind::Attack => {
                    let spr = self.sprite(PK::Drop, lighten(col, 50), (sz * 1.25 * k) as i32, 0);
                    fd::sprite_px(cv, spr, hx * k, hy * k, 1.0, 255);
                }
                ProjKind::Star => {
                    let rb = ((p.t * 14.0) as u32 % 8) as u8;
                    let szp = (sz * 1.6 * k).round() as i32;
                    let spr = self.sprite(PK::Star, col, szp, rb);
                    fd::sprite_px(cv, spr, hx * k, hy * k, 1.0, 255);
                }
            }
        }
        drop(_pj);
        let _pl = crate::prof::scope("fx: labels");
        // labels
        for l in &self.labels {
            if l.delay > 0.0 {
                continue;
            }
            let g = geoms[l.seat];
            if g.cell == 0 {
                continue;
            }
            let (x, y) = (g.fx as f32 + l.dx, g.fy as f32 + l.dy);
            let t = l.t;
            let left = if l.persist == Persist::No { l.dur - t } else { 9.0 };
            let (mut scale, mut alpha, mut lift) = (1.0f32, 1.0f32, 0.0f32);
            if q.motion {
                match l.kind {
                    LabelKind::Slam => {
                        if t < 0.09 {
                            let e = t / 0.09;
                            scale = 2.0 - 1.15 * e * e;
                            alpha = (t / 0.04).min(1.0);
                        } else {
                            scale = 0.85 + 0.15 * ease_out_back((t - 0.09) / 0.13);
                        }
                        if left < 0.22 {
                            let e = 1.0 - left / 0.22;
                            scale *= 1.0 + 0.12 * e;
                            alpha = 1.0 - e;
                            lift = e * 14.0;
                        }
                    }
                    LabelKind::Counter => {
                        scale = if t < 0.07 { 0.2 + 1.1 * t / 0.07 } else { 1.0 + 0.3 * (1.0 - ease_out_back((t - 0.07) / 0.14)).max(-0.1) };
                        lift = ease_out(t / l.dur) * 26.0;
                        if left < 0.3 {
                            alpha = left / 0.3;
                        }
                    }
                    LabelKind::Chip => {
                        scale = if t < 0.12 { 0.5 + 0.5 * ease_out_back(t / 0.12) } else { 1.0 };
                        alpha = (t / 0.06).min(1.0);
                        if left < 0.22 {
                            alpha = left / 0.22;
                            lift = (1.0 - left / 0.22) * 14.0;
                        }
                    }
                }
            } else {
                alpha = (t / 0.12).min(1.0);
                if left < 0.25 {
                    alpha = alpha.min(left / 0.25);
                }
            }
            let alpha = (alpha.clamp(0.0, 1.0) * 255.0) as u32;
            fd::sprite_px(cv, &l.spr, x * k, (y - lift) * k, scale, alpha);
        }
        // a sparkle where each headline lands
        let flares: Vec<(f32, f32, u32, f32)> = self
            .labels
            .iter()
            .filter(|l| l.flare != 0 && l.t > 0.07 && l.t < 0.45 && geoms[l.seat].cell > 0)
            .map(|l| {
                let g = geoms[l.seat];
                let hw = l.spr.w as f32 / k * 0.5;
                (g.fx as f32 + l.dx + hw * 0.82, g.fy as f32 + l.dy - l.spr.h as f32 / k * 0.28, l.flare, (l.t - 0.07) / 0.38)
            })
            .collect();
        for (x, y, c, e) in flares {
            let s = 46.0 * (e * 4.0).min(1.0) * (1.0 - e);
            let sz = (s * k).round() as i32;
            if sz >= 3 {
                let spr = self.sprite(PK::Spark, 0, sz, 0);
                fd::light_px(cv, spr, x * k, y * k, lighten(c, 80), 255, true);
            }
        }
        if self.screen > 0.02 && q.flashes {
            fd::rect(cv, 0, 0, 1280, 720, 0xFFFF_F4E0, (self.screen * 64.0) as u32, Mode::Add);
        }
    }

    fn sprite(&mut self, kind: PK, color: u32, s: i32, rot: u8) -> &Canvas {
        let s = bucket(s as f32);
        if self.sprites.len() > 4000 {
            self.sprites.clear();
        }
        self.sprites.entry((kind, color, s, rot)).or_insert_with(|| {
            let _p = crate::prof::scope("fx build sprite");
            make_sprite(kind, color, s, rot)
        })
    }

    fn draw_callouts(&self, cv: &mut Canvas, geoms: &[Geom; 6]) {
        for c in &self.callouts {
            let g = geoms[c.seat];
            if g.cell == 0 {
                continue;
            }
            let rise = (c.t * 30.0) as i32;
            let a = if c.t > 1.0 { ((1.4 - c.t) / 0.4 * 255.0) as u32 } else { 255 };
            let cx = g.fx + g.cols * g.cell / 2;
            let y = g.fy + g.rows * g.cell / 3 - rise;
            let mut sc = c.scale;
            while sc > 1 && Canvas::text_w(&c.text, sc) > g.cols * g.cell + g.cell * 2 {
                sc -= 1;
            }
            let txt = crate::ui::nice(&c.text);
            let w = Canvas::text_wb(&txt, sc, Weight::Black);
            let x0 = cx - w / 2;
            for (dx, dy) in [(-2, 0), (2, 0), (0, -2), (0, 2), (-2, -2), (2, 2), (-2, 2), (2, -2), (0, 3)] {
                cv.text_weight(x0 + dx, y + dy, &txt, sc, 0xFF0A_0E1E, a * 3 / 4, Weight::Black);
            }
            cv.text_weight(x0, y, &txt, sc, c.color, a, Weight::Black);
        }
    }

    /// Bench and shots: stage a heavy moment on every board at once (a long chain, a big
    /// attack at the next seat, a KO on the last one) without touching the sim.
    /// The first `big` seats get the big move; the rest a 2-chain or a double.
    pub fn stage(&mut self, m: &Match, geoms: &[Geom; 6], st: &Settings, chain: u32, ko_last: bool, big: usize) {
        let q = quality(st);
        self.motion = q.motion;
        let n_active = m.active_count();
        let seats: Vec<usize> = (0..6).filter(|&i| geoms[i].cell > 0 && m.seats[i].board.is_some()).collect();
        for (j, &s) in seats.iter().enumerate() {
            let g = geoms[s];
            let style = m.seats[s].cfg.profile.style();
            let mut ti = TickInfo::default();
            let cols = [RED, GREEN, BLUE, YELLOW, PURPLE];
            let (cw, ch) = (g.cols, g.rows);
            for c in 0..(cw * 3).min(18) {
                let x = c % cw;
                let y = ch - 1 - c / cw;
                ti.bursts.push(((g.fx + x * g.cell + g.cell / 2) as f32, (g.fy + y * g.cell + g.cell / 2) as f32, cols[(c as usize + j) % 5]));
            }
            match style {
                Style::Quad if j >= big => {
                    ti.clear = Some((2, "DOUBLE".into()));
                    ti.rows = (purtris_sim::quad::H - 2..purtris_sim::quad::H).collect();
                }
                _ if j >= big => {
                    ti.chain = 2;
                    ti.clear = Some((6, "2 CHAIN".into()));
                }
                Style::Quad => {
                    ti.clear = Some((8, if j % 2 == 0 { "B2B T-SPIN TRIPLE  4 COMBO".into() } else { "B2B QUAD".into() }));
                    ti.rows = (purtris_sim::quad::H - 3..purtris_sim::quad::H).collect();
                    ti.lock = Some((4, purtris_sim::quad::H - 2));
                }
                _ => {
                    ti.chain = chain;
                    ti.clear = Some((chain * 2 + 2, format!("{} CHAIN", chain)));
                }
            }
            let _ = &ti;
            self.seat_tick(m, s, &g, geoms, ti, &q, st, n_active);
            // an attack to the next seat
            if seats.len() > 1 {
                let d = seats[(j + 1) % seats.len()];
                let seed = (self.rnd() * 1e6) as u32;
                let (x, y) = well_center(&g);
                self.projs.push(Proj {
                    kind: ProjKind::Attack,
                    from: (x, y),
                    dst: d,
                    t: 0.0,
                    dur: 0.5,
                    delay: 0.0,
                    color: style_accent(style),
                    size: if j < big { 16.0 } else { 11.0 },
                    end: meter_point(m, d, &geoms[d]),
                    seed,
                });
            }
        }
        if ko_last {
            if let Some(&s) = seats.last() {
                let g = geoms[s];
                self.ko(m, s, &g, geoms, &q, st);
                // the label persists only while the seat is out; staged on a live seat it fades
                if let Some(l) = self.labels.iter_mut().rev().find(|l| l.seat == s && l.persist == Persist::WhileDead) {
                    l.persist = Persist::No;
                }
            }
        }
    }

    /// Bench and shots: stage an all clear on one seat.
    pub fn stage_all_clear(&mut self, m: &Match, geoms: &[Geom; 6], st: &Settings, seat: usize) {
        let q = quality(st);
        let ti = TickInfo { all_clear: true, clear: Some((6, String::new())), ..Default::default() };
        let g = geoms[seat];
        self.seat_tick(m, seat, &g, geoms, ti, &q, st, m.active_count());
    }
}
