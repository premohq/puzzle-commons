//! Immediate-mode UI on liquid glass: one design language for every menu, card and HUD plate.
//!
//! - Type: Mochiy Pop One (`Weight::Display`) for titles and big numbers, M PLUS Rounded
//!   (`Weight::Round`) for tabs, keycaps and small-caps labels, Lato for body text.
//! - Glass: `DEEP` for cards, `CLEAR` for chips over darker scenes, `ACCENT` for the thing
//!   that matters most on a screen (pause, results banner, the focused row's halo).
//! - Motion: selections spring between rows and tabs, presses dip; all of it snaps instantly
//!   with reduce motion. Text colors are picked for contrast against the glass over any scene.

use crate::font::Weight;
use crate::gfx::{lighten, mix, Canvas, Glass};
use crate::input::Nav;
use crate::theme::Look;
use std::cell::{Cell, RefCell};

/// Glass tint used everywhere: deep navy, strong enough for white text over bright scenes.
pub const GLASS: u32 = 0xFF0A_1124;
pub const GLASS_A: u32 = 165;

/// Cards: deep, vivid glass that keeps white text readable over the brightest world.
pub const DEEP: Glass = Glass { tint: 0xFF0A_1226, tint_a: 118, dark: 152, sat: 26, rim: 225, lens: 0.9, frost: 3, shadow: 84 };
/// Chips and pills over darker scenes: lets more of the world through.
pub const CLEAR: Glass = Glass { tint: 0xFF16_1E3A, tint_a: 70, dark: 168, sat: 28, rim: 245, lens: 1.0, frost: 3, shadow: 60 };
/// The hero card of a screen (pause, results, dialogs): tinted toward the twilight violet.
pub const ACCENT: Glass = Glass { tint: 0xFF2A_1A5C, tint_a: 140, dark: 140, sat: 24, rim: 240, lens: 1.0, frost: 3, shadow: 96 };

/// Ink for text on light surfaces (active tabs, keycaps).
pub const INK: u32 = 0xFF1B_1638;

// ------------------------------------------------------------------ frame context

#[derive(Clone, Copy)]
pub struct Frame {
    pub t: f32,
    pub dt: f32,
    /// animations on (off with reduce motion)
    pub motion: bool,
    /// the last input came from a gamepad: prompts show pad buttons
    pub pad: bool,
}
thread_local! {
    static FRAME: Cell<Frame> = const { Cell::new(Frame { t: 0.0, dt: 1.0 / 60.0, motion: true, pad: false }) };
    static TABS: RefCell<Vec<(i32, f32, f32, f32)>> = const { RefCell::new(Vec::new()) };
}
/// Called by the app once per frame, before any screen draws.
pub fn set_frame(f: Frame) {
    FRAME.with(|c| c.set(f));
}
pub fn frame() -> Frame {
    FRAME.with(|c| c.get())
}
/// Ease `cur` toward `target` (critically damped feel); snaps when motion is off.
pub fn ease(cur: &mut f32, target: f32, rate: f32) {
    let f = frame();
    if !f.motion || !cur.is_finite() {
        *cur = target;
        return;
    }
    let k = 1.0 - (-rate * f.dt.clamp(0.0, 0.1)).exp();
    *cur += (target - *cur) * k;
    if (target - *cur).abs() < 0.002 {
        *cur = target;
    }
}

// ------------------------------------------------------------------ type helpers

/// Text drawn with extra letter spacing (small-caps labels). Returns the width.
#[allow(clippy::too_many_arguments)]
pub fn tracked(cv: &mut Canvas, x: i32, y: i32, s: &str, scale: i32, c: u32, a: u32, w: Weight, track: f32) -> i32 {
    let px = Canvas::px(scale);
    let mut pen = x as f32;
    for ch in s.chars() {
        let mut b = [0u8; 4];
        let st = ch.encode_utf8(&mut b);
        crate::font::draw(cv, pen, y as f32 + px * 0.74, st, px, w, c, a);
        pen += crate::font::advance(ch, px, w) + track;
    }
    (pen - x as f32 - track).round() as i32
}
pub fn tracked_w(s: &str, scale: i32, w: Weight, track: f32) -> i32 {
    let px = Canvas::px(scale);
    (s.chars().map(|c| crate::font::advance(c, px, w) + track).sum::<f32>() - track).round() as i32
}
/// Text with a soft drop shadow in any weight.
#[allow(clippy::too_many_arguments)]
pub fn text_sw(cv: &mut Canvas, x: i32, y: i32, s: &str, scale: i32, c: u32, a: u32, w: Weight) {
    let off = if scale >= 3 { 2 } else { 1 };
    cv.text_weight(x + off, y + off, s, scale, 0xFF00_0000, a * 100 / 255, w);
    cv.text_weight(x, y, s, scale, c, a, w);
}
/// A display-face headline with a thick soft shadow (titles, big numbers).
pub fn headline(cv: &mut Canvas, x: i32, y: i32, s: &str, scale: i32, c: u32, a: u32) {
    let o = (scale / 2).max(1) + 1;
    cv.text_weight(x, y + o + 1, s, scale, 0xFF0B_0820, a * 70 / 255, Weight::Display);
    cv.text_weight(x, y + o, s, scale, 0xFF0B_0820, a * 90 / 255, Weight::Display);
    cv.text_weight(x, y, s, scale, c, a, Weight::Display);
}
/// A small-caps section label with a hairline fading off to the right.
pub fn section(cv: &mut Canvas, x: i32, y: i32, w: i32, label: &str, l: &Look) {
    let tw = tracked(cv, x, y, &label.to_uppercase(), 1, l.accent, 255, Weight::Round, 1.6);
    let lx = x + tw + 12;
    let ly = y + 8;
    let span = (x + w - lx).max(0);
    let steps = 6;
    for i in 0..steps {
        let a = 70 - i * 11;
        cv.rect_a(lx + span * i / steps, ly, span / steps + 1, 1, l.accent, a.max(0) as u32);
    }
}

// ------------------------------------------------------------------ rows and lists

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
    /// an icon drawn before the label ("" for none)
    pub icon: &'static str,
}
impl Row {
    pub fn act(l: impl Into<String>) -> Row {
        Row { label: l.into(), value: String::new(), kind: RK::Act, icon: "" }
    }
    pub fn opt(l: impl Into<String>, v: impl Into<String>) -> Row {
        Row { label: l.into(), value: v.into(), kind: RK::Opt, icon: "" }
    }
    pub fn head(l: impl Into<String>) -> Row {
        Row { label: l.into(), value: String::new(), kind: RK::Head, icon: "" }
    }
    /// The same row with an icon before its label.
    pub fn ic(mut self, icon: &'static str) -> Row {
        self.icon = icon;
        self
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
    /// the selection pill's animated row (fractional), and whether it has been placed yet
    pub fy: f32,
    pub placed: bool,
    /// the scroll offset, animated
    pub fs: f32,
    /// time of the last press (for the dip)
    pub press_t: f32,
}
impl ListState {
    pub fn at(sel: usize) -> ListState {
        ListState { sel, ..Default::default() }
    }
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
    list_impl(cv, l, nav, rows, s, g, t, true)
}

/// A list drawn straight onto a card the caller has already put down.
pub fn list_bare(cv: &mut Canvas, l: &Look, nav: &Nav, rows: &[Row], s: &mut ListState, g: &ListGeom, t: f32) -> LE {
    list_impl(cv, l, nav, rows, s, g, t, false)
}

/// The focus pill: a raised, lit glass lozenge with an accent spine and halo.
/// `press` 0..1 dips it (a quick squash when activated).
#[allow(clippy::too_many_arguments)]
pub fn focus_pill(cv: &mut Canvas, l: &Look, x: i32, y: i32, w: i32, h: i32, press: f32, t: f32) {
    let f = frame();
    let inset = (press * 4.0) as i32;
    let (x, y, w, h) = (x + inset, y + inset / 2, w - inset * 2, h - inset);
    let r = h / 2;
    // halo
    cv.round_rect(x - 5, y - 4, w + 10, h + 8, r + 4, l.accent, 18);
    cv.round_rect(x - 2, y - 2, w + 4, h + 4, r + 2, l.accent, 30);
    // body: a lit lozenge, brighter on top
    cv.round_grad(x, y, w, h, r, 0xFFFF_FFFF, 0xFF8C_9AD0, 50);
    cv.round_grad(x + 2, y + 1, w - 4, h / 2, r - 1, 0xFFFF_FFFF, 0xFFFF_FFFF, 22);
    // rim: bright on top, accent below
    cv.rim(x, y, w, h, r, 1.0, 0xFFFF_FFFF, lighten(l.accent, 60), 150);
    // spine
    let pulse = if f.motion { (t * 3.2).sin() * 0.5 + 0.5 } else { 0.6 };
    cv.round_rect(x + 9, y + h / 2 - (h / 2 - 9), 5, h - 18, 3, l.accent, 255);
    cv.glow(x as f32 + 11.5, (y + h / 2) as f32, 14.0, l.accent, (40.0 + pulse * 40.0) as u32);
    // a slow sheen that drifts across the pill
    if f.motion && w > 120 {
        let ph = ((t * 0.35).fract() * 1.8 - 0.4) * w as f32;
        let sx = x + ph as i32;
        cv.set_clip(x + r / 2, y, w - r, h);
        for i in 0..6 {
            cv.rect_a(sx + i * 6, y + 2, 6, h - 4, 0xFFFF_FFFF, (6 + 2 * (3 - (i - 3).abs())) as u32);
        }
        cv.reset_clip();
    }
}

/// `< value >` chevrons and value at the right edge of a row.
#[allow(clippy::too_many_arguments)]
fn value_chip(cv: &mut Canvas, l: &Look, rx: i32, cy: i32, v: &str, sc: i32, sel: bool, opt: bool) {
    let w = if sel { Weight::Bold } else { Weight::Regular };
    let vw = Canvas::text_wb(v, sc, w);
    let th = (Canvas::px(sc) * 0.74) as i32;
    if opt {
        let cw = vw + 56;
        let x = rx - cw;
        if sel {
            cv.round_rect(x, cy - 15, cw, 30, 15, 0xFF06_0A1C, 120);
            cv.rim(x, cy - 15, cw, 30, 15, 1.0, lighten(l.accent, 40), l.accent, 120);
        }
        let ac = if sel { l.accent } else { mix(l.dim, 0xFF00_0000, 60) };
        let (cyf, s) = (cy as f32, 5.5);
        cv.tri([((x + 13) as f32, cyf), ((x + 13) as f32 + s, cyf - s), ((x + 13) as f32 + s, cyf + s)], ac, 255);
        cv.tri([((rx - 13) as f32, cyf), ((rx - 13) as f32 - s, cyf - s), ((rx - 13) as f32 - s, cyf + s)], ac, 255);
        let vc = if sel { 0xFFFF_FFFF } else { mix(l.accent, 0xFFFF_FFFF, 60) };
        text_sw(cv, x + 28, cy - th / 2 - 2, v, sc, vc, 255, w);
    } else {
        text_sw(cv, rx - vw - 4, cy - th / 2 - 2, v, sc, l.dim, 255, Weight::Regular);
    }
}

#[allow(clippy::too_many_arguments)]
fn list_impl(cv: &mut Canvas, l: &Look, nav: &Nav, rows: &[Row], s: &mut ListState, g: &ListGeom, t: f32, card: bool) -> LE {
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
    // mouse: hover highlights, click activates; clicking the left part of an option steps back
    let mut hover = None;
    if let Some((mx, my)) = nav.mouse {
        if mx >= g.x - 12 && mx < g.x + g.w + 12 && my >= g.y {
            let k = ((my - g.y) / g.row_h) as usize + s.scroll;
            if k < rows.len() && k < s.scroll + g.visible && selectable(&rows[k]) {
                hover = Some(k);
                if nav.click {
                    s.sel = k;
                    ev = if rows[k].kind == RK::Opt {
                        if mx < g.x + g.w - 60 - Canvas::text_w(&rows[k].value, g.scale) {
                            LE::Chg(k, 1)
                        } else if mx < g.x + g.w - 28 - Canvas::text_w(&rows[k].value, g.scale) / 2 {
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
    }
    if matches!(ev, LE::Act(_) | LE::Chg(..)) {
        s.press_t = t;
    }
    // animation: the pill springs to the selection, the scroll glides
    if !s.placed {
        s.fy = s.sel as f32;
        s.fs = s.scroll as f32;
        s.placed = true;
    }
    ease(&mut s.fy, s.sel as f32, 20.0);
    ease(&mut s.fs, s.scroll as f32, 22.0);
    let press = if frame().motion { (1.0 - (t - s.press_t) / 0.16).clamp(0.0, 1.0) } else { 0.0 };
    // the card behind the rows
    let shown = rows.len().min(s.scroll + g.visible) - s.scroll;
    if card {
        cv.glass_ex(g.x - 24, g.y - 18, g.w + 48, shown as i32 * g.row_h + 36, 24, &DEEP);
    }
    let sc = g.scale;
    let scroll_px = ((s.fs - s.scroll as f32) * g.row_h as f32) as i32;
    cv.set_clip(g.x - 24, g.y - 10, g.w + 48, shown as i32 * g.row_h + 20);
    // hover ghost, then the focus pill under the rows
    if let Some(hk) = hover {
        if hk != s.sel {
            let y = g.y + (hk as i32 - s.scroll as i32) * g.row_h;
            cv.round_rect(g.x - 12, y + 3, g.w + 24, g.row_h - 6, (g.row_h - 6) / 2, 0xFFFF_FFFF, 16);
        }
    }
    let py = g.y + ((s.fy - s.fs) * g.row_h as f32) as i32;
    focus_pill(cv, l, g.x - 12, py + 3, g.w + 24, g.row_h - 6, press, t);
    let first = s.scroll.saturating_sub(1);
    let last = (s.scroll + g.visible + 1).min(rows.len());
    for k in first..last {
        let r = &rows[k];
        let y = g.y + (k as i32 - s.scroll as i32) * g.row_h - scroll_px;
        if y < g.y - g.row_h || y > g.y + g.visible as i32 * g.row_h {
            continue;
        }
        let sel = k == s.sel;
        // how close the pill is: text leans in as it arrives
        let near = (1.0 - (s.fy - k as f32).abs()).clamp(0.0, 1.0);
        let cy = y + g.row_h / 2;
        let ty = cy - (Canvas::px(sc) * 0.74) as i32 / 2 - 2;
        match r.kind {
            RK::Head => {
                section(cv, g.x + 4, y + g.row_h - 22, g.w - 8, &r.label, l);
            }
            _ => {
                // a hairline between neighbouring rows
                if k + 1 < rows.len() && rows[k + 1].kind != RK::Head && !sel && k + 1 != s.sel {
                    cv.rect_a(g.x + 8, y + g.row_h - 1, g.w - 16, 1, 0xFFFF_FFFF, 12);
                }
                let col = if r.kind == RK::Info { l.dim } else if sel { 0xFFFF_FFFF } else { l.text };
                let label = nice(&r.label);
                let w = if sel { Weight::Bold } else { Weight::Regular };
                let mut lx = g.x + 22 + (near * 6.0) as i32;
                if !r.icon.is_empty() {
                    let ic = if sel { l.accent } else { mix(l.text, 0xFF00_0000, 40) };
                    crate::assets::icon(cv, r.icon, lx + 11, cy, 24, ic, if sel { 255 } else { 210 });
                    lx += 36;
                }
                let a = if r.kind == RK::Info { 220 } else if sel { 255 } else { 225 };
                text_sw(cv, lx, ty, &label, sc, col, a, w);
                if !r.value.is_empty() {
                    value_chip(cv, l, g.x + g.w - 4, cy, &nice(&r.value), sc, sel, r.kind == RK::Opt);
                }
            }
        }
    }
    cv.reset_clip();
    // scrollbar
    if rows.len() > g.visible {
        let th = g.visible as i32 * g.row_h;
        let bx = g.x + g.w + 14;
        cv.round_rect(bx, g.y, 4, th, 2, 0xFFFF_FFFF, 26);
        let n = rows.len() as f32;
        let hh = ((g.visible as f32 / n) * th as f32).max(24.0) as i32;
        let ty = g.y + ((s.fs / (n - g.visible as f32).max(1.0)) * (th - hh) as f32) as i32;
        cv.round_rect(bx - 1, ty, 6, hh, 3, l.accent, 220);
    }
    ev
}

/// A glass card.
pub fn panel(cv: &mut Canvas, x: i32, y: i32, w: i32, h: i32, _l: &Look) {
    cv.glass_ex(x, y, w, h, 24, &DEEP);
}
/// A glass card with a header: an icon chip and a small-caps title over a hairline.
/// Returns the y where the body starts.
#[allow(clippy::too_many_arguments)]
pub fn panel_head(cv: &mut Canvas, x: i32, y: i32, w: i32, h: i32, icon: &str, head: &str, l: &Look) -> i32 {
    cv.glass_ex(x, y, w, h, 24, &DEEP);
    card_head(cv, x, y, w, icon, head, l)
}
/// The header strip of a card someone else put down.
pub fn card_head(cv: &mut Canvas, x: i32, y: i32, w: i32, icon: &str, head: &str, l: &Look) -> i32 {
    let mut tx = x + 24;
    if !icon.is_empty() {
        cv.circle((x + 38) as f32, (y + 34) as f32, 15.0, l.accent, 40);
        crate::assets::icon(cv, icon, x + 38, y + 34, 20, l.accent, 255);
        tx = x + 62;
    }
    tracked(cv, tx, y + 25, &head.to_uppercase(), 1, mix(l.accent, 0xFFFF_FFFF, 70), 255, Weight::Round, 1.8);
    let ly = y + 56;
    cv.rect_a(x + 20, ly, w - 40, 1, 0xFFFF_FFFF, 30);
    cv.rect_a(x + 20, ly + 1, w - 40, 1, 0xFF00_0000, 40);
    ly + 16
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

/// The icon a screen title gets in its tile.
fn title_icon(text: &str) -> &'static str {
    let t = text.to_lowercase();
    let pick = [
        ("versus", "sword"),
        ("solo", "timer"),
        ("arcade", "trophy"),
        ("training", "graduation-cap"),
        ("curriculum", "books"),
        ("sandbox", "flask"),
        ("calculator", "calculator"),
        ("seed", "dice-five"),
        ("replay", "film-strip"),
        ("setting", "gear-six"),
        ("control", "game-controller"),
        ("player", "identification-card"),
        ("how to play", "question"),
        ("puzzle", "puzzle-piece"),
        ("character", "user"),
        ("rank", "trophy"),
        ("level", "trophy"),
        ("board", "trophy"),
        ("lesson", "book-open"),
    ];
    pick.iter().find(|(k, _)| t.contains(k)).map(|p| p.1).unwrap_or("sparkle")
}

/// An icon tile: a glossy accent square with a white glyph (screen headers, menu items).
pub fn icon_tile(cv: &mut Canvas, l: &Look, icon: &str, x: i32, y: i32, s: i32) {
    let r = s * 3 / 10;
    cv.soft_shadow(x, y, s, s, r, 8, 90);
    cv.round_grad(x, y, s, s, r, lighten(l.accent, 50), mix(l.accent, 0xFFE0_6A8C, 110), 255);
    cv.round_grad(x + 3, y + 2, s - 6, s / 2 - 2, r - 2, 0xFFFF_FFFF, 0xFFFF_FFFF, 46);
    cv.rim(x, y, s, s, r, 1.0, 0xFFFF_FFFF, darken_a(l.accent), 170);
    crate::assets::icon(cv, icon, x + s / 2 + 1, y + s / 2 + 2, s * 6 / 10, 0xFF6A_3A10, 90);
    crate::assets::icon(cv, icon, x + s / 2, y + s / 2, s * 6 / 10, 0xFFFF_FFFF, 255);
}
fn darken_a(c: u32) -> u32 {
    crate::gfx::darken(c, 90)
}

/// Screen header: an icon tile, the title in the display face, the subtitle under it.
pub fn title(cv: &mut Canvas, text: &str, sub: &str, l: &Look) {
    let text = title_case(text);
    icon_tile(cv, l, title_icon(&text), 56, 26, 62);
    headline(cv, 134, 22, &text, 4, 0xFFFF_FFFF, 255);
    if !sub.is_empty() {
        text_sw(cv, 136, 74, sub, 2, l.dim, 235, Weight::Regular);
    }
}

/// A free-text hint pill at the bottom (prefer `prompts`).
pub fn footer(cv: &mut Canvas, text: &str, l: &Look) {
    let w = Canvas::text_wb(text, 1, Weight::Bold) + 44;
    cv.glass_ex(640 - w / 2, 678, w, 32, 16, &DEEP);
    text_sw(cv, 640 - w / 2 + 22, 686, text, 1, l.text, 255, Weight::Bold);
}

pub fn cycle<T: Copy + PartialEq>(all: &[T], cur: T, d: i32) -> T {
    let i = all.iter().position(|&x| x == cur).unwrap_or(0) as i32;
    all[(i + d).rem_euclid(all.len() as i32) as usize]
}

// ------------------------------------------------------------------ shared widgets

/// A row of tabs on a glass rail. The active tab is a lit accent pill that slides between
/// tabs; LB / RB (or Q / E) sit at the rail's ends. Returns the tab the mouse clicked.
pub fn tabs(cv: &mut Canvas, l: &Look, names: &[&str], active: usize, y: i32, mouse: Option<(i32, i32)>, click: bool) -> Option<usize> {
    let pad = 24;
    let gap = 6;
    let widths: Vec<i32> = names.iter().map(|n| Canvas::text_wb(n, 2, Weight::Round) + pad * 2).collect();
    let total: i32 = widths.iter().sum::<i32>() + gap * (names.len() as i32 - 1);
    let h = 44;
    let ends = 46;
    let x0 = 640 - total / 2;
    cv.glass_ex(x0 - 8 - ends, y - 7, total + 16 + ends * 2, h + 14, (h + 14) / 2, &DEEP);
    let pad_mode = frame().pad;
    let (lk, rk) = if pad_mode { ("LB", "RB") } else { ("Q", "E") };
    keycap(cv, x0 - ends - 2 + (ends - 30) / 2 + 4, y + h / 2, lk, pad_mode);
    keycap(cv, x0 + total + 10 + (ends - 30) / 2 - 2, y + h / 2, rk, pad_mode);
    // the sliding pill
    let mut xs = Vec::with_capacity(names.len());
    let mut x = x0;
    for w in &widths {
        xs.push(x);
        x += w + gap;
    }
    let (tx, tw) = (xs[active.min(xs.len() - 1)] as f32, widths[active.min(widths.len() - 1)] as f32);
    let (ax, aw) = TABS.with(|c| {
        let mut v = c.borrow_mut();
        let e = match v.iter_mut().find(|e| e.0 == y) {
            Some(e) => e,
            None => {
                v.push((y, tx, tw, 0.0));
                v.last_mut().unwrap()
            }
        };
        ease(&mut e.1, tx, 18.0);
        ease(&mut e.2, tw, 18.0);
        (e.1, e.2)
    });
    let (px, pw) = (ax.round() as i32, aw.round() as i32);
    cv.round_rect(px - 3, y - 3, pw + 6, h + 6, h / 2 + 3, l.accent, 40);
    cv.round_grad(px, y, pw, h, h / 2, lighten(l.accent, 70), l.accent, 255);
    cv.round_grad(px + 4, y + 2, pw - 8, h / 2 - 2, h / 2 - 2, 0xFFFF_FFFF, 0xFFFF_FFFF, 60);
    cv.rim(px, y, pw, h, h / 2, 1.0, 0xFFFF_FFFF, crate::gfx::darken(l.accent, 80), 160);
    let mut hit = None;
    for (i, n) in names.iter().enumerate() {
        let (x, w) = (xs[i], widths[i]);
        let mut hovered = false;
        if let Some((mx, my)) = mouse {
            if mx >= x && mx < x + w && my >= y && my < y + h {
                hovered = true;
                if click {
                    hit = Some(i);
                }
            }
        }
        // text color follows the pill as it slides over
        let cover = (1.0 - ((x as f32 + w as f32 / 2.0) - (ax + aw / 2.0)).abs() / (w as f32 * 0.6)).clamp(0.0, 1.0);
        if hovered && i != active {
            cv.round_rect(x, y, w, h, h / 2, 0xFFFF_FFFF, 22);
        }
        let c = mix(l.text, INK, (cover * 255.0) as u32);
        if cover < 0.5 {
            cv.text_weight(x + pad + 1, y + 12, n, 2, 0xFF00_0000, 90, Weight::Round);
        }
        cv.text_weight(x + pad, y + 11, n, 2, c, 255, Weight::Round);
    }
    hit
}

/// An on/off switch, right-aligned at `rx`, vertically centred on `cy`.
pub fn switch(cv: &mut Canvas, l: &Look, rx: i32, cy: i32, on: bool) {
    let (w, h) = (62, 32);
    let x = rx - w;
    let y = cy - h / 2;
    if on {
        cv.round_rect(x - 3, y - 3, w + 6, h + 6, h / 2 + 3, l.accent, 40);
        cv.round_grad(x, y, w, h, h / 2, crate::gfx::darken(l.accent, 30), lighten(l.accent, 30), 255);
    } else {
        cv.round_grad(x, y, w, h, h / 2, 0xFF05_0816, 0xFF1C_2446, 235);
    }
    cv.rim(x, y, w, h, h / 2, 1.0, 0xFF00_0000, 0xFFFF_FFFF, 70);
    let kx = if on { x + w - h / 2 } else { x + h / 2 };
    cv.circle(kx as f32, cy as f32 + 2.0, 13.0, 0xFF00_0000, 70);
    cv.circle(kx as f32, cy as f32, 12.5, 0xFFDA_DFEE, 255);
    cv.circle(kx as f32, cy as f32 - 1.0, 11.0, 0xFFFF_FFFF, 255);
}

/// A choice: `< value >` in a chip, the arrows lit when selected.
pub fn choice(cv: &mut Canvas, l: &Look, rx: i32, cy: i32, value: &str, sel: bool) {
    let tw = Canvas::text_wb(value, 2, Weight::Bold);
    let w = tw + 80;
    let x = rx - w;
    if sel {
        cv.round_rect(x - 2, cy - 20, w + 4, 40, 20, l.accent, 50);
    }
    cv.round_grad(x, cy - 18, w, 36, 18, 0xFF05_0816, 0xFF18_1F3E, 220);
    cv.rim(x, cy - 18, w, 36, 18, 1.0, 0xFF00_0000, if sel { l.accent } else { 0xFFFF_FFFF }, if sel { 200 } else { 50 });
    let ac = if sel { l.accent } else { mix(l.dim, 0xFF00_0000, 60) };
    let s = 6.0;
    let cyf = cy as f32;
    cv.tri([((x + 16) as f32, cyf), ((x + 16) as f32 + s, cyf - s), ((x + 16) as f32 + s, cyf + s)], ac, 255);
    cv.tri([((x + w - 16) as f32, cyf), ((x + w - 16) as f32 - s, cyf - s), ((x + w - 16) as f32 - s, cyf + s)], ac, 255);
    text_sw(cv, x + 40, cy - 13, value, 2, 0xFFFF_FFFF, 255, Weight::Bold);
}

/// A slider showing `v` of `0..=max` with its number.
pub fn slider(cv: &mut Canvas, l: &Look, rx: i32, cy: i32, v: i32, max: i32, label: &str) {
    let w = 240;
    let tw = Canvas::text_wb(label, 2, Weight::Display).max(56);
    let x = rx - tw - 26 - w;
    let f = (v.clamp(0, max.max(1)) as f32 / max.max(1) as f32 * w as f32) as i32;
    cv.round_grad(x, cy - 6, w, 12, 6, 0xFF03_0510, 0xFF1C_2446, 235);
    cv.rim(x, cy - 6, w, 12, 6, 1.0, 0xFF00_0000, 0xFFFF_FFFF, 60);
    // notches
    let n = max.clamp(1, 20);
    for i in 1..n {
        let nx = x + w * i / n;
        cv.rect_a(nx, cy - 2, 1, 4, 0xFFFF_FFFF, 40);
    }
    if f > 0 {
        cv.round_rect(x - 2, cy - 8, f.max(12) + 4, 16, 8, l.accent, 40);
        cv.round_grad(x, cy - 6, f.max(12), 12, 6, lighten(l.accent, 60), l.accent, 255);
    }
    cv.glow((x + f) as f32, cy as f32, 22.0, l.accent, 60);
    cv.circle((x + f) as f32, cy as f32 + 2.0, 13.0, 0xFF00_0000, 70);
    cv.circle((x + f) as f32, cy as f32, 12.5, 0xFFDA_DFEE, 255);
    cv.circle((x + f) as f32, cy as f32 - 1.0, 11.0, 0xFFFF_FFFF, 255);
    cv.circle((x + f) as f32, cy as f32, 4.0, l.accent, 255);
    let lw = Canvas::text_wb(label, 2, Weight::Display);
    headline(cv, rx - lw, cy - 15, label, 2, 0xFFFF_FFFF, 255);
}

// ------------------------------------------------------------------ controller prompts

fn is_pad_token(s: &str) -> bool {
    matches!(s, "A" | "B" | "X" | "Y" | "LB" | "RB" | "LT" | "RT" | "Start" | "Select" | "LB/RB")
}
/// The pad button for a keyboard key, when there is an obvious one.
fn pad_for(k: &str) -> Option<&'static str> {
    Some(match k {
        "Enter" | "Space" => "A",
        "Esc" => "B",
        "Delete" | "Backspace" | "Del" => "X",
        "Tab" => "Y",
        "Q" => "LB",
        "E" => "RB",
        "P" => "Start",
        _ => return None,
    })
}

/// One key or button glyph centred vertically on `cy`, starting at `x`; returns its width.
/// Keyboard keys are keycaps with a lip; pad face buttons are colored discs; shoulders are
/// tabs; arrows are drawn as arrows (or a d-pad on a gamepad).
pub fn keycap(cv: &mut Canvas, x: i32, cy: i32, k: &str, pad: bool) -> i32 {
    let arrow = matches!(k, "\u{2190}" | "\u{2192}" | "\u{2191}" | "\u{2193}");
    if pad && arrow {
        return dpad(cv, x, cy, k);
    }
    if pad {
        let face = match k {
            "A" => Some(0xFF3C_C46E),
            "B" => Some(0xFFEC_4A5A),
            "X" => Some(0xFF3F_86F6),
            "Y" => Some(0xFFF2_BE2E),
            _ => None,
        };
        if let Some(c) = face {
            let r = 11.5;
            let (fx, fy) = (x as f32 + r, cy as f32);
            cv.circle(fx, fy + 1.8, r, 0xFF00_0000, 90);
            cv.circle(fx, fy, r, crate::gfx::darken(c, 40), 255);
            cv.circle(fx, fy - 0.8, r - 1.4, c, 255);
            cv.ellipse(fx, fy - r * 0.45, r * 0.6, r * 0.32, 0xFFFF_FFFF, 70);
            let tw = Canvas::text_wb(k, 1, Weight::Round);
            cv.text_weight(x + r as i32 - tw / 2, cy - 9, k, 1, 0xFFFF_FFFF, 255, Weight::Round);
            return (r * 2.0) as i32;
        }
        // shoulders and menu buttons: dark rounded tabs
        let tw = Canvas::text_wb(k, 1, Weight::Round);
        let w = tw + 16;
        let shoulder = k.ends_with('B') || k.ends_with('T');
        let r = if shoulder { 7 } else { 11 };
        cv.round_rect(x, cy - 10, w, 22, r, 0xFF00_0000, 90);
        cv.round_grad(x, cy - 11, w, 21, r, 0xFF4A_5378, 0xFF262C48, 255);
        cv.rim(x, cy - 11, w, 21, r, 1.0, 0xFFFF_FFFF, 0xFF00_0000, 70);
        cv.text_weight(x + 8, cy - 9, k, 1, 0xFFFF_FFFF, 255, Weight::Round);
        return w;
    }
    let label = match k {
        "Esc" => "ESC",
        "Enter" => "ENTER",
        "Delete" | "Del" => "DEL",
        "Backspace" => "BKSP",
        "Space" => "SPACE",
        _ => k,
    };
    let tw = if arrow { 10 } else { Canvas::text_wb(label, 1, Weight::Round) };
    let w = (tw + 14).max(24);
    let h = 22;
    let y = cy - h / 2 - 1;
    cv.round_rect(x, y + 3, w, h, 6, 0xFF00_0000, 80);
    cv.round_rect(x, y + 2, w, h, 6, 0xFF8E_97B6, 255);
    cv.round_grad(x, y, w, h - 1, 6, 0xFFFF_FFFF, 0xFFDA_DFEE, 255);
    if arrow {
        let (cx, cyf, s) = ((x + w / 2) as f32, (y + h / 2) as f32 - 0.5, 4.5);
        let p = match k {
            "\u{2190}" => [(cx - s, cyf), (cx + s * 0.7, cyf - s), (cx + s * 0.7, cyf + s)],
            "\u{2192}" => [(cx + s, cyf), (cx - s * 0.7, cyf - s), (cx - s * 0.7, cyf + s)],
            "\u{2191}" => [(cx, cyf - s), (cx - s, cyf + s * 0.7), (cx + s, cyf + s * 0.7)],
            _ => [(cx, cyf + s), (cx - s, cyf - s * 0.7), (cx + s, cyf - s * 0.7)],
        };
        cv.tri(p, INK, 255);
    } else {
        cv.text_weight(x + (w - tw) / 2, y + 2, label, 1, INK, 255, Weight::Round);
    }
    w
}

/// A d-pad with the given direction (or both of its axis) lit.
fn dpad(cv: &mut Canvas, x: i32, cy: i32, k: &str) -> i32 {
    let s = 24;
    let arm = 8;
    let (cx, cyy) = (x + s / 2, cy);
    let base = 0xFF39_4064;
    cv.round_rect(cx - s / 2, cyy - arm / 2 + 1, s, arm, 2, 0xFF00_0000, 90);
    cv.round_rect(cx - s / 2, cyy - arm / 2, s, arm, 2, base, 255);
    cv.round_rect(cx - arm / 2, cyy - s / 2, arm, s, 2, base, 255);
    let lit = 0xFFFF_FFFF;
    let horiz = matches!(k, "\u{2190}" | "\u{2192}");
    if horiz {
        cv.round_rect(cx - s / 2 + 1, cyy - arm / 2 + 1, arm - 1, arm - 2, 2, lit, 255);
        cv.round_rect(cx + s / 2 - arm, cyy - arm / 2 + 1, arm - 1, arm - 2, 2, lit, 255);
    } else {
        cv.round_rect(cx - arm / 2 + 1, cyy - s / 2 + 1, arm - 2, arm - 1, 2, lit, 255);
        cv.round_rect(cx - arm / 2 + 1, cyy + s / 2 - arm, arm - 2, arm - 1, 2, lit, 255);
    }
    s
}

/// The glyphs for one prompt key string ("Enter / A", "\u{2190} \u{2192}", "Q / E"),
/// picked for the current device. Returns the width (drawing only when `draw`).
fn glyphs(cv: &mut Canvas, x: i32, cy: i32, keys: &str, pad: bool, draw: bool) -> i32 {
    let alts: Vec<&str> = keys.split(" / ").map(|a| a.trim()).collect();
    // keyboard prompts that pair keys with a slash ("Q / E") are one alternative, not two
    let pair = alts.len() == 2 && alts.iter().all(|a| a.chars().count() == 1 && !is_pad_token(a));
    let tokens: Vec<String> = if pair {
        let t: Vec<String> = alts.iter().map(|s| s.to_string()).collect();
        if pad {
            t.iter().map(|k| pad_for(k).map(|s| s.to_string()).unwrap_or(k.clone())).collect()
        } else {
            t
        }
    } else {
        let is_pad_alt = |a: &str| a.split_whitespace().all(is_pad_token);
        let chosen = if pad { alts.iter().find(|a| is_pad_alt(a)).copied() } else { alts.iter().find(|a| !is_pad_alt(a)).copied() };
        match chosen {
            Some(a) => a.split_whitespace().map(|s| s.to_string()).collect(),
            None => {
                let a = alts[0];
                a.split_whitespace().map(|k| if pad { pad_for(k).unwrap_or(k).to_string() } else { k.to_string() }).collect()
            }
        }
    };
    // on a pad, a pair of arrows is one d-pad
    let tokens: Vec<String> = if pad && tokens.len() == 2 && tokens.iter().all(|t| matches!(t.as_str(), "\u{2190}" | "\u{2192}" | "\u{2191}" | "\u{2193}")) { vec![tokens[0].clone()] } else { tokens };
    let mut w = 0;
    for (i, t) in tokens.iter().enumerate() {
        if i > 0 {
            w += 4;
        }
        if draw {
            w += keycap(cv, x + w, cy, t, pad);
        } else {
            let mut scratch = Canvas::new(1, 1);
            w += keycap(&mut scratch, 0, 0, t, pad);
        }
    }
    w
}

/// Button prompts along the bottom of a screen: pairs of (keys, action), shown as real
/// keycaps or pad buttons for whichever device was used last.
pub fn prompts(cv: &mut Canvas, l: &Look, items: &[(&str, &str)]) {
    let pad = frame().pad;
    let gap = 26;
    let widths: Vec<(i32, i32)> = items.iter().map(|(k, a)| (glyphs(cv, 0, 0, k, pad, false), Canvas::text_wb(a, 1, Weight::Bold))).collect();
    let total: i32 = widths.iter().map(|(a, b)| a + 9 + b).sum::<i32>() + gap * (items.len() as i32 - 1).max(0);
    let h = 38;
    let y = 674;
    let mut x = 640 - total / 2;
    cv.glass_ex(x - 20, y, total + 40, h, h / 2, &DEEP);
    let cy = y + h / 2;
    for (i, (k, a)) in items.iter().enumerate() {
        let (kw, aw) = widths[i];
        glyphs(cv, x, cy, k, pad, true);
        text_sw(cv, x + kw + 9, cy - 9, a, 1, l.text, 255, Weight::Bold);
        x += kw + 9 + aw + gap;
    }
}
