//! Immediate-mode UI on liquid-glass cards. Every text color here is chosen for contrast
//! against the dark glass tint, so text stays readable over any scene.
//!
//! House style: every part of the game has its own color (its *section*). The section color
//! paints the slanted header banner, the highlighted row and the menu tiles; halftone dots and
//! a rounded display face give the screens their arcade look. Button prompts sit on a blue
//! band in the bottom-right corner.

use crate::font::Weight;
use crate::gfx::{darken, lighten, Canvas};
use crate::input::Nav;
use crate::theme::Look;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

// ------------------------------------------------------------------ section colors
pub const S_STORY: u32 = 0xFF2C_B65A;
pub const S_VERSUS: u32 = 0xFFE8_3A66;
pub const S_SOLO: u32 = 0xFFF2_B21C;
pub const S_TRAINING: u32 = 0xFFF2_6A2C;
pub const S_REPLAYS: u32 = 0xFF2F_7BEA;
pub const S_PROFILE: u32 = 0xFFCC_4BC8;
pub const S_RANKS: u32 = 0xFF13_A7BD;
pub const S_SETTINGS: u32 = 0xFF7B_5BEA;
/// Deep navy ink for outlines and dark text on light panels.
pub const INK: u32 = 0xFF1C_1B48;
/// The prompt band along the bottom-right.
pub const BAND: u32 = 0xFF1E_5ED8;

static SECTION: AtomicU32 = AtomicU32::new(S_STORY);
/// Set the color of the part of the game being drawn (the App does this per screen).
pub fn set_section(c: u32) {
    SECTION.store(c, Ordering::Relaxed);
}
pub fn section() -> u32 {
    SECTION.load(Ordering::Relaxed)
}

// ------------------------------------------------------------------ shapes
/// A parallelogram leaning right: the top edge is `skew` units right of the bottom edge.
pub fn slant(x: i32, y: i32, w: i32, h: i32, skew: i32) -> [(f32, f32); 4] {
    let (x, y, w, h, k) = (x as f32, y as f32, w as f32, h as f32, skew as f32);
    [(x + k, y), (x + w + k, y), (x + w, y + h), (x, y + h)]
}
/// The horizontal band `t0..t1` (0 = top, 1 = bottom) of the same parallelogram.
pub fn slant_band(x: i32, y: i32, w: i32, h: i32, skew: i32, t0: f32, t1: f32) -> [(f32, f32); 4] {
    let (x, y, w, h, k) = (x as f32, y as f32, w as f32, h as f32, skew as f32);
    let (ya, yb) = (y + h * t0, y + h * t1);
    let (xa, xb) = (x + k * (1.0 - t0), x + k * (1.0 - t1));
    [(xa, ya), (xa + w, ya), (xb + w, yb), (xb, yb)]
}
/// A slanted slab with a border `b` units thick in `edge`.
#[allow(clippy::too_many_arguments)]
pub fn slab(cv: &mut Canvas, x: i32, y: i32, w: i32, h: i32, skew: i32, fill: u32, edge: u32, b: i32) {
    if b > 0 {
        // the border keeps the same lean: grow the shape by b on every side
        let k = skew as f32 / h.max(1) as f32;
        let bx = (b as f32 * (1.0 + k * k).sqrt()).round() as i32;
        cv.poly(&slant(x - bx, y - b, w + 2 * bx, h + 2 * b, skew + (2.0 * b as f32 * k).round() as i32), edge, 255);
    }
    cv.poly(&slant(x, y, w, h, skew), fill, 255);
}

/// Halftone dots inside a slanted area, growing from nothing at `from` (0..1 across) to `r`
/// at the right edge. `step` is the dot spacing.
#[allow(clippy::too_many_arguments)]
pub fn halftone(cv: &mut Canvas, x: i32, y: i32, w: i32, h: i32, skew: i32, c: u32, a: u32, step: i32, r: f32, from: f32) {
    let step = step.max(4);
    let mut row = 0;
    let mut yy = y + step / 2;
    while yy < y + h {
        let t = (yy - y) as f32 / h as f32;
        let left = x as f32 + skew as f32 * (1.0 - t);
        let off = if row % 2 == 1 { step as f32 / 2.0 } else { 0.0 };
        let mut xx = left + step as f32 / 2.0 + off;
        while xx < left + w as f32 - 2.0 {
            let u = (xx - left) / w as f32;
            let g = ((u - from) / (1.0 - from).max(0.01)).clamp(0.0, 1.0);
            let rr = r * g;
            if rr >= 0.6 && xx - rr > left && xx + rr < left + w as f32 {
                cv.circle(xx, yy as f32, rr, c, a);
            }
            xx += step as f32;
        }
        yy += step;
        row += 1;
    }
}

/// Text with a solid outline `t` units thick and a drop shadow: the display style.
#[allow(clippy::too_many_arguments)]
pub fn outlined(cv: &mut Canvas, x: i32, y: i32, s: &str, scale: i32, fill: u32, ink: u32, t: i32, w: Weight) {
    let t = t.max(1);
    cv.text_weight(x + 1, y + t + 2, s, scale, ink, 150, w);
    for (dx, dy) in [(-t, 0), (t, 0), (0, -t), (0, t), (-t, -t), (t, t), (-t, t), (t, -t), (0, t + 1)] {
        cv.text_weight(x + dx, y + dy, s, scale, ink, 255, w);
    }
    cv.text_weight(x, y, s, scale, fill, 255, w);
}

/// One line of text at the larger body size when it fits in `maxw`, else the small one.
#[allow(clippy::too_many_arguments)]
pub fn fit_text(cv: &mut Canvas, x: i32, y: i32, maxw: i32, s: &str, c: u32, a: u32, w: Weight) {
    let sc = if Canvas::text_wb(s, 2, w) <= maxw { 2 } else { 1 };
    // the small size sits on the same baseline area as the large one
    let dy = if sc == 2 { 0 } else { 5 };
    cv.text_weight(x, y + dy, s, sc, c, a, w);
}

/// A white strip with a dashed rule inside and dark text: descriptions under menus.
pub fn desc_strip(cv: &mut Canvas, x: i32, y: i32, w: i32, h: i32, text: &str) {
    cv.soft_shadow(x, y, w, h, 14, 8, 90);
    cv.round_rect(x, y, w, h, 14, 0xFFFF_FFFF, 246);
    let (ix, iy, iw, ih) = (x + 7, y + 7, w - 14, h - 14);
    let dash = |cv: &mut Canvas, x0: i32, y0: i32, len: i32, horiz: bool| {
        let mut p = 0;
        while p < len {
            let d = 9.min(len - p);
            if horiz {
                cv.rect_a(x0 + p, y0, d, 2, 0xFFA9_B3CE, 255);
            } else {
                cv.rect_a(x0, y0 + p, 2, d, 0xFFA9_B3CE, 255);
            }
            p += 15;
        }
    };
    dash(cv, ix + 10, iy, iw - 20, true);
    dash(cv, ix + 10, iy + ih - 2, iw - 20, true);
    dash(cv, ix, iy + 10, ih - 20, false);
    dash(cv, ix + iw - 2, iy + 10, ih - 20, false);
    let lines = crate::gfx::wrap(text, w - 60, 2);
    let lh = 26;
    let mut ty = y + (h - lines.len() as i32 * lh) / 2 + 2;
    for ln in lines.iter().take(2) {
        cv.text_weight(x + 30, ty, ln, 2, 0xFF2A_2E58, 255, Weight::Bold);
        ty += lh;
    }
}

/// Glass tint used everywhere: deep navy, strong enough for white text over bright scenes.
pub const GLASS: u32 = 0xFF0A_1124;
pub const GLASS_A: u32 = 165;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RK {
    Act,
    Opt,
    Head,
    Info,
}

pub struct Row {
    pub label: String,
    pub value: String,
    pub kind: RK,
}
impl Row {
    pub fn act(l: impl Into<String>) -> Row {
        Row { label: l.into(), value: String::new(), kind: RK::Act }
    }
    pub fn opt(l: impl Into<String>, v: impl Into<String>) -> Row {
        Row { label: l.into(), value: v.into(), kind: RK::Opt }
    }
    pub fn head(l: impl Into<String>) -> Row {
        Row { label: l.into(), value: String::new(), kind: RK::Head }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LE {
    None,
    Act(usize),
    Chg(usize, i32),
    Back,
    Moved,
}

#[derive(Clone, Copy, Default)]
pub struct ListState {
    pub sel: usize,
    pub scroll: usize,
}

pub struct ListGeom {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub row_h: i32,
    pub visible: usize,
    pub scale: i32,
}

fn selectable(r: &Row) -> bool {
    matches!(r.kind, RK::Act | RK::Opt)
}

pub fn list(cv: &mut Canvas, l: &Look, nav: &Nav, rows: &[Row], s: &mut ListState, g: &ListGeom, t: f32) -> LE {
    let mut ev = LE::None;
    if rows.is_empty() {
        return if nav.back { LE::Back } else { LE::None };
    }
    if s.sel >= rows.len() || !selectable(&rows[s.sel]) {
        s.sel = rows.iter().position(selectable).unwrap_or(0);
    }
    let step = |s: &mut ListState, d: i32| {
        let n = rows.len() as i32;
        let mut i = s.sel as i32;
        for _ in 0..n {
            i = (i + d).rem_euclid(n);
            if selectable(&rows[i as usize]) {
                s.sel = i as usize;
                break;
            }
        }
    };
    if nav.up {
        step(s, -1);
        ev = LE::Moved;
    }
    if nav.down {
        step(s, 1);
        ev = LE::Moved;
    }
    if nav.wheel != 0 {
        step(s, nav.wheel);
        ev = LE::Moved;
    }
    let cur = &rows[s.sel];
    if cur.kind == RK::Opt {
        if nav.left {
            ev = LE::Chg(s.sel, -1);
        }
        if nav.right {
            ev = LE::Chg(s.sel, 1);
        }
    }
    if nav.ok {
        ev = if cur.kind == RK::Opt { LE::Chg(s.sel, 1) } else { LE::Act(s.sel) };
    }
    if nav.back {
        ev = LE::Back;
    }
    if s.sel < s.scroll {
        s.scroll = s.sel;
    }
    if s.sel >= s.scroll + g.visible {
        s.scroll = s.sel + 1 - g.visible;
    }
    // mouse: hover selects, click activates; clicking the left part of an option steps back
    if let Some((mx, my)) = nav.mouse {
        if mx >= g.x - 12 && mx < g.x + g.w + 12 && my >= g.y {
            let k = ((my - g.y) / g.row_h) as usize + s.scroll;
            if k < rows.len() && k < s.scroll + g.visible && selectable(&rows[k]) && nav.click {
                s.sel = k;
                ev = if rows[k].kind == RK::Opt {
                    if mx < g.x + g.w - 40 - Canvas::text_w(&rows[k].value, g.scale) {
                        LE::Chg(k, 1)
                    } else if mx < g.x + g.w - 20 - Canvas::text_w(&rows[k].value, g.scale) / 2 {
                        LE::Chg(k, -1)
                    } else {
                        LE::Chg(k, 1)
                    }
                } else {
                    LE::Act(k)
                };
            }
        }
    }
    // the card behind the rows
    let shown = rows.len().min(s.scroll + g.visible) - s.scroll;
    card(cv, g.x - 22, g.y - 16, g.w + 44, shown as i32 * g.row_h + 32);
    let sc = g.scale;
    for (vi, k) in (s.scroll..s.scroll + shown).enumerate() {
        let r = &rows[k];
        let y = g.y + vi as i32 * g.row_h;
        let sel = k == s.sel;
        let ty = y + (g.row_h - (Canvas::px(sc) * 0.74) as i32) / 2 - 1;
        match r.kind {
            RK::Head => {
                let hs = (sc - 1).max(1);
                let hc = lighten(section(), 110);
                cv.text_weight(g.x + 4, y + g.row_h - (Canvas::px(hs) * 1.25) as i32, &r.label.to_uppercase(), hs, hc, 255, Weight::Bold);
                cv.rect_a(g.x + 4, y + g.row_h - 4, g.w - 8, 2, hc, 110);
            }
            _ => {
                if sel {
                    highlight(cv, g.x - 10, y + 3, g.w + 20, g.row_h - 6, t);
                    // the arcade cursor: a white arrow pointing at the chosen row
                    let (ax, ay) = ((g.x - 30) as f32 + ((t * 6.0).sin() * 2.0), (y + g.row_h / 2) as f32);
                    cv.tri([(ax + 12.0, ay), (ax, ay - 9.0), (ax, ay + 9.0)], darken(section(), 120), 255);
                    cv.tri([(ax + 10.0, ay - 1.0), (ax - 1.0, ay - 9.0), (ax - 1.0, ay + 7.0)], 0xFFFF_FFFF, 255);
                } else if vi % 2 == 1 && r.kind != RK::Info {
                    cv.round_rect(g.x - 10, y + 3, g.w + 20, g.row_h - 6, (g.row_h - 6) / 2, 0xFFFF_FFFF, 10);
                }
                let col = if r.kind == RK::Info { l.dim } else if sel { 0xFFFF_FFFF } else { l.text };
                let label = nice(&r.label);
                let r = &Row { label, value: nice(&r.value), kind: r.kind };
                let w = if sel { Weight::Bold } else { Weight::Regular };
                let shade = if sel { darken(section(), 150) } else { 0xFF00_0000 };
                cv.text_weight(g.x + 8 + 1, ty + 1, &r.label, sc, shade, if sel { 200 } else { 120 }, w);
                cv.text_weight(g.x + 8, ty, &r.label, sc, col, 255, w);
                if !r.value.is_empty() {
                    let v = if r.kind == RK::Opt && sel { format!("\u{2190}  {}  \u{2192}", r.value) } else { r.value.clone() };
                    let vw = Canvas::text_wb(&v, sc, Weight::Regular);
                    let vc = if sel { 0xFFFF_FFFF } else if r.kind == RK::Opt { l.accent } else { l.dim };
                    cv.text_weight(g.x + g.w - 8 - vw + 1, ty + 1, &v, sc, shade, if sel { 200 } else { 120 }, Weight::Regular);
                    cv.text_weight(g.x + g.w - 8 - vw, ty, &v, sc, vc, 255, Weight::Regular);
                }
            }
        }
    }
    if s.scroll > 0 {
        cv.text_center(g.x + g.w / 2, g.y - 14, "\u{2191}", 1, l.dim);
    }
    if s.scroll + g.visible < rows.len() {
        cv.text_center(g.x + g.w / 2, g.y + g.visible as i32 * g.row_h + 2, "\u{2193}", 1, l.dim);
    }
    ev
}

/// The highlighted row: a pill in the section color with a white rim that breathes.
pub fn highlight(cv: &mut Canvas, x: i32, y: i32, w: i32, h: i32, t: f32) {
    let c = section();
    let pulse = ((t * 4.0).sin() * 0.5 + 0.5) * 30.0;
    cv.round_rect(x - 2, y - 2, w + 4, h + 4, (h + 4) / 2, 0xFFFF_FFFF, 200 + pulse as u32);
    cv.round_grad(x, y, w, h, h / 2, lighten(c, 36), darken(c, 14), 255);
    cv.round_rect(x + h / 3, y + 2, w - h * 2 / 3, h * 2 / 5, h / 5, 0xFFFF_FFFF, 38);
}

/// A side card in the house style (see [`card`]).
pub fn panel(cv: &mut Canvas, x: i32, y: i32, w: i32, h: i32, _l: &Look) {
    card(cv, x, y, w, h);
}

/// The card that holds lists and side panels: a white rim, a deep navy body, and a strip of
/// the section color with halftone dots along the top, like the menu tiles.
pub fn card(cv: &mut Canvas, x: i32, y: i32, w: i32, h: i32) {
    let c = section();
    let r = 20;
    cv.soft_shadow(x, y, w, h, r, 12, 110);
    cv.round_rect(x - 3, y - 3, w + 6, h + 6, r + 3, 0xFFFF_FFFF, 215);
    cv.round_grad(x, y, w, h, r, 0xFF1B_2352, 0xFF0D_1230, 246);
    let sh = 10.min(h / 4);
    cv.set_clip(x, y, w, sh + 2);
    cv.round_rect(x, y, w, sh * 3, r, c, 255);
    cv.reset_clip();
    halftone(cv, x + w / 3, y, w - w / 3 - r / 2, sh, 0, lighten(c, 70), 150, 7, 2.2, 0.2);
}

const ACRONYMS: [&str; 14] = ["KO", "NES", "DAS", "ARR", "SDF", "SOCD", "CPU", "TSS", "TSD", "TST", "PC", "B2B", "HD", "UI"];

/// Turn ALL-CAPS text into sentence case (keeping real acronyms); anything else is unchanged.
pub fn nice(s: &str) -> String {
    let letters: Vec<char> = s.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.len() < 3 || letters.iter().any(|c| c.is_lowercase()) {
        return s.to_string();
    }
    let mut out = String::new();
    let mut first = true;
    for (i, word) in s.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let core: String = word.chars().filter(|c| c.is_alphanumeric()).collect();
        if ACRONYMS.contains(&core.as_str()) || core.chars().any(|c| c.is_ascii_digit()) && core.len() <= 4 {
            out.push_str(word);
        } else {
            let lw = word.to_lowercase();
            if first {
                let mut ch = lw.chars();
                if let Some(c0) = ch.next() {
                    out.extend(c0.to_uppercase());
                    out.push_str(ch.as_str());
                }
            } else {
                out.push_str(&lw);
            }
        }
        if word.chars().any(|c| c.is_alphabetic()) {
            first = word.ends_with('.') || word.ends_with(':') || word.ends_with('!');
        }
    }
    out
}

/// Title Case for screen titles.
pub fn title_case(s: &str) -> String {
    let n = nice(s);
    n.split(' ')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) if !["of", "and", "the", "to", "a", "vs"].contains(&w) => f.to_uppercase().collect::<String>() + c.as_str(),
                Some(f) => f.to_string() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The screen header: a slanted banner in the section color, the title in the display face.
pub fn title(cv: &mut Canvas, text: &str, sub: &str, l: &Look) {
    let text = title_case(text);
    let right = banner_at(cv, &text, section(), 12, 4);
    if !sub.is_empty() {
        // beside the banner when there is room (clear of the clock), else under it
        let sw = Canvas::text_w(sub, 2);
        let (sx, sy) = if right + 16 + sw + 40 <= 1150 { (right + 16, 33) } else { (40, 86) };
        cv.round_rect(sx - 14, sy - 6, sw + 28, 34, 17, GLASS, 170);
        cv.text_a(sx + 1, sy + 1, sub, 2, 0xFF00_0000, 150);
        cv.text_s(sx, sy, sub, 2, l.text);
    }
}

/// A panel heading: bold, in a light tint of the section color, with a rule under it.
pub fn heading(cv: &mut Canvas, x: i32, y: i32, w: i32, text: &str) {
    let c = lighten(section(), 110);
    cv.text_weight(x, y, text, 2, c, 255, Weight::Bold);
    cv.rect_a(x, y + 26, w, 2, c, 110);
}

/// The slanted header banner on its own (for screens that place their own subtitle).
pub fn banner(cv: &mut Canvas, text: &str, c: u32) {
    banner_at(cv, text, c, 12, 4);
}

/// A banner at `y` with the title spelled in the logo's letter tiles: `sc` 4 for screen
/// titles, 5 for the results. Returns the banner's right edge.
pub fn banner_at(cv: &mut Canvas, text: &str, c: u32, y: i32, sc: i32) -> i32 {
    let up = text.to_uppercase();
    // shrink long titles so the banner stays clear of the clock in the corner
    let mut size = if sc >= 5 { 54 } else { 44 };
    let w0 = crate::blocks::width(&up, size);
    if w0 > 880 {
        size = (size * 880 / w0).max(28);
    }
    let tw = crate::blocks::width(&up, size);
    let h = size + 20;
    let (x, w, k) = (-40, tw + 150, h * 2 / 5);
    cv.poly(&slant(x, y + 8, w + 4, h, k), darken(c, 110), 255);
    cv.poly(&slant(x, y, w, h, k), c, 255);
    cv.poly(&slant_band(x, y, w, h, k, 0.0, 0.46), 0xFFFF_FFFF, 34);
    halftone(cv, x, y, w, h, k, lighten(c, 80), 120, 9, 3.4, 0.55);
    cv.poly(&slant(x + w - 12, y, 6, h, k), 0xFFFF_FFFF, 200);
    crate::blocks::draw(cv, &up, 58, y + (h - size) / 2 - 1, size, tile_color0(&up), title_anim(&up), 255);
    x + w + k
}

/// Each title starts on its own tile color, so neighbouring screens don't all open in red.
fn tile_color0(up: &str) -> usize {
    up.bytes().fold(0usize, |a, b| a.wrapping_mul(31).wrapping_add(b as usize)) % 6
}

// ------------------------------------------------------------------ title drop-in
static NOW: AtomicU32 = AtomicU32::new(0);
static MOTION: AtomicBool = AtomicBool::new(true);
/// Screenshot runs draw one frame per screen, so they turn the drop-in off.
pub static NO_TITLE_ANIM: AtomicBool = AtomicBool::new(false);
thread_local! {
    /// title -> (time first drawn, time last drawn)
    static SEEN: std::cell::RefCell<std::collections::HashMap<String, (f32, f32)>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// The App's clock and the Reduce motion setting, set once per frame.
pub fn set_clock(t: f32, motion: bool) {
    NOW.store(t.to_bits(), Ordering::Relaxed);
    MOTION.store(motion, Ordering::Relaxed);
}

/// Seconds since this title appeared, while its tiles are still dropping in.
fn title_anim(text: &str) -> Option<f32> {
    if !MOTION.load(Ordering::Relaxed) || NO_TITLE_ANIM.load(Ordering::Relaxed) {
        return None;
    }
    let now = f32::from_bits(NOW.load(Ordering::Relaxed));
    SEEN.with(|c| {
        let mut m = c.borrow_mut();
        if m.len() > 64 {
            m.retain(|_, v| now - v.1 < 1.0);
        }
        let e = m.entry(text.to_string()).or_insert((now, now));
        // a title that was off screen for a moment drops in again when it comes back
        if now - e.1 > 0.25 || now < e.1 {
            e.0 = now;
        }
        e.1 = now;
        // long titles drop faster, so the last tile still lands within about half a second
        let n = text.chars().filter(|c| *c != ' ').count() as f32;
        let t = (now - e.0) * (n * 0.05 / 0.55).max(1.0);
        (t < 2.0).then_some(t)
    })
}

/// A title in letter tiles, centred on `cx` (dialogs and big moments).
pub fn tile_title(cv: &mut Canvas, text: &str, cx: i32, y: i32, size: i32) {
    let up = text.to_uppercase();
    let w = crate::blocks::width(&up, size);
    crate::blocks::draw(cv, &up, cx - w / 2, y, size, tile_color0(&up), title_anim(&up), 255);
}

/// A dialog box in the section color: a title strip over a glass body. Returns the body's top.
pub fn modal(cv: &mut Canvas, x: i32, y: i32, w: i32, h: i32, title: &str) -> i32 {
    let c = section();
    let th = 64;
    cv.soft_shadow(x, y, w, h, 22, 14, 120);
    cv.round_rect(x - 4, y - 4, w + 8, h + 8, 26, 0xFFFF_FFFF, 240);
    cv.round_rect(x, y, w, h, 22, darken(c, 40), 255);
    cv.round_rect(x + 8, y + th, w - 16, h - th - 8, 16, 0xFF12_1838, 255);
    halftone(cv, x + 8, y + 6, w - 16, th - 10, 0, lighten(c, 50), 110, 10, 3.2, 0.4);
    // the title in letter tiles, riding up over the top edge
    tile_title(cv, title, x + w / 2, y - 10, 50);
    y + th
}

/// A line of key hints on the prompt band.
pub fn footer(cv: &mut Canvas, text: &str, _l: &Look) {
    let sc = if Canvas::text_wb(text, 2, Weight::Bold) + 70 <= 1000 { 2 } else { 1 };
    let w = Canvas::text_wb(text, sc, Weight::Bold) + 70;
    prompt_band(cv, w);
    let ty = if sc == 2 { 682 } else { 687 };
    cv.text_weight(1280 - w + 39, ty + 1, text, sc, darken(BAND, 120), 200, Weight::Bold);
    cv.text_weight(1280 - w + 38, ty, text, sc, 0xFFFF_FFFF, 255, Weight::Bold);
}

/// The blue band in the bottom-right corner that carries the button prompts; returns its left x.
fn prompt_band(cv: &mut Canvas, w: i32) -> i32 {
    let (y, h, k) = (668, 52, 20);
    let x = 1280 - w;
    cv.poly(&slant(x - 10, y - 4, w + 40, h + 8, k), 0xFFFF_FFFF, 230);
    cv.poly(&slant(x, y, w + 30, h, k), BAND, 255);
    cv.poly(&slant_band(x, y, w + 30, h, k, 0.0, 0.42), 0xFFFF_FFFF, 26);
    halftone(cv, x, y, w + 30, h, k, lighten(BAND, 60), 70, 8, 2.6, 0.0);
    x
}

pub fn cycle<T: Copy + PartialEq>(all: &[T], cur: T, d: i32) -> T {
    let i = all.iter().position(|&x| x == cur).unwrap_or(0) as i32;
    all[(i + d).rem_euclid(all.len() as i32) as usize]
}

// ------------------------------------------------------------------ shared widgets

/// A row of tabs (glass pills); returns the tab the mouse clicked, if any.
pub fn tabs(cv: &mut Canvas, l: &Look, names: &[&str], active: usize, y: i32, mouse: Option<(i32, i32)>, click: bool) -> Option<usize> {
    tabs_ex(cv, l, names, active, y, mouse, click, None)
}

/// Tabs that can hold the keyboard focus: with `focus` = Some(time) the active tab wears a
/// breathing white ring and arrows on either side, to show that left / right switch tabs.
#[allow(clippy::too_many_arguments)]
pub fn tabs_ex(cv: &mut Canvas, l: &Look, names: &[&str], active: usize, y: i32, mouse: Option<(i32, i32)>, click: bool, focus: Option<f32>) -> Option<usize> {
    let pad = 28;
    let widths: Vec<i32> = names.iter().map(|n| Canvas::text_wb(n, 2, Weight::Bold) + pad * 2).collect();
    let total: i32 = widths.iter().sum::<i32>() + 10 * (names.len() as i32 - 1);
    let mut x = 640 - total / 2;
    let h = 46;
    cv.glass(x - 10, y - 8, total + 20, h + 16, (h + 16) / 2, GLASS, 150);
    let mut hit = None;
    for (i, n) in names.iter().enumerate() {
        let w = widths[i];
        if i == active {
            let c = section();
            if let Some(t) = focus {
                let pulse = ((t * 4.0).sin() * 0.5 + 0.5) * 120.0;
                cv.round_rect(x - 6, y - 6, w + 12, h + 12, (h + 12) / 2, 0xFFFF_FFFF, 110 + pulse as u32);
                let cy = (y + h / 2) as f32;
                cv.tri([((x - 22) as f32, cy), ((x - 12) as f32, cy - 8.0), ((x - 12) as f32, cy + 8.0)], 0xFFFF_FFFF, 255);
                cv.tri([((x + w + 22) as f32, cy), ((x + w + 12) as f32, cy - 8.0), ((x + w + 12) as f32, cy + 8.0)], 0xFFFF_FFFF, 255);
            }
            cv.round_rect(x - 2, y - 2, w + 4, h + 4, (h + 4) / 2, 0xFFFF_FFFF, 220);
            cv.round_grad(x, y, w, h, h / 2, lighten(c, 36), darken(c, 14), 255);
            cv.text_weight(x + pad + 1, y + 12, n, 2, darken(c, 150), 200, Weight::Bold);
            cv.text_weight(x + pad, y + 11, n, 2, 0xFFFF_FFFF, 255, Weight::Bold);
        } else {
            cv.text_weight(x + pad, y + 11, n, 2, l.text, 230, Weight::Bold);
        }
        if let Some((mx, my)) = mouse {
            if click && mx >= x && mx < x + w && my >= y && my < y + h {
                hit = Some(i);
            }
        }
        x += w + 10;
    }
    hit
}

/// An on/off switch, right-aligned at `rx`, vertically centred on `cy`.
pub fn switch(cv: &mut Canvas, l: &Look, rx: i32, cy: i32, on: bool) {
    let (w, h) = (64, 32);
    let x = rx - w;
    let _ = l;
    cv.round_rect(x - 2, cy - h / 2 - 2, w + 4, h + 4, h / 2 + 2, 0xFFFF_FFFF, if on { 200 } else { 0 });
    cv.round_rect(x, cy - h / 2, w, h, h / 2, if on { 0xFF2F_C46E } else { 0xFF2A_3150 }, 255);
    cv.round_rect(x + 1, cy - h / 2 + 1, w - 2, h / 2, h / 2, 0xFFFF_FFFF, 26);
    let kx = if on { x + w - h / 2 } else { x + h / 2 };
    cv.circle(kx as f32, cy as f32 + 1.5, 12.5, 0xFF00_0000, 70);
    cv.circle(kx as f32, cy as f32, 12.0, 0xFFF4_F6FF, 255);
}

/// A choice: `< value >` with the arrows shown when selected.
pub fn choice(cv: &mut Canvas, l: &Look, rx: i32, cy: i32, value: &str, sel: bool) {
    let tw = Canvas::text_wb(value, 2, Weight::Bold);
    let w = tw + 84;
    let x = rx - w;
    cv.round_rect(x, cy - 18, w, 36, 18, 0xFF1C_2340, 235);
    if sel {
        cv.round_rect(x, cy - 18, w, 36, 18, section(), 90);
    }
    let ac = if sel { lighten(section(), 120) } else { l.dim };
    cv.tri([((x + 20) as f32, cy as f32), ((x + 30) as f32, cy as f32 - 7.0), ((x + 30) as f32, cy as f32 + 7.0)], ac, 255);
    cv.tri([((x + w - 20) as f32, cy as f32), ((x + w - 30) as f32, cy as f32 - 7.0), ((x + w - 30) as f32, cy as f32 + 7.0)], ac, 255);
    cv.text_weight(x + 42, cy - 12, value, 2, 0xFFFF_FFFF, 255, Weight::Bold);
}

/// A slider showing `v` of `0..=max` with its number.
pub fn slider(cv: &mut Canvas, l: &Look, rx: i32, cy: i32, v: i32, max: i32, label: &str) {
    let w = 240;
    let tw = Canvas::text_wb(label, 2, Weight::Bold).max(60);
    let x = rx - tw - 26 - w;
    let f = (v.clamp(0, max.max(1)) as f32 / max.max(1) as f32 * w as f32) as i32;
    cv.round_rect(x, cy - 5, w, 10, 5, 0xFF2A_3150, 255);
    let _ = l;
    cv.round_rect(x, cy - 5, f.max(10), 10, 5, lighten(section(), 90), 255);
    cv.circle((x + f) as f32, cy as f32 + 1.5, 11.5, 0xFF00_0000, 70);
    cv.circle((x + f) as f32, cy as f32, 11.0, 0xFFF4_F6FF, 255);
    let lw = Canvas::text_wb(label, 2, Weight::Bold);
    cv.text_weight(rx - lw, cy - 12, label, 2, 0xFFFF_FFFF, 255, Weight::Bold);
}

/// Button prompts along the bottom of a screen: pairs of (key, action).
pub fn prompts(cv: &mut Canvas, _l: &Look, items: &[(&str, &str)]) {
    let gap = 24;
    let widths: Vec<(i32, i32)> = items.iter().map(|(k, a)| ((Canvas::text_wb(k, 2, Weight::Bold) + 20).max(34), Canvas::text_wb(a, 2, Weight::Bold))).collect();
    let total: i32 = widths.iter().map(|(a, b)| a + 9 + b).sum::<i32>() + gap * (items.len() as i32 - 1).max(0);
    let x0 = prompt_band(cv, total + 76);
    let mut x = x0 + 40;
    for (i, (k, a)) in items.iter().enumerate() {
        let (kw, aw) = widths[i];
        cv.round_rect(x, 682, kw, 30, 9, darken(BAND, 90), 255);
        cv.round_rect(x, 679, kw, 30, 9, 0xFFFF_FFFF, 255);
        let tw = Canvas::text_wb(k, 2, Weight::Bold);
        cv.text_weight(x + (kw - tw) / 2, 682, k, 2, INK, 255, Weight::Bold);
        cv.text_weight(x + kw + 9 + 1, 683, a, 2, darken(BAND, 120), 200, Weight::Bold);
        cv.text_weight(x + kw + 9, 682, a, 2, 0xFFFF_FFFF, 255, Weight::Bold);
        x += kw + 9 + aw + gap;
    }
}
