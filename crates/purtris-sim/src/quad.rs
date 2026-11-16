//! Quad (falling tetromino). Profiles quad.ppt (cross default) and quad.guideline (solo, 180).
//! Handling (DAS/ARR/SDF) is outside the sim. The client emits L/R/SD bits and the sim obeys them.

use crate::common::{bits as B, Ev, Native, Out, Packet};
use crate::matchs::SeatCtx;
use crate::rng::{Hasher64, Rng};
use std::collections::VecDeque;

pub const W: i32 = 10;
pub const H: i32 = 40;
pub const VIS0: i32 = 20;

pub const SHAPES: [[[(i32, i32); 4]; 4]; 7] = [
    // I
    [[(0, 1), (1, 1), (2, 1), (3, 1)], [(2, 0), (2, 1), (2, 2), (2, 3)], [(0, 2), (1, 2), (2, 2), (3, 2)], [(1, 0), (1, 1), (1, 2), (1, 3)]],
    // O
    [[(1, 0), (2, 0), (1, 1), (2, 1)], [(1, 0), (2, 0), (1, 1), (2, 1)], [(1, 0), (2, 0), (1, 1), (2, 1)], [(1, 0), (2, 0), (1, 1), (2, 1)]],
    // T
    [[(1, 0), (0, 1), (1, 1), (2, 1)], [(1, 0), (1, 1), (2, 1), (1, 2)], [(0, 1), (1, 1), (2, 1), (1, 2)], [(1, 0), (0, 1), (1, 1), (1, 2)]],
    // S
    [[(1, 0), (2, 0), (0, 1), (1, 1)], [(1, 0), (1, 1), (2, 1), (2, 2)], [(1, 1), (2, 1), (0, 2), (1, 2)], [(0, 0), (0, 1), (1, 1), (1, 2)]],
    // Z
    [[(0, 0), (1, 0), (1, 1), (2, 1)], [(2, 0), (1, 1), (2, 1), (1, 2)], [(0, 1), (1, 1), (1, 2), (2, 2)], [(1, 0), (0, 1), (1, 1), (0, 2)]],
    // J
    [[(0, 0), (0, 1), (1, 1), (2, 1)], [(1, 0), (2, 0), (1, 1), (1, 2)], [(0, 1), (1, 1), (2, 1), (2, 2)], [(1, 0), (1, 1), (0, 2), (1, 2)]],
    // L
    [[(2, 0), (0, 1), (1, 1), (2, 1)], [(1, 0), (1, 1), (1, 2), (2, 2)], [(0, 1), (1, 1), (2, 1), (0, 2)], [(0, 0), (1, 0), (1, 1), (1, 2)]],
];
pub const PIECE_NAMES: [char; 7] = ['I', 'O', 'T', 'S', 'Z', 'J', 'L'];
pub const T_PIECE: u8 = 2;
pub const GARBAGE: u8 = 8;

// SRS kicks, y-up as in the guideline tables. Index by (from, to).
const K_JLSTZ: [[(i32, i32); 5]; 8] = [
    [(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)], // 0>1
    [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],     // 1>0
    [(0, 0), (1, 0), (1, -1), (0, 2), (1, 2)],     // 1>2
    [(0, 0), (-1, 0), (-1, 1), (0, -2), (-1, -2)], // 2>1
    [(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],    // 2>3
    [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],  // 3>2
    [(0, 0), (-1, 0), (-1, -1), (0, 2), (-1, 2)],  // 3>0
    [(0, 0), (1, 0), (1, 1), (0, -2), (1, -2)],    // 0>3
];
const K_I: [[(i32, i32); 5]; 8] = [
    [(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)],
    [(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)],
    [(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)],
    [(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 1)],
    [(0, 0), (2, 0), (-1, 0), (2, 1), (-1, -2)],
    [(0, 0), (-2, 0), (1, 0), (-2, -1), (1, 2)],
    [(0, 0), (1, 0), (-2, 0), (1, -2), (-2, 1)],
    [(0, 0), (-1, 0), (2, 0), (-1, 2), (2, -1)],
];
fn kick_index(from: u8, to: u8) -> usize {
    match (from, to) {
        (0, 1) => 0,
        (1, 0) => 1,
        (1, 2) => 2,
        (2, 1) => 3,
        (2, 3) => 4,
        (3, 2) => 5,
        (3, 0) => 6,
        _ => 7, // 0>3
    }
}
const K_180: [(i32, i32); 4] = [(0, 0), (0, 1), (1, 0), (-1, 0)];

// Same-style Tetris-vs-Tetris (design doc table).
const TVT: [u32; 5] = [0, 0, 1, 2, 4];
const TVT_TSPIN: [u32; 4] = [0, 2, 4, 6];
// Cross gauge (Tetris vs Chain, after the nerf).
const TVP: [u32; 5] = [0, 0, 1, 2, 4];
const TVP_TSPIN: [u32; 4] = [0, 2, 3, 4];
/// PROVISIONAL combo column. Index 0 = first clear.
pub const COMBO: [u32; 13] = [0, 0, 1, 1, 1, 2, 2, 3, 3, 4, 4, 4, 5];

const TPR: [u32; 16] = [60, 60, 48, 37, 28, 21, 16, 11, 8, 6, 4, 3, 2, 1, 1, 1];
pub fn grav_for(level: u32) -> u32 {
    if level >= 20 {
        65536 * 20
    } else if level as usize >= TPR.len() {
        65536
    } else {
        65536 / TPR[level as usize]
    }
}
const LOCK_DELAY: u32 = 30;
const MAX_RESETS: u32 = 15;
const ARE: u32 = 6;
const CLEAR_DELAY: u32 = 18;

#[derive(Clone, Copy, Debug)]
pub struct QuadCfg {
    pub hold: bool,
    pub allow180: bool,
    pub versus_gravity: bool,
    pub level_up: bool,
    pub preview: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub t: u8,
    pub r: u8,
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Are,
    Active,
    Clear,
}

#[derive(Clone, Copy, Debug)]
pub struct Pending {
    pub n: u32,
    pub hole: i32,
    pub tick: u32,
}

#[derive(Clone, Debug)]
pub struct QuadState {
    pub cfg: QuadCfg,
    pub grid: Vec<u8>,
    pub queue: VecDeque<u8>,
    pub hold: Option<u8>,
    pub hold_used: bool,
    pub cur: Option<Piece>,
    pub bag_rng: Rng,
    pub hole_rng: Rng,
    pub grav: u32,
    pub grav_speed: u32,
    pub level: u32,
    pub lock_timer: u32,
    pub resets: u32,
    pub lowest: i32,
    pub last_rot: bool,
    pub last_kick: usize,
    pub combo: i32,
    pub b2b: bool,
    pub pending: Vec<Pending>,
    pub phase: Phase,
    pub timer: u32,
    pub clear_rows: Vec<i32>,
    pub lines: u32,
    pub score: u64,
    pub pieces: u32,
    pub ticks: u32,
    pub dead: bool,
    pub cross: bool,
    pub gauge_held: u32,
    pub native_held: u32,
    pub buf_hold: bool,
    pub piece_inputs: u32,
    /// Finesse evidence for the analyzer: (tick, inputs used, piece) per lock.
    pub last_lock: Option<(u32, u32, u8)>,
    pub holes_made: u32,
}

#[inline]
pub fn fits(grid: &[u8], t: u8, r: u8, x: i32, y: i32) -> bool {
    for &(dx, dy) in SHAPES[t as usize][r as usize].iter() {
        let cx = x + dx;
        let cy = y + dy;
        if cx < 0 || cx >= W || cy >= H {
            return false;
        }
        if cy >= 0 && grid[(cy * W + cx) as usize] != 0 {
            return false;
        }
    }
    true
}
pub fn drop_y(grid: &[u8], t: u8, r: u8, x: i32, mut y: i32) -> i32 {
    while fits(grid, t, r, x, y + 1) {
        y += 1;
    }
    y
}
pub fn count_holes(grid: &[u8]) -> u32 {
    let mut holes = 0;
    for x in 0..W {
        let mut seen = false;
        for y in 0..H {
            let v = grid[(y * W + x) as usize];
            if v != 0 {
                seen = true;
            } else if seen {
                holes += 1;
            }
        }
    }
    holes
}

impl QuadState {
    pub fn new(ctx: &SeatCtx, cfg: QuadCfg) -> Self {
        let level = ctx.opts.quad_level.max(1);
        let mut st = QuadState {
            cfg,
            grid: vec![0; (W * H) as usize],
            queue: VecDeque::new(),
            hold: None,
            hold_used: false,
            cur: None,
            bag_rng: Rng::stream(ctx.seed, "quad.bag", ctx.mix()),
            hole_rng: Rng::stream(ctx.seed, "quad.garbage_hole", ctx.mix()),
            grav: 0,
            grav_speed: grav_for(level),
            level,
            lock_timer: 0,
            resets: 0,
            lowest: 0,
            last_rot: false,
            last_kick: 0,
            combo: -1,
            b2b: false,
            pending: Vec::new(),
            phase: Phase::Are,
            timer: 1,
            clear_rows: Vec::new(),
            lines: 0,
            score: 0,
            pieces: 0,
            ticks: 0,
            dead: false,
            cross: ctx.cross,
            gauge_held: 0,
            native_held: 0,
            buf_hold: false,
            piece_inputs: 0,
            last_lock: None,
            holes_made: 0,
        };
        st.refill();
        st
    }

    fn refill(&mut self) {
        while self.queue.len() < 14 {
            let mut bag = [0u8, 1, 2, 3, 4, 5, 6];
            self.bag_rng.shuffle(&mut bag);
            self.queue.extend(bag.iter());
        }
    }

    fn spawn(&mut self, out: &mut Out, t: u8) {
        let mut p = Piece { t, r: 0, x: 3, y: VIS0 - 2 };
        self.grav = 0;
        self.lock_timer = 0;
        self.resets = 0;
        self.last_rot = false;
        self.last_kick = 0;
        self.piece_inputs = 0;
        if !fits(&self.grid, t, 0, p.x, p.y) {
            self.dead = true;
            self.cur = None;
            out.events.push(Ev::TopOut);
            return;
        }
        if fits(&self.grid, t, 0, p.x, p.y + 1) {
            p.y += 1;
        }
        self.lowest = p.y;
        self.cur = Some(p);
        self.phase = Phase::Active;
    }

    fn next_piece(&mut self, out: &mut Out) {
        self.refill();
        let t = self.queue.pop_front().unwrap();
        self.hold_used = false;
        self.spawn(out, t);
    }

    fn do_hold(&mut self, out: &mut Out) {
        let Some(c) = self.cur else { return };
        out.events.push(Ev::Hold);
        let held = self.hold.replace(c.t);
        match held {
            Some(h) => self.spawn(out, h),
            None => {
                self.refill();
                let t = self.queue.pop_front().unwrap();
                self.spawn(out, t);
            }
        }
        self.hold_used = true;
    }

    fn try_move(&mut self, dx: i32, dy: i32) -> bool {
        let Some(c) = self.cur.as_mut() else { return false };
        if fits(&self.grid, c.t, c.r, c.x + dx, c.y + dy) {
            c.x += dx;
            c.y += dy;
            true
        } else {
            false
        }
    }

    fn try_rotate(&mut self, dir: i32) -> bool {
        let Some(mut c) = self.cur else { return false };
        if c.t == 1 {
            return false;
        }
        let (nr, kicks): (u8, &[(i32, i32)]) = if dir == 2 {
            if !self.cfg.allow180 {
                return false;
            }
            ((c.r + 2) & 3, &K_180)
        } else {
            let nr = ((c.r as i32 + dir + 4) & 3) as u8;
            let k = kick_index(c.r, nr);
            (nr, if c.t == 0 { &K_I[k] } else { &K_JLSTZ[k] })
        };
        for (i, &(kx, ky)) in kicks.iter().enumerate() {
            let (dx, dy) = (kx, -ky);
            if fits(&self.grid, c.t, nr, c.x + dx, c.y + dy) {
                c.x += dx;
                c.y += dy;
                c.r = nr;
                self.cur = Some(c);
                self.last_rot = true;
                self.last_kick = i;
                return true;
            }
        }
        false
    }

    fn grounded(&self) -> bool {
        match self.cur {
            Some(c) => !fits(&self.grid, c.t, c.r, c.x, c.y + 1),
            None => false,
        }
    }
    fn reset_lock(&mut self) {
        if self.grounded() && self.resets < MAX_RESETS {
            self.lock_timer = 0;
            self.resets += 1;
        }
    }

    fn tspin(&self) -> u8 {
        let Some(c) = self.cur else { return 0 };
        if c.t != T_PIECE || !self.last_rot {
            return 0;
        }
        let occ = |x: i32, y: i32| -> u32 {
            if x < 0 || x >= W || y >= H {
                1
            } else if y >= 0 && self.grid[(y * W + x) as usize] != 0 {
                1
            } else {
                0
            }
        };
        let corners = [occ(c.x, c.y), occ(c.x + 2, c.y), occ(c.x + 2, c.y + 2), occ(c.x, c.y + 2)];
        if corners.iter().sum::<u32>() < 3 {
            return 0;
        }
        let front = [(0, 1), (1, 2), (2, 3), (3, 0)][c.r as usize];
        if corners[front.0] == 1 && corners[front.1] == 1 {
            2
        } else if self.last_kick == 4 {
            2
        } else {
            1
        }
    }

    fn lock(&mut self, out: &mut Out) {
        let Some(c) = self.cur else { return };
        let holes_before = count_holes(&self.grid);
        let mut above = true;
        for &(dx, dy) in SHAPES[c.t as usize][c.r as usize].iter() {
            let (x, y) = (c.x + dx, c.y + dy);
            if y >= 0 {
                self.grid[(y * W + x) as usize] = c.t + 1;
            }
            if y >= VIS0 {
                above = false;
            }
        }
        let ts = self.tspin();
        self.pieces += 1;
        out.events.push(Ev::Lock { x: c.x + 1, y: c.y + 1 });
        self.last_lock = Some((self.ticks, self.piece_inputs, c.t));
        self.cur = None;
        let mut full = Vec::new();
        for y in 0..H {
            if (0..W).all(|x| self.grid[(y * W + x) as usize] != 0) {
                full.push(y);
            }
        }
        let n = full.len();
        if above && n == 0 {
            self.dead = true;
            out.events.push(Ev::TopOut);
            return;
        }
        if n > 0 {
            self.combo += 1;
            let difficult = n == 4 || ts > 0;
            let b2b_bonus = if difficult && self.b2b { 1 } else { 0 };
            let filled = self.grid.iter().filter(|&&v| v != 0).count();
            let pc = filled == n * W as usize;
            let (mut lines, mut gauge) = if ts == 2 {
                (TVT_TSPIN[n.min(3)], TVP_TSPIN[n.min(3)])
            } else {
                (TVT[n], TVP[n])
            };
            let combo_b = COMBO[(self.combo as usize).min(COMBO.len() - 1)];
            if pc {
                lines = 10;
                gauge = 6;
            }
            lines += b2b_bonus + combo_b;
            gauge += b2b_bonus + combo_b;
            if difficult {
                self.b2b = true;
            } else if !pc {
                self.b2b = false;
            }
            self.lines += n as u32;
            let lvl = self.level as u64;
            let base: u64 = if ts == 2 {
                [400, 800, 1200, 1600][n.min(3)]
            } else if ts == 1 {
                [100, 200, 400][n.min(2)]
            } else {
                [0, 100, 300, 500, 800][n]
            };
            self.score += (base + if pc { 2000 } else { 0 }) * lvl + 50 * (self.combo.max(0) as u64) * lvl;
            let mut text = String::new();
            if b2b_bonus > 0 {
                text.push_str("B2B ");
            }
            if ts == 2 {
                text.push_str("T-SPIN ");
            } else if ts == 1 {
                text.push_str("MINI T-SPIN ");
            }
            text.push_str(["", "SINGLE", "DOUBLE", "TRIPLE", "QUAD"][n]);
            if self.combo > 0 {
                text.push_str(&format!("  {} COMBO", self.combo));
            }
            for &y in &full {
                for x in 0..W {
                    out.events.push(Ev::Burst { x, y, color: self.grid[(y * W + x) as usize] });
                }
            }
            out.events.push(Ev::Clear { power: n as u32 + if ts > 0 { 2 } else { 0 } + self.combo as u32, text });
            if pc {
                out.events.push(Ev::AllClear);
            }
            self.clear_rows = full;
            self.phase = Phase::Clear;
            self.timer = CLEAR_DELAY;
            if self.cross {
                self.gauge_held += gauge;
                self.native_held += lines;
            } else if lines > 0 {
                out.attacks.push(Packet { native: Native::Lines(lines), gauge });
            }
            if self.cfg.level_up {
                self.level = self.level.max(1 + self.lines / 10);
                self.grav_speed = grav_for(self.level);
            }
        } else {
            self.combo = -1;
            let holes_after = count_holes(&self.grid);
            if holes_after > holes_before {
                self.holes_made += holes_after - holes_before;
            }
            if self.cross && self.gauge_held > 0 {
                out.attacks.push(Packet { native: Native::Lines(self.native_held), gauge: self.gauge_held });
                self.gauge_held = 0;
                self.native_held = 0;
            }
            self.insert_garbage(out);
            if self.dead {
                return;
            }
            self.phase = Phase::Are;
            self.timer = ARE;
        }
    }

    fn insert_garbage(&mut self, out: &mut Out) {
        while let Some(&g) = self.pending.first() {
            if g.tick >= self.ticks {
                break; // applied this very tick
            }
            self.pending.remove(0);
            let n = (g.n as i32).min(H);
            for i in 0..(n * W) as usize {
                if self.grid[i] != 0 {
                    self.dead = true;
                }
            }
            self.grid.copy_within((n * W) as usize.., 0);
            for y in (H - n)..H {
                for x in 0..W {
                    self.grid[(y * W + x) as usize] = if x == g.hole { 0 } else { GARBAGE };
                }
            }
            out.events.push(Ev::GarbageLand { n: g.n });
            if self.dead {
                out.events.push(Ev::TopOut);
                return;
            }
        }
    }

    pub fn step(&mut self, bits: u16, prev: u16, out: &mut Out) {
        self.ticks += 1;
        if self.cfg.versus_gravity && self.ticks % 1800 == 0 && self.level < 8 {
            self.level += 1;
            self.grav_speed = grav_for(self.level);
        }
        if self.dead {
            return;
        }
        let press = bits & !prev;
        match self.phase {
            Phase::Clear => {
                if press & B::HOLD != 0 {
                    self.buf_hold = true;
                }
                self.timer = self.timer.saturating_sub(1);
                if self.timer == 0 {
                    let mut ng = vec![0u8; (W * H) as usize];
                    let mut w = H - 1;
                    for y in (0..H).rev() {
                        if self.clear_rows.contains(&y) {
                            continue;
                        }
                        let src = (y * W) as usize;
                        let dst = (w * W) as usize;
                        ng[dst..dst + W as usize].copy_from_slice(&self.grid[src..src + W as usize]);
                        w -= 1;
                    }
                    self.grid = ng;
                    self.clear_rows.clear();
                    self.phase = Phase::Are;
                    self.timer = ARE;
                }
                return;
            }
            Phase::Are => {
                if press & B::HOLD != 0 {
                    self.buf_hold = true;
                }
                self.timer = self.timer.saturating_sub(1);
                if self.timer == 0 {
                    self.next_piece(out);
                    if self.buf_hold && !self.dead && self.cfg.hold {
                        self.buf_hold = false;
                        self.do_hold(out);
                    }
                    self.buf_hold = false;
                }
                return;
            }
            Phase::Active => {}
        }
        if self.cur.is_none() {
            return;
        }
        if press != 0 {
            self.piece_inputs += press.count_ones();
        }
        if press & B::HOLD != 0 && self.cfg.hold && !self.hold_used {
            self.do_hold(out);
            if self.dead {
                return;
            }
        }
        if press & B::CW != 0 && self.try_rotate(1) {
            out.events.push(Ev::Rotate);
            self.reset_lock();
        }
        if press & B::CCW != 0 && self.try_rotate(-1) {
            out.events.push(Ev::Rotate);
            self.reset_lock();
        }
        if press & B::R180 != 0 && self.try_rotate(2) {
            out.events.push(Ev::Rotate);
            self.reset_lock();
        }
        let dir = if bits & B::L != 0 { -1 } else if bits & B::R != 0 { 1 } else { 0 };
        if dir != 0 {
            let mut moved = false;
            if bits & B::FAST != 0 {
                while self.try_move(dir, 0) {
                    moved = true;
                }
            } else {
                moved = self.try_move(dir, 0);
            }
            if moved {
                self.last_rot = false;
                out.events.push(Ev::Move);
                self.reset_lock();
            }
        }
        if press & B::HD != 0 {
            let c = self.cur.unwrap();
            let ny = drop_y(&self.grid, c.t, c.r, c.x, c.y);
            if ny > c.y {
                self.last_rot = false;
            }
            self.score += 2 * (ny - c.y) as u64;
            self.cur = Some(Piece { y: ny, ..c });
            out.events.push(Ev::HardDrop);
            self.lock(out);
            return;
        }
        if bits & B::SD != 0 {
            if bits & B::FAST != 0 {
                let c = self.cur.unwrap();
                let ny = drop_y(&self.grid, c.t, c.r, c.x, c.y);
                if ny > c.y {
                    self.last_rot = false;
                    self.score += (ny - c.y) as u64;
                    self.cur = Some(Piece { y: ny, ..c });
                }
            } else if self.try_move(0, 1) {
                self.last_rot = false;
                self.score += 1;
                self.grav = 0;
            }
        }
        self.grav += self.grav_speed;
        while self.grav >= 65536 {
            self.grav -= 65536;
            if self.try_move(0, 1) {
                self.last_rot = false;
            } else {
                self.grav = 0;
                break;
            }
        }
        let cy = self.cur.unwrap().y;
        if cy > self.lowest {
            self.lowest = cy;
            self.resets = 0;
        }
        if self.grounded() {
            self.lock_timer += 1;
            if self.lock_timer >= LOCK_DELAY {
                self.lock(out);
            }
        } else {
            self.lock_timer = 0;
        }
    }

    pub fn apply_incoming(&mut self, n: u32) {
        if n == 0 {
            return;
        }
        let hole = self.hole_rng.below(W as u32) as i32;
        self.pending.push(Pending { n, hole, tick: self.ticks + 1 });
    }
    pub fn pending_units(&self) -> u32 {
        self.pending.iter().map(|g| g.n).sum()
    }
    pub fn cancel_pending(&mut self, mut units: u32) -> u32 {
        let mut c = 0;
        while units > 0 {
            let Some(g) = self.pending.first_mut() else { break };
            if g.tick >= self.ticks {
                break;
            }
            let k = units.min(g.n);
            g.n -= k;
            units -= k;
            c += k;
            if g.n == 0 {
                self.pending.remove(0);
            }
        }
        c
    }
    pub fn danger(&self) -> bool {
        (VIS0 * W..(VIS0 + 4) * W).any(|i| self.grid[i as usize] != 0)
    }
    pub fn height(&self) -> i32 {
        for y in 0..H {
            if (0..W).any(|x| self.grid[(y * W + x) as usize] != 0) {
                return H - y;
            }
        }
        0
    }
    pub fn ghost_y(&self) -> Option<i32> {
        self.cur.map(|c| drop_y(&self.grid, c.t, c.r, c.x, c.y))
    }
    pub fn hash(&self, h: &mut Hasher64) {
        h.bytes(&self.grid);
        for &q in self.queue.iter() {
            h.byte(q);
        }
        h.byte(self.hold.map(|v| v + 1).unwrap_or(0));
        if let Some(c) = self.cur {
            h.byte(c.t);
            h.byte(c.r);
            h.i32(c.x);
            h.i32(c.y);
        }
        h.u32(self.bag_rng.s);
        h.u32(self.hole_rng.s);
        h.u32(self.grav);
        h.u32(self.lock_timer);
        h.u32(self.lines);
        h.u64(self.score);
        for g in &self.pending {
            h.u32(g.n);
            h.i32(g.hole);
        }
    }
}
