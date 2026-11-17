"""Capsule-mode germs: three original species, four animation frames each, face modelled in.

  red    "Spore"   a round germ ringed with club-tipped spikes, cross brows and a fanged grin
  blue   "Capsid"  a faceted twenty-sided head on stubby legs, half-lidded smug eyes
  yellow "Puff"    a gooey four-lobed cluster with wiggling tails, big shiny eyes, open mouth

Each color has its own silhouette so the three read apart without color.
Frames 0..3 loop (squash/stretch, spikes turn, legs step, lobes pulse, eyes glance).

Run (headless):
  flatpak run org.blender.Blender -b --factory-startup --python tools/blender/viruses.py -- assets/sprites [red|blue|yellow]
Writes virus_<color>_<frame>.tga (256x256 RGBA). The engine adds the ink outline and colorblind marks.
"""

import math
import os
import sys

import bpy
import bmesh

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import toon_lib as T  # noqa: E402

ARGS = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else []
OUT = os.path.abspath(ARGS[0] if ARGS else "assets/sprites")
ONLY = ARGS[1] if len(ARGS) > 1 else None
os.makedirs(OUT, exist_ok=True)
SIZE = 256
FRAMES = 4
INK = 0x1A1530


def tone(hexv, k):
    r, g, b = (hexv >> 16) & 255, (hexv >> 8) & 255, hexv & 255
    f = (lambda v: int(v + (255 - v) * k)) if k >= 0 else (lambda v: int(v * (1 + k)))
    return (f(r) << 16) | (f(g) << 8) | f(b)


def reset(ortho=2.0, cy=0.0):
    """Frame each species so it fills the cell the same amount (ortho = view width, cy = view centre)."""
    sc = T.eevee_scene(SIZE, SIZE, transparent=True, samples=96)
    T.world_backdrop(sc, 0x000000, 0x000000, ambient=0.25)
    cd = bpy.data.cameras.new("cam")
    cd.type = "ORTHO"
    cd.ortho_scale = ortho
    cam = bpy.data.objects.new("cam", cd)
    cam.location = (0, cy, 10)
    sc.collection.objects.link(cam)
    sc.camera = cam
    T.sun(sc, (-35, -28, 0), energy=4.0, color=0xFFF6E8, angle=2.0)
    return sc


def link(obj, *mats):
    bpy.context.scene.collection.objects.link(obj)
    for m in mats:
        obj.data.materials.append(m)
    return obj


def mesh_obj(name, build, smooth=True):
    me = bpy.data.meshes.new(name)
    bm = bmesh.new()
    build(bm)
    bm.to_mesh(me)
    bm.free()
    if smooth:
        for p in me.polygons:
            p.use_smooth = True
    return bpy.data.objects.new(name, me)


def sphere(name, r=1.0, seg=40, rings=20):
    return mesh_obj(name, lambda bm: bmesh.ops.create_uvsphere(bm, u_segments=seg, v_segments=rings, radius=r))


def cyl(name, r1, r2, depth, seg=24):
    return mesh_obj(name, lambda bm: bmesh.ops.create_cone(bm, cap_ends=True, segments=seg, radius1=r1, radius2=r2, depth=depth))


def ball(mat, loc, scale, rot=(0, 0, 0), name="b"):
    o = link(sphere(name), mat)
    o.location = loc
    o.scale = scale
    o.rotation_euler = rot
    return o


def stalk(mat, a, b, r1, r2, name="stalk"):
    """A tapered cylinder from point a to point b."""
    from mathutils import Vector

    a, b = Vector(a), Vector(b)
    d = b - a
    o = link(cyl(name, r1, r2, d.length), mat)
    o.location = (a + b) / 2
    o.rotation_euler = d.to_track_quat("Z", "Y").to_euler()
    return o


def tube(mat, pts, r, name="tube", taper=True):
    """A smooth beveled curve through pts (tails, legs)."""
    cu = bpy.data.curves.new(name, "CURVE")
    cu.dimensions = "3D"
    cu.bevel_depth = r
    cu.bevel_resolution = 6
    cu.use_fill_caps = True
    sp = cu.splines.new("NURBS")
    sp.points.add(len(pts) - 1)
    for p, c in zip(sp.points, pts):
        p.co = (c[0], c[1], c[2], 1.0)
    sp.use_endpoint_u = True
    sp.order_u = min(4, len(pts))
    if taper:
        for i, p in enumerate(sp.points):
            p.radius = 1.0 - 0.55 * i / max(1, len(pts) - 1)
    cu.materials.append(mat)
    o = bpy.data.objects.new(name, cu)
    bpy.context.scene.collection.objects.link(o)
    return o


def mats(color):
    body = T.toon("body", color, spec=0.55, spec_size=0.62, rim=0.35, threshold=0.3)
    light = T.toon("light", tone(color, 0.42), spec=0.6, spec_size=0.6, rim=0.25, threshold=0.28)
    dark = T.toon("dark", tone(color, -0.32), spec=0.25, rim=0.3, threshold=0.3)
    white = T.toon("white", 0xFFFFFF, shadow=(0.82, 0.84, 0.95), spec=0.0, rim=0.0, threshold=0.25)
    ink = T.toon("ink", INK, shadow=(1, 1, 1), spec=0.0, rim=0.0)
    mouth = T.toon("mouth", 0x5A1430, shadow=(0.8, 0.8, 0.9), spec=0.0, rim=0.0)
    tongue = T.toon("tongue", 0xFF6F8E, shadow=(0.8, 0.75, 0.85), spec=0.3, rim=0.0)
    glint = T.glow("glint", 0xFFFFFF, 1.0)
    return dict(body=body, light=light, dark=dark, white=white, ink=ink, mouth=mouth, tongue=tongue, glint=glint)


def eye(m, x, y, z, r, look=(0.0, 0.0), lid=0.0, lid_mat=None, tilt=0.0):
    """White eye with a dark pupil and a glint. lid in 0..1 covers the top of the eye."""
    ball(m["white"], (x, y, z), (r, r * 1.15, r * 0.45), name="sclera")
    pr = r * 0.6
    px, py = x + look[0] * r * 0.32, y + look[1] * r * 0.3 - r * 0.08
    ball(m["ink"], (px, py, z + r * 0.3), (pr, pr * 1.18, pr * 0.4), name="pupil")
    ball(m["glint"], (px - pr * 0.38, py + pr * 0.42, z + r * 0.48), (pr * 0.34, pr * 0.34, 0.01), name="glint")
    if lid > 0 and lid_mat is not None:
        # a lid: a flattened disc of body color lowered over the top of the eye
        lo = ball(lid_mat, (x, y + r * 1.15 * (1.0 - lid * 1.2), z + r * 0.55), (r * 1.12, r * 0.62, r * 0.25), rot=(0, 0, tilt), name="lid")
        lo.scale = (r * 1.15, r * 0.7, r * 0.3)
        ball(m["ink"], (x, y + r * 1.15 * (1.0 - lid * 1.2) - r * 0.62, z + r * 0.62), (r * 1.1, r * 0.08, r * 0.05), rot=(0, 0, tilt), name="lash")


def brow(m, x, y, z, w, ang):
    ball(m["ink"], (x, y, z), (w, w * 0.22, 0.03), rot=(0, 0, math.radians(ang)), name="brow")


def render(name):
    sc = bpy.context.scene
    path = os.path.join(OUT, name + ".tga")
    sc.render.filepath = path
    bpy.ops.render.render(write_still=True)
    print("rendered", path)


def on_sphere(r, x, y):
    return math.sqrt(max(0.0, r * r - x * x - y * y))


# ------------------------------------------------------------------ species


def spore(f):
    """Red: round germ, ring of club-tipped spikes, cross brows, fanged grin."""
    reset(2.12, -0.01)
    m = mats(0xE8475A)
    sq = [(1.0, 1.0), (1.06, 0.94), (1.0, 1.0), (0.95, 1.06)][f]
    turn = math.radians(9.0 * f)
    r = 0.6
    body = ball(m["body"], (0, -0.02, 0), (r * sq[0], r * sq[1], r))
    # spikes around the rim: stalk + club tip; alternate long/short
    n = 9
    for i in range(n):
        a = turn + i * 2 * math.pi / n + math.pi / 2
        ln = 0.84 if i % 2 == 0 else 0.8
        pulse = 0.04 * math.sin(f * math.pi / 2 + i)
        ca, sa = math.cos(a), math.sin(a)
        base = (ca * r * 0.8 * sq[0], sa * r * 0.8 * sq[1] - 0.02, 0.0)
        tip = (ca * (ln + pulse) * sq[0], sa * (ln + pulse) * sq[1] - 0.02, -0.05)
        stalk(m["dark"], base, tip, 0.085, 0.07)
        ball(m["light"], tip, (0.15, 0.15, 0.12), rot=(0, 0, a), name="club")
    # a few studs on the face side
    for (sx, sy, s) in ((-0.38, 0.34, 0.07), (0.4, 0.3, 0.06), (0.36, -0.38, 0.065), (-0.4, -0.3, 0.055)):
        ball(m["light"], (sx * sq[0], sy * sq[1], on_sphere(r, sx, sy) - 0.01), (s, s, s * 0.6), name="stud")
    zf = r * 0.88
    look = [(0, 0), (0.6, 0), (0, 0), (-0.6, 0.1)][f]
    eye(m, -0.18, 0.07, zf, 0.13, look)
    eye(m, 0.18, 0.07, zf, 0.13, look)
    brow(m, -0.18, 0.28, zf + 0.08, 0.16, -24)
    brow(m, 0.18, 0.28, zf + 0.08, 0.16, 24)
    # grin: a dark crescent with two fangs
    gw = [0.24, 0.27, 0.24, 0.22][f]
    ball(m["mouth"], (0, -0.21, zf - 0.02), (gw, 0.11, 0.06), name="grin")
    ball(m["body"], (0, -0.15, zf + 0.0), (gw * 1.05, 0.07, 0.07), name="lip")
    for sx in (-0.1, 0.1):
        fang = link(cyl("fang", 0.045, 0.0, 0.1, 12), m["white"])
        fang.location = (sx, -0.23, zf + 0.04)
        fang.rotation_euler = (math.radians(-90), 0, 0)
        fang.scale = (1.0, 0.5, 1.0)
    render("virus_red_%d" % f)


def capsid(f):
    """Blue: faceted 20-sided head on a collar and stubby legs, half-lidded smug eyes."""
    reset(2.0, -0.09)
    c = 0x4C7BF2
    m = mats(c)
    m2 = T.toon("facet2", tone(c, 0.18), spec=0.6, spec_size=0.65, rim=0.35, threshold=0.28)
    bob = [0.0, -0.03, 0.0, 0.02][f]
    hy = 0.26 + bob
    hr = 0.6
    head = link(mesh_obj("head", lambda bm: bmesh.ops.create_icosphere(bm, subdivisions=1, radius=hr), smooth=False), m["body"], m2)
    for i, p in enumerate(head.data.polygons):
        p.material_index = (i * 7) % 3 == 0
    head.location = (0, hy, 0)
    head.rotation_euler = (math.radians(12), math.radians(-8 + 4 * math.sin(f * math.pi / 2)), math.radians(4))
    # collar + neck
    col = link(cyl("collar", 0.24, 0.2, 0.12, 24), m["light"])
    col.location = (0, hy - hr * 0.86, 0)
    col.rotation_euler = (math.radians(90), 0, 0)
    neck = link(cyl("neck", 0.13, 0.13, 0.2, 20), m["dark"])
    neck.location = (0, hy - hr - 0.12, 0)
    neck.rotation_euler = (math.radians(90), 0, 0)
    # base plate
    plate = ball(m["dark"], (0, hy - hr - 0.24, -0.02), (0.3, 0.07, 0.22), name="plate")
    # stubby legs: three per side, alternate pairs lift
    py = hy - hr - 0.26
    for side in (-1, 1):
        for j, spread in enumerate((0.2, 0.5)):
            lift = 0.06 if (j + (1 if side > 0 else 0) + f) % 2 == 0 else 0.0
            x0 = side * 0.12 * (j + 1) * 0.8
            knee = (side * (0.22 + spread * 0.5), py - 0.04 + lift + 0.1, 0.02 - j * 0.06)
            foot = (side * (0.24 + spread * 0.62), py - 0.32 + lift, 0.0 - j * 0.06)
            tube(m["dark"], [(x0, py, -j * 0.04), knee, foot], 0.065, taper=False)
            ball(m["light"], foot, (0.095, 0.075, 0.07), name="toe")
    # face on the front facet
    zf = hr * 0.86
    look = [(0.5, 0), (0.5, 0), (-0.5, 0), (-0.5, 0)][f]
    lid = [0.45, 0.45, 0.5, 0.95 if f == 3 else 0.45][f]
    eye(m, -0.19, hy + 0.05, zf, 0.135, look, lid=lid, lid_mat=m["body"], tilt=math.radians(-8))
    eye(m, 0.19, hy + 0.05, zf, 0.135, look, lid=lid, lid_mat=m["body"], tilt=math.radians(8))
    # smirk: a short tilted ink stroke, deeper at one corner
    ball(m["ink"], (0.03, hy - 0.2, zf + 0.02), (0.13, 0.03, 0.03), rot=(0, 0, math.radians(10)), name="smirk")
    ball(m["ink"], (0.15, hy - 0.175, zf + 0.02), (0.04, 0.04, 0.03), name="dimple")
    render("virus_blue_%d" % f)


def puff(f):
    """Yellow: gooey four-lobed cluster (metaballs), wiggling tails, big shiny eyes, round open mouth."""
    reset(1.74, -0.08)
    c = 0xF5D547
    m = mats(c)
    mb = bpy.data.metaballs.new("puff")
    mb.resolution = 0.02
    mb.render_resolution = 0.015
    mb.threshold = 0.6
    ob = bpy.data.objects.new("puff", mb)
    bpy.context.scene.collection.objects.link(ob)
    mb.materials.append(m["body"])
    core = mb.elements.new()
    core.co = (0, -0.02, 0)
    core.radius = 0.78
    for i, (lx, ly) in enumerate(((-0.36, 0.32), (0.37, 0.3), (-0.4, -0.3), (0.38, -0.32), (0.0, 0.42))):
        e = mb.elements.new()
        k = 1.0 + 0.07 * math.sin(f * math.pi / 2 + i * math.pi / 2)
        e.co = (lx * k, ly * k, -0.08)
        e.radius = 0.4 * k
    # little bumps on the lobes
    # tails: three wavy tubes off the bottom, phase-shifted per frame
    for t, (ox, ang) in enumerate(((-0.25, -110), (0.05, -90), (0.3, -70))):
        pts = []
        for s in range(5):
            u = s / 4.0
            a = math.radians(ang)
            w = 0.07 * math.sin(u * 5.0 + f * math.pi / 2 + t)
            d = 0.42 + u * 0.42
            pts.append((ox + math.cos(a) * d * 0.5 + w * math.sin(a), -0.38 + math.sin(a) * (d - 0.42) - 0.05, -0.12))
        tube(m["dark"], pts, 0.06)
    zf = 0.5
    look = [(0, 0.3), (0.3, 0.2), (0, 0.3), (-0.3, 0.2)][f]
    eye(m, -0.19, 0.1, zf, 0.16, look)
    eye(m, 0.19, 0.1, zf, 0.16, look)
    # rosy cheeks
    ball(T.toon("blush", 0xFF8FA6, spec=0, rim=0), (-0.33, -0.08, zf - 0.08), (0.08, 0.05, 0.03), name="cheek")
    ball(T.toon("blush2", 0xFF8FA6, spec=0, rim=0), (0.33, -0.08, zf - 0.08), (0.08, 0.05, 0.03), name="cheek")
    # round open mouth with a tongue
    mo = [1.0, 0.85, 1.0, 1.12][f]
    ball(m["mouth"], (0, -0.2, zf), (0.1 * mo, 0.1 * mo, 0.04), name="mouth")
    ball(m["tongue"], (0, -0.24 - 0.01 * mo, zf + 0.02), (0.065 * mo, 0.045 * mo, 0.03), name="tongue")
    render("virus_yellow_%d" % f)


SPECIES = {"red": spore, "blue": capsid, "yellow": puff}
for name, fn in SPECIES.items():
    if ONLY and name != ONLY:
        continue
    for f in range(FRAMES):
        fn(f)
