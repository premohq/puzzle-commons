//! Story scenes, staged: the world behind (graded for the time of day, drifting slowly, with
//! its own motes in the air), the cast lit by that world's light (light.rs) and standing in
//! for the people present, a title card in the logo's letter tiles, letterbox bars, a dialogue
//! box with a typewriter that breathes at punctuation and a voice blip per character, and a
//! backlog, auto and skip. The script itself lives in cast.rs; nothing here changes it.

use crate::app::{App, Screen};
use crate::audio::Sfx;
use crate::cast::{self, Scene};
use crate::font::Weight;
use crate::gfx::Canvas;
use crate::input::{pad, Nav};
use crate::light::{self, Mood, Rig};
use minifb::Key;

/// Seconds of title card before the first line.
const INTRO: f32 = 2.3;
/// Seconds of fade at the end.
const OUTRO: f32 = 0.5;
/// Letterbox bar height (layout units).
const BAR: i32 = 38;
/// The dialogue box.
const BOX_X: i32 = 168;
const BOX_Y: i32 = 516;
const BOX_W: i32 = 944;
const BOX_H: i32 = 146;
const TEXT_SCALE: i32 = 3;
const LINE_H: i32 = 40;
const INK: u32 = 0xFF3B_2A57;

/// One person on stage.
pub struct Actor {
    who: &'static str,
    right: bool,
    x: f32,
    /// 0 = gone, 1 = fully on stage
    here: f32,
    /// 0 = in the background (dimmed), 1 = speaking (lit, forward)
    lit: f32,
    expr: &'static str,
    prev_expr: &'static str,
    /// scene time of the last expression change
    expr_t: f32,
}

pub struct SceneView {
    pub id: &'static str,
    pub then: Box<Screen>,
    line: usize,
    /// time on this line, and in the scene
    lt: f32,
    st: f32,
    actors: Vec<Actor>,
    auto: bool,
    /// time the current line has been fully shown (auto advance, skip pacing)
    shown_t: f32,
    /// backlog open: how many entries it is scrolled up
    log: Option<usize>,
    /// the box hidden to look at the scene
    hide: bool,
    /// fading out (seconds since it began)
    ending: Option<f32>,
    /// characters revealed when the last voice blip played
    blipped: usize,
}

impl SceneView {
    pub fn new(id: &'static str, then: Screen) -> Self {
        SceneView { id, then: Box::new(then), line: 0, lt: 0.0, st: 0.0, actors: Vec::new(), auto: false, shown_t: 0.0, log: None, hide: false, ending: None, blipped: 0 }
    }
}

/// For screenshots and tests: jump to a line and moment.
impl SceneView {
    pub fn seek(&mut self, line: usize, lt: f32, st: f32) {
        if self.line != line {
            self.blipped = 0;
        }
        self.line = line;
        self.lt = lt;
        self.st = st;
    }
    pub fn open_log(&mut self) {
        self.log = Some(0);
    }
}

// ------------------------------------------------------------------ the scene's look

/// A speaker's name, color and title. Ids outside the cast are bodiless voices (a talking box,
/// an intercom): a plain gray plate with their id as the name, and nobody on stage.
fn speaker(who: &str) -> (String, u32, &'static str) {
    match cast::CAST.iter().find(|m| m.id == who) {
        Some(m) => (m.name.to_string(), m.color, m.title),
        None => {
            let mut c = who.chars();
            let name = c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default();
            (name, 0xFFB8_C0D8, "")
        }
    }
}

/// When each scene takes place, from its id (the script decides what happens, this only
/// decides the light).
pub fn mood(id: &str, world: usize) -> Mood {
    match id {
        "prologue" => Mood::Dusk,
        "epilogue" => Mood::Golden,
        "w4_fall" => Mood::Night,
        _ if id.ends_with("_intro") => Mood::Day,
        _ if id.ends_with("_mid") => {
            if world == 1 || world == 3 {
                Mood::Dusk
            } else {
                Mood::Night
            }
        }
        _ if id.ends_with("_boss") => Mood::Duel,
        _ if id.ends_with("_clear") => Mood::Golden,
        _ => Mood::Day,
    }
}

/// The title card: (big tiled word, the line on the pill under it).
fn card(sc: &Scene) -> (String, String) {
    let wname = crate::content::WORLDS.get(sc.world).map(|w| w.name).unwrap_or("The Commons");
    let chapter = format!("Chapter {}", sc.world + 1);
    match sc.id {
        "prologue" => ("Prologue".into(), wname.into()),
        "epilogue" => ("Epilogue".into(), "The Commons".into()),
        "w4_fall" => (wname.into(), format!("{}  -  The Fall", chapter)),
        id => {
            let part = if id.ends_with("_intro") {
                "Arrival"
            } else if id.ends_with("_mid") {
                "Interlude"
            } else if id.ends_with("_boss") {
                "Showdown"
            } else if id.ends_with("_clear") {
                "Aftermath"
            } else {
                ""
            };
            (wname.into(), if part.is_empty() { chapter } else { format!("{}  -  {}", chapter, part) })
        }
    }
}

/// The same light, turned down and cooled for whoever is not speaking.
fn dim(r: &Rig) -> Rig {
    Rig {
        key: [r.key[0] * 0.50, r.key[1] * 0.52, r.key[2] * 0.60],
        sky: [r.sky[0] * 0.55, r.sky[1] * 0.58, r.sky[2] * 0.68],
        ground: [r.ground[0] * 0.5, r.ground[1] * 0.5, r.ground[2] * 0.55],
        rim: [r.rim[0] * 0.35, r.rim[1] * 0.35, r.rim[2] * 0.4],
        ..*r
    }
}

/// A voice for each cast member: pitch from who they are, so lines sound like someone.
fn voice_hz(who: &str) -> f32 {
    let low = ["kai", "haru", "taro", "ren", "toma", "jun"];
    let h = who.bytes().fold(7u32, |a, b| a.wrapping_mul(31).wrapping_add(b as u32));
    let base = if low.contains(&who) { 300.0 } else { 470.0 };
    base * 2f32.powf((h % 7) as f32 / 12.0)
}

/// Seconds the typewriter spends on each character (pauses at punctuation).
fn char_time(c: char, next: Option<char>) -> f32 {
    let base = 1.0 / 50.0;
    let end = next.map(|n| n == ' ' || n == '"').unwrap_or(false);
    match c {
        '.' | '!' | '?' if end => base + 0.24,
        '.' => base + 0.07,
        ',' | ';' | ':' if end => base + 0.11,
        '\u{2014}' => base + 0.12,
        _ => base,
    }
}

// ------------------------------------------------------------------ the backdrop

struct Backdrop {
    key: (usize, Mood, i32),
    img: Canvas,
}
thread_local! {
    static BACKDROP: std::cell::RefCell<Option<Backdrop>> = const { std::cell::RefCell::new(None) };
}
/// How much bigger than the screen the backdrop is (room to drift).
const OVER: f32 = 1.14;

fn build_backdrop(map: &Canvas, world: usize, mood: Mood, cv: &Canvas) -> Canvas {
    // scaled up a little past the screen (bilinear: soft enough to sit behind the cast)
    let (bw, bh) = ((cv.w as f32 * OVER) as i32, (cv.h as f32 * OVER) as i32);
    let mut b = crate::assets::resample(map, bw, bh);
    b.k = cv.k;
    let (tint, amt, glow) = light::grade(world, mood);
    let (tr, tg, tb) = crate::gfx::chans(tint);
    let (gr, gg, gb) = crate::gfx::chans(glow);
    let amt = amt as f32 / 255.0;
    let top_glow = if mood == Mood::Night { 0.14 } else { 0.18 };
    let (w, h) = (bw as usize, bh as usize);
    // one pass per pixel: grade toward the mood (multiply), sky glow from the top, vignette
    let fy: Vec<(f32, f32)> = (0..h)
        .map(|y| {
            let v = y as f32 / h as f32;
            let glow = (1.0 - v / 0.45).max(0.0).powi(2) * top_glow;
            let floor = 1.0 - ((v - 0.55) / 0.45).max(0.0).powf(1.3) * 0.62;
            (glow, floor)
        })
        .collect();
    let fx: Vec<f32> = (0..w)
        .map(|x| {
            let u = (x as f32 / w as f32 - 0.5).abs() * 2.0;
            1.0 - (u - 0.45).max(0.0).powf(1.6) * 0.75
        })
        .collect();
    crate::gfx::par_rows(&mut b.px, w, 0, bh, w * h * 2, |y, row| {
        let (glow, floor) = fy[y as usize];
        for (x, p) in row.iter_mut().enumerate() {
            let (r, g, bb) = crate::gfx::chans(*p);
            let vig = fx[x] * floor;
            let ch = |c: u32, t: u32, gl: u32| -> u32 {
                let c = c as f32;
                let graded = c * (1.0 - amt) + c * (t as f32 / 255.0) * amt;
                (graded * vig + gl as f32 * glow).min(255.0) as u32
            };
            *p = 0xFF00_0000 | (ch(r, tr, gr) << 16) | (ch(g, tg, gg) << 8) | ch(bb, tb, gb);
        }
    });
    b
}

/// Copy the backdrop's window at (ox, oy) pixels onto the screen.
fn draw_backdrop(cv: &mut Canvas, b: &Canvas, ox: i32, oy: i32) {
    let w = cv.w as usize;
    let ox = ox.clamp(0, b.w - cv.w) as usize;
    let oy = oy.clamp(0, b.h - cv.h) as usize;
    for y in 0..cv.h as usize {
        let s = (y + oy) * b.w as usize + ox;
        cv.px[y * w..(y + 1) * w].copy_from_slice(&b.px[s..s + w]);
    }
}

/// Motes in the air, one kind per world: sea glints, pollen, moon dust, sand, embers.
fn motes(cv: &mut Canvas, world: usize, mood: Mood, t: f32, still: bool) {
    let (n, col, rise, sway, size) = match world {
        0 => (22, 0xFFDF_F6FF, 9.0, 14.0, 2.2),
        1 => (26, 0xFFFF_E9A8, -7.0, 22.0, 2.4),
        2 => (30, 0xFFB8_D0FF, 5.0, 8.0, 1.8),
        3 => (20, 0xFFFF_D79A, 2.0, 30.0, 2.0),
        _ => (34, 0xFFFF_8A3A, 34.0, 10.0, 2.2),
    };
    let col = if mood == Mood::Night && world != 4 { 0xFFC8_D8FF } else { col };
    let t = if still { 0.0 } else { t };
    for i in 0..n {
        let f = i as f32;
        let hx = ((f * 97.13).sin() * 43758.55).fract().abs();
        let hy = ((f * 13.71).sin() * 24634.63).fract().abs();
        let sp = 0.6 + ((f * 7.31).sin() * 0.5 + 0.5) * 0.8;
        let x = (hx * 1380.0 + (t * 0.4 * sp + f).sin() * sway + t * 4.0 * sp).rem_euclid(1380.0) - 50.0;
        let y = (hy * 820.0 - t * rise * sp).rem_euclid(820.0) - 50.0;
        let tw = ((t * (1.3 + sp) + f * 2.1).sin() * 0.5 + 0.5).powf(1.5);
        let r = size * (0.7 + sp * 0.5);
        cv.glow(x, y, r * 4.5, col, (tw * 46.0) as u32);
        cv.circle(x, y, r * 0.55, col, (90.0 + tw * 150.0) as u32);
    }
}

// ------------------------------------------------------------------ drawing helpers

fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// A slanted plate (parallelogram): a rectangle with a thin triangle at each end (cheap; big
/// triangles are rasterized over their whole bounding box).
fn slant(cv: &mut Canvas, x: i32, y: i32, w: i32, h: i32, lean: i32, c: u32, a: u32) {
    cv.rect_a(x + lean, y, w - lean, h, c, a);
    let (xf, yf, wf, hf, l) = (x as f32, y as f32, w as f32, h as f32, lean as f32);
    cv.tri([(xf + l, yf), (xf + l, yf + hf), (xf, yf + hf)], c, a);
    cv.tri([(xf + wf, yf), (xf + wf + l, yf), (xf + wf, yf + hf)], c, a);
}

/// The cream pill of the logo ("Commons"), with ink text: used under the title card's tiles.
fn pill(cv: &mut Canvas, cx: i32, y: i32, text: &str, alpha: u32) {
    let tw = Canvas::text_wb(text, 2, Weight::Black);
    let w = tw + 64;
    let x = cx - w / 2;
    cv.round_rect(x - 4, y + 4, w + 8, 52, 26, 0xFF24_1838, alpha * 3 / 4);
    cv.round_rect(x - 4, y - 4, w + 8, 52, 26, INK, alpha);
    cv.round_rect(x, y, w, 44, 22, 0xFFFF_F6D6, alpha);
    cv.round_rect(x + 22, y + 6, w - 44, 5, 3, 0xFFFF_FFFF, alpha * 4 / 5);
    cv.text_weight(x + 32, y + 9, text, 2, INK, alpha, Weight::Black);
}

/// A keycap and its action, on the letterbox bar. Returns the width used.
fn hint(cv: &mut Canvas, x: i32, y: i32, key: &str, action: &str, on: bool, accent: u32) -> i32 {
    let kw = Canvas::text_wb(key, 1, Weight::Bold) + 14;
    cv.round_rect(x, y, kw, 19, 5, if on { accent } else { 0xFFDD_E2F0 }, if on { 255 } else { 220 });
    cv.text_weight(x + 7, y + 1, key, 1, 0xFF14_1830, 255, Weight::Bold);
    cv.text_a(x + kw + 7, y + 1, action, 1, if on { accent } else { 0xFFB8_C0D8 }, 255);
    kw + 7 + Canvas::text_w(action, 1)
}

impl App {
    /// Play scene `id` (once), then go to `then`.
    pub(crate) fn play_scene(&mut self, id: &'static str, then: Screen) -> Screen {
        if cast::scene(id).is_none() {
            return then;
        }
        self.mark_scene(id);
        Screen::Scene(Box::new(SceneView::new(id, then)))
    }

    pub(crate) fn scene_screen(&mut self, nav: &Nav, mut v: Box<SceneView>) -> Screen {
        let Some(sc) = cast::scene(v.id) else { return *v.then };
        let dt = self.dt;
        let still = self.st.reduce_motion;
        v.st += dt;
        v.lt += dt;
        let world = sc.world.min(4);
        let mood = mood(v.id, world);
        let rig = light::world(world, mood);
        let n = sc.lines.len();
        let line = v.line.min(n - 1);
        let cur = &sc.lines[line];
        let accent = speaker(cur.who).1;
        let talking = v.st >= INTRO - 0.25;

        // ---------------------------------------------------------- input
        let pads_held = |m: u32| (0..4).any(|i| self.input.pad_now[i] & m != 0);
        let pads_hit = |m: u32| (0..4).any(|i| self.input.pad_pressed(i, m));
        let skip_held = self.input.key_held(Key::LeftCtrl) || self.input.key_held(Key::RightCtrl) || self.input.key_held(Key::Tab) || pads_held(pad::RB | pad::RT);
        let log_key = self.input.key_pressed(Key::L) || pads_hit(pad::Y);
        let auto_key = self.input.key_pressed(Key::F) || pads_hit(pad::X);
        let hide_key = self.input.key_pressed(Key::H) || pads_hit(pad::LB);
        let full = cur.text.chars().count();
        // the typewriter: characters whose time has come
        let chars: Vec<char> = cur.text.chars().collect();
        let reveal_t = (v.lt - 0.12).max(0.0);
        let mut shown = 0usize;
        let mut acc = 0.0;
        let mut frac = 0.0f32;
        for i in 0..full {
            let c = char_time(chars[i], chars.get(i + 1).copied());
            if acc + c <= reveal_t {
                acc += c;
                shown = i + 1;
            } else {
                frac = ((reveal_t - acc) / (1.0 / 50.0)).clamp(0.0, 1.0);
                break;
            }
        }
        let done = shown >= full;
        if done {
            v.shown_t += dt;
        } else {
            v.shown_t = 0.0;
        }
        let mut advance = false;
        if v.ending.is_none() {
            if let Some(off) = v.log {
                // the backlog takes the input while open
                let max = line;
                if nav.up || nav.wheel > 0 {
                    v.log = Some((off + 1).min(max));
                } else if nav.down || nav.wheel < 0 {
                    if off == 0 {
                        v.log = None;
                    } else {
                        v.log = Some(off - 1);
                    }
                }
                if nav.back || log_key || nav.ok || nav.click {
                    v.log = None;
                    self.sfx(Sfx::Back);
                }
            } else if v.hide {
                if nav.ok || nav.click || nav.back || hide_key {
                    v.hide = false;
                }
            } else if nav.back {
                self.sfx(Sfx::Back);
                v.ending = Some(0.0);
            } else if !talking {
                if nav.ok || nav.click {
                    v.st = INTRO - 0.25; // skip the title card
                }
            } else {
                if log_key || nav.up || nav.wheel > 0 {
                    v.log = Some(0);
                    self.sfx(Sfx::Menu);
                }
                if auto_key {
                    v.auto = !v.auto;
                    self.sfx(Sfx::Menu);
                }
                if hide_key {
                    v.hide = true;
                }
                if nav.ok || nav.click || nav.right {
                    advance = true;
                }
                if skip_held && (!done || v.shown_t > 0.06) {
                    advance = true;
                    v.lt = 100.0;
                }
                let wait = 1.1 + full as f32 * 0.018;
                if v.auto && done && v.shown_t > wait {
                    advance = true;
                }
            }
        }
        if advance {
            if !done {
                v.lt = 100.0; // finish the line at once
            } else if line + 1 < n {
                v.line = line + 1;
                v.lt = 0.0;
                v.shown_t = 0.0;
                v.blipped = 0;
                if !skip_held && !v.auto {
                    self.sfx(Sfx::Menu);
                }
            } else {
                self.sfx(Sfx::Select);
                v.ending = Some(0.0);
            }
        }
        // voice blips while the text runs (not when skipping)
        if talking && !done && !skip_held && v.log.is_none() && shown >= v.blipped + 3 {
            v.blipped = shown;
            let c = chars[shown.saturating_sub(1)];
            if c.is_alphanumeric() {
                let wob = [1.0, 1.06, 0.95, 1.12, 1.0, 0.9][shown / 3 % 6];
                self.sfx(Sfx::Voice((voice_hz(cur.who) * wob) as u32));
            }
        }
        let line = v.line.min(n - 1);
        let cur = &sc.lines[line];

        // ---------------------------------------------------------- the stage
        // who stands where: each side shows its three most recent voices, in order of arrival
        let on_right = |w: &str| sc.right.contains(&w);
        let said = &sc.lines[..=line];
        let mut sides: [Vec<&'static str>; 2] = [Vec::new(), Vec::new()];
        for (si, side) in sides.iter_mut().enumerate() {
            let mut recent: Vec<&'static str> = Vec::new();
            for l in said.iter().rev() {
                if on_right(l.who) == (si == 1) && !recent.contains(&l.who) && crate::assets::has_art(l.who) {
                    recent.push(l.who);
                }
            }
            recent.truncate(3);
            let mut order: Vec<&'static str> = Vec::new();
            for l in said.iter() {
                if recent.contains(&l.who) && !order.contains(&l.who) {
                    order.push(l.who);
                }
            }
            *side = order;
        }
        let k_pos = 1.0 - (-dt * 9.0).exp();
        let k_lit = 1.0 - (-dt * 11.0).exp();
        let appear = talking && v.ending.is_none();
        for (si, side) in sides.iter().enumerate() {
            let cnt = side.len();
            for (i, &who) in side.iter().enumerate() {
                let slots: &[f32] = match cnt {
                    1 => &[300.0],
                    2 => &[232.0, 452.0],
                    _ => &[176.0, 350.0, 520.0],
                };
                let x = slots[i.min(slots.len() - 1)];
                let tx = if si == 1 { 1280.0 - x } else { x };
                // their expression is the one from their latest line
                let expr = said.iter().rev().find(|l| l.who == who).map(|l| l.expr).unwrap_or("normal");
                let st = v.st;
                match v.actors.iter_mut().find(|a| a.who == who) {
                    Some(a) => {
                        if a.expr != expr {
                            a.prev_expr = a.expr;
                            a.expr = expr;
                            a.expr_t = st;
                        }
                        a.right = si == 1;
                        if appear {
                            a.x += (tx - a.x) * if still { 1.0 } else { k_pos };
                        }
                    }
                    None if appear => v.actors.push(Actor { who, right: si == 1, x: if still { tx } else if si == 1 { tx + 260.0 } else { tx - 260.0 }, here: 0.0, lit: 0.0, expr, prev_expr: expr, expr_t: -10.0 }),
                    None => {}
                }
            }
        }
        for a in v.actors.iter_mut() {
            let staying = appear && sides[a.right as usize].contains(&a.who);
            let target = if staying { 1.0 } else { 0.0 };
            a.here += (target - a.here) * if still { 1.0 } else { 1.0 - (-dt * 8.0).exp() };
            if !staying && !still && v.ending.is_none() {
                a.x += if a.right { 140.0 } else { -140.0 } * dt * 2.0;
            }
            let speaking = a.who == cur.who;
            a.lit += (if speaking { 1.0 } else { 0.0 } - a.lit) * k_lit;
        }
        v.actors.retain(|a| a.here > 0.01 || (appear && sides[a.right as usize].contains(&a.who)));

        // ---------------------------------------------------------- backdrop
        let key = (world, mood, self.cv.w);
        let need = BACKDROP.with(|b| b.borrow().as_ref().map(|b| b.key != key).unwrap_or(true));
        if need {
            let img = if world < self.maps.len() { build_backdrop(&self.maps[world].image, world, mood, &self.cv) } else { Canvas::screen(self.cv.w) };
            BACKDROP.with(|b| *b.borrow_mut() = Some(Backdrop { key, img }));
        }
        let pan = if still { 0.5 } else { 0.5 + 0.42 * (1.0 - ease_out(v.st / 3.2)) + 0.3 * (v.st * 0.045).sin() * ease_out((v.st - 1.0) / 6.0) };
        BACKDROP.with(|b| {
            let b = b.borrow();
            let b = &b.as_ref().unwrap().img;
            let ox = ((b.w - self.cv.w) as f32 * pan.clamp(0.0, 1.0)) as i32;
            let oy = ((b.h - self.cv.h) as f32 * (0.55 + 0.25 * (v.st * 0.03).cos())) as i32;
            draw_backdrop(&mut self.cv, b, ox, oy);
        });
        motes(&mut self.cv, world, mood, v.st, still);

        // ---------------------------------------------------------- the cast
        let dim_rig = dim(&rig);
        let fade_all = v.ending.map(|e| 1.0 - (e / OUTRO).min(1.0)).unwrap_or(1.0);
        let mut order: Vec<usize> = (0..v.actors.len()).collect();
        order.sort_by(|&a, &b| v.actors[a].lit.partial_cmp(&v.actors[b].lit).unwrap_or(std::cmp::Ordering::Equal));
        for &i in &order {
            let a = &v.actors[i];
            let crowd = sides[a.right as usize].len() >= 3;
            let h = if crowd { 1080 } else { 1170 };
            let lift = a.lit * 12.0;
            let inward = a.lit * 16.0 * if a.right { -1.0 } else { 1.0 };
            let hop = if still { 0.0 } else { (((v.st - a.expr_t) / 0.32).clamp(0.0, 1.0) * std::f32::consts::PI).sin() * 14.0 };
            let breathe = if still { 0.0 } else { (self.t * 1.9 + i as f32).sin() * 2.2 * a.lit };
            let cx = (a.x + inward) as i32;
            let bottom = (1296.0 - lift - hop + breathe + (1.0 - a.lit) * 10.0) as i32;
            let al = (a.here * fade_all * 255.0) as u32;
            if al == 0 {
                continue;
            }
            // expression changes cross-fade quickly; speaking / listening cross-fades the light
            let xf = ((v.st - a.expr_t) / 0.14).clamp(0.0, 1.0);
            let mut layer = |rig: &Rig, alpha: u32| {
                if alpha == 0 {
                    return;
                }
                light::set(*rig);
                if xf < 1.0 && a.prev_expr != a.expr {
                    crate::assets::draw_char_ex(&mut self.cv, a.who, a.prev_expr, cx, bottom, h, alpha, a.right);
                    crate::assets::draw_char_ex(&mut self.cv, a.who, a.expr, cx, bottom, h, (alpha as f32 * xf) as u32, a.right);
                } else {
                    crate::assets::draw_char_ex(&mut self.cv, a.who, a.expr, cx, bottom, h, alpha, a.right);
                }
            };
            if a.lit > 0.97 {
                layer(&rig, al);
            } else if a.lit < 0.03 {
                layer(&dim_rig, al);
            } else {
                layer(&dim_rig, al);
                layer(&rig, (al as f32 * a.lit) as u32);
            }
        }
        // get the next speaker's light ready a frame ahead (lighting a pose takes a moment)
        if let Some(next) = sc.lines.get(line + 1) {
            if v.actors.iter().any(|a| a.who == next.who) {
                light::set(rig);
                let a = v.actors.iter().find(|a| a.who == next.who).unwrap();
                let crowd = sides[a.right as usize].len() >= 3;
                crate::assets::prewarm_char(&self.cv, next.who, next.expr, if crowd { 1080 } else { 1170 }, a.right);
            }
        }
        light::set(Rig::STUDIO);

        // ---------------------------------------------------------- letterbox
        let bars = if still { 1.0 } else { ease_out((v.st - 0.05) / 0.6) };
        let bh = (BAR as f32 * bars) as i32;
        self.cv.rect(0, 0, 1280, bh, 0xFF00_0000);
        self.cv.rect(0, 720 - bh, 1280, bh, 0xFF00_0000);
        // a hairline of the speaker's color along the inside of the bars
        if talking && bh > 0 {
            self.cv.rect_a(0, bh, 1280, 1, accent, 70);
            self.cv.rect_a(0, 720 - bh - 1, 1280, 1, accent, 70);
        }

        // ---------------------------------------------------------- the dialogue box
        let box_in = if still { if talking { 1.0 } else { 0.0 } } else { ease_out((v.st - (INTRO - 0.25)) / 0.45) };
        let box_a = (box_in * fade_all * if v.hide { 0.0 } else { 1.0 } * 255.0) as u32;
        if box_a > 0 && v.log.is_none() {
            self.dialogue(&v, sc, line, shown, frac, done, box_in, box_a);
        }
        // the corner of the top bar: where we are
        if talking && v.log.is_none() {
            let (big, small) = card(sc);
            let where_ = format!("{}   -   {}", small, if sc.id == "prologue" || sc.id == "epilogue" { big.as_str() } else { "" });
            let where_ = where_.trim_end_matches([' ', '-']).to_string();
            self.cv.text_a(24, 12, &where_, 1, 0xFFB8_C0D8, (box_in * fade_all * 200.0) as u32);
            // progress through the scene
            let pw = 120;
            let px = 1280 - 24 - pw;
            self.cv.round_rect(px, 17, pw, 4, 2, 0xFF2A_3050, (box_in * 255.0) as u32);
            let f = ((line + 1) as f32 / n as f32 * pw as f32) as i32;
            self.cv.round_rect(px, 17, f.max(4), 4, 2, accent, (box_in * fade_all * 255.0) as u32);
        }
        // hints along the bottom bar
        if talking && bh >= BAR - 2 && v.log.is_none() && !v.hide {
            let mut x = 24;
            let y = 720 - BAR + 10;
            x += hint(&mut self.cv, x, y, "Enter", "Next", false, accent) + 22;
            x += hint(&mut self.cv, x, y, "F", if v.auto { "Auto on" } else { "Auto" }, v.auto, accent) + 22;
            x += hint(&mut self.cv, x, y, "Ctrl", "Skip", skip_held, accent) + 22;
            x += hint(&mut self.cv, x, y, "L", "Log", false, accent) + 22;
            x += hint(&mut self.cv, x, y, "H", "Hide", false, accent) + 22;
            let _ = hint(&mut self.cv, x, y, "Esc", "End scene", false, accent);
        }

        // ---------------------------------------------------------- the title card
        if !talking || v.st < INTRO + 0.4 {
            let (big, small) = card(sc);
            let out = if still { if v.st > INTRO - 0.25 { 1.0 } else { 0.0 } } else { ((v.st - (INTRO - 0.45)) / 0.45).clamp(0.0, 1.0) };
            let a = ((1.0 - out) * 255.0) as u32;
            if a > 0 {
                let size = 70;
                let w = crate::blocks::width(&big, size);
                let lift = (out * 26.0) as i32;
                let y = 232 - lift;
                // a soft dark band behind the card
                for i in 0..12 {
                    let ba = ((1.0 - (i as f32 / 12.0)) * 26.0 * (1.0 - out)) as u32;
                    self.cv.rect_a(0, y - 30 + i * 4, 1280, 196 - i * 8, 0xFF04_0610, ba);
                }
                crate::blocks::draw(&mut self.cv, &big, 640 - w / 2, y, size, world + 1, if still { None } else { Some(v.st - 0.3) }, a);
                let pa = (ease_out((v.st - 0.75) / 0.4) * (1.0 - out) * 255.0) as u32;
                let py = y + size + 26 + ((1.0 - ease_out((v.st - 0.75) / 0.4)) * 14.0) as i32;
                if pa > 0 {
                    pill(&mut self.cv, 640, py, &small, pa);
                }
            }
        }

        // ---------------------------------------------------------- the backlog
        if let Some(off) = v.log {
            self.backlog(sc, line, off);
        }

        // ---------------------------------------------------------- fades
        if !still && v.st < 0.6 {
            self.cv.rect_a(0, 0, 1280, 720, 0xFF00_0000, ((1.0 - v.st / 0.6) * 255.0) as u32);
        }
        if let Some(e) = &mut v.ending {
            *e += dt;
            let t = (*e / OUTRO).min(1.0);
            self.cv.rect_a(0, 0, 1280, 720, 0xFF00_0000, (t * 255.0) as u32);
            if t >= 1.0 || still {
                self.fade_in = 0.45;
                return *v.then;
            }
        }
        Screen::Scene(v)
    }

    #[allow(clippy::too_many_arguments)]
    fn dialogue(&mut self, v: &SceneView, sc: &Scene, line: usize, shown: usize, frac: f32, done: bool, box_in: f32, a: u32) {
        let cur = &sc.lines[line];
        let (name, col, title) = speaker(cur.who);
        let right = sc.right.contains(&cur.who);
        let rise = ((1.0 - box_in) * 40.0) as i32;
        let (bx, by, bw, bh) = (BOX_X, BOX_Y + rise, BOX_W, BOX_H);
        // the box: deep ink glass with a lit edge and the speaker's color along the top
        self.cv.soft_shadow(bx, by, bw, bh, 20, 14, a * 150 / 255);
        self.cv.round_grad(bx, by, bw, bh, 20, 0xFF1B_2040, 0xFF0B_0E20, a * 236 / 255);
        self.cv.round_ring(bx, by, bw, bh, 20, 2, crate::gfx::mix(col, 0xFFFF_FFFF, 90), 0xFF2A_3058, a * 150 / 255);
        for i in 0..6 {
            let ga = (a as f32 * (1.0 - i as f32 / 6.0) * 0.16) as u32;
            self.cv.rect_a(bx + 18, by + 2 + i * 3, bw - 36, 3, col, ga);
        }
        // the name plate: slanted, in the speaker's color, on their side of the box
        let nw = Canvas::text_wb(&name, 3, Weight::Black) + 52;
        let nx = if right { bx + bw - nw - 44 } else { bx + 36 };
        let ny = by - 26;
        slant(&mut self.cv, nx + 4, ny + 5, nw, 44, 12, 0xFF05_0610, a * 120 / 255);
        slant(&mut self.cv, nx - 3, ny - 3, nw + 6, 50, 12, INK, a);
        slant(&mut self.cv, nx, ny, nw, 44, 12, col, a);
        slant(&mut self.cv, nx, ny + 30, nw, 14, 4, crate::gfx::darken(col, 40), a * 120 / 255);
        self.cv.rect_a(nx + 18, ny + 6, nw - 36, 3, 0xFFFF_FFFF, a * 150 / 255);
        self.cv.text_weight(nx + 26, ny + 6, &name, 3, INK, a, Weight::Black);
        // first words of the scene: who they are, beside the plate
        let first = sc.lines[..line].iter().all(|l| l.who != cur.who);
        if first && !title.is_empty() {
            let ta = (a as f32 * ease_out(v.lt / 0.4)) as u32;
            let tw = Canvas::text_wb(title, 1, Weight::Bold);
            let tx = if right { nx - tw - 26 } else { nx + nw + 28 };
            self.cv.round_rect(tx - 12, ny + 10, tw + 24, 24, 12, 0xFF0A_0D1E, ta * 200 / 255);
            self.cv.text_weight(tx, ny + 13, title, 1, crate::gfx::lighten(col, 60), ta, Weight::Bold);
        }
        // the words, laid out in full first so nothing jumps between lines while it types
        let lines = crate::gfx::wrap(cur.text, bw - 92, TEXT_SCALE);
        let ty = by + 34;
        let mut idx = 0usize;
        for (k, ln) in lines.iter().enumerate().take(3) {
            let len = ln.chars().count();
            let y = ty + k as i32 * LINE_H;
            let x = bx + 46;
            if shown >= idx + len {
                self.cv.text_weight(x, y, ln, TEXT_SCALE, 0xFFF4_F6FF, a, Weight::Regular);
            } else if shown > idx || (shown == idx && frac > 0.0) {
                let part: String = ln.chars().take(shown - idx).collect();
                self.cv.text_weight(x, y, &part, TEXT_SCALE, 0xFFF4_F6FF, a, Weight::Regular);
                // the next letter fades in
                if let Some(c) = ln.chars().nth(shown - idx) {
                    let px = x + Canvas::text_wb(&part, TEXT_SCALE, Weight::Regular);
                    let mut s = [0u8; 4];
                    self.cv.text_weight(px, y, c.encode_utf8(&mut s), TEXT_SCALE, 0xFFF4_F6FF, (a as f32 * frac) as u32, Weight::Regular);
                }
            }
            // the wrap ate the space between lines
            idx += len + 1;
        }
        // waiting for you: a bobbing arrow, or a stop square on the last line
        if done {
            let still = self.st.reduce_motion;
            let bob = if still { 0.0 } else { (self.t * 5.0).sin() * 3.0 };
            let (ax, ay) = ((bx + bw - 34) as f32, (by + bh - 26) as f32 + bob);
            if line + 1 < sc.lines.len() {
                self.cv.glow(ax, ay, 16.0, col, a * 50 / 255);
                self.cv.tri([(ax - 10.0, ay - 6.0), (ax + 10.0, ay - 6.0), (ax, ay + 8.0)], col, a);
                self.cv.tri([(ax - 6.0, ay - 4.0), (ax + 6.0, ay - 4.0), (ax, ay + 4.0)], crate::gfx::lighten(col, 80), a);
            } else {
                self.cv.round_rect(ax as i32 - 7, ay as i32 - 7, 14, 14, 3, col, a);
            }
        }
        if v.auto {
            let pulse = if self.st.reduce_motion { 1.0 } else { 0.7 + 0.3 * (self.t * 4.0).sin() };
            let w = Canvas::text_wb("AUTO", 1, Weight::Black) + 20;
            let x = if right { bx + 24 } else { bx + bw - w - 24 };
            self.cv.round_rect(x, by + 10, w, 20, 10, col, (a as f32 * pulse) as u32);
            self.cv.text_weight(x + 10, by + 12, "AUTO", 1, INK, a, Weight::Black);
        }
    }

    /// Everything said so far, newest at the bottom, with faces.
    fn backlog(&mut self, sc: &Scene, line: usize, off: usize) {
        self.cv.rect_a(0, 0, 1280, 720, 0xFF04_0610, 215);
        crate::blocks::draw(&mut self.cv, "LOG", 210, 54, 46, 4, None, 255);
        self.cv.text_a(330, 70, "\u{2191} \u{2193}  scroll      L / Esc  close", 1, 0xFFB8_C0D8, 255);
        let (x0, w) = (210, 860);
        self.cv.set_clip(0, 118, 1280, 560);
        let mut y = 664;
        let end = line - off.min(line);
        for i in (0..=end).rev() {
            let l = &sc.lines[i];
            let (name, color, _) = speaker(l.who);
            let rows = crate::gfx::wrap(l.text, w - 110, 2);
            let h = (rows.len() as i32 * 28 + 36).max(78);
            y -= h;
            if y < 120 {
                break;
            }
            let now = i == line;
            self.cv.round_rect(x0, y, w, h - 10, 14, if now { 0xFF1C_2244 } else { 0xFF10_1428 }, 230);
            self.cv.rect_a(x0, y + 10, 4, h - 30, color, 255);
            // the speaker's face, as they looked when they said it
            self.cv.set_clip(x0 + 14, (y + 8).max(118), 60, (60 - (118 - (y + 8)).max(0)).max(0));
            crate::assets::draw_portrait(&mut self.cv, l.who, l.expr, x0 + 4, y + 2, 80, 255, false);
            self.cv.set_clip(0, 118, 1280, 560);
            self.cv.round_ring(x0 + 12, y + 6, 64, 64, 12, 2, color, crate::gfx::darken(color, 60), 255);
            self.cv.text_weight(x0 + 92, y + 8, &name, 2, color, 255, Weight::Black);
            for (k, r) in rows.iter().enumerate() {
                self.cv.text_a(x0 + 92, y + 34 + k as i32 * 28, r, 2, if now { 0xFFF4_F6FF } else { 0xFFC8_CEE0 }, 255);
            }
        }
        self.cv.reset_clip();
    }
}
