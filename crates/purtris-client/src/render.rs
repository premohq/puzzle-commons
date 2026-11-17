//! Match rendering: Voyage scenes, boards for all four styles, HUD, particles, callouts.
//! The renderer only reads the last committed sim state. It never feeds anything back.

use crate::font::Weight;
use crate::gfx::{darken, hex, lighten, mix, Canvas};
use crate::save::Settings;
use crate::theme::*;
use purtris_sim::capsule as cap;
use purtris_sim::chain as ch;
use purtris_sim::panel as pn;
use purtris_sim::quad as qd;
use purtris_sim::{Board, Ev, Match, Outcome, SeatKind, Style};

pub const W: i32 = 1280;
pub const H: i32 = 720;

pub struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,
    max: f32,
    color: u32,
    size: f32,
}
pub struct Callout {
    seat: usize,
    text: String,
    t: f32,
    color: u32,
    scale: i32,
}

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
    /// width of the side column, how far the nameplate floats above the well, its height
    pub side: i32,
    pub lift: i32,
    pub plate_h: i32,
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

pub struct Fx {
    pub particles: Vec<Particle>,
    pub callouts: Vec<Callout>,
    pub shake: f32,
    pub flash: [f32; 6],
    pub energy: f32,
    seed: u32,
}
impl Fx {
    pub fn new() -> Self {
        Fx { particles: Vec::new(), callouts: Vec::new(), shake: 0.0, flash: [0.0; 6], energy: 0.0, seed: 77 }
    }
    fn rnd(&mut self) -> f32 {
        self.seed = self.seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (self.seed >> 8) as f32 / 16777216.0
    }
    pub fn burst(&mut self, x: f32, y: f32, color: u32, n: usize, power: f32, cap: usize) {
        for _ in 0..n {
            if self.particles.len() >= cap {
                return;
            }
            let a = self.rnd() * std::f32::consts::TAU;
            let sp = (0.4 + self.rnd() * 1.8) * power;
            let life = 0.25 + self.rnd() * 0.35;
            let size = 1.5 + self.rnd() * 2.5;
            self.particles.push(Particle { x, y, vx: a.cos() * sp, vy: a.sin() * sp - 1.0, life, max: life, color, size });
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
        for p in self.particles.iter_mut() {
            p.x += p.vx * dt * 60.0;
            p.y += p.vy * dt * 60.0;
            p.vy += 0.04 * dt * 60.0;
            p.vx *= 0.93;
            p.vy *= 0.93;
            p.life -= dt;
        }
        self.particles.retain(|p| p.life > 0.0);
        for c in self.callouts.iter_mut() {
            c.t += dt;
        }
        self.callouts.retain(|c| c.t < 1.4);
        self.shake *= 0.85;
        for f in self.flash.iter_mut() {
            *f *= 0.9;
        }
        self.energy *= 0.995;
    }
    pub fn draw(&self, cv: &mut Canvas, geoms: &[Geom; 6]) {
        for p in &self.particles {
            let a = (p.life / p.max * 255.0) as u32;
            cv.glow(p.x, p.y, p.size * 3.0, p.color, a / 3);
            cv.circle(p.x, p.y, p.size * 0.6, lighten(p.color, 120), a);
        }
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
}

fn style_dims(s: Style) -> (i32, i32) {
    match s {
        Style::Quad => (10, 20),
        Style::Chain => (6, 12),
        Style::Capsule => (8, 16),
        Style::Panel => (6, 12),
    }
}
/// Width of the side column (Next, Hold, counters) for a board `bw` wide.
fn side_w(bw: i32) -> i32 {
    (bw * 34 / 100).max(46)
}
/// Space kept left and right of the well: garbage meter (and Quad's Hold) | the side column.
fn gutters(style: Style, bw: i32) -> (i32, i32) {
    let sw = side_w(bw);
    (26 + if style == Style::Quad { sw + 6 } else { 0 }, 18 + sw + 2)
}
/// Space kept above and below the well, the nameplate height and its lift above the well.
/// When any Quad board plays, every plate is lifted clear of Quad's two spawn rows so the
/// plates line up across the match.
fn vert(bw: i32, any_quad: bool) -> (i32, i32, i32, i32) {
    let big = bw >= 200;
    let plate_h = if big { 50 } else { 34 };
    let pop = if big { 26 } else { 16 };
    let lift = if any_quad { bw / 5 + 6 } else { 14 };
    (pop + plate_h + lift, 8 + 8 + if big { 30 } else { 20 } + 4, plate_h, lift)
}
fn block_w(style: Style, bw: i32) -> i32 {
    let (l, r) = gutters(style, bw);
    l + bw + r
}
fn block_h(bw: i32, any_quad: bool) -> i32 {
    let (top, bot, _, _) = vert(bw, any_quad);
    top + bw * 2 + bot
}
fn gcd(a: i32, b: i32) -> i32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// Board area rectangles for the active seats. Every style's well is twice as tall as it is
/// wide, so boards of one size class come out the same size whatever their style.
pub fn layout(m: &Match, humans: &[bool; 6], area: (i32, i32, i32, i32)) -> [Geom; 6] {
    let mut out = [Geom::default(); 6];
    let active: Vec<usize> = (0..6).filter(|&i| m.seats[i].board.is_some()).collect();
    let n = active.len();
    if n == 0 {
        return out;
    }
    let (ax, ay, aw, ah) = area;
    let style = |i: usize| m.seats[i].cfg.profile.style();
    let any_quad = active.iter().any(|&i| style(i) == Style::Quad);
    let nh = active.iter().filter(|&&i| humans[i]).count();
    // (seat, block x, block y, board width)
    let mut placed: Vec<(usize, i32, i32, i32)> = Vec::new();
    if n <= 4 {
        let scale = |i: usize| if humans[i] && n > 2 && nh < n { 1.3f32 } else { 1.0 };
        const GAP: i32 = 14;
        let fits = |bws: &[i32]| {
            let tw: i32 = active.iter().zip(bws).map(|(&i, &b)| block_w(style(i), b)).sum::<i32>() + GAP * (n as i32 - 1);
            tw <= aw && bws.iter().all(|&b| block_h(b, any_quad) <= ah)
        };
        let solve = |snap: &dyn Fn(usize, f32) -> i32| -> Vec<i32> {
            let mut b = 420.0f32;
            loop {
                let bws: Vec<i32> = active.iter().map(|&i| snap(i, b)).collect();
                if fits(&bws) || b <= 40.0 {
                    return bws;
                }
                b -= 1.0;
            }
        };
        let free = solve(&|i, b| {
            let c = style_dims(style(i)).0;
            ((b * scale(i)) as i32 / c).max(4) * c
        });
        let mut bws = free.clone();
        if active.iter().all(|&i| scale(i) == 1.0) {
            // one size class: snap to a width every style in the match divides evenly
            let l = active.iter().fold(1, |l, &i| {
                let c = style_dims(style(i)).0;
                l / gcd(l, c) * c
            });
            let even = solve(&|_, b| ((b as i32) / l).max(1) * l);
            let best = *free.iter().max().unwrap();
            if fits(&even) && even[0] * 10 >= best * 9 {
                bws = even;
            }
        }
        let total: i32 = active.iter().zip(&bws).map(|(&i, &b)| block_w(style(i), b)).sum();
        let gap = if n > 1 { ((aw - total) / n as i32).clamp(0, 90) } else { 0 };
        let mut x = ax + (aw - total - gap * (n as i32 - 1)) / 2;
        for (&i, &bw) in active.iter().zip(&bws) {
            placed.push((i, x, ay + (ah - block_h(bw, any_quad)) / 2, bw));
            x += block_w(style(i), bw) + gap;
        }
    } else {
        let mut rects: Vec<(usize, (i32, i32, i32, i32))> = Vec::new();
        if nh == 1 {
            let hi = *active.iter().find(|&&i| humans[i]).unwrap();
            let lw = aw * 2 / 5;
            rects.push((hi, (ax, ay, lw, ah)));
            let others: Vec<usize> = active.iter().copied().filter(|&i| i != hi).collect();
            let cols = 3;
            let rows = (others.len() as i32 + cols - 1) / cols;
            let cw = (aw - lw) / cols;
            let chh = ah / rows.max(1);
            for (k, &i) in others.iter().enumerate() {
                let (c, r) = (k as i32 % cols, k as i32 / cols);
                rects.push((i, (ax + lw + c * cw, ay + r * chh, cw, chh)));
            }
        } else {
            let cols = 3;
            let (cw, chh) = (aw / cols, ah / 2);
            let mut order = active.clone();
            order.sort_by_key(|&i| !humans[i]);
            for (k, &i) in order.iter().enumerate() {
                let (c, r) = (k as i32 % cols, k as i32 / cols);
                rects.push((i, (ax + c * cw, ay + r * chh, cw, chh)));
            }
        }
        for (i, (x, y, w, h)) in rects {
            let c = style_dims(style(i)).0;
            let mut bw = (420 / c) * c;
            while bw > c * 4 && (block_w(style(i), bw) > w - 6 || block_h(bw, any_quad) > h - 4) {
                bw -= c;
            }
            placed.push((i, x + (w - block_w(style(i), bw)) / 2, y + (h - block_h(bw, any_quad)) / 2, bw));
        }
    }
    for (i, bx, by, bw) in placed {
        let (cols, rows) = style_dims(style(i));
        let (top, _, plate_h, lift) = vert(bw, any_quad);
        let (gl, _) = gutters(style(i), bw);
        out[i] = Geom { x: bx, y: by, w: block_w(style(i), bw), h: block_h(bw, any_quad), cell: bw / cols, fx: bx + gl, fy: by + top, cols, rows, side: side_w(bw), lift, plate_h };
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
    wells: std::collections::HashMap<(i32, i32, i32, u32, u32), Canvas>,
}

const WELL_PAD: i32 = 26;

/// The glass cabinet around a well: drop shadow, neon edge, deep glass with column lanes,
/// shading under the top and a glow at the floor, and small ticks at the grid corners.
/// Painted in real pixels at scale `k`, offset by WELL_PAD.
#[allow(clippy::too_many_arguments)]
fn paint_well(k: f32, fw: i32, fh: i32, cell: i32, cols: i32, rows: i32, accent: u32, r: i32) -> Canvas {
    let p = WELL_PAD;
    let mut c = Canvas::new(((fw + 2 * p) as f32 * k).round() as i32, ((fh + 2 * p) as f32 * k).round() as i32);
    c.k = k;
    c.soft_shadow(p - 6, p - 6, fw + 12, fh + 12, r + 4, 9, 150);
    c.round_ring(p - 5, p - 5, fw + 10, fh + 10, r + 3, 3, lighten(accent, 90), darken(accent, 30), 255);
    c.round_ring(p - 2, p - 2, fw + 4, fh + 4, r, 2, 0xFF04_0610, 0xFF04_0610, 255);
    c.round_grad(p, p, fw, fh, r - 2, 0xFF0E_1430, 0xFF05_0712, 255);
    for x in (0..cols).step_by(2) {
        c.rect_a(p + x * cell, p, cell, fh, 0xFFFF_FFFF, 5);
    }
    for i in 0..6 {
        c.rect_a(p, p + i * 3, fw, 3, 0xFF00_0000, (70 - i * 11) as u32);
    }
    for i in 0..8 {
        c.rect_a(p, p + fh - (i + 1) * fh / 40, fw, fh / 40, accent, (22 - i * 2) as u32);
    }
    let t = (cell / 9).max(2);
    for x in 1..cols {
        for y in 1..rows {
            let (px, py) = (p + x * cell, p + y * cell);
            c.rect_a(px - t, py, t * 2 + 1, 1, 0xFF8C_A6FF, 26);
            c.rect_a(px, py - t, 1, t * 2 + 1, 0xFF8C_A6FF, 26);
        }
    }
    c
}

impl Renderer {
    pub fn new() -> Self {
        Renderer { sprites: Sprites::new(), scene: Scene::new(), fx: Fx::new(), geoms: [Geom::default(); 6], wells: std::collections::HashMap::new() }
    }

    /// Feed this tick's sim events into particles/callouts. Returns nothing; audio is separate.
    pub fn on_events(&mut self, m: &Match, st: &Settings) {
        let cap = if st.theme == ThemeId::Tournament { 0 } else { st.preset.particle_cap() };
        let n_active = m.active_count();
        for (seat, e) in &m.events {
            let s = *seat as usize;
            let g = self.geoms[s];
            if g.cell == 0 {
                continue;
            }
            let style = m.seats[s].cfg.profile.style();
            match e {
                Ev::Burst { x, y, color } => {
                    let (px, py) = cell_pos(style, &g, *x, *y, m.seats[s].board.as_ref());
                    let c = theme_tint(st.theme, color_for(style, *color));
                    let n = if n_active > 2 && !m.seats[s].cfg.kind.eq(&SeatKind::Human) { 2 } else { 6 };
                    self.fx.burst(px + g.cell as f32 / 2.0, py + g.cell as f32 / 2.0, c, n, 1.0 + g.cell as f32 / 30.0, cap);
                }
                Ev::Clear { power, text } => {
                    let l = look(st.theme);
                    let scale = ((g.cell / 10).max(1) + if *power >= 6 { 1 } else { 0 }).min(5);
                    self.fx.callout(s, text.clone(), if *power >= 4 { l.accent } else { l.text }, scale);
                    self.fx.energy = (self.fx.energy + *power as f32 * 0.06).min(1.0);
                    if st.shake && *power >= 6 {
                        self.fx.shake = 6.0;
                    }
                }
                Ev::AllClear => self.fx.callout(s, "ALL CLEAR!".into(), look(st.theme).good, (g.cell / 9).max(2)),
                Ev::LevelClear => self.fx.callout(s, "BOTTLE CLEAR!".into(), look(st.theme).good, (g.cell / 9).max(2)),
                Ev::GarbageLand { .. } => self.fx.flash[s] = 1.0,
                Ev::Ko { .. } => {
                    let (cx, cy) = (g.fx as f32 + (g.cols * g.cell) as f32 / 2.0, g.fy as f32 + (g.rows * g.cell) as f32 / 2.0);
                    self.fx.burst(cx, cy, look(st.theme).warn, 40, 3.0, cap);
                }
                _ => {}
            }
        }
    }

    pub fn draw_match(&mut self, cv: &mut Canvas, m: &Match, v: &View, area: (i32, i32, i32, i32)) {
        self.sprites.set_scale(cv.k);
        self.geoms = layout(m, &v.humans, area);
        let st = v.settings;
        let n_active = m.active_count();
        for i in 0..6 {
            if m.seats[i].board.is_none() {
                continue;
            }
            let faces = st.decorations && st.theme == ThemeId::Voyage && (v.humans[i] || n_active <= 2);
            self.draw_seat(cv, m, i, faces, v);
        }
        if st.theme != ThemeId::Tournament {
            self.fx.draw(cv, &self.geoms);
        } else {
            // Tournament keeps callouts (information the original showed), no particles
            let parts = std::mem::take(&mut self.fx.particles);
            self.fx.draw(cv, &self.geoms);
            self.fx.particles = parts;
        }
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
        let glassy = st.theme == ThemeId::Voyage && st.preset != Preset::Potato;
        let danger = board.danger() && seat.alive;
        let pulse = ((v.t * 6.0).sin() * 0.5 + 0.5) as f32;
        if glassy {
            // the cabinet never changes during a match, so it is painted once and reused
            let r = (cell / 3).clamp(8, 16);
            let key = (fw, fh, cell, accent, cv.k.to_bits());
            if self.wells.len() > 24 {
                self.wells.clear();
            }
            let wc = self.wells.entry(key).or_insert_with(|| paint_well(cv.k, fw, fh, cell, g.cols, g.rows, accent, r));
            cv.blit(wc, g.fx - WELL_PAD, g.fy - WELL_PAD, 255);
            // the halo stays live: it breathes red when the stack is in danger
            let edge = if danger { mix(accent, l.warn, (120.0 + 135.0 * pulse) as u32) } else { accent };
            cv.round_ring(g.fx - 9, g.fy - 9, fw + 18, fh + 18, r + 7, 5, edge, edge, if danger { (40.0 + 50.0 * pulse) as u32 } else { 34 });
            if danger {
                cv.round_ring(g.fx - 5, g.fy - 5, fw + 10, fh + 10, r + 3, 3, lighten(edge, 90), darken(edge, 30), (120.0 + 120.0 * pulse) as u32);
            }
        } else {
            cv.frame(g.fx - 2, g.fy - 2, fw + 4, fh + 4, 2, if danger { l.warn } else if st.theme == ThemeId::Phosphor { l.dim } else { hex(0x555C6E) }, 255);
            cv.rect_a(g.fx, g.fy, fw, fh, l.well, l.well_alpha);
        }
        if self.fx.flash[i] > 0.05 && st.theme != ThemeId::Tournament && !st.reduce_motion {
            cv.rect_a(g.fx, g.fy, fw, fh, l.warn, (self.fx.flash[i] * 40.0) as u32);
        }
        let _s = crate::prof::scope("S:grid");
        // grid: hairlines on the flat themes (glass wells carry theirs in the cached cabinet)
        if !glassy {
            let grid_a = if style == Style::Chain || style == Style::Capsule { 40 } else { 70 };
            for x in 1..g.cols {
                cv.rect_a(g.fx + x * cell, g.fy, 1, fh, l.grid, grid_a);
            }
            for y in 1..g.rows {
                cv.rect_a(g.fx, g.fy + y * cell, fw, 1, l.grid, grid_a);
            }
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
        // header: a nameplate with the portrait, the name, the style chip, bot info / KOs, target
        let name = crate::ui::nice(&v.names[i]);
        let big = fw >= 200;
        let ns = if big { 2 } else { 1 };
        let plate_h = g.plate_h;
        let top = g.fy - g.lift - plate_h;
        let glassy = st.theme == ThemeId::Voyage && st.preset != Preset::Potato;
        let (plx, plw) = (g.fx - 8, fw + 16);
        hud_plate(cv, glassy, plx, top, plw, plate_h, plate_h / 2);
        let name_col = if v.humans[i] { l.accent } else { 0xFFFF_FFFF };
        // the character: a portrait that pops out of the plate and reacts to the board
        let mut name_x = plx + plate_h / 2 - 4;
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
            let px = plx + 2;
            let py = top + plate_h - ps;
            cv.circle((px + ps / 2) as f32, (py + ps / 2 + 2) as f32, ps as f32 * 0.47, member.color, 255);
            cv.set_clip(px, py - ps, ps, ps * 2 - 2);
            crate::assets::draw_portrait(cv, member.id, expr, px - ps / 10, py - ps / 8, ps + ps / 5, 255, false);
            cv.reset_clip();
            name_x = px + ps + 6;
        }
        let right = plx + plw - if big { 14 } else { 10 };
        let ny = top + if big { 5 } else { 4 };
        // style chip: always on a big plate, on a small one only when the whole name still fits
        let sname = crate::ui::title_case(style.name());
        let sw = Canvas::text_wb(&sname, 1, Weight::Bold) + 14;
        let sh = Canvas::px(1) as i32 + 4;
        let chip = big || Canvas::text_wb(&name, ns, Weight::Bold) + sw + 8 <= right - name_x;
        let name_room = right - name_x - if chip { sw + 8 } else { 0 };
        cv.text_weight(name_x, ny, &fit_text(&name, ns, Weight::Bold, name_room), ns, name_col, 255, Weight::Bold);
        if chip {
            let (sx, sy) = (right - sw, ny + if big { 3 } else { -1 });
            cv.round_rect(sx, sy, sw, sh, sh / 2, accent, 235);
            cv.text_weight(sx + 7, sy + 1, &sname, 1, 0xFF0A_0E1E, 255, Weight::Bold);
        }
        // second line: bot info and KO stars on the left, the current target on the right
        let line2 = ny + Canvas::px(ns) as i32 + if big { 1 } else { 0 };
        let mut room = right - name_x;
        if m.active_count() > 2 && seat.alive && seat.target != 0xFF {
            let t = format!("\u{2192} {}", crate::ui::nice(&v.names[seat.target as usize]));
            if Canvas::text_w(&t, 1) <= room {
                cv.text_right(right, line2, &t, 1, l.dim);
                room -= Canvas::text_w(&t, 1) + 8;
            }
        }
        let stars: String = std::iter::repeat('\u{2605}').take(seat.kos.min(8) as usize).collect();
        let mut info = String::new();
        if seat.cfg.kind == SeatKind::Bot {
            info = if big { format!("{}   Lv {}", crate::ui::title_case(seat.cfg.personality.name()), seat.cfg.strength) } else { format!("Lv {}", seat.cfg.strength) };
        }
        for cand in [if info.is_empty() { stars.clone() } else if stars.is_empty() { info.clone() } else { format!("{}  {}", info, stars) }, stars.clone(), info.clone()] {
            if !cand.is_empty() && Canvas::text_w(&cand, 1) <= room {
                cv.text_s(name_x, line2, &cand, 1, l.dim);
                break;
            }
        }
        let _s = crate::prof::scope("S:footer");
        // footer: the seat's numbers on a pill under the cabinet
        let vs = if big { 2 } else { 1 };
        let mut pairs = stat_pairs(m, i);
        let need = |p: &[(&str, String)]| p.iter().map(|(a, b)| Canvas::text_w(a, 1) + 6 + Canvas::text_wb(b, vs, Weight::Bold)).sum::<i32>() + 14 * (p.len() as i32 - 1);
        while pairs.len() > 1 && need(&pairs) > plw - 16 {
            pairs.pop();
        }
        let pill_h = if big { 30 } else { 20 };
        let py = g.fy + fh + 16;
        hud_plate(cv, glassy, plx, py, plw, pill_h, pill_h / 2);
        let colw = (plw - 16) / pairs.len().max(1) as i32;
        for (k, (lab, val)) in pairs.iter().enumerate() {
            let (lw, vw) = (Canvas::text_w(lab, 1), Canvas::text_wb(val, vs, Weight::Bold));
            let x0 = plx + 8 + k as i32 * colw + (colw - lw - 6 - vw) / 2;
            let vy = py + (pill_h - Canvas::px(vs) as i32) / 2 - 1;
            let ly = vy + ((Canvas::px(vs) - Canvas::px(1)) * 0.74) as i32;
            cv.text_a(x0, ly, lab, 1, l.dim, 255);
            cv.text_weight(x0 + lw + 6, vy, val, vs, l.text, 255, Weight::Bold);
        }
        if v.settings.input_display && v.humans[i] {
            draw_inputs(cv, g.fx, py + pill_h + 6, v.inputs[i], ts, &l);
        }
        // dead / winner overlays
        if !seat.alive {
            cv.rect_a(g.fx, g.fy, fw, fh, 0xFF00_0000, 150);
            let txt = if seat.ko_by.is_some() { "K.O." } else { "TOP OUT" };
            cv.text_center(g.fx + fw / 2, g.fy + fh / 2 - 8 * ts, txt, ts + 1, l.warn);
        }
        if m.outcome != Outcome::Running && m.is_winner(i) && !m.header.mode.is_solo() {
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
                    cv.round_rect(px + 1, py + 1, cell - 2, cell - 2, cell / 5, 0xFFFF_FFFF, (k * 200.0) as u32);
                }
            }
        }
        // cleared rows: a beam of light that sweeps out from the middle and fades
        if !q.clear_rows.is_empty() && !flat {
            let k = (q.timer as f32 / 18.0).clamp(0.0, 1.0);
            let fw = g.cols * cell;
            let span = (fw as f32 * (1.15 - k * 0.6)) as i32;
            for &y in q.clear_rows.iter() {
                let py = g.fy + (y - qd::VIS0) * cell;
                let cx = g.fx + fw / 2;
                for (grow, a) in [(cell / 2, 40u32), (cell / 4, 80), (2, 200)] {
                    cv.round_rect(cx - span / 2, py + cell / 2 - grow - 1, span, grow * 2 + 2, grow + 1, 0xFFFF_FFFF, (a as f32 * k) as u32);
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
        // hold and next, each in its own framed box beside the cabinet
        cv.reset_clip();
        let l = look(st.theme);
        let glassy = st.theme == ThemeId::Voyage && st.preset != Preset::Potato;
        let (sw, top) = (g.side, g.fy - 8);
        let s1 = ((sw - 12) / 4).max(3);
        let s2 = (s1 * 4 / 5).max(3);
        let head = side_head(sw, "HOLD");
        let hx = g.fx - 26 - sw;
        side_box(cv, glassy, hx, top, sw, head + s1 * 3 + 4, "HOLD", &l);
        if let Some(h) = q.hold {
            let c = if q.hold_used { GRAY } else { theme_tint(st.theme, qd_color(h + 1)) };
            self.mini_piece(cv, h, hx, top + head, sw, s1 * 3, s1, c, pat, flat);
        }
        let nx = g.fx + g.cols * cell + 18;
        let n = q.cfg.preview.min(q.queue.len()) as i32;
        if n > 0 {
            side_box(cv, glassy, nx, top, sw, head + s1 * 3 + (n - 1) * s2 * 3 + 4, "NEXT", &l);
            for (k, &t) in q.queue.iter().take(n as usize).enumerate() {
                let c = theme_tint(st.theme, qd_color(t + 1));
                let (s, y) = if k == 0 { (s1, top + head) } else { (s2, top + head + s1 * 3 + (k as i32 - 1) * s2 * 3) };
                self.mini_piece(cv, t, nx, y, sw, s * 3, s, c, pat, flat);
            }
        }
    }

    /// A piece centred in the slot (x, y, w, h), drawn with cells of size `s`.
    #[allow(clippy::too_many_arguments)]
    fn mini_piece(&mut self, cv: &mut Canvas, t: u8, x: i32, y: i32, w: i32, h: i32, s: i32, c: u32, pat: bool, flat: bool) {
        let shape = &qd::SHAPES[t as usize][0];
        let (x0, x1) = (shape.iter().map(|p| p.0).min().unwrap(), shape.iter().map(|p| p.0).max().unwrap() + 1);
        let (y0, y1) = (shape.iter().map(|p| p.1).min().unwrap(), shape.iter().map(|p| p.1).max().unwrap() + 1);
        let (ox, oy) = (x + (w - (x1 - x0) * s) / 2, y + (h - (y1 - y0) * s) / 2);
        for &(dx, dy) in shape.iter() {
            let spr = self.sprites.get(SpriteKey { kind: CellKind::Gem, color: c, size: s, face: false, pattern: pat && s >= 12, flat, frame: 0 });
            cv.blit(spr, ox + (dx - x0) * s, oy + (dy - y0) * s, 255);
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
                    let neck = cf * 0.5;
                    if x + 1 < ch::W && c.grid[i + 1] == v && !(popping && c.pop_cells.contains(&(i as i32 + 1))) {
                        let (nx, ny, nw, nh) = (px + cf * 0.5, py + (cf - neck) / 2.0, cf, neck);
                        cv.round_grad(nx as i32, ny as i32, nw as i32, nh as i32, (nh / 2.0) as i32, lighten(col, 40), darken(col, 60), 255);
                        cv.round_rect(nx as i32 + nw as i32 / 4, ny as i32 + (nh / 5.0) as i32, nw as i32 / 2, (nh / 5.0).max(1.0) as i32, 2, 0xFFFF_FFFF, 70);
                    }
                    if y + 1 < ch::H && c.grid[i + ch::W as usize] == v && !(popping && c.pop_cells.contains(&(i as i32 + ch::W))) {
                        let (nx, ny, nw, nh) = (px + (cf - neck) / 2.0, py + cf * 0.5, neck, cf);
                        cv.round_grad(nx as i32, ny as i32, nw as i32, nh as i32, (nw / 2.0) as i32, col, darken(col, 40), 255);
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
        let glassy = st.theme == ThemeId::Voyage && st.preset != Preset::Potato;
        let (sw, top) = (g.side, g.fy - 8);
        let nx = g.fx + g.cols * cell + 18;
        let s1 = cell.min(sw - 14).max(4);
        let s2 = (s1 * 3 / 4).max(4);
        let head = side_head(sw, "NEXT");
        let pairs = c.next_pairs();
        let gap = (s1 / 4).max(3);
        let bh = head + s1 * 2 + (pairs.len() as i32 - 1).max(0) * (s2 * 2 + gap) + 6;
        side_box(cv, glassy, nx, top, sw, bh, "NEXT", &l);
        for (k, (a, b)) in pairs.iter().enumerate() {
            let s = if k == 0 { s1 } else { s2 };
            let y0 = top + head + if k == 0 { 0 } else { s1 * 2 + gap + (k as i32 - 1) * (s2 * 2 + gap) };
            for (j, col) in [b, a].iter().enumerate() {
                let colr = theme_tint(st.theme, chain_color(**col));
                let spr = self.sprites.get(SpriteKey { kind: CellKind::Blob, color: colr, size: s, face: faces && s >= 16, pattern: st.colorblind, flat, frame: 0 });
                cv.blit(spr, nx + (sw - s) / 2, y0 + j as i32 * s, 255);
            }
        }
        if c.all_clear_bonus {
            side_value(cv, glassy, nx, top + bh + 8, sw, "BONUS", "+30", None, l.good, &l);
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
        let glassy = st.theme == ThemeId::Voyage && st.preset != Preset::Potato;
        let (sw, top) = (g.side, g.fy - 8);
        let nx = g.fx + g.cols * cell + 18;
        let s = cell.min((sw - 12) / 2).max(4);
        let head = side_head(sw, "NEXT");
        let bh = head + s + 8;
        side_box(cv, glassy, nx, top, sw, bh, "NEXT", &l);
        if let Some((a, b)) = c.preview() {
            for (j, (col, kind)) in [(a, CellKind::HalfL), (b, CellKind::HalfR)].iter().enumerate() {
                let colr = theme_tint(st.theme, capsule_color(*col));
                let spr = self.sprites.get(SpriteKey { kind: *kind, color: colr, size: s, face: faces, pattern: st.colorblind, flat, frame: 0 });
                cv.blit(spr, nx + sw / 2 - s + j as i32 * s, top + head + 1, 255);
            }
        }
        let mut y = top + bh + 8;
        y = side_value(cv, glassy, nx, y, sw, "VIRUS", &format!("{}", c.viruses), None, l.accent, &l);
        y = side_value(cv, glassy, nx, y, sw, "LEVEL", &format!("{}", c.level), Some(&crate::ui::title_case(cap::SPEED_NAMES[c.speed as usize])), l.text, &l);
        if c.nes_seeds {
            side_value(cv, glassy, nx, y, sw, "SEED", &format!("{:04X}", c.nes_seed), None, l.dim, &l);
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
            // a slab of frosted stone: dark rim, bevelled edge, a cool face and a gloss streak
            let rr = (cell / 4).max(3);
            cv.round_rect(x0 + 1, y0 + 1, w - 2, hgt - 2, rr, 0xFF23_2638, 255);
            cv.round_grad(x0 + 2, y0 + 2, w - 4, hgt - 4, rr - 1, hex(0xC9CEE0), hex(0x5A6078), 255);
            let bev = (cell / 9).max(2);
            cv.round_grad(x0 + 2 + bev, y0 + 2 + bev, w - 4 - 2 * bev, hgt - 4 - 2 * bev, (rr - bev).max(2), hex(0xA4AAC2), hex(0x787E98), 255);
            cv.round_rect(x0 + cell / 4, y0 + 2 + bev + 2, w - cell / 2, (cell / 8).max(2), cell / 16 + 1, 0xFFFF_FFFF, 90);
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
        let glassy = st.theme == ThemeId::Voyage && st.preset != Preset::Potato;
        let (sw, top) = (g.side, g.fy - 8);
        let nx = g.fx + g.cols * cell + 18;
        let mut y = top;
        if p.puzzle {
            y = side_value(cv, glassy, nx, y, sw, "MOVES", &format!("{}", p.moves_left.max(0)), None, l.accent, &l);
        } else {
            y = side_value(cv, glassy, nx, y, sw, "LEVEL", &format!("{}", p.level), None, l.text, &l);
        }
        if p.chain > 1 {
            y = side_value(cv, glassy, nx, y, sw, "CHAIN", &format!("x{}", p.chain), None, l.accent, &l);
        }
        if p.stop > 0 {
            // the stack is held: a bar that drains as the stop time runs out
            let head = side_head(sw, "STOP");
            side_box(cv, glassy, nx, y, sw, head + 16, "STOP", &l);
            let bw = sw - 16;
            cv.round_rect(nx + 8, y + head + 2, bw, 6, 3, 0xFF05_0814, 220);
            cv.round_rect(nx + 8, y + head + 2, (p.stop.min(240) as i32 * bw / 240).max(6), 6, 3, l.good, 255);
        }
    }
}

fn qd_color(v: u8) -> u32 {
    quad_color(v)
}
fn color_for(style: Style, c: u8) -> u32 {
    match style {
        Style::Quad => quad_color(c),
        Style::Chain => chain_color(c),
        Style::Capsule => capsule_color(c),
        Style::Panel => panel_color(c),
    }
}

fn cell_pos(style: Style, g: &Geom, x: i32, y: i32, b: Option<&Board>) -> (f32, f32) {
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

fn stat_pairs(m: &Match, i: usize) -> Vec<(&'static str, String)> {
    let s = &m.seats[i];
    match s.board.as_ref().unwrap() {
        Board::Quad(q) => {
            if m.header.mode.is_solo() {
                vec![("Lines", q.lines.to_string()), ("Level", q.level.to_string())]
            } else {
                vec![("Lines", q.lines.to_string()), ("Sent", s.stats.sent.to_string())]
            }
        }
        Board::Chain(c) => vec![("Score", c.score.to_string()), ("Target", c.target.to_string())],
        Board::Capsule(c) => vec![("Score", c.score.to_string())],
        Board::Panel(p) => vec![("Score", p.score.to_string()), ("Best", format!("x{}", p.max_chain))],
    }
}

/// The backing of every HUD element: glass on Voyage, a plain dark card on the flat themes.
fn hud_plate(cv: &mut Canvas, glassy: bool, x: i32, y: i32, w: i32, h: i32, r: i32) {
    if glassy {
        cv.glass(x, y, w, h, r, crate::ui::GLASS, 190);
    } else {
        cv.round_rect(x, y, w, h, r.min(6), 0xFF0A_0E1E, 220);
    }
}

/// Height of a side box's caption strip (narrow columns drop the caption).
fn side_head(w: i32, label: &str) -> i32 {
    if Canvas::text_wb(label, 1, Weight::Bold) <= w - 4 { 23 } else { 6 }
}

/// A framed box in the side column with a small caption on top.
#[allow(clippy::too_many_arguments)]
fn side_box(cv: &mut Canvas, glassy: bool, x: i32, y: i32, w: i32, h: i32, label: &str, l: &Look) {
    hud_plate(cv, glassy, x, y, w, h, (w / 6).clamp(5, 14));
    let lw = Canvas::text_wb(label, 1, Weight::Bold);
    if lw <= w - 4 {
        cv.text_weight(x + (w - lw) / 2, y + 6, label, 1, l.dim, 255, Weight::Bold);
    }
}

/// A captioned number in the side column (virus count, level, chain...). Returns the next y.
#[allow(clippy::too_many_arguments)]
fn side_value(cv: &mut Canvas, glassy: bool, x: i32, y: i32, w: i32, label: &str, value: &str, sub: Option<&str>, col: u32, l: &Look) -> i32 {
    let head = side_head(w, label);
    let mut vs = if w >= 70 { 3 } else if w >= 44 { 2 } else { 1 };
    while vs > 1 && Canvas::text_wb(value, vs, Weight::Bold) > w - 8 {
        vs -= 1;
    }
    let vh = (Canvas::px(vs) * 0.92) as i32;
    let sub = sub.filter(|s| Canvas::text_w(s, 1) <= w - 6);
    let h = head + vh + if sub.is_some() { 15 } else { 0 } + 6;
    side_box(cv, glassy, x, y, w, h, label, l);
    let vw = Canvas::text_wb(value, vs, Weight::Bold);
    cv.text_weight(x + (w - vw) / 2, y + head - 2, value, vs, col, 255, Weight::Bold);
    if let Some(s) = sub {
        cv.text_a(x + (w - Canvas::text_w(s, 1)) / 2, y + head + vh - 1, s, 1, l.dim, 255);
    }
    y + h + 8
}

/// Shorten text with an ellipsis until it fits `maxw`.
fn fit_text(s: &str, scale: i32, w: Weight, maxw: i32) -> String {
    if Canvas::text_wb(s, scale, w) <= maxw {
        return s.to_string();
    }
    let mut cs: Vec<char> = s.chars().collect();
    while cs.pop().is_some() {
        let t = format!("{}..", cs.iter().collect::<String>().trim_end());
        if Canvas::text_wb(&t, scale, w) <= maxw {
            return t;
        }
    }
    String::new()
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
