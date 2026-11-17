"""Paint the piece art for all four styles: glass blocks (Quad), jelly blobs (Chain), glossy
capsules and germs (Capsule), and beveled gem tiles (Panel). Every image is our own, shaded from
signed-distance height fields with a fixed key light, supersampled 4x, and written as 256x256
32-bit TGA into assets/sprites/ (the names the engine embeds). Needs numpy and Pillow.

  python3 tools/sprites/pieces.py [--preview out.png]
"""
import os
import struct
import sys

import numpy as np
from PIL import Image

N = 256
SS = 4
M = N * SS
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "assets", "sprites")

COLORS = {
    "red": 0xE8475A,
    "orange": 0xF59A3B,
    "yellow": 0xF5D547,
    "green": 0x5BD07A,
    "cyan": 0x47D1E8,
    "blue": 0x4C7BF2,
    "purple": 0xA66CF0,
    "gray": 0x8C93A8,
    "white": 0xD8DCE8,
}

# key light from the upper left, a little toward the viewer
L = np.array([-0.45, -0.62, 0.64])
L = L / np.linalg.norm(L)
H = L + np.array([0.0, 0.0, 1.0])
H = H / np.linalg.norm(H)

yy, xx = np.mgrid[0:M, 0:M].astype(np.float32)
U = (xx + 0.5) / M
V = (yy + 0.5) / M


def rgb(c):
    return np.array([(c >> 16) & 255, (c >> 8) & 255, c & 255], np.float32) / 255.0


def mixc(a, b, t):
    return a * (1 - t) + b * t


def smooth(e0, e1, x):
    t = np.clip((x - e0) / (e1 - e0), 0, 1)
    return t * t * (3 - 2 * t)


def sd_rbox(px, py, cx, cy, hw, hh, r):
    qx = np.abs(px - cx) - hw + r
    qy = np.abs(py - cy) - hh + r
    return np.hypot(np.maximum(qx, 0), np.maximum(qy, 0)) + np.minimum(np.maximum(qx, qy), 0) - r


def sd_circle(px, py, cx, cy, r):
    return np.hypot(px - cx, py - cy) - r


def smin(a, b, k):
    h = np.clip(0.5 + 0.5 * (b - a) / k, 0, 1)
    return mixc(b, a, h) - k * h * (1 - h)


def cover(d):
    """Edge coverage from a signed distance (in uv units), one supersample wide."""
    return np.clip(0.5 - d * M, 0, 1)


def normals(h):
    gy, gx = np.gradient(h)
    k = M * 0.9
    n = np.stack([-gx * k, -gy * k, np.ones_like(h)], -1)
    return n / np.linalg.norm(n, axis=-1, keepdims=True)


def light(n, spec_pow, spec_k):
    diff = np.clip((n * L).sum(-1), 0, 1)
    spec = np.clip((n * H).sum(-1), 0, 1) ** spec_pow * spec_k
    return diff, spec


def finish(col, alpha):
    """Premultiply, box-downsample the supersamples, unpremultiply; returns uint8 RGBA."""
    col = np.clip(col, 0, 1)
    a = np.clip(alpha, 0, 1)[..., None]
    pm = np.concatenate([col * a, a], -1).reshape(N, SS, N, SS, 4).mean((1, 3))
    out = np.zeros_like(pm)
    al = pm[..., 3:4]
    out[..., :3] = np.where(al > 1e-4, pm[..., :3] / np.maximum(al, 1e-4), 0)
    out[..., 3:] = al
    return (np.clip(out, 0, 1) * 255 + 0.5).astype(np.uint8)


def blob_layer(col, alpha, d, c, a=1.0):
    """Composite color c with coverage from distance d (soft) on top of (col, alpha)."""
    k = cover(d)[..., None] * a
    return mixc(col, c, k), alpha


def gloss_band(px, py, cx, cy, rx, ry, soft):
    d = np.hypot((px - cx) / rx, (py - cy) / ry) - 1
    return 1 - smooth(-soft, 0.02, d)


def sparkle(px, py, cx, cy, r):
    dx, dy = np.abs(px - cx), np.abs(py - cy)
    arms = np.maximum(np.clip(1 - dx / (r * 0.16), 0, 1) * np.clip(1 - dy / r, 0, 1), np.clip(1 - dy / (r * 0.16), 0, 1) * np.clip(1 - dx / r, 0, 1))
    core = np.clip(1 - np.hypot(dx, dy) / (r * 0.35), 0, 1)
    return np.clip(arms * 1.2 + core, 0, 1)


# ------------------------------------------------------------------ Quad: glass blocks
def glass_block(c, deep=False):
    base = rgb(c)
    d = sd_rbox(U, V, 0.5, 0.5, 0.47, 0.47, 0.15)
    bevel = 0.13 if not deep else 0.17
    t = np.clip(-d / bevel, 0, 1)
    h = np.sqrt(1 - (1 - t) ** 2) * bevel * 0.9
    n = normals(h)
    diff, spec = light(n, 40, 0.55)
    lit = base * (0.42 + 0.78 * diff[..., None])
    # the flat face: a cool-to-warm vertical tone, lighter at the top
    face = smooth(0.0, 0.02, -d - bevel)
    tone = mixc(np.minimum(base * 1.22 + 0.05, 1), base * 0.86, smooth(0.15, 0.9, V)[..., None])
    col = mixc(lit, tone, face[..., None])
    # inner glow at the bottom of the face (light passing through the glass)
    glow = np.exp(-((V - 0.80) / 0.12) ** 2) * face * 0.28
    col = col + (np.minimum(base * 1.5, 1) - col) * glow[..., None]
    col = col + spec[..., None]
    # glass gloss: a soft band over the upper face, clipped to an inner rounded rect
    inner = cover(sd_rbox(U, V, 0.5, 0.5, 0.36, 0.36, 0.10))
    g = gloss_band(U, V, 0.46, 0.18, 0.46, 0.30, 0.6) * smooth(0.52, 0.16, V) * inner * 0.42
    col = mixc(col, np.ones(3), g[..., None])
    col = mixc(col, np.ones(3), (sparkle(U, V, 0.27, 0.25, 0.085) * 0.9 * face)[..., None])
    # crisp dark rim and a light inner lip on the top edge
    rim = smooth(-0.022, -0.006, d)
    col = mixc(col, base * 0.38, rim[..., None])
    lip = np.exp(-((d + 0.03) / 0.008) ** 2) * smooth(0.6, 0.1, V) * 0.5
    col = mixc(col, np.ones(3), lip[..., None])
    return finish(col, cover(d))


# ------------------------------------------------------------------ Panel: gem tiles
def gem_tile(c):
    base = rgb(c)
    d = sd_rbox(U, V, 0.5, 0.5, 0.47, 0.47, 0.17)
    # a cut, chamfered bevel: linear height, so each side catches the light as one facet
    bevel = 0.15
    t = np.clip(-d / bevel, 0, 1)
    h = t * bevel * 0.8
    n = normals(h)
    diff, spec = light(n, 26, 0.35)
    lit = base * (0.38 + 0.85 * diff[..., None])
    face = smooth(0.0, 0.015, -d - bevel)
    r = np.hypot(U - 0.5, V - 0.46)
    tone = mixc(np.minimum(base * 1.18 + 0.04, 1), base * 0.80, smooth(0.0, 0.42, r)[..., None])
    col = mixc(lit, tone, face[..., None]) + spec[..., None] * (1 - face[..., None])
    g = gloss_band(U, V, 0.5, 0.17, 0.42, 0.20, 0.7) * face * 0.30
    col = mixc(col, np.ones(3), g[..., None])
    rim = smooth(-0.02, -0.005, d)
    col = mixc(col, base * 0.36, rim[..., None])
    return finish(col, cover(d))


# ------------------------------------------------------------------ Chain: jelly blobs
def jelly(c, clear=False):
    base = rgb(c)
    # a soft gumdrop: a little wider than tall, settled on its bottom
    px, py = (U - 0.5) / 0.465, (V - 0.535) / 0.445
    py = np.where(py > 0, py * 1.06, py)
    rr = np.hypot(px, py)
    d = (rr - 1) * 0.455
    z = np.sqrt(np.clip(1 - rr ** 2, 0, 1))
    n = np.stack([px, py * 0.95, z * 1.1], -1)
    n = n / np.linalg.norm(n, axis=-1, keepdims=True)
    diff, spec = light(n, 60, 0.9)
    deep = base * (0.55 if not clear else 0.75)
    col = mixc(deep, np.minimum(base * 1.15 + 0.04, 1), diff[..., None] ** 0.8)
    # subsurface: the lower inside glows brighter and more saturated
    sss = np.exp(-(((U - 0.52) / 0.30) ** 2 + ((V - 0.74) / 0.16) ** 2)) * 0.55
    col = mixc(col, np.minimum(base * 1.45 + 0.06, 1), sss[..., None])
    # rim light from behind
    fres = (1 - np.clip(n[..., 2], 0, 1)) ** 3
    col = col + np.minimum(base + 0.3, 1) * (fres * 0.32)[..., None]
    col = col + spec[..., None]
    # the big soft window highlight and a pin highlight
    hl = gloss_band(U, V, 0.36, 0.27, 0.15, 0.095, 0.9) * 0.82
    col = mixc(col, np.ones(3), hl[..., None])
    pin = np.clip(1 - np.hypot(U - 0.62, V - 0.22) / 0.028, 0, 1) * 0.8
    col = mixc(col, np.ones(3), pin[..., None])
    col = mixc(col, base * 0.35, smooth(-0.02, -0.004, d)[..., None])
    a = cover(d)
    if clear:
        a = a * 0.94
    return finish(col, a)


# ------------------------------------------------------------------ Capsule: glossy pills
def capsule_half(c):
    base = rgb(c)
    R = 0.355
    cy = 0.5
    # left half of a pill: a round cap on the left, cut flat at the right edge (the seam)
    d_cap = sd_circle(U, V, 0.06 + R, cy, R)
    # the shape (for coverage) and the tube profile (for shading): the cut edge gets no bevel
    d_body = np.where(U >= 0.06 + R, np.abs(V - cy) - R, 9.0)
    d = np.minimum(d_cap, d_body)
    d = np.maximum(d, U - 1.5)
    t = np.clip(-d / R, 0, 1)
    h = np.sqrt(1 - (1 - t) ** 2) * R
    n = normals(h)
    diff, spec = light(n, 70, 1.0)
    col = mixc(base * 0.5, np.minimum(base * 1.12 + 0.05, 1), diff[..., None] ** 0.9)
    under = np.exp(-((V - (cy + R * 0.62)) / 0.07) ** 2) * cover(d) * 0.35
    col = mixc(col, np.minimum(base * 1.5, 1), under[..., None])
    col = col + spec[..., None]
    streak = (1 - smooth(0.0, 0.03, np.abs(V - (cy - R * 0.55)) - 0.035)) * smooth(0.17, 0.30, U) * 0.85
    col = mixc(col, np.ones(3), streak[..., None])
    # the seam: a faint groove where the two halves meet
    seam = np.exp(-((U - 0.995) / 0.012) ** 2) * 0.35
    col = mixc(col, base * 0.45, seam[..., None])
    col = mixc(col, base * 0.36, smooth(-0.02, -0.004, d)[..., None])
    return finish(col, cover(d))


def capsule_single(c):
    base = rgb(c)
    R = 0.36
    d = sd_circle(U, V, 0.5, 0.5, R)
    t = np.clip(-d / R, 0, 1)
    h = np.sqrt(1 - (1 - t) ** 2) * R
    n = normals(h)
    diff, spec = light(n, 70, 1.0)
    col = mixc(base * 0.5, np.minimum(base * 1.12 + 0.05, 1), diff[..., None] ** 0.9) + spec[..., None]
    hl = gloss_band(U, V, 0.40, 0.31, 0.13, 0.07, 0.9) * 0.85
    col = mixc(col, np.ones(3), hl[..., None])
    col = mixc(col, base * 0.36, smooth(-0.02, -0.004, d)[..., None])
    return finish(col, cover(d))


def germ(c, frame):
    base = rgb(c)
    rot = 0.0 if frame == 0 else np.pi / 8
    sq = 1.0 if frame == 0 else 1.04
    px, py = 0.5 + (U - 0.5) / sq, 0.5 + (V - 0.5) * sq
    d = sd_circle(px, py, 0.5, 0.52, 0.30)
    for k in range(8):
        a = rot + k * np.pi / 4
        d = smin(d, sd_circle(px, py, 0.5 + np.cos(a) * 0.335, 0.52 + np.sin(a) * 0.335, 0.075), 0.06)
    t = np.clip(-d / 0.16, 0, 1)
    h = np.sqrt(1 - (1 - t) ** 2) * 0.16
    n = normals(h)
    diff, spec = light(n, 30, 0.45)
    col = mixc(base * 0.45, np.minimum(base * 1.1 + 0.05, 1), diff[..., None])
    # a few soft darker freckles on the body
    for fx, fy, fr in [(0.36, 0.66, 0.045), (0.64, 0.70, 0.035), (0.70, 0.40, 0.03), (0.30, 0.44, 0.028)]:
        f = np.clip(1 - np.hypot(px - fx, py - fy) / fr, 0, 1) ** 0.6 * 0.22
        col = col * (1 - f[..., None])
    col = col + spec[..., None]
    hl = gloss_band(U, V, 0.38, 0.33, 0.11, 0.06, 0.9) * 0.7
    col = mixc(col, np.ones(3), hl[..., None])
    col = mixc(col, base * 0.32, smooth(-0.02, -0.004, d)[..., None])
    return finish(col, cover(d))


def stone():
    base = rgb(0x7A8096)
    d = sd_rbox(U, V, 0.5, 0.5, 0.47, 0.47, 0.13)
    t = np.clip(-d / 0.12, 0, 1)
    h = np.sqrt(1 - (1 - t) ** 2) * 0.1
    for fx, fy, fr in [(0.32, 0.36, 0.12), (0.66, 0.62, 0.15), (0.70, 0.28, 0.08)]:
        h = h - np.clip(1 - np.hypot(U - fx, V - fy) / fr, 0, 1) ** 2 * 0.012
    n = normals(h)
    diff, spec = light(n, 12, 0.12)
    col = base * (0.45 + 0.7 * diff[..., None]) + spec[..., None]
    col = mixc(col, base * 0.35, smooth(-0.02, -0.005, d)[..., None])
    return finish(col, cover(d))


def write_tga(path, img):
    h, w, _ = img.shape
    bgra = img[..., [2, 1, 0, 3]]
    with open(path, "wb") as f:
        f.write(struct.pack("<BBBHHBHHHHBB", 0, 0, 2, 0, 0, 0, 0, 0, w, h, 32, 0x28))
        f.write(bgra.tobytes())


def main():
    out = {}
    for n in ["red", "orange", "yellow", "green", "cyan", "blue", "purple", "gray"]:
        out["gem_" + n] = glass_block(COLORS[n])
    for n in ["red", "green", "blue", "yellow", "purple"]:
        out["blob_" + n] = jelly(COLORS[n])
    out["blob_white"] = jelly(COLORS["white"], clear=True)
    for n in ["red", "yellow", "blue"]:
        out["half_" + n] = capsule_half(COLORS[n])
        out["single_" + n] = capsule_single(COLORS[n])
        for fr in (0, 1):
            out["virus_%s_%d" % (n, fr)] = germ(COLORS[n], fr)
    for n in ["red", "yellow", "green", "cyan", "purple", "blue"]:
        out["tile_" + n] = gem_tile(COLORS[n])
    out["stone"] = stone()
    for name, img in out.items():
        write_tga(os.path.join(OUT, name + ".tga"), img)
        print("wrote", name)
    if "--preview" in sys.argv:
        p = sys.argv[sys.argv.index("--preview") + 1]
        names = list(out)
        cols = 8
        rows = (len(names) + cols - 1) // cols
        sheet = Image.new("RGBA", (cols * 136, rows * 136), (14, 20, 44, 255))
        for i, nm in enumerate(names):
            im = Image.fromarray(out[nm], "RGBA").resize((128, 128), Image.LANCZOS)
            sheet.alpha_composite(im, ((i % cols) * 136 + 4, (i // cols) * 136 + 4))
        sheet.save(p)


if __name__ == "__main__":
    main()
