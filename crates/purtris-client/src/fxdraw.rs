//! Fast primitives for match effects. The general shapes in gfx.rs scan their whole bounding
//! box; effects are big, thin and many (rings 400 px across, rays half a screen long), so these
//! rasterize only the spans they cover, add light instead of only blending, and split big jobs
//! across the worker pool. Sprites here are premultiplied (rgb already times alpha), so they
//! scale with bilinear filtering without dark fringes.
//!
//! Coordinates: functions without `_px` take layout units (1280x720), like the rest of gfx.

use crate::gfx::{par_rows, Canvas};

/// Per-channel saturating add of two 0x00RRGGBB colors.
#[inline]
pub fn sat_add(x: u32, y: u32) -> u32 {
    let rb = (x & 0xFF00FF) + (y & 0xFF00FF);
    let m = rb & 0x0100_0100;
    let rb = (rb | (m - (m >> 8))) & 0xFF00FF;
    let g = (x & 0xFF00) + (y & 0xFF00);
    let mg = g & 0x1_0000;
    let g = (g | (mg - (mg >> 8))) & 0xFF00;
    rb | g
}
/// Scale a color's channels by `a` in 0..=256.
#[inline]
pub fn scale_rgb(c: u32, a: u32) -> u32 {
    ((((c & 0xFF00FF) * a) >> 8) & 0xFF00FF) | ((((c & 0xFF00) * a) >> 8) & 0xFF00)
}
/// Add light: `d + c * a`, a in 0..=256.
#[inline]
pub fn add_px(d: u32, c: u32, a: u32) -> u32 {
    0xFF00_0000 | sat_add(d, scale_rgb(c, a))
}
/// Blend over: `d + (c - d) * a`, a in 0..=256.
#[inline]
pub fn over_px(d: u32, c: u32, a: u32) -> u32 {
    0xFF00_0000 | sat_add(scale_rgb(d, 256 - a), scale_rgb(c, a))
}
/// Premultiplied source over `d`, the whole source scaled by `k` in 0..=256.
#[inline]
pub fn over_pm(d: u32, s: u32, k: u32) -> u32 {
    let sa = ((s >> 24) * k) >> 8;
    if sa == 0 {
        return d;
    }
    let sa = sa + (sa >> 7);
    0xFF00_0000 | sat_add(scale_rgb(d, 256 - sa.min(256)), scale_rgb(s, k))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// normal alpha blend
    Over,
    /// additive light
    Add,
}

#[inline]
fn put(px: &mut u32, c: u32, a: u32, mode: Mode) {
    *px = match mode {
        Mode::Over => over_px(*px, c, a),
        Mode::Add => add_px(*px, c, a),
    };
}

/// Convex polygon (up to 8 points), anti-aliased with 4 sub-scanlines per row; cost follows
/// the covered area, not the bounding box.
pub fn poly(cv: &mut Canvas, pts: &[(f32, f32)], c: u32, a: u32, mode: Mode) {
    let k = cv.k;
    let mut q = [(0.0f32, 0.0f32); 8];
    let n = pts.len().min(8);
    for i in 0..n {
        q[i] = (pts[i].0 * k, pts[i].1 * k);
    }
    poly_px(cv, &q[..n], c, a, mode);
}

pub fn poly_px(cv: &mut Canvas, p: &[(f32, f32)], c: u32, a: u32, mode: Mode) {
    let _p = crate::prof::scope("fx poly");
    let n = p.len();
    if n < 3 || a == 0 {
        return;
    }
    let (cx0, cy0, cx1, cy1) = cv.clip_px();
    let ymin = p.iter().map(|q| q.1).fold(f32::MAX, f32::min);
    let ymax = p.iter().map(|q| q.1).fold(f32::MIN, f32::max);
    let y0 = (ymin.floor() as i32).max(cy0);
    let y1 = (ymax.ceil() as i32).min(cy1);
    let a256 = a.min(255) + (a.min(255) >> 7);
    let w = cv.w;
    for y in y0..y1 {
        let mut sp = [(f32::MAX, f32::MIN); 4];
        let mut valid = 0;
        for (s, span) in sp.iter_mut().enumerate() {
            let sy = y as f32 + (s as f32 + 0.5) * 0.25;
            for i in 0..n {
                let (ax, ay) = p[i];
                let (bx, by) = p[(i + 1) % n];
                if (ay <= sy && by > sy) || (by <= sy && ay > sy) {
                    let x = ax + (sy - ay) / (by - ay) * (bx - ax);
                    span.0 = span.0.min(x);
                    span.1 = span.1.max(x);
                }
            }
            if span.1 > span.0 {
                valid += 1;
            }
        }
        if valid == 0 {
            continue;
        }
        let mut xl = f32::MAX;
        let mut xr = f32::MIN;
        let mut inl = f32::MIN;
        let mut inr = f32::MAX;
        for s in &sp {
            if s.1 > s.0 {
                xl = xl.min(s.0);
                xr = xr.max(s.1);
                inl = inl.max(s.0);
                inr = inr.min(s.1);
            } else {
                inl = f32::MAX;
            }
        }
        let fx0 = (xl.floor() as i32).max(cx0);
        let fx1 = (xr.ceil() as i32).min(cx1);
        let full0 = inl.ceil() as i32;
        let full1 = inr.floor() as i32;
        let row = (y * w) as usize;
        for x in fx0..fx1 {
            let px = &mut cv.px[row + x as usize];
            if valid == 4 && x >= full0 && x + 1 <= full1 {
                put(px, c, a256, mode);
                continue;
            }
            let (l, r) = (x as f32, x as f32 + 1.0);
            let mut cov = 0.0;
            for s in &sp {
                if s.1 > s.0 {
                    cov += (r.min(s.1) - l.max(s.0)).clamp(0.0, 1.0);
                }
            }
            let ca = (cov * 0.25 * a256 as f32) as u32;
            if ca > 0 {
                put(px, c, ca, mode);
            }
        }
    }
}

/// A tapered streak from (x0, y0) to (x1, y1), `w0` wide at the start, `w1` at the end.
#[allow(clippy::too_many_arguments)]
pub fn streak(cv: &mut Canvas, x0: f32, y0: f32, x1: f32, y1: f32, w0: f32, w1: f32, c: u32, a: u32, mode: Mode) {
    let (dx, dy) = (x1 - x0, y1 - y0);
    let l = (dx * dx + dy * dy).sqrt().max(1e-3);
    let (nx, ny) = (-dy / l, dx / l);
    let (h0, h1) = (w0 * 0.5, w1 * 0.5);
    if h1 < 0.05 {
        poly(cv, &[(x0 + nx * h0, y0 + ny * h0), (x1, y1), (x0 - nx * h0, y0 - ny * h0)], c, a, mode);
    } else {
        poly(cv, &[(x0 + nx * h0, y0 + ny * h0), (x1 + nx * h1, y1 + ny * h1), (x1 - nx * h1, y1 - ny * h1), (x0 - nx * h0, y0 - ny * h0)], c, a, mode);
    }
}

/// Anti-aliased ring that only visits the pixels near it. `soft` fades the band from its middle
/// line out (a glow ring); otherwise the band is solid with crisp edges (a toon ring).
#[allow(clippy::too_many_arguments)]
pub fn ring(cv: &mut Canvas, cx: f32, cy: f32, r: f32, thick: f32, c: u32, a: u32, mode: Mode, soft: bool) {
    let k = cv.k;
    ring_px(cv, cx * k, cy * k, r * k, (thick * k).max(1.0), c, a, mode, soft);
}

#[allow(clippy::too_many_arguments)]
pub fn ring_px(cv: &mut Canvas, cx: f32, cy: f32, r: f32, thick: f32, c: u32, a: u32, mode: Mode, soft: bool) {
    let _p = crate::prof::scope("fx ring");
    if a == 0 || r <= 0.0 || thick <= 0.0 {
        return;
    }
    let (clx0, cly0, clx1, cly1) = cv.clip_px();
    let ht = thick * 0.5;
    let ro = r + ht + 1.0;
    let ri = (r - ht - 1.0).max(0.0);
    let y0 = ((cy - ro).floor() as i32).max(cly0);
    let y1 = ((cy + ro).ceil() as i32).min(cly1);
    let a256 = a.min(255) + (a.min(255) >> 7);
    let w = cv.w as usize;
    let work = (std::f32::consts::TAU * r * (thick + 2.0)) as usize;
    par_rows(&mut cv.px, w, y0, y1, work, |y, row| {
        let dy = y as f32 + 0.5 - cy;
        let dy2 = dy * dy;
        if dy2 > ro * ro {
            return;
        }
        let xo = (ro * ro - dy2).sqrt();
        let xi = if dy2 < ri * ri { (ri * ri - dy2).sqrt() } else { -1.0 };
        let mut seg = |a0: f32, b0: f32| {
            let xa = (a0.floor() as i32).max(clx0);
            let xb = (b0.ceil() as i32).min(clx1);
            for x in xa..xb {
                let dx = x as f32 + 0.5 - cx;
                let d = ((dx * dx + dy2).sqrt() - r).abs();
                let cov = if soft { (1.0 - d / ht).max(0.0) } else { (ht - d + 0.5).clamp(0.0, 1.0) };
                if cov > 0.0 {
                    let ca = (cov * a256 as f32) as u32;
                    let p = &mut row[x as usize];
                    put(p, c, ca, mode);
                }
            }
        };
        if xi < 0.0 {
            seg(cx - xo, cx + xo);
        } else {
            seg(cx - xo, cx - xi);
            seg(cx + xi, cx + xo);
        }
    });
}

/// Rectangle of light or tint (layout units), on the worker pool.
pub fn rect(cv: &mut Canvas, x: i32, y: i32, w: i32, h: i32, c: u32, a: u32, mode: Mode) {
    let _p = crate::prof::scope("fx rect");
    if a == 0 {
        return;
    }
    let k = cv.k;
    let (clx0, cly0, clx1, cly1) = cv.clip_px();
    let x0 = ((x as f32 * k).round() as i32).max(clx0);
    let x1 = (((x + w) as f32 * k).round() as i32).min(clx1);
    let y0 = ((y as f32 * k).round() as i32).max(cly0);
    let y1 = (((y + h) as f32 * k).round() as i32).min(cly1);
    if x1 <= x0 {
        return;
    }
    let a256 = a.min(255) + (a.min(255) >> 7);
    let cw = cv.w as usize;
    par_rows(&mut cv.px, cw, y0, y1, ((x1 - x0) * (y1 - y0).max(0)) as usize, |_, row| {
        for p in &mut row[x0 as usize..x1 as usize] {
            put(p, c, a256, mode);
        }
    });
}

/// A premultiplied sprite centred at (cx, cy) in real pixels, scaled (bilinear) and faded.
pub fn sprite_px(cv: &mut Canvas, src: &Canvas, cx: f32, cy: f32, scale: f32, alpha: u32) {
    let _p = crate::prof::scope("fx sprite");
    if alpha == 0 || scale <= 0.01 || src.w == 0 {
        return;
    }
    let k = alpha.min(255) + (alpha.min(255) >> 7);
    let (clx0, cly0, clx1, cly1) = cv.clip_px();
    let w = cv.w as usize;
    if (scale - 1.0).abs() < 0.035 {
        let x = (cx - src.w as f32 * 0.5).round() as i32;
        let y = (cy - src.h as f32 * 0.5).round() as i32;
        let sx0 = (clx0 - x).max(0);
        let sx1 = (clx1 - x).min(src.w);
        let sy0 = (cly0 - y).max(0);
        let sy1 = (cly1 - y).min(src.h);
        if sx1 <= sx0 || sy1 <= sy0 {
            return;
        }
        par_rows(&mut cv.px, w, y + sy0, y + sy1, ((sx1 - sx0) * (sy1 - sy0)) as usize, |dy, row| {
            let sy = dy - y;
            let srow = &src.px[(sy * src.w) as usize..((sy + 1) * src.w) as usize];
            for sx in sx0..sx1 {
                let s = srow[sx as usize];
                if s >> 24 != 0 {
                    let d = &mut row[(x + sx) as usize];
                    *d = over_pm(*d, s, k);
                }
            }
        });
        return;
    }
    let dw = src.w as f32 * scale;
    let dh = src.h as f32 * scale;
    let x0 = ((cx - dw * 0.5).floor() as i32).max(clx0);
    let x1 = ((cx + dw * 0.5).ceil() as i32).min(clx1);
    let y0 = ((cy - dh * 0.5).floor() as i32).max(cly0);
    let y1 = ((cy + dh * 0.5).ceil() as i32).min(cly1);
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let inv = 1.0 / scale;
    let (sw, sh) = (src.w, src.h);
    if scale >= 1.25 {
        // a fast slam-in: nearest sampling, the motion hides it and it costs a quarter
        let ox = src.w as f32 * 0.5 - cx * inv;
        let oy = src.h as f32 * 0.5 - cy * inv;
        par_rows(&mut cv.px, w, y0, y1, ((x1 - x0) * (y1 - y0)) as usize, |y, row| {
            let sy = ((y as f32 + 0.5) * inv + oy) as i32;
            if sy < 0 || sy >= sh {
                return;
            }
            let srow = &src.px[(sy * sw) as usize..((sy + 1) * sw) as usize];
            for x in x0..x1 {
                let sx = ((x as f32 + 0.5) * inv + ox) as i32;
                if sx < 0 || sx >= sw {
                    continue;
                }
                let s = srow[sx as usize];
                if s >> 24 != 0 {
                    let p = &mut row[x as usize];
                    *p = over_pm(*p, s, k);
                }
            }
        });
        return;
    }
    let ox = src.w as f32 * 0.5 - cx * inv - 0.5;
    let oy = src.h as f32 * 0.5 - cy * inv - 0.5;
    let get = |x: i32, y: i32| -> u32 {
        if x < 0 || y < 0 || x >= sw || y >= sh {
            0
        } else {
            src.px[(y * sw + x) as usize]
        }
    };
    par_rows(&mut cv.px, w, y0, y1, ((x1 - x0) * (y1 - y0)) as usize, |y, row| {
        let fy = (y as f32 + 0.5) * inv + oy;
        let iy = fy.floor() as i32;
        let ty = ((fy - iy as f32) * 256.0) as u32;
        for x in x0..x1 {
            let fx = (x as f32 + 0.5) * inv + ox;
            let ix = fx.floor() as i32;
            if ix < -1 || ix >= sw || iy < -1 || iy >= sh {
                continue;
            }
            let tx = ((fx - ix as f32) * 256.0) as u32;
            let (a, b, c, d) = (get(ix, iy), get(ix + 1, iy), get(ix, iy + 1), get(ix + 1, iy + 1));
            if (a | b | c | d) >> 24 == 0 {
                continue;
            }
            let top = lerp4(a, b, tx);
            let bot = lerp4(c, d, tx);
            let s = lerp4(top, bot, ty);
            let p = &mut row[x as usize];
            *p = over_pm(*p, s, k);
        }
    });
}

/// Lerp all four channels (alpha included), t in 0..=256.
#[inline]
fn lerp4(a: u32, b: u32, t: u32) -> u32 {
    let it = 256 - t;
    let ag = (((a >> 8) & 0xFF00FF) * it + ((b >> 8) & 0xFF00FF) * t) & 0xFF00FF00;
    let rb = (((a & 0xFF00FF) * it + (b & 0xFF00FF) * t) >> 8) & 0xFF00FF;
    ag | rb
}

/// An intensity mask (alpha byte) added as light in `color`, centred at (cx, cy) real pixels.
/// `hot` also adds white where the mask is strongest, so bright cores read white-hot.
pub fn light_px(cv: &mut Canvas, mask: &Canvas, cx: f32, cy: f32, color: u32, alpha: u32, hot: bool) {
    let _p = crate::prof::scope("fx light");
    if alpha == 0 {
        return;
    }
    let (clx0, cly0, clx1, cly1) = cv.clip_px();
    let x = (cx - mask.w as f32 * 0.5).round() as i32;
    let y = (cy - mask.h as f32 * 0.5).round() as i32;
    let sx0 = (clx0 - x).max(0);
    let sx1 = (clx1 - x).min(mask.w);
    let sy0 = (cly0 - y).max(0);
    let sy1 = (cly1 - y).min(mask.h);
    if sx1 <= sx0 || sy1 <= sy0 {
        return;
    }
    let al = alpha.min(255) + 1;
    let w = cv.w as usize;
    let mw = mask.w;
    par_rows(&mut cv.px, w, y + sy0, y + sy1, ((sx1 - sx0) * (sy1 - sy0)) as usize, |dy, row| {
        let sy = dy - y;
        let srow = &mask.px[(sy * mw) as usize..((sy + 1) * mw) as usize];
        for sx in sx0..sx1 {
            let m = srow[sx as usize] >> 24;
            if m == 0 {
                continue;
            }
            let a = (m * al) >> 8;
            let d = &mut row[(x + sx) as usize];
            let mut v = sat_add(*d, scale_rgb(color, a + (a >> 7)));
            if hot && m > 140 {
                let h = ((m - 140) * al) >> 7;
                v = sat_add(v, h * 0x01_0101);
            }
            *d = 0xFF00_0000 | v;
        }
    });
}
