//! purtris-bot: deterministic AI. bot_input is a pure function of the published match state,
//! the bot profile, and the bot memory (whose RNG is seeded from `bot.mistakes`).
//! Bots never read another human's raw input and never exceed the sim's legal input rate.

use purtris_sim::capsule::{self, CapsuleState};
use purtris_sim::chain::{self, ChainState};
use purtris_sim::common::bits as B;
use purtris_sim::matchs::{Board, BotMem, Match, Mode, Personality};
use purtris_sim::panel::{self, PanelState};
use purtris_sim::quad::{self, QuadState};

/// Ticks between actions, by strength 0..=8. Strength 0 is the beginner the story's first
/// Easy matches use: slow, visibly thinking, often wrong (the versus room offers 1..=8).
const INTERVAL: [u16; 9] = [36, 22, 15, 11, 8, 6, 4, 3, 2];
/// Mistake percentage, by strength.
const MISTAKE: [u32; 9] = [45, 30, 20, 14, 9, 6, 3, 1, 0];

fn interval(s: u8) -> u16 {
    INTERVAL[s.min(8) as usize]
}
fn mistake(s: u8) -> u32 {
    MISTAKE[s.min(8) as usize]
}
/// Ticks between sideways taps: the beginner slides pieces at a pace a person can follow.
/// (Strengths 1..=8 keep their original timing so saved replays still resimulate.)
fn slide_gap(s: u8, gap: u16) -> u16 {
    if s == 0 {
        9
    } else {
        gap.min(3)
    }
}

/// The bot input for one seat this tick. Call before Match::step.
pub fn bot_input(m: &Match, seat: usize, mem: &mut BotMem) -> u16 {
    let cfg = m.seats[seat].cfg;
    if !m.seats[seat].alive {
        return 0;
    }
    let Some(board) = &m.seats[seat].board else { return 0 };
    match board {
        Board::Quad(q) => quad_bot(q, cfg.personality, cfg.strength, mem, m),
        Board::Chain(c) => chain_bot(c, cfg.personality, cfg.strength, mem, m, seat),
        Board::Capsule(c) => capsule_bot(c, cfg.personality, cfg.strength, mem, m.header.mode),
        Board::Panel(p) => panel_bot(p, cfg.personality, cfg.strength, mem),
    }
}

/// Play the queued (bits, ticks) actions.
fn play_queue(mem: &mut BotMem) -> u16 {
    while let Some(&(bits, ticks)) = mem.queue.front() {
        if ticks == 0 {
            mem.queue.pop_front();
            continue;
        }
        let f = mem.queue.front_mut().unwrap();
        f.1 -= 1;
        return bits;
    }
    mem.hold_bits
}

fn push_tap(mem: &mut BotMem, bits: u16, gap: u16) {
    mem.queue.push_back((bits, 1));
    mem.queue.push_back((0, gap.max(1)));
}

fn pick<T: Copy>(mem: &mut BotMem, ranked: &[(i64, T)], personality: Personality, strength: u8) -> Option<T> {
    if ranked.is_empty() {
        return None;
    }
    if personality == Personality::RandomLegal {
        let i = mem.rng.below(ranked.len() as u32) as usize;
        return Some(ranked[i].1);
    }
    if mem.rng.chance(mistake(strength)) {
        let k = ranked.len().min(4) as u32;
        let i = mem.rng.below(k) as usize;
        return Some(ranked[i].1);
    }
    Some(ranked[0].1)
}

// ---------------------------------------------------------------- Quad

fn quad_eval(grid: &[u8], cleared: u32, personality: Personality, danger: bool, well: bool, beginner: bool) -> i64 {
    let w = quad::W;
    let h = quad::H;
    let mut heights = [0i64; 10];
    for x in 0..w {
        for y in 0..h {
            if grid[(y * w + x) as usize] != 0 {
                heights[x as usize] = (h - y) as i64;
                break;
            }
        }
    }
    let agg: i64 = heights.iter().sum();
    let max_h = *heights.iter().max().unwrap();
    let holes = quad::count_holes(grid) as i64;
    let mut bump = 0;
    for x in 0..9 {
        bump += (heights[x] - heights[x + 1]).abs();
    }
    let mut score = -51 * agg - 36 * holes * if personality == Personality::Digger { 3 } else { 2 } - 18 * bump;
    score -= if max_h > 14 { (max_h - 14) * 120 } else { 0 };
    let line_value = match personality {
        Personality::Aggressor => [0, 120, 300, 500, 900],
        // the beginner clears whatever it can: small clears send little or nothing
        _ if beginner => [0, 160, 260, 320, 420],
        _ if danger => [0, 200, 400, 600, 1000],
        _ => [0, -40, 40, 200, 1200],
    };
    score += line_value[cleared.min(4) as usize];
    if well && !danger {
        // keep column 9 open for quads
        let col9 = heights[9];
        let others = heights[..9].iter().copied().min().unwrap_or(0);
        if col9 > others {
            score -= (col9 - others) * 60;
        }
        // undo the bumpiness penalty from the well itself
        score += 18 * (heights[8] - heights[9]).abs();
    }
    score
}

fn quad_bot(q: &QuadState, p: Personality, s: u8, mem: &mut BotMem, m: &Match) -> u16 {
    let key = q.pieces as u64 * 2 + q.hold_used as u64 + 1;
    if q.cur.is_none() {
        mem.queue.clear();
        mem.key = 0;
        mem.hold_bits = 0;
        return 0;
    }
    if mem.key != key {
        mem.key = key;
        mem.queue.clear();
        mem.hold_bits = 0;
        let cur = q.cur.unwrap();
        let danger = q.height() >= 14 || q.pending_units() >= 4;
        let well = matches!(p, Personality::Builder) && m.header.mode != Mode::QuadSprint;
        let mut ranked: Vec<(i64, (bool, u8, i32))> = Vec::new();
        let mut options = vec![(false, cur.t)];
        if q.cfg.hold && !q.hold_used && s >= 3 {
            let ht = q.hold.unwrap_or_else(|| *q.queue.front().unwrap_or(&cur.t));
            if ht != cur.t {
                options.push((true, ht));
            }
        }
        for &(use_hold, t) in &options {
            for r in 0..4u8 {
                for x in -2..quad::W {
                    let y0 = cur.y;
                    if !quad::fits(&q.grid, t, r, x, y0) {
                        continue;
                    }
                    // path: rotate at spawn, then slide sideways along the spawn row
                    let x0 = 3;
                    let step = if x > x0 { 1 } else { -1 };
                    let mut ok = quad::fits(&q.grid, t, r, x0, y0);
                    let mut cx = x0;
                    while ok && cx != x {
                        cx += step;
                        ok = quad::fits(&q.grid, t, r, cx, y0);
                    }
                    if !ok {
                        continue;
                    }
                    let y = quad::drop_y(&q.grid, t, r, x, y0);
                    let mut g = q.grid.clone();
                    for &(dx, dy) in quad::SHAPES[t as usize][r as usize].iter() {
                        let (gx, gy) = (x + dx, y + dy);
                        if gy >= 0 {
                            g[(gy * quad::W + gx) as usize] = t + 1;
                        }
                    }
                    let mut cleared = 0;
                    let mut ng = vec![0u8; g.len()];
                    let mut wy = quad::H - 1;
                    for yy in (0..quad::H).rev() {
                        let row = &g[(yy * quad::W) as usize..((yy + 1) * quad::W) as usize];
                        if row.iter().all(|&v| v != 0) {
                            cleared += 1;
                            continue;
                        }
                        ng[(wy * quad::W) as usize..((wy + 1) * quad::W) as usize].copy_from_slice(row);
                        wy -= 1;
                    }
                    let mut sc = quad_eval(&ng, cleared, p, danger, well, s == 0);
                    if use_hold {
                        sc -= 5;
                    }
                    ranked.push((sc, (use_hold, r, x)));
                }
            }
        }
        ranked.sort_by(|a, b| b.0.cmp(&a.0));
        let gap = interval(s);
        let Some((use_hold, r, x)) = pick(mem, &ranked, p, s) else {
            push_tap(mem, B::HD, gap);
            return play_queue(mem);
        };
        mem.queue.push_back((0, gap));
        if use_hold {
            push_tap(mem, B::HOLD, gap);
            // replan after the hold swaps pieces
            return play_queue(mem);
        }
        match r {
            1 => push_tap(mem, B::CW, gap),
            2 => {
                push_tap(mem, B::CW, gap);
                push_tap(mem, B::CW, gap);
            }
            3 => push_tap(mem, B::CCW, gap),
            _ => {}
        }
        let dx = x - 3;
        let bit = if dx < 0 { B::L } else { B::R };
        for _ in 0..dx.abs() {
            push_tap(mem, bit, slide_gap(s, gap));
        }
        if s == 0 {
            // the beginner never hard-drops: it pauses, then holds soft drop
            mem.queue.push_back((0, gap));
            mem.hold_bits = B::SD;
        } else {
            push_tap(mem, B::HD, gap);
        }
    }
    play_queue(mem)
}

// ---------------------------------------------------------------- Chain

fn chain_place(grid: &[u8], x: i32, r: u8, a: u8, b: u8) -> Option<Vec<u8>> {
    let w = chain::W;
    let (cx, _) = match r {
        0 => (x, 0),
        1 => (x + 1, 0),
        2 => (x, 0),
        _ => (x - 1, 0),
    };
    if x < 0 || x >= w || cx < 0 || cx >= w {
        return None;
    }
    let mut g = grid.to_vec();
    let top = |g: &Vec<u8>, col: i32| -> i32 {
        let mut y = chain::H - 1;
        while y >= 0 && g[(y * w + col) as usize] != 0 {
            y -= 1;
        }
        y
    };
    // column must be reachable: spawn column 2, lanes between must be low enough
    let (lo, hi) = (x.min(cx).min(2), x.max(cx).max(2));
    for col in lo..=hi {
        if g[(2 * w + col) as usize] != 0 {
            return None;
        }
    }
    let (first, second) = match r {
        0 => ((x, a), (x, b)),   // child above
        2 => ((x, b), (x, a)),   // child below lands first
        1 => ((x, a), (cx, b)),
        _ => ((x, a), (cx, b)),
    };
    let y1 = top(&g, first.0);
    if y1 < 1 {
        return None;
    }
    g[(y1 * w + first.0) as usize] = first.1;
    let y2 = top(&g, second.0);
    if y2 < 0 {
        return None;
    }
    g[(y2 * w + second.0) as usize] = second.1;
    Some(g)
}

fn chain_shape(grid: &[u8]) -> i64 {
    let w = chain::W;
    let h = chain::H;
    let mut s = 0i64;
    for y in chain::VIS0..h {
        for x in 0..w {
            let c = grid[(y * w + x) as usize];
            if c == 0 || c == chain::NUISANCE {
                continue;
            }
            if x + 1 < w && grid[(y * w + x + 1) as usize] == c {
                s += 12;
            }
            if y + 1 < h && grid[((y + 1) * w + x) as usize] == c {
                s += 14;
            }
        }
    }
    for x in 0..w {
        let mut ht = 0;
        for y in 0..h {
            if grid[(y * w + x) as usize] != 0 {
                ht = h - y;
                break;
            }
        }
        let weight = if x == 2 || x == 3 { 14 } else { 4 };
        s -= (ht as i64) * (ht as i64) * weight / 4;
    }
    s
}

fn chain_potential(grid: &[u8], colors: u8) -> u32 {
    let mut best = 0;
    for x in 0..chain::W {
        for c in 1..=colors {
            let mut g = grid.to_vec();
            let mut y = chain::H - 1;
            while y >= 0 && g[(y * chain::W + x) as usize] != 0 {
                y -= 1;
            }
            if y < chain::VIS0 {
                continue;
            }
            g[(y * chain::W + x) as usize] = c;
            let (l, _, _) = chain::simulate_chain(&mut g);
            best = best.max(l);
        }
    }
    best
}

fn chain_bot(c: &ChainState, p: Personality, s: u8, mem: &mut BotMem, m: &Match, seat: usize) -> u16 {
    let Some(pair) = c.pair else {
        mem.queue.clear();
        mem.key = 0;
        mem.hold_bits = 0;
        return 0;
    };
    let key = c.placements as u64 + 1;
    if mem.key != key {
        mem.key = key;
        mem.queue.clear();
        mem.hold_bits = 0;
        let incoming = m.incoming(seat);
        let threat = incoming.0 + incoming.1;
        let tallest = (0..chain::W)
            .map(|x| (0..chain::H).find(|&y| c.grid[(y * chain::W + x) as usize] != 0).map(|y| chain::H - y).unwrap_or(0))
            .max()
            .unwrap_or(0);
        let height_danger = c.danger() || tallest >= 10;
        let want = match p {
            Personality::Aggressor => 2,
            Personality::RandomLegal => 1,
            Personality::Digger => 2,
            // the beginner pops a group as soon as it can rather than building a chain
            _ if s == 0 => 1,
            _ => 2 + (s as u32 / 2).min(3),
        };
        let mut ranked: Vec<(i64, (i32, u8))> = Vec::new();
        for r in 0..4u8 {
            for x in 0..chain::W {
                let Some(mut g) = chain_place(&c.grid, x, r, pair.a, pair.b) else { continue };
                let (links, score, _) = chain::simulate_chain(&mut g);
                let mut sc = chain_shape(&g);
                if links > 0 {
                    let fire = links >= want || height_danger || (threat >= 6 && links >= 1) || p == Personality::Aggressor;
                    if fire {
                        sc += 400 * links as i64 * links as i64 + score as i64 / 10;
                    } else {
                        sc -= 300; // do not waste the resource
                    }
                } else if s >= 5 && p != Personality::RandomLegal {
                    sc += 120 * chain_potential(&g, c.colors).min(8) as i64;
                }
                if g[(chain::X_CELL.1 * chain::W + chain::X_CELL.0) as usize] != 0 {
                    sc -= 100_000;
                }
                ranked.push((sc, (x, r)));
            }
        }
        ranked.sort_by(|a, b| b.0.cmp(&a.0));
        let gap = interval(s);
        if let Some((x, r)) = pick(mem, &ranked, p, s) {
            mem.queue.push_back((0, gap));
            match r {
                1 => push_tap(mem, B::CW, gap),
                2 => {
                    push_tap(mem, B::CW, gap);
                    push_tap(mem, B::CW, gap);
                }
                3 => push_tap(mem, B::CCW, gap),
                _ => {}
            }
            let dx = x - pair.x;
            for _ in 0..dx.abs() {
                push_tap(mem, if dx < 0 { B::L } else { B::R }, gap.min(4));
            }
            mem.queue.push_back((0, gap));
        }
        mem.hold_bits = if s >= 3 { B::D } else { 0 };
    }
    play_queue(mem)
}

// ---------------------------------------------------------------- Capsule

fn capsule_sim(cs: &CapsuleState, x: i32, o: u8, p: u8, q: u8) -> Option<(i64, u32)> {
    let w = capsule::W;
    let h = capsule::H;
    let pill = capsule::Pill { x, y: capsule::VIS0, o, p, q };
    let fits = |pl: &capsule::Pill| {
        pl.cells().iter().all(|&(cx, cy, _, _)| cx >= 0 && cx < w && cy >= 0 && cy < h && cs.kind[(cy * w + cx) as usize] == 0)
    };
    // reachability along the top rows
    let (lo, hi) = (x.min(3), (x + 1).max(4));
    for col in lo..=hi.min(w - 1) {
        if cs.kind[(capsule::VIS0 * w + col) as usize] != 0 || cs.kind[((capsule::VIS0 + 1) * w + col) as usize] != 0 {
            return None;
        }
    }
    let mut pl = pill;
    if o % 2 == 1 {
        pl.y += 1;
    }
    if !fits(&pl) {
        return None;
    }
    loop {
        let mut n = pl;
        n.y += 1;
        if fits(&n) {
            pl = n;
        } else {
            break;
        }
    }
    let mut color = cs.color.clone();
    let mut kind = cs.kind.clone();
    for (cx, cy, c, k) in pl.cells() {
        color[(cy * w + cx) as usize] = c;
        kind[(cy * w + cx) as usize] = k;
    }
    let mut viruses_hit = 0u32;
    let mut cleared = 0u32;
    // one resolve step is enough to rank placements
    let mut mark = vec![false; (w * h) as usize];
    for y in 0..h {
        for x0 in 0..w {
            let c = color[(y * w + x0) as usize];
            if kind[(y * w + x0) as usize] == 0 {
                continue;
            }
            let mut e = x0;
            while e < w && kind[(y * w + e) as usize] != 0 && color[(y * w + e) as usize] == c {
                e += 1;
            }
            if e - x0 >= 4 {
                for i in x0..e {
                    mark[(y * w + i) as usize] = true;
                }
            }
            let mut f = y;
            while f < h && kind[(f * w + x0) as usize] != 0 && color[(f * w + x0) as usize] == c {
                f += 1;
            }
            if f - y >= 4 {
                for i in y..f {
                    mark[(i * w + x0) as usize] = true;
                }
            }
        }
    }
    for (i, &mk) in mark.iter().enumerate() {
        if mk {
            cleared += 1;
            if kind[i] == capsule::K_VIRUS {
                viruses_hit += 1;
            }
            kind[i] = 0;
        }
    }
    let mut score = viruses_hit as i64 * 1000 + cleared as i64 * 40;
    // adjacency of the placed halves to same color below/side
    for (cx, cy, c, _) in pl.cells() {
        for (dx, dy) in [(0, 1), (1, 0), (-1, 0)] {
            let (nx, ny) = (cx + dx, cy + dy);
            if nx >= 0 && nx < w && ny < h && kind[(ny * w + nx) as usize] != 0 {
                let nc = color[(ny * w + nx) as usize];
                if nc == c {
                    score += if kind[(ny * w + nx) as usize] == capsule::K_VIRUS { 90 } else { 45 };
                } else if dy == 1 {
                    score -= if kind[(ny * w + nx) as usize] == capsule::K_VIRUS { 110 } else { 40 };
                }
            }
        }
        score -= ((h - cy) as i64).pow(2) / 2;
        if (cx == 3 || cx == 4) && cy < capsule::VIS0 + 4 {
            score -= 400;
        }
    }
    Some((score, viruses_hit))
}

fn capsule_bot(c: &CapsuleState, p: Personality, s: u8, mem: &mut BotMem, _mode: Mode) -> u16 {
    let Some(pill) = c.pill else {
        mem.queue.clear();
        mem.key = 0;
        mem.hold_bits = 0;
        return 0;
    };
    let key = c.pills as u64 + 1;
    if mem.key != key {
        mem.key = key;
        mem.queue.clear();
        mem.hold_bits = 0;
        let mut ranked: Vec<(i64, (i32, u8))> = Vec::new();
        for o in 0..4u8 {
            for x in 0..capsule::W {
                if let Some((sc, _)) = capsule_sim(c, x, o, pill.p, pill.q) {
                    ranked.push((sc, (x, o)));
                }
            }
        }
        ranked.sort_by(|a, b| b.0.cmp(&a.0));
        let gap = interval(s);
        if let Some((x, o)) = pick(mem, &ranked, p, s) {
            mem.queue.push_back((0, gap / 2 + 1));
            for _ in 0..o {
                push_tap(mem, B::CCW, gap.min(6));
            }
            // rotating to vertical keeps x; moving afterward
            let dx = x - pill.x;
            for _ in 0..dx.abs() {
                push_tap(mem, if dx < 0 { B::L } else { B::R }, gap.min(4));
            }
            mem.queue.push_back((0, gap));
        }
        mem.hold_bits = if s >= 3 { B::D } else { 0 };
    }
    play_queue(mem)
}

// ---------------------------------------------------------------- Panel

/// Static board for search: colors only, garbage = 7, empty = 0.
fn panel_static(ps: &PanelState) -> Option<Vec<u8>> {
    let mut g = vec![0u8; (panel::W * panel::H) as usize];
    for (i, c) in ps.grid.iter().enumerate() {
        if c.s != panel::St::Idle && c.c != 0 {
            return None; // busy; wait for it to settle
        }
        g[i] = c.c;
    }
    Some(g)
}

fn panel_resolve(g: &mut [u8]) -> (u32, u32, bool) {
    let (w, h) = (panel::W, panel::H);
    let mut chain = 0;
    let mut cleared = 0;
    let mut near_garbage = false;
    loop {
        // gravity
        for x in 0..w {
            let mut wy = h - 1;
            for y in (0..h).rev() {
                let v = g[(y * w + x) as usize];
                if v == panel::GARB {
                    wy = y - 1;
                    continue;
                }
                if v != 0 {
                    g[(y * w + x) as usize] = 0;
                    g[(wy * w + x) as usize] = v;
                    wy -= 1;
                }
            }
        }
        let mut mark = vec![false; g.len()];
        for y in panel::VIS0..h {
            for x in 0..w {
                let c = g[(y * w + x) as usize];
                if c == 0 || c == panel::GARB {
                    continue;
                }
                if x + 2 < w && g[(y * w + x + 1) as usize] == c && g[(y * w + x + 2) as usize] == c {
                    for i in 0..3 {
                        mark[(y * w + x + i) as usize] = true;
                    }
                }
                if y + 2 < h && g[((y + 1) * w + x) as usize] == c && g[((y + 2) * w + x) as usize] == c {
                    for i in 0..3 {
                        mark[((y + i) * w + x) as usize] = true;
                    }
                }
            }
        }
        let n = mark.iter().filter(|&&b| b).count() as u32;
        if n == 0 {
            break;
        }
        chain += 1;
        cleared += n;
        for i in 0..g.len() {
            if mark[i] {
                let (x, y) = (i as i32 % w, i as i32 / w);
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx >= 0 && nx < w && ny >= 0 && ny < h && g[(ny * w + nx) as usize] == panel::GARB {
                        near_garbage = true;
                    }
                }
                g[i] = 0;
            }
        }
    }
    (chain, cleared, near_garbage)
}

fn panel_height(g: &[u8]) -> i32 {
    for y in 0..panel::H {
        if (0..panel::W).any(|x| g[(y * panel::W + x) as usize] != 0) {
            return panel::H - y;
        }
    }
    0
}

fn panel_bot(ps: &PanelState, p: Personality, s: u8, mem: &mut BotMem) -> u16 {
    let gap = interval(s);
    if mem.cooldown > 0 {
        mem.cooldown -= 1;
        return mem.hold_bits;
    }
    // follow an existing target
    if mem.key != 0 {
        let risen = ps.rows_risen.saturating_sub(mem.rows_at_plan) as i32;
        let (tx, ty) = (mem.target.0, mem.target.1 - risen);
        if ty < panel::VIS0 {
            mem.key = 0;
            return 0;
        }
        mem.cooldown = gap.max(2) as u32 - 1;
        mem.hold_bits = 0;
        if ps.cx != tx {
            return if ps.cx > tx { B::L } else { B::R };
        }
        if ps.cy != ty {
            return if ps.cy > ty { B::U } else { B::D };
        }
        mem.key = 0;
        mem.cooldown = gap as u32 * 2;
        return B::SWAP;
    }
    let Some(g) = panel_static(ps) else {
        mem.cooldown = 3;
        mem.hold_bits = 0;
        return 0;
    };
    let height = panel_height(&g);
    let mut ranked: Vec<(i64, (i32, i32))> = Vec::new();
    for y in panel::VIS0..panel::H {
        for x in 0..panel::W - 1 {
            let a = g[(y * panel::W + x) as usize];
            let b = g[(y * panel::W + x + 1) as usize];
            if a == b || a == panel::GARB || b == panel::GARB {
                continue;
            }
            let mut t = g.clone();
            t.swap((y * panel::W + x) as usize, (y * panel::W + x + 1) as usize);
            let (chain, cleared, near) = panel_resolve(&mut t);
            let mut sc: i64 = 0;
            if cleared > 0 {
                sc += cleared as i64 * 30 + (chain as i64 - 1) * 500;
                if p == Personality::Punisher && near {
                    sc += 400;
                }
                if p == Personality::Cleaner {
                    sc += (panel::H - y) as i64 * 25;
                }
            } else if s >= 5 && p != Personality::RandomLegal && (p == Personality::Builder || p == Personality::Punisher) {
                // one swap of look-ahead: does this set up a chain?
                let mut best = 0;
                for y2 in panel::VIS0..panel::H {
                    for x2 in 0..panel::W - 1 {
                        let mut u = t.clone();
                        u.swap((y2 * panel::W + x2) as usize, (y2 * panel::W + x2 + 1) as usize);
                        let (c2, _, _) = panel_resolve(&mut u);
                        best = best.max(c2);
                    }
                }
                if best >= 2 {
                    sc += 120 * best as i64;
                } else {
                    continue;
                }
            } else if (a == 0) != (b == 0) {
                // flatten: slide a panel off a tall column into a lower neighbor
                let (from, to) = if a == 0 { (x + 1, x) } else { (x, x + 1) };
                let col_h = |c: i32| (0..panel::H).find(|&yy| g[(yy * panel::W + c) as usize] != 0).map(|yy| panel::H - yy).unwrap_or(0);
                let drop = col_h(from) - col_h(to);
                if drop >= 2 && height >= 4 {
                    sc += drop as i64 * 20 + if p == Personality::Cleaner { 40 } else { 0 };
                } else {
                    continue;
                }
            } else {
                continue;
            }
            // cursor travel cost
            let dist = (ps.cx - x).abs() + (ps.cy - y).abs();
            sc -= dist as i64 * 6;
            ranked.push((sc, (x, y)));
        }
    }
    ranked.sort_by(|a, b| b.0.cmp(&a.0));
    if let Some((x, y)) = pick(mem, &ranked, p, s) {
        mem.key = 1;
        mem.target = (x, y);
        mem.rows_at_plan = ps.rows_risen;
        mem.hold_bits = 0;
        return 0;
    }
    // nothing to do: raise if low, else wait
    let low = match p {
        Personality::Cleaner => 4,
        _ => 6,
    };
    if height < low && ps.stop == 0 {
        mem.hold_bits = B::RAISE;
        mem.cooldown = 8;
        return B::RAISE;
    }
    mem.hold_bits = 0;
    mem.cooldown = gap as u32 * 2;
    0
}

/// Compute bot bits for every bot seat (helper for clients and tools).
pub fn fill_bot_inputs(m: &mut Match, inputs: &mut [u16; 6]) {
    for i in 0..6 {
        if m.seats[i].cfg.kind == purtris_sim::SeatKind::Bot && m.seats[i].alive {
            let mut mem = std::mem::take(&mut m.bot_mem[i]);
            inputs[i] = bot_input(m, i, &mut mem);
            m.bot_mem[i] = mem;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use purtris_sim::{quick_header, Outcome, Profile, SeatKind};

    /// The beginner (strength 0) plays every style: it places pieces, never hard-drops in Quad,
    /// and loses to a strength-6 bot.
    #[test]
    fn beginner_plays_and_is_weak() {
        for p in [Profile::QuadPpt, Profile::ChainTsu, Profile::CapsuleVs, Profile::PanelPdp] {
            let h = quick_header(7, Mode::Versus, &[(SeatKind::Bot, p, Personality::Cleaner, 0), (SeatKind::Bot, p, Personality::Builder, 6)]);
            let mut m = Match::new(h).expect("match");
            let mut hard_drops = 0;
            let mut moved = 0;
            while m.outcome == Outcome::Running && m.tick < 60 * 60 * 5 {
                let mut ins = [0u16; 6];
                crate::fill_bot_inputs(&mut m, &mut ins);
                if ins[0] & B::HD != 0 {
                    hard_drops += 1;
                }
                if ins[0] != 0 {
                    moved += 1;
                }
                m.step(ins);
            }
            assert!(moved > 0, "{:?}: the beginner never pressed anything", p);
            if p == Profile::QuadPpt {
                assert_eq!(hard_drops, 0, "the beginner hard-dropped");
            }
            assert!(!m.is_winner(0) || m.outcome == Outcome::Running, "{:?}: the beginner beat strength 6", p);
            // and it sends little: the point of Easy is a rival who does not bury you
            assert!(m.seats[0].stats.sent <= 12, "{:?}: the beginner sent {}", p, m.seats[0].stats.sent);
        }
    }
}
