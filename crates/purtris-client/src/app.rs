//! Screens: boot bench, title, main menu, story map, versus room, solo, training curriculum,
//! lessons, sandbox, calculators, seed browser, replays, results, settings, controls, card, help.

use crate::audio::{Sfx, SharedSynth};
use crate::content::*;
use crate::game::{Game, Origin};
use crate::gfx::{darken, lighten, Canvas};
use crate::input::{key_name, Action, Device, Input, Nav, ACTIONS};
use crate::render::{fmt_time, Renderer, View, SCENE_NAMES};
use crate::save::{Card, Progress, Settings};
use crate::theme::{look, Preset, ThemeId};
use crate::ui::{self, cycle, list, ListGeom, ListState, Row, LE};
use minifb::Key;
use purtris_sim::replay::Replay;
use purtris_sim::*;

// ------------------------------------------------------------------ room (local versus setup)

#[derive(Clone, Copy, PartialEq)]
pub enum SeatChoice {
    Closed,
    Human(Device),
    Bot,
}

#[derive(Clone)]
pub struct Room {
    pub seats: [(SeatChoice, Profile, Personality, u8, u8); 6],
    pub race: bool,
    pub target: TargetRule,
    pub opts: Opts,
    pub seed: Option<u64>,
    pub list: ListState,
    /// lobby focus: zone (0 rule pills, 1 seat cards, 2 start) and index in it
    pub focus: (u8, usize),
    /// the field being edited on the focused seat card
    pub edit: Option<usize>,
    /// the "More rules" dialog is open
    pub more: bool,
}
impl Room {
    pub fn new() -> Self {
        let mut seats = [(SeatChoice::Closed, Profile::QuadPpt, Personality::Builder, 4u8, 0u8); 6];
        seats[0] = (SeatChoice::Human(Device::KbFull), Profile::QuadPpt, Personality::Builder, 4, 0);
        seats[1] = (SeatChoice::Bot, Profile::ChainTsu, Personality::Builder, 4, 0);
        Room { seats, race: false, target: TargetRule::Direct, opts: Opts::default(), seed: None, list: ListState::default(), focus: (2, 0), edit: None, more: false }
    }
    pub fn header(&self, seed: u64) -> MatchHeader {
        let mut s = [SeatCfg::default(); 6];
        for i in 0..6 {
            let (c, p, per, st, team) = self.seats[i];
            s[i] = SeatCfg {
                kind: match c {
                    SeatChoice::Closed => SeatKind::Closed,
                    SeatChoice::Human(_) => SeatKind::Human,
                    SeatChoice::Bot => SeatKind::Bot,
                },
                profile: if self.race { Profile::CapsuleVs } else { p },
                team,
                personality: per,
                strength: st,
                bot_seed: 31 * (i as u32 + 1),
            };
        }
        let mut opts = self.opts;
        if self.race {
            opts.shared_deal = true;
        }
        let target = if s.iter().any(|x| x.kind != SeatKind::Closed && x.team != 0) && self.target == TargetRule::Direct { TargetRule::Team } else { self.target };
        MatchHeader { seed, mode: if self.race { Mode::CapsuleRace } else { Mode::Versus }, target, seats: s, opts }
    }
    fn path_text(&self) -> String {
        let act: Vec<Profile> = self.seats.iter().filter(|s| s.0 != SeatChoice::Closed).map(|s| if self.race { Profile::CapsuleVs } else { s.1 }).collect();
        if act.is_empty() {
            return "Open a seat to start.".into();
        }
        if !self.opts.force_cross && act.iter().all(|&p| p == act[0]) {
            "Everyone plays the same style, so attacks arrive just as that style sends them.".into()
        } else {
            "Mixed styles: every attack is converted into the target's own kind of garbage.".into()
        }
    }
}

const SEAT_DEVICES: [SeatChoice; 9] = [
    SeatChoice::Closed,
    SeatChoice::Human(Device::KbFull),
    SeatChoice::Human(Device::KbLeft),
    SeatChoice::Human(Device::KbRight),
    SeatChoice::Human(Device::Pad(0)),
    SeatChoice::Human(Device::Pad(1)),
    SeatChoice::Human(Device::Pad(2)),
    SeatChoice::Human(Device::Pad(3)),
    SeatChoice::Bot,
];
/// One-press room setups on the Versus screen: (label, icon).
const QUICK: [(&str, &str); 4] = [("You vs CPU", "sword"), ("1 vs 1", "game-controller"), ("You vs 3 CPUs", "target"), ("2 vs 2", "user")];

/// Two human devices for a shared screen: gamepads when there are enough, else split the keyboard.
fn two_devices(p1: Device, pads: usize) -> (Device, Device) {
    match pads {
        0 => (Device::KbLeft, Device::KbRight),
        1 => (Device::KbFull, Device::Pad(0)),
        _ => match p1 {
            Device::Pad(_) => (Device::Pad(0), Device::Pad(1)),
            _ => (Device::KbFull, Device::Pad(0)),
        },
    }
}

/// Set the room up as quick setup `k`. Every seat keeps the style it had; teams and seats reset.
fn apply_quick(room: &mut Room, k: usize, p1: Device, pads: usize) {
    for s in room.seats.iter_mut() {
        s.0 = SeatChoice::Closed;
        s.4 = 0;
    }
    room.edit = None;
    match k {
        0 => {
            room.seats[0].0 = SeatChoice::Human(p1);
            room.seats[1].0 = SeatChoice::Bot;
        }
        1 => {
            let (a, b) = two_devices(p1, pads);
            room.seats[0].0 = SeatChoice::Human(a);
            room.seats[1].0 = SeatChoice::Human(b);
        }
        2 => {
            room.seats[0].0 = SeatChoice::Human(p1);
            for i in 1..4 {
                room.seats[i].0 = SeatChoice::Bot;
            }
        }
        _ => {
            // you and a CPU partner against two CPUs
            room.seats[0].0 = SeatChoice::Human(p1);
            for i in 1..4 {
                room.seats[i].0 = SeatChoice::Bot;
            }
            room.seats[0].4 = 1;
            room.seats[1].4 = 1;
            room.seats[2].4 = 2;
            room.seats[3].4 = 2;
        }
    }
}

/// Which quick setup the room matches right now, if any.
fn quick_current(room: &Room) -> Option<usize> {
    let kinds: Vec<(bool, u8)> = room.seats.iter().filter(|s| s.0 != SeatChoice::Closed).map(|s| (matches!(s.0, SeatChoice::Human(_)), s.4)).collect();
    let humans = kinds.iter().filter(|k| k.0).count();
    let teams = kinds.iter().any(|k| k.1 != 0);
    match (kinds.len(), humans, teams) {
        (2, 1, false) => Some(0),
        (2, 2, false) => Some(1),
        (4, 1, false) => Some(2),
        (4, 1, true) => Some(3),
        _ => None,
    }
}

/// The fields a seat card can edit: 0 player, 1 style, 2 bot personality, 3 bot strength, 4 team.
fn seat_fields(room: &Room, i: usize) -> Vec<usize> {
    let c = room.seats[i].0;
    let mut v = vec![0];
    if c != SeatChoice::Closed {
        if !room.race {
            v.push(1);
        }
        if c == SeatChoice::Bot {
            v.push(3);
            v.push(2);
        }
        // the team only matters once the room plays in teams (More rules, or the 2 vs 2 setup)
        if room.seats.iter().any(|s| s.0 != SeatChoice::Closed && s.4 != 0) {
            v.push(4);
        }
    }
    v
}

fn seat_field_text(room: &Room, i: usize, f: usize) -> (&'static str, String) {
    let (c, p, per, st, team) = room.seats[i];
    match f {
        0 => (
            "Player",
            match c {
                SeatChoice::Closed => "Empty".into(),
                SeatChoice::Human(d) => device_short(d),
                SeatChoice::Bot => "CPU".into(),
            },
        ),
        1 => ("Style", format!("{} {}", ui::title_case(p.style().name()), profile_rules(p).split(' ').next().unwrap_or(""))),
        2 => ("Plays like", ui::title_case(per.name())),
        3 => ("Strength", format!("{} of 8", st)),
        _ => ("Team", if team == 0 { "None".into() } else { format!("Team {}", team) }),
    }
}

fn seat_change(room: &mut Room, i: usize, f: usize, d: i32) {
    let s = &mut room.seats[i];
    match f {
        0 => s.0 = cycle(&SEAT_DEVICES, s.0, d),
        1 => s.1 = cycle(&PLAY_PROFILES, s.1, d),
        2 => s.2 = cycle(&Personality::ALL, s.2, d),
        3 => s.3 = ((s.3 as i32 - 1 + d).rem_euclid(8) + 1) as u8,
        _ => s.4 = ((s.4 as i32 + d).rem_euclid(4)) as u8,
    }
}

fn device_short(d: Device) -> String {
    match d {
        Device::KbFull => "Keyboard".into(),
        Device::KbLeft => "Keyboard, left half".into(),
        Device::KbRight => "Keyboard, right half".into(),
        Device::Pad(n) => format!("Gamepad {}", n + 1),
    }
}

/// The rule set of a profile, in plain words.
fn profile_rules(p: Profile) -> &'static str {
    match p {
        Profile::QuadPpt => "Versus rules",
        Profile::QuadGuideline => "Guideline rules",
        Profile::ChainTsu => "Classic rules",
        Profile::CapsuleVs => "Versus rules",
        Profile::CapsuleNes => "Original rules",
        Profile::PanelPdp => "Classic rules",
    }
}

fn team_preset_name(room: &Room) -> String {
    let act: Vec<u8> = room.seats.iter().filter(|s| s.0 != SeatChoice::Closed).map(|s| s.4).collect();
    if act.iter().all(|&t| t == 0) {
        "Free for all".into()
    } else {
        let n = act.iter().filter(|&&t| t != 0).map(|&t| t).max().unwrap_or(1);
        format!("{} teams", n)
    }
}

/// Free for all, then two teams, then three (when there are enough seats).
fn next_team_preset(room: &mut Room) {
    let act: Vec<usize> = (0..6).filter(|&i| room.seats[i].0 != SeatChoice::Closed).collect();
    if act.is_empty() {
        return;
    }
    let teams = room.seats.iter().filter(|s| s.0 != SeatChoice::Closed).map(|s| s.4).max().unwrap_or(0);
    let next = match teams {
        0 => 2,
        2 if act.len() >= 6 => 3,
        _ => 0,
    };
    for (n, &i) in act.iter().enumerate() {
        room.seats[i].4 = if next == 0 { 0 } else { 1 + (n as u8 % next) };
    }
}

const PLAY_PROFILES: [Profile; 6] = [Profile::QuadPpt, Profile::QuadGuideline, Profile::ChainTsu, Profile::CapsuleVs, Profile::CapsuleNes, Profile::PanelPdp];
// ------------------------------------------------------------------ results and analyzer

#[allow(dead_code)] // `tick` is kept for a jump-to-moment link in the replay viewer
pub struct Finding {
    pub severity: &'static str,
    pub tick: u32,
    pub text: String,
    pub lesson: Option<&'static str>,
}

pub struct Results {
    pub game: Game,
    pub objectives: Vec<(String, bool)>,
    pub findings: Vec<Finding>,
    pub list: ListState,
    pub saved: Option<String>,
    pub new_record: Option<String>,
    /// XP earned and leaderboard places from this match
    pub gain: crate::ranks::Gain,
}

fn analyze(g: &Game) -> Vec<Finding> {
    let mut f = Vec::new();
    let s = &g.m.seats[0];
    let Some(b) = &s.board else { return f };
    if !s.alive {
        let (trav, tray) = g.m.incoming(0);
        let recv = s.stats.received;
        if recv > 0 {
            f.push(Finding {
                severity: "EXCHANGE",
                tick: s.death_tick,
                text: format!("Topped out at {} after receiving {} ({} still incoming). When the meter turns red, clear or offset before building higher.", fmt_time(s.death_tick), recv, trav + tray),
                lesson: Some("m1"),
            });
        } else {
            f.push(Finding { severity: "EXCHANGE", tick: s.death_tick, text: format!("Topped out at {} with no garbage received: the stack itself was the threat.", fmt_time(s.death_tick)), lesson: None });
        }
    }
    match b {
        Board::Quad(q) => {
            if q.pieces > 10 && q.holes_made * 8 > q.pieces {
                f.push(Finding { severity: "HABIT", tick: 0, text: format!("Covered {} holes in {} pieces. Keep the surface flat; never bury an empty cell.", q.holes_made, q.pieces), lesson: Some("q1") });
            }
            let extra: Vec<&(u32, u32)> = g.lock_inputs.iter().filter(|(_, n)| *n > 4).collect();
            if extra.len() >= 3 {
                f.push(Finding {
                    severity: "POLISH",
                    tick: extra[0].0,
                    text: format!("{} of {} pieces used more than 4 presses (finesse). Let DAS carry the piece to the wall.", extra.len(), g.lock_inputs.len()),
                    lesson: Some("q3"),
                });
            }
        }
        Board::Chain(_) => {
            let mut ones = 0;
            let mut last_n = 0;
            for (_, _, e) in &g.log {
                if let Ev::ChainStep { n } = e {
                    if *n == 1 && last_n == 1 {
                        ones += 1;
                    }
                    last_n = *n;
                }
            }
            if s.stats.max_chain < 3 {
                f.push(Finding { severity: "HABIT", tick: 0, text: format!("Longest chain was {}. Stack a group of three above another so one pop triggers the next.", s.stats.max_chain), lesson: Some("c2") });
            }
            if ones >= 4 {
                f.push(Finding { severity: "POLISH", tick: 0, text: format!("Fired {} single pops in a row. Pop small only to offset a loaded tray.", ones), lesson: Some("c4") });
            }
        }
        Board::Capsule(c) => {
            let clears = g.log.iter().filter(|(_, _, e)| matches!(e, Ev::Clear { .. })).count();
            let germs = g.log.iter().filter(|(_, _, e)| matches!(e, Ev::VirusClear)).count();
            if clears >= 4 && germs * 2 < clears {
                f.push(Finding { severity: "HABIT", tick: 0, text: format!("{} clears but only {} took a germ. Line up colors on the germs, not beside them.", clears, germs), lesson: Some("k1") });
            }
            if c.viruses > 0 && !s.alive {
                f.push(Finding { severity: "POLISH", tick: 0, text: format!("{} germs left. Take the low germs first so nothing gets buried.", c.viruses), lesson: Some("k4") });
            }
        }
        Board::Panel(p) => {
            if p.swaps > 5 && p.cursor_travel > p.swaps * 6 {
                f.push(Finding {
                    severity: "POLISH",
                    tick: 0,
                    text: format!("Cursor travelled {} cells for {} swaps. Work near the cursor and plan the next swap while a clear flashes.", p.cursor_travel, p.swaps),
                    lesson: Some("p1"),
                });
            }
            if p.max_chain < 2 && p.swaps > 10 {
                f.push(Finding { severity: "HABIT", tick: 0, text: "No chains this game. Put a panel above a gap so the next clear drops it into a match.".into(), lesson: Some("p3") });
            }
        }
    }
    f
}

// ------------------------------------------------------------------ screens

pub enum Screen {
    Boot { frames: u32, start: std::time::Instant },
    Title,
    Main(ListState),
    Story { world: usize, node: usize, diff: usize, chosen: usize, talk: bool },
    Versus(Box<Room>),
    /// back target: the Solo tile page
    Solo,
    Arcade(ListState, usize),
    /// back target: the Training tile page
    Training,
    Curriculum(ListState, usize),
    /// lesson, its action list, and which part Mira is explaining
    Lesson(usize, ListState, usize),
    Sandbox(ListState, usize, usize, u8),
    Calc(ListState, u32, u32, u32, u32),
    Seeds(ListState, u64, String),
    Replays(ListState, Vec<std::path::PathBuf>),
    Settings(ListState, usize),
    Controls(ListState, usize, Option<usize>),
    Card(ListState, Option<usize>),
    Help(usize),
    Play(Box<Game>),
    Results(Box<Results>),
    /// a story scene, then `then`
    Scene(Box<crate::scene_ui::SceneView>),
    /// character select before a match
    Select(Box<crate::story_ui::Select>),
    /// first-run setup with Mira
    Setup { page: usize, sel: usize },
    /// level and prestige (page 0) or a leaderboard (page 1..)
    Ranks { page: usize, confirm: bool },
    /// Mira's tour of the modes
    Tour { page: usize, t: f32 },
    /// naming the story's courier, then `then`
    NameHero { typed: String, then: Box<Screen> },
    Quit,
}

/// One part of the main menu: its tile, its color and the cast member who stands for it.
pub struct MenuCat {
    pub icon: &'static str,
    pub name: &'static str,
    pub color: u32,
    /// cast id and pose sprite (rendered by tools/blender/cast3d.py poses)
    pub mascot: (&'static str, &'static str),
}
pub const MENU_CATS: [MenuCat; 8] = [
    MenuCat { icon: "map-trifold", name: "Story", color: ui::S_STORY, mascot: ("kai", "p-run") },
    MenuCat { icon: "sword", name: "Versus", color: ui::S_VERSUS, mascot: ("luna", "p-rival") },
    MenuCat { icon: "timer", name: "Solo", color: ui::S_SOLO, mascot: ("sora", "p-thumb") },
    MenuCat { icon: "graduation-cap", name: "Training", color: ui::S_TRAINING, mascot: ("mira", "p-talk") },
    MenuCat { icon: "film-strip", name: "Replays", color: ui::S_REPLAYS, mascot: ("toma", "p-look") },
    MenuCat { icon: "user-circle", name: "Profile", color: ui::S_PROFILE, mascot: ("kai", "p-thumb") },
    MenuCat { icon: "trophy", name: "Ranks", color: ui::S_RANKS, mascot: ("jun", "p-arms") },
    MenuCat { icon: "gear-six", name: "Settings", color: ui::S_SETTINGS, mascot: ("aoi", "p-rival") },
];
/// The host on the top menu, presenting the tiles.
const HOST_POSE: (&str, &str) = ("mira", "p-present");

/// A few pieces of a style (0 Quad, 1 Chain, 2 Capsule, 3 Panel, 4 one of each) in a window.
#[allow(clippy::too_many_arguments)]
fn draw_pieces(cv: &mut Canvas, sprites: &mut crate::theme::Sprites, kind: usize, wx: i32, wy: i32, ww: i32, wh: i32, alpha: u32) {
    use crate::theme::{CellKind as K, SpriteKey, BLUE, CYAN, GREEN, PURPLE, RED, YELLOW};
    let pieces: &[(i32, i32, K, u32)] = match kind {
        0 => &[(0, 1, K::Gem, CYAN), (1, 1, K::Gem, CYAN), (2, 1, K::Gem, CYAN), (3, 1, K::Gem, CYAN), (1, 0, K::Gem, PURPLE), (2, 0, K::Gem, PURPLE), (3, 0, K::Gem, PURPLE), (2, -1, K::Gem, PURPLE)],
        1 => &[(0, 1, K::Blob, RED), (1, 1, K::Blob, RED), (0, 0, K::Blob, RED), (1, 0, K::Blob, GREEN), (2, 1, K::Blob, BLUE), (2, 0, K::Blob, GREEN), (3, 1, K::Blob, YELLOW)],
        2 => &[(0, 1, K::HalfL, YELLOW), (1, 1, K::HalfR, RED), (2, 1, K::Virus, BLUE), (3, 1, K::Virus, RED), (2, 0, K::HalfL, BLUE), (3, 0, K::HalfR, BLUE)],
        3 => &[(0, 1, K::Tile, RED), (1, 1, K::Tile, GREEN), (2, 1, K::Tile, RED), (3, 1, K::Tile, CYAN), (1, 0, K::Tile, PURPLE), (2, 0, K::Tile, RED)],
        _ => &[(0, 1, K::Gem, PURPLE), (1, 1, K::Blob, RED), (2, 1, K::Virus, BLUE), (3, 1, K::Tile, YELLOW), (1, 0, K::Gem, CYAN), (2, 0, K::Blob, GREEN)],
    };
    let cell = ((ww - 20) / 4).min((wh - 12) / 3).min(34);
    let (bx, by) = (wx + (ww - cell * 4) / 2, wy + wh - 6 - cell * 2);
    sprites.set_scale(cv.k);
    cv.set_clip(wx, wy, ww, wh);
    for &(cx, cy, kd, col) in pieces {
        let spr = sprites.get(SpriteKey { kind: kd, color: col, size: cell, face: true, pattern: false, flat: false, frame: 0 });
        cv.blit(spr, bx + cx * cell, by + cy * cell, alpha);
    }
    cv.reset_clip();
}

/// The little picture on an item tile: which pieces to show (None = no picture).
fn tile_pieces(cat: usize, i: usize) -> Option<usize> {
    // 0 Quad, 1 Chain, 2 Capsule, 3 Panel, 4 one of each
    match (cat, i) {
        (1, 0) => Some(4),
        (1, 1) => Some(0),
        (1, 2) => Some(2),
        (2, 0..=2) => Some(0),
        (2, 3) => Some(1),
        (2, 4) => Some(2),
        (2, 5) => Some(3),
        (2, _) => Some(4),
        (3, 0) => Some(4),
        (3, 1) => Some(0),
        (3, 2) => Some(3),
        (3, 3) => Some(1),
        (3, _) => Some(4),
        _ => None,
    }
}
const TILE_SKEW: i32 = 14;

/// Top menu: the three modes as big tiles, then five smaller ones.
fn top_tiles() -> [(i32, i32, i32, i32); 8] {
    let mut v = [(0, 0, 0, 0); 8];
    let (x0, x1) = (44, 978);
    let w1 = (x1 - x0 - 2 * 20) / 3;
    for (i, t) in v.iter_mut().take(3).enumerate() {
        *t = (x0 + i as i32 * (w1 + 20), 134, w1, 222);
    }
    let w2 = (x1 - x0 - 4 * 16) / 5;
    for i in 0..5 {
        v[3 + i] = (x0 + i as i32 * (w2 + 16), 378, w2, 156);
    }
    v
}

/// A section page: its items as a grid of tiles.
fn item_tiles(n: usize) -> Vec<(i32, i32, i32, i32)> {
    let n = n.max(1);
    let cols = match n {
        1..=3 => n,
        4 => 2,
        5 | 6 => 3,
        7 | 8 => 4,
        _ => 5,
    } as i32;
    let rows = (n as i32 + cols - 1) / cols;
    let (x0, x1, y0, y1) = (44, 978, 106, 540);
    let gap = 16;
    let w = (x1 - x0 - (cols - 1) * gap) / cols;
    let h = ((y1 - y0 - (rows - 1) * gap) / rows).min(200);
    let top = y0 + ((y1 - y0) - (rows * h + (rows - 1) * gap)) / 2;
    (0..n as i32).map(|i| (x0 + (i % cols) * (w + gap), top + (i / cols) * (h + gap), w, h)).collect()
}

/// Move from tile `cur` toward (dx, dy): the nearest tile within a 60-degree cone.
fn grid_step(tiles: &[(i32, i32, i32, i32)], cur: usize, dx: i32, dy: i32) -> usize {
    let mid = |t: &(i32, i32, i32, i32)| (t.0 + t.2 / 2, t.1 + t.3 / 2);
    let Some(c) = tiles.get(cur).map(mid) else { return cur };
    let mut best = (i32::MAX, cur);
    for (i, t) in tiles.iter().enumerate() {
        let p = mid(t);
        let (ex, ey) = (p.0 - c.0, p.1 - c.1);
        let along = ex * dx + ey * dy;
        let across = (ex * dy - ey * dx).abs();
        if i == cur || along <= 10 || across * 2 > along * 3 {
            continue;
        }
        let score = along + across * 2;
        if score < best.0 {
            best = (score, i);
        }
    }
    best.1
}

/// A menu tile: drop shadow, border (white when selected), the color, a sheen and halftone.
fn tile_base(cv: &mut Canvas, x: i32, y: i32, w: i32, h: i32, c: u32, on: bool) {
    let k = TILE_SKEW;
    cv.poly(&ui::slant(x + 6, y + 10, w, h, k), 0xFF00_0000, if on { 90 } else { 60 });
    if on {
        cv.glow((x + w / 2) as f32, (y + h / 2) as f32, (w.max(h) as f32) * 0.62, lighten(c, 60), 46);
    }
    ui::slab(cv, x, y, w, h, k, c, if on { 0xFFFF_FFFF } else { darken(c, 105) }, if on { 5 } else { 3 });
    cv.poly(&ui::slant_band(x, y, w, h, k, 0.0, 0.40), 0xFFFF_FFFF, if on { 40 } else { 28 });
    ui::halftone(cv, x, y, w, h, k, lighten(c, 70), if on { 150 } else { 120 }, 11, 3.8, 0.35);
}

/// A round white badge with an icon in the tile's color.
fn badge(cv: &mut Canvas, cx: i32, cy: i32, icon: &str, c: u32) {
    cv.circle(cx as f32 + 1.5, cy as f32 + 3.0, 25.0, darken(c, 120), 255);
    cv.circle(cx as f32, cy as f32, 25.0, 0xFFFF_FFFF, 255);
    crate::assets::icon(cv, icon, cx, cy, 30, c, 255);
}

/// How an item tile's label is set: "Kicker  -  Main" labels show the kicker small above.
struct TileLabel {
    kicker: String,
    lines: Vec<String>,
    sc: i32,
    lh: i32,
    /// height of the whole label block
    block: i32,
}

fn label_layout(w: i32, label: &str, sc: i32) -> TileLabel {
    let (kicker, main) = match label.split_once("  -  ") {
        Some((a, b)) => (a.trim(), b.trim()),
        None => ("", label.trim()),
    };
    let maxw = w - 34;
    let mut lines = wrap_display(main, maxw, sc);
    lines.truncate(2);
    let lh = (Canvas::px(sc) * 1.08) as i32;
    let block = lines.len() as i32 * lh + if kicker.is_empty() { 0 } else { 26 };
    TileLabel { kicker: kicker.to_string(), lines, sc, lh, block }
}

/// An item tile's label, set along the bottom edge.
fn item_label(cv: &mut Canvas, x: i32, y: i32, h: i32, lay: &TileLabel, c: u32) {
    use crate::font::Weight;
    let ink = darken(c, 130);
    let mut ty = y + h - 16 - lay.lines.len() as i32 * lay.lh;
    if !lay.kicker.is_empty() {
        cv.text_weight(x + 23, ty - 25, &lay.kicker, 2, ink, 160, Weight::Bold);
        cv.text_weight(x + 22, ty - 26, &lay.kicker, 2, 0xFFFF_FFFF, 240, Weight::Bold);
    }
    for ln in lay.lines.iter() {
        ui::outlined(cv, x + 20, ty, ln, lay.sc, 0xFFFF_FFFF, ink, if lay.sc >= 3 { 3 } else { 2 }, Weight::Display);
        ty += lay.lh;
    }
}

fn wrap_display(s: &str, maxw: i32, sc: i32) -> Vec<String> {
    use crate::font::Weight;
    let mut out: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in s.split_whitespace() {
        let trial = if line.is_empty() { word.to_string() } else { format!("{} {}", line, word) };
        if Canvas::text_wb(&trial, sc, Weight::Display) <= maxw || line.is_empty() {
            line = trial;
        } else {
            out.push(std::mem::take(&mut line));
            line.push_str(word);
        }
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}

pub struct App {
    pub st: Settings,
    pub prog: Progress,
    /// level, prestige and leaderboards
    pub ranks: crate::ranks::Ranks,
    pub card: Card,
    pub input: Input,
    pub synth: SharedSynth,
    pub audio_ok: bool,
    pub cv: Canvas,
    pub r: Renderer,
    pub screen: Screen,
    pub t: f32,
    pub host_pose: (u8, f32),
    pub toast: Option<(String, f32)>,
    pub maps: Vec<crate::assets::WorldMap>,
    pub bg_world: usize,
    pub xmb_cat: usize,
    pub xmb_item: [usize; 8],
    /// a section page is open (else the top tiles)
    pub menu_open: bool,
    /// time since the menu level changed, and each tile's selection lift
    pub menu_t: f32,
    pub menu_lift: [f32; 12],
    /// on the Settings screen, the keyboard focus is on the tab bar (else on the rows)
    pub settings_tabs: bool,
    /// world-map thumbnails for the story tiles, by (world, pixel size)
    thumbs: std::collections::HashMap<(usize, i32, i32), Canvas>,
    pub last_input_t: f32,
    /// this frame's length in seconds (animations ease with it)
    pub dt: f32,
    /// the primary monitor (size, refresh) for display settings
    pub monitor: crate::display::Monitor,
    /// the modes the monitor offers (resolution and refresh-rate choices)
    pub modes: Vec<crate::display::Mode>,
    /// measured frames per second (smoothed), for the optional counter
    pub fps: f32,
    /// scripted input for live tests (PC_SCRIPT)
    pub script: Option<crate::script::Script>,
    /// display settings to restore unless the player confirms, and when
    pub display_undo: Option<(crate::settings_ui::DisplaySnap, f32)>,
    /// characters picked on the select screen for the next match
    pub next_chars: Option<[usize; 6]>,
    /// seconds of fade-in from black left (after a story scene ends)
    pub fade_in: f32,
    boot_draw: f32,
    /// world maps scaled to the screen, sharp and blurred, keyed by (world, pixel width)
    scaled_maps: std::collections::HashMap<(usize, i32), (Canvas, std::sync::Arc<crate::gfx::Backdrop>)>,
    /// blurred and dimmed, keyed (world, width, dim)
    blurred_maps: std::collections::HashMap<(usize, i32, u32), (Canvas, std::sync::Arc<crate::gfx::Backdrop>)>,
    title_zoom: std::collections::HashMap<(usize, i32), (Canvas, std::sync::Arc<crate::gfx::Backdrop>)>,
    vignette: (i32, Vec<u8>),
}

fn style_profile_for(i: usize) -> Profile {
    [Profile::QuadPpt, Profile::ChainTsu, Profile::CapsuleVs, Profile::PanelPdp][i % 4]
}

impl App {
    pub fn new(input: Input, synth: SharedSynth, audio_ok: bool) -> Self {
        let st = Settings::load();
        let screen = if !st.benched {
            Screen::Boot { frames: 0, start: std::time::Instant::now() }
        } else if st.first_boot {
            Screen::Setup { page: 0, sel: 0 }
        } else {
            Screen::Title
        };
        let mut app = App {
            st,
            prog: Progress::load(),
            ranks: crate::ranks::Ranks::load(),
            card: Card::load(),
            input,
            synth,
            audio_ok,
            cv: Canvas::new(crate::render::W, crate::render::H),
            r: Renderer::new(),
            screen,
            t: 0.0,
            host_pose: (0, 0.0),
            toast: None,
            maps: crate::assets::load_maps(),
            bg_world: 0,
            xmb_cat: 0,
            xmb_item: [0; 8],
            menu_open: false,
            menu_t: 0.0,
            menu_lift: [0.0; 12],
            settings_tabs: false,
            thumbs: std::collections::HashMap::new(),
            last_input_t: 0.0,
            dt: 1.0 / 60.0,
            monitor: crate::display::Monitor::default(),
            modes: Vec::new(),
            fps: 0.0,
            script: crate::script::Script::from_env(),
            display_undo: None,
            next_chars: None,
            fade_in: 0.0,
            boot_draw: 0.0,
            scaled_maps: std::collections::HashMap::new(),
            blurred_maps: std::collections::HashMap::new(),
            title_zoom: std::collections::HashMap::new(),
            vignette: (0, Vec::new()),
        };
        app.apply_settings();
        app
    }

    pub fn apply_settings(&mut self) {
        self.input.bindings = self.st.bindings.clone();
        self.input.analog = self.st.analog;
        self.input.socd_neutral = self.st.socd_neutral;
        if let Ok(mut s) = self.synth.lock() {
            s.music_vol = self.st.music_vol as f32 / 10.0 * 0.8;
            s.sfx_vol = self.st.sfx_vol as f32 / 10.0;
            s.crt = self.st.theme == ThemeId::Phosphor;
            s.calm_only = self.st.preset == Preset::Potato;
        }
        self.r.sprites.clear();
    }

    pub(crate) fn sfx(&self, s: Sfx) {
        if let Ok(mut x) = self.synth.lock() {
            x.play(s);
        }
    }
    pub(crate) fn say(&mut self, s: impl Into<String>) {
        self.toast = Some((s.into(), 2.2));
    }

    pub fn frame(&mut self, win: Option<&minifb::Window>, dt: f32) -> bool {
        self.t += dt;
        let mut script_shots = Vec::new();
        let mut script_quit = false;
        // glass re-snapshots the scene each frame (taken lazily by the first glass panel)
        self.cv.drop_backdrop();
        // the cast stands in neutral light unless a screen says otherwise (story scenes)
        crate::light::set(crate::light::Rig::STUDIO);
        self.input.poll(win);
        self.dt = dt;
        if let Some(sc) = &mut self.script {
            let (p, h, shots, quit) = sc.step(dt);
            self.input.inject(&p, &h);
            script_shots = shots;
            script_quit = quit;
        }
        let nav = self.input.nav(win, dt);
        if nav.up || nav.down || nav.left || nav.right || nav.ok || nav.back || nav.click {
            self.last_input_t = self.t;
        }
        let scr = std::mem::replace(&mut self.screen, Screen::Title);
        let next = self.run(scr, &nav, dt);
        self.screen = next;
        if self.fade_in > 0.0 && !matches!(self.screen, Screen::Scene(_)) {
            self.cv.rect_a(0, 0, crate::render::W, crate::render::H, 0xFF00_0000, (self.fade_in / 0.45 * 255.0).min(255.0) as u32);
            self.fade_in -= dt;
        }
        if let Some((_, t)) = &mut self.toast {
            *t -= dt;
        }
        if self.toast.as_ref().map(|t| t.1 <= 0.0).unwrap_or(false) {
            self.toast = None;
        }
        if let Some((s, _)) = &self.toast {
            let s = &ui::nice(s);
            let l = look(self.st.theme);
            // top centre, clear of every footer and list
            let w = Canvas::text_wb(s, 2, crate::font::Weight::Bold) + 48;
            self.cv.glass(640 - w / 2, 60, w, 40, 20, ui::GLASS, 210);
            self.cv.text_weight(640 - w / 2 + 24, 69, s, 2, l.accent, 255, crate::font::Weight::Bold);
        }
        // persistent real-time clock, top right on every screen
        if !matches!(self.screen, Screen::Boot { .. } | Screen::Scene(_)) {
            let txt = crate::clock::text();
            let w = Canvas::text_wb(&txt, 2, crate::font::Weight::Bold) + 52;
            let x = crate::render::W - w - 10;
            self.cv.glass(x, 8, w, 36, 18, ui::GLASS, ui::GLASS_A);
            crate::assets::icon(&mut self.cv, "timer", x + 20, 26, 20, look(self.st.theme).accent, 255);
            self.cv.text_weight(x + 36, 16, &txt, 2, 0xFFFF_FFFF, 255, crate::font::Weight::Bold);
        }
        if self.st.theme == ThemeId::Phosphor {
            self.cv.scanlines(60);
        }
        if self.st.show_fps && self.fps > 0.0 {
            let txt = format!("{:.0} fps  {}x{}", self.fps, self.cv.w, self.cv.h);
            self.cv.text_weight(12, 700, &txt, 1, 0xFF7C_FFB0, 255, crate::font::Weight::Bold);
        }
        for f in script_shots {
            let _ = crate::png::write(&f, self.cv.w as usize, self.cv.h as usize, &self.cv.px);
        }
        !matches!(self.screen, Screen::Quit) && !script_quit
    }

    pub(crate) fn bg(&mut self, _energy: f32) {
        self.bg_blur(80);
    }
    /// Menu backdrop: a blurred, darkened copy of the current world (XMB-style), so text on
    /// top of it always reads.
    pub(crate) fn bg_blur(&mut self, dim: u32) {
        if self.st.theme == ThemeId::Voyage && !self.maps.is_empty() {
            let wi = self.bg_world % self.maps.len();
            let key = (wi, self.cv.w, dim);
            if !self.blurred_maps.contains_key(&key) {
                if self.blurred_maps.len() > 12 {
                    self.blurred_maps.clear();
                }
                // blur at layout size (cheap, and the blur is soft anyway), scale up, bake the dim
                let b = crate::gfx::blurred(&self.maps[wi].image, 14);
                let mut b = crate::assets::resample(&b, self.cv.w, self.cv.h);
                b.k = self.cv.k;
                b.rect_a(0, 0, crate::render::W, crate::render::H, 0xFF06_0A18, dim);
                let bd = crate::gfx::Backdrop::from_image(&b);
                self.blurred_maps.insert(key, (b, bd));
            }
            let (img, bd) = &self.blurred_maps[&key];
            self.cv.copy_from(img);
            self.cv.set_backdrop(bd.clone());
        } else {
            let t = self.t;
            self.r.scene.draw(&mut self.cv, &self.st, t, 0.0);
        }
    }
    /// Draw world `wi`'s rendered map over the whole screen (cached at the screen size).
    fn draw_map(&mut self, wi: usize) {
        let key = (wi, self.cv.w);
        if !self.scaled_maps.contains_key(&key) {
            if self.scaled_maps.len() > 10 {
                self.scaled_maps.clear();
            }
            let m = crate::assets::resample(&self.maps[wi].image, self.cv.w, self.cv.h);
            let bd = crate::gfx::Backdrop::from_image(&m);
            self.scaled_maps.insert(key, (m, bd));
        }
        let (img, bd) = &self.scaled_maps[&key];
        self.cv.copy_from(img);
        self.cv.set_backdrop(bd.clone());
    }

    /// The host, Mira. (x, y, s) keep the old blob's placement: centre and radius.
    fn host(&mut self, x: f32, y: f32, s: f32) {
        if !self.st.characters {
            return;
        }
        let pose = if self.host_pose.1 > self.t { self.host_pose.0 } else { 0 };
        let idle = self.t - self.last_input_t;
        let expr = match pose {
            1 => "delighted",
            2 => "shocked",
            3 => "sad",
            _ if idle > 25.0 => "sleepy",
            _ => "smile",
        };
        // the 3D cast is tall and slim: scale them up from the old chibi sizes
        let h = (s * 8.0) as i32;
        let bob = if self.st.reduce_motion { 0.0 } else { (self.t * 2.0).sin() * 2.0 };
        crate::assets::draw_char(&mut self.cv, "mira", expr, x as i32, (y + s * 1.25 + bob) as i32, h, 255);
    }
    fn pose(&mut self, p: u8) {
        self.host_pose = (p, self.t + 2.0);
    }

    /// Open the main menu on a section's tile page (2 Solo, 3 Training, ...) and draw it now.
    fn open_section(&mut self, cat: usize, nav: &Nav) -> Screen {
        if !self.menu_open || self.xmb_cat != cat {
            self.menu_open = true;
            self.menu_t = 0.0;
        }
        self.xmb_cat = cat;
        ui::set_section(MENU_CATS[cat].color);
        self.main_menu(nav, ListState::default())
    }

    /// The section color of a screen: its part of the game.
    fn section_of(&self, scr: &Screen) -> u32 {
        fn of_origin(o: &Origin) -> u32 {
            match o {
                Origin::Story { .. } => ui::S_STORY,
                Origin::Versus(_) => ui::S_VERSUS,
                Origin::Solo(..) | Origin::Arcade { .. } => ui::S_SOLO,
                Origin::Lesson(_) | Origin::Sandbox(..) | Origin::Demo(_) => ui::S_TRAINING,
                Origin::Replay => ui::S_REPLAYS,
            }
        }
        match scr {
            Screen::Main(_) => MENU_CATS[self.xmb_cat.min(MENU_CATS.len() - 1)].color,
            Screen::Story { .. } | Screen::Scene(_) | Screen::NameHero { .. } | Screen::Title | Screen::Boot { .. } => ui::S_STORY,
            Screen::Versus(_) | Screen::Select(_) => ui::S_VERSUS,
            Screen::Solo | Screen::Arcade(..) => ui::S_SOLO,
            Screen::Training | Screen::Curriculum(..) | Screen::Lesson(..) | Screen::Sandbox(..) | Screen::Calc(..) | Screen::Seeds(..) => ui::S_TRAINING,
            Screen::Replays(..) => ui::S_REPLAYS,
            Screen::Card(..) => ui::S_PROFILE,
            Screen::Ranks { .. } => ui::S_RANKS,
            Screen::Settings(..) | Screen::Controls(..) | Screen::Help(_) | Screen::Setup { .. } | Screen::Tour { .. } | Screen::Quit => ui::S_SETTINGS,
            Screen::Play(g) => of_origin(&g.origin),
            Screen::Results(r) => of_origin(&r.game.origin),
        }
    }

    fn run(&mut self, scr: Screen, nav: &Nav, dt: f32) -> Screen {
        let c = self.section_of(&scr);
        ui::set_section(if self.st.theme == ThemeId::Phosphor { crate::theme::theme_tint(ThemeId::Phosphor, c) } else { c });
        ui::set_clock(self.t, !self.st.reduce_motion);
        // bosses of unbeaten worlds are silhouettes everywhere except where you actually meet
        // them: their scenes, their matches and the results right after
        let meet = matches!(scr, Screen::Scene(_) | Screen::Play(_) | Screen::Results(_));
        crate::assets::set_hidden(if meet { Vec::new() } else { crate::cast::locked_bosses(&self.prog) });
        match scr {
            Screen::Boot { frames, start } => self.boot(frames, start),
            Screen::Title => self.title(nav),
            Screen::Main(s) => self.main_menu(nav, s),
            Screen::Story { world, node, diff, chosen, talk } => self.story(nav, world, node, diff, chosen, talk),
            Screen::Versus(room) => self.versus(nav, room),
            // the old Solo and Training lists are now the menu's tile pages for those sections
            Screen::Solo => self.open_section(2, nav),
            Screen::Arcade(s, k) => self.arcade(nav, s, k),
            Screen::Training => self.open_section(3, nav),
            Screen::Curriculum(s, tr) => self.curriculum(nav, s, tr),
            Screen::Lesson(i, s, page) => self.lesson(nav, i, s, page),
            Screen::Sandbox(s, a, b, c) => self.sandbox(nav, s, a, b, c),
            Screen::Calc(s, a, b, c, d) => self.calc(nav, s, a, b, c, d),
            Screen::Seeds(s, seed, txt) => self.seeds(nav, s, seed, txt),
            Screen::Replays(s, files) => self.replays(nav, s, files),
            Screen::Settings(s, tab) => self.settings(nav, s, tab),
            Screen::Controls(s, layout, waiting) => self.controls(nav, s, layout, waiting),
            Screen::Card(s, editing) => self.card_screen(nav, s, editing),
            Screen::Help(page) => self.help(nav, page),
            Screen::Play(g) => self.play(nav, g, dt),
            Screen::Results(r) => self.results(nav, r),
            Screen::Scene(v) => self.scene_screen(nav, v),
            Screen::Select(s) => self.select_screen(nav, s),
            Screen::Setup { page, sel } => self.setup_screen(nav, page, sel),
            Screen::Ranks { page, confirm } => self.ranks_screen(nav, page, confirm),
            Screen::Tour { page, t } => self.tour_screen(nav, page, t),
            Screen::NameHero { typed, then } => self.name_screen(nav, typed, then),
            Screen::Quit => Screen::Quit,
        }
    }

    // -------------------------------------------------------------- boot bench
    fn boot(&mut self, frames: u32, start: std::time::Instant) -> Screen {
        // One-second check of how long a full Voyage frame takes to *draw* (the loop is paced,
        // so frames per second would only measure the pacing). Slow machines get Potato.
        let saved = (self.st.theme, self.st.preset);
        self.st.theme = ThemeId::Voyage;
        self.st.preset = Preset::Voyage;
        let t0 = std::time::Instant::now();
        self.bg(1.0);
        for i in 0..12 {
            self.cv.glass(80 + i * 96, 300, 80, 120, 24, ui::GLASS, ui::GLASS_A);
        }
        let draw = t0.elapsed().as_secs_f32();
        let l = look(ThemeId::Voyage);
        self.cv.text_center(640, 330, "Getting the picture ready...", 3, l.text);
        self.st.theme = saved.0;
        self.st.preset = saved.1;
        self.boot_draw += draw;
        let el = start.elapsed().as_secs_f32();
        if el >= 1.0 && frames >= 10 {
            let ms = self.boot_draw / (frames + 1) as f32 * 1000.0;
            self.st.preset = if ms <= 24.0 { Preset::Voyage } else { Preset::Potato };
            self.st.benched = true;
            self.st.save();
            self.apply_settings();
            if self.st.preset == Preset::Potato {
                self.say(format!("This machine draws a frame in {:.0} ms: using the light Potato look (Settings to change)", ms));
            }
            self.boot_draw = 0.0;
            return if self.st.first_boot { Screen::Setup { page: 0, sel: 0 } } else { Screen::Title };
        }
        Screen::Boot { frames: frames + 1, start }
    }

    // -------------------------------------------------------------- title
    fn title(&mut self, nav: &Nav) -> Screen {
        use crate::font::Weight;
        let l = look(self.st.theme);
        let t = self.t;
        // attract mode: drift slowly across the five world maps, crossfading between them
        self.title_backdrop(t);
        crate::assets::logo(&mut self.cv, 640 - 450, 24, 900);
        let tag = "The open-source puzzle platform";
        let tw = Canvas::text_wb(tag, 3, Weight::Black) + 70;
        self.cv.round_rect(640 - tw / 2 - 4, 474, tw + 8, 60, 30, 0xFF3B_2A57, 255);
        self.cv.round_rect(640 - tw / 2, 478, tw, 52, 26, 0xFFFF_F6D6, 255);
        self.cv.text_weight(640 - (tw - 70) / 2, 488, tag, 3, 0xFF3B_2A57, 255, Weight::Black);
        if self.st.characters {
            crate::assets::draw_char(&mut self.cv, "mira", "delighted", 1120, 700, 330, 255);
        }
        // a bright pill that breathes: the one thing to do here
        let msg = "Press Enter or Start";
        let grow = if self.st.reduce_motion { 0 } else { (((t * 3.0).sin() * 0.5 + 0.5) * 6.0) as i32 };
        let pw = Canvas::text_wb(msg, 3, Weight::Display) + 70 + grow * 2;
        let (px, py, ph) = (640 - pw / 2, 578 - grow / 2, 56 + grow);
        self.cv.soft_shadow(px, py, pw, ph, ph / 2, 10, 110);
        self.cv.round_rect(px - 4, py - 4, pw + 8, ph + 8, ph / 2 + 4, 0xFFFF_FFFF, 255);
        self.cv.round_grad(px, py, pw, ph, ph / 2, 0xFFFF_4F8E, 0xFFE0_2A6C, 255);
        self.cv.round_rect(px + 20, py + 4, pw - 40, ph * 2 / 5, ph / 5, 0xFFFF_FFFF, 50);
        ui::outlined(&mut self.cv, 640 - Canvas::text_wb(msg, 3, Weight::Display) / 2, py + (ph - 40) / 2, msg, 3, 0xFFFF_FFFF, 0xFF9A_1446, 2, Weight::Display);
        let _ = &l;
        if nav.ok || nav.click {
            self.sfx(Sfx::Select);
            return Screen::Main(ListState::default());
        }
        if nav.back {
            return Screen::Quit;
        }
        Screen::Title
    }

    pub(crate) fn title_backdrop(&mut self, t: f32) {
        let _p = crate::prof::scope("title_bg");
        if self.maps.is_empty() {
            self.cv.clear(0xFF1A_2A55);
            return;
        }
        // attract mode: each world map, 12.5% larger than the screen, pans slowly; the next
        // one crossfades in. All in real pixels at the current resolution.
        const SEG: f32 = 9.0;
        let (sw, sh) = (self.cv.w, self.cv.h);
        let (zw, zh) = (sw * 9 / 8, sh * 9 / 8);
        let n = self.maps.len();
        let k = (t / SEG) as usize;
        let phase = (t % SEG) / SEG;
        for wi in [k % n, (k + 1) % n] {
            if !self.title_zoom.contains_key(&(wi, sw)) {
                if self.title_zoom.len() > 6 {
                    self.title_zoom.clear();
                }
                let z = crate::assets::resample(&self.maps[wi].image, zw, zh);
                let bd = crate::gfx::Backdrop::from_image(&z);
                self.title_zoom.insert((wi, sw), (z, bd));
            }
        }
        let reduce = self.st.reduce_motion;
        let offset = |ph: f32| {
            let ph = if reduce { 0.5 } else { ph };
            (((zw - sw) as f32 * ph) as i32, ((zh - sh) as f32 * (0.5 + 0.5 * (ph * 3.1).sin()) * 0.6) as i32)
        };
        let (ox, oy) = offset(phase);
        let fade = if phase > 0.85 { Some((((phase - 0.85) / 0.15 * 255.0) as u32).min(255)) } else { None };
        let (nx, ny) = offset(0.0);
        let (cur, cur_bd) = &self.title_zoom[&(k % n, sw)];
        let (nxt, _) = &self.title_zoom[&((k + 1) % n, sw)];
        // glass on the title refracts the panned map (prebuilt), not a fresh snapshot
        let cur_bd = cur_bd.clone();
        // vignette mask cached per resolution
        if self.vignette.0 != sw {
            let mut v = vec![0u8; (sw * sh) as usize];
            for y in 0..sh {
                let dy = (y as f32 / sh as f32 * 720.0 - 300.0) / 420.0;
                for x in 0..sw {
                    let dx = (x as f32 / sw as f32 * 1280.0 - 640.0) / 760.0;
                    v[(y * sw + x) as usize] = ((dx * dx + dy * dy) * 150.0).min(170.0) as u8;
                }
            }
            self.vignette = (sw, v);
        }
        let vig = &self.vignette.1;
        let dark = 0xFF06_0A18u32;
        // pan, crossfade and vignette in one pass, rows spread over the cores
        crate::gfx::par_rows(&mut self.cv.px, sw as usize, 0, sh, (sw * sh) as usize, |y, row| {
            let base = ((y + oy) * zw + ox) as usize;
            row.copy_from_slice(&cur.px[base..base + sw as usize]);
            if let Some(a) = fade {
                let nb = ((y + ny) * zw + nx) as usize;
                for (d, &s) in row.iter_mut().zip(nxt.px[nb..nb + sw as usize].iter()) {
                    *d = crate::gfx::mix(*d, s, a);
                }
            }
            let vrow = &vig[(y * sw) as usize..((y + 1) * sw) as usize];
            for (p, &a) in row.iter_mut().zip(vrow.iter()) {
                if a > 6 {
                    *p = crate::gfx::mix(*p, dark, a as u32);
                }
            }
        });
        self.cv.set_backdrop_at(cur_bd, ox, oy);
    }

    // -------------------------------------------------------------- main menu: XMB
    fn xmb_items(&self, cat: usize) -> Vec<(&'static str, String, String)> {
        let unlocked_world = |p: &Progress, wi: usize| wi == 0 || p.node(&format!("{}-5", wi - 1)) & 7 != 0;
        match cat {
            0 => {
                let mut v = vec![("compass", "Story map".to_string(), "Walk the overworld: five worlds that teach each style, then mixed matches.".to_string())];
                for (i, w) in WORLDS.iter().enumerate() {
                    let open = unlocked_world(&self.prog, i);
                    let done = (0..6).filter(|n| self.prog.node(&format!("{}-{}", i, n)) & 7 != 0).count();
                    v.push(("map-trifold", format!("World {}  -  {}", i + 1, w.name), if open { format!("{} of 6 nodes cleared. Learn {}.", done, w.style.to_lowercase()) } else { "Locked: clear the previous world's last node.".to_string() }));
                }
                v
            }
            1 => vec![
                ("sword", "Local match".to_string(), "Up to six seats on one machine. Each seat picks its own style. Keyboards, gamepads and bots.".to_string()),
                ("lightning", "Quick 1 vs 1".to_string(), "You against a bot right away, in your favorite style.".to_string()),
                ("flag-checkered", "Capsule race".to_string(), "Same bottle, first to clear every germ wins. The original two-player game.".to_string()),
            ],
            2 => vec![
                ("timer", "Quad Sprint".to_string(), "Clear 40 lines as fast as you can.".to_string()),
                ("flag-checkered", "Quad Twenty".to_string(), "Clear 20 lines.".to_string()),
                ("infinity", "Quad Marathon".to_string(), "Play until you top out. Speed rises every 10 lines.".to_string()),
                ("infinity", "Chain Endless".to_string(), "Build chains for as long as you can.".to_string()),
                ("target", "Capsule Level Run".to_string(), "Clear a bottle, then the next, and the next.".to_string()),
                ("infinity", "Panel Endless".to_string(), "Keep the rising stack under the top line.".to_string()),
                ("trophy", "Arcade ladder".to_string(), "Six rivals of rising strength, every style.".to_string()),
            ],
            3 => vec![
                ("books", "Curriculum".to_string(), {
                    // progress on the tile: lessons passed and the one to try next (they never gate anything)
                    let done = self.prog.lessons.len();
                    match LESSONS.iter().find(|x| !self.prog.lessons.contains_key(x.id)) {
                        Some(n) => format!("{} of {} lessons passed. Next up: {} ({}).", done, LESSONS.len(), n.title, ui::title_case(n.track)),
                        None => format!("All {} lessons passed. Replay any of them to beat your par.", LESSONS.len()),
                    }
                }),
                ("flask", "Sandbox".to_string(), "Rewind, frame advance, save states and garbage on demand.".to_string()),
                ("puzzle-piece", "Panel puzzles".to_string(), "Fixed boards. Clear every panel within the moves.".to_string()),
                ("calculator", "Calculators".to_string(), "Chain nuisance, attack tables and the cross-play gauge.".to_string()),
                ("dice-five", "Seed browser".to_string(), "See and share any deal: bags, dropsets, bottles and stacks.".to_string()),
            ],
            4 => {
                let mut v = vec![("folder-open", "All replays".to_string(), format!("Saved in {}", crate::save::data_dir().join("replays").display()))];
                for p in crate::save::list_replays().iter().take(6) {
                    v.push(("play-circle", p.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default(), "Watch this replay. Press T while watching to take over.".to_string()));
                }
                v
            }
            5 => vec![("identification-card", "Player card".to_string(), format!("{}  -  favorite style {}. Local only, no account needed.", self.card.name, Style::ALL[self.card.favorite % 4].name()))],
            6 => {
                let r = &self.ranks;
                let mut v = vec![("user-circle", format!("Your level  -  {}", r.level), format!("{}  {}  -  {} XP earned in all. Reach level 50 to prestige.", crate::ranks::title(r.level), crate::ranks::PRESTIGE[r.prestige as usize].0, r.lifetime))];
                for b in crate::ranks::BOARDS.iter() {
                    let top = r.boards.get(b.id).and_then(|l| l.first()).map(|e| format!("Top: {} by {}", crate::ranks::fmt_value(b, e.value), e.name)).unwrap_or_else(|| "No scores yet. Be the first!".into());
                    v.push(("trophy", b.name.to_string(), top));
                }
                v
            }
            _ => vec![
                ("palette", "Settings".to_string(), "Theme, picture preset, sound, handling and accessibility.".to_string()),
                ("game-controller", "Controls".to_string(), "Rebind keys. Gamepads are detected automatically.".to_string()),
                ("question", "How to play".to_string(), "The rules of each style, one page each.".to_string()),
                ("sparkle", crate::cast::say("Tour with Mira").into_owned(), crate::cast::say("Mira shows you around the modes and controls, with pictures.").into_owned()),
                ("power", "Quit".to_string(), "See you soon!".to_string()),
            ],
        }
    }

    fn main_menu(&mut self, nav: &Nav, _s: ListState) -> Screen {
        use crate::font::Weight;
        let l = look(self.st.theme);
        self.bg_blur(60);
        let n_cat = MENU_CATS.len();
        self.xmb_cat = self.xmb_cat.min(n_cat - 1);
        let k = 1.0 - (-16.0 * self.dt.clamp(0.0, 0.1)).exp();
        self.menu_t += self.dt;
        // ---- input: arrows move between tiles; hover selects, click opens
        let tiles = if self.menu_open { item_tiles(self.xmb_items(self.xmb_cat).len()) } else { top_tiles().to_vec() };
        let cur = if self.menu_open { self.xmb_item[self.xmb_cat].min(tiles.len().saturating_sub(1)) } else { self.xmb_cat };
        let mut next = cur;
        for (go, dx, dy) in [(nav.left, -1, 0), (nav.right, 1, 0), (nav.up, 0, -1), (nav.down, 0, 1)] {
            if go {
                next = grid_step(&tiles, next, dx, dy);
            }
        }
        if nav.wheel != 0 {
            next = (next as i32 + nav.wheel).clamp(0, tiles.len() as i32 - 1) as usize;
        }
        let mut clicked = false;
        if let Some((mx, my)) = nav.mouse {
            if let Some(i) = tiles.iter().position(|&(x, y, w, h)| mx >= x && mx < x + w + TILE_SKEW && my >= y && my < y + h) {
                if nav.click {
                    clicked = true;
                }
                // hovering selects only while the pointer moves; a resting mouse leaves the
                // arrow keys in charge
                if nav.moved || nav.click {
                    next = i;
                }
            }
        }
        if next != cur {
            self.sfx(Sfx::Menu);
        }
        if self.menu_open {
            self.xmb_item[self.xmb_cat] = next;
        } else {
            self.xmb_cat = next;
        }
        let activate = nav.ok || clicked;
        // ---- leave or open
        if nav.back {
            self.sfx(Sfx::Back);
            if self.menu_open {
                self.menu_open = false;
                self.menu_t = 0.0;
                return Screen::Main(ListState::default());
            }
            return Screen::Title;
        }
        let cat = self.xmb_cat;
        let items = self.xmb_items(cat);
        if activate && !self.menu_open && items.len() > 1 {
            self.sfx(Sfx::Select);
            self.menu_open = true;
            self.menu_t = 0.0;
            return Screen::Main(ListState::default());
        }
        let sel = if self.menu_open { self.xmb_item[cat].min(items.len().saturating_sub(1)) } else { 0 };
        // ---- draw
        let c = ui::section();
        let n = tiles.len();
        for (i, lift) in self.menu_lift.iter_mut().enumerate() {
            let on = if self.menu_open { i == sel } else { i == cat };
            *lift += ((on as u32 as f32) - *lift) * k;
            if i >= n {
                *lift = 0.0;
            }
        }
        let bob = |t: f32| if self.st.reduce_motion { 0 } else { ((t * 2.2).sin() * 3.0) as i32 };
        if !self.menu_open {
            crate::assets::logo(&mut self.cv, 26, 8, 150);
            let who = self.card.name.to_string();
            let ww = Canvas::text_wb(&who, 2, Weight::Bold) + 64;
            self.cv.glass(1130 - ww, 14, ww, 40, 20, ui::GLASS, ui::GLASS_A);
            crate::assets::icon(&mut self.cv, "user-circle", 1130 - ww + 26, 34, 24, l.accent, 255);
            self.cv.text_weight(1130 - ww + 46, 23, &who, 2, 0xFFFF_FFFF, 255, Weight::Bold);
            // the host stands beside the tiles, talking you through them
            if self.st.characters {
                let (id, pose) = HOST_POSE;
                crate::assets::draw_char_ex(&mut self.cv, id, pose, 1128, 662 + bob(self.t) / 3, 470, 255, true);
            }
            for (i, mc) in MENU_CATS.iter().enumerate() {
                let (x, y, w, h) = tiles[i];
                let lift = self.menu_lift[i];
                let y = y - (lift * 7.0) as i32;
                let on = i == cat;
                tile_base(&mut self.cv, x, y, w, h, mc.color, on);
                if i < 3 {
                    // a big tile: the mode's character standing in it, head above the edge
                    if self.st.characters {
                        let (id, pose) = mc.mascot;
                        let fy = y + h - 4 + if on { bob(self.t + i as f32) } else { 0 };
                        self.cv.set_clip(x - 40, 0, w + 80, y + h - 4);
                        crate::assets::draw_char_ex(&mut self.cv, id, pose, x + w - 84, fy + 26, 300, 255, i == 1);
                        self.cv.reset_clip();
                    }
                    badge(&mut self.cv, x + 46, y + 42, mc.icon, mc.color);
                    ui::outlined(&mut self.cv, x + 24, y + h - 62, mc.name, 4, 0xFFFF_FFFF, darken(mc.color, 130), 3, Weight::Display);
                } else {
                    crate::assets::icon(&mut self.cv, mc.icon, x + w / 2 + 9, y + 60, 66, darken(mc.color, 110), 255);
                    crate::assets::icon(&mut self.cv, mc.icon, x + w / 2 + 7, y + 56, 66, 0xFFFF_FFFF, 255);
                    let tw = Canvas::text_wb(mc.name, 3, Weight::Display);
                    ui::outlined(&mut self.cv, x + (w - tw) / 2 + 4, y + h - 50, mc.name, 3, 0xFFFF_FFFF, darken(mc.color, 130), 3, Weight::Display);
                }
            }
            let desc = match cat {
                0 => "Walk the overworld: five worlds, a rival in each, and a mystery underneath.",
                1 => "Up to six players on one machine, every one in the style they like.",
                2 => "Sprints, marathons and endless runs. Records stay on this machine.",
                3 => "Lessons with Mira, the sandbox, puzzles and calculators.",
                4 => "Watch saved matches, or take one over from any moment.",
                5 => "Your name, your favorite style, your card.",
                6 => "Your level, prestige and nine leaderboards.",
                _ => "Picture, sound, controls, and Mira's tour.",
            };
            ui::desc_strip(&mut self.cv, 40, 560, 950, 64, desc);
            ui::prompts(&mut self.cv, &l, &[("\u{2190}\u{2192}", "Choose"), ("Enter", "Open"), ("Esc", "Title")]);
        } else {
            let mc = &MENU_CATS[cat];
            ui::banner(&mut self.cv, mc.name, c);
            if self.st.characters {
                let (id, pose) = mc.mascot;
                let slide = if self.st.reduce_motion { 0.0 } else { (1.0 - (self.menu_t * 5.0).min(1.0)).powi(2) * 160.0 };
                crate::assets::draw_char_ex(&mut self.cv, id, pose, 1124 + slide as i32, 662 + bob(self.t) / 3, 480, 255, true);
            }
            // one label size for the whole page: the big one only if every label fits on a line
            let page_sc = if items.iter().zip(tiles.iter()).all(|((_, lb, _), t)| label_layout(t.2, lb, 3).lines.len() == 1) { 3 } else { 2 };
            for (i, (ic, label, desc)) in items.iter().enumerate() {
                let (x, y, w, h) = tiles[i];
                let lift = self.menu_lift[i];
                let y = y - (lift * 6.0) as i32;
                let on = i == sel;
                let locked = desc.starts_with("Locked");
                let shade = if locked { 0xFF6A_7088 } else if i % 2 == 1 { lighten(c, 14) } else { c };
                tile_base(&mut self.cv, x, y, w, h, shade, on);
                let lay = label_layout(w, label, page_sc);
                // the picture sits between the top edge and the label
                let wh = (h * 11 / 20).min(y + h - 18 - lay.block - 12 - (y + 16));
                if wh >= 40 {
                    self.tile_art(cat, i, x, y, w, wh, locked);
                }
                // leaderboards show their record in the middle of the tile
                if cat == 6 && i > 0 {
                    let top = desc.strip_prefix("Top: ").and_then(|d| d.split(" by ").next()).unwrap_or("No score yet");
                    let sc = if top.len() > 9 { 2 } else { 3 };
                    let tw = Canvas::text_wb(top, sc, Weight::Display);
                    let ty = y + 62 + (h - 62 - lay.block - 18 - (Canvas::px(sc) * 1.2) as i32) / 2;
                    ui::outlined(&mut self.cv, x + (w - tw) / 2 + 4, ty, top, sc, 0xFFFF_F0A0, darken(shade, 130), 2, Weight::Display);
                }
                badge(&mut self.cv, x + 42, y + 40, if locked { "question" } else { ic }, shade);
                item_label(&mut self.cv, x, y, h, &lay, shade);
            }
            if let Some((_, _, desc)) = items.get(sel) {
                ui::desc_strip(&mut self.cv, 40, 560, 950, 64, desc);
            }
            ui::prompts(&mut self.cv, &l, &[("\u{2190}\u{2192}", "Choose"), ("Enter", "Start"), ("Esc", "Back")]);
        }
        if !activate {
            return Screen::Main(ListState::default());
        }
        self.sfx(Sfx::Select);
        let dev = self.default_dev();
        match (cat, sel) {
            (0, 0) => Screen::Story { world: 0, node: 0, diff: 0, chosen: self.card.favorite % 4, talk: true },
            (0, w) => Screen::Story { world: w - 1, node: 0, diff: 0, chosen: self.card.favorite % 4, talk: true },
            (1, 0) => Screen::Versus(Box::new(Room::new())),
            (1, 1) => {
                let p = style_profile_for(self.card.favorite % 4);
                let foe = style_profile_for((self.card.favorite + 1) % 4);
                let h = quick_header(crate::now_seed(), Mode::Versus, &[(SeatKind::Human, p, Personality::Builder, 1), (SeatKind::Bot, foe, Personality::Builder, 4)]);
                let mut d = [None; 6];
                d[0] = Some(dev);
                let mut room = Room::new();
                room.seats[0].1 = p;
                room.seats[1].1 = foe;
                self.select_for(h, d, Origin::Versus(Box::new(room)), Screen::Main(ListState::default()))
            }
            (1, _) => {
                let mut room = Room::new();
                room.race = true;
                Screen::Versus(Box::new(room))
            }
            (2, 6) => Screen::Arcade(ListState::default(), 0),
            (2, i) => {
                let modes = [
                    (Mode::QuadSprint, Profile::QuadGuideline),
                    (Mode::QuadTwenty, Profile::QuadGuideline),
                    (Mode::QuadMarathon, Profile::QuadGuideline),
                    (Mode::ChainEndless, Profile::ChainTsu),
                    (Mode::CapsuleLevelRun, Profile::CapsuleNes),
                    (Mode::PanelEndless, Profile::PanelPdp),
                ];
                let (m, p) = modes[i.min(5)];
                let h = quick_header(crate::now_seed(), m, &[(SeatKind::Human, p, Personality::Builder, 1)]);
                let mut d = [None; 6];
                d[0] = Some(dev);
                self.start(h, d, Origin::Solo(m, p), false)
            }
            (3, 0) => Screen::Curriculum(ListState::default(), 0),
            (3, 1) => Screen::Sandbox(ListState::default(), 0, 0, 4),
            (3, 2) => Screen::Sandbox(ListState::default(), 99, 0, 0),
            (3, 3) => Screen::Calc(ListState::default(), 3, 4, 70, 0),
            (3, _) => Screen::Seeds(ListState::default(), 0x1234_5678, String::new()),
            (4, 0) => Screen::Replays(ListState::default(), crate::save::list_replays()),
            (4, i) => {
                let files = crate::save::list_replays();
                match files.get(i - 1).and_then(|p| std::fs::read(p).ok()).and_then(|b| Replay::from_bytes(&b)).and_then(|r| Game::from_replay(r).ok()) {
                    Some(g) => Screen::Play(Box::new(g)),
                    None => {
                        self.say("Could not open that replay");
                        Screen::Main(ListState::default())
                    }
                }
            }
            (5, _) => Screen::Card(ListState::default(), None),
            (6, i) => Screen::Ranks { page: i, confirm: false },
            (_, 0) => Screen::Settings(ListState::default(), 0),
            (_, 1) => Screen::Controls(ListState::default(), 0, None),
            (_, 2) => Screen::Help(0),
            (_, 3) => Screen::Tour { page: 0, t: 0.0 },
            _ => Screen::Quit,
        }
    }

    /// A tile's picture window: the world's map for story items, a few pieces for the styles.
    #[allow(clippy::too_many_arguments)]
    fn tile_art(&mut self, cat: usize, i: usize, x: i32, y: i32, w: i32, wh: i32, locked: bool) {
        let (wx, wy) = (x + w * 2 / 5 + 4, y + 16);
        let (ww, wh) = (w - w * 2 / 5 - 16, wh.min(120));
        if cat == 0 && !self.maps.is_empty() {
            let wi = i.saturating_sub(1).min(self.maps.len() - 1);
            let (pw, ph) = (self.cv.us(ww), self.cv.us(wh));
            let key = (wi, pw, ph);
            if !self.thumbs.contains_key(&key) {
                if self.thumbs.len() > 24 {
                    self.thumbs.clear();
                }
                // centre crop of the world map to the window's shape
                let src = &self.maps[wi].image;
                let sw = src.w.min(src.h * pw / ph.max(1));
                let sh = sw * ph / pw.max(1);
                let mut crop = Canvas::new(sw, sh);
                let (ox, oy) = ((src.w - sw) / 2, (src.h - sh) / 2);
                for yy in 0..sh {
                    let a = ((yy + oy) * src.w + ox) as usize;
                    crop.px[(yy * sw) as usize..((yy + 1) * sw) as usize].copy_from_slice(&src.px[a..a + sw as usize]);
                }
                self.thumbs.insert(key, crate::assets::resample(&crop, pw, ph));
            }
            self.cv.round_rect(wx - 4, wy - 4, ww + 8, wh + 8, 12, 0xFFFF_FFFF, 255);
            let img = &self.thumbs[&key];
            self.cv.blit(img, wx, wy, 255);
            if locked {
                self.cv.rect_a(wx, wy, ww, wh, 0xFF20_2436, 170);
            }
            return;
        }
        let Some(kind) = tile_pieces(cat, i) else { return };
        self.cv.round_rect(wx - 4, wy - 4, ww + 8, wh + 8, 12, 0xFFFF_FFFF, 255);
        self.cv.round_grad(wx, wy, ww, wh, 9, 0xFF14_1C44, 0xFF0A_0F28, 255);
        draw_pieces(&mut self.cv, &mut self.r.sprites, kind, wx, wy, ww, wh, if locked { 120 } else { 255 });
    }

    fn names_for(&self, h: &MatchHeader, chars: &[usize; 6]) -> [String; 6] {
        let mut hn = 0;
        std::array::from_fn(|i| match h.seats[i].kind {
            SeatKind::Human => {
                hn += 1;
                if hn == 1 {
                    ui::nice(&self.card.name)
                } else {
                    format!("P{}", i + 1)
                }
            }
            SeatKind::Bot => crate::cast::name_of(crate::cast::CAST[chars[i]].id),
            SeatKind::Closed => String::new(),
        })
    }

    /// Characters for a match nobody picked them for: you are your card's character (Mira in
    /// the story); bots are the world's rival and locals in the story, otherwise the champion
    /// of their style, then the arcade regulars, never repeating.
    fn default_chars(&self, h: &MatchHeader, origin: &Origin) -> [usize; 6] {
        use crate::cast::index;
        let mut out = [0usize; 6];
        let mut used: Vec<usize> = Vec::new();
        let me = match origin {
            Origin::Story { .. } => index("mira"),
            _ => index(self.card.character()),
        };
        // the story names who plays every bot seat of every node (content.rs)
        let story_foes: Option<&[&str]> = match origin {
            Origin::Story { world, node, .. } => Some(crate::content::WORLDS[(*world).min(4)].nodes[(*node).min(5)].who),
            _ => None,
        };
        let mut human_seen = false;
        let mut foe_k = 0;
        for i in 0..6 {
            let pick = match h.seats[i].kind {
                SeatKind::Closed => continue,
                SeatKind::Human if !human_seen => {
                    human_seen = true;
                    me
                }
                SeatKind::Human => ["kai", "sora", "emi", "ren", "taro"].iter().map(|n| index(n)).find(|c| !used.contains(c)).unwrap_or(1),
                SeatKind::Bot => {
                    // the champion of the bot's style, then anyone whose favourite style it is
                    let si = h.seats[i].profile.style().index();
                    let style_champ = ["nami", "hana", "yuki", "rin"][si];
                    let fans: Vec<&str> = crate::cast::CAST.iter().filter(|m| m.style == si).map(|m| m.id).collect();
                    let mut options: Vec<&str> = Vec::new();
                    if let Some(f) = story_foes {
                        if let Some(id) = f.get(foe_k) {
                            options.push(*id);
                        }
                        foe_k += 1;
                    }
                    options.push(style_champ);
                    options.extend(fans);
                    options.extend(["kai", "sora", "emi", "ren", "taro", "nami", "hana", "yuki", "rin", "kaede"]);
                    // outside the story, bots never play a boss you haven't beaten yet
                    let locked = if story_foes.is_some() { Vec::new() } else { crate::cast::locked_bosses(&self.prog) };
                    options.iter().filter(|n| !locked.contains(n)).map(|n| index(n)).find(|c| !used.contains(c)).unwrap_or(0)
                }
            };
            used.push(pick);
            out[i] = pick;
        }
        out
    }

    pub(crate) fn start_pub(&mut self, h: MatchHeader, devices: [Option<Device>; 6], origin: Origin, training: bool) -> Screen {
        self.start(h, devices, origin, training)
    }
    pub(crate) fn default_chars_pub(&self, h: &MatchHeader, origin: &Origin) -> [usize; 6] {
        self.default_chars(h, origin)
    }

    fn start(&mut self, h: MatchHeader, devices: [Option<Device>; 6], origin: Origin, training: bool) -> Screen {
        let chars = self.next_chars.take().unwrap_or_else(|| self.default_chars(&h, &origin));
        let names = self.names_for(&h, &chars);
        match Game::new(h, devices, names, origin, training) {
            Ok(mut g) => {
                g.chars = chars;
                self.r.fx = crate::render::Fx::new();
                self.r.scene.index = (self.r.scene.index + 1) % 5;
                self.bg_world = self.r.scene.index;
                if let Ok(mut s) = self.synth.lock() {
                    s.set_scene(self.r.scene.index);
                }
                Screen::Play(Box::new(g))
            }
            Err(e) => {
                self.say(format!("CANNOT START: {}", e));
                Screen::Main(ListState::default())
            }
        }
    }
    fn default_dev(&self) -> Device {
        match self.input.last_device {
            Some(Device::Pad(n)) => Device::Pad(n),
            _ => Device::KbFull,
        }
    }

    // -------------------------------------------------------------- story
    fn story(&mut self, nav: &Nav, mut world: usize, mut node: usize, mut diff: usize, mut chosen: usize, mut talk: bool) -> Screen {
        use crate::font::Weight;
        let l = look(self.st.theme);
        let unlocked_world = |p: &Progress, wi: usize| wi == 0 || p.node(&format!("{}-5", wi - 1)) & 7 != 0;
        let unlocked = |p: &Progress, wi: usize, ni: usize| unlocked_world(p, wi) && (ni == 0 || p.node(&format!("{}-{}", wi, ni - 1)) & 7 != 0);
        // the story: the prologue first, then each world's introduction on arrival
        const INTROS: [&str; 5] = ["w0_intro", "w1_intro", "w2_intro", "w3_intro", "w4_intro"];
        // the player names the courier before anything else happens
        if self.card.hero.is_empty() {
            self.input.take_typed();
            return Screen::NameHero { typed: String::new(), then: Box::new(Screen::Story { world, node, diff, chosen, talk }) };
        }
        if !self.scene_seen("prologue") {
            let back = Screen::Story { world, node, diff, chosen, talk };
            return self.play_scene("prologue", back);
        }
        if unlocked_world(&self.prog, world) && !self.scene_seen(INTROS[world.min(4)]) {
            let back = Screen::Story { world, node, diff, chosen, talk };
            return self.play_scene(INTROS[world.min(4)], back);
        }
        // the rendered overworld fills the screen
        let pts: Vec<(i32, i32)> = if world < self.maps.len() && self.maps[world].nodes.len() == 6 {
            self.draw_map(world);
            self.maps[world].nodes.clone()
        } else {
            self.bg(0.1);
            vec![(160, 470), (330, 360), (520, 430), (700, 320), (880, 400), (1080, 300)]
        };
        let w = &WORLDS[world];
        // header card
        // header: the world's name in letter tiles on the story banner, its number under it
        ui::banner(&mut self.cv, w.name, ui::section());
        let sub = format!("World {}   \u{00B7}   Learn {}", world + 1, w.style.to_lowercase());
        let sw = Canvas::text_wb(&sub, 2, Weight::Bold) + 36;
        self.cv.round_rect(28, 86, sw, 34, 17, ui::GLASS, 200);
        self.cv.text_weight(46, 91, &sub, 2, 0xFFFF_FFFF, 255, Weight::Bold);
        // node markers
        let t = self.t;
        for (k, &(x, y)) in pts.iter().enumerate() {
            let bits = self.prog.node(&format!("{}-{}", world, k));
            let open = unlocked(&self.prog, world, k);
            let done = bits & 7 != 0;
            let sel = k == node;
            let r = if sel { 30.0 + (t * 4.0).sin() * 2.0 } else { 24.0 };
            let ring = if !open { 0xFF5A_6075 } else if done { l.good } else { l.accent };
            self.cv.glass(x - r as i32, y - 26 - r as i32 - 10, (r * 2.0) as i32, (r * 2.0) as i32, r as i32, ui::GLASS, 150);
            self.cv.ring(x as f32, y as f32 - 36.0, r - 2.0, 3.0, ring, 255);
            let label = if open { format!("{}", k + 1) } else { "-".into() };
            let lw = Canvas::text_wb(&label, 2, Weight::Black);
            self.cv.text_weight(x - lw / 2, y - 47, &label, 2, if open { 0xFFFF_FFFF } else { 0xFF8A_90A6 }, 255, Weight::Black);
            for d in 0..3 {
                let on = bits & (1 << d) != 0;
                self.cv.circle((x - 12 + d * 12) as f32, y as f32 - 4.0, 4.5, if on { l.good } else { 0xFF2A_3046 }, 255);
                self.cv.ring((x - 12 + d * 12) as f32, y as f32 - 4.0, 4.5, 1.2, 0xFFFF_FFFF, 140);
            }
            if bits & 8 != 0 {
                self.cv.star(x as f32 + r, y as f32 - 36.0 - r, 9.0, l.accent, 255);
            }
        }
        let (hx, hy) = pts[node];
        self.host(hx as f32 + 40.0, hy as f32 - 80.0, 22.0);
        // node card, kept to the bottom band the maps leave clear
        let n = &w.nodes[node];
        let open = unlocked(&self.prog, world, node);
        self.cv.glass(40, 532, 1200, 160, 26, ui::GLASS, ui::GLASS_A);
        self.cv.text_weight(70, 548, &format!("{}.  {}", node + 1, n.name), 3, 0xFFFF_FFFF, 255, Weight::Black);
        let kind = match (n.level.mode, n.level.foes.len(), n.level.allies.len()) {
            (Mode::CapsuleRace, _, _) => "Capsule race".to_string(),
            (m, 0, _) if m.is_solo() => m.name().to_string(),
            (_, f, 0) if n.level.ffa => format!("Free for all, {} seats", f + 1),
            (_, 1, 0) => "1 vs 1".into(),
            (_, f, a) => format!("{} vs {}", a + 1, f),
        };
        let rid = crate::content::node_rival(world, node);
        let rname = crate::cast::name_of(rid);
        if self.st.characters {
            crate::light::set(crate::light::world(world.min(4), crate::light::Mood::Day));
            // the rival in their signature pose, with a name plate in the section color
            let bob = if self.st.reduce_motion { 0 } else { ((self.t * 2.0).sin() * 2.0) as i32 };
            crate::assets::draw_char_ex(&mut self.cv, rid, "p-rival", 1140, 676 + bob, 400, 255, true);
            let tag = format!("Rival  {}", if crate::assets::is_hidden(rid) { "???".to_string() } else { rname });
            let tw = Canvas::text_wb(&tag, 2, Weight::Display) + 40;
            let c = ui::section();
            let (tx, ty) = (1140 - tw / 2, 650);
            ui::slab(&mut self.cv, tx, ty, tw, 36, 10, c, 0xFFFF_FFFF, 3);
            ui::outlined(&mut self.cv, tx + 22, ty + 5, &tag, 2, 0xFFFF_FFFF, darken(c, 130), 2, Weight::Display);
        }
        let kw = Canvas::text_w(&kind, 2);
        self.cv.text_s(1040 - kw, 554, &kind, 2, l.accent);
        if talk || !self.st.decorations {
            self.cv.text_wrap(70, 590, 960, &crate::cast::say(n.line), 2, l.text);
        }
        let human_style = n.level.human.map(|p| p.style().name().to_string()).unwrap_or_else(|| format!("{}   (Tab to change)", style_profile_for(chosen).style().name()));
        self.cv.text_s(70, 650, &format!("You: {}", human_style), 2, l.text);
        self.cv.text_s(470, 650, &format!("Difficulty: {}", DIFF_NAMES[diff]), 2, l.accent);
        self.cv.text_s(740, 650, &format!("\u{2605}  {}", n.extra.text(diff)), 2, l.dim);
        let hint = if open { "Enter  play      \u{2190} \u{2192}  node and world      \u{2191} \u{2193}  difficulty      Esc  back" } else { "Locked: clear the previous node first" };
        self.cv.text_s(70, 676, hint, 1, if open { l.dim } else { l.warn });
        // left / right walk the nodes, and past either end into the next or previous world
        if nav.left {
            if node > 0 {
                node -= 1;
                talk = true;
                self.sfx(Sfx::Menu);
            } else if world > 0 {
                world -= 1;
                node = 5;
                talk = true;
                self.sfx(Sfx::Menu);
            }
        }
        if nav.right {
            if node < 5 {
                node += 1;
                talk = true;
                self.sfx(Sfx::Menu);
            } else if world < 4 && unlocked_world(&self.prog, world + 1) {
                world += 1;
                node = 0;
                talk = true;
                self.sfx(Sfx::Menu);
            } else if world < 4 {
                self.say("Finish this world's last node first");
            }
        }
        if nav.up && diff > 0 {
            diff -= 1;
        }
        if nav.down && diff < 2 {
            diff += 1;
        }
        if self.input.key_pressed(Key::Tab) || (0..4).any(|i| self.input.pad_pressed(i, crate::input::pad::Y)) {
            chosen = (chosen + 1) % 4;
        }
        if self.input.key_pressed(Key::E) || (0..4).any(|i| self.input.pad_pressed(i, crate::input::pad::RB)) {
            if world < 4 && unlocked_world(&self.prog, world + 1) {
                world += 1;
                node = 0;
            } else {
                self.say("Finish this world's last node first");
            }
        }
        if (self.input.key_pressed(Key::Q) || (0..4).any(|i| self.input.pad_pressed(i, crate::input::pad::LB))) && world > 0 {
            world -= 1;
            node = 0;
        }
        if let Some((mx, my)) = nav.mouse {
            if nav.click {
                for (k, &(x, y)) in pts.iter().enumerate() {
                    if (mx - x).abs() < 30 && (my - (y - 36)).abs() < 34 {
                        if k == node && open {
                            return self.start_node(world, node, diff, chosen);
                        }
                        node = k;
                    }
                }
            }
        }
        if nav.ok && open {
            return self.start_node(world, node, diff, chosen);
        }
        if nav.back {
            return Screen::Main(ListState::default());
        }
        self.bg_world = world;
        Screen::Story { world, node, diff, chosen, talk }
    }
    fn start_node(&mut self, world: usize, node: usize, diff: usize, chosen: usize) -> Screen {
        self.sfx(Sfx::Select);
        let seed = crate::now_seed();
        let p = style_profile_for(chosen);
        let h = node_header(world, node, diff, p, seed);
        let mut dev = [None; 6];
        dev[0] = Some(self.default_dev());
        let play = self.start(h, dev, Origin::Story { world, node, diff, chosen: p }, false);
        // before the match: the node's own short scene (wN_nK), and before that the world's
        // turning points: the quieter scene halfway (node 3) and the champion's scene (node 6)
        let pre = format!("w{}_n{}", world.min(4), node);
        let mut next = play;
        if !self.scene_seen(&pre) {
            next = self.play_scene(&pre, next);
        }
        let big = match node {
            2 => format!("w{}_mid", world.min(4)),
            5 => format!("w{}_boss", world.min(4)),
            _ => String::new(),
        };
        if !big.is_empty() && !self.scene_seen(&big) {
            next = self.play_scene(&big, next);
        }
        next
    }

    /// After a story match: the world's victory scene (and the epilogue) the first time its
    /// last node falls.
    fn after_match(&mut self, origin: &Origin, won: bool) -> Screen {
        let back = self.back_from(origin);
        const CLEAR: [&str; 5] = ["w0_clear", "w1_clear", "w2_clear", "w3_clear", "w4_clear"];
        if let Origin::Story { world, node: 5, .. } = origin {
            if won && !self.scene_seen(CLEAR[(*world).min(4)]) {
                // the finale: the Commons reopen, then the fall, then everyone brings Kai back
                let next = if *world >= 4 && !self.scene_seen("epilogue") {
                    let ep = self.play_scene("epilogue", back);
                    self.play_scene("w4_fall", ep)
                } else {
                    back
                };
                return self.play_scene(CLEAR[(*world).min(4)], next);
            }
        }
        // the rival (or Kai) reacts to the first win and the first loss at every node
        if let Origin::Story { world, node, .. } = origin {
            let id = format!("w{}_n{}_{}", (*world).min(4), node, if won { "win" } else { "lose" });
            if !self.scene_seen(&id) {
                return self.play_scene(&id, back);
            }
        }
        back
    }

    // -------------------------------------------------------------- versus room
    fn versus(&mut self, nav: &Nav, mut room: Box<Room>) -> Screen {
        use crate::font::Weight;
        self.bg(0.1);
        let l = look(self.st.theme);
        ui::title(&mut self.cv, if room.race { "Capsule Race" } else { "Versus" }, "", &l);
        // while the "More rules" dialog is open the lobby gets no input; the dialog gets it all
        // (and not on the frame that opens it, or the same Enter would flip its first rule)
        let real = if room.more { *nav } else { Nav::default() };
        let lobby = if room.more { Nav::default() } else { *nav };
        let nav = &lobby;
        // ---- geometry: one-press setups along the top, then "More rules"
        let current = quick_current(&room);
        let mut px = 40;
        let pill_rects: Vec<(i32, i32, i32, i32)> = QUICK
            .iter()
            .map(|q| q.0)
            .chain(std::iter::once("More rules"))
            .map(|k| {
                let w = Canvas::text_wb(k, 2, Weight::Bold) + 84;
                let r = (px, 92, w, 46);
                px += w + 12;
                r
            })
            .collect();
        let n_pills = pill_rects.len();
        let (cy, ch, cw, gap) = (160, 400, 188, 14);
        let card = |i: usize| (40 + i as i32 * (cw + gap), cy, cw, ch);
        let start_r = (460, 588, 360, 62);
        // moving up or down between rows lands on whatever sits nearest above or below
        let nearest = |rects: &[(i32, i32, i32, i32)], x: i32| -> usize {
            let mut best = (i32::MAX, 0);
            for (i, r) in rects.iter().enumerate() {
                let d = (r.0 + r.2 / 2 - x).abs();
                if d < best.0 {
                    best = (d, i);
                }
            }
            best.1
        };
        let cards: Vec<(i32, i32, i32, i32)> = (0..6).map(card).collect();
        // ---- input
        let mut act: Option<(u8, usize)> = None;
        let mut chg: Option<(usize, usize, i32)> = None;
        let hit = |r: (i32, i32, i32, i32), m: (i32, i32)| m.0 >= r.0 && m.0 < r.0 + r.2 && m.1 >= r.1 && m.1 < r.1 + r.3;
        if let Some(f) = room.edit {
            let i = room.focus.1;
            let fields = seat_fields(&room, i);
            let mut fi = fields.iter().position(|&x| x == f).unwrap_or(0);
            if nav.up && fi > 0 {
                fi -= 1;
                self.sfx(Sfx::Menu);
            }
            if nav.down && fi + 1 < fields.len() {
                fi += 1;
                self.sfx(Sfx::Menu);
            }
            room.edit = Some(fields[fi]);
            if nav.left {
                chg = Some((i, fields[fi], -1));
            }
            if nav.right {
                chg = Some((i, fields[fi], 1));
            }
            if let Some(m) = nav.mouse {
                if nav.click {
                    let (x, y, w, _) = card(i);
                    for (k, &fld) in fields.iter().enumerate() {
                        if hit((x, y + 92 + k as i32 * 50, w, 46), m) {
                            room.edit = Some(fld);
                            chg = Some((i, fld, 1));
                        }
                    }
                    if !hit(card(i), m) {
                        room.edit = None;
                    }
                }
            }
            if nav.ok || nav.back {
                room.edit = None;
                self.sfx(Sfx::Back);
            }
        } else {
            let (z, k) = room.focus;
            let before = room.focus;
            match z {
                0 => {
                    if nav.left && k > 0 {
                        room.focus.1 -= 1;
                    }
                    if nav.right && k + 1 < n_pills {
                        room.focus.1 += 1;
                    }
                    if nav.down {
                        let r = pill_rects[k.min(n_pills - 1)];
                        room.focus = (1, nearest(&cards, r.0 + r.2 / 2));
                    }
                }
                1 => {
                    if nav.left && k > 0 {
                        room.focus.1 -= 1;
                    }
                    if nav.right && k < 5 {
                        room.focus.1 += 1;
                    }
                    if nav.up {
                        let r = cards[k.min(5)];
                        room.focus = (0, nearest(&pill_rects, r.0 + r.2 / 2));
                    }
                    if nav.down {
                        room.focus = (2, 0);
                    }
                }
                _ => {
                    if nav.up {
                        room.focus = (1, nearest(&cards, start_r.0 + start_r.2 / 2));
                    }
                    // left / right from Start go to the seats on either side of it
                    if nav.left {
                        room.focus = (1, nearest(&cards, start_r.0 - 40));
                    }
                    if nav.right {
                        room.focus = (1, nearest(&cards, start_r.0 + start_r.2 + 40));
                    }
                }
            }
            // the pointer takes the focus only while it moves (or clicks), never by resting
            if let Some(m) = nav.mouse.filter(|_| nav.moved || nav.click) {
                for (i, r) in pill_rects.iter().enumerate() {
                    if hit(*r, m) {
                        room.focus = (0, i);
                        if nav.click {
                            act = Some((0, i));
                        }
                    }
                }
                for i in 0..6 {
                    if hit(card(i), m) {
                        room.focus = (1, i);
                        if nav.click {
                            act = Some((1, i));
                        }
                    }
                }
                if hit(start_r, m) {
                    room.focus = (2, 0);
                    if nav.click {
                        act = Some((2, 0));
                    }
                }
            }
            if room.focus != before {
                self.sfx(Sfx::Menu);
            }
            if nav.ok {
                act = Some(room.focus);
            }
            if nav.back {
                self.sfx(Sfx::Back);
                return Screen::Main(ListState::default());
            }
        }
        if let Some((i, f, d)) = chg {
            self.sfx(Sfx::Menu);
            seat_change(&mut room, i, f, d);
        }
        match act {
            Some((0, k)) if k < QUICK.len() => {
                // one press sets up the whole room; the cursor jumps to Start
                self.sfx(Sfx::Select);
                let pads = self.input.pad_names().len();
                let p1 = self.default_dev();
                apply_quick(&mut room, k, p1, pads);
                room.focus = (2, 0);
                self.say(format!("{}: press Enter to start", QUICK[k].0));
            }
            Some((0, _)) => {
                self.sfx(Sfx::Select);
                room.more = true;
                room.list = ListState::default();
            }
            Some((1, i)) => {
                self.sfx(Sfx::Select);
                if room.seats[i].0 == SeatChoice::Closed {
                    room.seats[i].0 = SeatChoice::Bot;
                }
                room.edit = Some(0);
            }
            Some(_) => {
                let seed = room.seed.unwrap_or_else(crate::now_seed);
                let h = room.header(seed);
                let humans = h.seats.iter().filter(|s| s.kind == SeatKind::Human).count();
                let active = h.seats.iter().filter(|s| s.kind != SeatKind::Closed).count();
                if active < 2 {
                    self.say("Open at least two seats");
                } else if humans == 0 {
                    self.say("Add a player: set a seat to a keyboard or gamepad");
                } else {
                    let mut devs = [None; 6];
                    for i in 0..6 {
                        if let SeatChoice::Human(d) = room.seats[i].0 {
                            devs[i] = Some(d);
                        }
                    }
                    let mut r2 = room.clone();
                    r2.edit = None;
                    self.sfx(Sfx::Select);
                    let back = Screen::Versus(r2.clone());
                    return self.select_for(h, devs, Origin::Versus(r2), back);
                }
            }
            None => {}
        }
        // ---- draw: quick setups (the one that matches the room wears a check) and More rules
        let c = ui::section();
        for (i, r) in pill_rects.iter().enumerate() {
            let on = room.focus == (0, i) && room.edit.is_none();
            let (x, y, w, h) = *r;
            let (icon, label) = if i < QUICK.len() { (QUICK[i].1, QUICK[i].0) } else { ("gear-six", "More rules") };
            if on {
                ui::highlight(&mut self.cv, x, y, w, h, self.t);
            } else if current == Some(i) {
                self.cv.round_rect(x - 2, y - 2, w + 4, h + 4, (h + 4) / 2, lighten(c, 60), 255);
                self.cv.round_rect(x, y, w, h, h / 2, 0xFF12_1838, 235);
            } else {
                self.cv.round_rect(x, y, w, h, h / 2, 0xFF12_1838, 200);
            }
            let ic = if on { 0xFFFF_FFFF } else { lighten(c, 90) };
            crate::assets::icon(&mut self.cv, icon, x + 30, y + h / 2, 26, ic, 255);
            self.cv.text_weight(x + 52, y + 11, label, 2, if on || current == Some(i) { 0xFFFF_FFFF } else { l.text }, 255, Weight::Bold);
            if current == Some(i) {
                let (bx, by) = (x + w - 14, y + 4);
                self.cv.circle(bx as f32, by as f32, 11.0, 0xFFFF_FFFF, 255);
                self.cv.circle(bx as f32, by as f32, 8.5, 0xFF2F_C46E, 255);
                let (fx, fy) = (bx as f32, by as f32);
                self.cv.poly(&[(fx - 5.0, fy - 0.5), (fx - 2.0, fy + 2.5), (fx - 2.0, fy + 5.5), (fx - 6.5, fy + 1.0)], 0xFFFF_FFFF, 255);
                self.cv.poly(&[(fx - 2.0, fy + 2.5), (fx + 4.5, fy - 4.0), (fx + 6.0, fy - 2.5), (fx - 2.0, fy + 5.5)], 0xFFFF_FFFF, 255);
            }
        }
        // ---- seat cards
        for i in 0..6 {
            let (x, y0, w, h) = card(i);
            let on = room.focus == (1, i);
            let y = y0 - if on { 6 } else { 0 };
            let (choice, prof, per, strength, team) = room.seats[i];
            let col = match choice {
                SeatChoice::Human(_) => crate::story_ui::SEAT_COLORS[i],
                SeatChoice::Bot => 0xFF5B_6894,
                SeatChoice::Closed => 0xFF2C_3356,
            };
            tile_base(&mut self.cv, x, y, w, h, col, on);
            let ink = darken(col, 130);
            if choice == SeatChoice::Closed {
                self.cv.circle((x + w / 2 + 6) as f32, (y + h / 2 - 20) as f32, 34.0, 0xFFFF_FFFF, if on { 230 } else { 90 });
                let pc = if on { col } else { 0xFF2C_3356 };
                self.cv.rect(x + w / 2 + 6 - 16, y + h / 2 - 23, 32, 6, pc);
                self.cv.rect(x + w / 2 + 6 - 3, y + h / 2 - 36, 6, 32, pc);
                let msg = "Add a seat";
                let tw = Canvas::text_wb(msg, 2, Weight::Bold);
                self.cv.text_weight(x + (w - tw) / 2 + 6, y + h / 2 + 30, msg, 2, 0xFFFF_FFFF, if on { 255 } else { 140 }, Weight::Bold);
                continue;
            }
            let (tag, who) = match choice {
                SeatChoice::Human(d) => (format!("P{}", i + 1), device_short(d)),
                _ => ("CPU".to_string(), ui::title_case(per.name())),
            };
            ui::outlined(&mut self.cv, x + 22, y + 12, &tag, 4, 0xFFFF_FFFF, ink, 3, Weight::Display);
            ui::fit_text(&mut self.cv, x + 20, y + 58, w - 40, &who, 0xFFFF_FFFF, 235, Weight::Bold);
            if team != 0 {
                let tc = [0xFFE8_3A4A, 0xFF2F_7BEA, 0xFF2C_B65A][(team as usize - 1) % 3];
                let (bx, by) = (x + w - 26, y + 34);
                self.cv.circle(bx as f32, by as f32, 19.0, 0xFFFF_FFFF, 255);
                self.cv.circle(bx as f32, by as f32, 15.0, tc, 255);
                let tn = format!("{}", team);
                let tw = Canvas::text_wb(&tn, 2, Weight::Display);
                self.cv.text_weight(bx - tw / 2, by - 13, &tn, 2, 0xFFFF_FFFF, 255, Weight::Display);
            }
            let profile = if room.race { Profile::CapsuleVs } else { prof };
            if room.edit.is_some() && on {
                // the seat's settings, edited in place
                for (k, &f) in seat_fields(&room, i).iter().enumerate() {
                    let ry = y + 92 + k as i32 * 50;
                    let sel = room.edit == Some(f);
                    if sel {
                        ui::highlight(&mut self.cv, x + 10, ry, w - 8, 46, self.t);
                    } else {
                        self.cv.round_rect(x + 10, ry, w - 8, 46, 23, 0xFF12_1838, 150);
                    }
                    let (lbl, val) = seat_field_text(&room, i, f);
                    self.cv.text_weight(x + 26, ry + 3, lbl, 1, 0xFFFF_FFFF, 200, Weight::Bold);
                    let v = if sel { format!("\u{2190} {} \u{2192}", val) } else { val };
                    ui::fit_text(&mut self.cv, x + 26, ry + 16, w - 44, &v, 0xFFFF_FFFF, 255, Weight::Bold);
                }
                continue;
            }
            // the style: a window of its pieces, its name, its rules
            let st = profile.style();
            let kind = match st {
                Style::Quad => 0,
                Style::Chain => 1,
                Style::Capsule => 2,
                Style::Panel => 3,
            };
            let (wx, wy, ww, wh) = (x + 18, y + 94, w - 24, 132);
            self.cv.round_rect(wx - 4, wy - 4, ww + 8, wh + 8, 12, 0xFFFF_FFFF, 255);
            self.cv.round_grad(wx, wy, ww, wh, 9, 0xFF14_1C44, 0xFF0A_0F28, 255);
            draw_pieces(&mut self.cv, &mut self.r.sprites, kind, wx, wy, ww, wh, 255);
            let sname = ui::title_case(st.name());
            ui::outlined(&mut self.cv, x + 22, y + 244, &sname, 3, 0xFFFF_FFFF, ink, 3, Weight::Display);
            ui::fit_text(&mut self.cv, x + 22, y + 284, w - 36, profile_rules(profile), 0xFFFF_FFFF, 220, Weight::Bold);
            if choice == SeatChoice::Bot {
                ui::fit_text(&mut self.cv, x + 22, y + 316, w - 36, "Strength", 0xFFFF_FFFF, 220, Weight::Bold);
                for k in 0..8 {
                    let filled = k < strength as i32;
                    let (sx, sy) = (x + 28 + k * 19, y + 352);
                    self.cv.star(sx as f32 + 1.0, sy as f32 + 2.0, 9.0, ink, 200);
                    self.cv.star(sx as f32, sy as f32, 9.0, if filled { 0xFFFF_E066 } else { 0xFF3A_4268 }, 255);
                }
            } else {
                ui::fit_text(&mut self.cv, x + 22, y + 316, w - 36, "Ready to play", 0xFFFF_FFFF, 220, Weight::Bold);
            }
        }
        // ---- how attacks travel in this room, beside the start button
        self.cv.text_wrap(40, 590, 400, &room.path_text(), 2, l.text);
        // ---- start
        let on = room.focus == (2, 0);
        let (sx, sy, sw, sh) = start_r;
        let sy = sy - if on { 4 } else { 0 };
        self.cv.soft_shadow(sx, sy, sw, sh, sh / 2, 10, 110);
        self.cv.round_rect(sx - 4, sy - 4, sw + 8, sh + 8, sh / 2 + 4, 0xFFFF_FFFF, if on { 255 } else { 160 });
        self.cv.round_grad(sx, sy, sw, sh, sh / 2, lighten(c, 40), darken(c, 20), 255);
        self.cv.round_rect(sx + 24, sy + 5, sw - 48, sh * 2 / 5, sh / 5, 0xFFFF_FFFF, 46);
        let msg = "Start match";
        let tw = Canvas::text_wb(msg, 4, Weight::Display);
        ui::outlined(&mut self.cv, sx + (sw - tw) / 2, sy + 9, msg, 4, 0xFFFF_FFFF, darken(c, 130), 3, Weight::Display);
        if room.edit.is_some() {
            ui::prompts(&mut self.cv, &l, &[("\u{2191}\u{2193}", "Setting"), ("\u{2190}\u{2192}", "Change"), ("Enter", "Done")]);
        } else {
            ui::prompts(&mut self.cv, &l, &[("Enter", if room.focus.0 == 1 { "Edit" } else { "Select" }), ("Esc", "Back")]);
        }
        // ---- the full rules, in a dialog over the lobby
        if room.more {
            self.cv.rect_a(0, 0, 1280, 720, 0xFF00_0000, 120);
            let body = ui::modal(&mut self.cv, 330, 60, 620, 636, "More rules");
            let o = room.opts;
            let yn = |b: bool| if b { "On" } else { "Off" };
            let rows = vec![
                Row::opt("Mode", if room.race { "Capsule race" } else { "Versus" }),
                Row::opt("Who attacks go to", room.target.name()),
                Row::opt("Teams", team_preset_name(&room)),
                Row::opt("Same deal for everyone", yn(o.shared_deal)),
                Row::opt("Always convert attacks", yn(o.force_cross)),
                Row::opt("Clearing every germ also wins", yn(o.virus_clear_wins)),
                Row::opt("Quad start level", format!("{}", o.quad_level)),
                Row::opt("Chain colors", format!("{}", o.chain_colors)),
                Row::opt("Capsule level", format!("{}", o.capsule_level)),
                Row::opt("Capsule speed", purtris_sim::capsule::SPEED_NAMES[o.capsule_speed as usize]),
                Row::opt("Capsule original-style deals", yn(o.nes_seeds)),
                Row::opt("Panel level", format!("{}", o.panel_level)),
                Row::opt("Panel colors", format!("{}", o.panel_colors)),
                Row::opt("Match seed", room.seed.map(|s| format!("{:08X}", s)).unwrap_or_else(|| "Random".into())),
                Row::act("Done"),
            ];
            let g = ListGeom { x: 370, y: body + 18, w: 540, row_h: 34, visible: 15, scale: 2 };
            let ev = list(&mut self.cv, &l, &real, &rows, &mut room.list, &g, self.t);
            let o = &mut room.opts;
            match ev {
                LE::Moved => self.sfx(Sfx::Menu),
                LE::Back | LE::Act(14) => {
                    self.sfx(Sfx::Back);
                    room.more = false;
                }
                LE::Chg(k, d) => {
                    self.sfx(Sfx::Menu);
                    match k {
                        0 => room.race = !room.race,
                        1 => room.target = cycle(&TargetRule::ALL, room.target, d),
                        2 => next_team_preset(&mut room),
                        3 => o.shared_deal = !o.shared_deal,
                        4 => o.force_cross = !o.force_cross,
                        5 => o.virus_clear_wins = !o.virus_clear_wins,
                        6 => o.quad_level = ((o.quad_level as i32 - 1 + d).rem_euclid(15) + 1) as u32,
                        7 => o.chain_colors = ((o.chain_colors as i32 - 3 + d).rem_euclid(3) + 3) as u8,
                        8 => o.capsule_level = ((o.capsule_level as i32 + d).rem_euclid(21)) as u32,
                        9 => o.capsule_speed = ((o.capsule_speed as i32 + d).rem_euclid(3)) as u8,
                        10 => o.nes_seeds = !o.nes_seeds,
                        11 => o.panel_level = ((o.panel_level as i32 - 1 + d).rem_euclid(10) + 1) as u32,
                        12 => o.panel_colors = ((o.panel_colors as i32 - 4 + d).rem_euclid(3) + 4) as u8,
                        13 => room.seed = if room.seed.is_some() { None } else { Some(crate::now_seed() & 0xFFFF_FFFF) },
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        Screen::Versus(room)
    }

    fn arcade(&mut self, nav: &Nav, mut s: ListState, _k: usize) -> Screen {
        self.bg(0.1);
        let l = look(self.st.theme);
        ui::title(&mut self.cv, "ARCADE LADDER", "Six rivals, rising strength, every style. Pick yours.", &l);
        let rows: Vec<Row> = Style::ALL.iter().map(|st| Row::act(format!("PLAY AS {}", st.name()))).chain(std::iter::once(Row::act("BACK"))).collect();
        let g = ListGeom { x: 90, y: 160, w: 600, row_h: 50, visible: 6, scale: 3 };
        let ev = list(&mut self.cv, &l, nav, &rows, &mut s, &g, self.t);
        ui::prompts(&mut self.cv, &l, &[("\u{2191}\u{2193}", "Choose"), ("Enter", "Start"), ("Esc", "Back")]);
        match ev {
            LE::Back => return Screen::Solo,
            LE::Act(4) => return Screen::Solo,
            LE::Act(i) => return self.arcade_stage(style_profile_for(i), 0),
            _ => {}
        }
        Screen::Arcade(s, 0)
    }
    fn arcade_stage(&mut self, profile: Profile, stage: usize) -> Screen {
        let foes = [Profile::QuadPpt, Profile::ChainTsu, Profile::CapsuleVs, Profile::PanelPdp, Profile::ChainTsu, Profile::QuadPpt];
        let pers = [Personality::Builder, Personality::Builder, Personality::Aggressor, Personality::Punisher, Personality::Aggressor, Personality::Builder];
        let fp = foes[(stage + profile.style().index()) % 6];
        let per = if fp.style() == Style::Panel && pers[stage] == Personality::Aggressor { Personality::Punisher } else { pers[stage] };
        let mut h = quick_header(crate::now_seed(), Mode::Versus, &[(SeatKind::Human, profile, Personality::Builder, 1), (SeatKind::Bot, fp, per, (2 + stage) as u8)]);
        h.opts.capsule_level = 3 + stage as u32;
        let mut dev = [None; 6];
        dev[0] = Some(self.default_dev());
        self.say(format!("STAGE {} OF 6", stage + 1));
        self.start(h, dev, Origin::Arcade { profile, stage }, false)
    }

    fn curriculum(&mut self, nav: &Nav, mut s: ListState, mut track: usize) -> Screen {
        self.bg(0.05);
        let l = look(self.st.theme);
        ui::title(&mut self.cv, "CURRICULUM", "", &l);
        // track tabs: left / right (or Q / E, LB / RB, a click) change track
        let names: Vec<String> = TRACKS.iter().map(|t| ui::title_case(t)).collect();
        let names: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
        let before = track;
        if let Some(t) = ui::tabs(&mut self.cv, &l, &names, track, 104, nav.mouse, nav.click) {
            track = t;
        }
        if nav.right || self.input.key_pressed(Key::E) || (0..4).any(|i| self.input.pad_pressed(i, crate::input::pad::RB)) {
            track = (track + 1) % TRACKS.len();
        }
        if nav.left || self.input.key_pressed(Key::Q) || (0..4).any(|i| self.input.pad_pressed(i, crate::input::pad::LB)) {
            track = (track + TRACKS.len() - 1) % TRACKS.len();
        }
        if track != before {
            s = ListState::default();
            self.sfx(Sfx::Menu);
        }
        let idxs: Vec<usize> = (0..LESSONS.len()).filter(|&i| LESSONS[i].track == TRACKS[track]).collect();
        let mut rows = Vec::new();
        let mut last_tier = "";
        let mut map = Vec::new();
        for &i in &idxs {
            let le = &LESSONS[i];
            if le.tier != last_tier {
                rows.push(Row::head(le.tier));
                map.push(usize::MAX);
                last_tier = le.tier;
            }
            let passed = self.prog.lessons.contains_key(le.id);
            rows.push(Row { label: le.title.to_uppercase(), value: if passed { "\u{2605} PASSED".into() } else { String::new() }, kind: ui::RK::Act });
            map.push(i);
        }
        let g = ListGeom { x: 90, y: 180, w: 760, row_h: 40, visible: 11, scale: 2 };
        let ev = list(&mut self.cv, &l, nav, &rows, &mut s, &g, self.t);
        ui::prompts(&mut self.cv, &l, &[("\u{2190}\u{2192}", "Track"), ("\u{2191}\u{2193}", "Lesson"), ("Enter", "Open"), ("Esc", "Back")]);
        if track == 1 {
            self.cv.text_wrap(900, 180, 330, "The Quad NES track (hypertap and rolling on the console repeat) waits on the quad.nes profile and its timing file. The tap lesson ships on Capsule, whose repeat lives in the sim.", 1, l.dim);
        }
        match ev {
            LE::Moved => self.sfx(Sfx::Menu),
            LE::Back => return Screen::Training,
            LE::Act(k) if map[k] != usize::MAX => return Screen::Lesson(map[k], ListState::default(), 0),
            _ => {}
        }
        Screen::Curriculum(s, track)
    }

    fn lesson(&mut self, nav: &Nav, i: usize, mut s: ListState, mut page: usize) -> Screen {
        use crate::font::Weight;
        self.bg(0.05);
        let l = look(self.st.theme);
        let le = &LESSONS[i];
        // Mira teaches each lesson in five short parts; the demo and the drill sit beside her
        let par = format!("Par: {}. Watch me do it in the demo first, then try the drill yourself.", le.obj.text(1));
        let parts: [(&str, &str, &str); 5] = [("The idea", le.idea, "smile"), ("Why it works", le.why, "smug"), ("Demo and drill", par.as_str(), "delighted"), ("Watch out", le.fail, "shocked"), ("In your replays", le.review, "smile")];
        page = page.min(parts.len() - 1);
        let (head, body, expr) = parts[page];
        let bob = if self.st.reduce_motion { 0 } else { ((self.t * 1.8).sin() * 3.0) as i32 };
        crate::assets::draw_char(&mut self.cv, "mira", expr, 190, 712 + bob, 640, 255);
        // lesson title strip
        let title = ui::title_case(le.title);
        self.cv.glass(360, 24, 880, 76, 28, ui::GLASS, 190);
        self.cv.text_weight(392, 36, &title, 3, 0xFFFF_FFFF, 255, Weight::Black);
        let sub = format!("{}  /  {}", ui::title_case(le.track), ui::title_case(le.tier));
        self.cv.text_s(392, 72, &sub, 1, l.dim);
        // Mira's speech bubble with the current part
        self.cv.glass(360, 118, 880, 300, 30, ui::GLASS, 205);
        self.cv.tri([(372.0, 380.0), (344.0, 406.0), (380.0, 400.0)], 0xFF2A_3456, 200);
        self.cv.text_weight(392, 136, &format!("{}.  {}", page + 1, head), 3, l.accent, 255, Weight::Black);
        self.cv.text_wrap(392, 186, 820, &crate::cast::say(body), 2, l.text);
        for k in 0..parts.len() {
            let on = k == page;
            self.cv.circle(392.0 + k as f32 * 22.0, 398.0, if on { 6.0 } else { 4.0 }, if on { l.accent } else { 0xFF5A_6488 }, 255);
        }
        if nav.right && page + 1 < parts.len() {
            page += 1;
            self.sfx(Sfx::Menu);
        }
        if nav.left && page > 0 {
            page -= 1;
            self.sfx(Sfx::Menu);
        }
        let rows = vec![Row::act(crate::cast::say("Watch Mira's demo").into_owned()), Row::act("Start the drill"), Row::act("Back to the curriculum")];
        let g = ListGeom { x: 420, y: 470, w: 520, row_h: 52, visible: 3, scale: 2 };
        let ev = list(&mut self.cv, &l, nav, &rows, &mut s, &g, self.t);
        let notes = crate::cast::say("Mira's notes");
        ui::prompts(&mut self.cv, &l, &[("\u{2190} \u{2192}", &notes), ("\u{2191} \u{2193}", "Choose"), ("Enter", "Select"), ("Esc", "Back")]);
        match ev {
            LE::Back | LE::Act(2) => {
                let tr = TRACKS.iter().position(|t| *t == le.track).unwrap_or(0);
                return Screen::Curriculum(ListState::default(), tr);
            }
            LE::Act(0) => {
                let h = lesson_header(le, 4242, false);
                let r = self.start(h, [None; 6], Origin::Demo(i), true);
                if let Screen::Play(mut g) = r {
                    g.stop_at = Some(60 * 25);
                    g.toast = Some(("DEMO: P pauses, O steps one tick, BACKSPACE rewinds".into(), 4.0));
                    return Screen::Play(g);
                }
                return r;
            }
            LE::Act(1) => {
                let h = lesson_header(le, crate::now_seed(), true);
                let mut dev = [None; 6];
                dev[0] = Some(self.default_dev());
                let r = self.start(h, dev, Origin::Lesson(i), true);
                if let Screen::Play(mut g) = r {
                    match le.obj {
                        Obj::SurviveTicks(n, _) => g.stop_at = Some(n),
                        Obj::ClearViruses => g.stop_on_clear = true,
                        _ => {}
                    }
                    return Screen::Play(g);
                }
                return r;
            }
            _ => {}
        }
        Screen::Lesson(i, s, page)
    }

    /// Sandbox and puzzle picker. style 99 = puzzle list.
    fn sandbox(&mut self, nav: &Nav, mut s: ListState, style: usize, opp: usize, strength: u8) -> Screen {
        self.bg(0.05);
        let l = look(self.st.theme);
        if style == 99 {
            ui::title(&mut self.cv, "PANEL PUZZLES", "A fixed board that does not rise. Clear every panel within the moves.", &l);
            let mut rows: Vec<Row> = purtris_sim::panel::PUZZLES
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let done = self.prog.records.contains_key(&format!("puzzle{}", i));
                    Row { label: format!("{}. {}", i + 1, p.name.to_uppercase()), value: format!("{} move{}{}", p.moves, if p.moves > 1 { "s" } else { "" }, if done { "  \u{2605}" } else { "" }), kind: ui::RK::Act }
                })
                .collect();
            rows.push(Row::act("BACK"));
            let g = ListGeom { x: 90, y: 150, w: 700, row_h: 44, visible: 10, scale: 2 };
            let ev = list(&mut self.cv, &l, nav, &rows, &mut s, &g, self.t);
            let n = purtris_sim::panel::PUZZLES.len();
            match ev {
                LE::Back => return Screen::Training,
                LE::Act(i) if i == n => return Screen::Training,
                LE::Act(i) => {
                    let h = quick_header(1, Mode::PanelPuzzle(i as u8), &[(SeatKind::Human, Profile::PanelPdp, Personality::Builder, 1)]);
                    let mut dev = [None; 6];
                    dev[0] = Some(self.default_dev());
                    return self.start(h, dev, Origin::Solo(Mode::PanelPuzzle(i as u8), Profile::PanelPdp), true);
                }
                _ => {}
            }
            return Screen::Sandbox(s, style, opp, strength);
        }
        ui::title(&mut self.cv, "SANDBOX", "Training mode: rewind, frame advance, save states, inject garbage.", &l);
        let opp_name = if opp == 0 { "none".to_string() } else { style_profile_for(opp - 1).style().name().to_string() };
        let rows = vec![
            Row::opt("Your style", style_profile_for(style).style().name()),
            Row::opt("Sparring bot", opp_name),
            Row::opt("Bot strength", format!("{}", strength)),
            Row::act("START"),
            Row::act("BACK"),
        ];
        let g = ListGeom { x: 90, y: 160, w: 640, row_h: 48, visible: 5, scale: 2 };
        let ev = list(&mut self.cv, &l, nav, &rows, &mut s, &g, self.t);
        ui::panel(&mut self.cv, 780, 160, 440, 300, &l);
        let help = ["P / START  pause", "O  step one tick (paused)", "BACKSPACE  hold to rewind", "F5 / F6  save state 1 / 2", "F8 / F9  load state 1 / 2", "G  inject garbage on yourself", "I  input display", "ESC  pause menu"];
        for (k, h) in help.iter().enumerate() {
            self.cv.text_s(800, 180 + k as i32 * 32, h, 2, l.text);
        }
        let (mut style, mut opp, mut strength) = (style, opp, strength);
        match ev {
            LE::Back | LE::Act(4) => return Screen::Training,
            LE::Chg(0, d) => style = ((style as i32 + d).rem_euclid(4)) as usize,
            LE::Chg(1, d) => opp = ((opp as i32 + d).rem_euclid(5)) as usize,
            LE::Chg(2, d) => strength = ((strength as i32 - 1 + d).rem_euclid(8) + 1) as u8,
            LE::Act(3) => {
                let p = style_profile_for(style);
                let mut seats = vec![(SeatKind::Human, if p == Profile::QuadPpt && opp == 0 { Profile::QuadGuideline } else { p }, Personality::Builder, 1)];
                if opp > 0 {
                    seats.push((SeatKind::Bot, style_profile_for(opp - 1), Personality::Builder, strength));
                }
                let mode = if opp > 0 { Mode::Versus } else { Mode::Training };
                let h = quick_header(crate::now_seed(), mode, &seats);
                let mut dev = [None; 6];
                dev[0] = Some(self.default_dev());
                let sb = if opp > 0 { Some((style_profile_for(opp - 1), strength)) } else { None };
                return self.start(h, dev, Origin::Sandbox(p, sb), true);
            }
            _ => {}
        }
        Screen::Sandbox(s, style, opp, strength)
    }

    fn calc(&mut self, nav: &Nav, mut s: ListState, links: u32, size: u32, target: u32, leftover: u32) -> Screen {
        self.bg(0.0);
        let l = look(self.st.theme);
        ui::title(&mut self.cv, "CALCULATORS", "Native units for each style, and the cross-play gauge.", &l);
        let rows = vec![
            Row::opt("Chain length", format!("{}", links)),
            Row::opt("Puyos per link", format!("{}", size)),
            Row::opt("Target points", format!("{}", target)),
            Row::opt("Leftover (seventieths)", format!("{}", leftover)),
            Row::act("BACK"),
        ];
        let g = ListGeom { x: 60, y: 140, w: 520, row_h: 40, visible: 5, scale: 2 };
        let ev = list(&mut self.cv, &l, nav, &rows, &mut s, &g, self.t);
        ui::prompts(&mut self.cv, &l, &[("\u{2191}\u{2193}", "Choose"), ("\u{2190}\u{2192}", "Change"), ("Esc", "Back")]);
        // chain result
        let mut total = 0u32;
        let mut left = leftover;
        ui::panel(&mut self.cv, 40, 356, 560, 328, &l);
        let mut y = 372;
        for (h, x) in [("Link", 110), ("Score", 250), ("Nuisance", 400), ("Leftover", 560)] {
            self.cv.text_right(x, y, h, 2, l.accent);
        }
        y += 28;
        for k in 1..=links {
            let sc = purtris_sim::chain::link_score(k, &[size], 1 << 1);
            let numer = sc + left;
            let sent = numer / target.max(1);
            left = numer % target.max(1);
            total += sent;
            {
                if y < 600 {
                    for (v, x) in [(k, 110), (sc, 250), (sent, 400), (left, 560)] {
                        self.cv.text_right(x, y, &v.to_string(), 2, l.text);
                    }
                    y += 24;
                }
            }
        }
        self.cv.text_s(60, 618, &format!("Total nuisance {}  =  {} rocks + {}", total, total / 30, total % 30), 2, l.good);
        let after = (target * 3 / 4).max(1);
        self.cv.text_s(60, 648, &format!("After the next margin step the target is {}", after), 1, l.dim);
        // tables
        ui::panel(&mut self.cv, 640, 130, 590, 540, &l);
        let mut y = 146;
        let t = |cv: &mut Canvas, y: &mut i32, s: &str, c: u32| {
            cv.text_s(660, *y, s, 1, c);
            *y += 14;
        };
        t(&mut self.cv, &mut y, "QUAD VS QUAD (lines): single 0  double 1  triple 2  quad 4", l.text);
        t(&mut self.cv, &mut y, "  T-spin single 2  double 4  triple 6  perfect clear 10  B2B +1", l.text);
        t(&mut self.cv, &mut y, "QUAD GAUGE (cross): s0 d1 t2 q4  TSS2 TSD3 TST4  PC6  B2B+1", l.text);
        t(&mut self.cv, &mut y, &format!("  combo column (PROVISIONAL): {:?}", purtris_sim::quad::COMBO), l.warn);
        y += 8;
        t(&mut self.cv, &mut y, "GAUGE -> NUISANCE (Chain defender):", l.accent);
        for row in 0..10 {
            let a = row + 1;
            let s = format!("  {:>2}->{:>3}    {:>2}->{:>3}    {:>2}->{:>3}", a, GAUGE_NUISANCE[a], a + 10, GAUGE_NUISANCE[a + 10], a + 20, GAUGE_NUISANCE[a + 20]);
            t(&mut self.cv, &mut y, &s, l.text);
        }
        y += 8;
        t(&mut self.cv, &mut y, "CAPSULE cross.capsule.v1 (original, provisional):", l.accent);
        t(&mut self.cv, &mut y, "  1 line no germ 0, with germ 1 | 2 lines 1 | 3 lines 2 | 4+ lines 4", l.text);
        t(&mut self.cv, &mut y, "  +1 per extra fall-chain step. 1 surviving gauge = 2 halves.", l.text);
        y += 8;
        t(&mut self.cv, &mut y, "PANEL chart (PROVISIONAL, waits on capture):", l.accent);
        t(&mut self.cv, &mut y, "  combo 4:3w 5:4w 6:5w 7:6w 8:3w+4w ...   chain n: 6 wide, n-1 tall", l.text);
        t(&mut self.cv, &mut y, "  cross.panel.v1: 3 cells = 1 gauge. Panel never cancels.", l.text);
        y += 8;
        t(&mut self.cv, &mut y, "CHAIN export cross.chain.v1: 6 nuisance = 1 gauge (provisional).", l.text);
        t(&mut self.cv, &mut y, "Shared cross travel delay: 30 ticks (0.5 s).", l.text);
        let (mut links, mut size, mut target, mut leftover) = (links, size, target, leftover);
        match ev {
            LE::Back | LE::Act(4) => return Screen::Training,
            LE::Chg(0, d) => links = ((links as i32 - 1 + d).rem_euclid(19) + 1) as u32,
            LE::Chg(1, d) => size = ((size as i32 - 4 + d).rem_euclid(8) + 4) as u32,
            LE::Chg(2, d) => target = ((target as i32 - 1 + d * 5).rem_euclid(120) + 1) as u32,
            LE::Chg(3, d) => leftover = ((leftover as i32 + d * 5).rem_euclid(target as i32)) as u32,
            _ => {}
        }
        Screen::Calc(s, links, size, target, leftover)
    }

    fn seeds(&mut self, nav: &Nav, mut s: ListState, mut seed: u64, mut text: String) -> Screen {
        self.bg(0.0);
        let l = look(self.st.theme);
        ui::title(&mut self.cv, "SEED BROWSER", "See and share the deal. Type hex digits to enter a seed.", &l);
        for c in self.input.take_typed() {
            if c.is_ascii_hexdigit() && text.len() < 8 {
                text.push(c.to_ascii_uppercase());
            }
        }
        if self.input.key_pressed(Key::Backspace) {
            text.pop();
        }
        if text.len() == 8 {
            if let Ok(v) = u64::from_str_radix(&text, 16) {
                seed = v;
            }
            text.clear();
        }
        let rows = vec![Row::opt("Seed", format!("{:08X}", seed)), Row::act("RANDOM SEED"), Row::act("BACK")];
        let g = ListGeom { x: 60, y: 130, w: 480, row_h: 40, visible: 3, scale: 2 };
        let nav2 = Nav { back: nav.back && !self.input.key_pressed(Key::Backspace), ..*nav };
        let ev = list(&mut self.cv, &l, &nav2, &rows, &mut s, &g, self.t);
        ui::prompts(&mut self.cv, &l, &[("\u{2191}\u{2193}", "Choose"), ("\u{2190}\u{2192}", "Change"), ("0-9 A-F", "Type a seed"), ("Esc", "Back")]);
        if !text.is_empty() {
            self.cv.text_s(560, 140, &format!("typing: {}_", text), 2, l.accent);
        }
        let h = quick_header(
            seed,
            Mode::Versus,
            &[
                (SeatKind::Human, Profile::QuadPpt, Personality::Builder, 1),
                (SeatKind::Human, Profile::ChainTsu, Personality::Builder, 1),
                (SeatKind::Human, Profile::CapsuleVs, Personality::Builder, 1),
                (SeatKind::Human, Profile::PanelPdp, Personality::Builder, 1),
            ],
        );
        if let Ok(m) = Match::new(h) {
            // both cards end above the prompt band
            ui::panel(&mut self.cv, 40, 268, 600, 380, &l);
            ui::panel(&mut self.cv, 680, 268, 560, 380, &l);
            let mut y = 284;
            if let Some(Board::Quad(q)) = &m.seats[0].board {
                let s: String = q.queue.iter().take(28).map(|&t| purtris_sim::quad::PIECE_NAMES[t as usize]).collect();
                self.cv.text_s(60, y, &format!("Quad 7-bag (first {}):", s.len()), 2, l.accent);
                self.cv.text_s(60, y + 24, &s, 2, l.text);
                y += 70;
            }
            if let Some(Board::Chain(c)) = &m.seats[1].board {
                self.cv.text_s(60, y, "Chain dropset (first 4 pairs):", 2, l.accent);
                for (k, &(a, b)) in c.queue.iter().enumerate() {
                    for (j, col) in [b, a].iter().enumerate() {
                        self.cv.circle(80.0 + k as f32 * 40.0, (y + 40 + j as i32 * 24) as f32, 10.0, crate::theme::chain_color(*col), 255);
                    }
                }
                y += 110;
            }
            if let Some(Board::Capsule(c)) = &m.seats[2].board {
                self.cv.text_s(60, y, "Capsule bottle (level 5):", 2, l.accent);
                for yy in 1..purtris_sim::capsule::H {
                    for xx in 0..purtris_sim::capsule::W {
                        let i = (yy * purtris_sim::capsule::W + xx) as usize;
                        if c.kind[i] != 0 {
                            self.cv.circle(70.0 + xx as f32 * 10.0, (y + 22 + yy * 9) as f32, 4.0, crate::theme::capsule_color(c.color[i]), 255);
                        }
                    }
                }
            }
            if let Some(Board::Panel(p)) = &m.seats[3].board {
                self.cv.text_s(700, 284, "Panel starting stack:", 2, l.accent);
                for yy in purtris_sim::panel::VIS0..purtris_sim::panel::H {
                    for xx in 0..purtris_sim::panel::W {
                        let c = p.grid[(yy * purtris_sim::panel::W + xx) as usize].c;
                        if c != 0 {
                            self.cv.round_rect(700 + xx * 26, 300 + (yy - purtris_sim::panel::VIS0) * 26, 24, 24, 5, crate::theme::panel_color(c), 255);
                        }
                    }
                }
            }
        }
        match ev {
            LE::Back | LE::Act(2) => return Screen::Training,
            LE::Act(1) => seed = crate::now_seed() & 0xFFFF_FFFF,
            LE::Chg(0, d) => seed = (seed as i64 + d as i64).rem_euclid(1 << 32) as u64,
            _ => {}
        }
        Screen::Seeds(s, seed, text)
    }

    // -------------------------------------------------------------- replays
    fn replays(&mut self, nav: &Nav, mut s: ListState, files: Vec<std::path::PathBuf>) -> Screen {
        self.bg(0.0);
        let l = look(self.st.theme);
        ui::title(&mut self.cv, "REPLAYS", &format!("Saved in {}", crate::save::data_dir().join("replays").display()), &l);
        let mut rows: Vec<Row> = files.iter().take(60).map(|p| Row::act(p.file_stem().map(|x| x.to_string_lossy().to_string()).unwrap_or_default())).collect();
        rows.push(Row::act("BACK"));
        let g = ListGeom { x: 60, y: 140, w: 760, row_h: 34, visible: 15, scale: 2 };
        let ev = list(&mut self.cv, &l, nav, &rows, &mut s, &g, self.t);
        ui::prompts(&mut self.cv, &l, &[("\u{2191}\u{2193}", "Choose"), ("Enter", "Watch"), ("Esc", "Back")]);
        ui::panel(&mut self.cv, 860, 140, 360, 220, &l);
        self.cv.text_wrap(880, 160, 320, "ENTER: watch. While watching: T takes over seat 1 from this tick (the fork is a new replay), 1/2/4 sets speed, P pauses, O steps.", 2, l.text);
        match ev {
            LE::Back => return Screen::Main(ListState::default()),
            LE::Act(i) if i >= files.len().min(60) => return Screen::Main(ListState::default()),
            LE::Act(i) => {
                let p = &files[i];
                match std::fs::read(p).ok().and_then(|b| Replay::from_bytes(&b)) {
                    Some(rep) => match Game::from_replay(rep) {
                        Ok(g) => return Screen::Play(Box::new(g)),
                        Err(e) => self.say(format!("REPLAY ERROR: {}", e)),
                    },
                    None => self.say("Not a Puzzle Commons replay"),
                }
            }
            _ => {}
        }
        Screen::Replays(s, files)
    }

    // -------------------------------------------------------------- the match
    fn play(&mut self, nav: &Nav, mut g: Box<Game>, dt: f32) -> Screen {
        // inputs outside the sim: pause, training tools, theme
        // the key that opens the pause menu must not also close it in the same frame
        let mut just_paused = false;
        if g.paused {
            // pause menu handled below
        } else if self.input.pause_pressed() && !g.finished() {
            g.paused = true;
            g.pause_sel = 0;
            just_paused = true;
            self.sfx(Sfx::Back);
        }
        if g.training || g.playback.is_some() {
            if self.input.key_pressed(Key::O) && g.paused {
                g.step_once = true;
            }
            if self.input.key_held(Key::Backspace) {
                g.rewind(2);
            }
            if g.playback.is_none() {
                if self.input.key_pressed(Key::F5) {
                    g.save_slot(0);
                }
                if self.input.key_pressed(Key::F6) {
                    g.save_slot(1);
                }
                if self.input.key_pressed(Key::F8) {
                    g.load_slot(0);
                }
                if self.input.key_pressed(Key::F9) {
                    g.load_slot(1);
                }
                if self.input.key_pressed(Key::G) {
                    g.inject();
                }
            } else {
                for (k, sp) in [(Key::Key1, 1), (Key::Key2, 2), (Key::Key4, 4)] {
                    if self.input.key_pressed(k) {
                        g.speed = sp;
                    }
                }
                if self.input.key_pressed(Key::T) {
                    let d = self.default_dev();
                    g.take_over(d);
                    g.training = true;
                }
            }
            if self.input.key_pressed(Key::I) {
                self.st.input_display = !self.st.input_display;
            }
        }
        {
            let _p = crate::prof::scope("sim+bots");
            g.update(&mut self.input, &self.st, &self.synth, &mut self.r, dt);
        }
        let _pd = crate::prof::scope("draw");
        // draw
        let energy = self.r.fx.energy;
        let _ = energy;
        self.bg_blur(70);
        let shake = if self.st.shake && !self.st.reduce_motion { self.r.fx.shake } else { 0.0 };
        let sx = (shake * ((self.t * 50.0).sin())) as i32;
        let ah = if g.training { 634 } else { 668 };
        let active: Vec<usize> = (0..6).filter(|&i| g.m.seats[i].board.is_some()).collect();
        // the characters stand beside the boards, reacting to the match: always in the story,
        // optional in versus and solo; with more than two seats, you and the lead rival
        let story = matches!(g.origin, Origin::Story { .. });
        let flank = self.st.characters && self.st.theme != ThemeId::Tournament && ((story && active.len() <= 4) || (self.st.chars_versus && active.len() <= 2));
        let crowd = active.len() > 2;
        let area = if !flank {
            (20 + sx, 44, 1240, ah)
        } else if crowd {
            (205 + sx, 44, 870, ah)
        } else {
            (250 + sx, 44, 780, ah)
        };
        if flank {
            // story matches: the characters stand in their world's light
            if let Origin::Story { world, node, .. } = g.origin {
                crate::light::set(crate::light::world(world.min(4), if node == 5 { crate::light::Mood::Duel } else { crate::light::Mood::Day }));
            }
            let shown: Vec<usize> = if crowd { vec![active[0], *active.iter().find(|&&i| !g.humans[i]).unwrap_or(&active[1])] } else { active.clone() };
            for (k, &i) in shown.iter().enumerate() {
                let seat = &g.m.seats[i];
                let member = &crate::cast::CAST[g.chars[i].min(crate::cast::CAST.len() - 1)];
                let danger = seat.board.as_ref().map(|b| b.danger()).unwrap_or(false);
                let expr = if g.m.outcome != Outcome::Running && g.m.is_winner(i) {
                    "delighted"
                } else if !seat.alive {
                    "sad"
                } else if danger {
                    "shocked"
                } else if self.r.fx.flash[i] > 0.3 {
                    "angry"
                } else if g.countdown > 0 {
                    "smug"
                } else {
                    "smile"
                };
                let right = k == 1 || (active.len() == 1 && false);
                let cx = if right { if crowd { 1180 } else { 1150 } } else if crowd { 100 } else { 130 };
                let bob = if self.st.reduce_motion { 0 } else { ((self.t * 1.6 + k as f32).sin() * 3.0) as i32 };
                crate::assets::draw_char_ex(&mut self.cv, member.id, expr, cx, 712 + bob, if crowd { 520 } else { 600 }, 255, right);
                let mname = crate::cast::name_of(member.id);
                let nw = Canvas::text_wb(&mname, 2, crate::font::Weight::Black) + 36;
                self.cv.round_grad(cx - nw / 2, 640, nw, 36, 18, crate::gfx::lighten(member.color, 40), member.color, 255);
                self.cv.text_weight(cx - nw / 2 + 18, 646, &mname, 2, 0xFF16_1028, 255, crate::font::Weight::Black);
            }
        }
        let v = View {
            chars: g.chars,
            names: &g.names,
            humans: g.humans,
            settings: &self.st,
            t: self.t,
            tick_frac: 0.0,
            show_overlay_info: g.training,
            ghost_on: true,
            inputs: g.last_inputs,
            paused_label: None,
        };
        self.r.draw_match(&mut self.cv, &g.m, &v, area);
        let l = look(self.st.theme);
        // top bar: one glass strip, mode on the left, timer centred
        self.cv.glass(-20, -20, 1320, 58, 18, ui::GLASS, 150);
        let mode = if g.playback.is_some() { format!("Replay x{}", g.speed) } else { ui::title_case(g.m.header.mode.name()) };
        self.cv.text_weight(18, 7, &mode, 2, l.accent, 255, crate::font::Weight::Bold);
        let path = if g.m.cross { "Cross path" } else { "Authentic path" };
        let mw = Canvas::text_wb(&mode, 2, crate::font::Weight::Bold);
        self.cv.text_s(34 + mw, 13, &format!("{}    Seed {:08X}", path, g.m.header.seed as u32), 1, l.dim);
        let tm = fmt_time(g.m.tick);
        self.cv.text_weight(640 - Canvas::text_wb(&tm, 2, crate::font::Weight::Bold) / 2, 7, &tm, 2, 0xFFFF_FFFF, 255, crate::font::Weight::Bold);
        if self.st.theme == ThemeId::Voyage && self.st.preset != Preset::Potato {
            self.cv.text_right(1160, 13, SCENE_NAMES[self.r.scene.index], 1, l.dim);
        }
        if g.training && !g.paused {
            ui::footer(&mut self.cv, "Training:   P pause    O step    Backspace rewind    F5 / F8 save state    G garbage", &l);
        }
        // countdown: Ready, 3, 2, 1, then Go! (each number pops in and settles)
        let go = g.countdown == 0 && g.m.tick < 45 && !g.training && g.playback.is_none();
        if g.countdown > 0 || go {
            use crate::font::Weight;
            let n = g.countdown / 50 + 1;
            let txt = if go { "Go!".to_string() } else if n > 3 { "Ready".to_string() } else { format!("{}", n.min(3)) };
            let phase = if go { g.m.tick as f32 / 45.0 } else { 1.0 - (g.countdown % 50) as f32 / 50.0 };
            let pop = if self.st.reduce_motion { 0 } else { ((1.0 - (phase * 4.0).min(1.0)) * 3.0) as i32 };
            let sc = 9 + pop;
            let a = if go { ((1.0 - phase) * 255.0) as u32 } else { 255 };
            let w = Canvas::text_wb(&txt, sc, Weight::Display);
            let (x, y) = (640 - w / 2, 300 - (Canvas::px(sc) * 0.4) as i32);
            for (dx, dy) in [(-5, 0), (5, 0), (0, -5), (0, 5), (-4, -4), (4, 4), (-4, 4), (4, -4), (0, 8), (2, 9)] {
                self.cv.text_weight(x + dx, y + dy, &txt, sc, 0xFF14_1030, a * 9 / 10, Weight::Display);
            }
            self.cv.text_weight(x, y, &txt, sc, if go { l.good } else { l.accent }, a, Weight::Display);
        }
        if let Some((t, _)) = &g.toast {
            self.cv.text_center(640, 40, t, 2, l.accent);
        }
        if g.paused {
            self.cv.rect_a(0, 0, 1280, 720, 0xFF00_0000, 140);
            let rows = vec![Row::act("RESUME"), Row::act("RESTART"), Row::act(format!("Theme: {}", self.st.theme.name())), Row::act("QUIT TO MENU")];
            let mut ls = ListState { sel: g.pause_sel, scroll: 0 };
            let body = ui::modal(&mut self.cv, 430, 196, 420, if g.training { 330 } else { 306 }, "Paused");
            let gg = ListGeom { x: 470, y: body + 22, w: 340, row_h: 50, visible: 4, scale: 2 };
            let nav2 = Nav { back: false, ..*nav };
            let ev = list(&mut self.cv, &l, &nav2, &rows, &mut ls, &gg, self.t);
            g.pause_sel = ls.sel;
            if g.training {
                self.cv.text_center(640, 496, "O: step one tick    Backspace: rewind", 1, l.dim);
            }
            let resume = !just_paused && (nav.back || (self.input.pause_pressed() && !nav.ok));
            match ev {
                LE::Act(0) => {
                    g.paused = false;
                    g.update(&mut self.input, &self.st, &self.synth, &mut self.r, 0.0);
                }
                LE::Act(1) => {
                    let h = g.rep.header;
                    let mut h2 = h;
                    h2.seed = crate::now_seed();
                    let origin = g.origin.clone();
                    let dev = g.devices;
                    let training = g.training;
                    return self.start(h2, dev, origin, training);
                }
                LE::Act(2) => {
                    self.st.theme = cycle(&ThemeId::ALL, self.st.theme, 1);
                    self.st.save();
                    self.apply_settings();
                }
                LE::Act(3) => return self.back_from(&g.origin),
                _ => {
                    if resume && ev == LE::None {
                        g.paused = false;
                        g.update(&mut self.input, &self.st, &self.synth, &mut self.r, 0.0);
                    }
                }
            }
        }
        if g.finished() && g.over_t > 100 {
            return self.to_results(g);
        }
        Screen::Play(g)
    }

    fn back_from(&mut self, o: &Origin) -> Screen {
        if let Ok(mut s) = self.synth.lock() {
            s.set_intensity(false, false, false);
        }
        match o {
            Origin::Versus(r) => Screen::Versus(r.clone()),
            Origin::Solo(Mode::PanelPuzzle(_), _) => Screen::Sandbox(ListState::default(), 99, 0, 0),
            Origin::Solo(..) => Screen::Solo,
            Origin::Story { world, node, diff, chosen } => {
                let ci = Style::ALL.iter().position(|s| *s == chosen.style()).unwrap_or(0);
                Screen::Story { world: *world, node: *node, diff: *diff, chosen: ci, talk: false }
            }
            Origin::Lesson(i) | Origin::Demo(i) => Screen::Lesson(*i, ListState::default(), 2),
            Origin::Sandbox(..) => Screen::Sandbox(ListState::default(), 0, 0, 4),
            Origin::Arcade { .. } => Screen::Arcade(ListState::default(), 0),
            Origin::Replay => Screen::Replays(ListState::default(), crate::save::list_replays()),
        }
    }

    fn to_results(&mut self, g: Box<Game>) -> Screen {
        let mut objectives = Vec::new();
        let mut new_record = None;
        let won = g.won();
        let mut xp_bonus: Vec<(String, u64)> = Vec::new();
        if let Origin::Story { world, node, diff, .. } = &g.origin {
            let before = self.prog.node(&format!("{}-{}", world, node));
            if won && before & (1 << diff) == 0 {
                xp_bonus.push(("Story node cleared".into(), 150 + 50 * *diff as u64));
            }
            let n = &WORLDS[*world].nodes[*node];
            if won && *diff > 0 && before & 8 == 0 && g.objective_met(n.extra, *diff) {
                xp_bonus.push(("Bonus star".into(), 100));
            }
        }
        if let Origin::Lesson(i) = &g.origin {
            let le = &LESSONS[*i];
            if !self.prog.lessons.contains_key(le.id) && g.objective_met(le.obj, 1) {
                xp_bonus.push(("Lesson passed".into(), 120));
            }
        }
        match &g.origin {
            Origin::Story { world, node, diff, .. } => {
                let n = &WORLDS[*world].nodes[*node];
                objectives.push(("Win".to_string(), won));
                let extra_ok = *diff > 0 && g.objective_met(n.extra, *diff);
                if *diff > 0 {
                    objectives.push((n.extra.text(*diff), extra_ok));
                }
                if won {
                    let id = format!("{}-{}", world, node);
                    let mut bits = self.prog.node(&id) | (1 << diff);
                    if extra_ok {
                        bits |= 8;
                    }
                    self.prog.nodes.insert(id, bits);
                    self.prog.save();
                }
            }
            Origin::Lesson(i) => {
                let le = &LESSONS[*i];
                let ok = g.objective_met(le.obj, 1);
                objectives.push((format!("Par: {}", le.obj.text(1)), ok));
                if ok {
                    self.prog.lessons.insert(le.id.to_string(), 1);
                    self.prog.save();
                }
            }
            Origin::Solo(m, _) => {
                let key = format!("{:?}", m);
                match m {
                    Mode::QuadSprint | Mode::QuadTwenty if won => {
                        let t = g.m.tick as u64;
                        let best = self.prog.records.get(&key).copied().unwrap_or(u64::MAX);
                        if t < best {
                            self.prog.records.insert(key, t);
                            new_record = Some(format!("NEW RECORD {}", fmt_time(t as u32)));
                        }
                    }
                    Mode::PanelPuzzle(i) if won => {
                        self.prog.records.insert(format!("puzzle{}", i), 1);
                    }
                    Mode::PanelPuzzle(_) => {}
                    _ => {
                        let sc = g.m.seats[0].board.as_ref().map(|b| b.score()).unwrap_or(0);
                        let best = self.prog.records.get(&key).copied().unwrap_or(0);
                        if sc > best {
                            self.prog.records.insert(key, sc);
                            new_record = Some(format!("NEW RECORD {}", sc));
                        }
                    }
                }
                self.prog.save();
            }
            _ => {}
        }
        let findings = if matches!(g.origin, Origin::Lesson(_) | Origin::Sandbox(..) | Origin::Story { .. } | Origin::Solo(..) | Origin::Arcade { .. }) { analyze(&g) } else { Vec::new() };
        if new_record.is_some() {
            xp_bonus.push(("Personal record".into(), 150));
        }
        let gain = self.award(&g, won, xp_bonus);
        self.sfx(if won { Sfx::Win } else { Sfx::Lose });
        self.pose(if won { 1 } else { 3 });
        if let Ok(mut s) = self.synth.lock() {
            s.set_intensity(false, false, false);
        }
        Screen::Results(Box::new(Results { game: *g, objectives, findings, list: ListState::default(), saved: None, new_record, gain }))
    }

    /// XP for a finished match and its leaderboard entries.
    fn award(&mut self, g: &Game, won: bool, bonus: Vec<(String, u64)>) -> crate::ranks::Gain {
        use crate::ranks::Gain;
        let mut gain = Gain::default();
        if matches!(g.origin, Origin::Demo(_) | Origin::Replay) || g.playback.is_some() {
            return gain;
        }
        let me = (0..6).find(|&i| g.humans[i]).unwrap_or(0);
        let seat = &g.m.seats[me];
        let mut add = |why: &str, n: u64| {
            if n > 0 {
                gain.reasons.push((why.to_string(), n));
            }
        };
        add("Match played", if g.m.tick > 600 { 40 } else { 10 });
        if won && g.m.active_count() > 1 {
            add("Victory", 120);
        } else if won {
            add("Goal reached", 80);
        }
        if let Some(b) = &seat.board {
            match b {
                Board::Quad(q) => add("Lines cleared", (q.lines as u64 * 3).min(240)),
                Board::Chain(_) => add("Best chain", (seat.stats.max_chain as u64 * 30).min(300)),
                Board::Capsule(_) => {
                    let germs = g.log.iter().filter(|(_, s, e)| *s as usize == me && matches!(e, Ev::VirusClear)).count() as u64;
                    add("Germs cleared", (germs * 6).min(240));
                }
                Board::Panel(p) => add("Best chain", (p.max_chain as u64 * 25).min(250)),
            }
        }
        add("Attack sent", (seat.stats.sent as u64 * 2).min(200));
        for (why, n) in bonus {
            add(&why, n);
        }
        let base: u64 = gain.reasons.iter().map(|r| r.1).sum();
        let (xp, levels) = self.ranks.add_xp(base);
        gain.xp = xp;
        gain.levels_gained = levels;
        // leaderboards
        let name = ui::nice(&self.card.name);
        let ch = crate::cast::CAST[g.chars[me].min(crate::cast::CAST.len() - 1)].id.to_string();
        let place = |board: &str, rank: Option<usize>, gain: &mut Gain| {
            if let (Some(r), Some(b)) = (rank, crate::ranks::BOARDS.iter().find(|b| b.id == board)) {
                gain.placed.push((b.name.to_string(), r));
            }
        };
        match &g.origin {
            Origin::Solo(m, _) => {
                let (board, value) = match m {
                    Mode::QuadSprint if won => ("QuadSprint", Some(g.m.tick as u64)),
                    Mode::QuadTwenty if won => ("QuadTwenty", Some(g.m.tick as u64)),
                    Mode::QuadMarathon => ("QuadMarathon", seat.board.as_ref().map(|b| b.score())),
                    Mode::ChainEndless => ("ChainEndless", seat.board.as_ref().map(|b| b.score())),
                    Mode::CapsuleLevelRun => ("CapsuleLevelRun", seat.board.as_ref().map(|b| b.score())),
                    Mode::PanelEndless => ("PanelEndless", seat.board.as_ref().map(|b| b.score())),
                    _ => ("", None),
                };
                if let Some(v) = value.filter(|v| *v > 0) {
                    let r = self.ranks.submit(board, v, &name, &ch);
                    place(board, r, &mut gain);
                }
            }
            Origin::Arcade { stage, .. } => {
                let reached = *stage as u64 + if won { 1 } else { 0 };
                if reached > 0 {
                    let r = self.ranks.submit("Arcade", reached, &name, &ch);
                    place("Arcade", r, &mut gain);
                }
            }
            Origin::Versus(_) if won => {
                let r = self.ranks.bump("VersusWins", &name, &ch, 1, false);
                place("VersusWins", r, &mut gain);
            }
            _ => {}
        }
        let life = self.ranks.lifetime;
        self.ranks.bump("TotalXP", &name, &ch, life, true);
        self.ranks.save();
        gain
    }

    fn results(&mut self, nav: &Nav, mut r: Box<Results>) -> Screen {
        self.bg(0.1);
        let l = look(self.st.theme);
        let g = &r.game;
        let won = g.won();
        let head = match (&g.origin, g.m.outcome.clone()) {
            (Origin::Demo(_), _) => "DEMO OVER".to_string(),
            (Origin::Replay, _) => "REPLAY OVER".to_string(),
            (_, Outcome::Draw) => "DRAW".to_string(),
            _ if g.m.header.mode.is_solo() => {
                if won {
                    "CLEAR!".into()
                } else {
                    "GAME OVER".into()
                }
            }
            (_, Outcome::Winner(k)) => {
                let who: Vec<String> = (0..6).filter(|&i| g.m.seats[i].board.is_some() && purtris_sim::matchs::team_key(i, g.m.seats[i].cfg.team) == k).map(|i| g.names[i].clone()).collect();
                format!("{} WIN{}", who.join(" + "), if who.len() == 1 { "S" } else { "" })
            }
            _ => "MATCH OVER".into(),
        };
        let head = ui::title_case(&head);
        // the result on a big slanted banner: the section color for a win, slate otherwise
        let bc = if won || matches!(g.m.outcome, Outcome::Winner(_)) && !g.m.header.mode.is_solo() { ui::section() } else { 0xFF5A_6386 };
        let right = ui::banner_at(&mut self.cv, &head, bc, 14, 5);
        if won {
            for k in 0..3 {
                let sx = right + 26 + k * 46;
                self.cv.star(sx as f32 + 2.0, 50.0, 19.0, darken(bc, 130), 255);
                self.cv.star(sx as f32, 47.0, 19.0, 0xFFFF_E066, 255);
            }
        }
        if let Some(rec) = &r.new_record {
            self.cv.text_a(41, 101, &ui::nice(rec), 2, 0xFF00_0000, 150);
            self.cv.text_s(40, 100, &ui::nice(rec), 2, l.good);
        }
        // seat table in fixed columns
        ui::panel(&mut self.cv, 40, 130, 700, 300, &l);
        let cols: [(&str, i32, bool); 7] = [("Seat", 84, false), ("Style", 230, false), ("Sent", 400, true), ("Recv", 470, true), ("Chain", 545, true), ("KO", 600, true), ("Score", 712, true)];
        for (name, x, right) in cols {
            if right {
                self.cv.text_right(x, 145, name, 2, l.accent);
            } else {
                self.cv.text_s(x, 145, name, 2, l.accent);
            }
        }
        let mut y = 182;
        for i in 0..6 {
            let s = &g.m.seats[i];
            if let Some(b) = &s.board {
                let c = if g.m.is_winner(i) { l.good } else { l.text };
                // each row leads with that seat's character, cheering or sulking
                if self.st.characters {
                    let member = &crate::cast::CAST[g.chars[i].min(crate::cast::CAST.len() - 1)];
                    let expr = if g.m.is_winner(i) { "delighted" } else { "sad" };
                    self.cv.circle(44.0 + 14.0, y as f32 + 11.0, 16.0, member.color, 255);
                    self.cv.set_clip(40, y - 14, 36, 42);
                    crate::assets::draw_portrait(&mut self.cv, member.id, expr, 38, y - 8, 40, 255, false);
                    self.cv.reset_clip();
                }
                let vals = [g.names[i].chars().take(12).collect::<String>(), ui::title_case(b.style().name()), s.stats.sent.to_string(), s.stats.received.to_string(), s.stats.max_chain.to_string(), s.kos.to_string(), b.score().to_string()];
                for (k, (_, x, right)) in cols.iter().enumerate() {
                    if *right {
                        self.cv.text_right(*x, y, &vals[k], 2, c);
                    } else {
                        self.cv.text_s(*x, y, &vals[k], 2, c);
                    }
                }
                y += 36;
            }
        }
        // the cast reacts: Mira cheers or pouts; in story mode the world's rival does the opposite
        if self.st.characters {
            if let Origin::Story { world, node, .. } = g.origin {
                let rid = crate::content::node_rival(world, node);
                crate::assets::draw_char(&mut self.cv, rid, if won { "sad" } else { "delighted" }, 1010, 668, 220, 255);
            }
        }
        // XP: the bar fills from where you were to where you are now
        if r.gain.xp > 0 {
            use crate::font::Weight;
            let gn = &r.gain;
            let lv = self.ranks.level;
            let need = crate::ranks::xp_to_next(lv);
            let frac = if lv >= crate::ranks::MAX_LEVEL { 1.0 } else { self.ranks.xp as f32 / need as f32 };
            crate::ranks_ui::level_badge(&mut self.cv, 74, 452, 26, lv, self.ranks.prestige);
            self.cv.text_weight(110, 432, &format!("+{} XP", gn.xp), 2, l.accent, 255, Weight::Black);
            let why: Vec<String> = gn.reasons.iter().take(4).map(|(w, n)| format!("{} {}", w, n)).collect();
            self.cv.text_s(230, 438, &why.join("   "), 1, l.dim);
            self.cv.round_rect(110, 462, 610, 10, 5, 0xFF2A_3150, 255);
            self.cv.round_rect(110, 462, ((610.0 * frac) as i32).max(10), 10, 5, l.accent, 255);
            if gn.levels_gained > 0 {
                let msg = format!("Level up! Level {}  {}", lv, crate::ranks::title(lv));
                self.cv.text_weight(110, 474, &msg, 1, l.good, 255, Weight::Bold);
            } else if let Some((b, k)) = gn.placed.first() {
                self.cv.text_weight(110, 474, &format!("#{} on the {} leaderboard", k, b), 1, l.good, 255, Weight::Bold);
            }
        }
        // objectives and analyzer (or match notes)
        ui::panel(&mut self.cv, 770, 130, 470, 540, &l);
        let mut y = 146;
        if r.objectives.is_empty() && r.findings.is_empty() {
            ui::heading(&mut self.cv, 790, y, 430, "About this match");
            y += 40;
            let notes = [
                (if g.m.cross { "Attacks were converted between styles" } else { "Attacks arrived just as each style sends them" }).to_string(),
                format!("Mode: {}", g.m.header.mode.name()),
                format!("Who attacks go to: {}", g.m.header.target.name()),
                format!("Seed: {:08X} (share it to replay the same deal)", g.m.header.seed as u32),
                format!("Length: {}", fmt_time(g.m.tick)),
                "Save the replay to study it in Training, or take it over from any moment.".to_string(),
            ];
            for n in notes.iter() {
                y += self.cv.text_wrap(790, y, 430, n, 2, l.text) + 8;
            }
        }
        if !r.objectives.is_empty() {
            ui::heading(&mut self.cv, 790, y, 430, "Goals");
            y += 38;
            for (t, ok) in &r.objectives {
                self.cv.text_s(790, y, &format!("{} {}", if *ok { "\u{2605}" } else { "\u{25CB}" }, t), 2, if *ok { l.good } else { l.dim });
                y += 24;
            }
            y += 12;
        }
        if !r.findings.is_empty() {
            ui::heading(&mut self.cv, 790, y, 430, "What to work on");
            y += 38;
            for f in r.findings.iter().take(3) {
                self.cv.text_s(790, y, f.severity, 1, if f.severity == "EXCHANGE" { l.warn } else { l.dim });
                y += 14;
                y += self.cv.text_wrap(790, y, 430, &f.text, 1, l.text);
                if let Some(id) = f.lesson {
                    if let Some(le) = LESSONS.iter().find(|x| x.id == id) {
                        self.cv.text_s(790, y, &format!("\u{2192} lesson: {}", le.title), 1, l.accent);
                        y += 12;
                    }
                }
                y += 12;
            }
        }
        let mut rows = vec![Row::act("CONTINUE"), Row::act("PLAY AGAIN"), Row::act(if r.saved.is_some() { "REPLAY SAVED" } else { "SAVE REPLAY" }), Row::act("WATCH REPLAY")];
        let next_stage = matches!(g.origin, Origin::Arcade { stage, .. } if won && stage < 5);
        if next_stage {
            rows.insert(0, Row::act("NEXT RIVAL"));
        }
        let gg = ListGeom { x: 60, y: 512, w: 660, row_h: 38, visible: 5, scale: 2 };
        let ev = list(&mut self.cv, &l, nav, &rows, &mut r.list, &gg, self.t);
        if let Some(p) = &r.saved {
            self.cv.text_s(60, 680, p, 1, l.dim);
        }
        let off = if next_stage { 1 } else { 0 };
        match ev {
            LE::Act(0) if next_stage => {
                if let Origin::Arcade { profile, stage } = r.game.origin {
                    return self.arcade_stage(profile, stage + 1);
                }
            }
            LE::Back => {
                let won = r.game.won();
                return self.after_match(&r.game.origin.clone(), won);
            }
            LE::Act(k) if k == off => {
                let o = r.game.origin.clone();
                let won = r.game.won();
                return self.after_match(&o, won);
            }
            LE::Act(k) if k == off + 1 => {
                let mut h = r.game.rep.header;
                h.seed = crate::now_seed();
                let o = r.game.origin.clone();
                if let Origin::Replay = o {
                    return Screen::Main(ListState::default());
                }
                let dev = r.game.devices;
                let tr = r.game.training;
                let s = self.start(h, dev, o.clone(), tr);
                if let (Screen::Play(mut gm), Origin::Lesson(i)) = (s, o.clone()) {
                    match LESSONS[i].obj {
                        Obj::SurviveTicks(n, _) => gm.stop_at = Some(n),
                        Obj::ClearViruses => gm.stop_on_clear = true,
                        _ => {}
                    }
                    return Screen::Play(gm);
                } else {
                    let h2 = r.game.rep.header;
                    let _ = h2;
                }
                let mut h3 = r.game.rep.header;
                h3.seed = crate::now_seed();
                return self.start(h3, r.game.devices, o, r.game.training);
            }
            LE::Act(k) if k == off + 2 => {
                if r.saved.is_none() {
                    let mut rep = r.game.rep.clone();
                    rep.tas = r.game.tas;
                    let label = g.m.header.mode.name().replace(' ', "-");
                    match crate::save::save_replay(&rep.to_bytes(), &label) {
                        Some(p) => r.saved = Some(format!("{}", p.display())),
                        None => self.say("COULD NOT WRITE THE REPLAY"),
                    }
                }
            }
            LE::Act(k) if k == off + 3 => {
                let rep = r.game.rep.clone();
                if let Ok(gm) = Game::from_replay(rep) {
                    return Screen::Play(Box::new(gm));
                }
            }
            _ => {}
        }
        Screen::Results(r)
    }

    fn controls(&mut self, nav: &Nav, mut s: ListState, mut layout: usize, waiting: Option<usize>) -> Screen {
        self.bg(0.0);
        let l = look(self.st.theme);
        ui::title(&mut self.cv, "CONTROLS", "", &l);
        let names = ["KEYBOARD (ONE PLAYER)", "KEYBOARD LEFT (WASD SIDE)", "KEYBOARD RIGHT (ARROWS SIDE)"];
        self.cv.text_s(90, 120, &format!("\u{2190} {} \u{2192}", names[layout]), 2, l.accent);
        if let Some(a) = waiting {
            ui::panel(&mut self.cv, 340, 280, 600, 140, &l);
            self.cv.text_center(640, 310, &format!("PRESS A KEY FOR: {}", ACTIONS[a].name().to_uppercase()), 2, l.accent);
            self.cv.text_center(640, 350, "ESC cancels", 2, l.dim);
            if self.input.key_pressed(Key::Escape) {
                return Screen::Controls(s, layout, None);
            }
            if let Some(k) = self.input.any_key_pressed() {
                {
                    self.st.bindings[layout][a] = vec![k];
                    self.st.save();
                    self.apply_settings();
                    return Screen::Controls(s, layout, None);
                }
            }
            return Screen::Controls(s, layout, waiting);
        }
        let mut rows: Vec<Row> = ACTIONS
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let keys: Vec<String> = self.st.bindings[layout][i].iter().map(|k| key_name(*k)).collect();
                Row { label: a.name().to_string(), value: keys.join(" / "), kind: ui::RK::Act }
            })
            .collect();
        rows.push(Row::act("BACK"));
        let g = ListGeom { x: 90, y: 160, w: 760, row_h: 36, visible: 12, scale: 2 };
        let ev = list(&mut self.cv, &l, nav, &rows, &mut s, &g, self.t);
        ui::prompts(&mut self.cv, &l, &[("\u{2190}\u{2192}", "Layout"), ("\u{2191}\u{2193}", "Choose"), ("Enter", "Rebind"), ("Del", "Reset"), ("Esc", "Back")]);
        ui::panel(&mut self.cv, 890, 160, 350, 440, &l);
        let pad = [
            "Gamepad (auto-detected):",
            "D-pad      move / cursor",
            "Up         hard drop",
            "A          rotate CCW / swap",
            "B          rotate CW / swap",
            "X, LB      hold / raise",
            "Y          rotate 180",
            "RB         hard drop / raise",
            "RT / LT    target next / prev",
            "Start      pause",
        ];
        for (k, t) in pad.iter().enumerate() {
            let (a, b) = t.split_at(t.find("  ").unwrap_or(t.len()));
            self.cv.text_s(912, 182 + k as i32 * 40, a.trim(), 2, if k == 0 { l.accent } else { 0xFFFF_FFFF });
            if !b.trim().is_empty() {
                self.cv.text_s(1030, 182 + k as i32 * 40, b.trim(), 2, l.text);
            }
        }
        if nav.right || self.input.key_pressed(Key::E) {
            layout = (layout + 1) % 3;
            self.sfx(Sfx::Menu);
        }
        if nav.left || self.input.key_pressed(Key::Q) {
            layout = (layout + 2) % 3;
            self.sfx(Sfx::Menu);
        }
        if self.input.key_pressed(Key::Delete) {
            self.st.bindings[layout] = crate::input::default_bindings()[layout].clone();
            self.st.save();
            self.apply_settings();
        }
        match ev {
            LE::Back | LE::Act(11) => return Screen::Settings(ListState::default(), 4),
            LE::Act(i) => return Screen::Controls(s, layout, Some(i)),
            _ => {}
        }
        let _ = Action::Left;
        Screen::Controls(s, layout, None)
    }

    fn card_screen(&mut self, nav: &Nav, mut s: ListState, editing: Option<usize>) -> Screen {
        self.bg(0.0);
        let l = look(self.st.theme);
        ui::title(&mut self.cv, "PLAYER CARD", "Local only. No account is needed to play.", &l);
        if let Some(f) = editing {
            let typed = self.input.take_typed();
            let field = match f {
                0 => &mut self.card.name,
                1 => &mut self.card.bio,
                _ => &mut self.card.country,
            };
            let max = [12, 160, 24][f.min(2)];
            for c in typed {
                if (c.is_ascii_graphic() || c == ' ') && field.chars().count() < max {
                    field.push(c.to_ascii_uppercase());
                }
            }
            if self.input.key_pressed(Key::Backspace) {
                field.pop();
            }
            if self.input.key_pressed(Key::Enter) || self.input.key_pressed(Key::Escape) {
                if self.card.name.trim().is_empty() {
                    self.card.name = "PLAYER".into();
                }
                self.card.save();
                return Screen::Card(s, None);
            }
        }
        let rows = vec![
            Row::opt("Display name", format!("{}{}", self.card.name, if editing == Some(0) { "_" } else { "" })),
            Row::opt("Bio (160)", if editing == Some(1) { format!("{}_", self.card.bio) } else if self.card.bio.is_empty() { "(empty)".into() } else { "edit".into() }),
            Row::opt("Country (optional)", format!("{}{}", self.card.country, if editing == Some(2) { "_" } else { "" })),
            Row::opt("Favorite style", Style::ALL[self.card.favorite % 4].name()),
            Row::opt("Story courier", format!("{} ({})", crate::cast::hero(), if self.card.hero_boy { "boy" } else { "girl" })),
            Row::act("BACK"),
        ];
        let g = ListGeom { x: 90, y: 160, w: 560, row_h: 44, visible: 6, scale: 2 };
        let nav2 = if editing.is_some() { Nav::default() } else { *nav };
        let ev = list(&mut self.cv, &l, &nav2, &rows, &mut s, &g, self.t);
        ui::prompts(&mut self.cv, &l, &[("\u{2191}\u{2193}", "Choose"), ("\u{2190}\u{2192}", "Change"), ("Enter", "Edit"), ("Esc", "Back")]);
        // the card
        let (cx, cy) = (760, 150);
        let accent = [crate::theme::CYAN, crate::theme::GREEN, crate::theme::PINK, crate::theme::YELLOW][self.card.favorite % 4];
        self.cv.round_rect(cx - 6, cy - 6, 432, 492, 26, accent, 230);
        self.cv.glass(cx, cy, 420, 480, 22, ui::GLASS, 200);
        crate::assets::draw_char(&mut self.cv, "mira", "smile", cx + 210, cy + 250, 230, 255);
        self.cv.text_center(cx + 210, cy + 262, &self.card.name, 4, l.accent);
        self.cv.text_center(cx + 210, cy + 308, &format!("Favorite: {}", ui::title_case(Style::ALL[self.card.favorite % 4].name())), 2, l.text);
        if !self.card.country.is_empty() {
            self.cv.text_center(cx + 210, cy + 336, &self.card.country, 2, l.dim);
        }
        self.cv.text_wrap(cx + 24, cy + 368, 372, &self.card.bio, 1, l.text);
        let stars = self.prog.nodes.values().filter(|v| **v & 7 != 0).count();
        self.cv.text_center(cx + 210, cy + 448, &format!("Story nodes {} / 30     Lessons {}", stars, self.prog.lessons.len()), 1, l.dim);
        match ev {
            LE::Back | LE::Act(5) => return Screen::Main(ListState::default()),
            LE::Act(4) | LE::Chg(4, _) => {
                self.input.take_typed();
                return Screen::NameHero { typed: String::new(), then: Box::new(Screen::Card(s, None)) };
            }
            LE::Chg(3, d) => {
                self.card.favorite = ((self.card.favorite as i32 + d).rem_euclid(4)) as usize;
                self.card.save();
            }
            LE::Chg(i, _) if i < 3 => {
                self.input.take_typed();
                if i == 1 {
                    self.card.bio.clear();
                }
                return Screen::Card(s, Some(i));
            }
            _ => {}
        }
        Screen::Card(s, editing)
    }

    fn help(&mut self, nav: &Nav, mut page: usize) -> Screen {
        self.bg(0.0);
        let l = look(self.st.theme);
        let pages: [(&str, &str); 6] = [
            ("THE IDEA", "Puzzle Commons puts four puzzle sports in one match. Every seat picks its own style. When everyone plays the same style, the original rules and attack units are used (the authentic path). When styles mix, attacks pass through a common gauge and arrive as that player's own garbage (the cross path). The meter at the left of each board shows what is coming: orange is still travelling (you can cancel it), red is in your tray. Last player or team standing wins."),
            ("QUAD", "Pieces of four blocks fall. Move, rotate, and fill whole rows to clear them. Clearing 2+ rows, quads (4 rows), T-spins and back-to-back clears send garbage lines. Hold saves a piece. Up rotates, so the arrow keys alone play the whole game; Space hard drops. Your DAS/ARR/soft-drop speed is set in Settings and never changes the rules. Lose when a piece cannot spawn."),
            ("CHAIN", "Pairs of blobs fall. Four or more of one color touching (not diagonally) pop. Blobs above fall, and if they make a new group, that's a chain: each link is worth far more. Chains send nuisance blobs, and also cancel nuisance in your own tray first. Never let a blob reach the X cell (third column, top row)."),
            ("CAPSULE", "Two-colored capsules fall into a bottle full of germs. Line up four or more of one color, across or down, to clear them. Germs never fall; capsule halves do. Clearing two or more lines at once, or chaining falls, sends loose halves to opponents. In Capsule Race, the first to clear every germ wins. Lose when the bottle neck is blocked."),
            ("PANEL", "Panel de Pon, released in the US as Tetris Attack. The stack rises from the bottom. Move the two-cell cursor and swap the panels under it. Three or more in a line, across or down, clear. Panels that fall into a new match make a chain. Combos and chains pause the rise and send garbage stones. Clear panels beside a stone to crack it into new panels. Lose when the stack is pushed through the top line. Raise the stack yourself with the hold/drop key."),
            ("KEYS", "Keyboard: Left/Right move, Up rotates, Down soft drops: the arrows alone are enough. Z/X rotate either way (Panel: swap), A rotates 180, C holds (Panel: raise), Space hard drops (Panel: raise), Tab/Q change target. Two or three people can share one keyboard with the LEFT (WASD + F G R H) and RIGHT (arrows + , . / L) layouts. Gamepads work in any seat. Esc or Start pauses. The flat Tournament theme is in Settings."),
        ];
        let (h, body) = pages[page];
        ui::title(&mut self.cv, &format!("HOW TO PLAY: {}", h), &format!("Page {} of {}", page + 1, pages.len()), &l);
        ui::prompts(&mut self.cv, &l, &[("\u{2190}\u{2192}", "Turn the page"), ("Esc", "Back")]);
        ui::panel(&mut self.cv, 60, 130, 1160, 520, &l);
        self.cv.text_wrap(90, 160, 1100, body, 3, l.text);
        if nav.right || nav.ok {
            page = (page + 1) % pages.len();
        }
        if nav.left {
            page = (page + pages.len() - 1) % pages.len();
        }
        if nav.back {
            return Screen::Main(ListState::default());
        }
        Screen::Help(page)
    }
}
