//! The main menu: a bento grid of big, colorful mode tiles (each with its own color, halftone,
//! a framed scene with one of the cast, a blob mascot and a chunky cloud label), a slanted
//! banner, a description strip, and a pop-up card for modes that have several entries.
//!
//! Tiles are painted once per screen width into images (two each: resting and focused) and
//! then only blitted, so the menu stays cheap on the software renderer.

use crate::app::{App, Screen};
use crate::font::Weight;
use crate::gfx::{darken, lighten, mix, Canvas};
use crate::input::Nav;
use crate::theme::{look, CellKind, SpriteKey};
use crate::ui::{self, ListGeom, ListState, LE};

pub struct Tile {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub color: u32,
    pub label: &'static str,
    pub icon: &'static str,
    /// cast member and expression in the tile's scene
    pub who: &'static str,
    pub expr: &'static str,
    /// world map behind them (big tiles)
    pub world: usize,
    /// blob mascot color (a base palette color)
    pub blob: u32,
}

/// Story, Versus, Solo, Training, Replays, Profile, Ranks, Settings (the order of `xmb_items`).
pub const TILES: [Tile; 8] = [
    Tile { x: 40, y: 112, w: 440, h: 230, color: 0xFF2E_B24E, label: "Story", icon: "map-trifold", who: "mira", expr: "delighted", world: 0, blob: crate::theme::GREEN },
    Tile { x: 494, y: 112, w: 380, h: 230, color: 0xFFE2_3D63, label: "Versus", icon: "sword", who: "kaede", expr: "smug", world: 4, blob: crate::theme::RED },
    Tile { x: 888, y: 112, w: 352, h: 230, color: 0xFFF2_9A1E, label: "Solo", icon: "timer", who: "nami", expr: "smile", world: 1, blob: crate::theme::YELLOW },
    Tile { x: 40, y: 362, w: 300, h: 200, color: 0xFF2F_6BE0, label: "Training", icon: "graduation-cap", who: "yuki", expr: "smile", world: 2, blob: crate::theme::BLUE },
    Tile { x: 354, y: 362, w: 214, h: 200, color: 0xFF8A_4BE0, label: "Replays", icon: "film-strip", who: "rin", expr: "smug", world: 3, blob: crate::theme::PURPLE },
    Tile { x: 582, y: 362, w: 206, h: 200, color: 0xFF17_A9C0, label: "Profile", icon: "identification-card", who: "hana", expr: "delighted", world: 1, blob: crate::theme::CYAN },
    Tile { x: 802, y: 362, w: 206, h: 200, color: 0xFFE0_A818, label: "Ranks", icon: "trophy", who: "emi", expr: "delighted", world: 0, blob: crate::theme::ORANGE },
    Tile { x: 1022, y: 362, w: 218, h: 200, color: 0xFF5A_5FB0, label: "Settings", icon: "gear-six", who: "kai", expr: "smile", world: 2, blob: crate::theme::PINK },
];

/// Margin around a tile image (room for the focus ring and the shear).
const M: i32 = 18;
/// Vertical shear: the right side of a tile sits higher than its left.
const SHEAR: f32 = 0.035;

/// The cached tile images for one screen width: (resting, focused) per tile.
pub struct TileArt {
    pub width: i32,
    pub tiles: Vec<(Canvas, Canvas)>,
    /// the page banner, painted once (layout origin at 0, 0)
    pub banner: Canvas,
}

/// A white cloud bubble with a dark outline, sized around a `w` x `h` label at (x, y).
fn cloud(cv: &mut Canvas, x: i32, y: i32, w: i32, h: i32, edge: u32) {
    let n = ((w / (h / 2).max(1)) + 2).max(3);
    let pass = |cv: &mut Canvas, grow: f32, c: u32| {
        let r = h as f32 * 0.55 + grow;
        cv.round_rect(x - grow as i32 - h / 4, y + h / 6 - grow as i32, w + h / 2 + 2 * grow as i32, h * 2 / 3 + 2 * grow as i32, h / 3, c, 255);
        for i in 0..n {
            let fx = x as f32 + w as f32 * i as f32 / (n - 1) as f32;
            let fy = y as f32 + h as f32 * 0.5 + if i % 2 == 0 { -h as f32 * 0.12 } else { h as f32 * 0.14 };
            let rr = r * if i == 0 || i == n - 1 { 0.78 } else { 1.0 };
            cv.circle(fx, fy, rr, c, 255);
        }
    };
    pass(cv, 4.0, edge);
    pass(cv, 0.0, 0xFFFF_FFFF);
}

/// Chunky display text: a thick dark outline, then the fill, with a light top highlight.
pub fn chunky(cv: &mut Canvas, x: i32, y: i32, s: &str, scale: i32, fill: u32, edge: u32) {
    let o = (scale / 2 + 1).max(2);
    for (dx, dy) in [(-o, 0), (o, 0), (0, -o), (0, o), (-o, -o), (o, o), (-o, o), (o, -o), (0, o + 2), (o, o + 2), (-o, o + 2)] {
        cv.text_weight(x + dx, y + dy, s, scale, edge, 255, Weight::Display);
    }
    cv.text_weight(x, y, s, scale, fill, 255, Weight::Display);
    cv.set_clip(0, 0, crate::gfx::LW, y + (Canvas::px(scale) * 0.42) as i32);
    cv.text_weight(x, y, s, scale, lighten(fill, 70), 120, Weight::Display);
    cv.reset_clip();
}

/// Shear a tile image vertically (columns slide up toward the right), with filtering.
fn shear(src: &Canvas) -> Canvas {
    let mut out = Canvas::new(src.w, src.h);
    out.k = src.k;
    let cx = src.w as f32 / 2.0;
    for x in 0..src.w {
        let off = (x as f32 - cx) * SHEAR;
        let fo = off.floor();
        let t = ((off - fo) * 256.0) as u32;
        let io = fo as i32;
        for y in 0..src.h {
            // out(x, y) = src(x, y + off)
            let sy = y + io;
            let a = if sy >= 0 && sy < src.h { src.px[(sy * src.w + x) as usize] } else { 0 };
            let b = if sy + 1 >= 0 && sy + 1 < src.h { src.px[((sy + 1) * src.w + x) as usize] } else { 0 };
            out.px[(y * src.w + x) as usize] = lerp_argb(a, b, t);
        }
    }
    out
}
fn lerp_argb(a: u32, b: u32, t: u32) -> u32 {
    let it = 256 - t;
    let f = |s: u32| -> u32 { (((a >> s) & 255) * it + ((b >> s) & 255) * t) >> 8 };
    (f(24) << 24) | (f(16) << 16) | (f(8) << 8) | f(0)
}

impl App {
    /// Paint (or fetch) the tile images for the current screen width.
    fn tile_art(&mut self) -> &TileArt {
        let width = self.cv.w;
        if self.menu_art.as_ref().map(|a| a.width != width).unwrap_or(true) {
            let k = self.cv.k;
            let mut sprites = crate::theme::Sprites::new();
            sprites.set_scale(k);
            let mut tiles = Vec::new();
            for (i, t) in TILES.iter().enumerate() {
                let a = self.paint_tile(i, t, k, false, &mut sprites);
                let b = self.paint_tile(i, t, k, true, &mut sprites);
                tiles.push((a, b));
            }
            let mut b = Canvas::new((560.0 * k).ceil() as i32, (150.0 * k).ceil() as i32);
            b.k = k;
            banner(&mut b, "Main Menu");
            self.menu_art = Some(TileArt { width, tiles, banner: b });
        }
        self.menu_art.as_ref().unwrap()
    }

    fn paint_tile(&self, _i: usize, t: &Tile, k: f32, focus: bool, sprites: &mut crate::theme::Sprites) -> Canvas {
        let (pw, ph) = (((t.w + 2 * M) as f32 * k).ceil() as i32, ((t.h + 2 * M) as f32 * k).ceil() as i32);
        let mut cv = Canvas::new(pw, ph);
        cv.k = k;
        let (w, h) = (t.w, t.h);
        let r = 16;
        let col = if focus { lighten(t.color, 18) } else { t.color };
        let ink = darken(t.color, 150);
        // frame: focus ring, dark outline, white rim, the colored body
        if focus {
            cv.round_rect(M - 12, M - 12, w + 24, h + 24, r + 12, 0xFFFF_E066, 70);
            cv.round_rect(M - 8, M - 8, w + 16, h + 16, r + 8, 0xFFFF_E066, 255);
        }
        cv.round_rect(M - 4, M - 4, w + 8, h + 8, r + 4, ink, 255);
        cv.round_rect(M - 1, M - 1, w + 2, h + 2, r + 1, 0xFFFF_FFFF, 255);
        cv.round_grad(M + 5, M + 5, w - 10, h - 10, r - 4, lighten(col, 34), darken(col, 34), 255);
        // halftone: dots that swell toward the bottom right
        cv.set_clip(M + 9, M + 9, w - 18, h - 18);
        let step = 13;
        let dot = lighten(col, 90);
        let mut row = 0;
        let mut yy = M + 6;
        while yy < M + h {
            let mut xx = M + 6 + if row % 2 == 1 { step / 2 } else { 0 };
            while xx < M + w {
                let f = ((xx - M) as f32 / w as f32 * 0.45 + (yy - M) as f32 / h as f32 * 0.55).clamp(0.0, 1.0);
                cv.circle(xx as f32, yy as f32, 1.0 + 3.4 * f * f, dot, 70);
                xx += step;
            }
            yy += step;
            row += 1;
        }
        cv.reset_clip();
        let big = w >= 300;
        // the scene window: a world, one of the cast in it (right of a big tile, across the
        // top of a small one)
        {
            let (wx, wy, ww, wh) = if big { (M + w * 38 / 100, M + 16, w * 57 / 100, h * 58 / 100) } else { (M + 16, M + 16, w - 32, h * 50 / 100) };
            cv.round_rect(wx - 4, wy - 4, ww + 8, wh + 8, 12, ink, 255);
            cv.round_rect(wx - 2, wy - 2, ww + 4, wh + 4, 11, 0xFFFF_FFFF, 255);
            if !self.maps.is_empty() {
                let m = &self.maps[t.world % self.maps.len()].image;
                let img = crate::assets::resample(m, cv.us(ww), cv.us(wh));
                cv.blit(&img, wx, wy, 255);
            }
            cv.rect_a(wx, wy, ww, wh, col, 50);
            cv.set_clip(wx, wy, ww, wh);
            if t.label == "Versus" {
                crate::assets::draw_char_ex(&mut cv, "kaede", "smug", wx + ww * 28 / 100, wy + wh + wh * 3 / 4, wh * 2, 255, false);
                crate::assets::draw_char_ex(&mut cv, "kai", "smile", wx + ww * 74 / 100, wy + wh + wh * 3 / 4, wh * 2, 255, true);
                // the spark between them
                cv.glow((wx + ww / 2) as f32, (wy + wh / 2) as f32, 30.0, 0xFFFF_F2A0, 160);
            } else {
                crate::assets::draw_char_ex(&mut cv, t.who, t.expr, wx + ww * 62 / 100, wy + wh + wh * 3 / 4, wh * 2, 255, false);
            }
            cv.reset_clip();
        }
        if big {
            // a piece motif left of the window, and the mode's icon under it
            let c = [crate::theme::YELLOW, crate::theme::CYAN, crate::theme::PURPLE, crate::theme::ORANGE][_i % 4];
            let cs = 24;
            let img = sprites.get(SpriteKey { kind: CellKind::Gem, color: c, size: cs, face: false, pattern: false, flat: false, frame: 0 }).clone();
            let shape: [(i32, i32); 4] = [[(0, 0), (1, 0), (0, 1), (1, 1)], [(0, 0), (0, 1), (1, 1), (2, 1)], [(1, 0), (0, 1), (1, 1), (2, 1)], [(0, 0), (1, 0), (1, 1), (2, 1)]][_i % 4];
            for (dx, dy) in shape {
                cv.blit(&img, M + 22 + dx * cs, M + 20 + dy * cs, 240);
            }
        } else {
            // the mode's icon in a badge on the window's corner
            let (bx, by) = (M + w - 34, M + 20 + h * 50 / 100 - 8);
            cv.circle(bx as f32, by as f32, 21.0, ink, 255);
            cv.circle(bx as f32, by as f32, 18.5, 0xFFFF_FFFF, 255);
            cv.circle(bx as f32, by as f32, 16.0, col, 255);
            crate::assets::icon(&mut cv, t.icon, bx, by, 20, 0xFFFF_FFFF, 255);
        }
        // the blob mascot, bottom left, spilling over the frame
        let bs = if big { 74 } else { 58 };
        let blob = sprites.get(SpriteKey { kind: CellKind::Blob, color: t.blob, size: bs, face: true, pattern: false, flat: false, frame: 0 }).clone();
        cv.ellipse((M + bs / 2 + 2) as f32, (M + h - 2) as f32, bs as f32 * 0.42, 6.0, 0xFF00_0000, 70);
        cv.blit(&blob, M - 4, M + h - bs + 2, 255);
        // the cloud label
        let room = if big { w * 60 / 100 } else { w - 70 };
        let mut sc = if w >= 350 { 5 } else { 4 };
        while sc > 2 && Canvas::text_wb(t.label, sc, Weight::Display) > room {
            sc -= 1;
        }
        let lw = Canvas::text_wb(t.label, sc, Weight::Display);
        let lh = (Canvas::px(sc) * 1.0) as i32;
        let (lx, ly) = if big { (M + w * 30 / 100, M + h - lh - 26) } else { (M + (w - lw) / 2 + 16, M + h - lh - 20) };
        let lx = lx.min(M + w - lw - 26);
        cloud(&mut cv, lx, ly, lw, lh, ink);
        chunky(&mut cv, lx, ly - (lh / 10), t.label, sc, t.color, ink);
        shear(&cv)
    }

    pub(crate) fn main_menu(&mut self, nav: &Nav, _s: ListState) -> Screen {
        let l = look(self.st.theme);
        let t = self.t;
        self.bg_blur(36);
        let n = TILES.len();
        let mut cat = self.xmb_cat.min(n - 1);
        let items = self.xmb_items(cat);
        let mut go: Option<(usize, usize)> = None;
        let mut moved = false;
        // ---- input
        if !self.menu_open {
            let pick = |cat: usize, dx: i32, dy: i32| -> usize {
                let c = &TILES[cat];
                let (cx, cy) = (c.x + c.w / 2, c.y + c.h / 2);
                let mut best = (i32::MAX, cat);
                for (i, o) in TILES.iter().enumerate() {
                    let (ox, oy) = (o.x + o.w / 2, o.y + o.h / 2);
                    let (ddx, ddy) = (ox - cx, oy - cy);
                    let along = ddx * dx + ddy * dy;
                    if i == cat || along <= 0 {
                        continue;
                    }
                    let across = (ddx * dy).abs() + (ddy * dx).abs();
                    let score = along + across * 3;
                    if score < best.0 {
                        best = (score, i);
                    }
                }
                best.1
            };
            let before = cat;
            if nav.left {
                cat = pick(cat, -1, 0);
            }
            if nav.right {
                cat = pick(cat, 1, 0);
            }
            if nav.up {
                cat = pick(cat, 0, -1);
            }
            if nav.down {
                cat = pick(cat, 0, 1);
            }
            let mut clicked = false;
            if let Some((mx, my)) = nav.mouse {
                if let Some(i) = TILES.iter().position(|o| mx >= o.x && mx < o.x + o.w && my >= o.y && my < o.y + o.h) {
                    if nav.click {
                        clicked = i == cat;
                        cat = i;
                    }
                }
            }
            if cat != before {
                moved = true;
                self.menu_pop_t = t;
            }
            if nav.ok || clicked {
                let items = self.xmb_items(cat);
                if items.len() <= 1 {
                    go = Some((cat, 0));
                } else {
                    self.menu_open = true;
                    self.menu_open_t = t;
                    self.menu_ls = ListState::at(self.xmb_item[cat].min(items.len() - 1));
                    self.sfx(crate::audio::Sfx::Select);
                }
            } else if nav.back {
                self.sfx(crate::audio::Sfx::Back);
                return Screen::Title;
            }
        }
        self.xmb_cat = cat;
        if moved {
            self.sfx(crate::audio::Sfx::Menu);
        }
        // the player chip
        {
            let who = self.card.name.clone();
            let lvl = format!("Lv {}  {}", self.ranks.level, crate::ranks::title(self.ranks.level));
            let ww = Canvas::text_wb(&who, 2, Weight::Bold).max(Canvas::text_wb(&lvl, 1, Weight::Round)) + 78;
            self.cv.glass_ex(1130 - ww, 10, ww, 48, 24, &ui::DEEP);
            self.cv.circle((1130 - ww + 26) as f32, 34.0, 17.0, l.accent, 255);
            self.cv.circle((1130 - ww + 26) as f32, 34.0, 15.0, 0xFF1A_2048, 255);
            crate::assets::icon(&mut self.cv, "user", 1130 - ww + 26, 34, 20, 0xFFFF_FFFF, 255);
            self.cv.text_weight(1130 - ww + 52, 14, &who, 2, 0xFFFF_FFFF, 255, Weight::Bold);
            self.cv.text_weight(1130 - ww + 53, 38, &lvl, 1, l.accent, 255, Weight::Round);
        }
        // ---- tiles (focused last, popped up a little)
        let motion = ui::frame().motion;
        let pop = if motion { ((t - self.menu_pop_t) / 0.18).clamp(0.0, 1.0) } else { 1.0 };
        let pop = 1.0 - (1.0 - pop) * (1.0 - pop);
        let bob = if motion { ((t * 3.0).sin() * 1.5) as i32 } else { 0 };
        self.tile_art();
        let art = self.menu_art.take().unwrap();
        self.cv.blit(&art.banner, 0, 0, 255);
        for i in (0..n).filter(|&i| i != cat).chain(std::iter::once(cat)) {
            let tl = &TILES[i];
            let (img, dy) = if i == cat { (&art.tiles[i].1, -(6.0 * pop) as i32 + bob) } else { (&art.tiles[i].0, 0) };
            if i == cat {
                self.cv.soft_shadow(tl.x + 6, tl.y + 14, tl.w - 12, tl.h - 10, 18, 18, 110);
            }
            self.cv.blit(img, tl.x - M, tl.y - M + dy, 255);
        }
        self.menu_art = Some(art);
        // ---- the description strip
        let open_items = if self.menu_open { Some(self.xmb_items(cat)) } else { None };
        let (dic, dlabel, ddesc) = match &open_items {
            Some(v) => {
                let s = self.menu_ls.sel.min(v.len().saturating_sub(1));
                (v[s].0, v[s].1.clone(), v[s].2.clone())
            }
            None => (TILES[cat].icon, TILES[cat].label.to_string(), items.first().map(|x| x.2.clone()).unwrap_or_default()),
        };
        let _ = dic;
        strip(&mut self.cv, &l, TILES[cat].color, &dlabel, &ddesc);
        // ---- the pop-up card for modes with several entries
        if let Some(v) = open_items {
            let e = if motion { ((t - self.menu_open_t) / 0.2).clamp(0.0, 1.0) } else { 1.0 };
            let e = 1.0 - (1.0 - e).powi(3);
            self.cv.rect_a(0, 0, 1280, 720, 0xFF05_0414, (150.0 * e) as u32);
            let tl = &TILES[cat];
            let rows: Vec<ui::Row> = v.iter().map(|(ic, lbl, _)| ui::Row::act(lbl.clone()).ic(ic)).collect();
            let vis = rows.len().min(7);
            let ch = 118 + vis as i32 * 48 + 26;
            let (cw, cx) = (600, 340);
            let cy = 320 - ch / 2 + 40 + ((1.0 - e) * 40.0) as i32;
            let ink = darken(tl.color, 150);
            self.cv.soft_shadow(cx, cy, cw, ch, 26, 22, 140);
            self.cv.round_rect(cx - 4, cy - 4, cw + 8, ch + 8, 30, ink, 255);
            self.cv.round_rect(cx - 1, cy - 1, cw + 2, ch + 2, 27, 0xFFFF_FFFF, 255);
            self.cv.round_grad(cx + 5, cy + 5, cw - 10, 92, 22, lighten(tl.color, 30), tl.color, 255);
            self.cv.glass_ex(cx + 5, cy + 90, cw - 10, ch - 95, 22, &ui::DEEP);
            ui::icon_tile(&mut self.cv, &l, tl.icon, cx + 26, cy + 22, 54);
            chunky(&mut self.cv, cx + 98, cy + 22, tl.label, 4, 0xFFFF_FFFF, ink);
            let gg = ListGeom { x: cx + 40, y: cy + 112, w: cw - 80, row_h: 48, visible: vis, scale: 2 };
            let mut ls = self.menu_ls;
            let ev = ui::list_bare(&mut self.cv, &l, nav, &rows, &mut ls, &gg, t);
            self.menu_ls = ls;
            match ev {
                LE::Act(k) => {
                    self.xmb_item[cat] = k;
                    go = Some((cat, k));
                }
                LE::Back => {
                    self.menu_open = false;
                    self.xmb_item[cat] = self.menu_ls.sel;
                    self.sfx(crate::audio::Sfx::Back);
                }
                LE::Moved => self.sfx(crate::audio::Sfx::Menu),
                _ => {}
            }
            ui::prompts(&mut self.cv, &l, &[("\u{2191} \u{2193}", "Choose"), ("Enter / A", "OK"), ("Esc / B", "Back")]);
        } else {
            ui::prompts(&mut self.cv, &l, &[("\u{2190} \u{2192}", "Mode"), ("\u{2191} \u{2193}", "Row"), ("Enter / A", "OK"), ("Esc / B", "Title")]);
        }
        if let Some((c, s)) = go {
            self.menu_open = false;
            self.sfx(crate::audio::Sfx::Select);
            return self.menu_go(c, s);
        }
        Screen::Main(ListState::default())
    }
}

/// The slanted page banner: a starburst, a ribbon with halftone, chunky yellow lettering.
fn banner(cv: &mut Canvas, text: &str) {
    let ink = 0xFF3B_1030;
    // starburst behind the left end
    let (sx, sy) = (70.0f32, 66.0f32);
    let spin = 0.3;
    let pts = 14;
    for i in 0..pts {
        let a0 = spin + i as f32 / pts as f32 * std::f32::consts::TAU;
        let a1 = spin + (i as f32 + 0.5) / pts as f32 * std::f32::consts::TAU;
        let a2 = spin + (i as f32 + 1.0) / pts as f32 * std::f32::consts::TAU;
        let (ro, ri) = (66.0, 40.0);
        cv.tri([(sx + a0.cos() * ri, sy + a0.sin() * ri), (sx + a1.cos() * ro, sy + a1.sin() * ro), (sx + a2.cos() * ri, sy + a2.sin() * ri)], 0xFFFF_7A2E, 255);
    }
    cv.circle(sx, sy, 42.0, 0xFFFF_7A2E, 255);
    // the ribbon: a parallelogram with a notched tail
    let tw = Canvas::text_wb(text, 4, Weight::Display);
    let (x0, x1, y0, y1) = (-20.0f32, 120.0 + tw as f32 + 70.0, 38.0f32, 96.0f32);
    let sk = 10.0;
    let quad = |cv: &mut Canvas, g: f32, c: u32| {
        let p = [(x0 - g, y0 - g + sk), (x1 + g, y0 - g), (x1 - 22.0 + g, (y0 + y1) / 2.0 - sk / 2.0), (x1 + g, y1 + g - sk), (x0 - g, y1 + g)];
        cv.tri([p[0], p[1], p[2]], c, 255);
        cv.tri([p[0], p[2], p[4]], c, 255);
        cv.tri([p[2], p[3], p[4]], c, 255);
    };
    quad(cv, 4.0, ink);
    quad(cv, 1.5, 0xFFFF_FFFF);
    quad(cv, -2.0, 0xFFE8_3A4E);
    cv.set_clip(0, 36, x1 as i32, 54);
    let mut yy = 38;
    let mut row = 0;
    while yy < 92 {
        let mut xx = if row % 2 == 1 { 5 } else { 0 };
        while (xx as f32) < x1 {
            cv.circle(xx as f32, yy as f32, 1.0 + (xx as f32 / x1) * 2.2, 0xFFFF_8A9A, 90);
            xx += 10;
        }
        yy += 10;
        row += 1;
    }
    cv.reset_clip();
    chunky(cv, 112, 42, text, 4, 0xFFFF_E066, ink);
}

/// The description strip: a white card with a dashed border and the mode color's tab.
fn strip(cv: &mut Canvas, _l: &crate::theme::Look, color: u32, label: &str, desc: &str) {
    let (x, y, w, h) = (40, 578, 1200, 78);
    cv.soft_shadow(x, y, w, h, 18, 12, 90);
    cv.round_rect(x - 3, y - 3, w + 6, h + 6, 21, darken(color, 150), 255);
    cv.round_grad(x, y, w, h, 18, 0xFFFF_FFFF, 0xFFEE_F0F8, 255);
    // dashed inner border
    let (ix, iy, iw, ih) = (x + 172, y + 9, w - 182, h - 18);
    let dash = 14;
    let mut dx = ix;
    while dx < ix + iw {
        cv.rect_a(dx, iy, 8.min(ix + iw - dx), 2, 0xFF9A_A0B8, 200);
        cv.rect_a(dx, iy + ih - 2, 8.min(ix + iw - dx), 2, 0xFF9A_A0B8, 200);
        dx += dash;
    }
    // the colored tab with the label
    cv.round_grad(x + 6, y + 6, 156, h - 12, 14, lighten(color, 30), color, 255);
    let ink = darken(color, 150);
    let lw = Canvas::text_wb(label, 2, Weight::Display);
    let sc = if lw > 136 { 1 } else { 2 };
    let lw = Canvas::text_wb(label, sc, Weight::Display);
    chunky(cv, x + 6 + (156 - lw) / 2, y + h / 2 - (Canvas::px(sc) * 0.5) as i32 - 2, label, sc, 0xFFFF_FFFF, ink);
    let lines = crate::gfx::wrap(desc, iw - 40, 2);
    let ty = y + h / 2 - lines.len().min(2) as i32 * 13 - 1;
    for (i, ln) in lines.iter().take(2).enumerate() {
        cv.text_weight(ix + 20, ty + i as i32 * 26, ln, 2, ui::INK, 255, Weight::Bold);
    }
    let _ = mix;
}
