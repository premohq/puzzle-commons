//! Titles spelled in the logo's letter tiles (tools/blender/tiles.py): one tilted, colored,
//! ink-bordered tile per letter, colors cycling like the "PUZZLE" row of the logo.

use crate::gfx::Canvas;
use std::collections::HashMap;

static TILES_PNG: &[u8] = include_bytes!("../../../assets/sprites/tiles.png");
static TILES_TXT: &str = include_str!("../../../assets/sprites/tiles.txt");

struct Atlas {
    img: Canvas,
    px: i32,
    colors: usize,
    chars: Vec<char>,
    sized: HashMap<(usize, usize, i32), Canvas>,
}

thread_local! {
    static ATLAS: std::cell::RefCell<Option<Atlas>> = const { std::cell::RefCell::new(None) };
}

/// Tile advance and word gap, as fractions of the tile size.
const ADVANCE: f32 = 0.86;
const SPACE: f32 = 0.42;

fn with_atlas<R>(f: impl FnOnce(&mut Atlas) -> R) -> R {
    ATLAS.with(|c| {
        let mut b = c.borrow_mut();
        if b.is_none() {
            let mut it = TILES_TXT.split_whitespace();
            let px = it.next().and_then(|v| v.parse().ok()).unwrap_or(144);
            let colors = it.next().and_then(|v| v.parse().ok()).unwrap_or(6);
            let chars = it.next().unwrap_or("").chars().collect();
            let img = crate::png::decode(TILES_PNG).unwrap_or_else(|| Canvas::transparent(1, 1));
            *b = Some(Atlas { img, px, colors, chars, sized: HashMap::new() });
        }
        f(b.as_mut().unwrap())
    })
}

/// Width in layout units of `text` spelled in tiles `size` units tall.
pub fn width(text: &str, size: i32) -> i32 {
    let mut w = 0.0;
    let n = text.chars().count();
    for (i, ch) in text.chars().enumerate() {
        w += if ch == ' ' { size as f32 * SPACE } else if i + 1 == n { size as f32 } else { size as f32 * ADVANCE };
    }
    w as i32
}

/// Spell `text` (upper-cased) in tiles `size` units tall, top-left at (x, y). `color0` picks
/// the first tile's color; with `anim` = Some(seconds since it began) the tiles drop in one by
/// one with a little bounce. Characters without a tile leave a gap. Returns the width.
pub fn draw(cv: &mut Canvas, text: &str, x: i32, y: i32, size: i32, color0: usize, anim: Option<f32>, alpha: u32) -> i32 {
    let up: String = text.to_uppercase();
    let px = cv.us(size).max(4);
    let k = cv.k;
    with_atlas(|a| {
        let mut cx = x as f32;
        let mut tile = 0usize;
        let n = up.chars().count();
        for (i, ch) in up.chars().enumerate() {
            if ch == ' ' {
                cx += size as f32 * SPACE;
                continue;
            }
            let adv = if i + 1 == n { size as f32 } else { size as f32 * ADVANCE };
            let Some(ci) = a.chars.iter().position(|&c| c == ch) else {
                cx += adv;
                continue;
            };
            let col = (color0 + tile) % a.colors;
            // gentle up/down rhythm, as in the logo
            let mut dy = if tile % 2 == 1 { size as f32 * 0.05 } else { -size as f32 * 0.02 };
            let mut al = alpha;
            if let Some(t) = anim {
                let lt = t - tile as f32 * 0.05;
                if lt <= 0.0 {
                    tile += 1;
                    cx += adv;
                    continue;
                }
                // drop in with an overshoot (back-out ease)
                let u = (lt / 0.38).min(1.0);
                let c1 = 2.2;
                let e = 1.0 + (c1 + 1.0) * (u - 1.0).powi(3) + c1 * (u - 1.0).powi(2);
                dy -= (1.0 - e) * size as f32 * 0.9;
                al = (alpha as f32 * (lt / 0.12).min(1.0)) as u32;
            }
            let key = (ci, col, px);
            if !a.sized.contains_key(&key) {
                if a.sized.len() > 400 {
                    a.sized.clear();
                }
                let mut cell = Canvas::transparent(a.px, a.px);
                for yy in 0..a.px {
                    let sy = col as i32 * a.px + yy;
                    let sx = ci as i32 * a.px;
                    if sy >= a.img.h || sx + a.px > a.img.w {
                        continue;
                    }
                    let s = (sy * a.img.w + sx) as usize;
                    cell.px[(yy * a.px) as usize..((yy + 1) * a.px) as usize].copy_from_slice(&a.img.px[s..s + a.px as usize]);
                }
                a.sized.insert(key, crate::assets::resample(&cell, px, px));
            }
            let img = &a.sized[&key];
            cv.blit_px(img, (cx * k).round() as i32, ((y as f32 + dy) * k).round() as i32, al);
            tile += 1;
            cx += adv;
        }
        (cx - x as f32) as i32
    })
}
