//! Immediate-mode UI on liquid-glass cards. Every text color here is chosen for contrast
//! against the dark glass tint, so text stays readable over any scene.

use crate::font::Weight;
use crate::gfx::Canvas;
use crate::input::Nav;
use crate::theme::Look;

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
    cv.glass(g.x - 22, g.y - 16, g.w + 44, shown as i32 * g.row_h + 32, 22, GLASS, GLASS_A);
    let sc = g.scale;
    for (vi, k) in (s.scroll..s.scroll + shown).enumerate() {
        let r = &rows[k];
        let y = g.y + vi as i32 * g.row_h;
        let sel = k == s.sel;
        let ty = y + (g.row_h - (Canvas::px(sc) * 0.74) as i32) / 2 - 1;
        match r.kind {
            RK::Head => {
                let hs = (sc - 1).max(1);
                cv.text_weight(g.x + 4, y + g.row_h - (Canvas::px(hs) * 1.25) as i32, &r.label.to_uppercase(), hs, l.accent, 255, Weight::Bold);
                cv.rect_a(g.x + 4, y + g.row_h - 4, g.w - 8, 1, l.accent, 90);
            }
            _ => {
                if sel {
                    let pulse = ((t * 4.0).sin() * 0.5 + 0.5) * 14.0;
                    cv.round_rect(g.x - 10, y + 3, g.w + 20, g.row_h - 6, (g.row_h - 6) / 2, 0xFFFF_FFFF, 30 + pulse as u32);
                    cv.round_rect(g.x - 10, y + 8, 4, g.row_h - 16, 2, l.accent, 255);
                }
                let col = if r.kind == RK::Info { l.dim } else if sel { 0xFFFF_FFFF } else { l.text };
                let label = nice(&r.label);
                let r = &Row { label, value: nice(&r.value), kind: r.kind };
                let w = if sel { Weight::Bold } else { Weight::Regular };
                cv.text_weight(g.x + 8 + 1, ty + 1, &r.label, sc, 0xFF00_0000, 120, w);
                cv.text_weight(g.x + 8, ty, &r.label, sc, col, 255, w);
                if !r.value.is_empty() {
                    let v = if r.kind == RK::Opt && sel { format!("\u{2190}  {}  \u{2192}", r.value) } else { r.value.clone() };
                    let vw = Canvas::text_wb(&v, sc, Weight::Regular);
                    let vc = if r.kind == RK::Opt { l.accent } else { l.dim };
                    cv.text_weight(g.x + g.w - 8 - vw + 1, ty + 1, &v, sc, 0xFF00_0000, 120, Weight::Regular);
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

/// A glass card.
pub fn panel(cv: &mut Canvas, x: i32, y: i32, w: i32, h: i32, _l: &Look) {
    cv.glass(x, y, w, h, 22, GLASS, GLASS_A);
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

/// A screen title spelled in the logo's letter tiles, with a line of help under it.
pub fn title(cv: &mut Canvas, text: &str, sub: &str, l: &Look) {
    let text = nice(text).to_uppercase();
    // shrink long titles to stay clear of the clock in the corner
    let mut size = 50;
    let w = crate::blocks::width(&text, size);
    if w > 1000 {
        size = (size * 1000 / w).max(30);
    }
    // each title starts on its own color, so neighbouring screens don't all open in red
    let color0 = text.bytes().fold(0usize, |a, b| a.wrapping_mul(31).wrapping_add(b as usize)) % 6;
    crate::blocks::draw(cv, &text, 52, 14 + (50 - size) / 2, size, color0, None, 255);
    if !sub.is_empty() {
        cv.text_s(62, 76, sub, 2, l.dim);
    }
}

pub fn footer(cv: &mut Canvas, text: &str, l: &Look) {
    let w = Canvas::text_w(text, 1) + 40;
    cv.glass(640 - w / 2, 680, w, 30, 15, GLASS, GLASS_A);
    cv.text_center(640, 689, text, 1, l.text);
}

pub fn cycle<T: Copy + PartialEq>(all: &[T], cur: T, d: i32) -> T {
    let i = all.iter().position(|&x| x == cur).unwrap_or(0) as i32;
    all[(i + d).rem_euclid(all.len() as i32) as usize]
}

// ------------------------------------------------------------------ shared widgets

/// A row of tabs (glass pills); returns the tab the mouse clicked, if any.
pub fn tabs(cv: &mut Canvas, l: &Look, names: &[&str], active: usize, y: i32, mouse: Option<(i32, i32)>, click: bool) -> Option<usize> {
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
            cv.round_grad(x, y, w, h, h / 2, crate::gfx::lighten(l.accent, 40), l.accent, 255);
            cv.text_weight(x + pad, y + 11, n, 2, 0xFF14_1022, 255, Weight::Bold);
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
    cv.round_rect(x, cy - h / 2, w, h, h / 2, if on { l.accent } else { 0xFF2A_3150 }, 255);
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
        cv.round_rect(x, cy - 18, w, 36, 18, l.accent, 40);
    }
    let ac = if sel { l.accent } else { l.dim };
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
    cv.round_rect(x, cy - 5, f.max(10), 10, 5, l.accent, 255);
    cv.circle((x + f) as f32, cy as f32 + 1.5, 11.5, 0xFF00_0000, 70);
    cv.circle((x + f) as f32, cy as f32, 11.0, 0xFFF4_F6FF, 255);
    let lw = Canvas::text_wb(label, 2, Weight::Bold);
    cv.text_weight(rx - lw, cy - 12, label, 2, 0xFFFF_FFFF, 255, Weight::Bold);
}

/// Button prompts along the bottom of a screen: pairs of (key, action).
pub fn prompts(cv: &mut Canvas, l: &Look, items: &[(&str, &str)]) {
    let gap = 34;
    let widths: Vec<(i32, i32)> = items.iter().map(|(k, a)| (Canvas::text_wb(k, 1, Weight::Bold) + 16, Canvas::text_w(a, 1))).collect();
    let total: i32 = widths.iter().map(|(a, b)| a + 8 + b).sum::<i32>() + gap * (items.len() as i32 - 1).max(0);
    let mut x = 640 - total / 2;
    cv.glass(x - 22, 676, total + 44, 34, 17, GLASS, GLASS_A);
    for (i, (k, a)) in items.iter().enumerate() {
        let (kw, aw) = widths[i];
        cv.round_rect(x, 683, kw, 20, 6, 0xFFE8_ECF8, 235);
        cv.text_weight(x + 8, 685, k, 1, 0xFF14_1830, 255, Weight::Bold);
        cv.text_s(x + kw + 8, 685, a, 1, l.text);
        x += kw + 8 + aw + gap;
    }
}
