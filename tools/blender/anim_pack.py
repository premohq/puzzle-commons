"""Pack a character's rendered clip frames (target/anim_frames/<id>, from anim3d.py) into
assets/anim/<id>.pca, a small video format the game decodes frame by frame (anim.rs).

Run with Blender's Python (for numpy):
  blender -b --factory-startup --python tools/blender/anim_pack.py -- <repo> [ids] [q:N]

Planes: albedo as Y Co Cg (lossy) and alpha (lossless) at full size; the lighting map at half
size as normal x, y (lossy) and material class (lossless). Each 16 x 16 block of a frame is
predicted from the previous decoded frame, moved by a motion vector, and only the difference
is stored: as an 8 x 8 DCT with quantized coefficients for the lossy planes, as exact bytes
for the lossless ones. The inverse DCT is integer arithmetic, defined identically here and
in anim.rs, so the game rebuilds exactly the frames this encoder predicted from (no drift).
The first frame of idle is predicted from a blank frame; every other clip starts from idle's
first frame. Streams are deflated (zlib).

File: "PCA1", u16 width, height, fps, clip count, still x, still y, still w, still h (where
the 448 x 960 still sits in the frame, so the game can line them up), quality; then per clip: u8 name
length, name, u16 frames, f32 seconds, u8 loop, u8 starts-from-idle, 5 x u32 stream lengths;
then every clip's five streams.
"""
import os
import struct
import sys
import zlib

import bpy
import numpy as np

ARGV = sys.argv[sys.argv.index("--") + 1:]
REPO = ARGV[0]
ONLY = set(a for a in ARGV[1:] if ":" not in a)
QUAL = next((int(a[2:]) for a in ARGV[1:] if a.startswith("q:")), 82)
SRC = os.path.join(REPO, "target", "anim_frames")
OUT = os.path.join(REPO, "assets", "anim")
ORDER = ["idle", "talk", "attack", "cast", "hit", "hit_big", "worried", "win", "lose"]
MB = 16
R = 4  # motion search range, pixels

# where the still sits inside an animation frame (anim3d.py margins)
STILL = (64, 64, 448, 960)

ZIGZAG = np.array([
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21,
    28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54,
    47, 55, 62, 63])
T_LUM = np.array([
    16, 11, 10, 16, 24, 40, 51, 61, 12, 12, 14, 19, 26, 58, 60, 55, 14, 13, 16, 24, 40, 57, 69, 56, 14, 17, 22, 29, 51, 87, 80, 62,
    18, 22, 37, 56, 68, 109, 103, 77, 24, 35, 55, 64, 81, 104, 113, 92, 49, 64, 78, 87, 103, 121, 120, 101, 72, 92, 95, 98, 112, 100, 103, 99], dtype=np.float64).reshape(8, 8)
T_CHR = np.array([
    17, 18, 24, 47, 99, 99, 99, 99, 18, 21, 26, 66, 99, 99, 99, 99, 24, 26, 56, 99, 99, 99, 99, 99, 47, 66, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99], dtype=np.float64).reshape(8, 8)


def qtable(t, q):
    s = 5000.0 / q if q < 50 else 200.0 - 2.0 * q
    return np.clip(np.floor((t * s + 50.0) / 100.0), 1, 255).astype(np.int64)


Q_Y = qtable(T_LUM, QUAL)
Q_C = qtable(T_CHR, QUAL)
Q_N = qtable(T_LUM, min(95, QUAL + 6))

# DCT basis: C[u][x] = c(u) cos((2x+1) u pi / 16), orthonormal
_C = np.array([[(np.sqrt(1 / 8) if u == 0 else 0.5) * np.cos((2 * x + 1) * u * np.pi / 16) for x in range(8)] for u in range(8)])
CI = np.round(_C * 4096).astype(np.int64)  # the integer basis anim.rs uses too


def fdct(blocks):
    """(N, 8, 8) float -> coefficients (N, 8, 8)."""
    return np.einsum("ux,nxy,vy->nuv", _C, blocks, _C)


def idct_int(coef):
    """Integer inverse DCT, exactly as anim.rs: (N, 8, 8) int64 -> (N, 8, 8) int64."""
    # rows: tmp[v][x] = sum_u F[v][u] C[u][x]
    tmp = np.einsum("nvu,ux->nvx", coef, CI)
    tmp = (tmp + 256) >> 9
    # columns: out[y][x] = sum_v C[v][y] tmp[v][x]
    out = np.einsum("vy,nvx->nyx", CI, tmp)
    return (out + (1 << 14)) >> 15


def load(path):
    img = bpy.data.images.load(path)
    img.colorspace_settings.name = "Non-Color"
    w, h = img.size
    a = np.empty(w * h * 4, dtype=np.float32)
    img.pixels.foreach_get(a)
    bpy.data.images.remove(img)
    return np.flipud((a.reshape(h, w, 4) * 255.0 + 0.5).astype(np.int64)).copy()


def ycocg(rgb):
    r, g, b = rgb[..., 0], rgb[..., 1], rgb[..., 2]
    y = (r + 2 * g + b + 2) >> 2
    co = ((r - b + 1) >> 1) + 128
    cg = ((-r + 2 * g - b + 2) >> 2) + 128
    return np.clip(y, 0, 255), np.clip(co, 0, 255), np.clip(cg, 0, 255)


def to_rgb(y, co, cg):
    t = y - (cg - 128)
    g = y + (cg - 128)
    r = t + (co - 128)
    b = t - (co - 128)
    return np.clip(r, 0, 255), np.clip(g, 0, 255), np.clip(b, 0, 255)


def dilate(vals, mask, steps):
    """Spread values from masked pixels into the unmasked ring around them."""
    vals = vals.copy()
    m = mask.copy()
    for _ in range(steps):
        add = np.zeros_like(m)
        acc = np.zeros(vals.shape, dtype=np.int64)
        cnt = np.zeros(m.shape, dtype=np.int64)
        for dy, dx in ((0, 1), (0, -1), (1, 0), (-1, 0)):
            sm = np.roll(m, (dy, dx), (0, 1))
            sv = np.roll(vals, (dy, dx), (0, 1))
            acc += sv * sm[..., None] if vals.ndim == 3 else sv * sm
            cnt += sm
        grow = (~m) & (cnt > 0)
        if vals.ndim == 3:
            vals[grow] = acc[grow] // cnt[grow][:, None]
        else:
            vals[grow] = acc[grow] // cnt[grow]
        m = m | grow
    return vals, m


def blocks_of(plane, size):
    h, w = plane.shape
    return plane.reshape(h // size, size, w // size, size).transpose(0, 2, 1, 3)


def shifted(plane, dx, dy, pad):
    """plane moved so that out[y][x] = plane[y + dy][x + dx] (edges repeated)."""
    p = np.pad(plane, pad, mode="edge")
    h, w = plane.shape
    return p[pad + dy: pad + dy + h, pad + dx: pad + dx + w]


class Planes:
    def __init__(self, W, H, blank=True):
        self.f = {}
        if blank:
            self.f = {"y": np.zeros((H, W), np.int64), "co": np.full((H, W), 128, np.int64), "cg": np.full((H, W), 128, np.int64),
                      "a": np.zeros((H, W), np.int64), "nx": np.full((H // 2, W // 2), 128, np.int64),
                      "ny": np.full((H // 2, W // 2), 128, np.int64), "k": np.zeros((H // 2, W // 2), np.int64)}

    def copy(self):
        p = Planes(0, 0, blank=False)
        p.f = {k: v.copy() for k, v in self.f.items()}
        return p


def encode_frame(cur, ref, st):
    """cur: target planes (dict), ref: Planes (decoded previous). Returns the decoded frame
    (what the game will rebuild) and appends to the streams in st."""
    H, W = cur["y"].shape
    my, mx = H // MB, W // MB
    # --- motion search on luma and alpha (alpha counts double), color only where opaque
    opaque = cur["a"] > 0
    best = np.full((my, mx), np.iinfo(np.int64).max, np.int64)
    bdx = np.zeros((my, mx), np.int64)
    bdy = np.zeros((my, mx), np.int64)
    cands = sorted(((dx, dy) for dy in range(-R, R + 1) for dx in range(-R, R + 1)), key=lambda v: abs(v[0]) + abs(v[1]))
    for dx, dy in cands:
        sy = shifted(ref.f["y"], dx, dy, R)
        sa = shifted(ref.f["a"], dx, dy, R)
        cost = np.abs(sy - cur["y"]) * opaque + 2 * np.abs(sa - cur["a"])
        c = blocks_of(cost, MB).sum(axis=(2, 3)) + (abs(dx) + abs(dy)) * 4
        better = c < best
        best = np.where(better, c, best)
        bdx = np.where(better, dx, bdx)
        bdy = np.where(better, dy, bdy)
    # empty blocks (no alpha anywhere in the block now) carry no vector: they keep the
    # previous frame in place
    a_blk = blocks_of(cur["a"], MB)
    empty = a_blk.max(axis=(2, 3)) == 0
    bdx = np.where(empty, 0, bdx)
    bdy = np.where(empty, 0, bdy)
    # --- prediction per block, assembled into whole planes
    pred = {}
    full = ("y", "co", "cg", "a")
    half = ("nx", "ny", "k")
    for name in full + half:
        size = MB if name in full else MB // 2
        pad = R if name in full else R // 2 + 1
        src = ref.f[name]
        out = np.empty_like(src)
        ob = blocks_of(out, size)
        for dx in range(-R, R + 1):
            for dy in range(-R, R + 1):
                sel = (bdx == dx) & (bdy == dy)
                if not sel.any():
                    continue
                if name in full:
                    s = shifted(src, dx, dy, pad)
                else:
                    s = shifted(src, dx >> 1, dy >> 1, pad)
                ob[sel] = blocks_of(s, size)[sel]
        pred[name] = out
    # --- residuals
    rec = Planes(0, 0, blank=False)
    flags = np.zeros((my, mx), np.int64)
    coef_out = {}
    for name, Q in (("y", Q_Y), ("co", Q_C), ("cg", Q_C), ("nx", Q_N), ("ny", Q_N)):
        p = pred[name]
        c = cur[name]
        if name in ("nx", "ny"):
            tgt = np.where(cur["dmask"], c, p)
        else:
            tgt = np.where(opaque, c, p)
        res = (tgt - p).astype(np.float64)
        b8 = blocks_of(res, 8).reshape(-1, 8, 8)
        q = np.round(fdct(b8) / Q).astype(np.int64)
        deq = q * Q
        r = idct_int(deq)
        hh, ww = p.shape
        r_plane = r.reshape(hh // 8, ww // 8, 8, 8).transpose(0, 2, 1, 3).reshape(hh, ww)
        rec.f[name] = np.clip(p + r_plane, 0, 255)
        coef_out[name] = q.reshape(hh // 8, ww // 8, 64)
    for name in ("a", "k"):
        tgt = cur[name] if name == "a" else np.where(cur["dmask"], cur[name], pred[name])
        rec.f[name] = tgt.copy()
        coef_out[name] = (tgt - pred[name]) % 256
    # empty blocks keep the prediction for color (no residual) and clear alpha
    for name in rec.f:
        size = MB if name in full else MB // 2
        rb = blocks_of(rec.f[name], size)
        pb = blocks_of(pred[name], size)
        if name == "a":
            rb[empty] = 0
        else:
            rb[empty] = pb[empty]
    # --- serialize per block
    modes = bytearray()
    mvs = bytearray()
    coefs = bytearray()
    araw = bytearray()
    kraw = bytearray()
    zz = ZIGZAG
    for by in range(my):
        for bx in range(mx):
            if empty[by, bx]:
                modes.append(0)
                continue
            f = 0
            col = []
            for name in ("y", "co", "cg"):
                for sy in range(2):
                    for sx in range(2):
                        col.append(coef_out[name][by * 2 + sy, bx * 2 + sx])
            if any(v.any() for v in col):
                f |= 1
            ablk = coef_out["a"][by * MB:(by + 1) * MB, bx * MB:(bx + 1) * MB]
            if ablk.any():
                f |= 2
            nrm = [coef_out["nx"][by, bx], coef_out["ny"][by, bx]]
            if any(v.any() for v in nrm):
                f |= 4
            kblk = coef_out["k"][by * 8:(by + 1) * 8, bx * 8:(bx + 1) * 8]
            if kblk.any():
                f |= 8
            modes.append(1 + (f << 1))
            mvs += struct.pack("bb", int(bdx[by, bx]), int(bdy[by, bx]))
            for blk in (col if f & 1 else []) + (nrm if f & 4 else []):
                z = blk[zz]
                nz = np.nonzero(z)[0]
                n = int(nz[-1]) + 1 if len(nz) else 0
                coefs.append(n)
                coefs += z[:n].astype("<i2").tobytes()
            if f & 2:
                araw += ablk.astype(np.uint8).tobytes()
            if f & 8:
                kraw += kblk.astype(np.uint8).tobytes()
    st[0] += modes
    st[1] += mvs
    st[2] += coefs
    st[3] += araw
    st[4] += kraw
    return rec


def frame_planes(path_a, path_n):
    a = load(path_a)
    n = load(path_n)
    H, W = a.shape[:2]
    if n.shape[0] != H // 2:
        raise SystemExit("normal map must be half size: " + path_n)
    y, co, cg = ycocg(a[..., :3])
    dm = n[..., 3] > 0
    nv, dm2 = dilate(n[..., :3], dm, 2)
    return {"y": y, "co": co, "cg": cg, "a": a[..., 3], "nx": nv[..., 0], "ny": nv[..., 1], "k": nv[..., 2], "dmask": dm2}


def psnr(a, b, mask):
    if not mask.any():
        return 99.0
    d = ((a - b)[mask] ** 2).mean()
    return 99.0 if d == 0 else 10 * np.log10(255.0 * 255.0 / d)


def pack(cid):
    d = os.path.join(SRC, cid)
    clips = []
    for name in ORDER:
        meta = os.path.join(d, name + ".txt")
        if os.path.exists(meta):
            kv = dict(l.split(None, 1) for l in open(meta).read().strip().splitlines())
            clips.append((name, int(kv["frames"]), float(kv["secs"]), int(kv["loop"])))
    if not clips or clips[0][0] != "idle":
        print(cid, ": no idle clip, skipped")
        return
    first = frame_planes(os.path.join(d, "idle_00.png"), os.path.join(d, "idle_00_n.png"))
    H, W = first["y"].shape
    fps = 15
    meta = bytearray()
    blobs = bytearray()
    idle0 = None
    worst = 99.0
    for ci, (name, n, secs, loop) in enumerate(clips):
        st = [bytearray() for _ in range(5)]
        ref = Planes(W, H) if ci == 0 else idle0.copy()
        for k in range(n):
            cur = frame_planes(os.path.join(d, "%s_%02d.png" % (name, k)), os.path.join(d, "%s_%02d_n.png" % (name, k)))
            ref = encode_frame(cur, ref, st)
            if ci == 0 and k == 0:
                idle0 = ref.copy()
            m = cur["a"] > 8
            worst = min(worst, psnr(ref.f["y"], cur["y"], m))
        z = [zlib.compress(bytes(s), 9) for s in st]
        nm = name.encode()
        meta += struct.pack("<B", len(nm)) + nm + struct.pack("<HfBB", n, secs, loop, 0 if ci == 0 else 1)
        meta += struct.pack("<5I", *[len(x) for x in z])
        for x in z:
            blobs += x
        print("  %-8s %3d frames %7.1f KB  (%s)" % (name, n, sum(len(x) for x in z) / 1e3, " ".join("%.0f" % (len(x) / 1e3) for x in z)))
    head = b"PCA1" + struct.pack("<9H", W, H, fps, len(clips), *STILL, QUAL)
    os.makedirs(OUT, exist_ok=True)
    with open(os.path.join(OUT, cid + ".pca"), "wb") as f:
        f.write(head + bytes(meta) + bytes(blobs))
    print("%s: %.2f MB, worst luma PSNR %.1f dB" % (cid, (len(head) + len(meta) + len(blobs)) / 1e6, worst))


for cid in sorted(os.listdir(SRC)):
    if ONLY and cid not in ONLY:
        continue
    pack(cid)
