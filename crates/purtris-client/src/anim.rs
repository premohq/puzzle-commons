//! The animated cast. Each character's motion clips (idle, talk, attack or cast, hit, big hit,
//! worried, win, lose) are rendered from the rigged 3D models (tools/blender/anim3d.py: CC0
//! motion from Quaternius's Universal Animation Library retargeted onto the VRoid skeletons,
//! hair and skirts simulated with VRM spring bones) and packed as a small video per character
//! (tools/blender/anim_pack.py, assets/anim/<id>.pca).
//!
//! On screen a character is an `Actor`, kept per slot ("seat0", "scene:left:1", ...). Each
//! actor has a worker thread that decodes the next frame ahead of time and lights it with the
//! screen's light rig (light.rs), so drawing costs one warped blit. On top of the frames sits a
//! procedural layer: squash and stretch, a lean toward the action, a recoil, all on damped
//! springs, so reactions land with weight. Reactions blend in and the one-shots (attack, hit)
//! end exactly on idle's first frame, so they settle back without a pop.
//!
//! Reduce motion: characters stand still (the expression stills, as before animation).

use crate::gfx::Canvas;
use crate::light::Rig;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Instant;

include!(concat!(env!("OUT_DIR"), "/anim_files.rs"));

// ------------------------------------------------------------------ the file
struct ClipInfo {
    name: String,
    frames: u16,
    secs: f32,
    looped: bool,
    from_idle: bool,
    streams: [&'static [u8]; 5],
}

pub struct Pca {
    w: i32,
    h: i32,
    /// where the 448 x 960 still sits inside a frame
    still: (i32, i32, i32, i32),
    q: i64,
    clips: Vec<ClipInfo>,
}

fn rd16(b: &[u8], i: usize) -> Option<u16> {
    Some(u16::from_le_bytes([*b.get(i)?, *b.get(i + 1)?]))
}
fn rd32(b: &[u8], i: usize) -> Option<u32> {
    Some(u32::from_le_bytes([*b.get(i)?, *b.get(i + 1)?, *b.get(i + 2)?, *b.get(i + 3)?]))
}

fn parse(b: &'static [u8]) -> Option<Pca> {
    if b.get(0..4)? != b"PCA1" {
        return None;
    }
    let f = |k: usize| rd16(b, 4 + k * 2);
    let (w, h, _fps, n) = (f(0)? as i32, f(1)? as i32, f(2)?, f(3)? as usize);
    let still = (f(4)? as i32, f(5)? as i32, f(6)? as i32, f(7)? as i32);
    let q = f(8)? as i64;
    let mut p = 4 + 9 * 2;
    let mut metas = Vec::new();
    for _ in 0..n {
        let ln = *b.get(p)? as usize;
        let name = String::from_utf8_lossy(b.get(p + 1..p + 1 + ln)?).to_string();
        p += 1 + ln;
        let frames = rd16(b, p)?;
        let secs = f32::from_le_bytes(b.get(p + 2..p + 6)?.try_into().ok()?);
        let looped = *b.get(p + 6)? != 0;
        let from_idle = *b.get(p + 7)? != 0;
        p += 8;
        let mut lens = [0usize; 5];
        for (k, l) in lens.iter_mut().enumerate() {
            *l = rd32(b, p + k * 4)? as usize;
        }
        p += 20;
        metas.push((name, frames, secs, looped, from_idle, lens));
    }
    let mut clips = Vec::new();
    for (name, frames, secs, looped, from_idle, lens) in metas {
        let mut streams: [&'static [u8]; 5] = [&[]; 5];
        for (k, l) in lens.iter().enumerate() {
            streams[k] = b.get(p..p + l)?;
            p += l;
        }
        clips.push(ClipInfo { name, frames, secs, looped, from_idle, streams });
    }
    Some(Pca { w, h, still, q, clips })
}

fn pca(id: &str) -> Option<&'static Pca> {
    static ALL: std::sync::OnceLock<HashMap<&'static str, Pca>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| ANIM_FILES.iter().filter_map(|(n, b)| Some((*n, parse(b)?))).collect()).get(id)
}

/// Whether a cast member has animation clips.
/// Whether cast member `id` has motion clips (the courier: whichever model the player picked).
pub fn has_clips(id: &str) -> bool {
    has_anim(if id == "mira" { crate::cast::hero_sprite() } else { id })
}

pub fn has_anim(id: &str) -> bool {
    pca(id).is_some()
}

impl Pca {
    fn clip(&self, name: &str) -> Option<usize> {
        self.clips.iter().position(|c| c.name == name)
    }
}

// ------------------------------------------------------------------ the decoder
// Mirrors tools/blender/anim_pack.py exactly (integer inverse DCT, same tables, same order).
const ZIGZAG: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30,
    37, 44, 51, 58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];
const T_LUM: [i64; 64] = [
    16, 11, 10, 16, 24, 40, 51, 61, 12, 12, 14, 19, 26, 58, 60, 55, 14, 13, 16, 24, 40, 57, 69, 56, 14, 17, 22, 29, 51, 87, 80, 62, 18, 22, 37, 56, 68, 109, 103, 77, 24, 35, 55,
    64, 81, 104, 113, 92, 49, 64, 78, 87, 103, 121, 120, 101, 72, 92, 95, 98, 112, 100, 103, 99,
];
const T_CHR: [i64; 64] = [
    17, 18, 24, 47, 99, 99, 99, 99, 18, 21, 26, 66, 99, 99, 99, 99, 24, 26, 56, 99, 99, 99, 99, 99, 47, 66, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
];
const MB: i32 = 16;

fn qtable(t: &[i64; 64], q: i64) -> [i64; 64] {
    // 200 - 2q for q >= 50, 5000 / q below (in hundredths, as the encoder computes in floats)
    let mut o = [1i64; 64];
    for i in 0..64 {
        let v = if q < 50 {
            ((t[i] as f64 * (5000.0 / q as f64) + 50.0) / 100.0).floor()
        } else {
            ((t[i] as f64 * (200.0 - 2.0 * q as f64) + 50.0) / 100.0).floor()
        };
        o[i] = (v as i64).clamp(1, 255);
    }
    o
}

fn idct_basis() -> &'static [[i64; 8]; 8] {
    static C: std::sync::OnceLock<[[i64; 8]; 8]> = std::sync::OnceLock::new();
    C.get_or_init(|| {
        let mut c = [[0i64; 8]; 8];
        for (u, row) in c.iter_mut().enumerate() {
            for (x, v) in row.iter_mut().enumerate() {
                let cu = if u == 0 { (1.0f64 / 8.0).sqrt() } else { 0.5 };
                let f = cu * ((2 * x + 1) as f64 * u as f64 * std::f64::consts::PI / 16.0).cos();
                // numpy's round: halves to even
                let s = f * 4096.0;
                let r = s.round();
                *v = if (s - s.trunc()).abs() == 0.5 { 2.0 * (s / 2.0).round() } else { r } as i64;
            }
        }
        c
    })
}

/// Integer inverse DCT of dequantized coefficients (row-major, [v][u]) into `out` ([y][x]).
fn idct(coef: &[i64; 64], out: &mut [i32; 64]) {
    let c = idct_basis();
    let mut tmp = [0i64; 64];
    for v in 0..8 {
        for x in 0..8 {
            let mut s = 0i64;
            for u in 0..8 {
                s += coef[v * 8 + u] * c[u][x];
            }
            tmp[v * 8 + x] = (s + 256) >> 9;
        }
    }
    for y in 0..8 {
        for x in 0..8 {
            let mut s = 0i64;
            for v in 0..8 {
                s += c[v][y] * tmp[v * 8 + x];
            }
            out[y * 8 + x] = ((s + (1 << 14)) >> 15) as i32;
        }
    }
}

#[derive(Clone)]
struct Planes {
    w: i32,
    h: i32,
    y: Vec<u8>,
    co: Vec<u8>,
    cg: Vec<u8>,
    a: Vec<u8>,
    nx: Vec<u8>,
    ny: Vec<u8>,
    k: Vec<u8>,
}

impl Planes {
    fn blank(w: i32, h: i32) -> Planes {
        let (n, m) = ((w * h) as usize, ((w / 2) * (h / 2)) as usize);
        Planes { w, h, y: vec![0; n], co: vec![128; n], cg: vec![128; n], a: vec![0; n], nx: vec![128; m], ny: vec![128; m], k: vec![0; m] }
    }
}

struct Reader<'a> {
    b: &'a [u8],
    p: usize,
}
impl Reader<'_> {
    fn u8(&mut self) -> u8 {
        let v = self.b.get(self.p).copied().unwrap_or(0);
        self.p += 1;
        v
    }
    fn i8(&mut self) -> i32 {
        self.u8() as i8 as i32
    }
    fn i16(&mut self) -> i64 {
        let v = i16::from_le_bytes([self.b.get(self.p).copied().unwrap_or(0), self.b.get(self.p + 1).copied().unwrap_or(0)]);
        self.p += 2;
        v as i64
    }
}

struct Decoder {
    pca: &'static Pca,
    qy: [i64; 64],
    qc: [i64; 64],
    qn: [i64; 64],
    /// inflated streams per clip, made on first use
    data: Vec<Option<[Vec<u8>; 5]>>,
    idle0: Option<Planes>,
    cur: Planes,
    spare: Planes,
    /// the clip and the next frame index `cur` is ready to decode into (cur holds frame - 1)
    clip: usize,
    next: u16,
    cursors: [usize; 5],
}

impl Decoder {
    fn new(pca: &'static Pca) -> Decoder {
        let q = pca.q;
        Decoder {
            pca,
            qy: qtable(&T_LUM, q),
            qc: qtable(&T_CHR, q),
            qn: qtable(&T_LUM, (q + 6).min(95)),
            data: (0..pca.clips.len()).map(|_| None).collect(),
            idle0: None,
            cur: Planes::blank(pca.w, pca.h),
            spare: Planes::blank(pca.w, pca.h),
            clip: usize::MAX,
            next: 0,
            cursors: [0; 5],
        }
    }

    fn streams(&mut self, clip: usize) -> bool {
        if self.data[clip].is_none() {
            let c = &self.pca.clips[clip];
            let mut v: [Vec<u8>; 5] = Default::default();
            for k in 0..5 {
                let s = c.streams[k];
                v[k] = if s.len() > 2 { crate::png::inflate(&s[2..]).unwrap_or_default() } else { Vec::new() };
            }
            self.data[clip] = Some(v);
        }
        true
    }

    /// Make `cur` hold frame `frame` of `clip`.
    fn seek(&mut self, clip: usize, frame: u16) {
        if clip >= self.pca.clips.len() {
            return;
        }
        let frame = frame.min(self.pca.clips[clip].frames.saturating_sub(1));
        if !(self.clip == clip && self.next == frame + 1) {
            if !(self.clip == clip && self.next <= frame) {
                self.restart(clip);
            }
            while self.next <= frame {
                self.step();
            }
        }
    }

    fn restart(&mut self, clip: usize) {
        self.streams(clip);
        if self.pca.clips[clip].from_idle {
            if self.idle0.is_none() {
                // idle is always the first clip
                self.restart(0);
                self.step();
            }
            self.cur = self.idle0.clone().unwrap_or_else(|| Planes::blank(self.pca.w, self.pca.h));
        } else {
            self.cur = Planes::blank(self.pca.w, self.pca.h);
        }
        self.streams(clip);
        self.clip = clip;
        self.next = 0;
        self.cursors = [0; 5];
    }

    /// Decode the next frame of the current clip into `cur`.
    fn step(&mut self) {
        let clip = self.clip;
        let Some(st) = self.data[clip].as_ref() else { return };
        let (w, h) = (self.pca.w, self.pca.h);
        let (hw, hh) = (w / 2, h / 2);
        let (mx, my) = (w / MB, h / MB);
        let mut modes = Reader { b: &st[0], p: self.cursors[0] };
        let mut mvs = Reader { b: &st[1], p: self.cursors[1] };
        let mut coefs = Reader { b: &st[2], p: self.cursors[2] };
        let mut araw = Reader { b: &st[3], p: self.cursors[3] };
        let mut kraw = Reader { b: &st[4], p: self.cursors[4] };
        let refp = &self.cur;
        let out = &mut self.spare;
        let mut blk = [0i32; 64];
        let mut coef = [0i64; 64];
        for by in 0..my {
            for bx in 0..mx {
                let mode = modes.u8();
                let (dx, dy, f) = if mode == 0 { (0, 0, 0) } else { (mvs.i8(), mvs.i8(), (mode - 1) >> 1) };
                // predictions: full-size planes move by (dx, dy), half-size by half that
                let (x0, y0) = (bx * MB, by * MB);
                for (pl_ref, pl_out) in [(&refp.y, &mut out.y), (&refp.co, &mut out.co), (&refp.cg, &mut out.cg), (&refp.a, &mut out.a)] {
                    for yy in 0..MB {
                        let sy = (y0 + yy + dy).clamp(0, h - 1);
                        let orow = ((y0 + yy) * w) as usize;
                        let srow = (sy * w) as usize;
                        for xx in 0..MB {
                            let sx = (x0 + xx + dx).clamp(0, w - 1);
                            pl_out[orow + (x0 + xx) as usize] = pl_ref[srow + sx as usize];
                        }
                    }
                }
                let (hdx, hdy) = (dx >> 1, dy >> 1);
                let (hx0, hy0) = (bx * 8, by * 8);
                for (pl_ref, pl_out) in [(&refp.nx, &mut out.nx), (&refp.ny, &mut out.ny), (&refp.k, &mut out.k)] {
                    for yy in 0..8 {
                        let sy = (hy0 + yy + hdy).clamp(0, hh - 1);
                        let orow = ((hy0 + yy) * hw) as usize;
                        let srow = (sy * hw) as usize;
                        for xx in 0..8 {
                            let sx = (hx0 + xx + hdx).clamp(0, hw - 1);
                            pl_out[orow + (hx0 + xx) as usize] = pl_ref[srow + sx as usize];
                        }
                    }
                }
                if mode == 0 {
                    for yy in 0..MB {
                        let o = ((y0 + yy) * w + x0) as usize;
                        out.a[o..o + MB as usize].fill(0);
                    }
                    continue;
                }
                let mut add = |plane: &mut Vec<u8>, pw: i32, ox: i32, oy: i32, q: &[i64; 64], coefs: &mut Reader| {
                    let n = coefs.u8() as usize;
                    if n == 0 {
                        return;
                    }
                    coef.fill(0);
                    for (zi, &pos) in ZIGZAG.iter().enumerate().take(n.min(64)) {
                        let _ = zi;
                        coef[pos] = coefs.i16() * q[pos];
                    }
                    idct(&coef, &mut blk);
                    for yy in 0..8 {
                        let row = ((oy + yy) * pw + ox) as usize;
                        for xx in 0..8 {
                            let i = row + xx as usize;
                            plane[i] = (plane[i] as i32 + blk[(yy * 8 + xx) as usize]).clamp(0, 255) as u8;
                        }
                    }
                };
                if f & 1 != 0 {
                    for (pl, q) in [(&mut out.y, &self.qy), (&mut out.co, &self.qc), (&mut out.cg, &self.qc)] {
                        for sy in 0..2 {
                            for sx in 0..2 {
                                add(pl, w, x0 + sx * 8, y0 + sy * 8, q, &mut coefs);
                            }
                        }
                    }
                }
                if f & 4 != 0 {
                    add(&mut out.nx, hw, hx0, hy0, &self.qn, &mut coefs);
                    add(&mut out.ny, hw, hx0, hy0, &self.qn, &mut coefs);
                }
                if f & 2 != 0 {
                    for yy in 0..MB {
                        let row = ((y0 + yy) * w + x0) as usize;
                        for xx in 0..MB as usize {
                            out.a[row + xx] = out.a[row + xx].wrapping_add(araw.u8());
                        }
                    }
                }
                if f & 8 != 0 {
                    for yy in 0..8 {
                        let row = ((hy0 + yy) * hw + hx0) as usize;
                        for xx in 0..8 {
                            out.k[row + xx] = out.k[row + xx].wrapping_add(kraw.u8());
                        }
                    }
                }
            }
        }
        self.cursors = [modes.p, mvs.p, coefs.p, araw.p, kraw.p];
        std::mem::swap(&mut self.cur, &mut self.spare);
        if clip == 0 && self.next == 0 && self.idle0.is_none() {
            self.idle0 = Some(self.cur.clone());
        }
        self.next += 1;
    }

    /// The current frame as an albedo image and a half-size lighting map.
    fn images(&self) -> (Canvas, Canvas, (i32, i32, i32, i32)) {
        let p = &self.cur;
        let (w, h) = (p.w, p.h);
        let mut al = Canvas::transparent(w, h);
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
        for yy in 0..h {
            let row = (yy * w) as usize;
            for xx in 0..w {
                let i = row + xx as usize;
                let a = p.a[i] as u32;
                if a == 0 {
                    continue;
                }
                let (y, co, cg) = (p.y[i] as i32, p.co[i] as i32 - 128, p.cg[i] as i32 - 128);
                let t = y - cg;
                let g = (y + cg).clamp(0, 255) as u32;
                let r = (t + co).clamp(0, 255) as u32;
                let b = (t - co).clamp(0, 255) as u32;
                al.px[i] = (a << 24) | (r << 16) | (g << 8) | b;
                x0 = x0.min(xx);
                x1 = x1.max(xx + 1);
                y0 = y0.min(yy);
                y1 = y1.max(yy + 1);
            }
        }
        let (hw, hh) = (w / 2, h / 2);
        let mut d = Canvas::transparent(hw, hh);
        for i in 0..(hw * hh) as usize {
            d.px[i] = 0xFF00_0000 | (p.nx[i] as u32) << 16 | (p.ny[i] as u32) << 8 | p.k[i] as u32;
        }
        if x1 <= x0 {
            (x0, y0, x1, y1) = (0, 0, 1, 1);
        }
        (al, d, (x0, y0, x1, y1))
    }
}

// ------------------------------------------------------------------ the worker
/// A lit frame, cropped to its content. Pixel (0, 0) is frame pixel (ox, oy) at 1/`mip` size.
pub struct Lit {
    img: Canvas,
    ox: f32,
    oy: f32,
    mip: f32,
}

#[derive(Clone, Copy)]
struct Req {
    clip: usize,
    frame: u16,
    rig: Rig,
    flip: bool,
    mip: u8,
}

struct Res {
    clip: usize,
    frame: u16,
    key: (u64, bool, u8),
    lit: Arc<Lit>,
}

fn half(src: &Canvas) -> Canvas {
    // 2 x 2 box filter, premultiplied, for small sizes (keeps the ink line from shimmering)
    let (w, h) = (src.w / 2, src.h / 2);
    let mut o = Canvas::transparent(w.max(1), h.max(1));
    for y in 0..h {
        for x in 0..w {
            let mut s = [0u32; 4];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let p = src.px[((y * 2 + dy) * src.w + x * 2 + dx) as usize];
                let a = p >> 24;
                s[0] += ((p >> 16) & 255) * a;
                s[1] += ((p >> 8) & 255) * a;
                s[2] += (p & 255) * a;
                s[3] += a;
            }
            if s[3] > 0 {
                o.px[(y * w + x) as usize] = ((s[3] / 4) << 24) | ((s[0] / s[3]) << 16) | ((s[1] / s[3]) << 8) | (s[2] / s[3]);
            }
        }
    }
    o
}

fn spawn(pca: &'static Pca) -> (Sender<Req>, Receiver<Res>) {
    let (tx, rx) = channel::<Req>();
    let (rtx, rrx) = channel::<Res>();
    let _ = std::thread::Builder::new().name("cast-anim".into()).spawn(move || {
        let mut dec = Decoder::new(pca);
        while let Ok(mut req) = rx.recv() {
            // only the newest request matters
            while let Ok(r) = rx.try_recv() {
                req = r;
            }
            dec.seek(req.clip, req.frame);
            let (albedo, data, rect) = dec.images();
            let mut img = crate::light::light_rect(&albedo, &data, &req.rig, req.flip, rect);
            let (ox, oy) = (rect.0 as f32, rect.1 as f32);
            let mut mip = 1.0;
            for _ in 0..req.mip {
                img = half(&img);
                mip *= 2.0;
            }
            let lit = Arc::new(Lit { img, ox, oy, mip });
            if rtx.send(Res { clip: req.clip, frame: dec.next.saturating_sub(1), key: (req.rig.id(), req.flip, req.mip), lit }).is_err() {
                return;
            }
        }
    });
    (tx, rrx)
}

// ------------------------------------------------------------------ actors
/// What a character is doing between reactions.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mood {
    Idle,
    /// the talk clip (rendered; scenes use the puppet layer, so nothing asks for it yet)
    #[allow(dead_code)]
    Talk,
    Worried,
    Win,
    Lose,
}

/// A one-off reaction; strength 0..1 scales the procedural kick.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum React {
    /// sent a big attack (punch or spell, by character)
    Attack(f32),
    /// took garbage
    Hit(f32),
}

impl Mood {
    fn clip(self) -> &'static str {
        match self {
            Mood::Idle => "idle",
            Mood::Talk => "talk",
            Mood::Worried => "worried",
            Mood::Win => "win",
            Mood::Lose => "lose",
        }
    }
    /// The still shown when there is no animation (or with Reduce motion).
    pub fn expr(self) -> &'static str {
        match self {
            Mood::Idle | Mood::Talk => "smile",
            Mood::Worried => "shocked",
            Mood::Win => "delighted",
            Mood::Lose => "sad",
        }
    }
}

/// A damped spring toward 0 (used for every procedural motion).
#[derive(Clone, Copy, Default)]
struct Spring {
    x: f32,
    v: f32,
}
impl Spring {
    fn step(&mut self, dt: f32, stiff: f32, damp: f32) {
        // semi-implicit Euler in small steps (stable at any frame rate)
        let n = ((dt / 0.004).ceil() as i32).clamp(1, 30);
        let h = dt / n as f32;
        for _ in 0..n {
            self.v += (-stiff * self.x - damp * self.v) * h;
            self.x += self.v * h;
        }
    }
}

struct Actor {
    id: String,
    pca: Option<&'static Pca>,
    tx: Option<Sender<Req>>,
    rx: Option<Receiver<Res>>,
    mood: Mood,
    /// the clip playing and how far in (seconds)
    clip: usize,
    t: f32,
    oneshot: bool,
    /// frames received for the current clip (frame, lit)
    have: Vec<(u16, Arc<Lit>)>,
    shown: Option<Arc<Lit>>,
    /// crossfade from the previous clip's last frame
    fade: Option<(Arc<Lit>, f32)>,
    pending: Option<(usize, u16, (u64, bool, u8))>,
    key: (u64, bool, u8),
    last: Instant,
    /// procedural layer: squash (+ = stretched tall), lean (+ = toward the screen's centre),
    /// a horizontal recoil in layout units
    squash: Spring,
    lean: Spring,
    recoil: Spring,
    life: f32,
}

impl Actor {
    fn new(id: &str) -> Actor {
        let pca = pca(id);
        let (tx, rx) = match pca {
            Some(p) => {
                let (a, b) = spawn(p);
                (Some(a), Some(b))
            }
            None => (None, None),
        };
        Actor {
            id: id.to_string(),
            pca,
            tx,
            rx,
            mood: Mood::Idle,
            clip: 0,
            t: 0.0,
            oneshot: false,
            have: Vec::new(),
            shown: None,
            fade: None,
            pending: None,
            key: (0, false, 0),
            last: Instant::now(),
            squash: Spring::default(),
            lean: Spring::default(),
            recoil: Spring::default(),
            life: 0.0,
        }
    }

    fn info(&self) -> Option<&'static ClipInfo> {
        self.pca.and_then(|p| p.clips.get(self.clip))
    }

    fn frame_at(&self) -> u16 {
        let Some(c) = self.info() else { return 0 };
        let n = c.frames.max(1) as f32;
        if c.looped {
            ((self.t / c.secs.max(1e-3)) * n).floor().rem_euclid(n) as u16
        } else {
            (((self.t / c.secs.max(1e-3)) * (n - 1.0)).round().clamp(0.0, n - 1.0)) as u16
        }
    }

    /// Switch clips; `fade` seconds of crossfade from what is showing now.
    fn play(&mut self, clip: usize, oneshot: bool, fade: f32) {
        if clip == self.clip && !oneshot && !self.oneshot {
            return;
        }
        if fade > 0.0 {
            if let Some(s) = self.shown.clone() {
                self.fade = Some((s, fade));
            }
        }
        self.clip = clip;
        self.t = 0.0;
        self.oneshot = oneshot;
        self.have.clear();
        self.pending = None;
    }

    fn mood_clip(&self) -> usize {
        self.pca.and_then(|p| p.clip(self.mood.clip())).unwrap_or(0)
    }

    fn react(&mut self, r: React, toward: f32) {
        // the procedural kick, then the clip
        match r {
            React::Attack(s) => {
                self.squash.v += 1.4 + 1.2 * s;
                self.lean.v += (0.9 + 0.8 * s) * toward;
            }
            React::Hit(s) => {
                self.squash.v -= 1.6 + 1.6 * s;
                self.lean.v -= (0.7 + 0.9 * s) * toward;
                self.recoil.v -= (60.0 + 90.0 * s) * toward;
            }
        }
        let Some(p) = self.pca else { return };
        let name = match r {
            React::Attack(_) => {
                if p.clip("attack").is_some() {
                    "attack"
                } else {
                    "cast"
                }
            }
            React::Hit(s) => {
                if s > 0.5 {
                    "hit_big"
                } else {
                    "hit"
                }
            }
        };
        // a hit never cuts into a lose or win (the match is over by then)
        if matches!(self.mood, Mood::Win | Mood::Lose) {
            return;
        }
        if let Some(c) = p.clip(name) {
            self.play(c, true, 0.08);
        }
    }

    fn update(&mut self, dt: f32, req: Req) {
        self.life += dt;
        self.squash.step(dt, 260.0, 13.0);
        self.lean.step(dt, 120.0, 11.0);
        self.recoil.step(dt, 150.0, 16.0);
        if let Some((_, f)) = &mut self.fade {
            *f -= dt;
            if *f <= 0.0 {
                self.fade = None;
            }
        }
        let Some(p) = self.pca else { return };
        // mood changes blend over a quarter second; one-shots finish first
        let want = self.mood_clip();
        if !self.oneshot && want != self.clip {
            self.play(want, false, 0.25);
        }
        self.t += dt;
        if self.oneshot {
            let secs = p.clips.get(self.clip).map(|c| c.secs).unwrap_or(0.0);
            if self.t >= secs {
                // the one-shot ends on idle's first frame: carry straight on
                let idle = p.clip(Mood::Idle.clip()).unwrap_or(0);
                let next = if self.mood == Mood::Idle { idle } else { want };
                let fade = if next == idle { 0.0 } else { 0.25 };
                self.oneshot = false;
                if let Some(s) = self.shown.clone() {
                    if fade > 0.0 {
                        self.fade = Some((s, fade));
                    }
                }
                self.clip = next;
                self.t = 0.0;
                self.have.clear();
                self.pending = None;
            }
        }
        // results in
        if let Some(rx) = &self.rx {
            while let Ok(r) = rx.try_recv() {
                if self.pending.map(|p| p.0 == r.clip && p.2 == r.key).unwrap_or(false) {
                    self.pending = None;
                }
                if r.clip == self.clip && r.key == self.key {
                    self.have.retain(|(f, _)| *f != r.frame);
                    self.have.push((r.frame, r.lit));
                    if self.have.len() > 3 {
                        self.have.remove(0);
                    }
                }
            }
        }
        let key = (req.rig.id(), req.flip, req.mip);
        if key != self.key {
            self.key = key;
            self.have.clear();
            self.pending = None;
        }
        let f = self.frame_at();
        // show the wanted frame, or hold the nearest one before it
        if let Some((_, l)) = self.have.iter().find(|(k, _)| *k == f) {
            self.shown = Some(l.clone());
        } else if let Some((_, l)) = self.have.iter().filter(|(k, _)| *k < f).max_by_key(|(k, _)| *k) {
            self.shown = Some(l.clone());
        }
        // ask for what comes next
        if self.pending.is_none() {
            let c = &p.clips[self.clip];
            let ahead = if c.looped { (f + 1) % c.frames.max(1) } else { (f + 1).min(c.frames.saturating_sub(1)) };
            let target = if self.have.iter().any(|(k, _)| *k == f) { ahead } else { f };
            if !self.have.iter().any(|(k, _)| *k == target) {
                if let Some(tx) = &self.tx {
                    let _ = tx.send(Req { clip: self.clip, frame: target, ..req });
                    self.pending = Some((self.clip, target, key));
                }
            }
        }
    }
}

struct State {
    actors: HashMap<String, Actor>,
    reduce: bool,
    frame: u64,
}

thread_local! {
    static STATE: std::cell::RefCell<State> = std::cell::RefCell::new(State { actors: HashMap::new(), reduce: false, frame: 0 });
}

/// Reduce motion: characters keep still (called each frame from the app).
pub fn set_reduce_motion(on: bool) {
    STATE.with(|s| s.borrow_mut().reduce = on);
}

/// Forget actors that have not been drawn for a while (their workers stop).
pub fn sweep() {
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        s.frame += 1;
        if s.frame % 60 == 0 {
            s.actors.retain(|_, a| a.last.elapsed().as_secs_f32() < 3.0);
        }
    });
}

/// Trigger a reaction on the character in `slot`. `toward` is +1 when the action is to the
/// character's screen right, -1 to the left.
pub fn react(slot: &str, r: React, toward: f32) {
    STATE.with(|s| {
        if let Some(a) = s.borrow_mut().actors.get_mut(slot) {
            a.react(r, toward);
        }
    });
}

/// Restart the character in `slot` (a new match, a new scene).
#[allow(dead_code)]
pub fn reset(slot: &str) {
    STATE.with(|s| {
        s.borrow_mut().actors.remove(slot);
    });
}

/// Draw cast member `id` in `slot`, feet at (cx, bottom), `h` layout units tall (the same
/// size and place draw_char gives the still), doing `mood` between reactions.
#[allow(clippy::too_many_arguments)]
pub fn draw(cv: &mut Canvas, slot: &str, id: &str, mood: Mood, cx: i32, bottom: i32, h: i32, alpha: u32, flip: bool) {
    let id = if id == "mira" { crate::cast::hero_sprite() } else { id };
    let reduce = STATE.with(|s| s.borrow().reduce);
    if reduce || crate::assets::is_creature(id) && !has_anim(id) {
        if reduce {
            crate::assets::draw_char_ex(cv, id, mood.expr(), cx, bottom, h, alpha, flip);
        } else {
            creature(cv, slot, id, mood, cx, bottom, h, alpha, flip);
        }
        return;
    }
    let rig = crate::light::current();
    let ph = cv.us(h) as f32;
    // frames are 960 px per still height; draw small sizes from a half-size copy
    let mip = if ph / 960.0 < 0.62 { 1 } else { 0 };
    let req = Req { clip: 0, frame: 0, rig, flip, mip };
    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let first = !s.actors.contains_key(slot);
        let a = s.actors.entry(slot.to_string()).or_insert_with(|| Actor::new(id));
        if a.id != id {
            *a = Actor::new(id);
        }
        let now = Instant::now();
        let dt = now.duration_since(a.last).as_secs_f32().min(0.1);
        a.last = now;
        a.mood = mood;
        a.update(if first { 0.0 } else { dt }, req);
        if first && a.pca.is_some() {
            // wait briefly for the very first frame so a character never pops in late
            if let Some(rx) = &a.rx {
                if let Ok(r) = rx.recv_timeout(std::time::Duration::from_millis(120)) {
                    a.pending = None;
                    a.have.push((r.frame, r.lit.clone()));
                    a.shown = Some(r.lit);
                }
            }
        }
        let toward = if flip { -1.0 } else { 1.0 };
        let p = Proc { squash: a.squash.x, lean: a.lean.x * toward, recoil: a.recoil.x * toward };
        match (a.pca, a.shown.clone()) {
            (Some(pc), Some(lit)) => {
                shadow(cv, cx, bottom, h, alpha);
                blit_frame(cv, pc, &lit, cx, bottom, h, alpha, flip, p);
                if let Some((old, f)) = &a.fade {
                    let k = (f / 0.25).clamp(0.0, 1.0);
                    blit_frame(cv, pc, old, cx, bottom, h, (alpha as f32 * k) as u32, flip, p);
                }
            }
            _ => {
                // no clips for this character (or not decoded yet): the still, with the same springs
                crate::assets::draw_char_ex(cv, id, mood.expr(), cx + p.recoil as i32, bottom, h, alpha, flip);
            }
        }
    });
}

#[derive(Clone, Copy)]
struct Proc {
    squash: f32,
    lean: f32,
    recoil: f32,
}

fn shadow(cv: &mut Canvas, cx: i32, bottom: i32, h: i32, alpha: u32) {
    cv.ellipse(cx as f32, bottom as f32 - 4.0, h as f32 * 0.12, (h as f32 * 0.022).max(3.0), 0xFF00_0000, alpha / 3);
}

/// Draw a lit frame with the procedural layer: the still's feet stay planted at (cx, bottom);
/// squash scales about the feet (keeping volume), lean shears the body over its feet.
#[allow(clippy::too_many_arguments)]
fn blit_frame(cv: &mut Canvas, pc: &Pca, lit: &Lit, cx: i32, bottom: i32, h: i32, alpha: u32, flip: bool, p: Proc) {
    if alpha == 0 {
        return;
    }
    let s = cv.us(h) as f32 / pc.still.3 as f32; // screen px per frame px
    let sq = p.squash.clamp(-0.25, 0.25) * 0.06;
    let sy = s * (1.0 + sq);
    let sx = s * (1.0 - sq * 0.5);
    let shear = p.lean.clamp(-1.5, 1.5) * 0.035;
    // the frame point that stands on (cx, bottom)
    let fax = (pc.still.0 + pc.still.2 / 2) as f32;
    let fay = (pc.still.1 + pc.still.3) as f32;
    let ax = cv.u(cx) as f32 + p.recoil * cv.k;
    let ay = cv.u(bottom) as f32;
    let img = &lit.img;
    let (iw, ih) = (img.w, img.h);
    // screen bounds of the frame's content
    let top = ay - (fay - lit.oy) * sy;
    let bot = ay - (fay - (lit.oy + ih as f32 * lit.mip)) * sy;
    let span = |fx: f32| (fx - fax) * sx;
    let (l0, l1) = (span(lit.ox), span(lit.ox + iw as f32 * lit.mip));
    let (mut lx, mut rx) = if flip { (-l1, -l0) } else { (l0, l1) };
    let lean_max = shear * (ay - top);
    lx += lean_max.min(0.0);
    rx += lean_max.max(0.0);
    let y0 = (top.floor() as i32 - 1).max(0);
    let y1 = (bot.ceil() as i32 + 1).min(cv.h);
    let x0 = ((ax + lx).floor() as i32 - 1).max(0);
    let x1 = ((ax + rx).ceil() as i32 + 1).min(cv.w);
    if y1 <= y0 || x1 <= x0 {
        return;
    }
    let w = cv.w as usize;
    let inv_m = 1.0 / lit.mip;
    let work = ((x1 - x0) * (y1 - y0)) as usize * 3;
    let (ox, oy) = (lit.ox, lit.oy);
    crate::gfx::par_rows(&mut cv.px, w, y0, y1, work, |y, row| {
        let dy = y as f32 + 0.5 - ay;
        // frame y, then image y
        let fy = fay + dy / sy;
        let iy = (fy - oy) * inv_m - 0.5;
        if iy < -1.0 || iy > ih as f32 {
            return;
        }
        let shift = shear * -dy;
        let y_a = iy.floor() as i32;
        let ty = iy - y_a as f32;
        for x in x0..x1 {
            let mut dx = x as f32 + 0.5 - ax - shift;
            if flip {
                dx = -dx;
            }
            let fx = fax + dx / sx;
            let ix = (fx - ox) * inv_m - 0.5;
            if ix < -1.0 || ix > iw as f32 {
                continue;
            }
            let x_a = ix.floor() as i32;
            let tx = ix - x_a as f32;
            let get = |xx: i32, yy: i32| -> u32 {
                if xx < 0 || yy < 0 || xx >= iw || yy >= ih {
                    0
                } else {
                    img.px[(yy * iw + xx) as usize]
                }
            };
            let q = [get(x_a, y_a), get(x_a + 1, y_a), get(x_a, y_a + 1), get(x_a + 1, y_a + 1)];
            if (q[0] | q[1] | q[2] | q[3]) >> 24 == 0 {
                continue;
            }
            let wts = [(1.0 - tx) * (1.0 - ty), tx * (1.0 - ty), (1.0 - tx) * ty, tx * ty];
            let (mut r, mut g, mut b, mut a) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for k in 0..4 {
                let pa = (q[k] >> 24) as f32 * wts[k];
                r += ((q[k] >> 16) & 255) as f32 * pa;
                g += ((q[k] >> 8) & 255) as f32 * pa;
                b += (q[k] & 255) as f32 * pa;
                a += pa;
            }
            if a < 0.5 {
                continue;
            }
            let c = ((r / a) as u32) << 16 | ((g / a) as u32) << 8 | (b / a) as u32;
            let al = (a as u32).min(255) * alpha / 255;
            let d = &mut row[x as usize];
            *d = crate::gfx::blend_px(*d, c, al);
        }
    });
}

/// Creatures (no rig in the game, only stills): motion that fits a round body. A squashy
/// breathing pulse, a little hop now and then, and a wobble when they react.
#[allow(clippy::too_many_arguments)]
fn creature(cv: &mut Canvas, slot: &str, id: &str, mood: Mood, cx: i32, bottom: i32, h: i32, alpha: u32, flip: bool) {
    let t = STATE.with(|s| {
        let mut s = s.borrow_mut();
        let a = s.actors.entry(slot.to_string()).or_insert_with(|| Actor::new(id));
        let now = Instant::now();
        let dt = now.duration_since(a.last).as_secs_f32().min(0.1);
        a.last = now;
        a.squash.step(dt, 200.0, 7.0);
        a.recoil.step(dt, 150.0, 12.0);
        a.life += dt;
        (a.life, a.squash.x, a.recoil.x)
    });
    let (life, sq, rec) = t;
    let rate = match mood {
        Mood::Win => 2.2,
        Mood::Worried => 1.6,
        Mood::Lose => 0.5,
        _ => 0.9,
    };
    let pulse = (life * std::f32::consts::TAU * rate).sin();
    let hop = if matches!(mood, Mood::Win) { (life * std::f32::consts::TAU * rate).sin().abs() * h as f32 * 0.08 } else { 0.0 };
    let stretch = 1.0 + 0.03 * pulse + 0.06 * sq;
    let hh = (h as f32 * stretch) as i32;
    crate::assets::draw_char_ex(cv, id, mood.expr(), cx + rec as i32, bottom - hop as i32, hh, alpha, flip);
}
