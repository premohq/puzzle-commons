//! Software rasterizer. Everything on screen is drawn here, pixel by pixel, into a u32 buffer
//! that the OS layer presents. Colors are 0xAARRGGBB; alpha is coverage for sprites.
//!
//! Resolution independence: every drawing call takes coordinates in *layout units* (the game is
//! laid out on a 1280x720 grid). A canvas has a scale `k`; the screen canvas is 1280k x 720k real
//! pixels, so the same layout renders natively at 720p, 1080p, 1440p or 4K. Sprite canvases are
//! built at k = 1 directly in pixels. Images passed to `blit` are already in real pixels (the
//! sprite, glyph, icon and character caches render them at the canvas scale).

use crate::font;

#[inline]
pub fn hex(h: u32) -> u32 {
    0xFF00_0000 | (h & 0xFF_FFFF)
}
#[inline]
pub fn chans(c: u32) -> (u32, u32, u32) {
    ((c >> 16) & 0xFF, (c >> 8) & 0xFF, c & 0xFF)
}
/// Mix two colors, t in 0..=255 (0 = a, 255 = b).
#[inline]
pub fn mix(a: u32, b: u32, t: u32) -> u32 {
    // two channels per multiply (red+blue, then green); weights sum to 256
    let t = t.min(255);
    let t = t + (t >> 7);
    let it = 256 - t;
    let rb = (((a & 0xFF00FF) * it + (b & 0xFF00FF) * t) >> 8) & 0xFF00FF;
    let g = (((a & 0x00FF00) * it + (b & 0x00FF00) * t) >> 8) & 0x00FF00;
    0xFF00_0000 | rb | g
}
pub fn lighten(c: u32, t: u32) -> u32 {
    mix(c, 0xFFFF_FFFF, t)
}
pub fn darken(c: u32, t: u32) -> u32 {
    mix(c, 0xFF00_0000, t)
}

/// Run `f(y, row)` over rows `y0..y1` of a `w`-wide pixel buffer, split across the cores when
/// the job is big enough to pay for the threads.
pub fn par_rows<F: Fn(i32, &mut [u32]) + Sync>(px: &mut [u32], w: usize, y0: i32, y1: i32, work: usize, f: F) {
    if y1 <= y0 || w == 0 {
        return;
    }
    let slice = &mut px[y0 as usize * w..y1 as usize * w];
    let rows = (y1 - y0) as usize;
    let threads = if work > 40_000 && rows >= 8 && std::env::var_os("PC_ONE_THREAD").is_none() { crate::pool::size().min(rows / 4).max(1) } else { 1 };
    if threads <= 1 {
        for (j, row) in slice.chunks_mut(w).enumerate() {
            f(y0 + j as i32, row);
        }
        return;
    }
    // interleave bands so uneven rows (rounded corners, sparse sprites) balance out
    let per = rows.div_ceil(threads * 2).max(1);
    let f = &f;
    let mut jobs: Vec<Box<dyn FnOnce() + Send + '_>> = Vec::new();
    for (ci, chunk) in slice.chunks_mut(per * w).enumerate() {
        jobs.push(Box::new(move || {
            for (j, row) in chunk.chunks_mut(w).enumerate() {
                f(y0 + (ci * per + j) as i32, row);
            }
        }));
    }
    crate::pool::run(jobs);
}

/// Blend color `c` over pixel `d` with alpha `a`, tracking coverage in the alpha byte.
#[inline]
pub fn blend_px(d: u32, c: u32, a: u32) -> u32 {
    if a >= 255 {
        return 0xFF00_0000 | (c & 0xFF_FFFF);
    }
    let da = d >> 24;
    let na = a + da * (255 - a) / 255;
    (na << 24) | (mix(d, c, a) & 0xFF_FFFF)
}

/// Logical layout size every screen is designed for.
pub const LW: i32 = 1280;
pub const LH: i32 = 720;

#[derive(Clone)]
pub struct Canvas {
    /// real pixel size
    pub w: i32,
    pub h: i32,
    pub px: Vec<u32>,
    /// layout units -> pixels
    pub k: f32,
    clip: (i32, i32, i32, i32),
    /// blurred, low-resolution copy of the scene behind the glass (see `capture_backdrop`)
    backdrop: Option<std::sync::Arc<Backdrop>>,
    /// where screen pixel (0, 0) sits in the backdrop (for a backdrop of a larger, panned image)
    bd_off: (i32, i32),
    /// draw glass as plain opaque cards (the Potato preset)
    pub flat_glass: bool,
}

/// A liquid-glass recipe (see `Canvas::glass_ex`).
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Glass {
    /// tint color and how much of it covers the scene
    pub tint: u32,
    pub tint_a: u32,
    /// brightness kept from the scene, 0..=256
    pub dark: u32,
    /// saturation of the scene in sixteenths (16 = unchanged)
    pub sat: u32,
    /// specular rim strength 0..=255
    pub rim: u32,
    /// refraction strength, in band widths
    pub lens: f32,
    /// frost grain mask (0 = none, 3 or 7 = fine grain)
    pub frost: u32,
    /// drop shadow strength (0 = none)
    pub shadow: u32,
}
impl Glass {
    /// The default recipe for a tint: `tint_a` is how dark the old flat wash was.
    pub const fn tinted(tint: u32, tint_a: u32) -> Glass {
        let a = if tint_a > 255 { 255 } else { tint_a };
        Glass { tint, tint_a: a * 3 / 4, dark: 236 - a / 2, sat: 24, rim: 220, lens: 0.9, frost: 3, shadow: 80 }
    }
    fn key(&self) -> i64 {
        ((self.tint as i64 & 0xFF_FFFF) << 32) | ((self.tint_a as i64) << 24) | ((self.dark as i64 & 0x1FF) << 12) | ((self.sat as i64 & 0x3F) << 6) | (self.frost as i64 & 0x3F)
    }
}

/// Cheap per-pixel hash for frost grain.
#[inline]
fn grain(x: i32, y: i32) -> u32 {
    let mut h = (x as u32).wrapping_mul(0x9E37_79B1) ^ (y as u32).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h >> 24
}

impl Backdrop {
    /// `wide` run through a glass recipe's vibrancy and tint (cached per recipe).
    fn tinted(&self, g: &Glass) -> std::sync::Arc<Vec<u32>> {
        let key = g.key();
        let mut c = self.tinted.lock().unwrap();
        if let Some((_, v)) = c.iter().find(|(kk, _)| *kk == key) {
            return v.clone();
        }
        let ta = g.tint_a.min(255);
        let t8 = ta + (ta >> 7);
        let it8 = 256 - t8;
        let tint = g.tint & 0xFF_FFFF;
        let trb = (tint & 0xFF00FF) * t8;
        let tg = (tint & 0x00FF00) * t8;
        let sat = g.sat as i32;
        let dark = g.dark;
        let v: Vec<u32> = self
            .wide
            .iter()
            .map(|&s| {
                // vibrancy: push colors away from their grey, then tone the light down
                let (r, gg, b) = (((s >> 16) & 255) as i32, ((s >> 8) & 255) as i32, (s & 255) as i32);
                let l = (r * 77 + gg * 150 + b * 29) >> 8;
                let f = |c: i32| -> u32 { (((l + ((c - l) * sat >> 4)).clamp(0, 255) as u32) * dark) >> 8 };
                let s = (f(r) << 16) | (f(gg) << 8) | f(b);
                ((((s & 0xFF00FF) * it8 + trb) >> 8) & 0xFF00FF) | ((((s & 0x00FF00) * it8 + tg) >> 8) & 0x00FF00)
            })
            .collect();
        let v = std::sync::Arc::new(v);
        if c.len() > 8 {
            c.clear();
        }
        c.push((key, v.clone()));
        v
    }

    /// Downsample, blur and widen an opaque image (a screen-sized canvas).
    pub fn from_image(img: &Canvas) -> std::sync::Arc<Backdrop> {
        let _p = crate::prof::scope("backdrop");
        let ds = (img.w / 320).max(2);
        let bw = ((img.w + ds - 1) / ds) as usize;
        let bh = ((img.h + ds - 1) / ds) as usize;
        // box-average each ds x ds block; one low-res row per job, rows spread over the cores
        let mut packed = vec![0u32; bw * bh];
        par_rows(&mut packed, bw, 0, bh as i32, img.w as usize * img.h as usize, |by, out| {
            for (bx, o) in out.iter_mut().enumerate() {
                let (mut r0, mut g0, mut b0, mut n) = (0u32, 0u32, 0u32, 0u32);
                for yy in (by * ds)..((by + 1) * ds).min(img.h) {
                    let row = (yy * img.w) as usize;
                    for xx in (bx as i32 * ds)..((bx as i32 + 1) * ds).min(img.w) {
                        let c = img.px[row + xx as usize];
                        r0 += (c >> 16) & 255;
                        g0 += (c >> 8) & 255;
                        b0 += c & 255;
                        n += 1;
                    }
                }
                let n = n.max(1);
                *o = ((r0 / n) << 16) | ((g0 / n) << 8) | (b0 / n);
            }
        });
        let mut buf: Vec<[u32; 3]> = packed.iter().map(|&c| [(c >> 16) & 255, (c >> 8) & 255, c & 255]).collect();
        box_blur(&mut buf, bw, bh, 2);
        box_blur(&mut buf, bw, bh, 2);
        box_blur(&mut buf, bw, bh, 2);
        // saturate a little (glass makes colors richer), pack
        let packed: Vec<u32> = buf
            .iter()
            .map(|b| {
                let l = (b[0] * 30 + b[1] * 59 + b[2] * 11) / 100;
                let sat = |c: u32| ((c as i32 + (c as i32 - l as i32) / 4).clamp(0, 255)) as u32;
                (sat(b[0]) << 16) | (sat(b[1]) << 8) | sat(b[2])
            })
            .collect();
        // widen every low-res row to full width (horizontal lerp), precomputed taps
        let w = img.w;
        let taps: Vec<(usize, usize, u32)> = (0..w)
            .map(|x| {
                let g = ((x as f32 + 0.5) / ds as f32 - 0.5).max(0.0);
                let x0 = (g as usize).min(bw - 1);
                (x0, (x0 + 1).min(bw - 1), ((g - x0 as f32) * 255.0) as u32)
            })
            .collect();
        let mut wide = vec![0u32; bh * w as usize];
        par_rows(&mut wide, w as usize, 0, bh as i32, bh * w as usize, |by, dst| {
            let src = &packed[by as usize * bw..(by as usize + 1) * bw];
            for (d, &(x0, x1, t)) in dst.iter_mut().zip(taps.iter()) {
                *d = mix(src[x0], src[x1], t) & 0xFF_FFFF;
            }
        });
        let id = BACKDROP_IDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        std::sync::Arc::new(Backdrop { ds, rows: bh as i32, wide, w, tinted: std::sync::Mutex::new(Vec::new()), id })
    }
}

/// What liquid glass refracts: the scene, downsampled, blurred and slightly saturated once per
/// frame, then widened to full width so a glass pixel is one vertical lerp.
pub struct Backdrop {
    ds: i32,
    /// low-res row count
    rows: i32,
    /// `rows` rows, each `w` pixels wide (horizontally interpolated)
    wide: Vec<u32>,
    w: i32,
    /// `wide` pre-tinted, per (tint, alpha): glass interiors are then one vertical blend
    tinted: std::sync::Mutex<Vec<(i64, std::sync::Arc<Vec<u32>>)>>,
    /// unique per backdrop, so cached glass can never match a recycled allocation
    id: u64,
}

static BACKDROP_IDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Rendered glass panels and shadow masks, reused while nothing about them changes.
/// Glass reads only its backdrop (never the live canvas), so a cached panel is exact.
#[derive(Default)]
struct ShapeCache {
    glass: std::collections::HashMap<[i64; 12], std::sync::Arc<Vec<u32>>>,
    shadows: std::collections::HashMap<[i32; 5], std::sync::Arc<Vec<u8>>>,
    /// per-shape edge geometry of glass (independent of what is behind it)
    geo: std::collections::HashMap<[i64; 6], std::sync::Arc<GlassGeo>>,
    pixels: usize,
}

/// One edge pixel of a glass shape: where it samples (relative to itself), its red/blue
/// split, how much of the clear rim shows, its bevel shade, its highlight and its coverage.
#[derive(Clone, Copy)]
struct GeoPx {
    lx: i32,
    ox: f32,
    oy: f32,
    cx: f32,
    cy: f32,
    clear: u32,
    shade: u32,
    add: u32,
    cov: u32,
    plain: bool,
}
/// The edge pixels of a glass shape, row by row (`rows[j]` is a range into `px`).
struct GlassGeo {
    rows: Vec<(u32, u32)>,
    px: Vec<GeoPx>,
}
thread_local! {
    static SHAPES: std::cell::RefCell<ShapeCache> = std::cell::RefCell::new(ShapeCache::default());
}
fn shape_cache_room(add: usize) {
    SHAPES.with(|c| {
        let mut c = c.borrow_mut();
        if c.pixels + add > 12_000_000 {
            c.glass.clear();
            c.shadows.clear();
            c.geo.clear();
            c.pixels = 0;
        }
        c.pixels += add;
    });
}

/// Packed saturating add of `a` (0..255) to each of R, G, B.
#[inline]
pub fn add_sat(c: u32, a: u32) -> u32 {
    let rb = (c & 0xFF00FF) + a * 0x0001_0001;
    let m = rb & 0x0100_0100;
    let rb = (rb | (m - (m >> 8))) & 0x00FF_00FF;
    let g = (c & 0x00FF00) + a * 0x100;
    let m = g & 0x0001_0000;
    let g = (g | (m - (m >> 8))) & 0x0000_FF00;
    rb | g
}
/// Packed lerp, `t` in 0..=256 (0 = a); the result has a zero top byte.
#[inline]
pub fn lerp_px(a: u32, b: u32, t: u32) -> u32 {
    let it = 256 - t;
    let rb = (((a & 0xFF00FF) * it + (b & 0xFF00FF) * t) >> 8) & 0xFF00FF;
    let g = (((a & 0x00FF00) * it + (b & 0x00FF00) * t) >> 8) & 0x00FF00;
    rb | g
}
/// Packed lerp with precomputed weights (`wa + wb == 256`).
#[inline]
fn lerp_w(a: u32, b: u32, wa: u32, wb: u32) -> u32 {
    let rb = (((a & 0xFF00FF) * wa + (b & 0xFF00FF) * wb) >> 8) & 0xFF00FF;
    let g = (((a & 0x00FF00) * wa + (b & 0x00FF00) * wb) >> 8) & 0x00FF00;
    rb | g
}

impl Canvas {
    /// A pixel canvas (k = 1): sprites, glyphs, offscreen work.
    pub fn new(w: i32, h: i32) -> Self {
        Canvas { w, h, px: vec![0; (w.max(1) * h.max(1)) as usize], k: 1.0, clip: (0, 0, w, h), backdrop: None, bd_off: (0, 0), flat_glass: false }
    }
    pub fn transparent(w: i32, h: i32) -> Self {
        Canvas::new(w, h)
    }
    /// A screen canvas: the 1280x720 layout rendered `w` real pixels wide (16:9).
    pub fn screen(w: i32) -> Self {
        let w = w.max(320);
        let h = (w * LH + LW / 2) / LW;
        let mut c = Canvas::new(w, h);
        c.k = w as f32 / LW as f32;
        c
    }

    // ------------------------------------------------------------ unit conversion
    /// Layout units to pixels (edges round consistently, so neighbours never leave gaps).
    #[inline]
    pub fn u(&self, v: i32) -> i32 {
        if self.k == 1.0 {
            v
        } else {
            (v as f32 * self.k).round() as i32
        }
    }
    /// Pixel size of a layout length (at least 1 when the length is positive).
    #[inline]
    pub fn us(&self, v: i32) -> i32 {
        if v <= 0 {
            0
        } else {
            ((v as f32 * self.k).round() as i32).max(1)
        }
    }
    #[inline]
    fn edges(&self, x: i32, y: i32, w: i32, h: i32) -> (i32, i32, i32, i32) {
        let x0 = self.u(x);
        let y0 = self.u(y);
        (x0, y0, self.u(x + w) - x0, self.u(y + h) - y0)
    }

    pub fn set_clip(&mut self, x: i32, y: i32, w: i32, h: i32) {
        let (x, y, w, h) = self.edges(x, y, w, h);
        let x0 = x.max(0);
        let y0 = y.max(0);
        let x1 = (x + w).min(self.w);
        let y1 = (y + h).min(self.h);
        self.clip = (x0, y0, x1.max(x0), y1.max(y0));
    }
    pub fn reset_clip(&mut self) {
        self.clip = (0, 0, self.w, self.h);
    }
    pub fn clear(&mut self, c: u32) {
        self.px.fill(c);
    }

    /// Blend one pixel with alpha a (0..255). Keeps track of coverage in the alpha byte.
    #[inline]
    pub fn blend(&mut self, x: i32, y: i32, c: u32, a: u32) {
        if x < self.clip.0 || y < self.clip.1 || x >= self.clip.2 || y >= self.clip.3 || a == 0 {
            return;
        }
        let i = (y * self.w + x) as usize;
        let d = self.px[i];
        if a >= 255 {
            self.px[i] = 0xFF00_0000 | (c & 0xFF_FFFF);
            return;
        }
        let da = d >> 24;
        let na = a + da * (255 - a) / 255;
        let m = mix(d, c, a) & 0xFF_FFFF;
        self.px[i] = (na << 24) | m;
    }
    #[inline]
    pub fn add(&mut self, x: i32, y: i32, c: u32, a: u32) {
        if x < self.clip.0 || y < self.clip.1 || x >= self.clip.2 || y >= self.clip.3 || a == 0 {
            return;
        }
        let i = (y * self.w + x) as usize;
        let d = self.px[i];
        let (dr, dg, db) = chans(d);
        let (sr, sg, sb) = chans(c);
        let r = (dr + sr * a / 255).min(255);
        let g = (dg + sg * a / 255).min(255);
        let b = (db + sb * a / 255).min(255);
        self.px[i] = (d & 0xFF00_0000) | (r << 16) | (g << 8) | b;
    }

    // ------------------------------------------------------------ rectangles
    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32) {
        let (x, y, w, h) = self.edges(x, y, w, h);
        self.rect_px(x, y, w, h, c);
    }
    pub fn rect_px(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32) {
        let _p = crate::prof::scope("rect");
        let x0 = x.max(self.clip.0);
        let y0 = y.max(self.clip.1);
        let x1 = (x + w).min(self.clip.2);
        let y1 = (y + h).min(self.clip.3);
        let c = 0xFF00_0000 | (c & 0xFF_FFFF);
        if x1 <= x0 {
            return;
        }
        for yy in y0..y1 {
            let row = (yy * self.w) as usize;
            self.px[row + x0 as usize..row + x1 as usize].fill(c);
        }
    }
    pub fn rect_a(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32, a: u32) {
        let (x, y, w, h) = self.edges(x, y, w, h);
        self.rect_a_px(x, y, w, h, c, a);
    }
    pub fn rect_a_px(&mut self, x: i32, y: i32, w: i32, h: i32, c: u32, a: u32) {
        let _p = crate::prof::scope("rect_a");
        if a >= 255 {
            return self.rect_px(x, y, w, h, c);
        }
        if a == 0 {
            return;
        }
        let x0 = x.max(self.clip.0);
        let y0 = y.max(self.clip.1);
        let x1 = (x + w).min(self.clip.2);
        let y1 = (y + h).min(self.clip.3);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let t = a + (a >> 7);
        let it = 256 - t;
        let crb = (c & 0xFF00FF) * t;
        let cg = (c & 0x00FF00) * t;
        let w = self.w as usize;
        let work = ((x1 - x0) * (y1 - y0)) as usize;
        par_rows(&mut self.px, w, y0, y1, work, |_, row| {
            for p in &mut row[x0 as usize..x1 as usize] {
                let d = *p;
                let da = d >> 24;
                let na = a + da * (255 - a) / 255;
                let rb = (((d & 0xFF00FF) * it + crb) >> 8) & 0xFF00FF;
                let g = (((d & 0x00FF00) * it + cg) >> 8) & 0x00FF00;
                *p = (na << 24) | rb | g;
            }
        });
    }
    pub fn frame(&mut self, x: i32, y: i32, w: i32, h: i32, t: i32, c: u32, a: u32) {
        let (x, y, w, h) = self.edges(x, y, w, h);
        let t = self.us(t);
        self.rect_a_px(x, y, w, t, c, a);
        self.rect_a_px(x, y + h - t, w, t, c, a);
        self.rect_a_px(x, y + t, t, h - 2 * t, c, a);
        self.rect_a_px(x + w - t, y + t, t, h - 2 * t, c, a);
    }

    /// Rounded rectangle with anti-aliased corners.
    pub fn round_rect(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, c: u32, a: u32) {
        let (x, y, w, h) = self.edges(x, y, w, h);
        let r = self.u(r);
        self.round_grad_px(x, y, w, h, r, c, c, a);
    }

    /// Vertical gradient inside a rounded rect.
    pub fn round_grad(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, top: u32, bot: u32, a: u32) {
        let (x, y, w, h) = self.edges(x, y, w, h);
        let r = self.u(r);
        self.round_grad_px(x, y, w, h, r, top, bot, a);
    }
    #[allow(clippy::too_many_arguments)]
    pub fn round_grad_px(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, top: u32, bot: u32, a: u32) {
        self.round_ring_px(x, y, w, h, r, 0, top, bot, a);
    }
    /// Rounded rectangle (gradient) that only fills a `ring`-pixel border when `ring > 0`
    /// (for layered frames whose middle is covered anyway). Rows run in parallel.
    #[allow(clippy::too_many_arguments)]
    pub fn round_ring_px(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, ring: i32, top: u32, bot: u32, a: u32) {
        let _p = crate::prof::scope("round_rect");
        if w <= 0 || h <= 0 || a == 0 {
            return;
        }
        let r = r.min(w / 2).min(h / 2).max(0);
        let rf = r as f32;
        let y0 = y.max(self.clip.1);
        let y1 = (y + h).min(self.clip.3);
        let (cx0, cx1) = (self.clip.0, self.clip.2);
        let sw = self.w as usize;
        let work = if ring > 0 { (2 * (w + h) * ring) as usize } else { (w * h) as usize };
        par_rows(&mut self.px, sw, y0, y1, work, |py, row| {
            let yy = py - y;
            let c = if top == bot { top } else { mix(top, bot, (yy * 255 / h.max(1)) as u32) };
            let fill = |row: &mut [u32], xa: i32, xb: i32| {
                let (xa, xb) = (xa.max(cx0), xb.min(cx1));
                for p in row.iter_mut().take(xb.max(xa) as usize).skip(xa as usize) {
                    *p = blend_px(*p, c, a);
                }
            };
            let dy = if yy < r {
                rf - yy as f32 - 0.5
            } else if yy >= h - r {
                yy as f32 + 0.5 - (h - r) as f32
            } else {
                -1.0
            };
            let hollow = ring > 0 && yy >= ring && yy < h - ring;
            if dy < 0.0 || r == 0 {
                if hollow {
                    fill(row, x, x + ring);
                    fill(row, x + w - ring, x + w);
                } else {
                    fill(row, x, x + w);
                }
                return;
            }
            // the straight middle of the row, then the two anti-aliased corner spans
            if hollow {
                fill(row, x + r, x + ring.max(r));
                fill(row, x + w - ring.max(r), x + w - r);
            } else {
                fill(row, x + r, x + w - r);
            }
            for xx in (0..r).chain(w - r..w) {
                let px = x + xx;
                if px < cx0 || px >= cx1 {
                    continue;
                }
                let dx = if xx < r { rf - xx as f32 - 0.5 } else { xx as f32 + 0.5 - (w - r) as f32 };
                let d = (dx * dx + dy * dy).sqrt();
                let mut cov = (rf - d + 0.5).clamp(0.0, 1.0);
                if ring > 0 {
                    // a true band: take away the inner shape (inset by `ring`, same corner centre)
                    let inner = if ring < r {
                        ((rf - ring as f32) - d + 0.5).clamp(0.0, 1.0)
                    } else if xx >= ring && xx < w - ring && yy >= ring && yy < h - ring {
                        1.0
                    } else {
                        0.0
                    };
                    cov = (cov - inner).max(0.0);
                }
                if cov > 0.0 {
                    let p = &mut row[px as usize];
                    *p = blend_px(*p, c, (cov * a as f32) as u32);
                }
            }
        });
    }
    /// Layout-unit ring version of `round_grad` (see `round_ring_px`).
    #[allow(clippy::too_many_arguments)]
    pub fn round_ring(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, ring: i32, top: u32, bot: u32, a: u32) {
        let (x, y, w, h) = self.edges(x, y, w, h);
        let r = self.u(r);
        let ring = self.us(ring) + 1;
        self.round_ring_px(x, y, w, h, r, ring, top, bot, a);
    }

    /// An anti-aliased outline of a rounded rectangle, `thick` layout units wide, its color
    /// running from `top` to `bot` (a lit rim: bright above, darker below).
    #[allow(clippy::too_many_arguments)]
    pub fn rim(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, thick: f32, top: u32, bot: u32, a: u32) {
        let _p = crate::prof::scope("round_rect");
        let (x, y, w, h) = self.edges(x, y, w, h);
        if w <= 2 || h <= 2 || a == 0 {
            return;
        }
        let r = self.u(r).min(w / 2).min(h / 2).max(0);
        let th = (thick * self.k).max(0.8);
        let rf = r as f32;
        let (wf, hf) = (w as f32, h as f32);
        let y0 = y.max(self.clip.1);
        let y1 = (y + h).min(self.clip.3);
        let (cx0, cx1) = (self.clip.0, self.clip.2);
        let band = th.ceil() as i32 + 1;
        par_rows(&mut self.px, self.w as usize, y0, y1, (2 * (w + h) * band) as usize, |py, row| {
            let ly = (py - y) as f32 + 0.5;
            let c = mix(top, bot, ((ly / hf) * 255.0) as u32);
            let qy = (ly - hf / 2.0).abs() - (hf / 2.0 - rf);
            // only the straight top and bottom edges need whole rows; elsewhere the outline lies
            // within the corner radius (plus its width) of either end
            let edge_row = ly < th + 1.5 || ly > hf - th - 1.5;
            let side = (r + band).min(w / 2);
            let spans: [(i32, i32); 2] = if edge_row { [(x, x + w), (0, 0)] } else { [(x, x + side), (x + w - side, x + w)] };
            for (a0, a1) in spans {
                for px in a0.max(cx0)..a1.min(cx1) {
                    let lx = (px - x) as f32 + 0.5;
                    let qx = (lx - wf / 2.0).abs() - (wf / 2.0 - rf);
                    let d = if qx > 0.0 && qy > 0.0 { rf - (qx * qx + qy * qy).sqrt() } else { rf - qx.max(qy) };
                    let cov = (d + 0.5).clamp(0.0, 1.0) - (d - th + 0.5).clamp(0.0, 1.0);
                    if cov > 0.0 {
                        let p = &mut row[px as usize];
                        *p = blend_px(*p, c, (cov * a as f32) as u32);
                    }
                }
            }
        });
    }

    pub fn vgrad(&mut self, x: i32, y: i32, w: i32, h: i32, top: u32, bot: u32) {
        let (x, y, w, h) = self.edges(x, y, w, h);
        for yy in 0..h {
            let c = mix(top, bot, (yy * 255 / h.max(1)) as u32);
            self.rect_px(x, y + yy, w, 1, c);
        }
    }

    // ------------------------------------------------------------ round shapes (layout units)
    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, c: u32, a: u32) {
        let k = self.k;
        self.circle_px(cx * k, cy * k, r * k, c, a);
    }
    pub fn circle_px(&mut self, cx: f32, cy: f32, r: f32, c: u32, a: u32) {
        let _p = crate::prof::scope("circle");
        if r <= 0.0 || a == 0 {
            return;
        }
        let x0 = ((cx - r - 1.0).floor() as i32).max(self.clip.0);
        let x1 = ((cx + r + 1.0).ceil() as i32).min(self.clip.2 - 1);
        let y0 = ((cy - r - 1.0).floor() as i32).max(self.clip.1);
        let y1 = ((cy + r + 1.0).ceil() as i32).min(self.clip.3 - 1);
        for y in y0..=y1 {
            let dy = y as f32 + 0.5 - cy;
            for x in x0..=x1 {
                let dx = x as f32 + 0.5 - cx;
                let d = (dx * dx + dy * dy).sqrt();
                let cov = (r - d + 0.5).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend(x, y, c, (cov * a as f32) as u32);
                }
            }
        }
    }
    pub fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, c: u32, a: u32) {
        let k = self.k;
        self.ellipse_px(cx * k, cy * k, rx * k, ry * k, c, a);
    }
    pub fn ellipse_px(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, c: u32, a: u32) {
        let _p = crate::prof::scope("ellipse");
        if rx <= 0.0 || ry <= 0.0 || a == 0 {
            return;
        }
        let x0 = ((cx - rx - 1.0).floor() as i32).max(self.clip.0);
        let x1 = ((cx + rx + 1.0).ceil() as i32).min(self.clip.2 - 1);
        let y0 = ((cy - ry - 1.0).floor() as i32).max(self.clip.1);
        let y1 = ((cy + ry + 1.0).ceil() as i32).min(self.clip.3 - 1);
        let rm = rx.min(ry);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let dx = (x as f32 + 0.5 - cx) / rx;
                let dy = (y as f32 + 0.5 - cy) / ry;
                let d = (dx * dx + dy * dy).sqrt();
                let cov = ((1.0 - d) * rm + 0.5).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend(x, y, c, (cov * a as f32) as u32);
                }
            }
        }
    }
    pub fn ring(&mut self, cx: f32, cy: f32, r: f32, thick: f32, c: u32, a: u32) {
        let k = self.k;
        self.ring_px(cx * k, cy * k, r * k, (thick * k).max(1.0), c, a);
    }
    pub fn ring_px(&mut self, cx: f32, cy: f32, r: f32, thick: f32, c: u32, a: u32) {
        let _p = crate::prof::scope("ring");
        if a == 0 {
            return;
        }
        let x0 = ((cx - r - thick - 1.0).floor() as i32).max(self.clip.0);
        let x1 = ((cx + r + thick + 1.0).ceil() as i32).min(self.clip.2 - 1);
        let y0 = ((cy - r - thick - 1.0).floor() as i32).max(self.clip.1);
        let y1 = ((cy + r + thick + 1.0).ceil() as i32).min(self.clip.3 - 1);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let dx = x as f32 + 0.5 - cx;
                let dy = y as f32 + 0.5 - cy;
                let d = ((dx * dx + dy * dy).sqrt() - r).abs();
                let cov = (thick / 2.0 - d + 0.5).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend(x, y, c, (cov * a as f32) as u32);
                }
            }
        }
    }
    /// Additive soft glow (quadratic falloff).
    pub fn glow(&mut self, cx: f32, cy: f32, r: f32, c: u32, a: u32) {
        let k = self.k;
        self.glow_px(cx * k, cy * k, r * k, c, a);
    }
    pub fn glow_px(&mut self, cx: f32, cy: f32, r: f32, c: u32, a: u32) {
        let _p = crate::prof::scope("glow");
        if r < 1.0 || a == 0 {
            return;
        }
        let x0 = ((cx - r).floor() as i32).max(self.clip.0);
        let x1 = ((cx + r).ceil() as i32).min(self.clip.2 - 1);
        let y0 = ((cy - r).floor() as i32).max(self.clip.1);
        let y1 = ((cy + r).ceil() as i32).min(self.clip.3 - 1);
        let inv = 1.0 / (r * r);
        for y in y0..=y1 {
            let dy = y as f32 + 0.5 - cy;
            for x in x0..=x1 {
                let dx = x as f32 + 0.5 - cx;
                let t = 1.0 - (dx * dx + dy * dy) * inv;
                if t > 0.0 {
                    self.add(x, y, c, (t * t * a as f32) as u32);
                }
            }
        }
    }
    /// Anti-aliased capsule-shaped line (distance to a segment), `thick` wide.
    pub fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, thick: f32, c: u32, a: u32) {
        let k = self.k;
        self.line_px(x0 * k, y0 * k, x1 * k, y1 * k, (thick * k).max(1.0), c, a);
    }
    #[allow(clippy::too_many_arguments)]
    pub fn line_px(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, thick: f32, c: u32, a: u32) {
        let _p = crate::prof::scope("line");
        if a == 0 {
            return;
        }
        let hr = thick / 2.0;
        let bx0 = ((x0.min(x1) - hr - 1.0).floor() as i32).max(self.clip.0);
        let bx1 = ((x0.max(x1) + hr + 1.0).ceil() as i32).min(self.clip.2 - 1);
        let by0 = ((y0.min(y1) - hr - 1.0).floor() as i32).max(self.clip.1);
        let by1 = ((y0.max(y1) + hr + 1.0).ceil() as i32).min(self.clip.3 - 1);
        let (dx, dy) = (x1 - x0, y1 - y0);
        let ll = (dx * dx + dy * dy).max(1e-6);
        for y in by0..=by1 {
            for x in bx0..=bx1 {
                let (px, py) = (x as f32 + 0.5 - x0, y as f32 + 0.5 - y0);
                let t = ((px * dx + py * dy) / ll).clamp(0.0, 1.0);
                let (ex, ey) = (px - dx * t, py - dy * t);
                let d = (ex * ex + ey * ey).sqrt();
                let cov = (hr - d + 0.5).clamp(0.0, 1.0);
                if cov > 0.0 {
                    self.blend(x, y, c, (cov * a as f32) as u32);
                }
            }
        }
    }
    pub fn tri(&mut self, p: [(f32, f32); 3], c: u32, a: u32) {
        let k = self.k;
        self.tri_px([(p[0].0 * k, p[0].1 * k), (p[1].0 * k, p[1].1 * k), (p[2].0 * k, p[2].1 * k)], c, a);
    }
    /// Filled triangle with 4x4 supersampled edges.
    pub fn tri_px(&mut self, p: [(f32, f32); 3], c: u32, a: u32) {
        let _p = crate::prof::scope("tri");
        let minx = (p.iter().map(|q| q.0).fold(f32::MAX, f32::min).floor() as i32).max(self.clip.0);
        let maxx = (p.iter().map(|q| q.0).fold(f32::MIN, f32::max).ceil() as i32).min(self.clip.2 - 1);
        let miny = (p.iter().map(|q| q.1).fold(f32::MAX, f32::min).floor() as i32).max(self.clip.1);
        let maxy = (p.iter().map(|q| q.1).fold(f32::MIN, f32::max).ceil() as i32).min(self.clip.3 - 1);
        let edge = |a: (f32, f32), b: (f32, f32), x: f32, y: f32| (b.0 - a.0) * (y - a.1) - (b.1 - a.1) * (x - a.0);
        let area = edge(p[0], p[1], p[2].0, p[2].1);
        if area.abs() < 0.01 || a == 0 {
            return;
        }
        let inside = |fx: f32, fy: f32| {
            let w0 = edge(p[1], p[2], fx, fy) / area;
            let w1 = edge(p[2], p[0], fx, fy) / area;
            let w2 = edge(p[0], p[1], fx, fy) / area;
            w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0
        };
        for y in miny..=maxy {
            for x in minx..=maxx {
                let (fx, fy) = (x as f32, y as f32);
                // fast path: all four corners inside means full coverage
                let corners = inside(fx, fy) as u32 + inside(fx + 1.0, fy) as u32 + inside(fx, fy + 1.0) as u32 + inside(fx + 1.0, fy + 1.0) as u32;
                let cov = if corners == 4 {
                    16
                } else {
                    let mut n = 0;
                    for sy in 0..4 {
                        for sx in 0..4 {
                            if inside(fx + (sx as f32 + 0.5) / 4.0, fy + (sy as f32 + 0.5) / 4.0) {
                                n += 1;
                            }
                        }
                    }
                    n
                };
                if cov > 0 {
                    self.blend(x, y, c, a * cov / 16);
                }
            }
        }
    }
    pub fn star(&mut self, cx: f32, cy: f32, r: f32, c: u32, a: u32) {
        let mut pts = [(0.0f32, 0.0f32); 10];
        for (i, p) in pts.iter_mut().enumerate() {
            let ang = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
            let rr = if i % 2 == 0 { r } else { r * 0.45 };
            *p = (cx + ang.cos() * rr, cy + ang.sin() * rr);
        }
        // the inner pentagon, then the five points (no overlapping seams)
        for i in 0..5 {
            self.tri([(cx, cy), pts[2 * i + 1], pts[(2 * i + 3) % 10]], c, a);
            self.tri([pts[2 * i + 1], pts[(2 * i + 2) % 10], pts[(2 * i + 3) % 10]], c, a);
        }
    }

    /// Soft drop shadow for a rounded rectangle (layout units).
    pub fn soft_shadow(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, spread: i32, alpha: u32) {
        let (x, y, w, h) = self.edges(x, y, w, h);
        let r = self.u(r);
        let spread = self.us(spread).max(1);
        self.soft_shadow_px(x, y, w, h, r, spread, alpha);
    }
    /// The shadow is only drawn outside the shape (whatever casts it covers the inside), as a
    /// smooth falloff of the distance to the shape moved down by half the spread.
    #[allow(clippy::too_many_arguments)]
    fn soft_shadow_px(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, spread: i32, alpha: u32) {
        let _p = crate::prof::scope("shadow");
        if w <= 0 || h <= 0 {
            return;
        }
        let off = spread / 2;
        // the mask covers (x - spread, y + off - spread) .. (x + w + spread, y + off + h + spread)
        let (mw, mh) = (w + 2 * spread, h + 2 * spread);
        let key = [w, h, r, spread, alpha as i32];
        let mask = match SHAPES.with(|c| c.borrow().shadows.get(&key).cloned()) {
            Some(m) => m,
            None => {
                let sf = spread as f32;
                let rr = r.min(w / 2).min(h / 2).max(0) as f32;
                let (hw, hh) = (w as f32 / 2.0, h as f32 / 2.0);
                // shape centre in mask coordinates
                let (cx, cy) = (spread as f32 + hw, spread as f32 + hh);
                let lut: Vec<u8> = (0..=(spread * 4))
                    .map(|i| {
                        let f = (1.0 - (i as f32 / 4.0) / sf).max(0.0);
                        (alpha as f32 * f * f).min(255.0) as u8
                    })
                    .collect();
                let mut m = vec![0u8; (mw * mh) as usize];
                for my in 0..mh {
                    let py = my as f32 + 0.5;
                    let qy = (py - cy).abs() - (hh - rr);
                    for mx in 0..mw {
                        let qx = (mx as f32 + 0.5 - cx).abs() - (hw - rr);
                        let d = if qx > 0.0 && qy > 0.0 { (qx * qx + qy * qy).sqrt() - rr } else { qx.max(qy) - rr };
                        // nothing under the caster itself (its round corners stay clear)
                        let qcy = (py - (cy - off as f32)).abs() - (hh - rr);
                        let dc = if qx > 0.0 && qcy > 0.0 { (qx * qx + qcy * qcy).sqrt() - rr } else { qx.max(qcy) - rr };
                        if d < sf && dc > -0.5 {
                            m[(my * mw + mx) as usize] = lut[((d.max(0.0) * 4.0) as usize).min(lut.len() - 1)];
                        }
                    }
                }
                let m = std::sync::Arc::new(m);
                shape_cache_room((mw * mh) as usize / 4);
                SHAPES.with(|c| {
                    c.borrow_mut().shadows.insert(key, m.clone());
                });
                m
            }
        };
        let (ox, oy) = (x - spread, y + off - spread);
        let y0 = oy.max(self.clip.1);
        let y1 = (oy + mh).min(self.clip.3);
        let (cx0, cx1) = (self.clip.0, self.clip.2);
        let sw = self.w as usize;
        let work = (2 * (w + h) * spread) as usize;
        par_rows(&mut self.px, sw, y0, y1, work, |py, row| {
            let my = py - oy;
            let mrow = &mask[(my * mw) as usize..((my + 1) * mw) as usize];
            // the caster covers its own rectangle: only the side bands show in its rows
            let inner = py >= y + r && py < y + h - r;
            let spans: [(i32, i32); 2] = if inner { [(0, spread), (w + spread, mw)] } else { [(0, mw), (0, 0)] };
            for (a0, a1) in spans {
                let a0 = a0.max(cx0 - ox);
                let a1 = a1.min(cx1 - ox);
                for mx in a0..a1 {
                    let a = mrow[mx as usize] as u32;
                    if a > 0 {
                        let p = &mut row[(ox + mx) as usize];
                        *p = blend_px(*p, 0xFF00_0000, a);
                    }
                }
            }
        });
    }

    /// Snapshot the scene for the glass that follows (call after drawing a screen's backdrop).
    /// Glass drawn without a snapshot takes one itself.
    pub fn capture_backdrop(&mut self) {
        self.backdrop = Some(Backdrop::from_image(self));
        self.bd_off = (0, 0);
    }
    /// Use a prepared backdrop (static scenes build theirs once and reuse it every frame).
    pub fn set_backdrop(&mut self, b: std::sync::Arc<Backdrop>) {
        self.set_backdrop_at(b, 0, 0);
    }
    /// A backdrop of a larger image of which the screen shows the part at (ox, oy).
    pub fn set_backdrop_at(&mut self, b: std::sync::Arc<Backdrop>, ox: i32, oy: i32) {
        self.backdrop = Some(b);
        self.bd_off = (ox, oy);
    }
    pub fn drop_backdrop(&mut self) {
        self.backdrop = None;
        self.bd_off = (0, 0);
    }

    /// Liquid glass with the default recipe for a tint (kept for older call sites).
    pub fn glass(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, tint: u32, tint_a: u32) {
        self.glass_ex(x, y, w, h, r, &Glass::tinted(tint, tint_a));
    }

    /// Liquid glass: a thick, clear slab over the blurred scene.
    /// - vibrancy: what is behind is saturated and toned down (not washed flat) and then tinted;
    /// - refraction: near the rim the slab bends the scene, sampling it from just outside the
    ///   edge and compressing it (a lens), with a slight red/blue split right at the edge;
    /// - light: a specular rim that follows a top-left light (a fainter bounce bottom-right), a
    ///   soft inner glow under it, a darker thickness band inside the edge, a top sheen and a
    ///   fine frost grain.
    ///
    /// Panels are shaded once into an image and cached while their backdrop and shape are the
    /// same; rows are shaded in parallel.
    pub fn glass_ex(&mut self, x: i32, y: i32, w: i32, h: i32, r: i32, g: &Glass) {
        let _p = crate::prof::scope("glass");
        if w <= 4 || h <= 4 {
            return;
        }
        if g.shadow > 0 {
            self.soft_shadow(x, y + 2, w, h, r, 14, g.shadow);
        }
        if self.flat_glass {
            // cheap look (Potato): an opaque card with a lit top edge and a dark lip
            let base = mix(g.tint, 0xFF10_1528, 80);
            self.round_grad(x, y, w, h, r, lighten(base, 22), base, 246);
            self.rim(x, y, w, h, r, 1.0, 0xFFFF_FFFF, 0xFF00_0000, 70);
            return;
        }
        let k = self.k;
        let (x, y, w, h) = self.edges(x, y, w, h);
        let r = self.u(r).min(w / 2).min(h / 2);
        let x0 = x.max(self.clip.0);
        let y0 = y.max(self.clip.1);
        let x1 = (x + w).min(self.clip.2);
        let y1 = (y + h).min(self.clip.3);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        if self.backdrop.is_none() {
            self.capture_backdrop();
        }
        let bd = self.backdrop.clone().unwrap();
        let (box_, boy) = self.bd_off;
        let (cw, ch) = (x1 - x0, y1 - y0);
        let key: [i64; 12] = [
            x as i64,
            y as i64,
            w as i64,
            h as i64,
            r as i64,
            g.key(),
            ((g.rim as i64) << 32) | (g.lens * 16.0) as i64,
            bd.id as i64,
            box_ as i64,
            boy as i64,
            ((x0 as i64) << 32) | y0 as i64,
            ((x1 as i64) << 32) | y1 as i64,
        ];
        if let Some(img) = SHAPES.with(|c| c.borrow().glass.get(&key).cloned()) {
            self.composite(&img, x0, y0, cw, ch);
            return;
        }
        let rf = r as f32;
        let (wf, hf) = (w as f32, h as f32);
        // the refracting band: wide on big panels, narrower on small pills
        let edge = (16.0 * k).min(hf * 0.36).min(wf * 0.36).max(2.0);
        let lens = edge * g.lens;
        // normals come from a rounder copy of the shape so they turn smoothly
        let rn = rf.max(edge * 1.1).min(wf / 2.0).min(hf / 2.0);
        let ca = 1.6 * k;
        let bw = bd.w as usize;
        let tinted = bd.tinted(g);
        let tw: &[u32] = &tinted;
        // the rim concentrates light: there the scene comes through brighter and less tinted
        let clear = bd.tinted(&Glass { tint_a: g.tint_a / 3, dark: ((g.dark + 300) / 2).min(256), sat: g.sat + 6, ..*g });
        let cw_: &[u32] = &clear;
        let rows = bd.rows as usize;
        let ds = bd.ds as f32;
        let tap = |sx: f32, sy: f32| -> u32 {
            let sxi = (sx as i32 + box_).clamp(0, bd.w - 1) as usize;
            let gy = ((sy + boy as f32 + 0.5) / ds - 0.5).max(0.0);
            let r0 = (gy as usize).min(rows - 1);
            let r1 = (r0 + 1).min(rows - 1);
            let wb = (((gy - r0 as f32) * 256.0) as u32).min(256);
            lerp_w(tw[r0 * bw + sxi], tw[r1 * bw + sxi], 256 - wb, wb)
        };
        let tap_clear = |sx: f32, sy: f32| -> u32 {
            let sxi = (sx as i32 + box_).clamp(0, bd.w - 1) as usize;
            let gy = ((sy + boy as f32 + 0.5) / ds - 0.5).max(0.0);
            let r0 = (gy as usize).min(rows - 1);
            let r1 = (r0 + 1).min(rows - 1);
            let wb = (((gy - r0 as f32) * 256.0) as u32).min(256);
            lerp_w(cw_[r0 * bw + sxi], cw_[r1 * bw + sxi], 256 - wb, wb)
        };
        // light from the top left
        let (lx_, ly_) = (-0.55f32, -0.835f32);
        let rim_k = g.rim as f32 / 255.0;
        let hl_w = 1.5 * k;
        let glow_w = 9.0 * k;
        let frost = g.frost;
        let gkey: [i64; 6] = [w as i64, h as i64, r as i64, (k * 1000.0) as i64, g.rim as i64, (g.lens * 1000.0) as i64];
        let geo = match SHAPES.with(|c| c.borrow().geo.get(&gkey).cloned()) {
            Some(gg) => gg,
            None => {
                let mut rows = Vec::with_capacity(h as usize);
                let mut pxs: Vec<GeoPx> = Vec::new();
                for j in 0..h {
                    let start = pxs.len() as u32;
                    let ly = j as f32 + 0.5;
                    let qy = (ly - hf / 2.0).abs() - (hf / 2.0 - rf);
                    let qny = (ly - hf / 2.0).abs() - (hf / 2.0 - rn);
                    let sheen_row = (1.0 - ly / (hf * 0.5).max(1.0)).max(0.0);
                    let sheen_row = sheen_row * sheen_row * 26.0;
                    // columns that can be edge pixels on this row
                    let inner = ly > edge + 1.0 && ly < hf - edge - 1.0;
                    let corner_rows = ly < rf.max(edge) || ly > hf - rf.max(edge);
                    let span = if inner { (if corner_rows { edge.max(rf) } else { edge } + 2.0).ceil() as i32 } else { w };
                    let cols: Vec<i32> = if span * 2 >= w { (0..w).collect() } else { (0..span).chain(w - span..w).collect() };
                    for lxi in cols {
                        let lx = lxi as f32 + 0.5;
                        let qx = (lx - wf / 2.0).abs() - (wf / 2.0 - rf);
                        let d = if qx > 0.0 && qy > 0.0 { rf - (qx * qx + qy * qy).sqrt() } else { rf - qx.max(qy) };
                        if d <= -0.5 {
                            continue;
                        }
                        let cov = (d + 0.5).min(1.0);
                        let d = d.max(0.0);
                        let sheen = sheen_row * (1.15 - 0.5 * lx / wf);
                        let cova = if cov >= 1.0 { 255 } else { (cov * 255.0) as u32 };
                        if d >= edge {
                            pxs.push(GeoPx { lx: lxi, ox: 0.0, oy: 0.0, cx: 0.0, cy: 0.0, clear: 0, shade: 0, add: sheen as u32, cov: cova, plain: true });
                            continue;
                        }
                        let qnx = (lx - wf / 2.0).abs() - (wf / 2.0 - rn);
                        let (mut nx, mut ny) = if qnx > 0.0 && qny > 0.0 {
                            let l = (qnx * qnx + qny * qny).sqrt().max(1e-3);
                            (qnx / l, qny / l)
                        } else if qnx > qny {
                            (1.0, 0.0)
                        } else {
                            (0.0, 1.0)
                        };
                        if lx < wf / 2.0 {
                            nx = -nx;
                        }
                        if ly < hf / 2.0 {
                            ny = -ny;
                        }
                        let t = 1.0 - d / edge;
                        let t2 = t * t;
                        let disp = lens * t2 * t;
                        let cs = if t > 0.45 { ca * (t - 0.45) / 0.55 } else { 0.0 };
                        let clear = if t > 0.2 {
                            let u = (t - 0.2) / 0.8;
                            (u * u * 170.0) as u32
                        } else {
                            0
                        };
                        let s = nx * lx_ + ny * ly_;
                        let spec = s.max(0.0);
                        let bounce = (-s).max(0.0);
                        let bev = t2 * t.sqrt();
                        let shade = ((bev * (26.0 + 70.0 * bounce) * (1.0 - spec)) as u32).min(200);
                        let hl = ((hl_w + 0.6 - d) / hl_w).clamp(0.0, 1.0);
                        let gl = (1.0 - d / glow_w).max(0.0);
                        let gl = gl * gl;
                        let spec2 = spec * spec;
                        let b2 = bounce * bounce;
                        let add = hl * (64.0 + 191.0 * spec2.sqrt() * spec + 110.0 * b2) * rim_k + gl * (70.0 * spec2 + 22.0 * b2) * rim_k + bev * 64.0 * spec2 * rim_k + sheen;
                        pxs.push(GeoPx { lx: lxi, ox: nx * disp, oy: ny * disp, cx: nx * cs, cy: ny * cs, clear, shade, add: (add as u32).min(255), cov: cova, plain: false });
                    }
                    rows.push((start, pxs.len() as u32));
                }
                let gg = std::sync::Arc::new(GlassGeo { rows, px: pxs });
                shape_cache_room(gg.px.len() * 2);
                SHAPES.with(|c| {
                    c.borrow_mut().geo.insert(gkey, gg.clone());
                });
                gg
            }
        };
        let shade_row = |py: i32, row: &mut [u32]| {
            let ly = (py - y) as f32 + 0.5;
            let _qy = (ly - hf / 2.0).abs() - (hf / 2.0 - rf);
            let _qny = (ly - hf / 2.0).abs() - (hf / 2.0 - rn);
            // sheen: a soft lit band across the top that leans to the left
            let sheen_row = (1.0 - ly / (hf * 0.5).max(1.0)).max(0.0);
            let sheen_row = sheen_row * sheen_row * 26.0;
            // the interior span needs no edge work
            let corner_rows = ly < rf.max(edge) || ly > hf - rf.max(edge);
            let mx = if corner_rows { edge.max(rf) } else { edge } + 1.0;
            let (ia, ib) = if ly > edge + 1.0 && ly < hf - edge - 1.0 { ((x as f32 + mx).ceil() as i32, (x as f32 + wf - mx).floor() as i32) } else { (x1, x1) };
            let ia = ia.clamp(x0, x1);
            let ib = ib.clamp(x0, x1).max(ia);
            if ib > ia {
                let gy = ((py as f32 + boy as f32 + 0.5) / ds - 0.5).max(0.0);
                let r0 = (gy as usize).min(rows - 1);
                let r1 = (r0 + 1).min(rows - 1);
                let wb = (((gy - r0 as f32) * 256.0) as u32).min(256);
                let wa = 256 - wb;
                let (sa, sb) = ((ia + box_).clamp(0, bd.w) as usize, (ib + box_).clamp(0, bd.w) as usize);
                let ra = &tw[r0 * bw + sa..r0 * bw + sb];
                let rb = &tw[r1 * bw + sa..r1 * bw + sb];
                let out = &mut row[(ia - x0) as usize..(ib - x0) as usize];
                // the sheen also fades to the right: linear in x across the row
                let s0 = sheen_row * 1.15;
                let ds_ = -sheen_row * 0.5 / wf;
                let mut px = ia;
                for ((o, &a), &b) in out.iter_mut().zip(ra).zip(rb) {
                    let mut c = lerp_w(a, b, wa, wb);
                    let sh = s0 + ds_ * (px - x) as f32;
                    if sh >= 1.0 {
                        c = add_sat(c, sh as u32);
                    }
                    if frost > 0 {
                        let n = grain(px, py) & frost;
                        c = add_sat(c, n);
                    }
                    *o = 0xFF00_0000 | c;
                    px += 1;
                }
            }
            let j = (py - y) as usize;
            if j >= geo.rows.len() {
                return;
            }
            let (g0, g1) = geo.rows[j];
            for gp in &geo.px[g0 as usize..g1 as usize] {
                let px = x + gp.lx;
                if px < x0 || px >= x1 || (px >= ia && px < ib) {
                    continue;
                }
                let (fx, fy) = (px as f32, py as f32);
                let c = if gp.plain {
                    let mut c = tap(fx, fy);
                    if frost > 0 {
                        c = add_sat(c, grain(px, py) & frost);
                    }
                    c
                } else {
                    let (bx, by) = (fx + gp.ox, fy + gp.oy);
                    let c = if gp.cx != 0.0 || gp.cy != 0.0 {
                        let cr = tap(bx + gp.cx, by + gp.cy);
                        let cg = tap(bx, by);
                        let cb = tap(bx - gp.cx, by - gp.cy);
                        (cr & 0xFF0000) | (cg & 0x00FF00) | (cb & 0x0000FF)
                    } else {
                        tap(bx, by)
                    };
                    let c = if gp.clear > 0 { lerp_px(c, tap_clear(bx, by), gp.clear) } else { c };
                    lerp_px(c, 0, gp.shade)
                };
                row[(px - x0) as usize] = (gp.cov << 24) | add_sat(c, gp.add);
            }
        };
        let mut img = vec![0u32; (cw * ch) as usize];
        let work = (cw * ch) as usize * 2;
        par_rows(&mut img, cw as usize, 0, ch, work, |j, row| shade_row(y0 + j, row));
        let img = std::sync::Arc::new(img);
        shape_cache_room(work / 2);
        SHAPES.with(|c| {
            c.borrow_mut().glass.insert(key, img.clone());
        });
        self.composite(&img, x0, y0, cw, ch);
    }

    /// Put a panel image (alpha = coverage) at pixel (x0, y0); it is already clipped.
    fn composite(&mut self, img: &[u32], x0: i32, y0: i32, cw: i32, ch: i32) {
        let sw = self.w as usize;
        par_rows(&mut self.px, sw, y0, y0 + ch, (cw * ch) as usize, |py, row| {
            let src = &img[((py - y0) * cw) as usize..((py - y0 + 1) * cw) as usize];
            let dst = &mut row[x0 as usize..(x0 + cw) as usize];
            for (d, &v) in dst.iter_mut().zip(src) {
                let a = v >> 24;
                if a >= 255 {
                    *d = v;
                } else if a > 0 {
                    *d = 0xFF00_0000 | (mix(*d, v, a) & 0xFF_FFFF);
                }
            }
        });
    }

    // ------------------------------------------------------------ images (already in pixels)
    /// Blit an image at a layout position. The image is in real pixels.
    pub fn blit(&mut self, src: &Canvas, x: i32, y: i32, alpha: u32) {
        let (x, y) = (self.u(x), self.u(y));
        self.blit_px(src, x, y, alpha);
    }
    pub fn blit_px(&mut self, src: &Canvas, x: i32, y: i32, alpha: u32) {
        let _p = crate::prof::scope("blit");
        if alpha == 0 {
            return;
        }
        let sy0 = (self.clip.1 - y).max(0);
        let sy1 = (self.clip.3 - y).min(src.h);
        let sx0 = (self.clip.0 - x).max(0);
        let sx1 = (self.clip.2 - x).min(src.w);
        if sy1 <= sy0 || sx1 <= sx0 {
            return;
        }
        let w = self.w as usize;
        let work = ((sx1 - sx0) * (sy1 - sy0)) as usize;
        par_rows(&mut self.px, w, y + sy0, y + sy1, work, |dy, row| {
            let sy = dy - y;
            let srow = &src.px[(sy * src.w) as usize..((sy + 1) * src.w) as usize];
            for sx in sx0..sx1 {
                let s = srow[sx as usize];
                let sa = s >> 24;
                if sa == 0 {
                    continue;
                }
                let a = if alpha >= 255 { sa } else { sa * alpha / 255 };
                let d = &mut row[(x + sx) as usize];
                *d = blend_px(*d, s, a);
            }
        });
    }
    /// Opaque copy of a same-size image (backgrounds).
    pub fn copy_from(&mut self, src: &Canvas) {
        let _p = crate::prof::scope("copy");
        if src.w == self.w && src.h == self.h {
            self.px.copy_from_slice(&src.px);
        } else {
            let s = crate::assets::resample(src, self.w, self.h);
            self.px.copy_from_slice(&s.px);
        }
    }

    // ---------------------------------------------------------- text
    /// Draw a coverage mask (glyphs, icons) in one color at a pixel position.
    pub fn blit_tint_px(&mut self, src: &Canvas, x: i32, y: i32, color: u32, alpha: u32) {
        let _p = crate::prof::scope("glyph/icon");
        if alpha == 0 {
            return;
        }
        let sy0 = (self.clip.1 - y).max(0);
        let sy1 = (self.clip.3 - y).min(src.h);
        let sx0 = (self.clip.0 - x).max(0);
        let sx1 = (self.clip.2 - x).min(src.w);
        if sx1 <= sx0 {
            return;
        }
        let w = self.w as usize;
        for sy in sy0..sy1 {
            let srow = &src.px[(sy * src.w) as usize..((sy + 1) * src.w) as usize];
            let base = (y + sy) as usize * w + x as usize;
            for sx in sx0..sx1 {
                let m = srow[sx as usize] >> 24;
                if m == 0 {
                    continue;
                }
                let a = if alpha >= 255 { m } else { m * alpha / 255 };
                let d = &mut self.px[base + sx as usize];
                *d = blend_px(*d, color, a);
            }
        }
    }
    /// Em size in layout units for a UI text scale step.
    pub fn px(scale: i32) -> f32 {
        (9 * scale + 4) as f32
    }
    pub fn text_w(s: &str, scale: i32) -> i32 {
        font::measure(s, Canvas::px(scale), font::Weight::Regular).round() as i32
    }
    pub fn text_wb(s: &str, scale: i32, w: font::Weight) -> i32 {
        font::measure(s, Canvas::px(scale), w).round() as i32
    }
    pub fn text(&mut self, x: i32, y: i32, s: &str, scale: i32, c: u32) {
        self.text_a(x, y, s, scale, c, 255);
    }
    pub fn text_a(&mut self, x: i32, y: i32, s: &str, scale: i32, c: u32, a: u32) {
        let px = Canvas::px(scale);
        font::draw(self, x as f32, y as f32 + px * 0.74, s, px, font::Weight::Regular, c, a);
    }
    pub fn text_weight(&mut self, x: i32, y: i32, s: &str, scale: i32, c: u32, a: u32, w: font::Weight) {
        let px = Canvas::px(scale);
        font::draw(self, x as f32, y as f32 + px * 0.74, s, px, w, c, a);
    }
    /// Text with a soft drop shadow for readability over scenes.
    pub fn text_s(&mut self, x: i32, y: i32, s: &str, scale: i32, c: u32) {
        let off = if scale >= 3 { 2 } else { 1 };
        self.text_a(x + off, y + off, s, scale, 0xFF00_0000, 110);
        self.text(x, y, s, scale, c);
    }
    pub fn text_center(&mut self, cx: i32, y: i32, s: &str, scale: i32, c: u32) {
        let w = Canvas::text_w(s, scale);
        self.text_s(cx - w / 2, y, s, scale, c);
    }
    pub fn text_right(&mut self, rx: i32, y: i32, s: &str, scale: i32, c: u32) {
        let w = Canvas::text_w(s, scale);
        self.text_s(rx - w, y, s, scale, c);
    }
    /// Word-wrapped text; returns the height used.
    pub fn text_wrap(&mut self, x: i32, y: i32, maxw: i32, s: &str, scale: i32, c: u32) -> i32 {
        let lines = wrap(s, maxw, scale);
        let lh = (Canvas::px(scale) * 1.3) as i32;
        for (i, l) in lines.iter().enumerate() {
            self.text_s(x, y + i as i32 * lh, l, scale, c);
        }
        lines.len() as i32 * lh
    }

    /// CRT look: darken alternate scanlines (one per layout row, whatever the resolution).
    pub fn scanlines(&mut self, strength: u32) {
        let period = (2.0 * self.k).round().max(2.0) as i32;
        for y in 0..self.h {
            if y % period >= period / 2 {
                continue;
            }
            let row = (y * self.w) as usize;
            for x in 0..self.w as usize {
                let c = self.px[row + x];
                self.px[row + x] = darken(c, strength);
            }
        }
    }
}

pub fn wrap(s: &str, maxw: i32, scale: i32) -> Vec<String> {
    let mut out = Vec::new();
    for para in s.split('\n') {
        let mut line = String::new();
        for word in para.split(' ') {
            let trial = if line.is_empty() { word.to_string() } else { format!("{} {}", line, word) };
            if Canvas::text_w(&trial, scale) <= maxw || line.is_empty() {
                line = trial;
            } else {
                out.push(std::mem::take(&mut line));
                line.push_str(word);
            }
        }
        out.push(line);
    }
    out
}

/// In-place separable box blur on an RGB buffer.
fn box_blur(buf: &mut [[u32; 3]], w: usize, h: usize, r: usize) {
    if w == 0 || h == 0 {
        return;
    }
    let mut tmp = vec![[0u32; 3]; w.max(h)];
    for y in 0..h {
        let row = &mut buf[y * w..(y + 1) * w];
        blur_line(row, &mut tmp[..w], r);
    }
    let mut col = vec![[0u32; 3]; h];
    for x in 0..w {
        for y in 0..h {
            col[y] = buf[y * w + x];
        }
        blur_line(&mut col, &mut tmp[..h], r);
        for y in 0..h {
            buf[y * w + x] = col[y];
        }
    }
}
fn blur_line(line: &mut [[u32; 3]], tmp: &mut [[u32; 3]], r: usize) {
    let n = line.len();
    if n == 0 {
        return;
    }
    let win = (2 * r + 1) as u32;
    let mut acc = [0u32; 3];
    for i in 0..=2 * r {
        let p = line[(i as isize - r as isize).clamp(0, n as isize - 1) as usize];
        for c in 0..3 {
            acc[c] += p[c];
        }
    }
    for i in 0..n {
        for c in 0..3 {
            tmp[i][c] = acc[c] / win;
        }
        let out = line[(i as isize - r as isize).clamp(0, n as isize - 1) as usize];
        let inn = line[(i as isize + r as isize + 1).clamp(0, n as isize - 1) as usize];
        for c in 0..3 {
            acc[c] = acc[c] + inn[c] - out[c];
        }
    }
    line.copy_from_slice(tmp);
}

/// A blurred copy of an opaque image (menu backdrops). `r` is in the image's own pixels.
pub fn blurred(src: &Canvas, r: usize) -> Canvas {
    let (w, h) = (src.w as usize, src.h as usize);
    let mut buf: Vec<[u32; 3]> = src.px.iter().map(|&c| [(c >> 16) & 255, (c >> 8) & 255, c & 255]).collect();
    box_blur(&mut buf, w, h, r);
    box_blur(&mut buf, w, h, r);
    box_blur(&mut buf, w, h, r);
    let mut out = Canvas::new(src.w, src.h);
    out.k = src.k;
    for (d, p) in out.px.iter_mut().zip(buf.iter()) {
        *d = 0xFF00_0000 | (p[0] << 16) | (p[1] << 8) | p[2];
    }
    out
}
