//! Cel lighting for the 3D cast. Blender renders each pose twice (tools/blender/cast3d.py
//! `passes`): the unlit albedo, and a half-size map of camera-space normals (R, G) with a
//! material class (B). The game lights them here with the light of wherever they stand: a key
//! light cut into two hard tones (lit / shade, with an anime shade tint), a sky-and-ground
//! ambient, a banded hair highlight and a rim light from the environment (lava glow, moonlight,
//! sea glare). Results are cached per rig, so lighting costs nothing per frame.

use crate::gfx::Canvas;

/// A lighting setup. Directions are in screen space: x right, y up, z toward the viewer.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rig {
    /// toward the key light
    pub key_dir: [f32; 3],
    /// key light color times strength (linear)
    pub key: [f32; 3],
    /// what the shade side keeps of the key light (anime shade tint)
    pub shade: [f32; 3],
    /// ambient from above and bounce from below (linear)
    pub sky: [f32; 3],
    pub ground: [f32; 3],
    /// the screen direction a rim light comes from, and its color
    pub rim_dir: [f32; 2],
    pub rim: [f32; 3],
    /// where the lit / shade cut falls on N.L (lower = more of the body lit)
    pub cut: f32,
}

impl Rig {
    /// Even, slightly warm studio light for menus, the select screen and boards.
    pub const STUDIO: Rig = Rig {
        key_dir: [-0.66, 0.42, 0.62],
        key: [0.86, 0.83, 0.79],
        shade: [0.62, 0.58, 0.80],
        sky: [0.25, 0.27, 0.33],
        ground: [0.17, 0.15, 0.17],
        rim_dir: [0.8, 0.6],
        rim: [0.20, 0.22, 0.28],
        cut: 0.22,
    };

    /// A stable id for caches.
    pub fn id(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let v = [
            self.key_dir[0], self.key_dir[1], self.key_dir[2], self.key[0], self.key[1], self.key[2], self.shade[0], self.shade[1], self.shade[2], self.sky[0], self.sky[1], self.sky[2],
            self.ground[0], self.ground[1], self.ground[2], self.rim_dir[0], self.rim_dir[1], self.rim[0], self.rim[1], self.rim[2], self.cut,
        ];
        for f in v {
            h ^= f.to_bits() as u64;
            h = h.wrapping_mul(0x100_0000_01b3);
        }
        h
    }
}

thread_local! {
    static CURRENT: std::cell::Cell<Rig> = const { std::cell::Cell::new(Rig::STUDIO) };
}
/// The light the cast is drawn with from now on (screens set it before drawing characters).
pub fn set(r: Rig) {
    CURRENT.with(|c| c.set(r));
}
pub fn current() -> Rig {
    CURRENT.with(|c| c.get())
}

fn srgb_to_lin_lut() -> &'static [f32; 256] {
    static L: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
    L.get_or_init(|| {
        let mut t = [0.0; 256];
        for (i, v) in t.iter_mut().enumerate() {
            let c = i as f32 / 255.0;
            *v = if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) };
        }
        t
    })
}
const OUT_N: usize = 4096;
fn lin_to_srgb_lut() -> &'static [u8; OUT_N] {
    static L: std::sync::OnceLock<[u8; OUT_N]> = std::sync::OnceLock::new();
    L.get_or_init(|| {
        let mut t = [0u8; OUT_N];
        for (i, v) in t.iter_mut().enumerate() {
            let c = i as f32 / (OUT_N - 1) as f32;
            let s = if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
            *v = (s * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
        }
        t
    })
}

#[inline]
fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Soft shoulder so bright rims and highlights roll off instead of clipping flat.
#[inline]
fn tone(x: f32) -> f32 {
    if x <= 0.82 {
        x.max(0.0)
    } else {
        0.82 + 0.18 * (1.0 - (-(x - 0.82) / 0.18).exp())
    }
}

/// How each material class (B channel: unlit 0, face .25, skin .5, cloth .75, hair 1) takes light.
#[derive(Clone, Copy)]
struct Mat {
    /// how much of the shade tone shows
    shade: f32,
    /// hair highlight
    spec: f32,
    rim: f32,
    /// 1 = drawn as is (eyes, brows, mouth, ink), only dimmed by the scene's brightness
    unlit: f32,
    /// moves the lit / shade cut: faces stay lit except at the far edge, as anime faces are drawn
    cut: f32,
    /// skin takes a warm, rosy shade instead of the scene's cool one
    warm: f32,
}

#[inline]
fn material(cls: f32) -> Mat {
    const T: [Mat; 5] = [
        Mat { shade: 0.0, spec: 0.0, rim: 0.0, unlit: 1.0, cut: 0.0, warm: 0.0 },
        Mat { shade: 0.62, spec: 0.0, rim: 0.5, unlit: 0.0, cut: -0.42, warm: 1.0 },
        Mat { shade: 0.85, spec: 0.0, rim: 0.85, unlit: 0.0, cut: -0.14, warm: 1.0 },
        Mat { shade: 1.0, spec: 0.0, rim: 1.0, unlit: 0.0, cut: 0.0, warm: 0.0 },
        Mat { shade: 1.0, spec: 1.0, rim: 1.0, unlit: 0.0, cut: 0.0, warm: 0.0 },
    ];
    // piecewise linear, so blended edges between classes stay smooth
    let f = (cls.clamp(0.0, 1.0) * 4.0).min(3.999);
    let i = f as usize;
    let t = f - i as f32;
    let (a, b) = (T[i], T[i + 1]);
    let m = |x: f32, y: f32| x + (y - x) * t;
    Mat { shade: m(a.shade, b.shade), spec: m(a.spec, b.spec), rim: m(a.rim, b.rim), unlit: m(a.unlit, b.unlit), cut: m(a.cut, b.cut), warm: m(a.warm, b.warm) }
}

/// The rosy shade skin takes (multiplied into the key light on the shade side).
const SKIN_SHADE: [f32; 3] = [0.86, 0.62, 0.64];

/// Light an albedo image with its data map under `rig`. `flip` means the result will be
/// mirrored on screen, so the light is mirrored here to land on the correct side.
pub fn light(albedo: &Canvas, data: &Canvas, rig: &Rig, flip: bool) -> Canvas {
    let data = if data.w != albedo.w || data.h != albedo.h { crate::assets::resample(data, albedo.w, albedo.h) } else { data.clone() };
    let s2l = srgb_to_lin_lut();
    let l2s = lin_to_srgb_lut();
    let fx = if flip { -1.0 } else { 1.0 };
    let norm = |v: [f32; 3]| {
        let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
        [v[0] / l, v[1] / l, v[2] / l]
    };
    let ld = norm([rig.key_dir[0] * fx, rig.key_dir[1], rig.key_dir[2]]);
    // half vector for the hair highlight (viewer straight on), lifted a little: anime "angel rings"
    // sit high on the head
    let hv = norm([ld[0] * 0.6, ld[1] * 0.6 + 0.25, ld[2] + 1.0]);
    let rd = {
        let (x, y) = (rig.rim_dir[0] * fx, rig.rim_dir[1]);
        let l = (x * x + y * y).sqrt().max(1e-6);
        (x / l, y / l)
    };
    // average ambient, for the unlit layers (eyes keep their color but not their brightness at night)
    let amb_avg = [
        (rig.key[0] + (rig.sky[0] + rig.ground[0]) * 0.5) / (Rig::STUDIO.key[0] + (Rig::STUDIO.sky[0] + Rig::STUDIO.ground[0]) * 0.5),
        (rig.key[1] + (rig.sky[1] + rig.ground[1]) * 0.5) / (Rig::STUDIO.key[1] + (Rig::STUDIO.sky[1] + Rig::STUDIO.ground[1]) * 0.5),
        (rig.key[2] + (rig.sky[2] + rig.ground[2]) * 0.5) / (Rig::STUDIO.key[2] + (Rig::STUDIO.sky[2] + Rig::STUDIO.ground[2]) * 0.5),
    ];
    let mut out = Canvas::transparent(albedo.w, albedo.h);
    let w = albedo.w as usize;
    let work = (albedo.w * albedo.h) as usize * 8;
    let r = *rig;
    crate::gfx::par_rows(&mut out.px, w, 0, albedo.h, work, |y, row| {
        let base = y as usize * w;
        for x in 0..w {
            let a = albedo.px[base + x];
            let al = a >> 24;
            if al == 0 {
                continue;
            }
            let d = data.px[base + x];
            let base_c = [s2l[((a >> 16) & 255) as usize], s2l[((a >> 8) & 255) as usize], s2l[(a & 255) as usize]];
            let (nx, ny, cls) = if d >> 24 == 0 {
                (0.0, 0.0, 0.75)
            } else {
                (((d >> 16) & 255) as f32 / 127.5 - 1.0, ((d >> 8) & 255) as f32 / 127.5 - 1.0, (d & 255) as f32 / 255.0)
            };
            let nz = (1.0 - nx * nx - ny * ny).max(0.0).sqrt();
            let m = material(cls);
            // two-tone cel: a hard (but anti-aliased) cut between lit and shade
            let ndl = nx * ld[0] + ny * ld[1] + nz * ld[2];
            let cut = r.cut + m.cut;
            let lit = smoothstep(cut - 0.035, cut + 0.035, ndl);
            let lit = 1.0 - (1.0 - lit) * m.shade;
            let hemi = ny * 0.5 + 0.5;
            let fres = 1.0 - nz;
            let rim = smoothstep(0.28, 0.56, fres) * ((nx * rd.0 + ny * rd.1) * 1.4).clamp(0.0, 1.0) * m.rim;
            let ndh = nx * hv[0] + ny * hv[1] + nz * hv[2];
            let hi = smoothstep(0.93, 0.945, ndh) * m.spec * lit;
            // bright hair (white, blonde) is already near the top: lift it less, or it blows out
            let lum = base_c[0] * 0.3 + base_c[1] * 0.55 + base_c[2] * 0.15;
            let hi = hi * (1.0 - lum.min(1.0) * 0.8);
            let mut c = [0u32; 3];
            for k in 0..3 {
                let shade = r.shade[k] + (r.shade[k] * SKIN_SHADE[k] * 1.25 - r.shade[k]) * m.warm;
                let key = r.key[k] * (shade + (1.0 - shade) * lit);
                let amb = r.ground[k] + (r.sky[k] - r.ground[k]) * hemi;
                // the hair highlight is a lighter shade of the hair itself, not a white sticker
                let lit_c = base_c[k] * (key + amb) * (1.0 + hi * 1.1) + r.key[k] * hi * 0.025 + r.rim[k] * rim;
                let unlit_c = base_c[k] * (0.45 + 0.55 * amb_avg[k]);
                let v = lit_c + (unlit_c - lit_c) * m.unlit;
                c[k] = l2s[(tone(v) * (OUT_N - 1) as f32) as usize] as u32;
            }
            row[x] = (al << 24) | (c[0] << 16) | (c[1] << 8) | c[2];
        }
    });
    out
}

/// The time of day or mood a story scene is lit for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mood {
    Day,
    Dusk,
    Night,
    /// a showdown: lower, harder light and a strong rim
    Duel,
    /// after a win: warm, low, golden
    Golden,
}

/// Each world's light: sun, sky, what the ground bounces back, and what rims the silhouette.
pub fn world(w: usize, mood: Mood) -> Rig {
    let day = match w {
        // Quad Reef: bright tropical sun, blue sky, sea glare from below
        0 => Rig { key_dir: [-0.45, 0.55, 0.70], key: [0.92, 0.86, 0.76], shade: [0.50, 0.58, 0.86], sky: [0.20, 0.30, 0.42], ground: [0.08, 0.20, 0.26], rim_dir: [0.85, -0.2], rim: [0.30, 0.48, 0.58], cut: 0.20 },
        // Bloom Valley: soft golden sun, green bounce off the meadow
        1 => Rig { key_dir: [-0.40, 0.60, 0.70], key: [0.92, 0.85, 0.66], shade: [0.55, 0.60, 0.80], sky: [0.22, 0.28, 0.36], ground: [0.12, 0.20, 0.07], rim_dir: [0.8, 0.4], rim: [0.42, 0.40, 0.24], cut: 0.18 },
        // Clinic Moon: cold moonlight, violet sky, a pale rim
        2 => Rig { key_dir: [0.40, 0.62, 0.68], key: [0.58, 0.64, 0.86], shade: [0.42, 0.42, 0.72], sky: [0.14, 0.15, 0.30], ground: [0.07, 0.07, 0.13], rim_dir: [-0.8, 0.5], rim: [0.40, 0.52, 0.95], cut: 0.24 },
        // Tile Bazaar: hard desert sun, sand bounce
        3 => Rig { key_dir: [-0.50, 0.66, 0.56], key: [1.00, 0.86, 0.64], shade: [0.62, 0.52, 0.66], sky: [0.26, 0.26, 0.30], ground: [0.30, 0.20, 0.10], rim_dir: [0.9, 0.1], rim: [0.55, 0.40, 0.22], cut: 0.22 },
        // Cinder Crown: dim ash light above, lava glow from below and behind
        _ => Rig { key_dir: [-0.35, 0.40, 0.84], key: [0.66, 0.46, 0.40], shade: [0.48, 0.34, 0.52], sky: [0.11, 0.09, 0.13], ground: [0.50, 0.18, 0.05], rim_dir: [0.35, -0.94], rim: [1.10, 0.42, 0.10], cut: 0.26 },
    };
    match mood {
        Mood::Day => day,
        Mood::Dusk => Rig {
            key_dir: [-0.70, 0.22, 0.68],
            key: [0.95 * 0.92, 0.62 * 0.92, 0.40 * 0.92],
            shade: [0.42, 0.40, 0.70],
            sky: [day.sky[0] * 0.55 + 0.04, day.sky[1] * 0.50 + 0.03, day.sky[2] * 0.70 + 0.08],
            ground: [day.ground[0] * 0.6 + 0.04, day.ground[1] * 0.5 + 0.02, day.ground[2] * 0.5 + 0.03],
            rim_dir: [0.85, 0.35],
            rim: [0.50, 0.30, 0.55],
            cut: 0.26,
        },
        Mood::Night => Rig {
            key_dir: [0.45, 0.62, 0.64],
            key: [0.36, 0.42, 0.64],
            shade: [0.38, 0.40, 0.70],
            sky: [0.07, 0.09, 0.19],
            ground: [day.ground[0] * 0.35, day.ground[1] * 0.35, day.ground[2] * 0.45 + 0.02],
            rim_dir: [-0.8, 0.45],
            rim: [0.38, 0.50, 0.95],
            cut: 0.26,
        },
        Mood::Duel => Rig {
            key_dir: [day.key_dir[0] * 1.3, day.key_dir[1] * 0.7, day.key_dir[2] * 0.75],
            key: [day.key[0] * 1.05, day.key[1] * 1.0, day.key[2] * 0.95],
            shade: [day.shade[0] * 0.8, day.shade[1] * 0.8, day.shade[2] * 0.85],
            sky: [day.sky[0] * 0.7, day.sky[1] * 0.7, day.sky[2] * 0.75],
            ground: day.ground,
            rim_dir: day.rim_dir,
            rim: [day.rim[0] * 1.6 + 0.08, day.rim[1] * 1.6 + 0.05, day.rim[2] * 1.6 + 0.04],
            cut: day.cut + 0.10,
        },
        Mood::Golden => Rig {
            key_dir: [-0.62, 0.34, 0.70],
            key: [1.00, 0.80, 0.55],
            shade: [0.56, 0.48, 0.70],
            sky: [day.sky[0] * 0.8 + 0.06, day.sky[1] * 0.75 + 0.03, day.sky[2] * 0.7],
            ground: [day.ground[0] * 0.8 + 0.06, day.ground[1] * 0.8 + 0.03, day.ground[2] * 0.7],
            rim_dir: [0.9, 0.3],
            rim: [0.70, 0.50, 0.28],
            cut: 0.22,
        },
    }
}

/// How the backdrop is graded to match a rig: (multiply color, strength 0..255, top glow color).
pub fn grade(w: usize, mood: Mood) -> (u32, u32, u32) {
    match mood {
        Mood::Day => (0xFFFF_FFFF, 0, 0xFFFF_F4DC),
        Mood::Dusk => (0xFF6A_3C66, 120, 0xFFFF_9A5C),
        Mood::Night => (0xFF10_1840, 175, 0xFF6E_88E0),
        Mood::Duel => (if w == 4 { 0xFF40_1010 } else { 0xFF20_1830 }, 110, if w == 4 { 0xFFFF_6A2A } else { 0xFFFF_D0A0 }),
        Mood::Golden => (0xFFB0_6A30, 80, 0xFFFF_C878),
    }
}
