//! Blender-rendered art (tools/blender/export_cast.py), embedded in the binary.
//! TGA decoding and resampling are ours. Missing art falls back to the procedural painter.

use crate::gfx::Canvas;
use std::collections::HashMap;

static FILES: &[(&str, &[u8])] = &[
    ("blob_blue", include_bytes!("../../../assets/sprites/blob_blue.tga")),
    ("blob_green", include_bytes!("../../../assets/sprites/blob_green.tga")),
    ("blob_purple", include_bytes!("../../../assets/sprites/blob_purple.tga")),
    ("blob_red", include_bytes!("../../../assets/sprites/blob_red.tga")),
    ("blob_white", include_bytes!("../../../assets/sprites/blob_white.tga")),
    ("blob_yellow", include_bytes!("../../../assets/sprites/blob_yellow.tga")),
    ("gem_blue", include_bytes!("../../../assets/sprites/gem_blue.tga")),
    ("gem_cyan", include_bytes!("../../../assets/sprites/gem_cyan.tga")),
    ("gem_gray", include_bytes!("../../../assets/sprites/gem_gray.tga")),
    ("gem_green", include_bytes!("../../../assets/sprites/gem_green.tga")),
    ("gem_orange", include_bytes!("../../../assets/sprites/gem_orange.tga")),
    ("gem_purple", include_bytes!("../../../assets/sprites/gem_purple.tga")),
    ("gem_red", include_bytes!("../../../assets/sprites/gem_red.tga")),
    ("gem_yellow", include_bytes!("../../../assets/sprites/gem_yellow.tga")),
    ("half_blue", include_bytes!("../../../assets/sprites/half_blue.tga")),
    ("half_red", include_bytes!("../../../assets/sprites/half_red.tga")),
    ("half_yellow", include_bytes!("../../../assets/sprites/half_yellow.tga")),
    ("host", include_bytes!("../../../assets/sprites/host.tga")),
    ("single_blue", include_bytes!("../../../assets/sprites/single_blue.tga")),
    ("single_red", include_bytes!("../../../assets/sprites/single_red.tga")),
    ("single_yellow", include_bytes!("../../../assets/sprites/single_yellow.tga")),
    ("stone", include_bytes!("../../../assets/sprites/stone.tga")),
    ("tile_blue", include_bytes!("../../../assets/sprites/tile_blue.tga")),
    ("tile_cyan", include_bytes!("../../../assets/sprites/tile_cyan.tga")),
    ("tile_green", include_bytes!("../../../assets/sprites/tile_green.tga")),
    ("tile_purple", include_bytes!("../../../assets/sprites/tile_purple.tga")),
    ("tile_red", include_bytes!("../../../assets/sprites/tile_red.tga")),
    ("tile_yellow", include_bytes!("../../../assets/sprites/tile_yellow.tga")),
    ("virus_blue_0", include_bytes!("../../../assets/sprites/virus_blue_0.tga")),
    ("virus_blue_1", include_bytes!("../../../assets/sprites/virus_blue_1.tga")),
    ("virus_red_0", include_bytes!("../../../assets/sprites/virus_red_0.tga")),
    ("virus_red_1", include_bytes!("../../../assets/sprites/virus_red_1.tga")),
    ("virus_yellow_0", include_bytes!("../../../assets/sprites/virus_yellow_0.tga")),
    ("virus_yellow_1", include_bytes!("../../../assets/sprites/virus_yellow_1.tga")),
];

/// Decode an uncompressed 32-bit TGA into a canvas with straight alpha.
pub fn decode_tga(b: &[u8]) -> Option<Canvas> {
    if b.len() < 18 || b[2] != 2 || (b[16] != 32 && b[16] != 24) {
        return None;
    }
    let n = (b[16] / 8) as usize;
    let idl = b[0] as usize;
    let w = u16::from_le_bytes([b[12], b[13]]) as i32;
    let h = u16::from_le_bytes([b[14], b[15]]) as i32;
    let top = b[17] & 0x20 != 0;
    let data = b.get(18 + idl..18 + idl + (w * h) as usize * n)?;
    let mut cv = Canvas::transparent(w, h);
    for y in 0..h {
        let sy = if top { y } else { h - 1 - y };
        for x in 0..w {
            let i = (sy * w + x) as usize * n;
            let a = if n == 4 { data[i + 3] as u32 } else { 255 };
            let (bb, g, r) = (data[i] as u32, data[i + 1] as u32, data[i + 2] as u32);
            cv.px[(y * w + x) as usize] = (a << 24) | (r << 16) | (g << 8) | bb;
        }
    }
    Some(cv)
}

/// Resample: area average when shrinking, bilinear when enlarging (premultiplied either way,
/// so transparent edges never go dark).
pub fn resample(src: &Canvas, w: i32, h: i32) -> Canvas {
    if w >= src.w && h >= src.h && (w > src.w || h > src.h) {
        return upsample(src, w, h);
    }
    let mut out = Canvas::transparent(w.max(1), h.max(1));
    let sx = src.w as f32 / w as f32;
    let sy = src.h as f32 / h as f32;
    for y in 0..h {
        let y0 = (y as f32 * sy) as i32;
        let y1 = (((y + 1) as f32 * sy).ceil() as i32).min(src.h).max(y0 + 1);
        for x in 0..w {
            let x0 = (x as f32 * sx) as i32;
            let x1 = (((x + 1) as f32 * sx).ceil() as i32).min(src.w).max(x0 + 1);
            let (mut r, mut g, mut b, mut a, mut n) = (0u32, 0u32, 0u32, 0u32, 0u32);
            for yy in y0..y1 {
                for xx in x0..x1 {
                    let p = src.px[(yy * src.w + xx) as usize];
                    let pa = p >> 24;
                    r += ((p >> 16) & 255) * pa;
                    g += ((p >> 8) & 255) * pa;
                    b += (p & 255) * pa;
                    a += pa;
                    n += 1;
                }
            }
            let px = if a == 0 {
                0
            } else {
                ((a / n) << 24) | ((r / a) << 16) | ((g / a) << 8) | (b / a)
            };
            out.px[(y * w + x) as usize] = px;
        }
    }
    out
}

fn upsample(src: &Canvas, w: i32, h: i32) -> Canvas {
    let mut out = Canvas::transparent(w, h);
    let (sw, sh) = (src.w, src.h);
    let fx = sw as f32 / w as f32;
    let fy = sh as f32 / h as f32;
    // precomputed column taps
    let cols: Vec<(usize, usize, u32)> = (0..w)
        .map(|x| {
            let gx = ((x as f32 + 0.5) * fx - 0.5).max(0.0);
            let x0 = (gx as i32).min(sw - 1);
            let x1 = (x0 + 1).min(sw - 1);
            (x0 as usize, x1 as usize, ((gx - x0 as f32) * 256.0) as u32)
        })
        .collect();
    let pm = |p: u32| -> [u32; 4] {
        let a = p >> 24;
        [((p >> 16) & 255) * a, ((p >> 8) & 255) * a, (p & 255) * a, a * 255]
    };
    let work = (w * h) as usize * 4;
    crate::gfx::par_rows(&mut out.px, w as usize, 0, h, work, |y, row| {
        let gy = ((y as f32 + 0.5) * fy - 0.5).max(0.0);
        let y0 = (gy as i32).min(sh - 1);
        let y1 = (y0 + 1).min(sh - 1);
        let ty = ((gy - y0 as f32) * 256.0) as u32;
        let r0 = (y0 * sw) as usize;
        let r1 = (y1 * sw) as usize;
        for (x, &(x0, x1, tx)) in cols.iter().enumerate() {
            let (a, b, c, d) = (pm(src.px[r0 + x0]), pm(src.px[r0 + x1]), pm(src.px[r1 + x0]), pm(src.px[r1 + x1]));
            let mut o = [0u32; 4];
            for i in 0..4 {
                let top = a[i] * (256 - tx) + b[i] * tx;
                let bot = c[i] * (256 - tx) + d[i] * tx;
                o[i] = (top / 256 * (256 - ty) + bot / 256 * ty) / 256;
            }
            let al = o[3] / 255;
            row[x] = if al == 0 { 0 } else { (al.min(255) << 24) | ((o[0] / al).min(255) << 16) | ((o[1] / al).min(255) << 8) | (o[2] / al).min(255) };
        }
    });
    out
}

pub fn mirror_x(src: &Canvas) -> Canvas {
    let mut o = Canvas::transparent(src.w, src.h);
    for y in 0..src.h {
        for x in 0..src.w {
            o.px[(y * src.w + x) as usize] = src.px[(y * src.w + (src.w - 1 - x)) as usize];
        }
    }
    o
}
/// Rotate 90 degrees clockwise.
pub fn rot_cw(src: &Canvas) -> Canvas {
    let mut o = Canvas::transparent(src.h, src.w);
    for y in 0..src.h {
        for x in 0..src.w {
            o.px[(x * o.w + (src.h - 1 - y)) as usize] = src.px[(y * src.w + x) as usize];
        }
    }
    o
}

pub struct Art {
    map: HashMap<String, Canvas>,
}
impl Art {
    pub fn load() -> Self {
        let mut map = HashMap::new();
        for (n, b) in FILES {
            if let Some(c) = decode_tga(b) {
                map.insert(n.to_string(), c);
            }
        }
        // derived capsule halves: right = mirror, top = rotate cw, bottom = rotate ccw
        let halves: Vec<(String, Canvas)> = map.iter().filter(|(k, _)| k.starts_with("half_")).map(|(k, v)| (k.clone(), v.clone())).collect();
        for (k, v) in halves {
            let c = &k[5..];
            map.insert(format!("halfr_{}", c), mirror_x(&v));
            map.insert(format!("halft_{}", c), rot_cw(&v));
            map.insert(format!("halfb_{}", c), rot_cw(&rot_cw(&rot_cw(&v))));
        }
        Art { map }
    }
    pub fn get(&self, name: &str) -> Option<&Canvas> {
        self.map.get(name)
    }
}

static MAPS: [(&[u8], &str); 5] = [
    (include_bytes!("../../../assets/maps/world0.tga"), include_str!("../../../assets/maps/world0.txt")),
    (include_bytes!("../../../assets/maps/world1.tga"), include_str!("../../../assets/maps/world1.txt")),
    (include_bytes!("../../../assets/maps/world2.tga"), include_str!("../../../assets/maps/world2.txt")),
    (include_bytes!("../../../assets/maps/world3.tga"), include_str!("../../../assets/maps/world3.txt")),
    (include_bytes!("../../../assets/maps/world4.tga"), include_str!("../../../assets/maps/world4.txt")),
];

/// A rendered overworld and the screen position of each of its six nodes.
pub struct WorldMap {
    pub image: Canvas,
    pub nodes: Vec<(i32, i32)>,
}

pub fn load_maps() -> Vec<WorldMap> {
    MAPS.iter()
        .map(|(b, t)| WorldMap {
            image: decode_tga(b).unwrap_or_else(|| Canvas::new(1280, 720)),
            nodes: t
                .lines()
                .filter_map(|l| {
                    let mut it = l.split_whitespace().filter_map(|v| v.parse::<i32>().ok());
                    Some((it.next()?, it.next()?))
                })
                .collect(),
        })
        .collect()
}

// ------------------------------------------------------------------ UI icons (Phosphor Icons, MIT)
static ICONS_TGA: &[u8] = include_bytes!("../../../assets/sprites/icons.tga");
static ICONS_TXT: &str = include_str!("../../../assets/sprites/icons.txt");

thread_local! {
    static ICONS: std::cell::RefCell<Option<(Canvas, Vec<String>, HashMap<(String, i32), Canvas>)>> = const { std::cell::RefCell::new(None) };
}

/// Draw a named icon centred at (cx, cy), `size` pixels square, in one color.
pub fn icon(cv: &mut Canvas, name: &str, cx: i32, cy: i32, size: i32, color: u32, alpha: u32) {
    ICONS.with(|cell| {
        let mut b = cell.borrow_mut();
        if b.is_none() {
            let atlas = decode_tga(ICONS_TGA).unwrap_or_else(|| Canvas::transparent(1, 1));
            let names = ICONS_TXT.lines().map(|s| s.trim().to_string()).collect();
            *b = Some((atlas, names, HashMap::new()));
        }
        let (atlas, names, cache) = b.as_mut().unwrap();
        let Some(k) = names.iter().position(|n| n == name) else { return };
        let size = cv.us(size);
        let key = (name.to_string(), size);
        if !cache.contains_key(&key) {
            let (col, row) = (k as i32 % 8, k as i32 / 8);
            let mut cellc = Canvas::transparent(128, 128);
            for y in 0..128 {
                for x in 0..128 {
                    let sx = col * 128 + x;
                    let sy = row * 128 + y;
                    if sx < atlas.w && sy < atlas.h {
                        cellc.px[(y * 128 + x) as usize] = atlas.px[(sy * atlas.w + sx) as usize];
                    }
                }
            }
            cache.insert(key.clone(), resample(&cellc, size.max(4), size.max(4)));
        }
        if let Some(g) = cache.get(&key) {
            cv.blit_tint_px(g, cv.u(cx) - g.w / 2, cv.u(cy) - g.h / 2, color, alpha);
        }
    });
}

static ART: std::sync::OnceLock<Art> = std::sync::OnceLock::new();
pub fn art() -> &'static Art {
    ART.get_or_init(Art::load)
}

static LOGO_TGA: &[u8] = include_bytes!("../../../assets/sprites/logo.tga");
static LOGO: std::sync::OnceLock<Canvas> = std::sync::OnceLock::new();
thread_local! {
    static LOGO_SIZES: std::cell::RefCell<HashMap<i32, Canvas>> = std::cell::RefCell::new(HashMap::new());
}
/// Draw the logo `w` layout units wide at (x, y).
pub fn logo(cv: &mut Canvas, x: i32, y: i32, w: i32) {
    let src = LOGO.get_or_init(|| decode_tga(LOGO_TGA).unwrap_or_else(|| Canvas::transparent(2, 1)));
    let pw = cv.us(w);
    LOGO_SIZES.with(|c| {
        let mut m = c.borrow_mut();
        if m.len() > 8 {
            m.clear();
        }
        let img = m.entry(pw).or_insert_with(|| resample(src, pw, pw * src.h / src.w.max(1)));
        cv.blit(img, x, y, 255);
    });
}


// ------------------------------------------------------------------ the 3D cast
// Rendered in Blender by tools/blender/cast3d.py: pixiv's CC0 VRoid sample avatars (anime
// humans) and Quaternius's CC0 cute monsters (creatures), toon shaded with an ink outline.
// Humans have `<id>_<expr>_full` and `<id>_<expr>_bust`; creatures have `<id>_<expr>`.
include!(concat!(env!("OUT_DIR"), "/cast_files.rs"));

thread_local! {
    /// decoded sprites, and lit sizes made from them, keyed by (file, pixel height, mirrored, light rig)
    static CAST_IMG: std::cell::RefCell<(HashMap<&'static str, Canvas>, HashMap<(&'static str, i32, bool, u64), Canvas>)> = std::cell::RefCell::new((HashMap::new(), HashMap::new()));
}


/// The file for a cast member, expression and framing, falling back to a close expression.
fn cast_key(name: &str, expr: &str, bust: bool) -> Option<&'static str> {
    // the courier is drawn with whichever model the player picked
    let name = if name == "mira" { crate::cast::hero_sprite() } else { name };
    fn creature_expr(e: &str) -> &str {
        match e {
            "normal" | "smug" => "smile",
            "sleepy" => "sad",
            x => x,
        }
    }
    let tries: Vec<String> = {
        let mut v = Vec::new();
        let suffix = if bust { "bust" } else { "full" };
        v.push(format!("{}_{}_{}", name, expr, suffix));
        for e in ["smile", "normal"] {
            v.push(format!("{}_{}_{}", name, e, suffix));
        }
        v.push(format!("{}_{}", name, creature_expr(expr)));
        v.push(format!("{}_smile", name));
        v
    };
    for t in tries {
        if let Some((n, _)) = CAST_FILES.iter().find(|(n, _)| *n == t) {
            return Some(n);
        }
    }
    None
}

/// Whether a speaker has a body to draw (story voices, like a box or an intercom, do not).
pub fn has_art(name: &str) -> bool {
    cast_key(name, "normal", false).is_some()
}

/// Whether a cast id is one of the creatures (they have no separate bust framing).
pub fn is_creature(name: &str) -> bool {
    !CAST_FILES.iter().any(|(n, _)| n.starts_with(name) && n.ends_with("_full"))
}


fn decoded(src: &mut HashMap<&'static str, Canvas>, key: &str) -> Option<&'static str> {
    let (name, bytes) = CAST_FILES.iter().find(|(n, _)| *n == key)?;
    if !src.contains_key(name) {
        src.insert(name, crate::png::decode(bytes)?);
    }
    Some(name)
}

/// The sprite at pixel height `ph`, lit by the current light rig (light.rs) when it has a
/// lighting map, mirrored if asked.
fn with_cast<R>(key: &'static str, ph: i32, flip: bool, f: impl FnOnce(&Canvas) -> R) -> Option<R> {
    let rig = crate::light::current();
    CAST_IMG.with(|c| {
        let mut c = c.borrow_mut();
        let (src, sized) = &mut *c;
        decoded(src, key)?;
        let k = (key, ph, flip, rig.id());
        if !sized.contains_key(&k) {
            // big story sprites are ~10 MB each at 1440p: keep the cache bounded by size
            let total: usize = sized.values().map(|c| c.px.len()).sum();
            if sized.len() > 96 || total > 48_000_000 {
                sized.clear();
            }
            let data = decoded(src, &format!("{}_n", key));
            let s = &src[key];
            let lit;
            let s = match data {
                Some(d) => {
                    lit = crate::light::light(s, &src[d], &rig, flip);
                    &lit
                }
                None => s,
            };
            let w = (s.w * ph / s.h.max(1)).max(1);
            let mut img = resample(s, w, ph);
            if flip {
                img = mirror_x(&img);
            }
            sized.insert(k, img);
        }
        Some(f(&sized[&k]))
    })
}

/// Draw a cast member standing with feet at (cx, bottom), the image `h` layout units tall.
pub fn draw_char(cv: &mut Canvas, name: &str, expr: &str, cx: i32, bottom: i32, h: i32, alpha: u32) {
    draw_char_ex(cv, name, expr, cx, bottom, h, alpha, false);
}

#[allow(clippy::too_many_arguments)]
pub fn draw_char_ex(cv: &mut Canvas, name: &str, expr: &str, cx: i32, bottom: i32, h: i32, alpha: u32, flip: bool) {
    let Some(key) = cast_key(name, expr, false) else { return };
    // creatures are square renders: draw them a bit smaller than people
    let h = if is_creature(name) { h * 11 / 20 } else { h };
    let ph = cv.us(h);
    let (cxp, bp) = (cv.u(cx), cv.u(bottom));
    let k = cv.k;
    // the puppet layer (puppet.rs): breathing, sway, reactions, a crossfade between expressions
    let mo = crate::puppet::motion(name, key, expr, false, cx, bottom, h);
    with_cast(key, ph, flip, |img| {
        // soft contact shadow under the feet (smaller and lighter while airborne), then the character
        let air = (1.0 - mo.pose.lift.max(0.0) * 6.0).clamp(0.4, 1.0);
        cv.ellipse(cx as f32, bottom as f32 - 4.0, img.w as f32 / k * 0.26 * mo.pose.sx * air, (h as f32 * 0.022).max(3.0) * air, 0xFF00_0000, (alpha as f32 / 3.0 * air) as u32);
        crate::puppet::blit(cv, img, cxp, bp, alpha, &mo.pose);
    });
    if let Some((pk, m)) = mo.prev {
        with_cast(pk, ph, flip, |img| crate::puppet::blit(cv, img, cxp, bp, (alpha as f32 * m) as u32, &mo.pose));
    }
}

/// Light and size a standing sprite now, so drawing it later costs nothing.
pub fn prewarm_char(cv: &Canvas, name: &str, expr: &str, h: i32, flip: bool) {
    let Some(key) = cast_key(name, expr, false) else { return };
    let h = if is_creature(name) { h * 11 / 20 } else { h };
    with_cast(key, cv.us(h), flip, |_| ());
}

/// Head-and-shoulders portrait `size` layout units square, top-left at (x, y).
#[allow(clippy::too_many_arguments)]
pub fn draw_portrait(cv: &mut Canvas, name: &str, expr: &str, x: i32, y: i32, size: i32, alpha: u32, flip: bool) {
    let Some(key) = cast_key(name, expr, true) else { return };
    let ph = cv.us(size);
    let (xp, yp) = (cv.u(x), cv.u(y));
    // small icons stay crisp; bigger portraits breathe and react like the full figures
    if size < 90 {
        with_cast(key, ph, flip, |img| cv.blit_px(img, xp + (ph - img.w) / 2, yp, alpha));
        return;
    }
    let mo = crate::puppet::motion(name, key, expr, true, x + size / 2, y + size, size);
    with_cast(key, ph, flip, |img| crate::puppet::blit(cv, img, xp + ph / 2, yp + img.h, alpha, &mo.pose));
    if let Some((pk, m)) = mo.prev {
        with_cast(pk, ph, flip, |img| crate::puppet::blit(cv, img, xp + ph / 2, yp + img.h, (alpha as f32 * m) as u32, &mo.pose));
    }
}

/// The rival who waits in each story world.
pub fn rival(world: usize) -> (&'static str, &'static str) {
    [("nami", "Nami"), ("hana", "Hana"), ("yuki", "Yuki"), ("rin", "Rin"), ("kaede", "Kaede")][world.min(4)]
}
