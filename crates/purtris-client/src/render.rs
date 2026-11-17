//! Match rendering: Voyage scenes, boards for all four styles, HUD, particles, callouts.
//! The renderer only reads the last committed sim state. It never feeds anything back.

use crate::font::Weight;
use crate::gfx::{darken, hex, lighten, mix, Canvas};
use crate::fx::Fx;
use crate::save::Settings;
use crate::theme::*;
use purtris_sim::capsule as cap;
use purtris_sim::chain as ch;
use purtris_sim::panel as pn;
use purtris_sim::quad as qd;
use purtris_sim::{Board, Match, Outcome, SeatKind, Style};

pub const W: i32 = 1280;
pub const H: i32 = 720;

#[derive(Clone, Copy, Default)]
#[allow(dead_code)] // x/y/w/h: the seat's whole slot, kept for overlays
pub struct Geom {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub cell: i32,
    pub fx: i32,
    pub fy: i32,
    pub cols: i32,
    pub rows: i32,
}

pub struct Scene {
    bg: Canvas,
    built_for: (usize, u8, u8, i32),
    stars: Vec<(f32, f32, f32, f32)>,
    pub index: usize,
}

pub const SCENE_NAMES: [&str; 5] = ["Tidepool Nebula", "Aurora Steppe", "Glass Desert Night", "Lantern Harbor", "Deep Orbit"];
const SCENE_COLORS: [(u32, u32, u32); 5] = [
    (0x0A1640, 0x2B0F45, 0x2EC5D8),
    (0x061A2A, 0x0F3B3A, 0x6CF0A8),
    (0x1A0E2E, 0x4A1E3A, 0xFFB86B),
    (0x10102A, 0x3A1A1A, 0xFF7A5C),
    (0x02030A, 0x101040, 0x9C7AFF),
];

impl Scene {
    pub fn new() -> Self {
        let mut stars = Vec::new();
        let mut s = 0x9E37_79B9u32;
        for _ in 0..170 {
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            let x = (s >> 8) as f32 / 16777216.0 * W as f32;
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            let y = (s >> 8) as f32 / 16777216.0 * H as f32;
            s = s.wrapping_mul(1664525).wrapping_add(1013904223);
            let z = 0.2 + (s >> 8) as f32 / 16777216.0;
            stars.push((x, y, z, ((s >> 4) & 0xFF) as f32 / 255.0));
        }
        Scene { bg: Canvas::screen(W), built_for: (999, 9, 9, 0), stars, index: 0 }
    }

    fn build(&mut self, theme: ThemeId, preset: Preset, width: i32) {
        let key = (self.index, theme as u8, preset as u8, width);
        if self.built_for == key {
            return;
        }
        self.built_for = key;
        if self.bg.w != width {
            self.bg = Canvas::screen(width);
        }
        let l = look(theme);
        let cv = &mut self.bg;
        match theme {
            ThemeId::Tournament => cv.clear(l.bg_top),
            ThemeId::Phosphor => {
                cv.vgrad(0, 0, W, H, l.bg_top, l.bg_bot);
                for i in 0..40 {
                    cv.rect_a(0, i * 18, W, 1, hex(0x0E3018), 120);
                }
            }
            ThemeId::Voyage => {
                let (a, b, c) = SCENE_COLORS[self.index % 5];
                cv.vgrad(0, 0, W, H, hex(a), hex(b));
                if preset != Preset::Potato {
                    // nebula blobs and a distant planet arc, all procedural
                    let mut s = 1234 + self.index as u32 * 77;
                    for _ in 0..14 {
                        s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                        let x = (s >> 20) as f32 / 4096.0 * W as f32;
                        s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                        let y = (s >> 20) as f32 / 4096.0 * H as f32;
                        s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                        let r = 120.0 + (s >> 24) as f32;
                        cv.glow(x, y, r, mix(hex(c), hex(b), (s >> 3) & 0x7F), 46);
                    }
                    let px = W as f32 * 0.82;
                    let py = H as f32 * 1.05;
                    cv.glow(px, py, 420.0, hex(c), 40);
                    cv.circle(px, py, 300.0, darken(hex(b), 60), 200);
                    cv.ring(px, py, 300.0, 3.0, lighten(hex(c), 80), 120);
                    for k in 0..3 {
                        cv.ring(W as f32 * 0.18, H as f32 * 0.2, 60.0 + k as f32 * 26.0, 1.0, hex(c), 40);
                    }
                }
            }
        }
    }

    pub fn draw(&mut self, cv: &mut Canvas, st: &Settings, t: f32, energy: f32) {
        self.build(st.theme, st.preset, cv.w);
        cv.copy_from(&self.bg);
        if st.theme == ThemeId::Voyage && st.preset != Preset::Potato {
            let moving = st.preset == Preset::Voyage && !st.reduce_motion;
            let (_, _, c) = SCENE_COLORS[self.index % 5];
            for &(x, y, z, tw) in &self.stars {
                let dx = if moving { (t * 14.0 * z) % W as f32 } else { 0.0 };
                let sx = (x - dx).rem_euclid(W as f32);
                let tw2 = 0.6 + 0.4 * ((t * (1.0 + tw * 3.0) + tw * 10.0).sin());
                let a = (120.0 * z * tw2) as u32 + 40;
                cv.rect_a(sx as i32, y as i32, 2, 2, 0xFFFF_FFFF, a.min(255));
            }
            if moving && energy > 0.01 {
                cv.glow(W as f32 / 2.0, H as f32 * 0.55, 500.0, hex(c), (energy * 50.0) as u32);
            }
        }
    }
}

/// Board area rectangles for the active seats.
pub fn layout(m: &Match, humans: &[bool; 6], area: (i32, i32, i32, i32)) -> [Geom; 6] {
    let mut out = [Geom::default(); 6];
    let active: Vec<usize> = (0..6).filter(|&i| m.seats[i].board.is_some()).collect();
    let n = active.len();
    let (ax, ay, aw, ah) = area;
    let mut rects: Vec<(usize, (i32, i32, i32, i32))> = Vec::new();
    let nh = active.iter().filter(|&&i| humans[i]).count();
    if n <= 4 {
        // Width follows what each style needs, so cells come out about the same size.
        let need = |i: usize| match m.seats[i].cfg.profile.style() {
            Style::Quad => 19.2,
            Style::Chain => 10.4,
            Style::Capsule => 12.2,
            Style::Panel => 10.2,
        };
        let weights: Vec<f32> = active.iter().map(|&i| need(i) * if humans[i] && n > 2 { 1.35 } else { 1.0 }).collect();
        let tot: f32 = weights.iter().sum();
        let mut x = ax as f32;
        for (k, &i) in active.iter().enumerate() {
            let w = aw as f32 * weights[k] / tot;
            rects.push((i, (x as i32, ay, w as i32, ah)));
            x += w;
        }
    } else if nh == 1 {
        let hi = *active.iter().find(|&&i| humans[i]).unwrap();
        let lw = aw * 2 / 5;
        rects.push((hi, (ax, ay, lw, ah)));
        let others: Vec<usize> = active.iter().copied().filter(|&i| i != hi).collect();
        let cols = 3;
        let rows = (others.len() as i32 + cols - 1) / cols;
        let cw = (aw - lw) / cols;
        let chh = ah / rows.max(1);
        for (k, &i) in others.iter().enumerate() {
            let c = k as i32 % cols;
            let r = k as i32 / cols;
            rects.push((i, (ax + lw + c * cw, ay + r * chh, cw, chh)));
        }
    } else {
        let cols = 3;
        let cw = aw / cols;
        let chh = ah / 2;
        let mut order = active.clone();
        order.sort_by_key(|&i| !humans[i]);
        for (k, &i) in order.iter().enumerate() {
            let c = k as i32 % cols;
            let r = k as i32 / cols;
            rects.push((i, (ax + c * cw, ay + r * chh, cw, chh)));
        }
    }
    for (i, (x, y, w, h)) in rects {
        let style = m.seats[i].cfg.profile.style();
        let (cols, rows, side_l, side_r) = match style {
            Style::Quad => (10, 20, 4.6, 4.6),
            Style::Chain => (6, 12, 1.2, 3.2),
            Style::Capsule => (8, 16, 1.0, 3.2),
            Style::Panel => (6, 12, 1.2, 3.0),
        };
        let tot_c = cols as f32 + side_l + side_r;
        // vertical budget in pixels: nameplate + gap + cabinet rim above, rim + stat line below
        // (sizes match draw_seat); Quad also keeps two spawn rows above the well
        let spawn = if style == Style::Quad { 2.0 } else { 0.0 };
        let fit = |top: i32, bot: i32| ((w as f32 * 0.96 / tot_c).min((h - top - bot) as f32 / (rows as f32 + spawn))).floor().max(4.0) as i32;
        let (mut top, mut bot) = (50 + 12 + 8, 8 + 10 + 26);
        let mut cell = fit(top, bot);
        if cols * cell < 200 {
            top = 34 + 12 + 8;
            bot = 8 + 10 + 16;
            cell = fit(top, bot);
        }
        let fw = cols * cell;
        let fh = rows * cell;
        let block_w = (tot_c * cell as f32) as i32;
        let bx = x + (w - block_w) / 2;
        let fx = bx + (side_l * cell as f32) as i32;
        let used = top + bot + fh + (spawn * cell as f32) as i32;
        let fy = y + (h - used) / 2 + top + (spawn * cell as f32) as i32 - if style == Style::Quad { 4 } else { 0 };
        out[i] = Geom { x, y, w, h, cell, fx, fy, cols, rows };
        let _ = fw;
    }
    out
}

#[allow(dead_code)] // some fields are filled in ahead of the overlays that will read them
pub struct View<'a> {
    /// the cast member shown on each seat's nameplate (index into cast::CAST)
    pub chars: [usize; 6],
    pub names: &'a [String; 6],
    pub humans: [bool; 6],
    pub settings: &'a Settings,
    pub t: f32,
    pub tick_frac: f32,
    pub show_overlay_info: bool,
    pub ghost_on: bool,
    pub inputs: [u16; 6],
    pub paused_label: Option<&'a str>,
}

pub struct Renderer {
    pub sprites: Sprites,
    pub scene: Scene,
    pub fx: Fx,
    pub geoms: [Geom; 6],
}

impl Renderer {
    pub fn new() -> Self {
        Renderer { sprites: Sprites::new(), scene: Scene::new(), fx: Fx::new(), geoms: [Geom::default(); 6] }
    }

    /// Feed this tick's sim events to the effects. Audio is separate.
    pub fn on_events(&mut self, m: &Match, st: &Settings) {
        self.fx.on_events(m, &self.geoms, st);
    }

    pub fn draw_match(&mut self, cv: &mut Canvas, m: &Match, v: &View, area: (i32, i32, i32, i32)) {
        self.sprites.set_scale(cv.k);
        self.geoms = layout(m, &v.humans, area);
        // boards kick when hit and on big moves
        for i in 0..6 {
            let (jx, jy) = self.fx.jolt(i);
            let g = &mut self.geoms[i];
            g.x += jx;
            g.fx += jx;
            g.y += jy;
            g.fy += jy;
        }
        self.fx.k = cv.k;
        let st = v.settings;
        let n_active = m.active_count();
        for i in 0..6 {
            if m.seats[i].board.is_none() {
                continue;
            }
            let faces = st.decorations && st.theme == ThemeId::Voyage && (v.humans[i] || n_active <= 2);
            self.draw_seat(cv, m, i, faces, v);
        }
        self.fx.draw(cv, m, &self.geoms, st);
    }

    fn draw_seat(&mut self, cv: &mut Canvas, m: &Match, i: usize, faces: bool, v: &View) {
        let g = self.geoms[i];
        let st = v.settings;
        let l = look(st.theme);
        let seat = &m.seats[i];
        let board = seat.board.as_ref().unwrap();
        let style = board.style();
        let cell = g.cell;
        let fw = g.cols * cell;
        let fh = g.rows * cell;
        let flat = st.theme == ThemeId::Tournament || st.preset == Preset::Potato;
        let ts = (cell / 11).clamp(1, 3);
        let _s = crate::prof::scope("S:well");
        // well
        let accent = match style {
            Style::Quad => CYAN,
            Style::Chain => GREEN,
            Style::Capsule => PINK,
            Style::Panel => YELLOW,
        };
        if st.theme == ThemeId::Voyage && st.preset != Preset::Potato {
            // a solid, bevelled cabinet: drop shadow, dark rim, accent edge, inner lip
            let r = (cell / 3).clamp(6, 14);
            cv.soft_shadow(g.fx - 8, g.fy - 8, fw + 16, fh + 16, r + 4, 7, 130);
            // only the visible border of each layer is filled; the well covers the middle
            cv.round_ring(g.fx - 8, g.fy - 8, fw + 16, fh + 16, r + 4, 3, lighten(accent, 40), darken(accent, 90), 255);
            cv.round_ring(g.fx - 5, g.fy - 5, fw + 10, fh + 10, r + 1, 2, 0xFF0B_0F22, 0xFF0B_0F22, 255);
            cv.round_ring(g.fx - 3, g.fy - 3, fw + 6, fh + 6, r, 3, darken(accent, 120), darken(accent, 120), 255);
        } else {
            cv.frame(g.fx - 2, g.fy - 2, fw + 4, fh + 4, 2, if st.theme == ThemeId::Phosphor { l.dim } else { hex(0x555C6E) }, 255);
        }
        if st.theme == ThemeId::Voyage && st.preset != Preset::Potato {
            // deep well with a soft top-to-bottom tone and an accent wash at the floor
            cv.round_grad(g.fx, g.fy, fw, fh, 4, 0xFF10_1630, 0xFF07_0A18, 255);
            cv.rect_a(g.fx, g.fy + fh - fh / 5, fw, fh / 5, accent, 10);
            cv.rect_a(g.fx, g.fy + fh - fh / 10, fw, fh / 10, accent, 10);
        } else {
            cv.rect_a(g.fx, g.fy, fw, fh, l.well, l.well_alpha);
        }
        if self.fx.flash[i] > 0.05 && st.theme != ThemeId::Tournament && !st.reduce_motion {
            cv.rect_a(g.fx, g.fy, fw, fh, l.warn, (self.fx.flash[i] * 40.0) as u32);
        }
        let _s = crate::prof::scope("S:grid");
        // grid
        let grid_a = if style == Style::Chain || style == Style::Capsule { 40 } else { 70 };
        for x in 1..g.cols {
            cv.rect_a(g.fx + x * cell, g.fy, 1, fh, l.grid, grid_a);
        }
        for y in 1..g.rows {
            cv.rect_a(g.fx, g.fy + y * cell, fw, 1, l.grid, grid_a);
        }
        // pieces may poke above the well only in Quad (its spawn rows); Chain pairs slide in
        if style == Style::Quad {
            cv.set_clip(g.fx, g.fy - cell * 2, fw, fh + cell * 2);
        } else {
            cv.set_clip(g.fx, g.fy, fw, fh);
        }
        let _s = crate::prof::scope("S:pieces");
        match board {
            Board::Quad(q) => self.draw_quad(cv, q, &g, faces, flat, st, v.ghost_on),
            Board::Chain(c) => self.draw_chain(cv, c, &g, faces, flat, st, v.t),
            Board::Capsule(c) => self.draw_capsule(cv, c, &g, faces, flat, st, v.t),
            Board::Panel(p) => self.draw_panel(cv, p, &g, faces, flat, st, v.t),
        }
        cv.reset_clip();
        let _s = crate::prof::scope("S:hud");
        // danger is a shape and a position, not only a tint
        if board.danger() {
            let pulse = ((v.t * 6.0).sin() * 0.5 + 0.5) * 120.0 + 60.0;
            match style {
                Style::Quad => {
                    for k in 0..4 {
                        cv.rect_a(g.fx + k * fw / 4 + 4, g.fy + 2, fw / 8, 3, l.warn, pulse as u32);
                    }
                }
                Style::Panel => cv.rect_a(g.fx, g.fy, fw, 3, l.warn, pulse as u32),
                Style::Capsule => cv.frame(g.fx + 3 * cell, g.fy, 2 * cell, cell, 2, l.warn, pulse as u32),
                Style::Chain => {}
            }
        }
        // garbage meter on the left edge of the field
        let (trav, tray) = m.incoming(i);
        let per_row = match style {
            Style::Quad => 1,
            Style::Chain => 6,
            Style::Capsule => 2,
            Style::Panel => 6,
        };
        let rows_tray = ((tray + per_row - 1) / per_row).min(g.rows as u32) as i32;
        let rows_trav = ((trav + per_row - 1) / per_row).min(g.rows as u32 - rows_tray as u32) as i32;
        let mw = (cell / 4).clamp(4, 8);
        let mx = g.fx - 12 - mw;
        cv.round_rect(mx - 1, g.fy - 1, mw + 2, fh + 2, (mw + 2) / 2, 0xFF05_0814, 200);
        if rows_trav > 0 {
            let y0 = g.fy + fh - (rows_tray + rows_trav) * cell;
            cv.round_rect(mx, y0, mw, rows_trav * cell, mw / 2, hex(0xFF9F3B), 220);
        }
        if rows_tray > 0 {
            let pulse = ((v.t * 8.0).sin() * 0.5 + 0.5) * 60.0;
            cv.round_rect(mx, g.fy + fh - rows_tray * cell, mw, rows_tray * cell, mw / 2, l.warn, 255);
            cv.round_rect(mx, g.fy + fh - rows_tray * cell, mw, rows_tray * cell, mw / 2, 0xFFFF_FFFF, pulse as u32);
        }
        let _s = crate::prof::scope("S:plate");
        // header: a nameplate pill with the name, the style chip, bot info / KOs, target
        let name = &v.names[i];
        let big = fw >= 200;
        let ns = if big { 2 } else { 1 };
        let ss = 1;
        let plate_h = if big { 50 } else { 34 };
        let lift = if style == Style::Quad { cell * 2 + 4 } else { 12 };
        let top = g.fy - lift - plate_h;
        if st.theme == ThemeId::Voyage && st.preset != Preset::Potato {
            cv.glass(g.fx - 8, top, fw + 16, plate_h, plate_h / 2, crate::ui::GLASS, 175);
        }
        let name_col = if v.humans[i] { l.accent } else { 0xFFFF_FFFF };
        let ny = top + if big { 5 } else { 3 };
        // the character: a portrait that pops out of the plate and reacts to the board
        let mut name_x = g.fx + 6;
        if st.characters {
            let member = &crate::cast::CAST[v.chars[i].min(crate::cast::CAST.len() - 1)];
            let expr = if m.outcome != Outcome::Running && m.is_winner(i) {
                "delighted"
            } else if !seat.alive {
                "sad"
            } else if board.danger() {
                "shocked"
            } else if self.fx.flash[i] > 0.3 {
                "angry"
            } else {
                "smile"
            };
            let ps = if big { 76 } else { 50 };
            let px = g.fx - 4;
            let py = top + plate_h - ps;
            cv.circle((px + ps / 2) as f32, (py + ps / 2 + 2) as f32, ps as f32 * 0.47, member.color, 255);
            cv.set_clip(px, py - ps, ps, ps * 2 - 2);
            crate::assets::draw_portrait(cv, member.id, expr, px - ps / 10, py - ps / 8, ps + ps / 5, 255, false);
            cv.reset_clip();
            name_x = px + ps + 4;
        }
        cv.text_weight(name_x, ny, &crate::ui::nice(name), ns, name_col, 255, Weight::Bold);
        // style chip
        let sname = crate::ui::title_case(style.name());
        let sw = Canvas::text_wb(&sname, ss, Weight::Bold) + 14;
        let sh = Canvas::px(ss) as i32 + 4;
        let sx = g.fx + fw + 2 - sw;
        let sy = ny + 2;
        cv.round_rect(sx, sy, sw, sh, sh / 2, accent, 230);
        cv.text_weight(sx + 7, sy + 1, &sname, ss, 0xFF0A_0E1E, 255, Weight::Bold);
        let line2 = ny + Canvas::px(ns) as i32 + if big { 1 } else { 0 };
        let mut info = String::new();
        if seat.cfg.kind == SeatKind::Bot {
            info = format!("{}   Lv {}", crate::ui::title_case(seat.cfg.personality.name()), seat.cfg.strength);
        }
        if seat.kos > 0 {
            let stars: String = std::iter::repeat('\u{2605}').take(seat.kos.min(8) as usize).collect();
            info = if info.is_empty() { stars } else { format!("{}  {}", info, stars) };
        }
        if !info.is_empty() && big {
            cv.text_s(name_x, line2, &info, 1, l.dim);
        }
        if m.active_count() > 2 && seat.alive && seat.target != 0xFF {
            let t = format!("\u{2192} {}", v.names[seat.target as usize]);
            let ty = if big { line2 } else { sy + sh + 1 };
            cv.text_right(g.fx + fw, ty, &t, 1, l.dim);
        }
        let _s = crate::prof::scope("S:footer");
        // footer stats
        let fy = g.fy + fh + 10;
        let stat = crate::ui::nice(&stat_line(m, i));
        let fs = if fw >= 200 { 2 } else { 1 };
        cv.text_weight(g.fx + 1, fy + 1, &stat, fs, 0xFF00_0000, 140, Weight::Bold);
        cv.text_weight(g.fx, fy, &stat, fs, l.text, 255, Weight::Bold);
        if v.settings.input_display && v.humans[i] {
            draw_inputs(cv, g.fx, fy + 12 * ts, v.inputs[i], ts, &l);
        }
        // dead / winner overlays
        if !seat.alive {
            cv.rect_a(g.fx, g.fy, fw, fh, 0xFF00_0000, 150);
            if !self.fx.has_banner(i) {
                let txt = if seat.ko_by.is_some() { "K.O." } else { "TOP OUT" };
                cv.text_center(g.fx + fw / 2, g.fy + fh / 2 - 8 * ts, txt, ts + 1, l.warn);
            }
        }
        if m.outcome != Outcome::Running && m.is_winner(i) && !m.header.mode.is_solo() && !self.fx.has_banner(i) {
            cv.text_center(g.fx + fw / 2, g.fy + fh / 2 - 20 * ts, "WIN!", ts + 2, l.accent);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_quad(&mut self, cv: &mut Canvas, q: &qd::QuadState, g: &Geom, faces: bool, flat: bool, st: &Settings, ghost_on: bool) {
        let cell = g.cell;
        let pat = st.colorblind;
        for y in (qd::VIS0 - 2)..qd::H {
            for x in 0..qd::W {
                let v = q.grid[(y * qd::W + x) as usize];
                if v == 0 {
                    continue;
                }
                let px = g.fx + x * cell;
                let py = g.fy + (y - qd::VIS0) * cell;
                let clearing = q.clear_rows.contains(&y);
                let c = theme_tint(st.theme, qd_color(v));
                let spr = self.sprites.get(SpriteKey { kind: CellKind::Gem, color: c, size: cell, face: false, pattern: pat, flat, frame: 0 });
                cv.blit(spr, px, py, 255);
                if clearing {
                    let k = (q.timer as f32 / 18.0).clamp(0.0, 1.0);
                    cv.rect_a(px, py, cell, cell, 0xFFFF_FFFF, (k * 230.0) as u32);
                }
            }
        }
        if let Some(p) = q.cur {
            let c = theme_tint(st.theme, qd_color(p.t + 1));
            if ghost_on {
                if let Some(gy) = q.ghost_y() {
                    for &(dx, dy) in qd::SHAPES[p.t as usize][p.r as usize].iter() {
                        let spr = self.sprites.get(SpriteKey { kind: CellKind::Ghost, color: c, size: cell, face: false, pattern: false, flat, frame: 0 });
                        cv.blit(spr, g.fx + (p.x + dx) * cell, g.fy + (gy + dy - qd::VIS0) * cell, 255);
                    }
                }
            }
            for (k, &(dx, dy)) in qd::SHAPES[p.t as usize][p.r as usize].iter().enumerate() {
                // one face on the center cell of the piece, not on every cell
                let face_cell = faces && k == if p.t == 0 { 1 } else { 2 };
                let kind = if face_cell { CellKind::GemFace } else { CellKind::Gem };
                let spr = self.sprites.get(SpriteKey { kind, color: c, size: cell, face: face_cell, pattern: pat, flat, frame: 0 });
                cv.blit(spr, g.fx + (p.x + dx) * cell, g.fy + (p.y + dy - qd::VIS0) * cell, 255);
            }
        }
        // hold and next
        cv.reset_clip();
        let small = (cell * 3 / 5).max(4);
        let l = look(st.theme);
        let ts = if cell >= 20 { 2 } else { 1 };
        hud_label(cv, g.fx - (cell as f32 * 4.3) as i32, g.fy, "Hold", ts, l.dim);
        if let Some(h) = q.hold {
            let c = if q.hold_used { GRAY } else { theme_tint(st.theme, qd_color(h + 1)) };
            self.mini_piece(cv, h, g.fx - (cell as f32 * 4.3) as i32, g.fy + 12 * ts, small, c, pat, flat);
        }
        let nx = g.fx + g.cols * cell + (cell / 2);
        hud_label(cv, nx, g.fy, "Next", ts, l.dim);
        for (k, &t) in q.queue.iter().take(q.cfg.preview).enumerate() {
            let c = theme_tint(st.theme, qd_color(t + 1));
            self.mini_piece(cv, t, nx, g.fy + 12 * ts + k as i32 * small * 3, small, c, pat, flat);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn mini_piece(&mut self, cv: &mut Canvas, t: u8, x: i32, y: i32, s: i32, c: u32, pat: bool, flat: bool) {
        for &(dx, dy) in qd::SHAPES[t as usize][0].iter() {
            let spr = self.sprites.get(SpriteKey { kind: CellKind::Gem, color: c, size: s, face: false, pattern: pat && s >= 12, flat, frame: 0 });
            cv.blit(spr, x + dx * s, y + dy * s, 255);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_chain(&mut self, cv: &mut Canvas, c: &ch::ChainState, g: &Geom, faces: bool, flat: bool, st: &Settings, t: f32) {
        let cell = g.cell;
        let l = look(st.theme);
        // the X cell, always visible
        let xx = g.fx + ch::X_CELL.0 * cell;
        let xy = g.fy;
        let xc = if c.danger() { l.warn } else { darken(l.warn, 100) };
        cv.line(xx as f32 + cell as f32 * 0.2, xy as f32 + cell as f32 * 0.2, xx as f32 + cell as f32 * 0.8, xy as f32 + cell as f32 * 0.8, (cell / 8).max(1) as f32, xc, 200);
        cv.line(xx as f32 + cell as f32 * 0.8, xy as f32 + cell as f32 * 0.2, xx as f32 + cell as f32 * 0.2, xy as f32 + cell as f32 * 0.8, (cell / 8).max(1) as f32, xc, 200);
        let popping = c.phase == ch::Phase::Pop;
        // pop progress 0..1 (the sim's pop phase is 30 ticks)
        let pp = if popping { 1.0 - c.timer as f32 / 30.0 } else { 0.0 };
        let cf = cell as f32;
        // pass 1: gummy necks between connected same-color blobs
        if !flat {
            for y in ch::VIS0..ch::H {
                for x in 0..ch::W {
                    let i = (y * ch::W + x) as usize;
                    let v = c.grid[i];
                    if v == 0 || v == ch::NUISANCE || (popping && c.pop_cells.contains(&(i as i32))) {
                        continue;
                    }
                    let col = theme_tint(st.theme, chain_color(v));
                    let px = (g.fx + x * cell) as f32;
                    let py = (g.fy + (y - ch::VIS0) * cell) as f32;
                    let ink = darken(col, 150);
                    let neck = cf * 0.56;
                    if x + 1 < ch::W && c.grid[i + 1] == v && !(popping && c.pop_cells.contains(&(i as i32 + 1))) {
                        let (nx, ny, nw, nh) = (px + cf * 0.5, py + (cf - neck) / 2.0, cf, neck);
                        cv.round_rect(nx as i32 - 1, ny as i32 - 1, nw as i32 + 2, nh as i32 + 2, (nh / 2.0) as i32, ink, 255);
                        cv.round_grad(nx as i32, ny as i32, nw as i32, nh as i32, (nh / 2.0) as i32, lighten(col, 70), darken(col, 25), 255);
                    }
                    if y + 1 < ch::H && c.grid[i + ch::W as usize] == v && !(popping && c.pop_cells.contains(&(i as i32 + ch::W))) {
                        let (nx, ny, nw, nh) = (px + (cf - neck) / 2.0, py + cf * 0.5, neck, cf);
                        cv.round_rect(nx as i32 - 1, ny as i32 - 1, nw as i32 + 2, nh as i32 + 2, (nw / 2.0) as i32, ink, 255);
                        cv.round_grad(nx as i32, ny as i32, nw as i32, nh as i32, (nw / 2.0) as i32, col, darken(col, 30), 255);
                    }
                }
            }
        }
        // pass 2: the blobs; popping ones pulse, then squash and fade
        for y in 1..ch::H {
            for x in 0..ch::W {
                let i = (y * ch::W + x) as usize;
                let v = c.grid[i];
                if v == 0 {
                    continue;
                }
                let px = g.fx + x * cell;
                let py = g.fy + (y - ch::VIS0) * cell;
                let col = theme_tint(st.theme, ch::NUISANCE.eq(&v).then_some(0xFFD8_DCE8).unwrap_or(chain_color(v)));
                let kind = if v == ch::NUISANCE { CellKind::Nuisance } else { CellKind::Blob };
                let base_a = if y < ch::VIS0 { 110 } else { 255 };
                if popping && c.pop_cells.contains(&(i as i32)) {
                    if pp < 0.55 {
                        let spr = self.sprites.get(SpriteKey { kind, color: col, size: cell, face: faces, pattern: st.colorblind, flat, frame: 0 });
                        cv.blit(spr, px, py, base_a);
                        let pulse = ((pp / 0.55) * std::f32::consts::PI * 3.0).sin().abs();
                        cv.circle(px as f32 + cf / 2.0, py as f32 + cf / 2.0, cf * 0.44, 0xFFFF_FFFF, (pulse * 150.0) as u32);
                    } else {
                        let q = (pp - 0.55) / 0.45;
                        let sz = ((cf * (1.0 + 0.25 * q - 0.75 * q * q)) as i32 / 2 * 2).max(2);
                        let spr = self.sprites.get(SpriteKey { kind, color: col, size: sz, face: false, pattern: false, flat, frame: 0 });
                        let a = ((1.0 - q) * 255.0) as u32;
                        cv.blit(spr, px + (cell - sz) / 2, py + (cell - sz) / 2 + (q * cf * 0.15) as i32, a);
                        cv.glow(px as f32 + cf / 2.0, py as f32 + cf / 2.0, cf * (0.6 + q), col, ((1.0 - q) * 90.0) as u32);
                    }
                    continue;
                }
                let spr = self.sprites.get(SpriteKey { kind, color: col, size: cell, face: faces, pattern: st.colorblind, flat, frame: 0 });
                cv.blit(spr, px, py, base_a);
                if v == ch::NUISANCE && cell >= 10 {
                    let (cx, cy) = (px as f32 + cf / 2.0, py as f32 + cf / 2.0);
                    cv.tri([(cx, cy - cf * 0.14), (cx + cf * 0.14, cy), (cx, cy + cf * 0.14)], 0xFF5A_5F78, 200);
                    cv.tri([(cx, cy - cf * 0.14), (cx - cf * 0.14, cy), (cx, cy + cf * 0.14)], 0xFF5A_5F78, 200);
                }
            }
        }
        if let Some(p) = c.pair {
            let sub = p.sub as f32 / 16.0;
            let (cx, cy) = p.child();
            for (x, y, col) in [(p.x, p.y, p.a), (cx, cy, p.b)] {
                let colr = theme_tint(st.theme, chain_color(col));
                let spr = self.sprites.get(SpriteKey { kind: CellKind::Blob, color: colr, size: cell, face: faces, pattern: st.colorblind, flat, frame: 0 });
                let py = g.fy as f32 + (y as f32 + sub - ch::VIS0 as f32) * cell as f32;
                cv.blit(spr, g.fx + x * cell, py as i32, 255);
            }
            // axis marker: a soft pulsing glow behind the pivot blob
            let pulse = (t * 6.0).sin() * 0.5 + 0.5;
            cv.glow(
                (g.fx + p.x * cell) as f32 + cell as f32 / 2.0,
                g.fy as f32 + (p.y as f32 + sub - ch::VIS0 as f32) * cell as f32 + cell as f32 / 2.0,
                cell as f32 * 0.75,
                0xFFFF_FFFF,
                (25.0 + 35.0 * pulse) as u32,
            );
        }
        cv.reset_clip();
        let ts = (cell / 14).clamp(1, 2);
        let nx = g.fx + g.cols * cell + cell / 2;
        hud_label(cv, nx, g.fy, "Next", ts, l.dim);
        for (k, (a, b)) in c.next_pairs().iter().enumerate() {
            let s = if k == 0 { cell } else { cell * 3 / 4 };
            let y0 = g.fy + 12 * ts + k as i32 * (cell * 5 / 2);
            for (j, col) in [b, a].iter().enumerate() {
                let colr = theme_tint(st.theme, chain_color(**col));
                let spr = self.sprites.get(SpriteKey { kind: CellKind::Blob, color: colr, size: s, face: faces && s >= 16, pattern: st.colorblind, flat, frame: 0 });
                cv.blit(spr, nx, y0 + j as i32 * s, 255);
            }
        }
        if c.all_clear_bonus {
            cv.text_s(nx, g.fy + cell * 6, "All clear +30", ts, l.good);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_capsule(&mut self, cv: &mut Canvas, c: &cap::CapsuleState, g: &Geom, faces: bool, flat: bool, st: &Settings, t: f32) {
        let cell = g.cell;
        let frame = ((t * 3.0) as i32 % 2) as u8;
        let flash_on = ((t * 12.0) as i32) % 2 == 0;
        for y in cap::VIS0..cap::H {
            for x in 0..cap::W {
                let i = (y * cap::W + x) as usize;
                let k = c.kind[i];
                if k == 0 {
                    continue;
                }
                let col = theme_tint(st.theme, capsule_color(c.color[i]));
                let kind = match k {
                    cap::K_VIRUS => CellKind::Virus,
                    cap::K_LEFT => CellKind::HalfL,
                    cap::K_RIGHT => CellKind::HalfR,
                    cap::K_TOP => CellKind::HalfT,
                    cap::K_BOTTOM => CellKind::HalfB,
                    _ => CellKind::Single,
                };
                let spr = self.sprites.get(SpriteKey { kind, color: col, size: cell, face: faces, pattern: st.colorblind, flat, frame: if kind == CellKind::Virus { frame } else { 0 } });
                let px = g.fx + x * cell;
                let py = g.fy + (y - cap::VIS0) * cell;
                cv.blit(spr, px, py, 255);
                if c.phase == cap::Phase::Clear && c.clearing.contains(&(i as i32)) {
                    let k = (c.timer as f32 / 20.0).clamp(0.0, 1.0);
                    cv.circle(px as f32 + cell as f32 / 2.0, py as f32 + cell as f32 / 2.0, cell as f32 * 0.46, 0xFFFF_FFFF, (k * 210.0) as u32);
                    let _ = flash_on;
                }
            }
        }
        if let Some(p) = c.pill {
            for (x, y, col, k) in p.cells() {
                let kind = match k {
                    cap::K_LEFT => CellKind::HalfL,
                    cap::K_RIGHT => CellKind::HalfR,
                    cap::K_TOP => CellKind::HalfT,
                    _ => CellKind::HalfB,
                };
                let colr = theme_tint(st.theme, capsule_color(col));
                let spr = self.sprites.get(SpriteKey { kind, color: colr, size: cell, face: faces, pattern: st.colorblind, flat, frame: 0 });
                cv.blit(spr, g.fx + x * cell, g.fy + (y - cap::VIS0) * cell, 255);
            }
        }
        cv.reset_clip();
        let l = look(st.theme);
        let ts = (cell / 14).clamp(1, 2);
        let nx = g.fx + g.cols * cell + cell / 2;
        hud_label(cv, nx, g.fy, "Next", ts, l.dim);
        if let Some((a, b)) = c.preview() {
            for (j, (col, kind)) in [(a, CellKind::HalfL), (b, CellKind::HalfR)].iter().enumerate() {
                let colr = theme_tint(st.theme, capsule_color(*col));
                let spr = self.sprites.get(SpriteKey { kind: *kind, color: colr, size: cell, face: faces, pattern: st.colorblind, flat, frame: 0 });
                cv.blit(spr, nx + j as i32 * cell, g.fy + 12 * ts, 255);
            }
        }
        // virus counter as a big number
        hud_label(cv, nx, g.fy + cell * 3, "Virus", ts, l.dim);
        cv.text_s(nx, g.fy + cell * 3 + 10 * ts, &format!("{}", c.viruses), ts + 1, l.accent);
        cv.text_s(nx, g.fy + cell * 5 + 10 * ts, &format!("Lv {}", c.level), ts, l.dim);
        cv.text_s(nx, g.fy + cell * 6 + 10 * ts, &crate::ui::title_case(cap::SPEED_NAMES[c.speed as usize]), ts, l.dim);
        if c.nes_seeds {
            cv.text_s(nx, g.fy + cell * 7 + 10 * ts, &format!("{:04X}", c.nes_seed), ts, l.dim);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_panel(&mut self, cv: &mut Canvas, p: &pn::PanelState, g: &Geom, faces: bool, flat: bool, st: &Settings, t: f32) {
        let cell = g.cell;
        let rise = (p.rise_sub as i32 * cell) / pn::RISE_STEPS as i32;
        cv.set_clip(g.fx, g.fy, g.cols * cell, g.rows * cell);
        let flash_on = ((t * 14.0) as i32) % 2 == 0;
        let ypx = |y: i32| g.fy + (y - pn::VIS0) * cell - rise;
        // garbage blocks: one sleepy stone across the block
        for b in &p.blocks {
            let x0 = g.fx + b.x * cell;
            let y0 = ypx(b.y);
            let w = b.w * cell;
            let hgt = b.h * cell;
            let _ = flash_on;
            cv.round_rect(x0 + 1, y0 + 1, w - 2, hgt - 2, cell / 4, 0xFF2A_2E40, 255);
            cv.round_grad(x0 + 3, y0 + 3, w - 6, hgt - 6, cell / 5, hex(0xA9AFC6), hex(0x646A84), 255);
            cv.round_rect(x0 + cell / 5, y0 + 4, w - 2 * (cell / 5), (cell / 7).max(2), cell / 14 + 1, 0xFFFF_FFFF, 70);
            if b.crack > 0 {
                let pulse = (t * 14.0).sin() * 0.5 + 0.5;
                cv.round_rect(x0 + 3, y0 + 3, w - 6, hgt - 6, cell / 5, 0xFFFF_FFFF, (60.0 + 140.0 * pulse) as u32);
            }
            if !flat && cell >= 12 {
                for k in 0..b.w {
                    cv.circle((x0 + k * cell + cell / 3) as f32, (y0 + cell / 3) as f32, cell as f32 * 0.06, hex(0x9AA0B6), 200);
                }
                if faces {
                    let cx = x0 as f32 + w as f32 / 2.0;
                    let cy = y0 as f32 + hgt as f32 / 2.0;
                    for sx in [-1.0f32, 1.0] {
                        cv.line(cx + sx * cell as f32 * 0.4 - cell as f32 * 0.15, cy, cx + sx * cell as f32 * 0.4 + cell as f32 * 0.15, cy, 2.0, hex(0x30344A), 220);
                    }
                    cv.text_a(cx as i32 + cell / 2, (cy - cell as f32 * 0.6) as i32, "z", (cell / 14).max(1), hex(0xD8DCE8), 180);
                }
            }
        }
        for y in 0..pn::H {
            for x in 0..pn::W {
                let c = p.grid[(y * pn::W + x) as usize];
                if c.c == 0 || c.c == pn::GARB {
                    continue;
                }
                if c.s == pn::St::Match && c.t <= c.pop_at {
                    continue; // popped
                }
                let mut px = g.fx + x * cell;
                if c.s == pn::St::Swap {
                    px += (c.dir as i32) * -(c.t as i32) * cell / 4;
                }
                let py = ypx(y);
                let col = theme_tint(st.theme, panel_color(c.c));
                let spr = self.sprites.get(SpriteKey { kind: CellKind::Tile, color: col, size: cell, face: faces, pattern: st.colorblind, flat, frame: 0 });
                cv.blit(spr, px, py, 255);
                if c.s == pn::St::Match {
                    let pulse = ((t * 14.0).sin() * 0.5 + 0.5) * 150.0 + 30.0;
                    cv.round_rect(px + 2, py + 2, cell - 4, cell - 4, cell / 6, 0xFFFF_FFFF, pulse as u32);
                    let _ = flash_on;
                }
            }
        }
        // the row being born, dimmed
        for x in 0..pn::W {
            let c = p.next_row[x as usize];
            if c == 0 {
                continue;
            }
            let col = theme_tint(st.theme, panel_color(c));
            let spr = self.sprites.get(SpriteKey { kind: CellKind::Tile, color: col, size: cell, face: false, pattern: st.colorblind, flat, frame: 0 });
            cv.blit(spr, g.fx + x * cell, ypx(pn::H), 255);
            cv.rect_a(g.fx + x * cell, ypx(pn::H), cell, cell, 0xFF00_0000, 150);
        }
        // cursor (drawn unclipped so its brackets show on the bottom row too)
        cv.reset_clip();
        let l = look(st.theme);
        let cx = g.fx + p.cx * cell;
        let cy = ypx(p.cy);
        // corner brackets that breathe a little, outlined so they read on any tile colour
        let th = (cell / 9).max(2);
        let grow = ((t * 5.0).sin() * 0.5 + 0.5) * (cell as f32 * 0.06);
        let (x0, y0) = (cx as f32 - 3.0 - grow, cy as f32 - 3.0 - grow);
        let (x1, y1) = ((cx + cell * 2) as f32 + 3.0 + grow, (cy + cell) as f32 + 3.0 + grow);
        let arm = cell as f32 * 0.42;
        let thf = th as f32;
        for (col, w, a) in [(0xFF0A_0E1E, thf + 3.0, 200u32), (0xFFFF_FFFF, thf, 255u32)] {
            for (px, py, sx, sy) in [(x0, y0, 1.0, 1.0), (x1, y0, -1.0, 1.0), (x0, y1, 1.0, -1.0), (x1, y1, -1.0, -1.0)] {
                cv.line(px, py, px + sx * arm, py, w, col, a);
                cv.line(px, py, px, py + sy * arm, w, col, a);
            }
        }
        cv.round_rect(cx + cell - 1, cy + cell / 4, 2, cell / 2, 1, 0xFFFF_FFFF, 90);
        let ts = (cell / 14).clamp(1, 2);
        let nx = g.fx + g.cols * cell + cell / 2;
        if p.stop > 0 {
            hud_label(cv, nx, g.fy, "Stop", ts, l.good);
            let h = (p.stop.min(240) as i32 * g.rows * cell / 2) / 240;
            cv.rect(nx, g.fy + 12 * ts, (cell / 3).max(3), h, l.good);
        }
        if p.puzzle {
            hud_label(cv, nx, g.fy + cell * 5, "Moves", ts, l.dim);
            cv.text_s(nx, g.fy + cell * 5 + 10 * ts, &format!("{}", p.moves_left.max(0)), ts + 1, l.accent);
        } else {
            cv.text_s(nx, g.fy + cell * 8, &format!("Lv {}", p.level), ts, l.dim);
        }
        if p.chain > 1 {
            cv.text_s(nx, g.fy + cell * 6, &format!("x{}", p.chain), ts + 1, l.accent);
        }
    }
}

fn qd_color(v: u8) -> u32 {
    quad_color(v)
}
pub(crate) fn color_for(style: Style, c: u8) -> u32 {
    match style {
        Style::Quad => quad_color(c),
        Style::Chain => chain_color(c),
        Style::Capsule => capsule_color(c),
        Style::Panel => panel_color(c),
    }
}

pub(crate) fn cell_pos(style: Style, g: &Geom, x: i32, y: i32, b: Option<&Board>) -> (f32, f32) {
    let vis0 = match style {
        Style::Quad => qd::VIS0,
        Style::Chain => ch::VIS0,
        Style::Capsule => cap::VIS0,
        Style::Panel => pn::VIS0,
    };
    let rise = match b {
        Some(Board::Panel(p)) => (p.rise_sub as i32 * g.cell) / pn::RISE_STEPS as i32,
        _ => 0,
    };
    ((g.fx + x * g.cell) as f32, (g.fy + (y - vis0) * g.cell - rise) as f32)
}

pub fn fmt_time(ticks: u32) -> String {
    let cs = ticks as u64 * 100 / 60;
    format!("{}:{:02}.{:02}", cs / 6000, (cs / 100) % 60, cs % 100)
}

fn stat_line(m: &Match, i: usize) -> String {
    let s = &m.seats[i];
    match s.board.as_ref().unwrap() {
        Board::Quad(q) => {
            if m.header.mode.is_solo() {
                format!("Lines {}   Lv {}   {}", q.lines, q.level, fmt_time(m.tick))
            } else {
                format!("Lines {}   Sent {}", q.lines, s.stats.sent)
            }
        }
        Board::Chain(c) => format!("Score {}   Target {}", c.score, c.target),
        Board::Capsule(c) => format!("Score {}", c.score),
        Board::Panel(p) => format!("Score {}   Best x{}", p.score, p.max_chain),
    }
}

fn draw_inputs(cv: &mut Canvas, x: i32, y: i32, bits: u16, ts: i32, l: &Look) {
    use purtris_sim::bits as B;
    let items = [(B::L, "\u{2190}"), (B::R, "\u{2192}"), (B::U, "\u{2191}"), (B::D | B::SD, "\u{2193}"), (B::CCW, "A"), (B::CW, "B"), (B::HD, "HD"), (B::HOLD, "H"), (B::SWAP, "SW"), (B::RAISE, "RS")];
    let mut cx = x;
    for (b, s) in items {
        let on = bits & b != 0;
        cv.text_a(cx, y, s, ts.max(1), if on { l.accent } else { l.dim }, if on { 255 } else { 90 });
        cx += Canvas::text_w(s, ts.max(1)) + 4 * ts;
    }
}

/// Side-of-board label (Next, Hold, Virus...): bold with a soft shadow so it reads on any backdrop.
fn hud_label(cv: &mut Canvas, x: i32, y: i32, s: &str, ts: i32, c: u32) {
    cv.text_weight(x + 1, y + 1, s, ts, 0xFF00_0000, 150, Weight::Bold);
    cv.text_weight(x, y, s, ts, c, 255, Weight::Bold);
}
