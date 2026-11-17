//! A running match on screen: fixed 60 Hz sim clock, latched inputs, bots, replay recording,
//! training tools (pause, frame advance, rewind, save states, garbage inject), sound cues.

use crate::audio::{Sfx, SharedSynth};
use crate::content::{Obj, LESSONS};
use crate::input::{interpret_quad, DasState, Device, Input};
use crate::save::Settings;
use purtris_bot::fill_bot_inputs;
use purtris_sim::replay::Replay;
use purtris_sim::*;
use std::collections::VecDeque;
use std::time::Instant;

#[derive(Clone)]
#[allow(dead_code)] // the profiles are recorded so "play again" can rebuild the same setup
pub enum Origin {
    Versus(Box<crate::app::Room>),
    Solo(Mode, Profile),
    Story { world: usize, node: usize, diff: usize, chosen: Profile },
    Lesson(usize),
    Demo(usize),
    Sandbox(Profile, Option<(Profile, u8)>),
    Arcade { profile: Profile, stage: usize },
    Replay,
}

pub struct Game {
    pub m: Match,
    pub rep: Replay,
    pub devices: [Option<Device>; 6],
    das: [DasState; 6],
    acc: f64,
    last: Instant,
    pub paused: bool,
    pub pause_sel: usize,
    /// the pause menu's list (its focus animation)
    pub pause_ls: crate::ui::ListState,
    pub countdown: i32,
    pub training: bool,
    ring: VecDeque<Match>,
    slots: [Option<Match>; 2],
    pub names: [String; 6],
    /// the cast member (index into cast::CAST) shown for each seat
    pub chars: [usize; 6],
    pub humans: [bool; 6],
    pub origin: Origin,
    pub playback: Option<Replay>,
    pub speed: u32,
    pub last_inputs: [u16; 6],
    pub over_t: u32,
    pub log: Vec<(u32, u8, Ev)>,
    pub lock_inputs: Vec<(u32, u32)>,
    pub stop_at: Option<u32>,
    pub stop_on_clear: bool,
    pub forced_end: Option<bool>,
    pub toast: Option<(String, f32)>,
    pub step_once: bool,
    pub tas: bool,
    pub seen_level_clear: bool,
    danger_prev: [bool; 6],
    chain_t: f32,
    clear_t: f32,
}

impl Game {
    pub fn new(h: MatchHeader, devices: [Option<Device>; 6], names: [String; 6], origin: Origin, training: bool) -> Result<Game, String> {
        let m = Match::new(h).map_err(|e| format!("{:?}", e))?;
        let mut humans = [false; 6];
        for i in 0..6 {
            humans[i] = h.seats[i].kind == SeatKind::Human;
        }
        let mut rep = Replay::new(h);
        rep.names = names.to_vec();
        Ok(Game {
            m,
            rep,
            devices,
            das: [DasState::default(); 6],
            acc: 0.0,
            last: Instant::now(),
            paused: false,
            pause_sel: 0,
            pause_ls: Default::default(),
            countdown: if training { 0 } else { 150 },
            training,
            ring: VecDeque::new(),
            slots: [None, None],
            names,
            chars: [0; 6],
            humans,
            origin,
            playback: None,
            speed: 1,
            last_inputs: [0; 6],
            over_t: 0,
            log: Vec::new(),
            lock_inputs: Vec::new(),
            stop_at: None,
            stop_on_clear: false,
            forced_end: None,
            toast: None,
            step_once: false,
            tas: false,
            seen_level_clear: false,
            danger_prev: [false; 6],
            chain_t: 0.0,
            clear_t: 0.0,
        })
    }

    pub fn from_replay(rep: Replay) -> Result<Game, String> {
        let names: [String; 6] = std::array::from_fn(|i| rep.names.get(i).cloned().unwrap_or_else(|| format!("P{}", i + 1)));
        let mut g = Game::new(rep.header, [None; 6], names, Origin::Replay, true)?;
        g.humans = [false; 6];
        g.playback = Some(rep);
        g.countdown = 0;
        Ok(g)
    }

    pub fn finished(&self) -> bool {
        self.m.outcome != Outcome::Running || self.forced_end.is_some()
    }

    /// Take over a replay at the current tick: seat 0 becomes yours; the rest keep going.
    pub fn take_over(&mut self, dev: Device) {
        if let Some(rep) = self.playback.take() {
            let mut nr = Replay::new(rep.header);
            nr.names = rep.names.clone();
            nr.inputs = rep.inputs[..(self.m.tick as usize).min(rep.inputs.len())].to_vec();
            self.rep = nr;
            let mut slot = 0;
            for i in 0..6 {
                if self.m.seats[i].alive {
                    slot = i;
                    break;
                }
            }
            self.m.seats[slot].cfg.kind = SeatKind::Human;
            self.rep.header.seats[slot].kind = SeatKind::Human;
            for i in 0..6 {
                if i != slot && self.m.seats[i].board.is_some() && self.m.seats[i].cfg.kind == SeatKind::Human {
                    self.m.seats[i].cfg.kind = SeatKind::Bot;
                    self.rep.header.seats[i].kind = SeatKind::Bot;
                }
            }
            self.humans = [false; 6];
            self.humans[slot] = true;
            self.devices[slot] = Some(dev);
            self.toast = Some(("TAKEOVER: the fork is a new replay".into(), 2.5));
        }
    }

    fn sample(&mut self, input: &Input, st: &Settings) -> [u16; 6] {
        let mut ins = [0u16; 6];
        if let Some(rep) = &self.playback {
            let k = self.m.tick as usize;
            // bots still think so their memory stays in sync for a takeover
            let mut dummy = [0u16; 6];
            fill_bot_inputs(&mut self.m, &mut dummy);
            if k < rep.inputs.len() {
                ins = rep.inputs[k];
            } else {
                self.forced_end = Some(true);
            }
            return ins;
        }
        for i in 0..6 {
            if !self.humans[i] || !self.m.seats[i].alive {
                continue;
            }
            let Some(dev) = self.devices[i] else { continue };
            let style = self.m.seats[i].cfg.profile.style();
            let raw = input.sample(dev, style);
            ins[i] = if style == Style::Quad { interpret_quad(raw, &mut self.das[i], &st.handling) } else { raw };
        }
        fill_bot_inputs(&mut self.m, &mut ins);
        ins
    }

    fn one_tick(&mut self, input: &Input, st: &Settings, synth: &SharedSynth, r: &mut crate::render::Renderer) {
        let ins = self.sample(input, st);
        if self.training && self.playback.is_none() {
            self.ring.push_back(self.m.clone());
            if self.ring.len() > 600 {
                self.ring.pop_front();
            }
        }
        self.m.step(ins);
        self.rep.record(&self.m, ins);
        self.last_inputs = ins;
        self.after_tick(st, synth, r);
    }

    fn after_tick(&mut self, st: &Settings, synth: &SharedSynth, r: &mut crate::render::Renderer) {
        r.on_events(&self.m, st);
        let mut sfx: Vec<Sfx> = Vec::new();
        let many = self.m.active_count() > 2;
        for (s, e) in &self.m.events {
            let si = *s as usize;
            let mine = self.humans[si] || self.playback.is_some() && si == 0;
            if self.humans[si] && self.log.len() < 20000 {
                self.log.push((self.m.tick, *s, e.clone()));
            }
            let loud = mine || !many;
            match e {
                Ev::Move if mine => sfx.push(Sfx::Move),
                Ev::Rotate if mine => sfx.push(Sfx::Rotate),
                Ev::Swap if mine => sfx.push(Sfx::Swap),
                Ev::Hold if mine => sfx.push(Sfx::Hold),
                Ev::HardDrop if mine => sfx.push(Sfx::HardDrop),
                Ev::Lock { .. } if mine => {
                    sfx.push(Sfx::Lock);
                    if let Some(Board::Quad(q)) = &self.m.seats[si].board {
                        if let Some((t, n, _)) = q.last_lock {
                            self.lock_inputs.push((t, n));
                        }
                    }
                }
                Ev::Clear { power, .. } if loud => {
                    sfx.push(Sfx::Clear(*power));
                    self.clear_t = 4.0;
                }
                Ev::ChainStep { n } if loud => {
                    sfx.push(Sfx::Chain(*n));
                    if *n >= 2 {
                        self.chain_t = 6.0;
                    }
                }
                Ev::AllClear if loud => sfx.push(Sfx::AllClear),
                Ev::GarbageLand { .. } if mine => sfx.push(Sfx::Garbage),
                Ev::AttackSent { .. } if mine => sfx.push(Sfx::Attack),
                Ev::VirusClear if mine => sfx.push(Sfx::Virus),
                Ev::LevelClear => {
                    self.seen_level_clear = true;
                    if mine {
                        sfx.push(Sfx::AllClear);
                    }
                }
                Ev::Ko { .. } => sfx.push(Sfx::Ko),
                _ => {}
            }
        }
        let mut danger = false;
        for i in 0..6 {
            if !self.humans[i] {
                continue;
            }
            if let Some(b) = &self.m.seats[i].board {
                let d = b.danger() && self.m.seats[i].alive;
                if d && !self.danger_prev[i] {
                    sfx.push(Sfx::Danger);
                }
                self.danger_prev[i] = d;
                danger |= d;
            }
        }
        if let Ok(mut s) = synth.lock() {
            for x in sfx.into_iter().take(8) {
                s.play(x);
            }
            s.set_intensity(self.clear_t > 0.0, self.chain_t > 0.0, danger);
        }
        if let Some(t) = self.stop_at {
            if self.m.tick >= t && self.forced_end.is_none() {
                self.forced_end = Some(true);
            }
        }
        if self.stop_on_clear && self.seen_level_clear && self.forced_end.is_none() {
            self.forced_end = Some(true);
        }
    }

    /// Advance real time. Returns true while the match is still live.
    pub fn update(&mut self, input: &mut Input, st: &Settings, synth: &SharedSynth, r: &mut crate::render::Renderer, dt: f32) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        self.chain_t = (self.chain_t - dt).max(0.0);
        self.clear_t = (self.clear_t - dt).max(0.0);
        if let Some((_, t)) = &mut self.toast {
            *t -= dt;
            if *t <= 0.0 {
                self.toast = None;
            }
        }
        if self.finished() {
            // results wait a fixed time (counted in sim ticks), whatever the frame rate
            self.acc += elapsed;
            while self.acc >= PT_TICK {
                self.acc -= PT_TICK;
                self.over_t += 1;
            }
            r.fx.update(dt);
            input.clear_latch();
            return;
        }
        if self.paused {
            if self.step_once {
                self.step_once = false;
                self.one_tick(input, st, synth, r);
            }
            // keys pressed while paused (menu navigation) must not fire on resume
            input.clear_latch();
            self.acc = 0.0;
            return;
        }
        if self.countdown > 0 {
            // 3-2-1 runs on the sim clock (60 ticks a second), not on rendered frames
            self.acc += elapsed;
            if self.acc > 0.25 {
                self.acc = PT_TICK;
            }
            while self.acc >= PT_TICK && self.countdown > 0 {
                self.acc -= PT_TICK;
                if self.countdown % 50 == 0 {
                    if let Ok(mut s) = synth.lock() {
                        s.play(if self.countdown == 50 { Sfx::Go } else { Sfx::Count });
                    }
                }
                self.countdown -= 1;
            }
            input.clear_latch();
            r.fx.update(dt);
            return;
        }
        self.acc += elapsed * self.speed as f64;
        // A long hitch (window drag, sleep) does not fast-forward the match.
        if self.acc > 0.25 * self.speed as f64 {
            self.acc = PT_TICK;
        }
        let mut n = 0;
        while self.acc >= PT_TICK && n < 8 * self.speed {
            self.acc -= PT_TICK;
            self.one_tick(input, st, synth, r);
            input.clear_latch();
            n += 1;
            if self.finished() {
                break;
            }
        }
        r.fx.update(dt);
    }

    /// Headless: run n ticks immediately (screenshots, tests).
    pub fn fast_forward(&mut self, n: u32, input: &Input, st: &Settings, synth: &SharedSynth, r: &mut crate::render::Renderer) {
        self.countdown = 0;
        for _ in 0..n {
            if self.finished() {
                break;
            }
            self.one_tick(input, st, synth, r);
            r.fx.update(1.0 / 60.0);
        }
    }

    pub fn rewind(&mut self, ticks: usize) {
        for _ in 0..ticks {
            if let Some(m) = self.ring.pop_back() {
                self.m = m;
            }
        }
        self.rep.inputs.truncate(self.m.tick as usize);
        let t = self.m.tick;
        self.rep.checks.retain(|c| c.0 <= t);
        self.forced_end = None;
    }
    pub fn save_slot(&mut self, k: usize) {
        self.slots[k] = Some(self.m.clone());
        self.toast = Some((format!("SAVED STATE {}", k + 1), 1.2));
    }
    pub fn load_slot(&mut self, k: usize) {
        if let Some(m) = &self.slots[k] {
            self.m = m.clone();
            self.rep.inputs.truncate(self.m.tick as usize);
            self.toast = Some((format!("LOADED STATE {}", k + 1), 1.2));
            self.forced_end = None;
        }
    }
    /// Training only: the host injects a native attack on seat 0.
    pub fn inject(&mut self) {
        if let Some(b) = self.m.seats[0].board.as_mut() {
            let n = match b.style() {
                Style::Quad => Native::Lines(4),
                Style::Chain => Native::Nuisance(18),
                Style::Capsule => Native::Halves(4),
                Style::Panel => Native::Rects(vec![(6, 2)]),
            };
            b.apply_incoming(&n);
            self.toast = Some(("GARBAGE INJECTED".into(), 1.0));
            self.tas = true;
        }
    }

    /// Objective check after the match, from the sim log and final state.
    pub fn objective_met(&self, o: Obj, diff: usize) -> bool {
        let pick = |n: u32, h: u32| if diff >= 2 { h } else { n };
        let s = &self.m.seats[0];
        let won = self.won();
        match o {
            Obj::Win => won,
            Obj::TickPar(n, h) => won && self.m.tick <= pick(n, h),
            Obj::SendAtLeast(n, h) => s.stats.sent >= pick(n, h),
            Obj::ChainAtLeast(n, h) => s.stats.max_chain >= pick(n, h),
            Obj::ClearViruses => match &s.board {
                Some(Board::Capsule(c)) => c.cleared_all || c.levels_cleared > 0 || self.seen_level_clear,
                _ => false,
            },
            Obj::TeamTwoAlive => {
                won && self.m.seats.iter().filter(|x| x.board.is_some() && x.alive && x.cfg.team == s.cfg.team && s.cfg.team != 0).count() >= 2
            }
            Obj::SurviveTicks(n, h) => self.m.tick >= pick(n, h),
        }
    }

    pub fn won(&self) -> bool {
        if let Some(ok) = self.forced_end {
            if let Origin::Lesson(i) = self.origin {
                return match LESSONS[i].obj {
                    Obj::SurviveTicks(..) | Obj::ClearViruses => ok && self.m.seats[0].alive,
                    _ => self.m.is_winner(0),
                };
            }
            return ok && self.m.seats[0].alive;
        }
        self.m.is_winner(0)
    }
}

pub const PT_TICK: f64 = 1.0 / 60.0;
