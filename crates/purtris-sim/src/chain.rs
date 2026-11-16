//! Chain (connect-four blobs), profile chain.tsu.
//! Row 0 = 14th row (vanishes), row 1 = hidden 13th row (never pops), rows 2..13 visible.

use crate::common::{bits as B, Ev, Native, Out, Packet, GAUGE_NUISANCE};
use crate::matchs::SeatCtx;
use crate::rng::{Hasher64, Rng};
use std::collections::VecDeque;

pub const W: i32 = 6;
pub const H: i32 = 14;
pub const VIS0: i32 = 2;
pub const X_CELL: (i32, i32) = (2, 2);
pub const NUISANCE: u8 = 6;

// Versus Tsu chain power, links 1..24, then +32 to 999.
const CHAIN_POWER: [u32; 24] = [
    0, 8, 16, 32, 64, 96, 128, 160, 192, 224, 256, 288, 320, 352, 384, 416, 448, 480, 512, 544, 576, 608, 640, 672,
];
const COLOR_BONUS: [u32; 7] = [0, 0, 3, 6, 12, 24, 48];
pub fn chain_power(link: u32) -> u32 {
    if link == 0 {
        return 0;
    }
    if (link as usize) <= CHAIN_POWER.len() {
        CHAIN_POWER[link as usize - 1]
    } else {
        (672 + 32 * (link - 24)).min(999)
    }
}
pub fn group_bonus(size: u32) -> u32 {
    match size {
        0..=4 => 0,
        5 => 2,
        6 => 3,
        7 => 4,
        8 => 5,
        9 => 6,
        10 => 7,
        _ => 10,
    }
}

// PROVISIONAL timings (ticks). Capture replaces these.
const SPAWN_DELAY: u32 = 8;
const FALL_SPEED: u32 = 1; // sixteenths of a cell per tick (16 ticks per cell)
const SOFT_SPEED: u32 = 8; // sixteenths per tick while down is held
const LOCK_DELAY: u32 = 20;
const GROUND_CAP: u32 = 90;
const SETTLE_TICKS: u32 = 1; // ticks per cell while loose blobs fall
const POP_TICKS: u32 = 30;
const DAS_CHARGE: u32 = 8;
const DAS_REPEAT: u32 = 2;
const QUICK_TURN_WINDOW: u32 = 20;
pub const MARGIN_TICKS: u32 = 96 * 60;
const MARGIN_STEP: u32 = 16 * 60;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pair {
    pub x: i32,
    pub y: i32,
    /// child direction: 0 up, 1 right, 2 down, 3 left
    pub r: u8,
    pub a: u8,
    pub b: u8,
    /// sixteenths of a cell fallen toward the next row (for smooth render)
    pub sub: u32,
}
impl Pair {
    pub fn child(&self) -> (i32, i32) {
        let (dx, dy) = [(0, -1), (1, 0), (0, 1), (-1, 0)][self.r as usize];
        (self.x + dx, self.y + dy)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Spawn,
    Active,
    Settle,
    Pop,
}

#[derive(Clone, Copy, Debug)]
pub struct Pending {
    pub n: u32,
    pub tick: u32,
}

#[derive(Clone, Debug)]
pub struct ChainState {
    pub grid: Vec<u8>,
    pub pair: Option<Pair>,
    pub queue: VecDeque<(u8, u8)>,
    pub drop_rng: Rng,
    pub nuis_rng: Rng,
    pub colors: u8,
    pub phase: Phase,
    pub timer: u32,
    pub lock_timer: u32,
    pub ground_total: u32,
    pub das: u32,
    pub das_dir: i32,
    pub quick_turn: u32,
    pub chain: u32,
    pub max_chain: u32,
    pub last_chain: u32,
    pub score: u64,
    pub leftover: u32,
    pub target: u32,
    pub margin_steps: u32,
    pub ticks: u32,
    pub pending: Vec<Pending>,
    pub all_clear_bonus: bool,
    pub drop_bonus: u32,
    pub pop_cells: Vec<i32>,
    pub dead: bool,
    pub cross: bool,
    pub gauge_carry: u32,
    pub placements: u32,
    pub sent_this_chain: u32,
    pub nuisance_dropped: bool,
}

fn idx(x: i32, y: i32) -> usize {
    (y * W + x) as usize
}
fn free(grid: &[u8], x: i32, y: i32) -> bool {
    x >= 0 && x < W && y < H && (y < 0 || grid[idx(x, y)] == 0)
}
pub fn pair_fits(grid: &[u8], p: &Pair) -> bool {
    let (cx, cy) = p.child();
    free(grid, p.x, p.y) && free(grid, cx, cy) && p.y >= 0
}

/// Let every loose blob fall one cell. Returns true if anything moved.
pub fn settle_once(grid: &mut [u8]) -> bool {
    let mut moved = false;
    for y in (0..H - 1).rev() {
        for x in 0..W {
            if grid[idx(x, y)] != 0 && grid[idx(x, y + 1)] == 0 {
                grid[idx(x, y + 1)] = grid[idx(x, y)];
                grid[idx(x, y)] = 0;
                moved = true;
            }
        }
    }
    moved
}
pub fn settle_all(grid: &mut [u8]) {
    for x in 0..W {
        let mut w = H - 1;
        for y in (0..H).rev() {
            let v = grid[idx(x, y)];
            if v != 0 {
                grid[idx(x, y)] = 0;
                grid[idx(x, w)] = v;
                w -= 1;
            }
        }
    }
}

/// Find groups of 4+ in rows >= VIS0. Returns (cells to clear incl. adjacent nuisance, group sizes, color mask).
pub fn find_pops(grid: &[u8]) -> (Vec<i32>, Vec<u32>, u32) {
    let mut seen = vec![false; (W * H) as usize];
    let mut clear = Vec::new();
    let mut sizes = Vec::new();
    let mut colors = 0u32;
    let mut stack = Vec::new();
    let mut group = Vec::new();
    for y in VIS0..H {
        for x in 0..W {
            let c = grid[idx(x, y)];
            if c == 0 || c == NUISANCE || seen[idx(x, y)] {
                continue;
            }
            group.clear();
            stack.clear();
            stack.push((x, y));
            seen[idx(x, y)] = true;
            while let Some((px, py)) = stack.pop() {
                group.push((px, py));
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, ny) = (px + dx, py + dy);
                    if nx < 0 || nx >= W || ny < VIS0 || ny >= H {
                        continue;
                    }
                    let i = idx(nx, ny);
                    if !seen[i] && grid[i] == c {
                        seen[i] = true;
                        stack.push((nx, ny));
                    }
                }
            }
            if group.len() >= 4 {
                sizes.push(group.len() as u32);
                colors |= 1 << c;
                for &(gx, gy) in &group {
                    clear.push(idx(gx, gy) as i32);
                }
            }
        }
    }
    // adjacent nuisance
    let base: Vec<i32> = clear.clone();
    for &i in &base {
        let (x, y) = (i % W, i / W);
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, ny) = (x + dx, y + dy);
            if nx < 0 || nx >= W || ny < VIS0 || ny >= H {
                continue;
            }
            let j = idx(nx, ny) as i32;
            if grid[j as usize] == NUISANCE && !clear.contains(&j) {
                clear.push(j);
            }
        }
    }
    (clear, sizes, colors)
}

/// Score of one link (classic formula, versus power table).
pub fn link_score(link: u32, sizes: &[u32], colors: u32) -> u32 {
    let cleared: u32 = sizes.iter().sum();
    let ncol = colors.count_ones() as usize;
    let gb: u32 = sizes.iter().map(|&s| group_bonus(s)).sum();
    let mult = (chain_power(link) + COLOR_BONUS[ncol.min(6)] + gb).clamp(1, 999);
    10 * cleared * mult
}

/// Resolve a whole chain on a copy (bots and the calculator use this).
pub fn simulate_chain(grid: &mut [u8]) -> (u32, u32, u32) {
    let mut link = 0;
    let mut score = 0;
    let mut popped = 0;
    loop {
        settle_all(grid);
        let (cells, sizes, colors) = find_pops(grid);
        if sizes.is_empty() {
            break;
        }
        link += 1;
        score += link_score(link, &sizes, colors);
        popped += sizes.iter().sum::<u32>();
        for &i in &cells {
            grid[i as usize] = 0;
        }
    }
    (link, score, popped)
}

pub fn gauge_to_nuisance(mut g: u32) -> u32 {
    let mut n = 0;
    while g > 30 {
        n += GAUGE_NUISANCE[30];
        g -= 30;
    }
    n + GAUGE_NUISANCE[g as usize]
}

impl ChainState {
    pub fn new(ctx: &SeatCtx) -> Self {
        let colors = ctx.opts.chain_colors.clamp(3, 5);
        let mut st = ChainState {
            grid: vec![0; (W * H) as usize],
            pair: None,
            queue: VecDeque::new(),
            drop_rng: Rng::stream(ctx.seed, "chain.dropset", ctx.mix()),
            nuis_rng: Rng::stream(ctx.seed, "chain.garbage_color", ctx.mix()),
            colors,
            phase: Phase::Spawn,
            timer: 1,
            lock_timer: 0,
            ground_total: 0,
            das: 0,
            das_dir: 0,
            quick_turn: 0,
            chain: 0,
            max_chain: 0,
            last_chain: 0,
            score: 0,
            leftover: 0,
            target: 70,
            margin_steps: 0,
            ticks: 0,
            pending: Vec::new(),
            all_clear_bonus: false,
            drop_bonus: 0,
            pop_cells: Vec::new(),
            dead: false,
            cross: ctx.cross,
            gauge_carry: 0,
            placements: 0,
            sent_this_chain: 0,
            nuisance_dropped: false,
        };
        st.refill();
        st
    }

    fn refill(&mut self) {
        while self.queue.len() < 4 {
            // First two pairs of a deal use only three colors, so the opening is fair.
            let n = if self.placements == 0 && self.queue.len() < 2 { self.colors.min(3) } else { self.colors };
            let a = 1 + self.drop_rng.below(n as u32) as u8;
            let b = 1 + self.drop_rng.below(n as u32) as u8;
            self.queue.push_back((a, b));
        }
    }

    pub fn next_pairs(&self) -> Vec<(u8, u8)> {
        self.queue.iter().take(2).copied().collect()
    }

    fn spawn(&mut self, out: &mut Out) {
        if self.grid[idx(X_CELL.0, X_CELL.1)] != 0 {
            self.dead = true;
            out.events.push(Ev::TopOut);
            return;
        }
        self.refill();
        let (a, b) = self.queue.pop_front().unwrap();
        self.refill();
        self.pair = Some(Pair { x: 2, y: 1, r: 0, a, b, sub: 0 });
        self.lock_timer = 0;
        self.ground_total = 0;
        self.quick_turn = 0;
        self.phase = Phase::Active;
    }

    fn try_shift(&mut self, dx: i32) -> bool {
        let Some(mut p) = self.pair else { return false };
        p.x += dx;
        if pair_fits(&self.grid, &p) {
            self.pair = Some(p);
            true
        } else {
            false
        }
    }

    fn rotate(&mut self, dir: i32) -> bool {
        let Some(p) = self.pair else { return false };
        let mut np = p;
        np.r = ((p.r as i32 + dir + 4) & 3) as u8;
        if pair_fits(&self.grid, &np) {
            self.pair = Some(np);
            self.quick_turn = 0;
            return true;
        }
        // kick away from the blocked side
        let (kx, ky) = match np.r {
            1 => (-1, 0),
            3 => (1, 0),
            2 => (0, -1),
            _ => (0, 1),
        };
        let mut kp = np;
        kp.x += kx;
        kp.y += ky;
        if kp.y >= 0 && pair_fits(&self.grid, &kp) {
            if ky != 0 {
                kp.sub = 0;
            }
            self.pair = Some(kp);
            self.quick_turn = 0;
            return true;
        }
        // Double rotate: second accepted press inside the window flips 180.
        if np.r == 1 || np.r == 3 {
            if self.quick_turn > 0 {
                let mut fp = p;
                fp.r = (p.r + 2) & 3;
                if !pair_fits(&self.grid, &fp) {
                    fp.y -= 1;
                    fp.sub = 0;
                }
                if fp.y >= 0 && pair_fits(&self.grid, &fp) {
                    self.pair = Some(fp);
                    self.quick_turn = 0;
                    return true;
                }
            } else {
                self.quick_turn = QUICK_TURN_WINDOW;
            }
        }
        false
    }

    fn lock(&mut self, out: &mut Out) {
        let Some(p) = self.pair.take() else { return };
        let (cx, cy) = p.child();
        if p.y >= 0 {
            self.grid[idx(p.x, p.y)] = p.a;
        }
        if cy >= 0 {
            self.grid[idx(cx, cy)] = p.b;
        }
        self.placements += 1;
        out.events.push(Ev::Lock { x: p.x, y: p.y });
        self.chain = 0;
        self.sent_this_chain = 0;
        self.nuisance_dropped = false;
        self.phase = Phase::Settle;
        self.timer = SETTLE_TICKS;
    }

    fn margin(&mut self) {
        if self.ticks >= MARGIN_TICKS {
            let steps = 1 + (self.ticks - MARGIN_TICKS) / MARGIN_STEP;
            while self.margin_steps < steps.min(14) && self.target > 1 {
                self.target = (self.target * 3 / 4).max(1);
                self.margin_steps += 1;
            }
        }
    }

    fn after_settle(&mut self, out: &mut Out) {
        // 14th row vanishes
        for x in 0..W {
            self.grid[idx(x, 0)] = 0;
        }
        let (cells, sizes, colors) = find_pops(&self.grid);
        if !sizes.is_empty() {
            self.chain += 1;
            self.max_chain = self.max_chain.max(self.chain);
            let mut sc = link_score(self.chain, &sizes, colors);
            if self.chain == 1 {
                sc += self.drop_bonus;
                self.drop_bonus = 0;
            }
            self.score += sc as u64;
            let numer = sc + self.leftover;
            let mut sent = numer / self.target;
            self.leftover = numer % self.target;
            if self.chain == 1 && self.all_clear_bonus {
                sent += 30;
                self.all_clear_bonus = false;
            }
            for &i in &cells {
                out.events.push(Ev::Burst { x: i % W, y: i / W, color: self.grid[i as usize] });
            }
            out.events.push(Ev::ChainStep { n: self.chain });
            out.events.push(Ev::Clear {
                power: self.chain * 2 + sizes.len() as u32,
                text: format!("{} CHAIN", self.chain),
            });
            if sent > 0 {
                let tot = sent + self.gauge_carry;
                let gauge = tot / 6;
                self.gauge_carry = tot % 6;
                self.sent_this_chain += sent;
                out.attacks.push(Packet { native: Native::Nuisance(sent), gauge });
            }
            self.pop_cells = cells;
            self.phase = Phase::Pop;
            self.timer = POP_TICKS;
            return;
        }
        // chain over
        if self.chain > 0 {
            self.last_chain = self.chain;
            if self.grid.iter().all(|&v| v == 0) {
                self.all_clear_bonus = true;
                out.events.push(Ev::AllClear);
            }
        }
        if !self.nuisance_dropped {
            self.nuisance_dropped = true;
            if self.drop_nuisance(out) {
                self.phase = Phase::Settle;
                self.timer = SETTLE_TICKS;
                return;
            }
        }
        self.chain = 0;
        self.phase = Phase::Spawn;
        self.timer = SPAWN_DELAY;
    }

    /// Drop up to 30 nuisance from the tray. Returns true if any fell.
    fn drop_nuisance(&mut self, out: &mut Out) -> bool {
        let mut total = 0;
        let mut i = 0;
        while i < self.pending.len() && total < 30 {
            if self.pending[i].tick >= self.ticks {
                i += 1;
                continue;
            }
            let take = self.pending[i].n.min(30 - total);
            self.pending[i].n -= take;
            total += take;
            if self.pending[i].n == 0 {
                self.pending.remove(i);
            } else {
                i += 1;
            }
        }
        if total == 0 {
            return false;
        }
        let rows = total / 6;
        let rem = total % 6;
        let mut cols = [0i32, 1, 2, 3, 4, 5];
        self.nuis_rng.shuffle(&mut cols);
        let mut count = [rows; 6];
        for k in 0..rem as usize {
            count[cols[k] as usize] += 1;
        }
        for x in 0..W {
            let mut y = 0;
            // place from the top; the settle phase lets them fall into place
            for _ in 0..count[x as usize] {
                while y < H && self.grid[idx(x, y)] != 0 {
                    y += 1;
                }
                if y < H {
                    self.grid[idx(x, y)] = NUISANCE;
                }
            }
        }
        out.events.push(Ev::GarbageLand { n: total });
        true
    }

    pub fn step(&mut self, bits: u16, prev: u16, out: &mut Out) {
        self.ticks += 1;
        self.margin();
        if self.dead {
            return;
        }
        let press = bits & !prev;
        match self.phase {
            Phase::Spawn => {
                self.timer = self.timer.saturating_sub(1);
                if self.timer == 0 {
                    self.spawn(out);
                }
            }
            Phase::Settle => {
                self.timer = self.timer.saturating_sub(1);
                if self.timer == 0 {
                    if settle_once(&mut self.grid) {
                        self.timer = SETTLE_TICKS;
                    } else {
                        self.after_settle(out);
                    }
                }
            }
            Phase::Pop => {
                self.timer = self.timer.saturating_sub(1);
                if self.timer == 0 {
                    for &i in &self.pop_cells {
                        self.grid[i as usize] = 0;
                    }
                    self.pop_cells.clear();
                    self.phase = Phase::Settle;
                    self.timer = SETTLE_TICKS;
                }
            }
            Phase::Active => self.step_active(bits, press, out),
        }
    }

    fn step_active(&mut self, bits: u16, press: u16, out: &mut Out) {
        if self.quick_turn > 0 {
            self.quick_turn -= 1;
        }
        if press & B::CW != 0 && self.rotate(1) {
            out.events.push(Ev::Rotate);
        }
        if press & B::CCW != 0 && self.rotate(-1) {
            out.events.push(Ev::Rotate);
        }
        // In-sim repeat: a press moves once; holding charges, then repeats.
        let dir = if bits & B::L != 0 { -1 } else if bits & B::R != 0 { 1 } else { 0 };
        if dir == 0 {
            self.das = 0;
            self.das_dir = 0;
        } else if dir != self.das_dir || press & (B::L | B::R) != 0 {
            self.das_dir = dir;
            self.das = 0;
            if self.try_shift(dir) {
                out.events.push(Ev::Move);
            }
        } else {
            self.das += 1;
            if self.das >= DAS_CHARGE && (self.das - DAS_CHARGE) % DAS_REPEAT == 0 && self.try_shift(dir) {
                out.events.push(Ev::Move);
            }
        }
        let down = bits & (B::D | B::SD) != 0;
        let Some(mut p) = self.pair else { return };
        let speed = if down { SOFT_SPEED } else { FALL_SPEED };
        p.sub += speed;
        let mut landed = false;
        while p.sub >= 16 {
            let mut q = p;
            q.y += 1;
            if pair_fits(&self.grid, &q) {
                p.y += 1;
                p.sub -= 16;
                if down {
                    self.drop_bonus += 1;
                }
            } else {
                p.sub = 0;
                landed = true;
                break;
            }
        }
        let mut q = p;
        q.y += 1;
        let grounded = !pair_fits(&self.grid, &q);
        if grounded {
            p.sub = 0;
        }
        self.pair = Some(p);
        if grounded {
            self.ground_total += 1;
            self.lock_timer += 1;
            if (down && landed) || (down && self.lock_timer > 1) || self.lock_timer >= LOCK_DELAY || self.ground_total >= GROUND_CAP {
                self.lock(out);
            }
        } else {
            self.lock_timer = 0;
        }
    }

    pub fn apply_incoming(&mut self, n: u32) {
        if n > 0 {
            self.pending.push(Pending { n, tick: self.ticks + 1 });
        }
    }
    pub fn pending_units(&self) -> u32 {
        self.pending.iter().map(|g| g.n).sum()
    }
    pub fn cancel_pending(&mut self, mut units: u32) -> u32 {
        let mut c = 0;
        let mut i = 0;
        while units > 0 && i < self.pending.len() {
            if self.pending[i].tick >= self.ticks {
                i += 1;
                continue;
            }
            let k = units.min(self.pending[i].n);
            self.pending[i].n -= k;
            units -= k;
            c += k;
            if self.pending[i].n == 0 {
                self.pending.remove(i);
            } else {
                i += 1;
            }
        }
        c
    }
    pub fn danger(&self) -> bool {
        (VIS0..VIS0 + 3).any(|y| self.grid[idx(X_CELL.0, y)] != 0 || self.grid[idx(X_CELL.0 + 1, y)] != 0)
    }
    pub fn hash(&self, h: &mut Hasher64) {
        h.bytes(&self.grid);
        if let Some(p) = self.pair {
            h.i32(p.x);
            h.i32(p.y);
            h.byte(p.r);
            h.byte(p.a);
            h.byte(p.b);
            h.u32(p.sub);
        }
        h.u32(self.drop_rng.s);
        h.u32(self.nuis_rng.s);
        h.u32(self.leftover);
        h.u32(self.target);
        h.u64(self.score);
        h.u32(self.timer);
        for g in &self.pending {
            h.u32(g.n);
        }
    }
}
