//! Capsule (pill and germ). Profiles capsule.vs (versus attack) and capsule.nes (race / level run).
//! 8 columns x 16 visible rows. Row 0 is the hidden neck where the capsule spawns.

use crate::common::{bits as B, Ev, Native, Out, Packet};
use crate::matchs::SeatCtx;
use crate::rng::{nes_lfsr_step, Hasher64, Rng};

pub const W: i32 = 8;
pub const H: i32 = 17;
pub const VIS0: i32 = 1;

pub const K_EMPTY: u8 = 0;
pub const K_VIRUS: u8 = 1;
pub const K_LEFT: u8 = 2;
pub const K_RIGHT: u8 = 3;
pub const K_TOP: u8 = 4;
pub const K_BOTTOM: u8 = 5;
pub const K_SINGLE: u8 = 6;

/// Nine capsule types: YY YR YB RY RR RB BY BR BB. Colors 1=Y 2=R 3=B.
pub const TYPES: [(u8, u8); 9] = [(1, 1), (1, 2), (1, 3), (2, 1), (2, 2), (2, 3), (3, 1), (3, 2), (3, 3)];

// PROVISIONAL timings (ticks). Capture fills rules/capsule/nes/timings.
const DAS_CHARGE: u32 = 16;
const DAS_REPEAT: u32 = 6;
const SOFT_TICKS: u32 = 2;
const CLEAR_TICKS: u32 = 20;
const FALL_TICKS: u32 = 6;
const SPAWN_TICKS: u32 = 10;
pub const SPEED_NAMES: [&str; 3] = ["LOW", "MED", "HI"];
const SPEED_TPR: [u32; 3] = [40, 26, 16];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pill {
    pub x: i32,
    pub y: i32,
    /// 0: p left, q right. 1: p bottom, q top. 2: q left, p right. 3: q bottom, p top.
    pub o: u8,
    pub p: u8,
    pub q: u8,
}
impl Pill {
    /// (x, y, color, kind) for both halves.
    pub fn cells(&self) -> [(i32, i32, u8, u8); 2] {
        match self.o {
            0 => [(self.x, self.y, self.p, K_LEFT), (self.x + 1, self.y, self.q, K_RIGHT)],
            1 => [(self.x, self.y, self.p, K_BOTTOM), (self.x, self.y - 1, self.q, K_TOP)],
            2 => [(self.x, self.y, self.q, K_LEFT), (self.x + 1, self.y, self.p, K_RIGHT)],
            _ => [(self.x, self.y, self.q, K_BOTTOM), (self.x, self.y - 1, self.p, K_TOP)],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Phase {
    Spawn,
    Active,
    Clear,
    Fall,
}

#[derive(Clone, Copy, Debug)]
pub struct Pending {
    pub n: u32,
    pub tick: u32,
}

#[derive(Clone, Debug)]
pub struct CapsuleState {
    pub color: Vec<u8>,
    pub kind: Vec<u8>,
    pub pill: Option<Pill>,
    pub next: Vec<u8>,
    pub pill_rng: Rng,
    pub virus_rng: Rng,
    pub garbage_rng: Rng,
    pub nes_seeds: bool,
    pub nes_buffer: Vec<u8>,
    pub nes_pos: usize,
    pub nes_seed: u16,
    pub level: u32,
    pub speed: u8,
    pub phase: Phase,
    pub timer: u32,
    pub grav: u32,
    pub das: u32,
    pub das_dir: i32,
    pub viruses: u32,
    pub viruses_start: u32,
    pub pills: u32,
    pub steps: u32,
    pub first_lines: u32,
    pub first_virus: bool,
    pub clearing: Vec<i32>,
    pub pending: Vec<Pending>,
    pub garbage_dropped: bool,
    pub score: u64,
    pub dead: bool,
    pub cross: bool,
    pub race: bool,
    pub level_run: bool,
    pub cleared_all: bool,
    pub levels_cleared: u32,
    pub ticks: u32,
    pub viruses_cleared: u32,
}

fn idx(x: i32, y: i32) -> usize {
    (y * W + x) as usize
}

/// cross.capsule.v1 gauge (original to DR-PURTRIS, provisional).
pub fn capsule_gauge(first_lines: u32, virus_in_first: bool, steps: u32) -> u32 {
    let base = match first_lines {
        0 => 0,
        1 => {
            if virus_in_first {
                1
            } else {
                0
            }
        }
        2 => 1,
        3 => 2,
        _ => 4,
    };
    if first_lines == 0 {
        return 0;
    }
    base + steps.saturating_sub(1)
}

pub fn virus_count(level: u32) -> u32 {
    (4 * (level + 1)).min(84)
}
fn virus_rows(level: u32) -> i32 {
    // PROVISIONAL height cap (rows from the bottom). The original lookup is capture data.
    match level {
        0..=14 => 10,
        15..=16 => 11,
        17..=18 => 12,
        _ => 13,
    }
}

impl CapsuleState {
    pub fn new(ctx: &SeatCtx, race: bool, level_run: bool) -> Self {
        let mut st = CapsuleState {
            color: vec![0; (W * H) as usize],
            kind: vec![0; (W * H) as usize],
            pill: None,
            next: Vec::new(),
            pill_rng: Rng::stream(ctx.seed, "capsule.pills", ctx.mix()),
            virus_rng: Rng::stream(ctx.seed, "capsule.virus", ctx.mix()),
            garbage_rng: Rng::stream(ctx.seed, "capsule.garbage_color", ctx.mix()),
            nes_seeds: ctx.opts.nes_seeds,
            nes_buffer: Vec::new(),
            nes_pos: 1,
            nes_seed: 0,
            level: ctx.opts.capsule_level.min(20),
            speed: ctx.opts.capsule_speed.min(2),
            phase: Phase::Spawn,
            timer: 30,
            grav: 0,
            das: 0,
            das_dir: 0,
            viruses: 0,
            viruses_start: 0,
            pills: 0,
            steps: 0,
            first_lines: 0,
            first_virus: false,
            clearing: Vec::new(),
            pending: Vec::new(),
            garbage_dropped: false,
            score: 0,
            dead: false,
            cross: ctx.cross,
            race,
            level_run,
            cleared_all: false,
            levels_cleared: 0,
            ticks: 0,
            viruses_cleared: 0,
        };
        if st.nes_seeds {
            // Seed shown to the player. 0x0000 and 0x0001 collapse the generator and are rejected.
            let mut s = (ctx.seed as u16) | 0x0100;
            if s <= 1 {
                s = 0x8988;
            }
            st.nes_seed = s;
            st.build_nes_buffer();
        }
        st.fill_bottle();
        st
    }

    /// 128 capsules generated backward from the NES LFSR (unverified against the corpus).
    fn build_nes_buffer(&mut self) {
        let mut seed = self.nes_seed;
        let mut buf = vec![0u8; 128];
        let mut prev: u8 = 0;
        for i in (0..128).rev() {
            seed = nes_lfsr_step(seed);
            let next = (((seed & 0xFF) as u8 % 16) + prev) % 9;
            buf[i] = next;
            prev = next;
        }
        self.nes_buffer = buf;
        self.nes_pos = 1; // play starts at the second capsule
        self.nes_seed = seed;
    }

    fn draw_type(&mut self) -> u8 {
        if self.nes_seeds {
            let t = self.nes_buffer[self.nes_pos % 128];
            self.nes_pos += 1;
            t
        } else {
            if self.next.is_empty() {
                let mut w: Vec<u8> = (0..9).collect();
                self.pill_rng.shuffle(&mut w);
                self.next = w;
            }
            self.next.remove(0)
        }
    }

    pub fn preview(&self) -> Option<(u8, u8)> {
        if self.nes_seeds {
            Some(TYPES[self.nes_buffer[self.nes_pos % 128] as usize])
        } else {
            self.next.first().map(|&t| TYPES[t as usize])
        }
    }

    fn fill_bottle(&mut self) {
        for v in self.color.iter_mut() {
            *v = 0;
        }
        for v in self.kind.iter_mut() {
            *v = 0;
        }
        let want = virus_count(self.level);
        let rows = virus_rows(self.level);
        let top = H - rows;
        let mut placed = 0;
        let mut attempts = 0;
        while placed < want && attempts < 20_000 {
            attempts += 1;
            let mut x = self.virus_rng.below(W as u32) as i32;
            let mut y = top + self.virus_rng.below(rows as u32) as i32;
            // walk to a free cell
            let mut walked = 0;
            while self.kind[idx(x, y)] != 0 && walked < W * rows {
                x += 1;
                if x >= W {
                    x = 0;
                    y += 1;
                    if y >= H {
                        y = top;
                    }
                }
                walked += 1;
            }
            if self.kind[idx(x, y)] != 0 {
                break;
            }
            let start = if placed % 4 == 3 { self.virus_rng.below(3) as u8 } else { (placed % 4) as u8 };
            let mut ok = None;
            for k in 0..3 {
                let c = 1 + (start + k) % 3;
                if !self.two_away_conflict(x, y, c) {
                    ok = Some(c);
                    break;
                }
            }
            if let Some(c) = ok {
                self.color[idx(x, y)] = c;
                self.kind[idx(x, y)] = K_VIRUS;
                placed += 1;
            }
        }
        self.viruses = placed;
        self.viruses_start = placed;
    }

    fn two_away_conflict(&self, x: i32, y: i32, c: u8) -> bool {
        for (dx, dy) in [(2, 0), (-2, 0), (0, 2), (0, -2), (1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, ny) = (x + dx, y + dy);
            if nx >= 0 && nx < W && ny >= 0 && ny < H && self.kind[idx(nx, ny)] == K_VIRUS && self.color[idx(nx, ny)] == c {
                // adjacent same color is allowed only if it cannot make three
                if dx.abs() == 2 || dy.abs() == 2 {
                    return true;
                }
            }
        }
        false
    }

    fn pill_fits(&self, p: &Pill) -> bool {
        p.cells().iter().all(|&(x, y, _, _)| x >= 0 && x < W && y >= 0 && y < H && self.kind[idx(x, y)] == 0)
    }

    fn spawn(&mut self, out: &mut Out) {
        if self.kind[idx(3, VIS0)] != 0 || self.kind[idx(4, VIS0)] != 0 {
            self.dead = true;
            out.events.push(Ev::TopOut);
            return;
        }
        let t = self.draw_type();
        let (p, q) = TYPES[t as usize];
        self.pill = Some(Pill { x: 3, y: VIS0, o: 0, p, q });
        self.grav = 0;
        self.phase = Phase::Active;
        self.pills += 1;
    }

    fn rotate(&mut self, dir: i32) -> bool {
        let Some(p) = self.pill else { return false };
        let mut np = p;
        np.o = ((p.o as i32 + dir + 4) & 3) as u8;
        if self.pill_fits(&np) {
            self.pill = Some(np);
            return true;
        }
        // Vertical to horizontal against a wall or block: try one cell left.
        if np.o % 2 == 0 {
            np.x -= 1;
            if self.pill_fits(&np) {
                self.pill = Some(np);
                return true;
            }
        }
        false
    }

    fn shift(&mut self, dx: i32) -> bool {
        let Some(mut p) = self.pill else { return false };
        p.x += dx;
        if self.pill_fits(&p) {
            self.pill = Some(p);
            true
        } else {
            false
        }
    }

    fn fall_pill(&mut self, out: &mut Out) -> bool {
        let Some(mut p) = self.pill else { return false };
        p.y += 1;
        if self.pill_fits(&p) {
            self.pill = Some(p);
            true
        } else {
            self.lock(out);
            false
        }
    }

    fn lock(&mut self, out: &mut Out) {
        let Some(p) = self.pill.take() else { return };
        for (x, y, c, k) in p.cells() {
            self.color[idx(x, y)] = c;
            self.kind[idx(x, y)] = k;
        }
        out.events.push(Ev::Lock { x: p.x, y: p.y });
        self.steps = 0;
        self.first_lines = 0;
        self.first_virus = false;
        self.garbage_dropped = false;
        self.resolve(out);
    }

    /// Mark 4+ lines. Returns number of separate lines found.
    fn find_lines(&self) -> (Vec<i32>, u32, bool) {
        let mut mark = vec![false; (W * H) as usize];
        let mut lines = 0;
        for y in 0..H {
            let mut x = 0;
            while x < W {
                let c = self.color[idx(x, y)];
                if self.kind[idx(x, y)] == 0 {
                    x += 1;
                    continue;
                }
                let mut e = x + 1;
                while e < W && self.kind[idx(e, y)] != 0 && self.color[idx(e, y)] == c {
                    e += 1;
                }
                if e - x >= 4 {
                    lines += 1;
                    for i in x..e {
                        mark[idx(i, y)] = true;
                    }
                }
                x = e;
            }
        }
        for x in 0..W {
            let mut y = 0;
            while y < H {
                let c = self.color[idx(x, y)];
                if self.kind[idx(x, y)] == 0 {
                    y += 1;
                    continue;
                }
                let mut e = y + 1;
                while e < H && self.kind[idx(x, e)] != 0 && self.color[idx(x, e)] == c {
                    e += 1;
                }
                if e - y >= 4 {
                    lines += 1;
                    for i in y..e {
                        mark[idx(x, i)] = true;
                    }
                }
                y = e;
            }
        }
        let mut cells = Vec::new();
        let mut virus = false;
        for (i, &m) in mark.iter().enumerate() {
            if m {
                cells.push(i as i32);
                if self.kind[i] == K_VIRUS {
                    virus = true;
                }
            }
        }
        (cells, lines, virus)
    }

    fn resolve(&mut self, out: &mut Out) {
        let (cells, lines, virus) = self.find_lines();
        if lines > 0 {
            self.steps += 1;
            if self.steps == 1 {
                self.first_lines = lines;
                self.first_virus = virus;
            }
            let mut killed = 0;
            for &i in &cells {
                if self.kind[i as usize] == K_VIRUS {
                    killed += 1;
                }
                out.events.push(Ev::Burst { x: i % W, y: i / W, color: self.color[i as usize] });
            }
            self.viruses = self.viruses.saturating_sub(killed);
            self.viruses_cleared += killed;
            self.score += (killed as u64) * 100 * (1 + self.speed as u64) * (self.steps as u64);
            if killed > 0 {
                out.events.push(Ev::VirusClear);
            }
            if self.steps > 1 {
                out.events.push(Ev::ChainStep { n: self.steps });
            }
            out.events.push(Ev::Clear {
                power: lines + self.steps,
                text: if self.steps > 1 { format!("{} CHAIN", self.steps) } else if lines > 1 { format!("{} LINES", lines) } else { String::new() },
            });
            self.clearing = cells;
            self.phase = Phase::Clear;
            self.timer = CLEAR_TICKS;
            return;
        }
        // Resolve finished: send, then garbage, then next capsule.
        if self.steps > 0 && !self.race {
            let g = capsule_gauge(self.first_lines, self.first_virus, self.steps);
            if g > 0 {
                out.attacks.push(Packet { native: Native::Halves(2 * g), gauge: g });
            }
            self.steps = 0;
        }
        if self.viruses == 0 && self.viruses_start > 0 && !self.cleared_all {
            self.cleared_all = true;
            out.events.push(Ev::LevelClear);
            if self.level_run {
                self.levels_cleared += 1;
                self.level = (self.level + 1).min(20);
                self.fill_bottle();
                self.cleared_all = false;
                self.phase = Phase::Spawn;
                self.timer = 60;
                return;
            }
        }
        if !self.garbage_dropped {
            self.garbage_dropped = true;
            if self.drop_garbage(out) {
                self.phase = Phase::Fall;
                self.timer = FALL_TICKS;
                return;
            }
        }
        self.phase = Phase::Spawn;
        self.timer = SPAWN_TICKS;
    }

    fn drop_garbage(&mut self, out: &mut Out) -> bool {
        let mut total = 0;
        let mut i = 0;
        while i < self.pending.len() && total < 4 {
            if self.pending[i].tick >= self.ticks {
                i += 1;
                continue;
            }
            let take = self.pending[i].n.min(4 - total);
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
        let mut cols = [0i32, 1, 2, 3, 4, 5, 6, 7];
        self.garbage_rng.shuffle(&mut cols);
        // halves land in alternating columns so they do not form capsules
        let mut placed = 0;
        for &x in cols.iter() {
            if placed >= total {
                break;
            }
            if self.kind[idx(x, VIS0)] == 0 {
                self.kind[idx(x, VIS0)] = K_SINGLE;
                self.color[idx(x, VIS0)] = 1 + self.garbage_rng.below(3) as u8;
                placed += 1;
            }
        }
        out.events.push(Ev::GarbageLand { n: placed });
        placed > 0
    }

    /// Break halves whose partner was removed.
    fn orphan(&mut self) {
        for y in 0..H {
            for x in 0..W {
                let k = self.kind[idx(x, y)];
                let partner = match k {
                    K_LEFT => Some((x + 1, y, K_RIGHT)),
                    K_RIGHT => Some((x - 1, y, K_LEFT)),
                    K_TOP => Some((x, y + 1, K_BOTTOM)),
                    K_BOTTOM => Some((x, y - 1, K_TOP)),
                    _ => None,
                };
                if let Some((px, py, pk)) = partner {
                    let ok = px >= 0 && px < W && py >= 0 && py < H && self.kind[idx(px, py)] == pk;
                    if !ok {
                        self.kind[idx(x, y)] = K_SINGLE;
                    }
                }
            }
        }
    }

    /// Everything unsupported falls one row. Returns true if anything moved.
    fn fall_once(&mut self) -> bool {
        let mut moved = false;
        for y in (0..H - 1).rev() {
            for x in 0..W {
                let k = self.kind[idx(x, y)];
                let empty_below = |s: &Self, xx: i32| s.kind[idx(xx, y + 1)] == 0;
                match k {
                    K_SINGLE | K_BOTTOM => {
                        if empty_below(self, x) {
                            self.move_cell(x, y, x, y + 1);
                            if k == K_BOTTOM && y > 0 && self.kind[idx(x, y - 1)] == K_TOP {
                                self.move_cell(x, y - 1, x, y);
                            }
                            moved = true;
                        }
                    }
                    K_LEFT => {
                        if x + 1 < W && empty_below(self, x) && empty_below(self, x + 1) {
                            self.move_cell(x, y, x, y + 1);
                            self.move_cell(x + 1, y, x + 1, y + 1);
                            moved = true;
                        }
                    }
                    _ => {}
                }
            }
        }
        moved
    }
    fn move_cell(&mut self, x: i32, y: i32, nx: i32, ny: i32) {
        self.kind[idx(nx, ny)] = self.kind[idx(x, y)];
        self.color[idx(nx, ny)] = self.color[idx(x, y)];
        self.kind[idx(x, y)] = 0;
        self.color[idx(x, y)] = 0;
    }

    pub fn step(&mut self, bits: u16, prev: u16, out: &mut Out) {
        self.ticks += 1;
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
            Phase::Clear => {
                self.timer = self.timer.saturating_sub(1);
                if self.timer == 0 {
                    for &i in &self.clearing {
                        self.kind[i as usize] = 0;
                        self.color[i as usize] = 0;
                    }
                    self.clearing.clear();
                    self.orphan();
                    self.phase = Phase::Fall;
                    self.timer = FALL_TICKS;
                }
            }
            Phase::Fall => {
                self.timer = self.timer.saturating_sub(1);
                if self.timer == 0 {
                    if self.fall_once() {
                        self.timer = FALL_TICKS;
                    } else {
                        self.resolve(out);
                    }
                }
            }
            Phase::Active => {
                if press & B::CCW != 0 && self.rotate(1) {
                    out.events.push(Ev::Rotate);
                }
                if press & B::CW != 0 && self.rotate(-1) {
                    out.events.push(Ev::Rotate);
                }
                let dir = if bits & B::L != 0 { -1 } else if bits & B::R != 0 { 1 } else { 0 };
                if dir == 0 {
                    self.das = 0;
                    self.das_dir = 0;
                } else if dir != self.das_dir || press & (B::L | B::R) != 0 {
                    self.das_dir = dir;
                    self.das = 0;
                    if self.shift(dir) {
                        out.events.push(Ev::Move);
                    }
                } else {
                    self.das += 1;
                    if self.das >= DAS_CHARGE && (self.das - DAS_CHARGE) % DAS_REPEAT == 0 && self.shift(dir) {
                        out.events.push(Ev::Move);
                    }
                }
                let tpr = SPEED_TPR[self.speed as usize].saturating_sub(self.pills / 10).max(4);
                let down = bits & (B::D | B::SD) != 0;
                self.grav += 1;
                let due = if down { self.grav >= SOFT_TICKS } else { self.grav >= tpr };
                if due {
                    self.grav = 0;
                    self.fall_pill(out);
                }
            }
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
        (VIS0..VIS0 + 3).any(|y| (2..6).any(|x| self.kind[idx(x, y)] != 0))
    }
    pub fn hash(&self, h: &mut Hasher64) {
        h.bytes(&self.color);
        h.bytes(&self.kind);
        if let Some(p) = self.pill {
            h.i32(p.x);
            h.i32(p.y);
            h.byte(p.o);
            h.byte(p.p);
            h.byte(p.q);
        }
        h.u32(self.pill_rng.s);
        h.u32(self.garbage_rng.s);
        h.u32(self.viruses);
        h.u32(self.timer);
    }
}
