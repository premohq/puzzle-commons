//! Panel (Panel de Pon / Tetris Attack), profile panel.pdp.
//! A rising stack, a two-cell cursor, horizontal swaps. Not a Tetris mode.
//! Rows 0..11 are hidden space above the frame (garbage waits there); rows 12..23 are visible.

use crate::common::{bits as B, Ev, Native, Out, Packet};
use crate::matchs::SeatCtx;
use crate::rng::{Hasher64, Rng};

pub const W: i32 = 6;
pub const H: i32 = 24;
pub const VIS0: i32 = 12;
pub const GARB: u8 = 7;
pub const RISE_STEPS: u32 = 16;

// PROVISIONAL timings (ticks). rules/panel/pdp/timings.toml replaces these after capture.
const SWAP_TICKS: u16 = 4;
const HOVER_TICKS: u16 = 12;
const FLASH_TICKS: u16 = 44;
const POP_TICKS: u16 = 9;
const CUR_CHARGE: u32 = 10;
const CUR_REPEAT: u32 = 2;
const GRACE_TICKS: u32 = 90;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum St {
    #[default]
    Idle,
    Swap,
    Hover,
    Fall,
    Match,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Cell {
    pub c: u8,
    pub s: St,
    pub t: u16,
    pub chain: bool,
    pub g: u16,
    /// ticks remaining at which this panel visibly pops (render only)
    pub pop_at: u16,
    pub dir: i8,
}

#[derive(Clone, Copy, Debug)]
pub struct Block {
    pub id: u16,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    pub crack: u16,
    pub landed: bool,
}

#[derive(Clone, Debug)]
pub struct PanelState {
    pub grid: Vec<Cell>,
    pub blocks: Vec<Block>,
    pub next_row: [u8; 6],
    pub rows_rng: Rng,
    pub transform_rng: Rng,
    pub colors: u8,
    pub cx: i32,
    pub cy: i32,
    pub cur_das: u32,
    pub cur_dir: (i32, i32),
    pub rise_sub: u32,
    pub rise_acc: u32,
    pub rise_ticks: u32,
    pub level: u32,
    pub stop: u32,
    pub chain: u32,
    pub max_chain: u32,
    pub best_combo: u32,
    pub death_timer: u32,
    pub incoming: Vec<(u8, u8)>,
    pub next_id: u16,
    pub drop_side: bool,
    pub rows_risen: u32,
    pub score: u64,
    pub dead: bool,
    pub cross: bool,
    pub ticks: u32,
    pub puzzle: bool,
    pub moves_left: i32,
    pub cursor_travel: u32,
    pub swaps: u32,
    pub manual_raise: bool,
}

#[inline]
fn idx(x: i32, y: i32) -> usize {
    (y * W + x) as usize
}

/// PROVISIONAL garbage chart: combo size -> rectangles.
pub fn combo_rects(combo: u32) -> Vec<(u8, u8)> {
    match combo {
        0..=3 => vec![],
        4 => vec![(3, 1)],
        5 => vec![(4, 1)],
        6 => vec![(5, 1)],
        7 => vec![(6, 1)],
        8 => vec![(3, 1), (4, 1)],
        9 => vec![(4, 1), (4, 1)],
        10 => vec![(5, 1), (5, 1)],
        11 => vec![(5, 1), (6, 1)],
        12 => vec![(6, 1), (6, 1)],
        _ => vec![(6, 1), (6, 1), (6, 1)],
    }
}
/// PROVISIONAL chain chart: chain n -> one 6-wide block of height n-1.
pub fn chain_rects(chain: u32) -> Vec<(u8, u8)> {
    if chain < 2 {
        vec![]
    } else {
        vec![(6, (chain - 1).min(12) as u8)]
    }
}
/// cross.panel.v1: 3 cells = 1 gauge.
pub fn rects_gauge(r: &[(u8, u8)]) -> u32 {
    r.iter().map(|&(w, h)| w as u32 * h as u32).sum::<u32>() / 3
}
/// Incoming gauge becomes a rectangle of the same cell mass.
pub fn gauge_rects(g: u32) -> Vec<(u8, u8)> {
    let cells = g * 3;
    if cells == 0 {
        return vec![];
    }
    if cells <= 6 {
        return vec![(cells.max(3) as u8, 1)];
    }
    let mut v = vec![(6u8, (cells / 6).min(12) as u8)];
    if cells % 6 >= 3 {
        v.push(((cells % 6) as u8, 1));
    }
    v
}

pub fn rise_ticks_for(level: u32) -> u32 {
    // ticks per sub-step; 16 sub-steps per row. Level 1 ~ 9.6 s per row, level 10 ~ 1.1 s.
    [36, 30, 24, 19, 15, 12, 9, 7, 5, 4][(level.clamp(1, 10) - 1) as usize]
}

/// Original puzzles (authored for DR-PURTRIS). Rows are top to bottom; '.' empty.
pub struct Puzzle {
    pub name: &'static str,
    pub moves: i32,
    pub rows: &'static [&'static str],
}
pub const PUZZLES: &[Puzzle] = &[
    Puzzle { name: "First Swap", moves: 1, rows: &["RR.R.."] },
    Puzzle { name: "Twin Towers", moves: 1, rows: &["B.....", "GB....", "BG....", "BG...."] },
    Puzzle { name: "Drop Chain", moves: 1, rows: &["..R...", "RRB.BB"] },
    Puzzle { name: "Two Rows", moves: 2, rows: &["BB.B..", "RR.R.."] },
    Puzzle { name: "Combo Six", moves: 2, rows: &["RR.R..", "GGBGBB"] },
    Puzzle { name: "Long Fall", moves: 1, rows: &["...Y..", "...BYY", "YY.YBB"] },
];
fn letter_color(ch: char) -> u8 {
    match ch {
        'R' => 1,
        'G' => 2,
        'B' => 3,
        'Y' => 4,
        'P' => 5,
        'C' => 6,
        _ => 0,
    }
}

impl PanelState {
    pub fn new(ctx: &SeatCtx, puzzle: Option<usize>) -> Self {
        let level = ctx.opts.panel_level.clamp(1, 10);
        let mut st = PanelState {
            grid: vec![Cell::default(); (W * H) as usize],
            blocks: Vec::new(),
            next_row: [0; 6],
            rows_rng: Rng::stream(ctx.seed, "panel.rows", ctx.mix()),
            transform_rng: Rng::stream(ctx.seed, "panel.transform", ctx.mix()),
            colors: ctx.opts.panel_colors.clamp(4, 6),
            cx: 2,
            cy: H - 4,
            cur_das: 0,
            cur_dir: (0, 0),
            rise_sub: 0,
            rise_acc: 0,
            rise_ticks: rise_ticks_for(level),
            level,
            stop: 120,
            chain: 1,
            max_chain: 1,
            best_combo: 0,
            death_timer: 0,
            incoming: Vec::new(),
            next_id: 1,
            drop_side: false,
            rows_risen: 0,
            score: 0,
            dead: false,
            cross: ctx.cross,
            ticks: 0,
            puzzle: false,
            moves_left: -1,
            cursor_travel: 0,
            swaps: 0,
            manual_raise: false,
        };
        if let Some(pi) = puzzle {
            let pz = &PUZZLES[pi % PUZZLES.len()];
            st.puzzle = true;
            st.moves_left = pz.moves;
            let n = pz.rows.len() as i32;
            for (ry, row) in pz.rows.iter().enumerate() {
                for (x, ch) in row.chars().enumerate().take(6) {
                    let y = H - n + ry as i32;
                    st.grid[idx(x as i32, y)].c = letter_color(ch);
                }
            }
            st.cy = H - 1;
        } else {
            // Initial fair stack: up to 6 rows, uneven tops, never a free match.
            let mut heights = [0i32; 6];
            for h in heights.iter_mut() {
                *h = 3 + st.rows_rng.below(4) as i32;
            }
            for y in (H - 6..H).rev() {
                for x in 0..W {
                    if H - y <= heights[x as usize] {
                        let c = st.safe_color(x, y);
                        st.grid[idx(x, y)].c = c;
                    }
                }
            }
            st.gen_next_row();
        }
        st
    }

    fn safe_color(&mut self, x: i32, y: i32) -> u8 {
        for _ in 0..16 {
            let c = 1 + self.rows_rng.below(self.colors as u32) as u8;
            let h = x >= 2 && self.grid[idx(x - 1, y)].c == c && self.grid[idx(x - 2, y)].c == c;
            let v = y + 2 < H && self.grid[idx(x, y + 1)].c == c && self.grid[idx(x, y + 2)].c == c;
            if !h && !v {
                return c;
            }
        }
        1 + self.rows_rng.below(self.colors as u32) as u8
    }

    /// Fair row generator. The design's six independent draws allowed rows that arrive
    /// already matched (a free clear); this redraws a color that would complete three.
    fn gen_next_row(&mut self) {
        let mut row = [0u8; 6];
        for x in 0..6usize {
            let mut c = 0;
            for _ in 0..16 {
                c = 1 + self.rows_rng.below(self.colors as u32) as u8;
                let h = x >= 2 && row[x - 1] == c && row[x - 2] == c;
                let above1 = self.grid[idx(x as i32, H - 1)].c;
                let above2 = self.grid[idx(x as i32, H - 2)].c;
                let v = above1 == c && above2 == c;
                if !h && !v {
                    break;
                }
            }
            row[x] = c;
        }
        self.next_row = row;
    }

    fn swappable(&self, x: i32, y: i32) -> bool {
        let c = self.grid[idx(x, y)];
        if c.c == GARB {
            return false;
        }
        match c.s {
            St::Idle | St::Fall => true,
            _ => false,
        }
    }

    fn try_swap(&mut self, out: &mut Out) {
        let (x, y) = (self.cx, self.cy);
        if !self.swappable(x, y) || !self.swappable(x + 1, y) {
            return;
        }
        let a = self.grid[idx(x, y)];
        let b = self.grid[idx(x + 1, y)];
        if a.c == 0 && b.c == 0 {
            return;
        }
        // A panel cannot slide under one that is hovering right above the gap.
        for (cx, other) in [(x, b), (x + 1, a)] {
            if other.c != 0 && y > 0 {
                let above = self.grid[idx(cx, y - 1)];
                if above.c != 0 && above.c != GARB && above.s == St::Hover && self.grid[idx(cx, y)].c == 0 {
                    return;
                }
            }
        }
        let mut na = b;
        let mut nb = a;
        na.s = St::Swap;
        na.t = SWAP_TICKS;
        na.dir = -1;
        nb.s = St::Swap;
        nb.t = SWAP_TICKS;
        nb.dir = 1;
        self.grid[idx(x, y)] = na;
        self.grid[idx(x + 1, y)] = nb;
        self.swaps += 1;
        if self.puzzle && self.moves_left > 0 {
            self.moves_left -= 1;
        }
        out.events.push(Ev::Swap);
    }

    fn move_cursor(&mut self, dx: i32, dy: i32) -> bool {
        let nx = (self.cx + dx).clamp(0, W - 2);
        let ny = (self.cy + dy).clamp(VIS0, H - 1);
        if nx != self.cx || ny != self.cy {
            self.cx = nx;
            self.cy = ny;
            self.cursor_travel += 1;
            true
        } else {
            false
        }
    }

    fn input(&mut self, bits: u16, press: u16, out: &mut Out) {
        let dx = if bits & B::L != 0 { -1 } else if bits & B::R != 0 { 1 } else { 0 };
        let dy = if bits & B::U != 0 { -1 } else if bits & B::D != 0 { 1 } else { 0 };
        let dir = (dx, dy);
        if dir == (0, 0) {
            self.cur_das = 0;
            self.cur_dir = dir;
        } else if dir != self.cur_dir || press & (B::L | B::R | B::U | B::D) != 0 {
            self.cur_dir = dir;
            self.cur_das = 0;
            if self.move_cursor(dx, dy) {
                out.events.push(Ev::Move);
            }
        } else {
            self.cur_das += 1;
            if self.cur_das >= CUR_CHARGE && (self.cur_das - CUR_CHARGE) % CUR_REPEAT == 0 && self.move_cursor(dx, dy) {
                out.events.push(Ev::Move);
            }
        }
        if press & B::SWAP != 0 && (!self.puzzle || self.moves_left != 0) {
            self.try_swap(out);
        }
        self.manual_raise = bits & B::RAISE != 0 && !self.puzzle;
    }

    fn busy(&self) -> bool {
        self.grid.iter().any(|c| c.s == St::Match) || self.blocks.iter().any(|b| b.crack > 0)
    }

    pub fn step(&mut self, bits: u16, prev: u16, out: &mut Out) {
        self.ticks += 1;
        if self.dead {
            return;
        }
        let press = bits & !prev;
        self.input(bits, press, out);
        self.update_timers(out);
        self.gravity(out);
        self.spawn_garbage();
        self.detect(out);
        self.chain_end(out);
        if !self.puzzle {
            self.rise(out);
        }
    }

    fn update_timers(&mut self, out: &mut Out) {
        let mut cleared_cols: Vec<(i32, i32)> = Vec::new();
        for y in 0..H {
            for x in 0..W {
                let c = &mut self.grid[idx(x, y)];
                match c.s {
                    St::Swap => {
                        c.t = c.t.saturating_sub(1);
                        if c.t == 0 {
                            c.s = St::Idle;
                            c.dir = 0;
                        }
                    }
                    St::Hover => {
                        c.t = c.t.saturating_sub(1);
                        if c.t == 0 {
                            c.s = St::Fall;
                        }
                    }
                    St::Match => {
                        c.t = c.t.saturating_sub(1);
                        if c.t == 0 {
                            *c = Cell::default();
                            cleared_cols.push((x, y));
                        }
                    }
                    _ => {}
                }
            }
        }
        // Panels above a clear hover and carry the chain flag.
        for (x, y) in cleared_cols {
            let mut yy = y - 1;
            while yy >= 0 {
                let c = &mut self.grid[idx(x, yy)];
                if c.c == 0 || c.c == GARB || c.s == St::Match || c.s == St::Swap {
                    break;
                }
                c.s = St::Hover;
                c.t = HOVER_TICKS;
                c.chain = true;
                yy -= 1;
            }
        }
        // Garbage cracks
        let mut i = 0;
        while i < self.blocks.len() {
            if self.blocks[i].crack > 0 {
                self.blocks[i].crack -= 1;
                if self.blocks[i].crack == 0 {
                    let b = self.blocks[i];
                    let by = b.y + b.h - 1;
                    for x in b.x..b.x + b.w {
                        let col = 1 + self.transform_rng.below(self.colors as u32) as u8;
                        if by >= 0 {
                            self.grid[idx(x, by)] = Cell { c: col, s: St::Hover, t: HOVER_TICKS, chain: true, ..Cell::default() };
                        }
                        out.events.push(Ev::Burst { x, y: by, color: GARB });
                    }
                    self.blocks[i].h -= 1;
                    if self.blocks[i].h <= 0 {
                        self.blocks.remove(i);
                        continue;
                    }
                }
            }
            i += 1;
        }
    }

    fn gravity(&mut self, _out: &mut Out) {
        for y in (0..H).rev() {
            for x in 0..W {
                let c = self.grid[idx(x, y)];
                if c.c == 0 || c.c == GARB {
                    continue;
                }
                let below_free = y + 1 < H && {
                    let b = self.grid[idx(x, y + 1)];
                    b.c == 0 && b.s != St::Swap
                };
                match c.s {
                    St::Fall => {
                        if below_free {
                            self.grid[idx(x, y + 1)] = c;
                            self.grid[idx(x, y)] = Cell::default();
                        } else {
                            self.grid[idx(x, y)].s = St::Idle;
                        }
                    }
                    St::Idle => {
                        if below_free {
                            let cell = &mut self.grid[idx(x, y)];
                            cell.s = St::Hover;
                            cell.t = HOVER_TICKS;
                        }
                    }
                    _ => {}
                }
            }
        }
        // Garbage blocks fall as units, lowest first.
        let mut order: Vec<usize> = (0..self.blocks.len()).collect();
        order.sort_by_key(|&i| -(self.blocks[i].y + self.blocks[i].h));
        for i in order {
            let b = self.blocks[i];
            if b.crack > 0 {
                continue;
            }
            let by = b.y + b.h;
            let can = by < H && (b.x..b.x + b.w).all(|x| {
                let c = self.grid[idx(x, by)];
                c.c == 0 && c.s != St::Swap
            });
            if can {
                for y in (b.y.max(0)..b.y + b.h).rev() {
                    for x in b.x..b.x + b.w {
                        self.grid[idx(x, y + 1)] = self.grid[idx(x, y)];
                        self.grid[idx(x, y)] = Cell::default();
                    }
                }
                self.blocks[i].y += 1;
            } else if !b.landed {
                self.blocks[i].landed = true;
                _out.events.push(Ev::GarbageLand { n: (b.w * b.h) as u32 });
            }
        }
    }

    fn spawn_garbage(&mut self) {
        if self.incoming.is_empty() {
            return;
        }
        let (w, h) = self.incoming[0];
        let (w, h) = (w as i32, h as i32);
        let x = if w >= W {
            0
        } else if self.drop_side {
            W - w
        } else {
            0
        };
        if (0..h).all(|y| (x..x + w).all(|xx| self.grid[idx(xx, y)].c == 0)) {
            self.incoming.remove(0);
            self.drop_side = !self.drop_side;
            let id = self.next_id;
            self.next_id = self.next_id.wrapping_add(1).max(1);
            for y in 0..h {
                for xx in x..x + w {
                    self.grid[idx(xx, y)] = Cell { c: GARB, g: id, ..Cell::default() };
                }
            }
            self.blocks.push(Block { id, x, y: 0, w, h, crack: 0, landed: false });
        }
    }

    fn detect(&mut self, out: &mut Out) {
        let elig = |c: &Cell| c.c >= 1 && c.c <= 6 && c.s == St::Idle;
        let mut mark = vec![false; (W * H) as usize];
        for y in VIS0..H {
            let mut x = 0;
            while x < W {
                let c = self.grid[idx(x, y)];
                if !elig(&c) {
                    x += 1;
                    continue;
                }
                let mut e = x + 1;
                while e < W && elig(&self.grid[idx(e, y)]) && self.grid[idx(e, y)].c == c.c {
                    e += 1;
                }
                if e - x >= 3 {
                    for i in x..e {
                        mark[idx(i, y)] = true;
                    }
                }
                x = e;
            }
        }
        for x in 0..W {
            let mut y = VIS0;
            while y < H {
                let c = self.grid[idx(x, y)];
                if !elig(&c) {
                    y += 1;
                    continue;
                }
                let mut e = y + 1;
                while e < H && elig(&self.grid[idx(x, e)]) && self.grid[idx(x, e)].c == c.c {
                    e += 1;
                }
                if e - y >= 3 {
                    for i in y..e {
                        mark[idx(x, i)] = true;
                    }
                }
                y = e;
            }
        }
        let cells: Vec<usize> = (0..mark.len()).filter(|&i| mark[i]).collect();
        // clear chain flags on idle panels that landed without matching
        if cells.is_empty() {
            for c in self.grid.iter_mut() {
                if c.s == St::Idle && c.chain {
                    c.chain = false;
                }
            }
            return;
        }
        let combo = cells.len() as u32;
        let chain_hit = cells.iter().any(|&i| self.grid[i].chain);
        if chain_hit {
            self.chain += 1;
            self.max_chain = self.max_chain.max(self.chain);
        }
        self.best_combo = self.best_combo.max(combo);
        let total = FLASH_TICKS + POP_TICKS * combo as u16;
        for (k, &i) in cells.iter().enumerate() {
            let c = &mut self.grid[i];
            c.s = St::Match;
            c.t = total;
            c.pop_at = total - FLASH_TICKS - POP_TICKS * k as u16;
            out.events.push(Ev::Burst { x: i as i32 % W, y: i as i32 / W, color: c.c });
        }
        for c in self.grid.iter_mut() {
            if c.s == St::Idle && c.chain {
                c.chain = false;
            }
        }
        self.score += 10 * combo as u64 + if chain_hit { 50 * self.chain as u64 } else { 0 };
        let mut text = String::new();
        if chain_hit {
            out.events.push(Ev::ChainStep { n: self.chain });
            text = format!("{} CHAIN", self.chain);
        }
        if combo >= 4 {
            if !text.is_empty() {
                text.push_str("  ");
            }
            text.push_str(&format!("{} COMBO", combo));
        }
        out.events.push(Ev::Clear { power: combo / 2 + self.chain * 2, text });
        // stop time
        let add = (if combo >= 4 { 40 + 10 * (combo - 4) } else { 0 }) + if chain_hit { 80 + 20 * self.chain } else { 0 };
        self.stop = self.stop.max(add);
        // combo garbage goes out immediately
        let rects = combo_rects(combo);
        if !rects.is_empty() {
            let g = rects_gauge(&rects);
            out.attacks.push(Packet { native: Native::Rects(rects), gauge: g });
        }
        // crack adjacent garbage (and garbage touching it)
        let mut crack_ids: Vec<u16> = Vec::new();
        for &i in &cells {
            let (x, y) = (i as i32 % W, i as i32 / W);
            for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx >= 0 && nx < W && ny >= 0 && ny < H {
                    let c = self.grid[idx(nx, ny)];
                    if c.c == GARB && !crack_ids.contains(&c.g) {
                        crack_ids.push(c.g);
                    }
                }
            }
        }
        let mut k = 0;
        while k < crack_ids.len() {
            let id = crack_ids[k];
            if let Some(b) = self.blocks.iter().find(|b| b.id == id).copied() {
                for y in b.y.max(0) - 1..=b.y + b.h {
                    for x in b.x - 1..=b.x + b.w {
                        if x >= 0 && x < W && y >= 0 && y < H {
                            let c = self.grid[idx(x, y)];
                            if c.c == GARB && c.g != id && !crack_ids.contains(&c.g) {
                                crack_ids.push(c.g);
                            }
                        }
                    }
                }
            }
            k += 1;
        }
        for b in self.blocks.iter_mut() {
            if crack_ids.contains(&b.id) && b.crack == 0 {
                b.crack = total + 6;
            }
        }
    }

    fn chain_end(&mut self, out: &mut Out) {
        if self.chain <= 1 {
            return;
        }
        let active = self.grid.iter().any(|c| c.chain || c.s == St::Match) || self.blocks.iter().any(|b| b.crack > 0);
        if !active {
            let rects = chain_rects(self.chain);
            let g = rects_gauge(&rects);
            out.attacks.push(Packet { native: Native::Rects(rects), gauge: g });
            self.chain = 1;
        }
    }

    fn rise(&mut self, out: &mut Out) {
        if self.busy() || self.blocks.iter().any(|b| !b.landed) {
            return;
        }
        let topped = (0..W).any(|x| self.grid[idx(x, VIS0)].c != 0) || self.blocks.iter().any(|b| b.y < VIS0 && b.landed);
        if self.manual_raise && !topped {
            self.stop = 0;
        }
        if self.stop > 0 {
            self.stop -= 1;
            return;
        }
        if topped {
            self.death_timer += 1;
            if self.death_timer >= GRACE_TICKS {
                self.dead = true;
                out.events.push(Ev::TopOut);
            }
            return;
        }
        self.death_timer = 0;
        if self.manual_raise {
            self.rise_sub += 2;
        } else {
            self.rise_acc += 1;
            if self.rise_acc >= self.rise_ticks {
                self.rise_acc = 0;
                self.rise_sub += 1;
            }
        }
        if self.rise_sub >= RISE_STEPS {
            self.rise_sub = 0;
            // shift everything up one row
            for y in 0..H - 1 {
                for x in 0..W {
                    self.grid[idx(x, y)] = self.grid[idx(x, y + 1)];
                }
            }
            for x in 0..W {
                self.grid[idx(x, H - 1)] = Cell { c: self.next_row[x as usize], ..Cell::default() };
            }
            for b in self.blocks.iter_mut() {
                b.y -= 1;
            }
            self.cy = (self.cy - 1).max(VIS0);
            self.rows_risen += 1;
            self.score += 1;
            self.gen_next_row();
            out.events.push(Ev::Rise);
            if self.rows_risen % 30 == 0 && self.level < 10 {
                self.level += 1;
                self.rise_ticks = rise_ticks_for(self.level);
            }
        }
    }

    pub fn apply_incoming(&mut self, rects: &[(u8, u8)]) {
        for &r in rects {
            if r.0 > 0 && r.1 > 0 {
                self.incoming.push((r.0.min(6), r.1.min(12)));
            }
        }
    }
    pub fn pending_units(&self) -> u32 {
        self.incoming.iter().map(|&(w, h)| w as u32 * h as u32).sum()
    }
    pub fn danger(&self) -> bool {
        (VIS0..VIS0 + 3).any(|y| (0..W).any(|x| self.grid[idx(x, y)].c != 0))
    }
    pub fn puzzle_state(&self) -> Option<bool> {
        if !self.puzzle {
            return None;
        }
        let settled = self.grid.iter().all(|c| c.s == St::Idle) && !self.busy();
        let empty = self.grid.iter().all(|c| c.c == 0);
        if empty && settled {
            Some(true)
        } else if self.moves_left == 0 && settled {
            Some(false)
        } else {
            None
        }
    }
    pub fn hash(&self, h: &mut Hasher64) {
        for c in &self.grid {
            h.byte(c.c);
            h.byte(c.s as u8);
            h.bytes(&c.t.to_le_bytes());
            h.byte(c.chain as u8);
        }
        h.bytes(&self.next_row);
        h.i32(self.cx);
        h.i32(self.cy);
        h.u32(self.rise_sub);
        h.u32(self.stop);
        h.u32(self.rows_rng.s);
        h.u32(self.transform_rng.s);
        for b in &self.blocks {
            h.i32(b.x);
            h.i32(b.y);
            h.i32(b.h);
        }
    }
}
