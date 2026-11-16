//! Match: six seats, the numbered tick, attack routing, targeting, KO credit, endings.

use crate::capsule::CapsuleState;
use crate::chain::{gauge_to_nuisance, ChainState};
use crate::common::{bits as B, Ev, Native, Out, Packet, Profile, Style};
use crate::panel::{gauge_rects, PanelState};
use crate::quad::{QuadCfg, QuadState};
use crate::rng::{Hasher64, Rng, SHARED_SCOPE};
use std::collections::VecDeque;

pub const SEATS: usize = 6;
/// KO credit window (ticks).
pub const KO_WINDOW: u32 = 600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeatKind {
    Closed,
    Human,
    Bot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Personality {
    Builder,
    Aggressor,
    Digger,
    RandomLegal,
    Cleaner,
    Punisher,
    Racer,
}
impl Personality {
    pub const ALL: [Personality; 7] = [
        Personality::Builder,
        Personality::Aggressor,
        Personality::Digger,
        Personality::RandomLegal,
        Personality::Cleaner,
        Personality::Punisher,
        Personality::Racer,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Personality::Builder => "builder",
            Personality::Aggressor => "aggressor",
            Personality::Digger => "digger",
            Personality::RandomLegal => "random-legal",
            Personality::Cleaner => "cleaner",
            Personality::Punisher => "punisher",
            Personality::Racer => "racer",
        }
    }
    pub fn code(self) -> u8 {
        Personality::ALL.iter().position(|&p| p == self).unwrap() as u8
    }
    pub fn from_code(c: u8) -> Personality {
        Personality::ALL.get(c as usize).copied().unwrap_or(Personality::Builder)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct SeatCfg {
    pub kind: SeatKind,
    pub profile: Profile,
    pub team: u8,
    pub personality: Personality,
    /// 1..=8
    pub strength: u8,
    pub bot_seed: u32,
}
impl Default for SeatCfg {
    fn default() -> Self {
        SeatCfg { kind: SeatKind::Closed, profile: Profile::QuadPpt, team: 0, personality: Personality::Builder, strength: 4, bot_seed: 0 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Versus,
    CapsuleRace,
    QuadSprint,
    QuadTwenty,
    QuadMarathon,
    ChainEndless,
    CapsuleLevelRun,
    PanelEndless,
    PanelPuzzle(u8),
    Training,
}
impl Mode {
    pub fn code(self) -> (u8, u8) {
        match self {
            Mode::Versus => (0, 0),
            Mode::CapsuleRace => (1, 0),
            Mode::QuadSprint => (2, 0),
            Mode::QuadTwenty => (3, 0),
            Mode::QuadMarathon => (4, 0),
            Mode::ChainEndless => (5, 0),
            Mode::CapsuleLevelRun => (6, 0),
            Mode::PanelEndless => (7, 0),
            Mode::PanelPuzzle(i) => (8, i),
            Mode::Training => (9, 0),
        }
    }
    pub fn from_code(a: u8, b: u8) -> Mode {
        match a {
            1 => Mode::CapsuleRace,
            2 => Mode::QuadSprint,
            3 => Mode::QuadTwenty,
            4 => Mode::QuadMarathon,
            5 => Mode::ChainEndless,
            6 => Mode::CapsuleLevelRun,
            7 => Mode::PanelEndless,
            8 => Mode::PanelPuzzle(b),
            9 => Mode::Training,
            _ => Mode::Versus,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Mode::Versus => "Versus",
            Mode::CapsuleRace => "Capsule Race",
            Mode::QuadSprint => "Quad Sprint (40 lines)",
            Mode::QuadTwenty => "Quad Twenty (20 lines)",
            Mode::QuadMarathon => "Quad Marathon",
            Mode::ChainEndless => "Chain Endless",
            Mode::CapsuleLevelRun => "Capsule Level Run",
            Mode::PanelEndless => "Panel Endless",
            Mode::PanelPuzzle(_) => "Panel Puzzle",
            Mode::Training => "Training",
        }
    }
    pub fn is_solo(self) -> bool {
        !matches!(self, Mode::Versus | Mode::CapsuleRace)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TargetRule {
    Direct,
    Split,
    Leader,
    Team,
}
impl TargetRule {
    pub const ALL: [TargetRule; 4] = [TargetRule::Direct, TargetRule::Split, TargetRule::Leader, TargetRule::Team];
    pub fn name(self) -> &'static str {
        match self {
            TargetRule::Direct => "Direct",
            TargetRule::Split => "Split",
            TargetRule::Leader => "Leader",
            TargetRule::Team => "Team",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Opts {
    pub quad_level: u32,
    pub chain_colors: u8,
    pub capsule_level: u32,
    pub capsule_speed: u8,
    pub nes_seeds: bool,
    pub panel_level: u32,
    pub panel_colors: u8,
    pub virus_clear_wins: bool,
    pub force_cross: bool,
    /// true: every seat draws the same deal (SHARED_SCOPE). false: private deals (PER_SEAT).
    pub shared_deal: bool,
    pub team_follow_split: bool,
}
impl Default for Opts {
    fn default() -> Self {
        Opts {
            quad_level: 1,
            chain_colors: 4,
            capsule_level: 5,
            capsule_speed: 1,
            nes_seeds: false,
            panel_level: 3,
            panel_colors: 5,
            virus_clear_wins: false,
            force_cross: false,
            shared_deal: true,
            team_follow_split: true,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MatchHeader {
    pub seed: u64,
    pub mode: Mode,
    pub target: TargetRule,
    pub seats: [SeatCfg; SEATS],
    pub opts: Opts,
}

/// What a ruleset sees at creation.
pub struct SeatCtx<'a> {
    pub seed: u64,
    pub seat: u8,
    pub shared: bool,
    pub cross: bool,
    pub opts: &'a Opts,
}
impl SeatCtx<'_> {
    pub fn mix(&self) -> u16 {
        if self.shared {
            SHARED_SCOPE
        } else {
            self.seat as u16
        }
    }
}

#[derive(Clone, Debug)]
pub enum Board {
    Quad(QuadState),
    Chain(ChainState),
    Capsule(CapsuleState),
    Panel(PanelState),
}
impl Board {
    pub fn new(profile: Profile, ctx: &SeatCtx, mode: Mode) -> Board {
        match profile {
            Profile::QuadPpt => Board::Quad(QuadState::new(
                ctx,
                QuadCfg { hold: true, allow180: false, versus_gravity: mode == Mode::Versus, level_up: false, preview: 5 },
            )),
            Profile::QuadGuideline => Board::Quad(QuadState::new(
                ctx,
                QuadCfg { hold: true, allow180: true, versus_gravity: false, level_up: mode == Mode::QuadMarathon, preview: 5 },
            )),
            Profile::ChainTsu => Board::Chain(ChainState::new(ctx)),
            Profile::CapsuleVs | Profile::CapsuleNes => {
                Board::Capsule(CapsuleState::new(ctx, mode == Mode::CapsuleRace, mode == Mode::CapsuleLevelRun))
            }
            Profile::PanelPdp => {
                let pz = if let Mode::PanelPuzzle(i) = mode { Some(i as usize) } else { None };
                Board::Panel(PanelState::new(ctx, pz))
            }
        }
    }
    pub fn style(&self) -> Style {
        match self {
            Board::Quad(_) => Style::Quad,
            Board::Chain(_) => Style::Chain,
            Board::Capsule(_) => Style::Capsule,
            Board::Panel(_) => Style::Panel,
        }
    }
    pub fn step(&mut self, bits: u16, prev: u16, out: &mut Out) {
        match self {
            Board::Quad(s) => s.step(bits, prev, out),
            Board::Chain(s) => s.step(bits, prev, out),
            Board::Capsule(s) => s.step(bits, prev, out),
            Board::Panel(s) => s.step(bits, prev, out),
        }
    }
    pub fn dead(&self) -> bool {
        match self {
            Board::Quad(s) => s.dead,
            Board::Chain(s) => s.dead,
            Board::Capsule(s) => s.dead,
            Board::Panel(s) => s.dead,
        }
    }
    pub fn danger(&self) -> bool {
        match self {
            Board::Quad(s) => s.danger(),
            Board::Chain(s) => s.danger(),
            Board::Capsule(s) => s.danger(),
            Board::Panel(s) => s.danger(),
        }
    }
    pub fn apply_incoming(&mut self, n: &Native) {
        match (self, n) {
            (Board::Quad(s), _) => s.apply_incoming(n.units()),
            (Board::Chain(s), _) => s.apply_incoming(n.units()),
            (Board::Capsule(s), _) => s.apply_incoming(n.units()),
            (Board::Panel(s), Native::Rects(r)) => s.apply_incoming(r),
            (Board::Panel(s), other) => s.apply_incoming(&gauge_rects(other.units())),
        }
    }
    pub fn pending_units(&self) -> u32 {
        match self {
            Board::Quad(s) => s.pending_units(),
            Board::Chain(s) => s.pending_units(),
            Board::Capsule(s) => s.pending_units(),
            Board::Panel(s) => s.pending_units(),
        }
    }
    pub fn cancel_pending(&mut self, units: u32) -> u32 {
        match self {
            Board::Quad(s) => s.cancel_pending(units),
            Board::Chain(s) => s.cancel_pending(units),
            Board::Capsule(s) => s.cancel_pending(units),
            Board::Panel(_) => 0,
        }
    }
    pub fn score(&self) -> u64 {
        match self {
            Board::Quad(s) => s.score,
            Board::Chain(s) => s.score,
            Board::Capsule(s) => s.score,
            Board::Panel(s) => s.score,
        }
    }
    pub fn hash(&self, h: &mut Hasher64) {
        match self {
            Board::Quad(s) => s.hash(h),
            Board::Chain(s) => s.hash(h),
            Board::Capsule(s) => s.hash(h),
            Board::Panel(s) => s.hash(h),
        }
    }
}

/// Convert a gauge into a defender's native units (cross path).
pub fn from_gauge(style: Style, g: u32) -> Native {
    match style {
        Style::Quad => Native::Lines(g),
        Style::Chain => Native::Nuisance(gauge_to_nuisance(g)),
        Style::Capsule => Native::Halves(2 * g),
        Style::Panel => Native::Rects(gauge_rects(g)),
    }
}

#[derive(Clone, Debug)]
pub struct Travel {
    pub src: u8,
    pub dst: u8,
    pub timer: u32,
    pub native: Native,
    pub tick: u32,
}

/// Deterministic bot memory lives in the match so rewind and replays include it.
#[derive(Clone, Debug, Default)]
pub struct BotMem {
    pub rng: Rng,
    pub queue: VecDeque<(u16, u16)>,
    pub key: u64,
    pub cooldown: u32,
    pub target: (i32, i32),
    pub rows_at_plan: u32,
    pub hold_bits: u16,
    pub idle: u32,
}

#[derive(Clone, Debug, Default)]
pub struct SeatStats {
    pub sent: u32,
    pub received: u32,
    pub max_chain: u32,
    pub clears: u32,
    pub attacks: u32,
}

#[derive(Clone, Debug)]
pub struct Seat {
    pub cfg: SeatCfg,
    pub board: Option<Board>,
    pub alive: bool,
    pub kos: u32,
    pub death_tick: u32,
    pub place: u8,
    pub target: u8,
    pub prev: u16,
    pub stats: SeatStats,
    pub ko_by: Option<u8>,
    /// Tick each source's garbage last landed on this seat (KO credit).
    pub last_hit: [u32; SEATS],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Running,
    /// winning team key (seat index + 100 for no team, or the team number)
    Winner(u32),
    Draw,
    SoloDone { success: bool },
}

#[derive(Clone, Debug)]
pub struct Match {
    pub header: MatchHeader,
    pub tick: u32,
    pub seats: Vec<Seat>,
    pub travel: Vec<Travel>,
    pub cross: bool,
    pub events: Vec<(u8, Ev)>,
    pub outcome: Outcome,
    pub bot_mem: Vec<BotMem>,
    pub finish_tick: u32,
    pub alive_order: Vec<u8>,
}

pub fn team_key(seat: usize, team: u8) -> u32 {
    if team == 0 {
        100 + seat as u32
    } else {
        team as u32
    }
}

#[derive(Debug)]
pub enum CreateError {
    NoSeats,
    BadMode(&'static str),
}

impl Match {
    pub fn new(header: MatchHeader) -> Result<Match, CreateError> {
        let active: Vec<usize> = (0..SEATS).filter(|&i| header.seats[i].kind != SeatKind::Closed).collect();
        if active.is_empty() {
            return Err(CreateError::NoSeats);
        }
        if header.mode == Mode::CapsuleRace && active.iter().any(|&i| header.seats[i].profile.style() != Style::Capsule) {
            return Err(CreateError::BadMode("Capsule Race needs every seat on Capsule"));
        }
        // The attack path is fixed at match create.
        let first = header.seats[active[0]].profile;
        let cross = header.opts.force_cross || active.iter().any(|&i| header.seats[i].profile != first);
        let mut seats = Vec::with_capacity(SEATS);
        let mut bot_mem = Vec::with_capacity(SEATS);
        for i in 0..SEATS {
            let cfg = header.seats[i];
            let board = if cfg.kind == SeatKind::Closed {
                None
            } else {
                let ctx = SeatCtx { seed: header.seed, seat: i as u8, shared: header.opts.shared_deal, cross, opts: &header.opts };
                Some(Board::new(cfg.profile, &ctx, header.mode))
            };
            seats.push(Seat {
                cfg,
                alive: board.is_some(),
                board,
                kos: 0,
                death_tick: 0,
                place: 0,
                target: 0xFF,
                prev: 0,
                stats: SeatStats::default(),
                ko_by: None,
                last_hit: [0; SEATS],
            });
            bot_mem.push(BotMem {
                rng: Rng::stream(header.seed ^ cfg.bot_seed as u64, "bot.mistakes", i as u16),
                ..BotMem::default()
            });
        }
        let mut m = Match {
            header,
            tick: 0,
            seats,
            travel: Vec::new(),
            cross,
            events: Vec::new(),
            outcome: Outcome::Running,
            bot_mem,
            finish_tick: 0,
            alive_order: Vec::new(),
        };
        // Initial Direct target. The design's default (0xFF = lowest legal seat for everyone)
        // makes every seat in a free-for-all gang up on seat 0. Fixed: each seat starts on the
        // next legal seat after its own index, wrapping, which spreads the opening attacks.
        for i in 0..SEATS {
            if m.seats[i].alive {
                let legal = m.legal_targets(i);
                m.seats[i].target = legal.iter().copied().find(|&s| s as usize > i).or_else(|| legal.first().copied()).unwrap_or(0xFF);
            }
        }
        Ok(m)
    }

    pub fn active_count(&self) -> usize {
        self.seats.iter().filter(|s| s.board.is_some()).count()
    }

    pub fn legal_targets(&self, src: usize) -> Vec<u8> {
        let team = self.seats[src].cfg.team;
        let any_team = self.seats.iter().any(|s| s.board.is_some() && s.cfg.team != 0);
        (0..SEATS)
            .filter(|&j| {
                j != src && self.seats[j].alive && self.seats[j].board.is_some() && !(any_team && team != 0 && self.seats[j].cfg.team == team)
            })
            .map(|j| j as u8)
            .collect()
    }

    fn style_of(&self, i: usize) -> Style {
        self.seats[i].cfg.profile.style()
    }

    /// Units of a packet as the defender would receive it.
    fn convert(&self, packet: &Packet, def: usize) -> Native {
        if self.cross {
            from_gauge(self.style_of(def), packet.gauge)
        } else {
            packet.native.clone()
        }
    }

    /// One tick. `inputs` are the six latched bitfields (bots already computed).
    pub fn step(&mut self, inputs: [u16; SEATS]) {
        if self.outcome != Outcome::Running {
            return;
        }
        self.tick += 1;
        self.events.clear();
        // 1. decrement every traveling timer once
        for t in self.travel.iter_mut() {
            t.timer = t.timer.saturating_sub(1);
        }
        let mut applied_by = [0u8; SEATS];
        let mut emissions: Vec<(usize, Packet)> = Vec::new();
        let mut out = Out::default();
        for i in 0..SEATS {
            if self.seats[i].board.is_none() {
                continue;
            }
            // 2. apply attacks whose timer is zero, ascending source order
            let mut arrive: Vec<Travel> = Vec::new();
            let mut k = 0;
            while k < self.travel.len() {
                if self.travel[k].dst as usize == i && self.travel[k].timer == 0 {
                    arrive.push(self.travel.remove(k));
                } else {
                    k += 1;
                }
            }
            arrive.sort_by_key(|t| t.src);
            if self.seats[i].alive {
                for t in arrive {
                    let units = t.native.units();
                    self.seats[i].board.as_mut().unwrap().apply_incoming(&t.native);
                    self.seats[i].stats.received += units;
                    applied_by[i] |= 1 << t.src;
                    self.seats[i].last_hit[t.src as usize] = self.tick;
                }
            }
            if !self.seats[i].alive {
                continue;
            }
            // 3. sample input (reserved bit 15 must be 0)
            let bits = inputs[i] & !B::RESERVED;
            let prev = self.seats[i].prev;
            let press = bits & !prev;
            // target bits: Direct only (and Team following Direct); both set = neither
            let tn = press & B::TNEXT != 0;
            let tp = press & B::TPREV != 0;
            let direct = self.header.target == TargetRule::Direct || (self.header.target == TargetRule::Team && !self.header.opts.team_follow_split);
            if direct && (tn ^ tp) {
                let legal = self.legal_targets(i);
                if !legal.is_empty() {
                    let cur = self.seats[i].target;
                    let pos = legal.iter().position(|&s| s == cur);
                    let n = legal.len();
                    let ni = match (pos, tn) {
                        (Some(p), true) => (p + 1) % n,
                        (Some(p), false) => (p + n - 1) % n,
                        (None, _) => 0,
                    };
                    self.seats[i].target = legal[ni];
                }
            }
            // 4. step the ruleset
            out.events.clear();
            out.attacks.clear();
            self.seats[i].board.as_mut().unwrap().step(bits, prev, &mut out);
            self.seats[i].prev = bits;
            for e in out.events.drain(..) {
                match &e {
                    Ev::ChainStep { n } => {
                        let st = &mut self.seats[i].stats;
                        st.max_chain = st.max_chain.max(*n);
                    }
                    Ev::Clear { .. } => self.seats[i].stats.clears += 1,
                    _ => {}
                }
                self.events.push((i as u8, e));
            }
            // 5. emit
            for p in out.attacks.drain(..) {
                emissions.push((i, p));
            }
        }
        // deaths this tick
        let mut died: Vec<usize> = Vec::new();
        for i in 0..SEATS {
            if self.seats[i].alive && self.seats[i].board.as_ref().map(|b| b.dead()).unwrap_or(false) {
                died.push(i);
            }
        }
        for &d in &died {
            self.seats[d].alive = false;
            self.seats[d].death_tick = self.tick;
            self.alive_order.push(d as u8);
        }
        // 6. cancel only what is still traveling (and the attacker's own tray, minus this tick's arrivals)
        let mut routed: Vec<(usize, Packet)> = Vec::new();
        for (i, mut p) in emissions {
            if self.style_of(i) != Style::Panel {
                let own = if self.cross { from_gauge(self.style_of(i), p.gauge).units() } else { p.native.units() };
                let mut left = own;
                // oldest traveling first
                let mut order: Vec<usize> = (0..self.travel.len()).filter(|&k| self.travel[k].dst as usize == i).collect();
                order.sort_by_key(|&k| (self.travel[k].tick, self.travel[k].src));
                for k in order {
                    if left == 0 {
                        break;
                    }
                    let u = self.travel[k].native.units();
                    let c = left.min(u);
                    self.travel[k].native = self.travel[k].native.with_units(u - c);
                    left -= c;
                }
                self.travel.retain(|t| !t.native.is_zero());
                if left > 0 && self.seats[i].alive {
                    let c = self.seats[i].board.as_mut().unwrap().cancel_pending(left);
                    left -= c;
                }
                if left < own {
                    if self.cross {
                        p.gauge = if own == 0 { 0 } else { p.gauge * left / own };
                    } else {
                        p.native = p.native.with_units(left);
                    }
                }
                if (self.cross && p.gauge == 0) || (!self.cross && p.native.is_zero()) {
                    continue;
                }
            }
            routed.push((i, p));
        }
        // 7. stamp the travel delay, route by target rule; seat index is the tie-break
        let delay = crate::CROSS_DELAY;
        for (i, p) in routed {
            let legal = self.legal_targets(i);
            if legal.is_empty() {
                continue;
            }
            let units = if self.cross { p.gauge } else { p.native.units() };
            self.seats[i].stats.sent += units;
            self.seats[i].stats.attacks += 1;
            self.events.push((i as u8, Ev::AttackSent { units }));
            let targets: Vec<(u8, Packet)> = match self.header.target {
                TargetRule::Direct => vec![(self.direct_target(i, &legal), p)],
                TargetRule::Leader => vec![(self.leader_target(i, &legal), p)],
                TargetRule::Split => self.split(&legal, p),
                TargetRule::Team => {
                    if self.header.opts.team_follow_split {
                        self.split(&legal, p)
                    } else {
                        vec![(self.direct_target(i, &legal), p)]
                    }
                }
            };
            for (dst, pk) in targets {
                let native = self.convert(&pk, dst as usize);
                if native.is_zero() {
                    continue;
                }
                self.travel.push(Travel { src: i as u8, dst, timer: delay, native, tick: self.tick });
            }
        }
        // KO credit. The design credited only garbage applied on the death tick, but Quad,
        // Chain and Capsule hold garbage in a tray and die on a later lock, so that rule
        // never fired. Fixed: the living sender whose garbage landed most recently within
        // KO_WINDOW ticks gets the KO (ties: lowest seat). A mutual death still draws.
        for &d in &died {
            let mut killer: Option<usize> = None;
            let mut best = 0u32;
            for s in 0..SEATS {
                let hit = self.seats[d].last_hit[s];
                let recent = applied_by[d] & (1 << s) != 0 || (hit > 0 && self.tick - hit <= KO_WINDOW);
                if recent && self.seats[s].alive && hit >= best && (killer.is_none() || hit > best) {
                    best = hit;
                    killer = Some(s);
                }
            }
            if let Some(k) = killer {
                self.seats[k].kos += 1;
                self.seats[d].ko_by = Some(k as u8);
            }
            self.events.push((d as u8, Ev::Ko { victim: d as u8 }));
        }
        self.check_end();
    }

    fn direct_target(&mut self, i: usize, legal: &[u8]) -> u8 {
        let cur = self.seats[i].target;
        if legal.contains(&cur) {
            return cur;
        }
        let next = legal.iter().copied().find(|&s| s > cur).unwrap_or(legal[0]);
        self.seats[i].target = next;
        next
    }
    fn leader_target(&self, i: usize, legal: &[u8]) -> u8 {
        let mut order: Vec<usize> = (0..SEATS).filter(|&s| self.seats[s].alive).collect();
        order.sort_by_key(|&s| (std::cmp::Reverse(self.seats[s].kos), s));
        for s in order {
            if s != i && legal.contains(&(s as u8)) {
                return s as u8;
            }
        }
        legal[0]
    }
    fn split(&self, legal: &[u8], p: Packet) -> Vec<(u8, Packet)> {
        let n = legal.len() as u32;
        let mut v = Vec::new();
        if self.cross {
            let each = p.gauge / n;
            let rem = p.gauge % n;
            for (k, &s) in legal.iter().enumerate() {
                let g = each + if (k as u32) < rem { 1 } else { 0 };
                if g > 0 {
                    v.push((s, Packet { native: p.native.clone(), gauge: g }));
                }
            }
        } else if let Native::Rects(r) = &p.native {
            for (k, rect) in r.iter().enumerate() {
                v.push((legal[k % legal.len()], Packet { native: Native::Rects(vec![*rect]), gauge: p.gauge }));
            }
        } else {
            let u = p.native.units();
            let each = u / n;
            let rem = u % n;
            for (k, &s) in legal.iter().enumerate() {
                let x = each + if (k as u32) < rem { 1 } else { 0 };
                if x > 0 {
                    v.push((s, Packet { native: p.native.with_units(x), gauge: p.gauge }));
                }
            }
        }
        v
    }

    fn check_end(&mut self) {
        let mode = self.header.mode;
        let active: Vec<usize> = (0..SEATS).filter(|&i| self.seats[i].board.is_some()).collect();
        let alive: Vec<usize> = active.iter().copied().filter(|&i| self.seats[i].alive).collect();
        let finish = |m: &mut Match, o: Outcome| {
            m.outcome = o;
            m.finish_tick = m.tick;
        };
        // Solo modes
        if mode.is_solo() {
            let s = active[0];
            let b = self.seats[s].board.as_ref().unwrap();
            let done = match (mode, b) {
                (Mode::QuadSprint, Board::Quad(q)) if q.lines >= 40 => Some(true),
                (Mode::QuadTwenty, Board::Quad(q)) if q.lines >= 20 => Some(true),
                (Mode::PanelPuzzle(_), Board::Panel(p)) => p.puzzle_state(),
                _ => None,
            };
            if let Some(ok) = done {
                finish(self, Outcome::SoloDone { success: ok });
            } else if alive.is_empty() {
                let ok = matches!(mode, Mode::QuadMarathon | Mode::ChainEndless | Mode::CapsuleLevelRun | Mode::PanelEndless | Mode::Training);
                finish(self, Outcome::SoloDone { success: ok });
            }
            return;
        }
        // Capsule race / virus clear
        if mode == Mode::CapsuleRace || self.header.opts.virus_clear_wins {
            for &i in &alive {
                if let Some(Board::Capsule(c)) = &self.seats[i].board {
                    if c.cleared_all {
                        let key = team_key(i, self.seats[i].cfg.team);
                        finish(self, Outcome::Winner(key));
                        return;
                    }
                }
            }
        }
        if active.len() == 1 {
            if alive.is_empty() {
                finish(self, Outcome::SoloDone { success: false });
            }
            return;
        }
        let mut teams: Vec<u32> = alive.iter().map(|&i| team_key(i, self.seats[i].cfg.team)).collect();
        teams.sort();
        teams.dedup();
        if teams.is_empty() {
            finish(self, Outcome::Draw);
        } else if teams.len() == 1 {
            finish(self, Outcome::Winner(teams[0]));
        }
    }

    pub fn checksum(&self) -> u64 {
        let mut h = Hasher64::default();
        h.u32(self.tick);
        for s in &self.seats {
            h.bytes(&s.prev.to_le_bytes());
            h.byte(s.alive as u8);
            h.byte(s.target);
            if let Some(b) = &s.board {
                b.hash(&mut h);
            }
        }
        for t in &self.travel {
            h.byte(t.src);
            h.byte(t.dst);
            h.u32(t.timer);
            h.u32(t.native.units());
        }
        h.0
    }

    /// Which seats won (for results screens).
    pub fn is_winner(&self, seat: usize) -> bool {
        match self.outcome {
            Outcome::Winner(k) => team_key(seat, self.seats[seat].cfg.team) == k,
            Outcome::SoloDone { success } => success,
            _ => false,
        }
    }

    /// Incoming units aimed at a seat, in its own units: (traveling, in tray).
    pub fn incoming(&self, seat: usize) -> (u32, u32) {
        let t = self.travel.iter().filter(|t| t.dst as usize == seat).map(|t| t.native.units()).sum();
        let p = self.seats[seat].board.as_ref().map(|b| b.pending_units()).unwrap_or(0);
        (t, p)
    }
}
