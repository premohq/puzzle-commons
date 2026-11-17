//! The puppet layer: every cast still (assets::draw_char_ex / draw_portrait) is drawn through
//! here as a soft puppet: one still image, deformed per row as it is blitted. Characters with
//! 3D motion clips in a match are drawn by anim.rs instead; this layer moves everyone else,
//! everywhere (story scenes, menus, select, results).
//!
//! - Idle: breathing (the chest and shoulders rise, the feet stay planted), a slow sway that
//!   grows toward the head, and a per-mood flavour (bouncy when delighted, a tremble when angry,
//!   a droop when sad, a nod when sleepy, a shiver when shocked, a swagger when smug).
//! - Every character keeps its own clock and phase, so a crowd never breathes in step.
//! - Expression changes crossfade and play a reaction picked from the new mood: a hop with
//!   anticipation and landing squash, a stomp, a recoil, a slump, a springy pop.
//! - Entrances rise and settle; moves between positions are smoothed.
//! - Talking: `talk(id)` nods the head with the syllables while a line is being typed.
//! - Explicit one-shots from the match: `react(id, React::Pump / Flinch / Cheer)`.
//!
//! Characters are tracked without any screen keeping state: each draw is matched to the
//! instance of the same character drawn closest to it last frame. Reduce motion keeps only the
//! crossfades and smoothing. Screenshot modes call `set_still(true)` for the rest pose.

use crate::gfx::Canvas;
use std::cell::RefCell;
use std::f32::consts::TAU;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum React {
    /// a hop: crouch, jump, land
    Hop,
    /// two hops, arms already up (wins)
    Cheer,
    /// a quick stamp down and back up
    Stomp,
    /// jolt up and back, a shiver
    Recoil,
    /// sinks and settles
    Slump,
    /// a springy squash-and-stretch pop
    Pop,
    /// a small settle (back to calm)
    Settle,
    /// a power-up stretch (a big clear or attack)
    Pump,
    /// knocked sideways and squashed (garbage landed)
    Flinch,
}

#[derive(Clone, Copy, PartialEq)]
enum Mood {
    Calm,
    Joy,
    Angry,
    Sad,
    Sleepy,
    Shock,
    Smug,
    Pose,
}

fn mood(expr: &str) -> Mood {
    match expr {
        "delighted" => Mood::Joy,
        "angry" => Mood::Angry,
        "sad" => Mood::Sad,
        "sleepy" => Mood::Sleepy,
        "shocked" => Mood::Shock,
        "smug" => Mood::Smug,
        e if e.starts_with("p-") => Mood::Pose,
        _ => Mood::Calm,
    }
}

/// How the still image is deformed this frame. Lengths are fractions of the image height.
#[derive(Clone, Copy, Debug)]
pub struct Pose {
    /// whole body sideways
    pub dx: f32,
    /// whole body up
    pub lift: f32,
    /// scale about the feet
    pub sx: f32,
    pub sy: f32,
    /// sideways offset at the top of the image (grows from the feet: a lean or sway)
    pub lean: f32,
    /// extra sideways offset of the head only
    pub head: f32,
    /// extra stretch of everything above the waist (breathing, shoulders)
    pub breath: f32,
    /// image-height fraction where the stretch starts
    pub waist: f32,
}

impl Pose {
    pub const REST: Pose = Pose { dx: 0.0, lift: 0.0, sx: 1.0, sy: 1.0, lean: 0.0, head: 0.0, breath: 0.0, waist: 0.45 };
    fn is_rest(&self) -> bool {
        self.dx.abs() < 1e-4 && self.lift.abs() < 1e-4 && (self.sx - 1.0).abs() < 1e-4 && (self.sy - 1.0).abs() < 1e-4 && self.lean.abs() < 1e-4 && self.head.abs() < 1e-4 && self.breath.abs() < 1e-4
    }
}

/// What a draw call should do: the pose, the image to fade out over the new one, and opacity.
pub struct Motion {
    pub pose: Pose,
    /// previous sprite key and how much of it is still showing (0..1)
    pub prev: Option<(&'static str, f32)>,
}

struct Inst {
    /// this frame's motion, for further draws of the same character in the same frame
    cached: Option<(Pose, Option<(&'static str, f32)>)>,
    name: String,
    bust: bool,
    key: &'static str,
    mood: Mood,
    prev: Option<&'static str>,
    changed: f32,
    born: f32,
    x: f32,
    y: f32,
    h: f32,
    seen: u64,
    used: u64,
    phase: f32,
    react: Option<(React, f32)>,
    talk_until: f32,
    /// where it was asked to be drawn this frame (before smoothing)
    tx: f32,
    ty: f32,
    th: f32,
}

struct State {
    t: f32,
    dt: f32,
    frame: u64,
    reduce: bool,
    still: bool,
    insts: Vec<Inst>,
    /// explicit reactions waiting for an instance that is not drawn yet this frame
    pending: Vec<(String, React, f32)>,
    talking: Vec<(String, f32)>,
    reacted: u64,
}

thread_local! {
    static ST: RefCell<State> = const { RefCell::new(State { t: 0.0, dt: 1.0 / 60.0, frame: 0, reduce: false, still: false, insts: Vec::new(), pending: Vec::new(), talking: Vec::new(), reacted: 0 }) };
}

/// Once per frame, before anything is drawn.
pub fn begin_frame(t: f32, reduce_motion: bool) {
    ST.with(|s| {
        let mut s = s.borrow_mut();
        s.dt = (t - s.t).clamp(0.0, 0.1);
        s.t = t;
        s.frame += 1;
        s.reduce = reduce_motion;
        if s.frame == 1 && std::env::var("PC_ANIM").map(|v| v == "0").unwrap_or(false) {
            // PC_ANIM=0: every character in its rest pose (debugging, comparisons)
            s.still = true;
        }
        let f = s.frame;
        // forget characters that were not drawn last frame
        s.insts.retain(|i| i.seen + 1 >= f);
        let now = s.t;
        s.pending.retain(|p| now - p.2 < 0.25);
        s.talking.retain(|p| p.1 > now);
    });
}

/// (characters drawn last frame, reactions started so far): for checks.
pub fn stats() -> (usize, u64) {
    ST.with(|s| {
        let s = s.borrow();
        (s.insts.len(), s.reacted)
    })
}

/// Screenshot and check modes: draw every character in its rest pose.
pub fn set_still(on: bool) {
    ST.with(|s| s.borrow_mut().still = on);
}

pub fn is_still() -> bool {
    ST.with(|s| s.borrow().still)
}

/// One-shot reaction for every place this character is drawn (in the match, say).
pub fn react(name: &str, r: React) {
    ST.with(|s| {
        let mut s = s.borrow_mut();
        let t = s.t;
        let mut hit = false;
        for i in s.insts.iter_mut().filter(|i| i.name == name && !i.bust) {
            i.react = Some((r, t));
            hit = true;
        }
        if hit {
            s.reacted += 1;
        }
        if !hit {
            s.pending.push((name.to_string(), r, t));
        }
    });
}

/// This character is speaking (call each frame while their line is being typed).
pub fn talk(name: &str) {
    ST.with(|s| {
        let mut s = s.borrow_mut();
        let until = s.t + 0.12;
        if let Some(p) = s.talking.iter_mut().find(|p| p.0 == name) {
            p.1 = until;
        } else {
            s.talking.push((name.to_string(), until));
        }
    });
}

fn hash(s: &str) -> u32 {
    s.bytes().fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619))
}

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Damped spring impulse: starts at 0, swings, dies away.
fn spring(r: f32, freq: f32, damp: f32) -> f32 {
    if r < 0.0 {
        return 0.0;
    }
    (-damp * r).exp() * (TAU * freq * r).sin()
}

/// Track a draw and work out its motion. `cx`, `bottom`, `h` are layout units.
pub fn motion(name: &str, key: &'static str, expr: &str, bust: bool, cx: i32, bottom: i32, h: i32) -> Motion {
    ST.with(|s| {
        let mut s = s.borrow_mut();
        if s.still {
            return Motion { pose: Pose::REST, prev: None };
        }
        let (t, dt, frame, reduce) = (s.t, s.dt, s.frame, s.reduce);
        let (fx, fy, fh) = (cx as f32, bottom as f32, h as f32);
        // drawn again this frame at the same spot (a second light layer): the same motion
        for i in s.insts.iter() {
            if i.used == frame && i.name == name && i.bust == bust && (i.tx - fx).abs() + (i.ty - fy).abs() < 3.0 && (i.th - fh).abs() < 3.0 {
                if let Some((pose, prev)) = i.cached {
                    return Motion { pose, prev };
                }
            }
        }
        // the same character drawn nearest here last frame (and not already claimed this frame)
        let mut best: Option<(usize, f32)> = None;
        for (k, i) in s.insts.iter().enumerate() {
            if i.name != name || i.bust != bust || i.used == frame {
                continue;
            }
            let r = fh / i.h.max(1.0);
            if !(0.7..1.43).contains(&r) {
                continue;
            }
            let d = (i.x - fx).abs() + (i.y - fy).abs();
            if d < fh * 0.4 && best.map(|b| d < b.1).unwrap_or(true) {
                best = Some((k, d));
            }
        }
        let m = mood(expr);
        let k = match best {
            Some((k, _)) => k,
            None => {
                let ph = (hash(name) % 1000) as f32 / 1000.0 * TAU;
                s.insts.push(Inst { cached: None, tx: fx, ty: fy, th: fh, name: name.to_string(), bust, key, mood: m, prev: None, changed: -10.0, born: t, x: fx, y: fy, h: fh, seen: frame, used: frame, phase: ph, react: None, talk_until: -1.0 });
                s.insts.len() - 1
            }
        };
        let pend: Vec<React> = s.pending.iter().filter(|p| p.0 == name && !bust).map(|p| p.1).collect();
        if !pend.is_empty() {
            s.pending.retain(|p| !(p.0 == name && !bust));
        }
        let talking = s.talking.iter().any(|p| p.0 == name);
        let i = &mut s.insts[k];
        i.seen = frame;
        i.used = frame;
        (i.tx, i.ty, i.th) = (fx, fy, fh);
        let mut started = 0;
        if let Some(r) = pend.last() {
            i.react = Some((*r, t));
            started += 1;
        }
        if talking {
            i.talk_until = t + 0.12;
        }
        // follow moves smoothly (a speaker stepping forward, a card sliding)
        let f = 1.0 - (-dt / 0.07).exp();
        i.x += (fx - i.x) * f;
        i.y += (fy - i.y) * f;
        i.h += (fh - i.h) * f;
        // a new expression: fade the old one out and react to the new mood
        if i.key != key {
            if t - i.born > 0.05 {
                i.prev = Some(i.key);
                i.changed = t;
                let r = match m {
                    Mood::Joy => React::Hop,
                    Mood::Angry => React::Stomp,
                    Mood::Shock => React::Recoil,
                    Mood::Sad | Mood::Sleepy => React::Slump,
                    Mood::Smug | Mood::Pose => React::Pop,
                    Mood::Calm => React::Settle,
                };
                // an explicit reaction that just started wins
                if i.react.map(|(_, at)| t - at > 0.12).unwrap_or(true) {
                    i.react = Some((r, t));
                    started += 1;
                }
            }
            i.key = key;
            i.mood = m;
        }
        // a menu pose and an expression are framed differently: swap those faster (the pop hides it)
        let quick = i.prev.map(|p| p.contains("_p-") != key.contains("_p-")).unwrap_or(false);
        s.reacted += started;
        let i = &mut s.insts[k];
        let fade = smooth(0.0, if quick { 0.09 } else { 0.16 }, t - i.changed);
        let prev = match i.prev {
            Some(p) if fade < 1.0 => Some((p, 1.0 - fade)),
            _ => {
                i.prev = None;
                None
            }
        };
        let mut pose = Pose::REST;
        if bust {
            pose.waist = 0.25;
        }
        if !reduce {
            idle(&mut pose, i, t, bust);
            if t < i.talk_until {
                // a nod per syllable, shoulders along with it
                let a = t - i.born;
                pose.head += 0.0045 * (TAU * 3.3 * a + i.phase).sin();
                pose.breath += 0.004 * (TAU * 3.3 * a).sin().abs();
            }
            if let Some((r, at)) = i.react {
                if !reaction(&mut pose, r, t - at, if bust { 0.4 } else { 1.0 }) {
                    i.react = None;
                }
            }
            // entrance: rise into place and settle
            let a = t - i.born;
            if a < 0.5 && !bust {
                let e = smooth(0.0, 0.32, a);
                pose.lift -= (1.0 - e) * 0.035;
                let sp = spring(a, 2.6, 7.0);
                pose.sy *= 1.0 + 0.045 * sp;
                pose.sx *= 1.0 - 0.03 * sp;
            }
        }
        i.cached = Some((pose, prev));
        Motion { pose, prev }
    })
}

fn idle(p: &mut Pose, i: &Inst, t: f32, bust: bool) {
    let ph = i.phase;
    // (breaths a second, breath depth, sway rate, sway size)
    let (bf, ba, sf, sa) = match i.mood {
        Mood::Calm => (0.27, 0.011, 0.17, 0.0075),
        Mood::Joy => (0.42, 0.013, 0.27, 0.010),
        Mood::Angry => (0.55, 0.013, 0.11, 0.004),
        Mood::Sad => (0.19, 0.008, 0.12, 0.004),
        Mood::Sleepy => (0.14, 0.013, 0.09, 0.006),
        Mood::Shock => (0.80, 0.007, 0.0, 0.0),
        Mood::Smug => (0.24, 0.010, 0.14, 0.011),
        Mood::Pose => (0.30, 0.010, 0.18, 0.0065),
    };
    let b = 0.5 - 0.5 * (TAU * bf * t + ph).cos();
    p.breath += ba * b;
    // breathing lifts the whole figure a hair too (heels, knees)
    p.sy *= 1.0 + 0.002 * b;
    let sway = (TAU * sf * t + ph * 1.7).sin() + 0.35 * (TAU * sf * 2.3 * t + ph * 0.6).sin();
    p.lean += sa * sway;
    // the head lags the body a little: a softer, later sway of its own
    p.head += sa * 0.35 * (TAU * sf * t + ph * 1.7 - 0.9).sin();
    let scale = if bust { 0.6 } else { 1.0 };
    match i.mood {
        Mood::Joy => {
            // a little bounce on the toes
            let bo = (TAU * 0.85 * t + ph).sin().max(0.0);
            p.lift += 0.007 * bo * scale;
            p.sy *= 1.0 + 0.008 * bo;
        }
        Mood::Angry => {
            p.dx += 0.0012 * (TAU * 8.5 * t + ph).sin() * scale;
            p.lean += 0.004;
        }
        Mood::Sad => {
            p.sy *= 0.988;
            p.head += 0.0035;
        }
        Mood::Sleepy => {
            // nodding off, then catching itself
            let n = (TAU * 0.11 * t + ph).sin();
            p.head += 0.008 * n.max(0.0).powf(3.0);
            p.sy *= 0.992;
        }
        Mood::Shock => {
            let since = t - i.changed;
            let shiver = if since >= 0.0 && since < 2.0 { (-2.0 * since).exp() } else { 0.15 };
            p.dx += 0.0028 * shiver * (TAU * 13.0 * t + ph).sin() * scale;
            p.sy *= 1.006;
        }
        Mood::Smug => {
            p.lean -= 0.003;
        }
        _ => {}
    }
}

/// Add a reaction `r` seconds in. False once it has finished.
fn reaction(p: &mut Pose, kind: React, r: f32, amt: f32) -> bool {
    match kind {
        React::Hop | React::Cheer => {
            let hops = if kind == React::Cheer { 2 } else { 1 };
            let len = 0.52;
            let k = (r / len) as i32;
            if k >= hops {
                // landing settle after the last hop
                let after = r - hops as f32 * len;
                let sp = spring(after, 3.2, 9.0);
                p.sy *= 1.0 - 0.035 * sp * amt;
                p.sx *= 1.0 + 0.025 * sp * amt;
                return after < 0.5;
            }
            let q = r - k as f32 * len;
            if q < 0.1 {
                // anticipation: crouch
                let c = smooth(0.0, 0.1, q);
                p.sy *= 1.0 - 0.06 * c * amt;
                p.sx *= 1.0 + 0.04 * c * amt;
            } else if q < 0.44 {
                // in the air: stretched on the way up, round at the top
                let a = (q - 0.1) / 0.34;
                p.lift += 0.075 * (std::f32::consts::PI * a).sin() * amt;
                let st = 1.0 - (2.0 * a - 1.0).abs();
                p.sy *= 1.0 + 0.05 * (1.0 - st) * amt;
                p.sx *= 1.0 - 0.03 * (1.0 - st) * amt;
            } else {
                // land: squash
                let a = (q - 0.44) / 0.08;
                let s = (std::f32::consts::PI * a.min(1.0)).sin();
                p.sy *= 1.0 - 0.07 * s * amt;
                p.sx *= 1.0 + 0.05 * s * amt;
            }
            true
        }
        React::Stomp => {
            let dip = if r < 0.07 { smooth(0.0, 0.07, r) } else { 1.0 - smooth(0.07, 0.22, r) };
            p.sy *= 1.0 - 0.06 * dip * amt;
            p.sx *= 1.0 + 0.04 * dip * amt;
            p.lean += 0.012 * dip * amt;
            p.dx += 0.004 * spring(r, 9.0, 8.0) * amt;
            r < 0.55
        }
        React::Recoil => {
            let up = if r < 0.08 { smooth(0.0, 0.08, r) } else { 1.0 - smooth(0.08, 0.45, r) };
            p.lift += 0.03 * up * amt;
            p.sy *= 1.0 + 0.06 * up * amt;
            p.sx *= 1.0 - 0.04 * up * amt;
            p.lean -= 0.015 * up * amt;
            p.head -= 0.01 * up * amt;
            r < 0.6
        }
        React::Slump => {
            let s = smooth(0.0, 0.35, r);
            let settle = spring(r - 0.35, 2.0, 6.0);
            p.sy *= 1.0 - (0.035 * (1.0 - s) * (r / 0.35).min(1.0) * 4.0 * (1.0 - (r / 0.35).min(1.0)) + 0.01 * settle) * amt;
            p.lean += 0.008 * (1.0 - s) * amt;
            r < 1.0
        }
        React::Pop | React::Settle => {
            let a = if kind == React::Pop { 0.05 } else { 0.025 };
            let sp = spring(r, 3.0, 8.0);
            p.sy *= 1.0 + a * sp * amt;
            p.sx *= 1.0 - a * 0.7 * sp * amt;
            r < 0.6
        }
        React::Pump => {
            let sp = spring(r, 2.4, 6.5);
            p.sy *= 1.0 + 0.055 * sp * amt;
            p.sx *= 1.0 - 0.035 * sp * amt;
            p.lift += 0.018 * sp.max(0.0) * amt;
            r < 0.7
        }
        React::Flinch => {
            let s = spring(r, 5.0, 7.0);
            p.dx += 0.012 * s * amt;
            p.lean -= 0.02 * s * amt;
            let d = 1.0 - smooth(0.0, 0.35, r);
            p.sy *= 1.0 - 0.04 * d * amt;
            r < 0.6
        }
    }
}

/// Draw `img` (real pixels) with its feet centred at (cx, bottom) in real pixels, deformed by
/// `pose`. Bilinear, alpha-weighted, blended over the canvas; spread over the worker pool.
pub fn blit(cv: &mut Canvas, img: &Canvas, cx: i32, bottom: i32, alpha: u32, pose: &Pose) {
    if alpha == 0 {
        return;
    }
    if pose.is_rest() {
        cv.blit_px(img, cx - img.w / 2, bottom - img.h, alpha);
        return;
    }
    let _p = crate::prof::scope("blit (puppet)");
    let (iw, ih) = (img.w as f32, img.h as f32);
    let p = *pose;
    let w0 = p.waist * ih;
    let b = p.breath;
    let top = p.lift * ih + p.sy * (ih + b * (ih - w0));
    let low = p.lift * ih;
    let (c0, c1, c2, c3) = cv.clip_px();
    let y0 = ((bottom as f32 - top).floor() as i32 - 1).max(c1);
    let y1 = ((bottom as f32 - low.min(0.0)).ceil() as i32 + 1).min(c3);
    if y1 <= y0 {
        return;
    }
    let width = cv.w as usize;
    let work = ((y1 - y0) as f32 * iw * p.sx) as usize;
    let inv_sx = 1.0 / p.sx.max(0.05);
    // the same centre the plain blit uses (odd widths round left)
    let cxf = (cx - img.w / 2) as f32 + iw * 0.5;
    crate::gfx::par_rows(&mut cv.px, width, y0, y1, work, |dy, row| {
        // height above the feet of this row's centre -> source height
        let hd = bottom as f32 - (dy as f32 + 0.5);
        let d = (hd - low) / p.sy;
        if d < -1.0 {
            return;
        }
        let s = if d <= w0 { d } else { (d + b * w0) / (1.0 + b) };
        if s > ih + 1.0 {
            return;
        }
        let u = (s / ih).clamp(0.0, 1.0);
        let off = (p.dx + p.lean * u.powf(1.6) + p.head * smooth(0.72, 0.92, u)) * ih;
        // source row from the top, bilinear between two rows (1/16 px steps)
        let sy = ih - s - 0.5;
        let fy0 = sy.floor();
        let wy = (((sy - fy0) * 16.0) as u32).min(16);
        let ry0 = fy0 as i32;
        let ry1 = ry0 + 1;
        let row_of = |r: i32| -> Option<&[u32]> {
            if r < 0 || r >= img.h {
                None
            } else {
                Some(&img.px[(r * img.w) as usize..((r + 1) * img.w) as usize])
            }
        };
        let (ra, rb) = (row_of(ry0), row_of(ry1));
        if ra.is_none() && rb.is_none() {
            return;
        }
        let left = cxf + off - iw * 0.5 * p.sx;
        let right = cxf + off + iw * 0.5 * p.sx;
        let x0 = (left.floor() as i32 - 1).max(c0);
        let x1 = (right.ceil() as i32 + 1).min(c2);
        // source x in 16.16 fixed point, stepping per destination pixel
        let sx_start = ((x0 as f32 + 0.5 - cxf - off) * inv_sx + iw * 0.5 - 0.5) * 65536.0;
        let step = (inv_sx * 65536.0) as i64;
        let mut fxp = sx_start as i64;
        let tap = |r: Option<&[u32]>, x: i64| -> u32 {
            match r {
                Some(r) if x >= 0 && x < img.w as i64 => r[x as usize],
                _ => 0,
            }
        };
        let wy0 = 16 - wy;
        for x in x0..x1 {
            let xi = fxp >> 16;
            let wx = ((fxp >> 12) & 15) as u32;
            fxp += step;
            let (p00, p01, p10, p11) = (tap(ra, xi), tap(ra, xi + 1), tap(rb, xi), tap(rb, xi + 1));
            if (p00 | p01 | p10 | p11) >> 24 == 0 {
                continue;
            }
            let wx0 = 16 - wx;
            // weights sum to 256
            let ws = [wx0 * wy0, wx * wy0, wx0 * wy, wx * wy];
            if (p00 & p01 & p10 & p11) >> 24 == 255 && alpha >= 255 {
                // inside the figure: all four taps opaque, a plain weighted average replaces the pixel
                let ch = |sh: u32| -> u32 { (((p00 >> sh) & 255) * ws[0] + ((p01 >> sh) & 255) * ws[1] + ((p10 >> sh) & 255) * ws[2] + ((p11 >> sh) & 255) * ws[3]) >> 8 };
                row[x as usize] = 0xFF00_0000 | (ch(16) << 16) | (ch(8) << 8) | ch(0);
                continue;
            }
            let ps = [p00, p01, p10, p11];
            let (mut sa, mut sr, mut sg, mut sb) = (0u32, 0u32, 0u32, 0u32);
            for j in 0..4 {
                let a = (ps[j] >> 24) * ws[j];
                if a == 0 {
                    continue;
                }
                sa += a;
                sr += ((ps[j] >> 16) & 255) * a;
                sg += ((ps[j] >> 8) & 255) * a;
                sb += (ps[j] & 255) * a;
            }
            // coverage 0..255 and premultiplied colour 0..255
            let mut a = sa >> 8;
            let (mut r, mut g, mut bl) = (sr >> 16, sg >> 16, sb >> 16);
            if alpha < 255 {
                a = a * alpha / 255;
                r = r * alpha / 255;
                g = g * alpha / 255;
                bl = bl * alpha / 255;
            }
            if a == 0 {
                continue;
            }
            let dp = &mut row[x as usize];
            let dv = *dp;
            let inv = 255 - a.min(255);
            let mixc = |dc: u32, pc: u32| -> u32 { (dc * inv / 255 + pc).min(255) };
            let nr = mixc((dv >> 16) & 255, r);
            let ng = mixc((dv >> 8) & 255, g);
            let nb = mixc(dv & 255, bl);
            let da = dv >> 24;
            let na = (a + da * inv / 255).min(255);
            *dp = (na << 24) | (nr << 16) | (ng << 8) | nb;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rest_pose_matches_plain_blit() {
        let mut img = Canvas::new(8, 8);
        for (k, p) in img.px.iter_mut().enumerate() {
            *p = if k % 3 == 0 { 0 } else { 0xFF00_0000 | (k as u32 * 997) };
        }
        let mut a = Canvas::new(32, 32);
        let mut b = Canvas::new(32, 32);
        a.blit_px(&img, 12, 12, 255);
        blit(&mut b, &img, 16, 20, 255, &Pose::REST);
        assert_eq!(a.px, b.px);
    }

    #[test]
    fn deformed_blit_stays_near_the_image() {
        let mut img = Canvas::new(10, 20);
        for p in img.px.iter_mut() {
            *p = 0xFFFF_0000;
        }
        let mut cv = Canvas::new(64, 64);
        let pose = Pose { lean: 0.05, breath: 0.03, sy: 1.02, sx: 0.99, ..Pose::REST };
        blit(&mut cv, &img, 32, 50, 255, &pose);
        let covered = cv.px.iter().filter(|p| (**p >> 24) > 128).count();
        assert!(covered > 180 && covered < 260, "covered {}", covered);
        // nothing below the feet
        assert!(cv.px[(52 * 64)..].iter().all(|p| *p >> 24 == 0));
    }

    #[test]
    fn expression_change_crossfades_and_reacts() {
        begin_frame(1.0, false);
        let m = motion("t_puppet", "a", "smile", false, 100, 600, 400);
        assert!(m.prev.is_none());
        begin_frame(1.2, false);
        let _ = motion("t_puppet", "a", "smile", false, 100, 600, 400);
        begin_frame(1.25, false);
        let m = motion("t_puppet", "b", "delighted", false, 100, 600, 400);
        assert_eq!(m.prev.map(|p| p.0), Some("a"));
        begin_frame(1.6, false);
        let m = motion("t_puppet", "b", "delighted", false, 100, 600, 400);
        assert!(m.prev.is_none());
    }
}
