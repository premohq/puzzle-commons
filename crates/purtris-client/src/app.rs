//! Screens: boot bench, title, main menu, story map, versus room, solo, training curriculum,
//! lessons, sandbox, calculators, seed browser, replays, results, settings, controls, card, help.

use crate::audio::{Sfx, SharedSynth};
use crate::content::*;
use crate::game::{Game, Origin};
use crate::gfx::{hex, Canvas};
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
}
impl Room {
    pub fn new() -> Self {
        let mut seats = [(SeatChoice::Closed, Profile::QuadPpt, Personality::Builder, 4u8, 0u8); 6];
        seats[0] = (SeatChoice::Human(Device::KbFull), Profile::QuadPpt, Personality::Builder, 4, 0);
        seats[1] = (SeatChoice::Bot, Profile::ChainTsu, Personality::Builder, 4, 0);
        Room { seats, race: false, target: TargetRule::Direct, opts: Opts::default(), seed: None, list: ListState::default() }
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
            return "No seats".into();
        }
        if !self.opts.force_cross && act.iter().all(|&p| p == act[0]) {
            format!("AUTHENTIC PATH: {} (native units)", act[0].id())
        } else {
            "CROSS PATH: attacks convert through the gauge".into()
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
fn seat_choice_name(c: SeatChoice) -> String {
    match c {
        SeatChoice::Closed => "Closed".into(),
        SeatChoice::Human(d) => format!("Human: {}", d.name()),
        SeatChoice::Bot => "Bot".into(),
    }
}
const PLAY_PROFILES: [Profile; 6] = [Profile::QuadPpt, Profile::QuadGuideline, Profile::ChainTsu, Profile::CapsuleVs, Profile::CapsuleNes, Profile::PanelPdp];
fn profile_name(p: Profile) -> String {
    format!("{} ({})", p.style().name(), p.id())
}

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
    Solo(ListState),
    Arcade(ListState, usize),
    Training(ListState),
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
    Quit,
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
    pub xmb_anim: (f32, [f32; 8]),
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
            xmb_anim: (0.0, [0.0; 8]),
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

    fn run(&mut self, scr: Screen, nav: &Nav, dt: f32) -> Screen {
        match scr {
            Screen::Boot { frames, start } => self.boot(frames, start),
            Screen::Title => self.title(nav),
            Screen::Main(s) => self.main_menu(nav, s),
            Screen::Story { world, node, diff, chosen, talk } => self.story(nav, world, node, diff, chosen, talk),
            Screen::Versus(room) => self.versus(nav, room),
            Screen::Solo(s) => self.solo(nav, s),
            Screen::Arcade(s, k) => self.arcade(nav, s, k),
            Screen::Training(s) => self.training(nav, s),
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
        let a = (((t * 2.5).sin() * 0.5 + 0.5) * 155.0 + 100.0) as u32;
        let msg = "Press Enter or Start";
        let pw = Canvas::text_wb(msg, 3, Weight::Bold) + 60;
        self.cv.glass(640 - pw / 2, 580, pw, 52, 26, ui::GLASS, 200);
        self.cv.text_weight(640 - (pw - 60) / 2, 592, msg, 3, l.accent, a, Weight::Bold);
        ui::footer(&mut self.cv, "Enter / Start  play      Esc  quit      Arrows / WASD / gamepad / mouse", &l);
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
                ("books", "Curriculum".to_string(), "Lessons for every style: the idea, why, a demo, a drill with a par.".to_string()),
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
                ("sparkle", "Tour with Mira".to_string(), "Mira shows you around the modes and controls, with pictures.".to_string()),
                ("power", "Quit".to_string(), "See you soon!".to_string()),
            ],
        }
    }

    fn main_menu(&mut self, nav: &Nav, _s: ListState) -> Screen {
        use crate::font::Weight;
        const CATS: [(&str, &str); 8] = [("map-trifold", "Story"), ("sword", "Versus"), ("timer", "Solo"), ("graduation-cap", "Training"), ("film-strip", "Replays"), ("user-circle", "Profile"), ("trophy", "Ranks"), ("gear-six", "Settings")];
        let l = look(self.st.theme);
        let t = self.t;
        self.bg_blur(60);
        // the XMB wave ribbon
        if self.st.theme != ThemeId::Tournament && !self.st.reduce_motion {
            for layer in 0..3 {
                let amp = 26.0 + layer as f32 * 12.0;
                let ph = t * (0.35 + layer as f32 * 0.12) + layer as f32 * 1.7;
                for x in (0..crate::render::W).step_by(2) {
                    let fx = x as f32 / 1280.0 * std::f32::consts::TAU;
                    let y = 300.0 + (fx * 1.1 + ph).sin() * amp + (fx * 2.3 - ph * 0.7).sin() * amp * 0.35;
                    let thick = 22.0 + layer as f32 * 18.0 + (fx + ph).cos() * 8.0;
                    let a = 10 + layer as u32 * 4;
                    self.cv.rect_a(x, (y - thick / 2.0) as i32, 2, thick as i32, 0xFFFF_FFFF, a);
                    self.cv.rect_a(x, y as i32, 2, 1, 0xFFFF_FFFF, 60);
                }
            }
        }
        // input
        let n_cat = CATS.len();
        let cat = self.xmb_cat.min(n_cat - 1);
        let items = self.xmb_items(cat);
        let mut moved = false;
        if nav.left && self.xmb_cat > 0 {
            self.xmb_cat -= 1;
            moved = true;
        }
        if nav.right && self.xmb_cat + 1 < n_cat {
            self.xmb_cat += 1;
            moved = true;
        }
        let it = &mut self.xmb_item[cat];
        if *it >= items.len() {
            *it = 0;
        }
        if nav.up && *it > 0 {
            *it -= 1;
            moved = true;
        }
        if nav.down && *it + 1 < items.len() {
            *it += 1;
            moved = true;
        }
        if nav.wheel != 0 {
            *it = ((*it as i32 + nav.wheel).clamp(0, items.len() as i32 - 1)) as usize;
        }
        if moved {
            self.sfx(Sfx::Menu);
        }
        // ease the animation toward the selection
        let k = 1.0 - (-18.0 * self.dt.clamp(0.0, 0.1)).exp();
        self.xmb_anim.0 += (self.xmb_cat as f32 - self.xmb_anim.0) * k;
        for c in 0..8 {
            let target = self.xmb_item[c] as f32;
            self.xmb_anim.1[c] += (target - self.xmb_anim.1[c]) * k;
        }
        let cat = self.xmb_cat;
        let items = self.xmb_items(cat);
        let sel = self.xmb_item[cat].min(items.len().saturating_sub(1));
        // header
        crate::assets::logo(&mut self.cv, 26, 8, 150);
        let who = format!("{}", self.card.name);
        let ww = Canvas::text_wb(&who, 2, Weight::Bold) + 64;
        self.cv.glass(1130 - ww, 14, ww, 40, 20, ui::GLASS, ui::GLASS_A);
        crate::assets::icon(&mut self.cv, "user-circle", 1130 - ww + 26, 34, 24, l.accent, 255);
        self.cv.text_weight(1130 - ww + 46, 23, &who, 2, 0xFFFF_FFFF, 255, Weight::Bold);
        // category bar
        let bar_y = 196;
        let anchor = 300.0;
        let spacing = 150.0;
        let mut click_cat = None;
        for (i, (ic, label)) in CATS.iter().enumerate() {
            let x = anchor + (i as f32 - self.xmb_anim.0) * spacing;
            if !(-80.0..1360.0).contains(&x) {
                continue;
            }
            let d = (i as f32 - self.xmb_anim.0).abs().min(1.0);
            let size = (92.0 - 34.0 * d) as i32;
            let a = (255.0 - 120.0 * d) as u32;
            if i == cat {
                self.cv.glow(x, bar_y as f32, 70.0, l.accent, 40);
            }
            crate::assets::icon(&mut self.cv, ic, x as i32 + 2, bar_y + 3, size, 0xFF00_0000, a / 3);
            crate::assets::icon(&mut self.cv, ic, x as i32, bar_y, size, 0xFFFF_FFFF, a);
            if i == cat {
                let lw = Canvas::text_wb(label, 2, Weight::Bold);
                self.cv.text_weight(x as i32 - lw / 2 + 1, bar_y + 55, label, 2, 0xFF00_0000, 140, Weight::Bold);
                self.cv.text_weight(x as i32 - lw / 2, bar_y + 54, label, 2, 0xFFFF_FFFF, 255, Weight::Bold);
            }
            if let Some((mx, my)) = nav.mouse {
                if nav.click && (mx - x as i32).abs() < 50 && (my - bar_y).abs() < 50 {
                    click_cat = Some(i);
                }
            }
        }
        // items: the selected one sits just under the bar; earlier ones scroll above it
        let col_x = anchor as i32 - 30;
        let row_h = 64.0;
        let off = self.xmb_anim.1[cat];
        let mut click_item = None;
        for (i, (ic, label, _)) in items.iter().enumerate() {
            let rel = i as f32 - off;
            let y = if rel < -0.01 { bar_y as f32 - 70.0 + rel * row_h * 0.8 } else { bar_y as f32 + 112.0 + rel * row_h };
            if !(-40.0..760.0).contains(&y) {
                continue;
            }
            let is_sel = i == sel;
            let d = rel.abs().min(1.0);
            let a = if rel < -0.01 { 90 } else { (255.0 - 90.0 * d) as u32 };
            let isz = if is_sel { 52 } else { 38 };
            if is_sel {
                let tw = Canvas::text_wb(label, 3, Weight::Bold) + isz + 60;
                self.cv.glass(col_x - 34, y as i32 - 32, tw, 64, 32, ui::GLASS, 120);
            }
            crate::assets::icon(&mut self.cv, ic, col_x, y as i32, isz, if is_sel { l.accent } else { 0xFFFF_FFFF }, a);
            let sc = if is_sel { 3 } else { 2 };
            let ty = y as i32 - (Canvas::px(sc) * 0.4) as i32 - 2;
            let wgt = if is_sel { Weight::Bold } else { Weight::Regular };
            self.cv.text_weight(col_x + 42 + 1, ty + 1, label, sc, 0xFF00_0000, a / 2, wgt);
            self.cv.text_weight(col_x + 42, ty, label, sc, 0xFFFF_FFFF, a, wgt);
            if let Some((mx, my)) = nav.mouse {
                if nav.click && mx > col_x - 40 && mx < col_x + 600 && (my - y as i32).abs() < 28 {
                    click_item = Some(i);
                }
            }
        }
        // description strip
        if let Some((_, _, desc)) = items.get(sel) {
            self.cv.glass(40, 636, 1200, 50, 25, ui::GLASS, ui::GLASS_A);
            self.cv.text_s(70, 650, desc, 2, l.text);
        }
        self.host(1150.0, 560.0, 46.0);
        ui::footer(&mut self.cv, "\u{2190} \u{2192}  category      \u{2191} \u{2193}  item      Enter / A  open      Esc / B  back", &l);
        if let Some(c) = click_cat {
            self.xmb_cat = c;
            return Screen::Main(ListState::default());
        }
        let activate = nav.ok || click_item.map(|i| i == sel).unwrap_or(false);
        if let Some(i) = click_item {
            if i != sel {
                self.xmb_item[cat] = i;
                return Screen::Main(ListState::default());
            }
        }
        if nav.back {
            self.sfx(Sfx::Back);
            return Screen::Title;
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
            SeatKind::Bot => crate::cast::CAST[chars[i]].name.to_string(),
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
        let story_foes: Option<[&str; 4]> = match origin {
            Origin::Story { world, .. } => Some(match *world {
                0 => ["nami", "sora", "taro", "emi"],
                1 => ["hana", "emi", "ren", "sora"],
                2 => ["yuki", "taro", "kai", "ren"],
                3 => ["rin", "ren", "sora", "emi"],
                _ => ["kaede", "kai", "nami", "rin"],
            }),
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
                        options.push(f[foe_k.min(3)]);
                        foe_k += 1;
                    }
                    options.push(style_champ);
                    options.extend(fans);
                    options.extend(["kai", "sora", "emi", "ren", "taro", "nami", "hana", "yuki", "rin", "kaede"]);
                    options.iter().map(|n| index(n)).find(|c| !used.contains(c)).unwrap_or(0)
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
        let head = format!("World {}  \u{00B7}  {}", world + 1, w.name);
        let hw = Canvas::text_wb(&head, 3, Weight::Black).max(Canvas::text_w("Q / E  change world", 1)) + 56;
        self.cv.glass(24, 18, hw, 74, 24, ui::GLASS, ui::GLASS_A);
        self.cv.text_weight(50, 30, &head.replace("\u{00B7}", "-"), 3, 0xFFFF_FFFF, 255, Weight::Black);
        self.cv.text_s(50, 68, &format!("Learn {}    Q / E  change world", w.style.to_lowercase()), 1, l.dim);
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
        let (rid, rname) = crate::assets::rival(world);
        if self.st.characters {
            crate::light::set(crate::light::world(world.min(4), crate::light::Mood::Day));
            let expr = if n.level.boss { "angry" } else { "smug" };
            crate::assets::draw_char(&mut self.cv, rid, expr, 1150, 690, 340, 255);
            let tag = format!("Rival: {}", rname);
            let tw = Canvas::text_wb(&tag, 1, Weight::Bold) + 24;
            self.cv.glass(1150 - tw / 2, 664, tw, 24, 12, ui::GLASS, 210);
            self.cv.text_weight(1150 - tw / 2 + 12, 669, &tag, 1, 0xFFFF_FFFF, 255, Weight::Bold);
        }
        let kw = Canvas::text_w(&kind, 2);
        self.cv.text_s(1040 - kw, 554, &kind, 2, l.accent);
        if talk || !self.st.decorations {
            self.cv.text_wrap(70, 590, 960, n.line, 2, l.text);
        }
        let human_style = n.level.human.map(|p| p.style().name().to_string()).unwrap_or_else(|| format!("{}   (Tab to change)", style_profile_for(chosen).style().name()));
        self.cv.text_s(70, 650, &format!("You: {}", human_style), 2, l.text);
        self.cv.text_s(470, 650, &format!("Difficulty: {}", DIFF_NAMES[diff]), 2, l.accent);
        self.cv.text_s(740, 650, &format!("\u{2605}  {}", n.extra.text(diff)), 2, l.dim);
        let hint = if open { "Enter  play      \u{2190} \u{2192}  node      \u{2191} \u{2193}  difficulty      Esc  back" } else { "Locked: clear the previous node first" };
        self.cv.text_s(70, 676, hint, 1, if open { l.dim } else { l.warn });
        if nav.left && node > 0 {
            node -= 1;
            talk = true;
            self.sfx(Sfx::Menu);
        }
        if nav.right && node < 5 {
            node += 1;
            talk = true;
            self.sfx(Sfx::Menu);
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
        const BOSS: [&str; 5] = ["w0_boss", "w1_boss", "w2_boss", "w3_boss", "w4_boss"];
        const MID: [&str; 5] = ["w0_mid", "w1_mid", "w2_mid", "w3_mid", "w4_mid"];
        if node == 5 && !self.scene_seen(BOSS[world.min(4)]) {
            return self.play_scene(BOSS[world.min(4)], play);
        }
        // halfway through each world the story takes a quieter turn
        if node == 2 && !self.scene_seen(MID[world.min(4)]) {
            return self.play_scene(MID[world.min(4)], play);
        }
        play
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
        back
    }

    // -------------------------------------------------------------- versus room
    fn versus(&mut self, nav: &Nav, mut room: Box<Room>) -> Screen {
        self.bg(0.1);
        let l = look(self.st.theme);
        ui::title(&mut self.cv, "VERSUS", "Up to six seats on one machine. Each seat picks its own style.", &l);
        #[derive(Clone, Copy)]
        enum R {
            Start,
            Mode,
            Target,
            Teams,
            Seat(usize),
            Prof(usize),
            Bot(usize),
            Str(usize),
            Team(usize),
            Shared,
            Cross,
            Virus,
            QLevel,
            CColors,
            KLevel,
            KSpeed,
            Nes,
            PLevel,
            PColors,
            Seed,
        }
        let mut rows = vec![Row::act("START MATCH")];
        let mut ids = vec![R::Start];
        let push = |r: Row, id: R, rows: &mut Vec<Row>, ids: &mut Vec<R>| {
            rows.push(r);
            ids.push(id);
        };
        push(Row::opt("Mode", if room.race { "Capsule Race" } else { "Versus (KO)" }), R::Mode, &mut rows, &mut ids);
        push(Row::opt("Targeting", room.target.name()), R::Target, &mut rows, &mut ids);
        push(Row::opt("Team preset", "FFA / 2v2 / 3v3 / 2v2v2"), R::Teams, &mut rows, &mut ids);
        for i in 0..6 {
            let (c, p, per, s, team) = room.seats[i];
            push(Row::head(format!("SEAT {}", i + 1)), R::Start, &mut rows, &mut ids);
            push(Row::opt(format!("  Seat {}", i + 1), seat_choice_name(c)), R::Seat(i), &mut rows, &mut ids);
            if c != SeatChoice::Closed {
                if !room.race {
                    push(Row::opt("  Style", profile_name(p)), R::Prof(i), &mut rows, &mut ids);
                }
                if c == SeatChoice::Bot {
                    push(Row::opt("  Bot personality", per.name()), R::Bot(i), &mut rows, &mut ids);
                    push(Row::opt("  Bot strength", format!("{} / 8", s)), R::Str(i), &mut rows, &mut ids);
                }
                push(Row::opt("  Team", if team == 0 { "none".to_string() } else { format!("{}", team) }), R::Team(i), &mut rows, &mut ids);
            }
        }
        push(Row::head("RULES"), R::Start, &mut rows, &mut ids);
        let o = room.opts;
        let yn = |b: bool| if b { "ON" } else { "OFF" };
        push(Row::opt("Same deal for everyone", yn(o.shared_deal)), R::Shared, &mut rows, &mut ids);
        push(Row::opt("Force cross path", yn(o.force_cross)), R::Cross, &mut rows, &mut ids);
        push(Row::opt("Virus clear also wins", yn(o.virus_clear_wins)), R::Virus, &mut rows, &mut ids);
        push(Row::opt("Quad start level", format!("{}", o.quad_level)), R::QLevel, &mut rows, &mut ids);
        push(Row::opt("Chain colors", format!("{}", o.chain_colors)), R::CColors, &mut rows, &mut ids);
        push(Row::opt("Capsule level", format!("{}", o.capsule_level)), R::KLevel, &mut rows, &mut ids);
        push(Row::opt("Capsule speed", purtris_sim::capsule::SPEED_NAMES[o.capsule_speed as usize]), R::KSpeed, &mut rows, &mut ids);
        push(Row::opt("Capsule NES-style seeds", yn(o.nes_seeds)), R::Nes, &mut rows, &mut ids);
        push(Row::opt("Panel level", format!("{}", o.panel_level)), R::PLevel, &mut rows, &mut ids);
        push(Row::opt("Panel colors", format!("{}", o.panel_colors)), R::PColors, &mut rows, &mut ids);
        push(Row::opt("Match seed", room.seed.map(|s| format!("{:08X}", s)).unwrap_or_else(|| "random".into())), R::Seed, &mut rows, &mut ids);
        let g = ListGeom { x: 60, y: 120, w: 640, row_h: 30, visible: 18, scale: 2 };
        let ev = list(&mut self.cv, &l, nav, &rows, &mut room.list, &g, self.t);
        // side info
        ui::panel(&mut self.cv, 760, 120, 460, 540, &l);
        self.cv.text_s(780, 140, "ATTACK PATH", 2, l.accent);
        self.cv.text_wrap(780, 170, 420, &room.path_text(), 2, l.text);
        let h = room.header(1);
        let cross = h.opts.force_cross || {
            let act: Vec<Profile> = h.seats.iter().filter(|s| s.kind != SeatKind::Closed).map(|s| s.profile).collect();
            act.iter().any(|&p| Some(p) != act.first().copied())
        };
        let mut y = 240;
        self.cv.text_s(780, y, "PROVISIONAL NUMBERS IN THIS ROOM", 1, l.warn);
        y += 16;
        let styles: Vec<Style> = h.seats.iter().filter(|s| s.kind != SeatKind::Closed).map(|s| s.profile.style()).collect();
        for (k, txt) in PROVISIONAL {
            let rel = (k.starts_with("quad") && styles.contains(&Style::Quad))
                || (k.starts_with("chain") && styles.contains(&Style::Chain) && (cross || !k.ends_with("cross")))
                || (k.starts_with("capsule") && styles.contains(&Style::Capsule))
                || (k.starts_with("panel") && styles.contains(&Style::Panel))
                || (k.starts_with("cross") && cross && styles.contains(&Style::Panel));
            if rel && y < 600 {
                y += 4 + self.cv.text_wrap(780, y, 420, &format!("- {}", txt), 1, l.dim);
            }
        }
        self.cv.text_wrap(780, 610, 420, "Keyboard left = WASD side, right = arrows side, so two or three people can share one keyboard. Gamepads: 4 max.", 1, l.dim);
        ui::footer(&mut self.cv, "\u{2190}\u{2192} change   Enter: start / change   Esc: back", &l);
        match ev {
            LE::Moved => self.sfx(Sfx::Menu),
            LE::Back => return Screen::Main(ListState::default()),
            LE::Act(k) => {
                if let R::Start = ids[k] {
                    let seed = room.seed.unwrap_or_else(crate::now_seed);
                    let h = room.header(seed);
                    let humans = h.seats.iter().filter(|s| s.kind == SeatKind::Human).count();
                    let active = h.seats.iter().filter(|s| s.kind != SeatKind::Closed).count();
                    if active < 1 {
                        self.say("OPEN AT LEAST ONE SEAT");
                    } else if humans == 0 {
                        self.say("ADD A HUMAN SEAT (OR WATCH: SET ONE TO BOT AND START FROM TRAINING)");
                    } else {
                        let mut devs = [None; 6];
                        for i in 0..6 {
                            if let SeatChoice::Human(d) = room.seats[i].0 {
                                devs[i] = Some(d);
                            }
                        }
                        let r2 = room.clone();
                        self.sfx(Sfx::Select);
                        let back = Screen::Versus(r2.clone());
                        return self.select_for(h, devs, Origin::Versus(r2), back);
                    }
                }
            }
            LE::Chg(k, d) => {
                self.sfx(Sfx::Menu);
                let o = &mut room.opts;
                match ids[k] {
                    R::Mode => room.race = !room.race,
                    R::Target => room.target = cycle(&TargetRule::ALL, room.target, d),
                    R::Teams => {
                        let act: Vec<usize> = (0..6).filter(|&i| room.seats[i].0 != SeatChoice::Closed).collect();
                        if act.is_empty() {
                            self.say("Open a seat first");
                            return Screen::Versus(room);
                        }
                        let cur = room.seats[act[0]].4;
                        let mode = if cur == 0 { 1 } else if act.len() >= 6 && room.seats[act[2]].4 == 1 { 2 } else if act.len() >= 6 && room.seats[act[2]].4 == 2 { 3 } else { 0 };
                        let next = (mode + 1) % 4;
                        for (n, &i) in act.iter().enumerate() {
                            room.seats[i].4 = match next {
                                0 => 0,
                                1 => 1 + (n as u8 % 2),
                                2 => 1 + (n as u8 / 3),
                                _ => 1 + (n as u8 / 2),
                            };
                        }
                    }
                    R::Seat(i) => room.seats[i].0 = cycle(&SEAT_DEVICES, room.seats[i].0, d),
                    R::Prof(i) => room.seats[i].1 = cycle(&PLAY_PROFILES, room.seats[i].1, d),
                    R::Bot(i) => room.seats[i].2 = cycle(&Personality::ALL, room.seats[i].2, d),
                    R::Str(i) => room.seats[i].3 = ((room.seats[i].3 as i32 - 1 + d).rem_euclid(8) + 1) as u8,
                    R::Team(i) => room.seats[i].4 = ((room.seats[i].4 as i32 + d).rem_euclid(4)) as u8,
                    R::Shared => o.shared_deal = !o.shared_deal,
                    R::Cross => o.force_cross = !o.force_cross,
                    R::Virus => o.virus_clear_wins = !o.virus_clear_wins,
                    R::QLevel => o.quad_level = ((o.quad_level as i32 - 1 + d).rem_euclid(15) + 1) as u32,
                    R::CColors => o.chain_colors = ((o.chain_colors as i32 - 3 + d).rem_euclid(3) + 3) as u8,
                    R::KLevel => o.capsule_level = ((o.capsule_level as i32 + d).rem_euclid(21)) as u32,
                    R::KSpeed => o.capsule_speed = ((o.capsule_speed as i32 + d).rem_euclid(3)) as u8,
                    R::Nes => o.nes_seeds = !o.nes_seeds,
                    R::PLevel => o.panel_level = ((o.panel_level as i32 - 1 + d).rem_euclid(10) + 1) as u32,
                    R::PColors => o.panel_colors = ((o.panel_colors as i32 - 4 + d).rem_euclid(3) + 4) as u8,
                    R::Seed => room.seed = if room.seed.is_some() { None } else { Some(crate::now_seed() & 0xFFFF_FFFF) },
                    R::Start => {}
                }
            }
            _ => {}
        }
        Screen::Versus(room)
    }

    // -------------------------------------------------------------- solo
    fn solo(&mut self, nav: &Nav, mut s: ListState) -> Screen {
        self.bg(0.1);
        let l = look(self.st.theme);
        ui::title(&mut self.cv, "SOLO", "Objective modes. Records are kept on this machine.", &l);
        let modes: [(Mode, Profile, &str); 9] = [
            (Mode::QuadSprint, Profile::QuadGuideline, "QUAD SPRINT: clear 40 lines"),
            (Mode::QuadTwenty, Profile::QuadGuideline, "QUAD TWENTY: clear 20 lines"),
            (Mode::QuadMarathon, Profile::QuadGuideline, "QUAD MARATHON: until you top out"),
            (Mode::ChainEndless, Profile::ChainTsu, "CHAIN ENDLESS"),
            (Mode::CapsuleLevelRun, Profile::CapsuleNes, "CAPSULE LEVEL RUN"),
            (Mode::PanelEndless, Profile::PanelPdp, "PANEL ENDLESS RISE"),
            (Mode::PanelPuzzle(0), Profile::PanelPdp, "PANEL PUZZLES"),
            (Mode::Versus, Profile::QuadPpt, "ARCADE LADDER (VS BOTS)"),
            (Mode::Training, Profile::QuadPpt, "BACK"),
        ];
        let mut rows: Vec<Row> = Vec::new();
        for (m, p, name) in modes.iter() {
            let rec = self.prog.records.get(&format!("{:?}", m)).copied();
            let v = match (m, rec) {
                (Mode::QuadSprint | Mode::QuadTwenty, Some(t)) => format!("best {}", fmt_time(t as u32)),
                (_, Some(x)) if !matches!(m, Mode::PanelPuzzle(_) | Mode::Versus | Mode::Training) => format!("best {}", x),
                _ => String::new(),
            };
            let _ = p;
            rows.push(Row { label: name.to_string(), value: v, kind: ui::RK::Act });
        }
        let g = ListGeom { x: 90, y: 150, w: 700, row_h: 44, visible: 10, scale: 2 };
        let ev = list(&mut self.cv, &l, nav, &rows, &mut s, &g, self.t);
        self.host(1060.0, 560.0, 62.0);
        match ev {
            LE::Moved => self.sfx(Sfx::Menu),
            LE::Back => return Screen::Main(ListState::default()),
            LE::Act(i) => {
                if i == 8 {
                    return Screen::Main(ListState::default());
                }
                if i == 7 {
                    return Screen::Arcade(ListState::default(), 0);
                }
                if let Mode::PanelPuzzle(_) = modes[i].0 {
                    return Screen::Sandbox(ListState::default(), 99, 0, 0);
                }
                let (m, p, _) = modes[i];
                let h = quick_header(crate::now_seed(), m, &[(SeatKind::Human, p, Personality::Builder, 1)]);
                let mut dev = [None; 6];
                dev[0] = Some(self.default_dev());
                return self.start(h, dev, Origin::Solo(m, p), false);
            }
            _ => {}
        }
        Screen::Solo(s)
    }

    fn arcade(&mut self, nav: &Nav, mut s: ListState, _k: usize) -> Screen {
        self.bg(0.1);
        let l = look(self.st.theme);
        ui::title(&mut self.cv, "ARCADE LADDER", "Six rivals, rising strength, every style. Pick yours.", &l);
        let rows: Vec<Row> = Style::ALL.iter().map(|st| Row::act(format!("PLAY AS {}", st.name()))).chain(std::iter::once(Row::act("BACK"))).collect();
        let g = ListGeom { x: 90, y: 160, w: 600, row_h: 50, visible: 6, scale: 3 };
        let ev = list(&mut self.cv, &l, nav, &rows, &mut s, &g, self.t);
        match ev {
            LE::Back => return Screen::Solo(ListState::default()),
            LE::Act(4) => return Screen::Solo(ListState::default()),
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

    // -------------------------------------------------------------- training
    fn training(&mut self, nav: &Nav, mut s: ListState) -> Screen {
        self.bg(0.05);
        let l = look(self.st.theme);
        ui::title(&mut self.cv, "TRAINING", "The curriculum leads. The sandbox and tools sit beside it.", &l);
        let rows = vec![
            Row::act("CURRICULUM"),
            Row::act("SANDBOX (REWIND, SAVE STATES, GARBAGE)"),
            Row::act("PANEL PUZZLES"),
            Row::act("CALCULATORS"),
            Row::act("SEED BROWSER"),
            Row::act("BACK"),
        ];
        let g = ListGeom { x: 90, y: 160, w: 700, row_h: 50, visible: 6, scale: 3 };
        let ev = list(&mut self.cv, &l, nav, &rows, &mut s, &g, self.t);
        // the one habit line
        let done = self.prog.lessons.len();
        ui::panel(&mut self.cv, 840, 160, 380, 200, &l);
        self.cv.text_s(860, 180, "PROGRESS", 2, l.accent);
        self.cv.text_s(860, 210, &format!("{} / {} lessons passed", done, LESSONS.len()), 2, l.text);
        let next = LESSONS.iter().find(|x| !self.prog.lessons.contains_key(x.id));
        if let Some(n) = next {
            self.cv.text_wrap(860, 250, 340, &format!("Recommended next: {} ({})", n.title, n.track), 2, l.dim);
        }
        self.cv.text_wrap(860, 320, 340, "Lessons never gate anything.", 1, l.dim);
        match ev {
            LE::Moved => self.sfx(Sfx::Menu),
            LE::Back | LE::Act(5) => return Screen::Main(ListState::default()),
            LE::Act(0) => return Screen::Curriculum(ListState::default(), 0),
            LE::Act(1) => return Screen::Sandbox(ListState::default(), 0, 0, 4),
            LE::Act(2) => return Screen::Sandbox(ListState::default(), 99, 0, 0),
            LE::Act(3) => return Screen::Calc(ListState::default(), 3, 4, 70, 0),
            LE::Act(4) => return Screen::Seeds(ListState::default(), 0x1234_5678, String::new()),
            _ => {}
        }
        Screen::Training(s)
    }

    fn curriculum(&mut self, nav: &Nav, mut s: ListState, mut track: usize) -> Screen {
        self.bg(0.05);
        let l = look(self.st.theme);
        ui::title(&mut self.cv, "CURRICULUM", "Q/E or LB/RB: change track", &l);
        // track tabs
        for (i, t) in TRACKS.iter().enumerate() {
            let x = 90 + i as i32 * 190;
            let on = i == track;
            self.cv.round_rect(x, 112, 180, 34, 17, if on { l.accent } else { 0xFF0A_1124 }, if on { 235 } else { 170 });
            self.cv.text_center(x + 90, 120, &ui::title_case(t), 2, if on { hex(0x101020) } else { l.text });
        }
        if self.input.key_pressed(Key::E) || (0..4).any(|i| self.input.pad_pressed(i, crate::input::pad::RB)) {
            track = (track + 1) % TRACKS.len();
            s = ListState::default();
        }
        if self.input.key_pressed(Key::Q) || (0..4).any(|i| self.input.pad_pressed(i, crate::input::pad::LB)) {
            track = (track + TRACKS.len() - 1) % TRACKS.len();
            s = ListState::default();
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
        if let Some((mx, my)) = nav.mouse {
            if nav.click && my > 118 && my < 148 {
                let k = ((mx - 90) / 190) as usize;
                if k < TRACKS.len() {
                    track = k;
                }
            }
        }
        let g = ListGeom { x: 90, y: 170, w: 760, row_h: 40, visible: 12, scale: 2 };
        let ev = list(&mut self.cv, &l, nav, &rows, &mut s, &g, self.t);
        if track == 1 {
            self.cv.text_wrap(900, 180, 330, "The Quad NES track (hypertap and rolling on the console repeat) waits on the quad.nes profile and its timing file. The tap lesson ships on Capsule, whose repeat lives in the sim.", 1, l.dim);
        }
        match ev {
            LE::Moved => self.sfx(Sfx::Menu),
            LE::Back => return Screen::Training(ListState::default()),
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
        self.cv.text_wrap(392, 186, 820, body, 2, l.text);
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
        let rows = vec![Row::act("Watch Mira's demo"), Row::act("Start the drill"), Row::act("Back to the curriculum")];
        let g = ListGeom { x: 420, y: 470, w: 520, row_h: 52, visible: 3, scale: 2 };
        let ev = list(&mut self.cv, &l, nav, &rows, &mut s, &g, self.t);
        ui::prompts(&mut self.cv, &l, &[("\u{2190} \u{2192}", "Mira's notes"), ("\u{2191} \u{2193}", "Choose"), ("Enter", "Select"), ("Esc", "Back")]);
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
                LE::Back => return Screen::Training(ListState::default()),
                LE::Act(i) if i == n => return Screen::Training(ListState::default()),
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
            LE::Back | LE::Act(4) => return Screen::Training(ListState::default()),
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
            LE::Back | LE::Act(4) => return Screen::Training(ListState::default()),
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
            ui::panel(&mut self.cv, 40, 268, 600, 420, &l);
            ui::panel(&mut self.cv, 680, 268, 560, 420, &l);
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
                            self.cv.circle(70.0 + xx as f32 * 12.0, (y + 20 + yy * 12) as f32, 5.0, crate::theme::capsule_color(c.color[i]), 255);
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
            LE::Back | LE::Act(2) => return Screen::Training(ListState::default()),
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
                let nw = Canvas::text_wb(member.name, 2, crate::font::Weight::Black) + 36;
                self.cv.round_grad(cx - nw / 2, 640, nw, 36, 18, crate::gfx::lighten(member.color, 40), member.color, 255);
                self.cv.text_weight(cx - nw / 2 + 18, 646, member.name, 2, 0xFF16_1028, 255, crate::font::Weight::Black);
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
            let w = Canvas::text_wb(&txt, sc, Weight::Black);
            let (x, y) = (640 - w / 2, 300 - (Canvas::px(sc) * 0.4) as i32);
            for (dx, dy) in [(-5, 0), (5, 0), (0, -5), (0, 5), (-4, -4), (4, 4), (-4, 4), (4, -4), (0, 8)] {
                self.cv.text_weight(x + dx, y + dy, &txt, sc, 0xFF14_1030, a * 9 / 10, Weight::Black);
            }
            self.cv.text_weight(x, y, &txt, sc, if go { l.good } else { l.accent }, a, Weight::Black);
        }
        if let Some((t, _)) = &g.toast {
            self.cv.text_center(640, 40, t, 2, l.accent);
        }
        if g.paused {
            self.cv.rect_a(0, 0, 1280, 720, 0xFF00_0000, 140);
            let rows = vec![Row::act("RESUME"), Row::act("RESTART"), Row::act(format!("Theme: {}", self.st.theme.name())), Row::act("QUIT TO MENU")];
            let mut ls = ListState { sel: g.pause_sel, scroll: 0 };
            ui::panel(&mut self.cv, 440, 220, 400, 280, &l);
            self.cv.text_center(640, 236, "PAUSED", 3, l.accent);
            let gg = ListGeom { x: 470, y: 290, w: 340, row_h: 46, visible: 4, scale: 2 };
            let nav2 = Nav { back: false, ..*nav };
            let ev = list(&mut self.cv, &l, &nav2, &rows, &mut ls, &gg, self.t);
            g.pause_sel = ls.sel;
            if g.training {
                self.cv.text_center(640, 470, "O: STEP ONE TICK   BACKSPACE: REWIND", 1, l.dim);
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
            Origin::Solo(..) => Screen::Solo(ListState::default()),
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
        let hw = Canvas::text_wb(&head, 6, crate::font::Weight::Black);
        self.cv.text_weight(640 - hw / 2 + 3, 32 + 3, &head, 6, 0xFF00_0000, 110, crate::font::Weight::Black);
        self.cv.text_weight(640 - hw / 2, 32, &head, 6, if won { l.accent } else { l.text }, 255, crate::font::Weight::Black);
        if let Some(rec) = &r.new_record {
            self.cv.text_center(640, 100, &ui::nice(rec), 2, l.good);
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
            if let Origin::Story { world, .. } = g.origin {
                let (rid, _) = crate::assets::rival(world);
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
            self.cv.text_s(790, y, "MATCH NOTES", 2, l.accent);
            y += 30;
            let notes = [
                format!("Attack path: {}", if g.m.cross { "cross (gauge)" } else { "authentic (native units)" }),
                format!("Mode: {}", g.m.header.mode.name()),
                format!("Targeting: {}", g.m.header.target.name()),
                format!("Seed: {:08X}", g.m.header.seed as u32),
                format!("Length: {}", fmt_time(g.m.tick)),
                "Save the replay to study it in Training, or take it over from any tick.".to_string(),
            ];
            for n in notes.iter() {
                y += self.cv.text_wrap(790, y, 430, n, 2, l.text) + 8;
            }
        }
        if !r.objectives.is_empty() {
            self.cv.text_s(790, y, "OBJECTIVES", 2, l.accent);
            y += 26;
            for (t, ok) in &r.objectives {
                self.cv.text_s(790, y, &format!("{} {}", if *ok { "\u{2605}" } else { "\u{25CB}" }, t), 2, if *ok { l.good } else { l.dim });
                y += 24;
            }
            y += 12;
        }
        if !r.findings.is_empty() {
            self.cv.text_s(790, y, "WHAT TO WORK ON", 2, l.accent);
            y += 26;
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
        ui::title(&mut self.cv, "CONTROLS", "Q/E: change keyboard layout. Enter: rebind. Delete: reset layout.", &l);
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
        if self.input.key_pressed(Key::E) {
            layout = (layout + 1) % 3;
        }
        if self.input.key_pressed(Key::Q) {
            layout = (layout + 2) % 3;
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
            Row::act("BACK"),
        ];
        let g = ListGeom { x: 90, y: 160, w: 560, row_h: 44, visible: 5, scale: 2 };
        let nav2 = if editing.is_some() { Nav::default() } else { *nav };
        let ev = list(&mut self.cv, &l, &nav2, &rows, &mut s, &g, self.t);
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
            LE::Back | LE::Act(4) => return Screen::Main(ListState::default()),
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
        ui::title(&mut self.cv, &format!("HOW TO PLAY: {}", h), &format!("page {} / {}   \u{2190}\u{2192} turn   Esc back", page + 1, pages.len()), &l);
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
