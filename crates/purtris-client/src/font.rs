//! Text: anti-aliased glyph atlases rendered in Blender from Lato, plus Mochiy Pop One and
//! M PLUS Rounded 1c baked by tools/font_atlas_pil.py (all SIL OFL 1.1, see assets/fonts/). Glyphs are cut from the atlas and resampled per size, cached.
//! A small vector icon set (arrows, star, dots, heart, play) is drawn as shapes.

use crate::gfx::Canvas;
use std::cell::RefCell;
use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Weight {
    Regular,
    Bold,
    Black,
    /// Mochiy Pop One: the chunky, friendly display face for titles, headers and big numbers
    Display,
    /// M PLUS Rounded 1c Black: rounded labels for buttons, tabs and key caps
    Round,
}

const ATLAS: [(&[u8], &str); 5] = [
    (include_bytes!("../../../assets/sprites/font_regular.tga"), include_str!("../../../assets/sprites/font_regular.txt")),
    (include_bytes!("../../../assets/sprites/font_bold.tga"), include_str!("../../../assets/sprites/font_bold.txt")),
    (include_bytes!("../../../assets/sprites/font_black.tga"), include_str!("../../../assets/sprites/font_black.txt")),
    (include_bytes!("../../../assets/sprites/font_display.tga"), include_str!("../../../assets/sprites/font_display.txt")),
    (include_bytes!("../../../assets/sprites/font_round.tga"), include_str!("../../../assets/sprites/font_round.txt")),
];
const CELL: i32 = 80; // 1.25 em at 64 px per em
const PPU: f32 = 64.0;
const ORIGIN_X: f32 = 6.4;
const BASELINE: f32 = 80.0 - 0.32 * 64.0;
const TRACK: f32 = 0.045;

struct Face {
    atlas: Canvas,
    xmax: [f32; 95],
}

struct Fonts {
    faces: Vec<Face>,
    /// glyph images trimmed to their ink, with the trim offset
    cache: HashMap<(Weight, char, i32), (Canvas, i32, i32)>,
}

thread_local! {
    static FONTS: RefCell<Option<Fonts>> = const { RefCell::new(None) };
}

fn with<R>(f: impl FnOnce(&mut Fonts) -> R) -> R {
    FONTS.with(|cell| {
        let mut b = cell.borrow_mut();
        if b.is_none() {
            let mut faces = Vec::new();
            for (tga, txt) in ATLAS.iter() {
                let atlas = crate::assets::decode_tga(tga).unwrap_or_else(|| Canvas::transparent(1, 1));
                let mut xmax = [0.3f32; 95];
                for line in txt.lines().skip(1) {
                    let p: Vec<&str> = line.split_whitespace().collect();
                    if p.len() == 3 {
                        if let (Ok(c), Ok(x1)) = (p[0].parse::<usize>(), p[2].parse::<f32>()) {
                            if (32..127).contains(&c) {
                                xmax[c - 32] = x1;
                            }
                        }
                    }
                }
                faces.push(Face { atlas, xmax });
            }
            *b = Some(Fonts { faces, cache: HashMap::new() });
        }
        f(b.as_mut().unwrap())
    })
}

fn wi(w: Weight) -> usize {
    match w {
        Weight::Regular => 0,
        Weight::Bold => 1,
        Weight::Black => 2,
        Weight::Display => 3,
        Weight::Round => 4,
    }
}

fn is_icon(c: char) -> bool {
    matches!(c, '\u{2190}' | '\u{2192}' | '\u{2191}' | '\u{2193}' | '\u{2605}' | '\u{25CF}' | '\u{25CB}' | '\u{2665}' | '\u{25B6}')
}

/// Advance width in pixels for a character at em size `px`.
pub fn advance(c: char, px: f32, w: Weight) -> f32 {
    if is_icon(c) {
        return px * 0.8;
    }
    let k = c as u32;
    if !(32..127).contains(&k) {
        return px * 0.5;
    }
    with(|f| (f.faces[wi(w)].xmax[(k - 32) as usize] + TRACK) * px)
}

pub fn measure(s: &str, px: f32, w: Weight) -> f32 {
    s.chars().map(|c| advance(c, px, w)).sum()
}

/// Draw text with its baseline at `base_y` (layout units). Glyphs are rasterized at the canvas's
/// real pixel size, so text stays sharp at every resolution. Returns the advance (layout units).
pub fn draw(cv: &mut Canvas, x: f32, base_y: f32, s: &str, px: f32, w: Weight, color: u32, alpha: u32) -> f32 {
    let k = cv.k;
    let ppx = px * k;
    let size = ppx.round().max(4.0) as i32;
    let f = size as f32 / PPU;
    let ox = (ORIGIN_X * f).round() as i32;
    let oy = (BASELINE * f).round() as i32;
    let by = (base_y * k).round() as i32;
    let mut pen = x;
    for c in s.chars() {
        if is_icon(c) {
            icon(cv, c, pen, base_y, px, color, alpha);
            pen += advance(c, px, w);
            continue;
        }
        let code = c as u32;
        if !(33..127).contains(&code) {
            pen += advance(c, px, w);
            continue;
        }
        with(|fs| {
            let key = (w, c, size);
            if !fs.cache.contains_key(&key) {
                if fs.cache.len() > 6000 {
                    fs.cache.clear();
                }
                let face = &fs.faces[wi(w)];
                let idx = (code - 32) as i32;
                let (col, row) = (idx % 16, idx / 16);
                let mut cell = Canvas::transparent(CELL, CELL);
                for yy in 0..CELL {
                    let sy = row * CELL + yy;
                    if sy >= face.atlas.h {
                        break;
                    }
                    for xx in 0..CELL {
                        let sx = col * CELL + xx;
                        if sx < face.atlas.w {
                            cell.px[(yy * CELL + xx) as usize] = face.atlas.px[(sy * face.atlas.w + sx) as usize];
                        }
                    }
                }
                let d = ((CELL as f32) * f).round().max(1.0) as i32;
                let g = crate::assets::resample(&cell, d, d);
                fs.cache.insert(key, trim(&g));
            }
            if let Some((g, tx, ty)) = fs.cache.get(&key) {
                cv.blit_tint_px(g, (pen * k).round() as i32 - ox + tx, by - oy + ty, color, alpha);
            }
        });
        pen += advance(c, px, w);
    }
    pen - x
}

/// Crop an image to the pixels with any coverage; returns it with its offset.
fn trim(g: &Canvas) -> (Canvas, i32, i32) {
    let (mut x0, mut y0, mut x1, mut y1) = (g.w, g.h, -1, -1);
    for y in 0..g.h {
        for x in 0..g.w {
            if g.px[(y * g.w + x) as usize] >> 24 != 0 {
                x0 = x0.min(x);
                x1 = x1.max(x);
                y0 = y0.min(y);
                y1 = y1.max(y);
            }
        }
    }
    if x1 < x0 {
        return (Canvas::transparent(1, 1), 0, 0);
    }
    let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
    let mut o = Canvas::transparent(w, h);
    for y in 0..h {
        let src = ((y + y0) * g.w + x0) as usize;
        o.px[(y * w) as usize..((y + 1) * w) as usize].copy_from_slice(&g.px[src..src + w as usize]);
    }
    (o, x0, y0)
}

fn icon(cv: &mut Canvas, c: char, x: f32, base: f32, px: f32, color: u32, a: u32) {
    let s = px * 0.62;
    let cx = x + px * 0.4;
    let cy = base - px * 0.33;
    match c {
        '\u{2190}' => cv.tri([(cx - s * 0.5, cy), (cx + s * 0.35, cy - s * 0.45), (cx + s * 0.35, cy + s * 0.45)], color, a),
        '\u{2192}' | '\u{25B6}' => cv.tri([(cx + s * 0.5, cy), (cx - s * 0.35, cy - s * 0.45), (cx - s * 0.35, cy + s * 0.45)], color, a),
        '\u{2191}' => cv.tri([(cx, cy - s * 0.5), (cx - s * 0.45, cy + s * 0.35), (cx + s * 0.45, cy + s * 0.35)], color, a),
        '\u{2193}' => cv.tri([(cx, cy + s * 0.5), (cx - s * 0.45, cy - s * 0.35), (cx + s * 0.45, cy - s * 0.35)], color, a),
        '\u{2605}' => cv.star(cx, cy, s * 0.62, color, a),
        '\u{25CF}' => cv.circle(cx, cy, s * 0.38, color, a),
        '\u{25CB}' => cv.ring(cx, cy, s * 0.34, (px * 0.08).max(1.2), color, a),
        _ => {
            cv.circle(cx - s * 0.2, cy - s * 0.1, s * 0.24, color, a);
            cv.circle(cx + s * 0.2, cy - s * 0.1, s * 0.24, color, a);
            cv.tri([(cx - s * 0.43, cy), (cx + s * 0.43, cy), (cx, cy + s * 0.42)], color, a);
        }
    }
}
