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
    /// where it pops, relative to the field's top-left (None: a third of the way down)
    at: Option<(f32, f32)>,
}

/// Where the falling piece will land (board cells), where it is now, and its colour.
#[derive(Clone, Default)]
pub struct Landing {
    pub cells: Vec<(i32, i32)>,
    pub now: Vec<(i32, i32)>,
    pub color: u32,
}

#[derive(Clone, Copy)]
enum MarkKind {
    /// a cell that just locked glows and fades
    Flash,
    /// a hard-drop trail: a column of light from y0 down to the cell
    Streak(f32),
    /// a shockwave ring from a big clear
    Ring(f32),
}
struct Mark {
    seat: usize,
    kind: MarkKind,
    /// field-relative layout position and cell size when it was made
    x: f32,
    y: f32,
    w: f32,
    color: u32,
    t: f32,
    life: f32,
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
    /// per seat: combo / chain heat (0..1), the board's hard-drop dip, a garbage shove,
    /// and a white flash after a big clear
    pub heat: [f32; 6],
    pub kick: [f32; 6],
    pub shove: [f32; 6],
    pub bright: [f32; 6],
    marks: Vec<Mark>,
    /// the falling piece as last drawn, per seat
    pub last: [Option<Landing>; 6],
    clock: f32,
}
impl Fx {
    pub fn new() -> Self {
        Fx {
            particles: Vec::new(),
            callouts: Vec::new(),
            shake: 0.0,
            flash: [0.0; 6],
            energy: 0.0,
            seed: 77,
            heat: [0.0; 6],
            kick: [0.0; 6],
            shove: [0.0; 6],
            bright: [0.0; 6],
            marks: Vec::new(),
            last: Default::default(),
            clock: 0.0,
        }
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
            let life = 0.3 + self.rnd() * 0.4;
            let size = 1.5 + self.rnd() * 2.5;
            self.particles.push(Particle { x, y, vx: a.cos() * sp, vy: a.sin() * sp - 1.2, life, max: life, color, size });
        }
    }
    pub fn callout(&mut self, seat: usize, text: String, color: u32, scale: i32) {
        self.callout_at(seat, text, color, scale, None);
    }
    pub fn callout_at(&mut self, seat: usize, text: String, color: u32, scale: i32, at: Option<(f32, f32)>) {
        if text.is_empty() {
            return;
        }
        self.callouts.retain(|c| c.seat != seat);
        self.callouts.push(Callout { seat, text, t: 0.0, color, scale, at });
    }
    fn mark(&mut self, seat: usize, kind: MarkKind, x: f32, y: f32, w: f32, color: u32, life: f32) {
        if self.marks.len() < 400 {
            self.marks.push(Mark { seat, kind, x, y, w, color, t: 0.0, life });
        }
    }
    pub fn update(&mut self, dt: f32) {
        for p in self.particles.iter_mut() {
            p.x += p.vx * dt * 60.0;
            p.y += p.vy * dt * 60.0;
            p.vy += 0.09 * dt * 60.0;
            p.vx *= 0.94;
            p.vy *= 0.96;
            p.life -= dt;
        }
        self.particles.retain(|p| p.life > 0.0);
        for c in self.callouts.iter_mut() {
            c.t += dt;
        }
        self.callouts.retain(|c| c.t < 1.5);
        for mk in self.marks.iter_mut() {
            mk.t += dt;
        }
        self.marks.retain(|mk| mk.t < mk.life);
        self.shake *= 0.85;
        let k = (-dt * 13.0).exp();
        for i in 0..6 {
            self.flash[i] *= 0.9;
            self.heat[i] = (self.heat[i] - dt * 0.22).max(0.0);
            self.kick[i] *= k;
            self.shove[i] *= (-dt * 7.0).exp();
            self.bright[i] *= (-dt * 6.0).exp();
        }
        self.clock += dt;
        self.energy *= 0.995;
    }
    /// The board's offset this frame: a dip after a hard drop, a wobble when garbage lands.
    pub fn offset(&self, i: usize) -> (i32, i32) {
        ((self.shove[i] * (self.clock * 48.0).sin()) as i32, self.kick[i] as i32)
    }
    pub fn draw(&self, cv: &mut Canvas, geoms: &[Geom; 6]) {
        for p in &self.particles {
            let a = (p.life / p.max * 255.0) as u32;
            // a spark: a short streak along its motion with a hot core
            cv.glow(p.x, p.y, p.size * 3.2, p.color, a / 4);
            cv.line(p.x - p.vx * 1.6, p.y - p.vy * 1.6, p.x, p.y, p.size * 0.7, lighten(p.color, 90), a * 3 / 4);
            cv.circle(p.x, p.y, p.size * 0.55, lighten(p.color, 170), a);
        }
        for c in &self.callouts {
            let g = geoms[c.seat];
            if g.cell == 0 {
                continue;
            }
            // pop in big, settle with a little overshoot, hold, then drift up and fade
            let t = c.t;
            let pop = if t < 0.12 {
                1.55 - 0.65 * (t / 0.12)
            } else if t < 0.24 {
                0.9 + 0.1 * ((t - 0.12) / 0.12)
            } else {
                1.0
            };
            let a = if t > 1.1 { ((1.5 - t) / 0.4 * 255.0).max(0.0) as u32 } else { 255 };
            let rise = if t > 0.6 { (t - 0.6) * 34.0 } else { 0.0 };
            let (cx, cy) = match c.at {
                Some((x, y)) => (g.fx as f32 + x, g.fy as f32 + y),
                None => ((g.fx + g.cols * g.cell / 2) as f32, (g.fy + g.rows * g.cell / 3) as f32),
            };
            let txt = crate::ui::nice(&c.text);
            let mut px = Canvas::px(c.scale);
            let maxw = (g.cols * g.cell) as f32 * 1.05;
            while px > 10.0 && crate::font::measure(&txt, px, Weight::Black) > maxw {
                px -= 1.0;
            }
            let px = px * pop;
            let w = crate::font::measure(&txt, px, Weight::Black);
            let fwf = (g.cols * g.cell) as f32;
            // keep it over its own board; a callout wider than the board stays centred on it
            let (lo, hi) = (g.fx as f32 - g.cell as f32 * 0.4, g.fx as f32 + fwf - w + g.cell as f32 * 0.4);
            let x0 = if hi >= lo { (cx - w / 2.0).clamp(lo, hi) } else { g.fx as f32 + (fwf - w) / 2.0 };
            let base = cy - rise + px * 0.3;
            cv.glow(x0 + w / 2.0, base - px * 0.35, w * 0.55 + 12.0, c.color, a / 4);
            let o = (px / 14.0).max(1.5);
            for (dx, dy) in [(-1.0, 0.0), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0), (-0.7, -0.7), (0.7, 0.7), (-0.7, 0.7), (0.7, -0.7), (0.0, 1.6)] {
                crate::font::draw(cv, x0 + dx * o, base + dy * o, &txt, px, Weight::Black, 0xFF0A_0E1E, a * 4 / 5);
            }
            crate::font::draw(cv, x0, base, &txt, px, Weight::Black, c.color, a);
            // a bright sheen on the upper half of the letters
            cv.set_clip(x0 as i32 - 2, (base - px * 0.74) as i32, w as i32 + 4, (px * 0.3) as i32);
            crate::font::draw(cv, x0, base, &txt, px, Weight::Black, lighten(c.color, 150), a / 2);
            cv.reset_clip();
        }
    }
    /// Lock flashes, drop trails and shockwaves (drawn inside the well, over the pieces).
    fn draw_marks(&self, cv: &mut Canvas, seat: usize, g: &Geom) {
        for mk in self.marks.iter().filter(|m| m.seat == seat) {
            let k = 1.0 - mk.t / mk.life;
            let (x, y) = (g.fx as f32 + mk.x, g.fy as f32 + mk.y);
            match mk.kind {
                MarkKind::Flash => {
                    let w = mk.w as i32;
                    cv.round_rect(x as i32 + 1, y as i32 + 1, w - 2, w - 2, w / 5, 0xFFFF_FFFF, (k * k * 210.0) as u32);
                }
                MarkKind::Streak(y0) => {
                    let top = g.fy as f32 + y0;
                    let len = (y - top).max(0.0);
                    if len < 1.0 {
                        continue;
                    }
                    let steps = 8;
                    for s in 0..steps {
                        let f = s as f32 / steps as f32;
                        let yy = top + len * f;
                        let a = (k * f * f * 120.0) as u32;
                        let inset = mk.w * (0.18 + 0.12 * (1.0 - f));
                        cv.rect_a((x + inset) as i32, yy as i32, (mk.w - inset * 2.0) as i32, (len / steps as f32).ceil() as i32 + 1, mk.color, a);
                    }
                    cv.rect_a((x + mk.w * 0.45) as i32, top as i32, (mk.w * 0.1).max(1.0) as i32, len as i32, 0xFFFF_FFFF, (k * 90.0) as u32);
                }
                MarkKind::Ring(r) => {
                    let rr = r * (0.25 + 0.75 * (1.0 - k * k));
                    cv.ring(x, y, rr, 2.0 + 5.0 * k, mk.color, (k * 200.0) as u32);
                    cv.ring(x, y, rr * 0.8, 1.5, 0xFFFF_FFFF, (k * 120.0) as u32);
                }
            }
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
    let f = frame_w(bw);
    (f + if style == Style::Quad { sw + 2 } else { 4 }, f + 10 + sw + 4)
}
/// Space kept above and below the well, the nameplate height and its lift above the well.
/// When any Quad board plays, every plate is lifted clear of Quad's two spawn rows so the
/// plates line up across the match.
fn vert(bw: i32, any_quad: bool) -> (i32, i32, i32, i32) {
    let f = frame_w(bw);
    let bar = bar_h(bw);
    // above: the HOLD / NEXT words that sit on the frame's top edge (and Quad's spawn rows);
    // below: the half of the score that hangs under the frame, the name bar, a line of info
    let top = if any_quad { (bw / 5 + 4).max(f + label_px(bw) as i32 / 2 + 4) } else { f + label_px(bw) as i32 / 2 + 4 };
    (top, f + score_px(bw) as i32 / 2 + 8 + bar + 18, bar, 0)
}
fn bar_h(bw: i32) -> i32 {
    if bw >= 200 { 38 } else { 26 }
}
fn label_px(bw: i32) -> f32 {
    if bw >= 200 { 21.0 } else { 14.0 }
}
fn score_px(bw: i32) -> f32 {
    (bw as f32 * 0.15).clamp(14.0, 38.0)
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

const WELL_PAD: i32 = 34;

/// Thickness of the coloured frame around a well, and the frame plus its ink outline.
fn bezel_t(fw: i32) -> i32 {
    (fw / 19).clamp(8, 16)
}

/// Each seat's colour: its frame, its name bar and the light behind its board.
pub fn seat_color(i: usize) -> u32 {
    [0xFF2F_7BFF, 0xFFF0_3C8C, 0xFF22_B864, 0xFFFF_8A1F, 0xFF9B_5CFF, 0xFF16_B8CF][i % 6]
}

/// The garbage meter's groove in the frame's left bar: (x offset from the field, width).
fn meter_slot(fw: i32) -> (i32, i32) {
    let t = bezel_t(fw);
    let w = ((t - 3) * 6 / 10).max(3);
    (-t + (t - 3 - w) / 2, w)
}
fn frame_w(fw: i32) -> i32 {
    bezel_t(fw) + 3
}
/// The ink of the logo: every toy-like part of the match HUD is outlined in it.
const INK: u32 = 0xFF24_1A3C;

/// Each style's colour.
pub fn style_accent(s: Style) -> u32 {
    match s {
        Style::Quad => CYAN,
        Style::Chain => GREEN,
        Style::Capsule => PINK,
        Style::Panel => YELLOW,
    }
}

/// The cabinet around a well, in the seat's colour: ink outline, a chunky glossy frame with a
/// light inner bevel and halftone dots toward the bottom, the garbage meter's groove in the
/// left bar, then a dark well tinted toward the seat's colour with a faint grid. Painted once
/// in real pixels at scale `k`, offset by WELL_PAD.
#[allow(clippy::too_many_arguments)]
fn paint_well(k: f32, fw: i32, fh: i32, cell: i32, cols: i32, rows: i32, style: Style, color: u32, r: i32) -> Canvas {
    let p = WELL_PAD;
    let mut c = Canvas::new(((fw + 2 * p) as f32 * k).round() as i32, ((fh + 2 * p) as f32 * k).round() as i32);
    c.k = k;
    let t = bezel_t(fw);
    let (ox, oy, ow, oh, orr) = (p - t, p - t, fw + 2 * t, fh + 2 * t, r + t);
    c.soft_shadow(ox - 3, oy + 7, ow + 6, oh + 6, orr + 3, 15, 210);
    c.round_rect(ox - 3, oy - 3, ow + 6, oh + 6, orr + 3, INK, 255);
    c.round_grad(ox, oy, ow, oh, orr, lighten(color, 110), darken(color, 35), 255);
    // halftone: dots that grow toward the bottom of the frame
    let step = (t / 3).max(4);
    let y_from = oy + oh * 55 / 100;
    let mut y = y_from;
    while y < oy + oh - 2 {
        let f = (y - y_from) as f32 / (oy + oh - y_from) as f32;
        let rad = step as f32 * (0.12 + 0.30 * f);
        let shift = if ((y - y_from) / step) % 2 == 0 { 0 } else { step / 2 };
        let mut x = ox + 3 + shift;
        while x < ox + ow - 3 {
            let inside = x > p - 4 && x < p + fw + 4 && y > p - 4 && y < p + fh + 4;
            if !inside {
                c.circle(x as f32, y as f32, rad, 0xFFFF_FFFF, 70);
            }
            x += step;
        }
        y += step;
    }
    // a light bevel just inside the outer edge, a darker one where the frame turns into the well
    c.round_ring(ox + 2, oy + 2, ow - 4, oh - 4, orr - 2, 2, lighten(color, 200), lighten(color, 60), 220);
    c.round_ring(p - 7, p - 7, fw + 14, fh + 14, r + 5, 3, darken(color, 70), lighten(color, 90), 200);
    // the logo's gloss bar and dot along the top edge
    let gb = (t / 3).max(2);
    let run = ow - 2 * orr;
    c.round_rect(ox + orr, oy + 3, run * 3 / 5, gb, gb / 2 + 1, 0xFFFF_FFFF, 210);
    c.round_rect(ox + orr + run * 3 / 5 + gb * 2, oy + 3, gb * 2, gb, gb / 2 + 1, 0xFFFF_FFFF, 210);
    // the garbage meter's groove in the left bar
    let (mx, mw) = meter_slot(fw);
    c.round_rect(p + mx - 1, p - 1, mw + 2, fh + 2, (mw + 2) / 2, INK, 230);
    // the well
    c.round_rect(p - 3, p - 3, fw + 6, fh + 6, r + 1, INK, 255);
    c.round_grad(p, p, fw, fh, r - 2, mix(0xFF0C_1132, color, 46), mix(0xFF04_050E, color, 24), 255);
    c.set_clip(p, p, fw, fh);
    let ga = if style == Style::Chain || style == Style::Capsule { 12 } else { 18 };
    for x in 1..cols {
        c.rect_a(p + x * cell, p, 1, fh, 0xFFFF_FFFF, ga);
    }
    for y in 1..rows {
        c.rect_a(p, p + y * cell, fw, 1, 0xFFFF_FFFF, ga);
    }
    for i in 0..6 {
        c.rect_a(p, p + i * 3, fw, 3, 0xFF00_0000, (90 - i * 15) as u32);
    }
    for i in 0..10 {
        c.rect_a(p, p + fh - (i + 1) * fh / 40, fw, fh / 40, lighten(color, 40), (26 - i * 2) as u32);
    }
    c.reset_clip();
    // the inner ink rim goes on last: its rounded inside masks the shading in the corners
    c.round_ring(p - 3, p - 3, fw + 6, fh + 6, r + 1, 3, INK, INK, 255);
    c
}

/// The stage each board stands on, baked into the match backdrop: a coloured spotlight, a pool
/// of light and a shadow on the floor, and a dark station panel that holds the board, its side
/// boxes, its nameplate and its stats together.
pub fn paint_arena(cv: &mut Canvas, m: &Match, geoms: &[Geom; 6]) {
    for (i, g) in geoms.iter().enumerate() {
        if g.cell == 0 || m.seats[i].board.is_none() {
            continue;
        }
        let accent = seat_color(i);
        let (fw, fh) = (g.cols * g.cell, g.rows * g.cell);
        let pill_h = g.plate_h;
        let bt = bezel_t(fw);
        let top = g.fy - g.lift - g.plate_h - 12;
        let bot = g.fy + fh + bt + score_px(fw) as i32 / 2 + 8 + pill_h + 6;
        let (x0, x1) = (g.x - 6, g.x + g.w + 6);
        let (cx, cy) = ((g.fx + fw / 2) as f32, (g.fy + fh / 2) as f32);
        // a backlight in the style's colour and a shadow on the floor
        cv.glow(cx, cy, fh as f32 * 0.8, accent, 46);
        cv.glow(cx, g.fy as f32 + fh as f32 * 0.15, fw as f32 * 1.0, 0xFFFF_FFFF, 10);
        let pw = (x1 - x0) as f32;
        cv.ellipse(cx, bot as f32 - 4.0, pw * 0.48, 14.0, 0xFF00_0000, 120);
        cv.ellipse(cx, bot as f32 - 6.0, pw * 0.36, 7.0, accent, 46);
        let _ = top;
    }
}

pub struct Renderer {
    pub sprites: Sprites,
    pub scene: Scene,
    pub fx: Fx,
    pub geoms: [Geom; 6],
    wells: std::collections::HashMap<(i32, i32, i32, u32, u32, u8), Canvas>,
    /// where each seat's side column has got to (the next free y)
    side_y: [i32; 6],
}

impl Renderer {
    pub fn new() -> Self {
        Renderer { sprites: Sprites::new(), scene: Scene::new(), fx: Fx::new(), geoms: [Geom::default(); 6], wells: std::collections::HashMap::new(), side_y: [0; 6] }
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
                    // the callout pops where the clear happened
                    let at = m.seats[s].board.as_ref().and_then(|b| hot_spot(b, &g));
                    self.fx.callout_at(s, text.clone(), if *power >= 4 { l.accent } else { l.text }, scale, at);
                    self.fx.energy = (self.fx.energy + *power as f32 * 0.06).min(1.0);
                    self.fx.heat[s] = (self.fx.heat[s] + *power as f32 * 0.07).min(1.0);
                    if *power >= 4 && cap > 0 {
                        let (x, y) = at.unwrap_or(((g.cols * g.cell) as f32 / 2.0, (g.rows * g.cell) as f32 / 2.0));
                        let col = match style {
                            Style::Quad => CYAN,
                            Style::Chain => GREEN,
                            Style::Capsule => PINK,
                            Style::Panel => YELLOW,
                        };
                        self.fx.mark(s, MarkKind::Ring((g.cols * g.cell) as f32 * 0.7), x, y, 0.0, col, 0.5);
                        self.fx.bright[s] = (*power as f32 / 10.0).min(1.0);
                    }
                    if st.shake && *power >= 6 {
                        self.fx.shake = 6.0;
                    }
                }
                Ev::HardDrop if cap > 0 => {
                    // a trail from where the piece was down to where it landed, and a dip
                    if let Some(ld) = self.fx.last[s].clone() {
                        let c = g.cell as f32;
                        for &(x, y) in &ld.cells {
                            if ld.cells.contains(&(x, y - 1)) {
                                continue;
                            }
                            let top = ld.now.iter().filter(|p| p.0 == x).map(|p| p.1).min().unwrap_or(y);
                            self.fx.mark(s, MarkKind::Streak(top as f32 * c), x as f32 * c, y as f32 * c, c, ld.color, 0.28);
                        }
                    }
                    self.fx.kick[s] = (g.cell as f32 / 4.0).max(3.0);
                }
                Ev::Lock { .. } if cap > 0 => {
                    if let Some(ld) = self.fx.last[s].clone() {
                        let c = g.cell as f32;
                        for &(x, y) in &ld.cells {
                            self.fx.mark(s, MarkKind::Flash, x as f32 * c, y as f32 * c, c, 0xFFFF_FFFF, 0.22);
                        }
                    }
                    self.fx.kick[s] = self.fx.kick[s].max(1.5);
                }
                Ev::AllClear => self.fx.callout(s, "ALL CLEAR!".into(), look(st.theme).good, (g.cell / 9).max(2)),
                Ev::LevelClear => self.fx.callout(s, "BOTTLE CLEAR!".into(), look(st.theme).good, (g.cell / 9).max(2)),
                Ev::GarbageLand { .. } => {
                    self.fx.flash[s] = 1.0;
                    if !st.reduce_motion {
                        self.fx.shove[s] = (g.cell as f32 / 5.0).max(2.0);
                    }
                }
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
        if !v.settings.reduce_motion {
            for i in 0..6 {
                let (dx, dy) = self.fx.offset(i);
                self.geoms[i].fx += dx;
                self.geoms[i].fy += dy;
            }
        }
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
        let color = seat_color(i);
        let glassy = st.theme == ThemeId::Voyage && st.preset != Preset::Potato;
        let deco = glassy.then_some(color);
        let danger = board.danger() && seat.alive;
        let pulse = ((v.t * 6.0).sin() * 0.5 + 0.5) as f32;
        let fwid = frame_w(fw);
        let r = (cell / 3).clamp(8, 16);
        if glassy {
            // the cabinet never changes during a match, so it is painted once and reused
            let sk = match style {
                Style::Quad => 0,
                Style::Chain => 1,
                Style::Capsule => 2,
                Style::Panel => 3,
            };
            let key = (fw, fh, cell, color, cv.k.to_bits(), sk);
            if self.wells.len() > 24 {
                self.wells.clear();
            }
            if style == Style::Quad {
                let (hx, hy, hw, hh, _) = hold_rect(&g);
                hold_tab(cv, deco, hx, hy, hw, hh, &l);
            }
            let wc = self.wells.entry(key).or_insert_with(|| paint_well(cv.k, fw, fh, cell, g.cols, g.rows, style, color, r));
            cv.blit(wc, g.fx - WELL_PAD, g.fy - WELL_PAD, 255);
            // live light on the frame: it heats up with combos and breathes red in danger
            let edge = if danger { mix(color, l.warn, (120.0 + 135.0 * pulse) as u32) } else { lighten(color, 80) };
            let heat = self.fx.heat[i];
            let halo = if danger { (50.0 + 70.0 * pulse) as u32 } else { (heat * 150.0) as u32 };
            if halo > 4 {
                cv.round_ring(g.fx - fwid - 5, g.fy - fwid - 5, fw + 2 * fwid + 10, fh + 2 * fwid + 10, r + fwid + 5, 6, edge, edge, halo);
            }
            if danger {
                cv.round_ring(g.fx - fwid + 3, g.fy - fwid + 3, fw + 2 * fwid - 6, fh + 2 * fwid - 6, r + fwid - 3, 3, lighten(edge, 80), edge, (120.0 + 120.0 * pulse) as u32);
            }
            if self.fx.bright[i] > 0.02 && !st.reduce_motion {
                cv.rect_a(g.fx, g.fy, fw, fh, 0xFFFF_FFFF, (self.fx.bright[i] * 50.0) as u32);
            }
        } else {
            if style == Style::Quad {
                let (hx, hy, hw, hh, _) = hold_rect(&g);
                hold_tab(cv, None, hx, hy, hw, hh, &l);
            }
            cv.frame(g.fx - 2, g.fy - 2, fw + 4, fh + 4, 2, if danger { l.warn } else if st.theme == ThemeId::Phosphor { l.dim } else { hex(0x555C6E) }, 255);
            cv.rect_a(g.fx, g.fy, fw, fh, l.well, l.well_alpha);
        }
        if self.fx.flash[i] > 0.05 && st.theme != ThemeId::Tournament && !st.reduce_motion {
            cv.rect_a(g.fx, g.fy, fw, fh, l.warn, (self.fx.flash[i] * 40.0) as u32);
        }
        let _s = crate::prof::scope("S:grid");
        // grid: hairlines on the flat themes (the glossy wells carry theirs in the cached cabinet)
        if !glassy {
            let grid_a = if style == Style::Chain || style == Style::Capsule { 40 } else { 70 };
            for x in 1..g.cols {
                cv.rect_a(g.fx + x * cell, g.fy, 1, fh, l.grid, grid_a);
            }
            for y in 1..g.rows {
                cv.rect_a(g.fx, g.fy + y * cell, fw, 1, l.grid, grid_a);
            }
        }
        // the side column starts level with the top of the frame
        self.side_y[i] = g.fy - fwid - side_head(g.side, "NEXT");
        // pieces may poke above the well only in Quad (its spawn rows); Chain pairs slide in
        if style == Style::Quad {
            cv.set_clip(g.fx, g.fy - cell * 2, fw, fh + cell * 2);
        } else {
            cv.set_clip(g.fx, g.fy, fw, fh);
        }
        let _s = crate::prof::scope("S:pieces");
        match board {
            Board::Quad(q) => self.draw_quad(cv, i, q, &g, faces, flat, st, v.ghost_on),
            Board::Chain(c) => self.draw_chain(cv, i, c, &g, faces, flat, st, v.t),
            Board::Capsule(c) => self.draw_capsule(cv, i, c, &g, faces, flat, st, v.t),
            Board::Panel(p) => self.draw_panel(cv, i, p, &g, faces, flat, st, v.t),
        }
        cv.set_clip(g.fx, g.fy - if style == Style::Quad { cell * 2 } else { 0 }, fw, fh + if style == Style::Quad { cell * 2 } else { 0 });
        self.fx.draw_marks(cv, i, &g);
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
        // garbage meter: on the glossy theme it fills the groove in the frame's left bar
        let (trav, tray) = m.incoming(i);
        let per_row = match style {
            Style::Quad => 1,
            Style::Chain => 6,
            Style::Capsule => 2,
            Style::Panel => 6,
        };
        let rows_tray = ((tray + per_row - 1) / per_row).min(g.rows as u32) as i32;
        let rows_trav = ((trav + per_row - 1) / per_row).min(g.rows as u32 - rows_tray as u32) as i32;
        let (mx, mw) = if glassy {
            let (o, w) = meter_slot(fw);
            (g.fx + o, w)
        } else {
            let w = (cell / 4).clamp(4, 8);
            (g.fx - 8 - w, w)
        };
        if !glassy {
            cv.round_rect(mx - 1, g.fy - 1, mw + 2, fh + 2, (mw + 2) / 2, 0xFF05_0814, 200);
        }
        if rows_trav > 0 {
            let y0 = g.fy + fh - (rows_tray + rows_trav) * cell;
            cv.round_rect(mx, y0, mw, rows_trav * cell, mw / 2, hex(0xFFB0_3B), 235);
        }
        if rows_tray > 0 {
            let pulse = ((v.t * 8.0).sin() * 0.5 + 0.5) * 70.0;
            cv.round_rect(mx, g.fy + fh - rows_tray * cell, mw, rows_tray * cell, mw / 2, l.warn, 255);
            cv.round_rect(mx, g.fy + fh - rows_tray * cell, mw, rows_tray * cell, mw / 2, 0xFFFF_FFFF, pulse as u32);
        }
        let _s = crate::prof::scope("S:plate");
        let big = fw >= 200;
        // the extra numbers (lines, sent, target...) go on cards at the foot of the side column
        let side_x = g.fx + fw + fwid + 10;
        let mut sy = self.side_y[i];
        for (lab, val) in stat_pairs(m, i).into_iter().filter(|p| p.0 != "Score") {
            sy = side_value(cv, deco, side_x, sy, g.side, &lab.to_uppercase(), &val, None, 0xFFFF_FFFF, &l);
        }
        // the score: big outlined digits over the frame's bottom edge
        let score = format!("{:08}", board.score());
        // big, but inside the board's width
        let mut spx = score_px(fw).min((2 * fwid + 2) as f32);
        while spx > 10.0 && chunky_w(&score, spx) > fw as f32 * 0.9 {
            spx -= 1.0;
        }
        let score_y = (g.fy + fh + fwid + 1) as f32;
        if glassy {
            chunky(cv, (g.fx + fw / 2) as f32, score_y, &score, spx, 0xFFFF_FFFF, 255);
        } else {
            let w = crate::font::measure(&score, spx, Weight::Bold);
            crate::font::draw(cv, (g.fx + fw / 2) as f32 - w / 2.0, score_y + spx * 0.35, &score, spx, Weight::Bold, l.text, 255);
        }
        // the name bar under the board, in the seat's colour
        let bar = g.plate_h;
        let (bx, bw) = (g.fx - fwid, fw + 2 * fwid);
        let by = g.fy + fh + fwid + spx as i32 / 2 + 8;
        let br = bar / 2;
        if glassy {
            cv.soft_shadow(bx, by + 3, bw, bar, br, 8, 150);
            cv.round_rect(bx - 3, by - 3, bw + 6, bar + 6, br + 3, INK, 255);
            cv.round_grad(bx, by, bw, bar, br, lighten(color, 100), darken(color, 30), 255);
            cv.round_ring(bx + 2, by + 2, bw - 4, bar - 4, br - 2, 2, 0xFFFF_FFFF, lighten(color, 120), 200);
            cv.round_rect(bx + br, by + 4, (bw - 2 * br) / 2, (bar / 8).max(2), 2, 0xFFFF_FFFF, 170);
        } else {
            cv.round_rect(bx, by, bw, bar, 6, 0xFF0A_0E1E, 230);
        }
        let left = bx + br;
        // the style chip at the right end
        let sname = crate::ui::title_case(style.name());
        let chip_w = Canvas::text_wb(&sname, 1, Weight::Bold) + 14;
        let chip_h = Canvas::px(1) as i32 + 4;
        let right = bx + bw - br / 2 - 4;
        let show_chip = big || bw >= 150;
        if show_chip {
            let (cx0, cy0) = (right - chip_w, by + (bar - chip_h) / 2);
            cv.round_rect(cx0 - 1, cy0 - 1, chip_w + 2, chip_h + 2, chip_h / 2 + 1, INK, 255);
            cv.round_rect(cx0, cy0, chip_w, chip_h, chip_h / 2, style_accent(style), 255);
            cv.text_weight(cx0 + 7, cy0 + 1, &sname, 1, INK, 255, Weight::Bold);
        }
        // the name, centred in the room that is left
        let name = crate::ui::nice(&v.names[i]);
        let room_r = if show_chip { right - chip_w - 6 } else { right };
        let mut npx = (bar as f32 * 0.56).min(24.0);
        while npx > 9.0 && crate::font::measure(&name, npx, Weight::Display) > (room_r - left - 4) as f32 {
            npx -= 1.0;
        }
        let ncx = (left + room_r) as f32 / 2.0;
        if glassy {
            chunky(cv, ncx, (by + bar / 2) as f32, &name, npx, if v.humans[i] { 0xFFFF_F3A0 } else { 0xFFFF_FFFF }, 255);
        } else {
            let w = crate::font::measure(&name, npx, Weight::Bold);
            crate::font::draw(cv, ncx - w / 2.0, (by + bar / 2) as f32 + npx * 0.35, &name, npx, Weight::Bold, if v.humans[i] { l.accent } else { l.text }, 255);
        }
        // under the bar: bot info and KO stars on the left, the current target on the right
        let line = by + bar + 5;
        let stars: String = std::iter::repeat('\u{2605}').take(seat.kos.min(8) as usize).collect();
        let mut info = String::new();
        if seat.cfg.kind == SeatKind::Bot {
            info = if big { format!("{}   Lv {}", crate::ui::title_case(seat.cfg.personality.name()), seat.cfg.strength) } else { format!("Lv {}", seat.cfg.strength) };
        }
        if !stars.is_empty() {
            info = if info.is_empty() { stars } else { format!("{}  {}", info, stars) };
        }
        let mut room = bw - 8;
        if m.active_count() > 2 && seat.alive && seat.target != 0xFF {
            let t = format!("\u{2192} {}", crate::ui::nice(&v.names[seat.target as usize]));
            if Canvas::text_w(&t, 1) <= room {
                cv.text_right(bx + bw - 4, line, &t, 1, 0xFFFF_FFFF);
                room -= Canvas::text_w(&t, 1) + 8;
            }
        }
        if !info.is_empty() && Canvas::text_w(&info, 1) <= room {
            cv.text_s(bx + 4, line, &info, 1, 0xFFE8_ECFF);
        }
        if v.settings.input_display && v.humans[i] {
            draw_inputs(cv, g.fx, line + 14, v.inputs[i], ts, &l);
        }
        // knocked out, or the winner
        if !seat.alive {
            cv.rect_a(g.fx, g.fy, fw, fh, 0xFF00_0000, 160);
            let txt = if seat.ko_by.is_some() { "K.O." } else { "TOP OUT" };
            chunky(cv, (g.fx + fw / 2) as f32, (g.fy + fh / 2) as f32, txt, (fw as f32 * 0.22).clamp(14.0, 56.0), l.warn, 255);
        }
        if m.outcome != Outcome::Running && m.is_winner(i) && !m.header.mode.is_solo() {
            chunky(cv, (g.fx + fw / 2) as f32, (g.fy + fh * 2 / 5) as f32, "WIN!", (fw as f32 * 0.26).clamp(16.0, 64.0), 0xFFFF_E14A, 255);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_quad(&mut self, cv: &mut Canvas, seat: usize, q: &qd::QuadState, g: &Geom, faces: bool, flat: bool, st: &Settings, ghost_on: bool) {
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
            let shape = &qd::SHAPES[p.t as usize][p.r as usize];
            let gy = q.ghost_y().unwrap_or(p.y);
            self.fx.last[seat] = Some(Landing {
                cells: shape.iter().map(|&(dx, dy)| (p.x + dx, gy + dy - qd::VIS0)).collect(),
                now: shape.iter().map(|&(dx, dy)| (p.x + dx, p.y + dy - qd::VIS0)).collect(),
                color: c,
            });
            if !flat && gy > p.y {
                // a soft light column from the piece down to where it will land
                for &(dx, dy) in shape.iter() {
                    if shape.contains(&(dx, dy + 1)) {
                        continue;
                    }
                    let x = g.fx + (p.x + dx) * cell;
                    let y0 = g.fy + (p.y + dy + 1 - qd::VIS0) * cell;
                    let y1 = g.fy + (gy + dy - qd::VIS0) * cell;
                    let h = y1 - y0;
                    for k in 0..8 {
                        cv.rect_a(x + 3, y0 + h * k / 8, cell - 6, (h * (k + 1) / 8) - (h * k / 8), c, 18 - k as u32 * 2);
                    }
                }
            }
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
        // Hold sits in the tab drawn with the frame; Next is a card with smaller cards under it
        cv.reset_clip();
        let l = look(st.theme);
        let glassy = st.theme == ThemeId::Voyage && st.preset != Preset::Potato;
        let deco = glassy.then_some(seat_color(seat));
        let fw = g.cols * cell;
        let fwid = frame_w(fw);
        let sw = g.side;
        let s1 = ((sw - 18) / 4).max(3);
        let s2 = (s1 * 3 / 4).max(3);
        let (hx, hy, hw, hh, hb) = hold_rect(g);
        if let Some(h) = q.hold {
            let c = if q.hold_used { GRAY } else { theme_tint(st.theme, qd_color(h + 1)) };
            let lab = (card_label_px(hw, "HOLD") * 0.5) as i32;
            self.mini_piece(cv, h, hx + hb, hy + hb + lab, hw - 2 * hb, hh - 2 * hb - lab, s1, c, pat, flat);
        }
        let nx = g.fx + fw + fwid + 10;
        let n = q.cfg.preview.min(q.queue.len()) as i32;
        let mut y = self.side_y[seat];
        if n > 0 {
            let head = side_head(sw, "NEXT");
            let lab = (card_label_px(sw, "NEXT") * 0.5) as i32;
            let h1 = s1 * 3 + 14 + lab;
            side_box(cv, deco, nx, y + head, sw, h1, "NEXT", &l);
            let t0 = q.queue[0];
            self.mini_piece(cv, t0, nx, y + head + lab, sw, h1 - lab, s1, theme_tint(st.theme, qd_color(t0 + 1)), pat, flat);
            y += head + h1 + 7;
            let w2 = sw * 4 / 5;
            for &t in q.queue.iter().take(n as usize).skip(1) {
                let h2 = s2 * 3 + 10;
                side_box(cv, deco, nx + (sw - w2) / 2, y, w2, h2, "", &l);
                self.mini_piece(cv, t, nx + (sw - w2) / 2, y + 1, w2, h2, s2, theme_tint(st.theme, qd_color(t + 1)), pat, flat);
                y += h2 + 6;
            }
            y += 8;
        }
        self.side_y[seat] = y;
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
    fn draw_chain(&mut self, cv: &mut Canvas, seat: usize, c: &ch::ChainState, g: &Geom, faces: bool, flat: bool, st: &Settings, t: f32) {
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
        let deco = glassy.then_some(seat_color(seat));
        let sw = g.side;
        let nx = g.fx + g.cols * cell + frame_w(g.cols * cell) + 10;
        let s1 = cell.min(sw - 22).max(4);
        let s2 = (s1 * 3 / 4).max(4);
        let pairs = c.next_pairs();
        let mut y = self.side_y[seat];
        let head = side_head(sw, "NEXT");
        for (k, (a, b)) in pairs.iter().enumerate() {
            let (s, w) = if k == 0 { (s1, sw) } else { (s2, sw * 4 / 5) };
            let lab = if k == 0 { (card_label_px(sw, "NEXT") * 0.5) as i32 } else { 0 };
            let h = s * 2 + 14 + lab;
            let (x, top) = (nx + (sw - w) / 2, if k == 0 { y + head } else { y });
            side_box(cv, deco, x, top, w, h, if k == 0 { "NEXT" } else { "" }, &l);
            for (j, col) in [b, a].iter().enumerate() {
                let colr = theme_tint(st.theme, chain_color(**col));
                let spr = self.sprites.get(SpriteKey { kind: CellKind::Blob, color: colr, size: s, face: faces && s >= 16, pattern: st.colorblind, flat, frame: 0 });
                cv.blit(spr, x + (w - s) / 2, top + 7 + lab + j as i32 * s, 255);
            }
            y = top + h + 7;
        }
        y += 8;
        if c.all_clear_bonus {
            y = side_value(cv, deco, nx, y, sw, "BONUS", "+30", None, l.good, &l);
        }
        self.side_y[seat] = y;
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_capsule(&mut self, cv: &mut Canvas, seat: usize, c: &cap::CapsuleState, g: &Geom, faces: bool, flat: bool, st: &Settings, t: f32) {
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
            let cells: Vec<(i32, i32)> = p.cells().iter().map(|&(x, y, _, _)| (x, y - cap::VIS0)).collect();
            self.fx.last[seat] = Some(Landing { now: cells.clone(), cells, color: 0xFFFF_FFFF });
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
        let deco = glassy.then_some(seat_color(seat));
        let sw = g.side;
        let nx = g.fx + g.cols * cell + frame_w(g.cols * cell) + 10;
        let s = cell.min((sw - 20) / 2).max(4);
        let head = side_head(sw, "NEXT");
        let mut y = self.side_y[seat];
        let lab = (card_label_px(sw, "NEXT") * 0.5) as i32;
        let bh = s + 16 + lab;
        side_box(cv, deco, nx, y + head, sw, bh, "NEXT", &l);
        if let Some((a, b)) = c.preview() {
            for (j, (col, kind)) in [(a, CellKind::HalfL), (b, CellKind::HalfR)].iter().enumerate() {
                let colr = theme_tint(st.theme, capsule_color(*col));
                let spr = self.sprites.get(SpriteKey { kind: *kind, color: colr, size: s, face: faces, pattern: st.colorblind, flat, frame: 0 });
                cv.blit(spr, nx + sw / 2 - s + j as i32 * s, y + head + 8 + lab, 255);
            }
        }
        y += head + bh + 10;
        y = side_value(cv, deco, nx, y, sw, "VIRUS", &format!("{}", c.viruses), None, 0xFFFF_E14A, &l);
        y = side_value(cv, deco, nx, y, sw, "LEVEL", &format!("{}", c.level), Some(&crate::ui::title_case(cap::SPEED_NAMES[c.speed as usize])), 0xFFFF_FFFF, &l);
        if c.nes_seeds {
            y = side_value(cv, deco, nx, y, sw, "SEED", &format!("{:04X}", c.nes_seed), None, 0xFFFF_FFFF, &l);
        }
        self.side_y[seat] = y;
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_panel(&mut self, cv: &mut Canvas, seat: usize, p: &pn::PanelState, g: &Geom, faces: bool, flat: bool, st: &Settings, t: f32) {
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
        let deco = glassy.then_some(seat_color(seat));
        let sw = g.side;
        let nx = g.fx + g.cols * cell + frame_w(g.cols * cell) + 10;
        let mut y = self.side_y[seat];
        if p.puzzle {
            y = side_value(cv, deco, nx, y, sw, "MOVES", &format!("{}", p.moves_left.max(0)), None, 0xFFFF_E14A, &l);
        } else {
            y = side_value(cv, deco, nx, y, sw, "LEVEL", &format!("{}", p.level), None, 0xFFFF_FFFF, &l);
        }
        if p.chain > 1 {
            y = side_value(cv, deco, nx, y, sw, "CHAIN", &format!("x{}", p.chain), None, 0xFFFF_E14A, &l);
        }
        if p.stop > 0 {
            // the stack is held: a bar that drains as the stop time runs out
            let head = side_head(sw, "STOP");
            let lab = (card_label_px(sw, "STOP") * 0.5) as i32;
            side_box(cv, deco, nx, y + head, sw, 22 + lab, "STOP", &l);
            let bw = sw - 18;
            cv.round_rect(nx + 9, y + head + 9 + lab, bw, 6, 3, 0xFF05_0814, 220);
            cv.round_rect(nx + 9, y + head + 9 + lab, (p.stop.min(240) as i32 * bw / 240).max(6), 6, 3, l.good, 255);
            y += head + 22 + lab + 10;
        }
        self.side_y[seat] = y;
    }
}

/// The middle of what is clearing on a board, relative to its field (for callouts and rings).
fn hot_spot(b: &Board, g: &Geom) -> Option<(f32, f32)> {
    let c = g.cell as f32;
    let avg = |pts: &[(i32, i32)], vis0: i32, rise: f32| -> Option<(f32, f32)> {
        if pts.is_empty() {
            return None;
        }
        let n = pts.len() as f32;
        let x = pts.iter().map(|p| p.0 as f32).sum::<f32>() / n;
        let y = pts.iter().map(|p| p.1 as f32).sum::<f32>() / n;
        Some(((x + 0.5) * c, (y - vis0 as f32 + 0.5) * c - rise - c * 0.6))
    };
    match b {
        Board::Quad(q) => {
            let pts: Vec<(i32, i32)> = q.clear_rows.iter().map(|&y| ((qd::W - 1) / 2, y)).collect();
            avg(&pts, qd::VIS0, 0.0).map(|(_, y)| ((g.cols as f32 * c) / 2.0, y))
        }
        Board::Chain(ch_) => avg(&ch_.pop_cells.iter().map(|&i| (i % ch::W, i / ch::W)).collect::<Vec<_>>(), ch::VIS0, 0.0),
        Board::Capsule(cp) => avg(&cp.clearing.iter().map(|&i| (i % cap::W, i / cap::W)).collect::<Vec<_>>(), cap::VIS0, 0.0),
        Board::Panel(p) => {
            let mut pts = Vec::new();
            for y in 0..pn::H {
                for x in 0..pn::W {
                    if p.grid[(y * pn::W + x) as usize].s == pn::St::Match {
                        pts.push((x, y));
                    }
                }
            }
            avg(&pts, pn::VIS0, (p.rise_sub as i32 * g.cell / pn::RISE_STEPS as i32) as f32)
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

/// Where Quad's Hold tab sits: out of the frame's top-left corner. (x, y, w, h, border)
fn hold_rect(g: &Geom) -> (i32, i32, i32, i32, i32) {
    let fw = g.cols * g.cell;
    let fwid = frame_w(fw);
    let sw = g.side;
    let s1 = ((sw - 18) / 4).max(3);
    let b = (sw / 9).clamp(4, 8);
    let lab = (card_label_px(sw, "HOLD") * 0.5) as i32;
    (g.fx - fwid - sw + 2, g.fy - fwid + 6, sw, s1 * 3 + 2 * b + 10 + lab, b)
}

/// Chunky outlined text in the display face, centred on (cx, cy): the HOLD / NEXT words, the
/// score, names and stamps.
/// The width `chunky` gives a text.
pub fn chunky_w(text: &str, px: f32) -> f32 {
    let sp = (px / 9.0).max(1.4) * 1.1;
    text.chars().map(|c| crate::font::measure(&c.to_string(), px, Weight::Display)).sum::<f32>() + sp * (text.chars().count().max(1) - 1) as f32
}

pub fn chunky(cv: &mut Canvas, cx: f32, cy: f32, text: &str, px: f32, fill: u32, a: u32) {
    // letters are spaced a little wider than the face's own spacing, so each outline closes
    // around its own letter instead of running into the next one
    let o = (px / 9.0).max(1.4);
    let sp = o * 1.1;
    let chars: Vec<String> = text.chars().map(|c| c.to_string()).collect();
    let adv: Vec<f32> = chars.iter().map(|c| crate::font::measure(c, px, Weight::Display)).collect();
    let w = adv.iter().sum::<f32>() + sp * (chars.len().max(1) - 1) as f32;
    let base = cy + px * 0.36;
    for pass in 0..2 {
        let mut x = cx - w / 2.0;
        for (c, &dx) in chars.iter().zip(adv.iter()) {
            if pass == 0 {
                for k in 0..12 {
                    let ang = k as f32 * std::f32::consts::TAU / 12.0;
                    crate::font::draw(cv, x + ang.cos() * o, base + ang.sin() * o + o * 0.5, c, px, Weight::Display, INK, a);
                }
            } else {
                crate::font::draw(cv, x, base, c, px, Weight::Display, fill, a);
            }
            x += dx + sp;
        }
    }
}

/// The caption size that fits a side card `w` wide.
fn card_label_px(w: i32, label: &str) -> f32 {
    let mut px = if w >= 70 { 19.0 } else { 14.0 };
    while px > 9.0 && crate::font::measure(label, px, Weight::Display) > (w - 6) as f32 {
        px -= 1.0;
    }
    px
}

/// Height of a side card's caption: the word sits on the card's top edge, half outside.
fn side_head(w: i32, label: &str) -> i32 {
    (card_label_px(w, label) * 0.55) as i32 + 4
}

/// A side card: a dark inside in a thick white rounded border with an ink outline; on the
/// flat themes a plain dark card. The caption sits on the top edge.
#[allow(clippy::too_many_arguments)]
fn side_box(cv: &mut Canvas, deco: Option<u32>, x: i32, y: i32, w: i32, h: i32, label: &str, l: &Look) {
    let r = (w / 6).clamp(5, 12);
    if deco.is_some() {
        cv.soft_shadow(x, y + 4, w, h, r, 7, 140);
        cv.round_rect(x - 2, y - 2, w + 4, h + 4, r + 2, INK, 255);
        cv.round_rect(x, y, w, h, r, 0xFFF6_F8FF, 255);
        let b = (w / 16).clamp(3, 5);
        cv.round_grad(x + b, y + b, w - 2 * b, h - 2 * b, (r - b).max(2), 0xFF1B_1F33, 0xFF06_070D, 255);
        if !label.is_empty() {
            chunky(cv, (x + w / 2) as f32, y as f32 + 1.0, label, card_label_px(w, label), 0xFFFF_FFFF, 255);
        }
    } else {
        cv.round_rect(x, y, w, h, r.min(6), 0xFF0A_0E1E, 220);
        let lw = Canvas::text_wb(label, 1, Weight::Bold);
        if lw <= w - 4 {
            cv.text_weight(x + (w - lw) / 2, y + 3, label, 1, l.dim, 255, Weight::Bold);
        }
    }
}

/// A Quad Hold tab: it grows out of the frame's top-left corner in the frame's colour.
fn hold_tab(cv: &mut Canvas, deco: Option<u32>, x: i32, y: i32, w: i32, h: i32, l: &Look) {
    let Some(color) = deco else {
        side_box(cv, None, x, y, w, h, "HOLD", l);
        return;
    };
    let r = (w / 6).clamp(5, 12);
    // the tab runs a little under the frame so the two read as one piece
    cv.soft_shadow(x, y + 5, w, h, r, 9, 150);
    cv.round_rect(x - 3, y - 3, w + 12, h + 6, r + 3, INK, 255);
    cv.round_grad(x, y, w + 10, h, r, lighten(color, 110), darken(color, 35), 255);
    cv.round_rect(x + r, y + 3, (w - r) * 3 / 5, 3, 2, 0xFFFF_FFFF, 200);
    let b = (w / 9).clamp(4, 8);
    cv.round_rect(x + b - 2, y + b - 2, w - 2 * b + 4, h - 2 * b + 4, (r - b).max(3) + 2, INK, 255);
    cv.round_grad(x + b, y + b, w - 2 * b, h - 2 * b, (r - b).max(3), 0xFF1B_1F33, 0xFF06_070D, 255);
    chunky(cv, (x + w / 2) as f32, y as f32 + 1.0, "HOLD", card_label_px(w, "HOLD"), 0xFFFF_FFFF, 255);
}

/// A captioned number on a side card (virus count, level, lines...). Returns the next y.
#[allow(clippy::too_many_arguments)]
fn side_value(cv: &mut Canvas, deco: Option<u32>, x: i32, y: i32, w: i32, label: &str, value: &str, sub: Option<&str>, col: u32, l: &Look) -> i32 {
    let head = side_head(w, label);
    let mut vpx = if w >= 70 { 30.0 } else if w >= 50 { 22.0 } else { 16.0 };
    while vpx > 10.0 && crate::font::measure(value, vpx, Weight::Display) > (w - 12) as f32 {
        vpx -= 1.0;
    }
    let sub = sub.filter(|s| Canvas::text_w(s, 1) <= w - 10);
    let lab = (card_label_px(w, label) * 0.5) as i32;
    let h = head + lab + vpx as i32 + if sub.is_some() { 14 } else { 0 } + 8;
    let y = y + head;
    side_box(cv, deco, x, y, w, h - head, label, l);
    let vy = y + lab + 2 + vpx as i32 / 2;
    if deco.is_some() {
        let vw = crate::font::measure(value, vpx, Weight::Display);
        crate::font::draw(cv, (x + w / 2) as f32 - vw / 2.0, vy as f32 + vpx * 0.36, value, vpx, Weight::Display, col, 255);
    } else {
        let vw = crate::font::measure(value, vpx, Weight::Bold);
        crate::font::draw(cv, (x + w / 2) as f32 - vw / 2.0, vy as f32 + vpx * 0.36, value, vpx, Weight::Bold, if col == 0xFFFF_FFFF { l.text } else { col }, 255);
    }
    if let Some(s) = sub {
        cv.text_a(x + (w - Canvas::text_w(s, 1)) / 2, vy + vpx as i32 / 2 + 2, s, 1, l.dim, 255);
    }
    y + h - head + 10
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
