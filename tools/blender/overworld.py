"""Puzzle Commons story overworld: five original, toon-shaded world maps (EEVEE).

Every world has its own layout, silhouette and camera:
  0 Reef        an archipelago of islets joined by plank bridges, more islands past the frame
  1 Valley      inland hills, a winding river with a bridge, snowy mountains behind
  2 Moon        inside a vast crater; the path spirals from the rim down to a domed clinic
  3 Bazaar      endless dunes, an oasis pond, a walled town, a sliver of coast in one corner
  4 Cinder      basalt plateaus on pillars over a lava sea, rope bridges, lavafalls, a volcano
The screen position of every node is written beside the image for the engine.

  blender -b --factory-startup --python tools/blender/overworld.py -- <out dir> [world index]
"""
import bmesh
import bpy
import math
import os
import random
import sys
from mathutils import Vector, noise
from bpy_extras.object_utils import world_to_camera_view

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import toon_lib as T  # noqa: E402

PH = os.path.normpath(os.path.join(HERE, "..", "..", "third_party", "polyhaven"))
args = sys.argv[sys.argv.index("--") + 1:] if "--" in sys.argv else ["assets/maps"]
OUT = os.path.abspath(args[0])
ONLY = int(args[1]) if len(args) > 1 else None
os.makedirs(OUT, exist_ok=True)
W, H = 1280, 720
WL = 0.18  # water level
PATH_H = 0.62


# ------------------------------------------------------------------ mesh helpers
def obj_from(name, build, material=None, loc=(0, 0, 0), smooth=True):
    me = bpy.data.meshes.new(name)
    bm = bmesh.new()
    build(bm)
    bm.to_mesh(me)
    bm.free()
    for p in me.polygons:
        p.use_smooth = smooth
    ob = bpy.data.objects.new(name, me)
    ob.location = loc
    bpy.context.scene.collection.objects.link(ob)
    if material:
        me.materials.append(material)
    return ob


def sphere(name, r, m, loc, scale=(1, 1, 1), seg=24):
    ob = obj_from(name, lambda bm: bmesh.ops.create_uvsphere(bm, u_segments=seg, v_segments=max(8, seg // 2), radius=r), m, loc)
    ob.scale = scale
    return ob


def cyl(name, r1, r2, depth, m, loc, seg=24, smooth=True, rot=(0, 0, 0)):
    def b(bm):
        bmesh.ops.create_cone(bm, cap_ends=True, segments=seg, radius1=r1, radius2=r2, depth=depth)
        bmesh.ops.translate(bm, vec=(0, 0, depth / 2), verts=bm.verts)

    ob = obj_from(name, b, m, loc, smooth)
    ob.rotation_euler = rot
    return ob


def box(name, sx, sy, sz, m, loc, rot=(0, 0, 0)):
    def b(bm):
        bmesh.ops.create_cube(bm, size=1.0)
        bmesh.ops.scale(bm, vec=(sx, sy, sz), verts=bm.verts)
        bmesh.ops.translate(bm, vec=(0, 0, sz / 2), verts=bm.verts)

    ob = obj_from(name, b, m, loc, smooth=False)
    ob.rotation_euler = rot
    return ob


def torus(name, R, r, m, loc):
    def b(bm):
        segs, rings = 40, 8
        verts = []
        for i in range(segs):
            a = i * 2 * math.pi / segs
            ring = []
            for j in range(rings):
                c = j * 2 * math.pi / rings
                ring.append(bm.verts.new(((R + r * math.cos(c)) * math.cos(a), (R + r * math.cos(c)) * math.sin(a), r * math.sin(c))))
            verts.append(ring)
        for i in range(segs):
            for j in range(rings):
                bm.faces.new((verts[i][j], verts[(i + 1) % segs][j], verts[(i + 1) % segs][(j + 1) % rings], verts[i][(j + 1) % rings]))

    return obj_from(name, b, m, loc)


def smoothstep(e0, e1, x):
    t = max(0.0, min(1.0, (x - e0) / (e1 - e0)))
    return t * t * (3 - 2 * t)


def seg_dist(p, a, b):
    ab = b - a
    t = max(0.0, min(1.0, (p - a).dot(ab) / max(ab.length_squared, 1e-9)))
    return (a + ab * t - p).length


def poly_dist(p, pts):
    return min(seg_dist(p, pts[i], pts[i + 1]) for i in range(len(pts) - 1))


# ------------------------------------------------------------------ CC0 rock shapes, toon painted
_LIB = {}


def rock_collection(name, mat):
    key = (name, mat.name)
    if key in _LIB:
        return _LIB[key]
    path = os.path.join(PH, name, name + ".gltf")
    if not os.path.exists(path):
        _LIB[key] = (None, 1.0)
        return _LIB[key]
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=path)
    new = [o for o in bpy.data.objects if o not in before]
    col = bpy.data.collections.new("lib_" + name)
    lo, hi = Vector((1e9,) * 3), Vector((-1e9,) * 3)
    bpy.context.view_layer.update()
    for o in new:
        for c in list(o.users_collection):
            c.objects.unlink(o)
        col.objects.link(o)
        if o.type == "MESH":
            o.data.materials.clear()
            o.data.materials.append(mat)
            for v in o.bound_box:
                w = o.matrix_world @ Vector(v)
                lo = Vector((min(lo.x, w.x), min(lo.y, w.y), min(lo.z, w.z)))
                hi = Vector((max(hi.x, w.x), max(hi.y, w.y), max(hi.z, w.z)))
    d = hi - lo
    _LIB[key] = (col, max(d.x, d.y, d.z, 1e-3))
    return _LIB[key]


def place_rock(name, mat, x, y, z, size, rot):
    col, s = rock_collection(name, mat)
    if col is None:
        return sphere("rock", size / 2, mat, (x, y, z), (1, 0.8, 0.6), seg=10)
    e = bpy.data.objects.new("inst_" + name, None)
    e.instance_type = "COLLECTION"
    e.instance_collection = col
    k = size / s
    e.scale = (k, k, k)
    e.location = (x, y, z - size * 0.15)
    e.rotation_euler = (0, 0, rot)
    bpy.context.scene.collection.objects.link(e)
    return e


# ------------------------------------------------------------------ stylized props
def puffy_tree(x, y, z, r, leaf, trunk, scale=1.0):
    h = (0.35 + r.random() * 0.2) * scale
    cyl("trunk", 0.06 * scale, 0.045 * scale, h, trunk, (x, y, z))
    s = (0.24 + r.random() * 0.1) * scale
    for k in range(4):
        a = k * 2.1 + r.random()
        o = 0.55 * s if k > 0 else 0.0
        sphere("leaf", s * (1.0 if k == 0 else 0.75), leaf, (x + math.cos(a) * o, y + math.sin(a) * o, z + h + s * (0.55 if k == 0 else 0.25)), (1, 1, 0.85), seg=16)


def pine(x, y, z, r, leaf, trunk):
    cyl("ptrunk", 0.05, 0.04, 0.2, trunk, (x, y, z), seg=8)
    for k in range(3):
        cyl("pine", 0.3 - k * 0.08, 0.0, 0.35, leaf, (x, y, z + 0.15 + k * 0.18), seg=10)


def palm(x, y, z, r, leaf, trunk):
    lean = r.uniform(-0.25, 0.25)
    px, py, pz = x, y, z
    for k in range(6):
        cyl("seg", 0.055 - k * 0.004, 0.05 - k * 0.004, 0.16, trunk, (px, py, pz), seg=10, rot=(0, lean * 0.6, 0))
        px += math.sin(lean * 0.6) * 0.16
        pz += 0.155
    for k in range(7):
        a = k * 2 * math.pi / 7 + r.random() * 0.4
        f = sphere("frond", 0.2, leaf, (px + math.cos(a) * 0.2, py + math.sin(a) * 0.2, pz - 0.02), (1.7, 0.42, 0.12), seg=12)
        f.rotation_euler = (math.radians(18) * math.sin(a), -math.radians(18) * math.cos(a), a)
    sphere("coco", 0.05, trunk, (px, py, pz - 0.05), seg=8)


def bush(x, y, z, r, leaf):
    for k in range(3):
        a = k * 2.2 + r.random()
        sphere("bush", 0.11 + r.random() * 0.04, leaf, (x + math.cos(a) * 0.08, y + math.sin(a) * 0.08, z + 0.07), (1, 1, 0.8), seg=12)


def cloud(x, y, z, r, m, size=1.0):
    n = 5 + r.randrange(4)
    for k in range(n):
        dx = ((k - n / 2) * 0.32 + r.uniform(-0.1, 0.1)) * size
        rr = (0.3 + r.random() * 0.25 - abs(k - n / 2) * 0.04) * size
        sphere("cloud", rr, m, (x + dx, y + r.uniform(-0.15, 0.15) * size, z + r.uniform(-0.05, 0.12) * size), (1, 0.8, 0.75), seg=16)


def lighthouse(x, y, z):
    """Original lighthouse: plinth, tapered octagonal tower with red bands, door and windows,
    a gallery with posts and rail, a glowing lantern room, a red dome and a finial."""
    stone = T.toon("lh_stone", 0x9AA6B8)
    white = T.toon("lh_white", 0xF6F2EA, spec=0.35)
    red = T.toon("lh_red", 0xD9414E, spec=0.5)
    dark = T.toon("lh_dark", 0x2E3550, spec=0.0, rim=0.0)
    cyl("plinth", 0.42, 0.38, 0.14, stone, (x, y, z), seg=8, smooth=False)
    Ht = 1.45
    cyl("tower", 0.3, 0.2, Ht, white, (x, y, z + 0.14), seg=8, smooth=False)
    for k, (h0, h1) in enumerate(((0.38, 0.62), (0.92, 1.12))):
        f0 = 0.3 - 0.1 * h0 / Ht
        f1 = 0.3 - 0.1 * h1 / Ht
        cyl("band%d" % k, f0 + 0.008, f1 + 0.008, h1 - h0, red, (x, y, z + 0.14 + h0), seg=8, smooth=False)
    box("door", 0.12, 0.05, 0.2, dark, (x, y - 0.29, z + 0.14))
    for k, hh in enumerate((0.75, 1.25)):
        box("win%d" % k, 0.07, 0.05, 0.1, dark, (x, y - (0.3 - 0.1 * hh / Ht) + 0.01, z + 0.14 + hh))
    top = z + 0.14 + Ht
    cyl("gallery", 0.3, 0.3, 0.05, red, (x, y, top), seg=16)
    for k in range(14):
        a = k * 2 * math.pi / 14
        cyl("post", 0.01, 0.01, 0.13, dark, (x + math.cos(a) * 0.27, y + math.sin(a) * 0.27, top + 0.05), seg=6)
    torus("rail", 0.27, 0.012, dark, (x, y, top + 0.18))
    cyl("lantern_frame", 0.17, 0.17, 0.05, dark, (x, y, top + 0.05), seg=16)
    cyl("lantern", 0.15, 0.15, 0.22, T.glow("lamp", 0xFFE7A0, 2.5), (x, y, top + 0.1), seg=16)
    cyl("dome", 0.2, 0.02, 0.2, red, (x, y, top + 0.32), seg=16)
    sphere("finial", 0.035, dark, (x, y, top + 0.54), seg=10)


def sea_material(th):
    """Organic caustic-like web: two warped Voronoi edge layers over softly shaded cells."""

    def sea_color(nt):
        tc = nt.nodes.new("ShaderNodeTexCoord")
        nz = nt.nodes.new("ShaderNodeTexNoise")
        nz.inputs["Scale"].default_value = 0.7
        nz.inputs["Detail"].default_value = 3.0
        warp = nt.nodes.new("ShaderNodeVectorMath")
        warp.operation = "MULTIPLY_ADD"
        warp.inputs[1].default_value = (1.35, 1.35, 1.35)
        nt.links.new(nz.outputs["Color"], warp.inputs[0])
        nt.links.new(tc.outputs["Object"], warp.inputs[2])

        def edges(scale, width, strength, offset):
            off = nt.nodes.new("ShaderNodeVectorMath")
            off.operation = "ADD"
            off.inputs[1].default_value = (offset, offset * 0.7, 0)
            nt.links.new(warp.outputs[0], off.inputs[0])
            v = nt.nodes.new("ShaderNodeTexVoronoi")
            v.feature = "DISTANCE_TO_EDGE"
            v.inputs["Scale"].default_value = scale
            nt.links.new(off.outputs[0], v.inputs["Vector"])
            r = T._ramp(nt, [(0.0, (strength,) * 3 + (1,)), (width, (0, 0, 0, 1))])
            nt.links.new(v.outputs["Distance"], r.inputs["Fac"])
            return r.outputs["Color"]

        big = edges(0.9, 0.016, 0.45, 0.0)
        small = edges(2.0, 0.012, 0.16, 7.0)
        lines = nt.nodes.new("ShaderNodeMix")
        lines.data_type = "RGBA"
        lines.blend_type = "LIGHTEN"
        lines.inputs["Factor"].default_value = 1.0
        nt.links.new(big, lines.inputs[6])
        nt.links.new(small, lines.inputs[7])
        # soft lighter cell centres
        f1 = nt.nodes.new("ShaderNodeTexVoronoi")
        f1.inputs["Scale"].default_value = 1.25
        nt.links.new(warp.outputs[0], f1.inputs["Vector"])
        sea_c = nt.nodes.new("ShaderNodeRGB")
        sea_c.outputs[0].default_value = T.lin(th["sea"])
        lighter = nt.nodes.new("ShaderNodeRGB")
        lighter.outputs[0].default_value = T.lin(th["sea_light"])
        cells = T._mix(nt, "MIX", f1.outputs["Distance"], lighter.outputs[0], sea_c.outputs[0])
        bw = nt.nodes.new("ShaderNodeRGBToBW")
        nt.links.new(lines.outputs[2], bw.inputs[0])
        return T._mix(nt, "MIX", bw.outputs[0], cells, (1, 1, 1, 1))

    return T.toon("sea", 0, base_socket_fn=sea_color, spec=0.35, spec_size=0.82, rim=0.0, alpha=0.66, threshold=0.2)



# ------------------------------------------------------------------ CC0 model library (toon converted)
OGA = os.path.normpath(os.path.join(HERE, "..", "..", "third_party", "oga"))
MK = os.path.join(OGA, "nature_megakit", "glTF")          # Quaternius Stylized Nature MegaKit (CC0)
KN = os.path.join(OGA, "kenney_nature", "Models", "GLTF format")  # Kenney Nature Kit (CC0)
VL = os.path.join(OGA, "village_megakit", "glTF")         # Quaternius Medieval Village MegaKit (CC0)
_MODELS = {}


def load_model(path, tint=None, key=None):
    """Import a glTF/GLB once into an unlinked collection, converted to the toon look.
    Returns (collection, max dimension, height) or (None, 1, 1) when missing."""
    key = key or (path, tint)
    if key in _MODELS:
        return _MODELS[key]
    if not os.path.exists(path):
        print("missing model", path)
        _MODELS[key] = (None, 1.0, 1.0)
        return _MODELS[key]
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=path)
    new = [o for o in bpy.data.objects if o not in before]
    col = bpy.data.collections.new("m_" + os.path.basename(path))
    lo, hi = Vector((1e9,) * 3), Vector((-1e9,) * 3)
    bpy.context.view_layer.update()
    converted = {}
    for o in new:
        for c in list(o.users_collection):
            c.objects.unlink(o)
        col.objects.link(o)
        if o.type == "MESH":
            for i, m in enumerate(o.data.materials):
                if m not in converted:
                    converted[m] = T.toonify(m, tint=tint)
                o.data.materials[i] = converted[m]
            for v in o.bound_box:
                w = o.matrix_world @ Vector(v)
                lo = Vector((min(lo.x, w.x), min(lo.y, w.y), min(lo.z, w.z)))
                hi = Vector((max(hi.x, w.x), max(hi.y, w.y), max(hi.z, w.z)))
    d = hi - lo
    _MODELS[key] = (col, max(d.x, d.y, d.z, 1e-3), max(d.z, 1e-3))
    return _MODELS[key]


def put(path, x, y, z, size, rot=0.0, by_height=True, tint=None):
    col, mx, ht = load_model(path, tint)
    if col is None:
        return None
    e = bpy.data.objects.new("i_" + os.path.basename(path), None)
    e.instance_type = "COLLECTION"
    e.instance_collection = col
    k = size / (ht if by_height else mx)
    e.scale = (k, k, k)
    e.location = (x, y, z)
    e.rotation_euler = (0, 0, rot)
    bpy.context.scene.collection.objects.link(e)
    return e


def mk(name):
    return os.path.join(MK, name + ".gltf")


def kn(name):
    return os.path.join(KN, name + ".glb")


def vl(name):
    return os.path.join(VL, name + ".gltf")


_HOUSES = {}


def house_collection(variant, tint=None):
    """Assemble a small house from Medieval Village MegaKit pieces (2 m wall modules, 4x4 roof)."""
    key = (variant, tint)
    if key in _HOUSES:
        return _HOUSES[key]
    col = bpy.data.collections.new("house_%d" % variant)
    walls = [("Wall_Plaster_Door_Round", "Wall_Plaster_Window_Wide_Round"),
             ("Wall_Plaster_Window_Wide_Round", "Wall_Plaster_Straight"),
             ("Wall_UnevenBrick_Window_Wide_Round", "Wall_Plaster_Window_Thin_Round"),
             ("Wall_Plaster_WoodGrid", "Wall_Plaster_Window_Wide_Round")]
    sides = [((0, -2), 0.0), ((2, 0), math.pi / 2), ((0, 2), math.pi), ((-2, 0), -math.pi / 2)]
    pieces = []
    for si, ((cx, cy), ang) in enumerate(sides):
        for k, along in enumerate((-1.0, 1.0)):
            name = walls[(variant + si) % 4][0 if (si == 0 and k == 0) else 1]
            if si == 0 and k == 0:
                name = "Wall_Plaster_Door_Round"
            ox = cx + math.cos(ang) * along
            oy = cy + math.sin(ang) * along
            pieces.append((vl(name), (ox, oy, 0.0), ang))
    for cx, cy in ((-2, -2), (2, -2), (2, 2), (-2, 2)):
        pieces.append((vl("Corner_Exterior_Wood"), (cx, cy, 0.0), 0.0))
    pieces.append((vl(["Roof_RoundTiles_4x4", "Roof_RoundTiles_4x4", "Roof_Tower_RoundTiles", "Roof_RoundTiles_4x4"][variant % 4]), (0, 0, 3.0), 0.0))
    for path, loc, ang in pieces:
        c, _, _ = load_model(path, tint)
        if c is None:
            continue
        e = bpy.data.objects.new("hp", None)
        e.instance_type = "COLLECTION"
        e.instance_collection = c
        e.location = loc
        e.rotation_euler = (0, 0, ang)
        col.objects.link(e)
    _HOUSES[key] = col
    return col


def put_house(variant, x, y, z, size, rot, tint=None):
    col = house_collection(variant, tint)
    e = bpy.data.objects.new("house", None)
    e.instance_type = "COLLECTION"
    e.instance_collection = col
    k = size / 4.4
    e.scale = (k, k, k)
    e.location = (x, y, z - 0.02)
    e.rotation_euler = (0, 0, rot)
    bpy.context.scene.collection.objects.link(e)
    return e


def node_pad(i, x, y, z, stone, stone2, medal):
    """Two-tier octagonal stone pad with a recessed medallion and studs."""
    cyl("padb%d" % i, 0.5, 0.47, 0.09, stone2, (x, y, z - 0.02), seg=8, smooth=False)
    cyl("padt%d" % i, 0.41, 0.39, 0.1, stone, (x, y, z + 0.07), seg=8, smooth=False)
    cyl("medal%d" % i, 0.25, 0.25, 0.03, medal, (x, y, z + 0.165), seg=32)
    for k in range(8):
        a = k * math.pi / 4 + math.pi / 8
        sphere("stud", 0.035, stone2, (x + math.cos(a) * 0.45, y + math.sin(a) * 0.45, z + 0.08), seg=8)


def sailboat(x, y, z, r, hullm, sailm, stripe):
    rot = r.uniform(0, 6.28)
    hull = sphere("hull", 0.3, hullm, (x, y, z + 0.02), (1.0, 0.36, 0.22), seg=16)
    hull.rotation_euler = (0, 0, rot)
    cyl("mast", 0.015, 0.012, 0.62, hullm, (x, y, z + 0.05), seg=6)

    def sail(bm):
        a = bm.verts.new((0.02, 0, 0.12))
        b = bm.verts.new((0.02, 0, 0.62))
        c = bm.verts.new((0.3, 0, 0.14))
        bm.faces.new((a, b, c))

    sl = obj_from("sail", sail, sailm, (x, y, z + 0.0), smooth=False)
    sl.rotation_euler = (0, 0, rot)
    st = box("stripe", 0.28, 0.005, 0.04, stripe, (x, y, z + 0.2))
    st.rotation_euler = (0, 0, rot)


def gull(x, y, z, m):
    for side in (-1, 1):
        w = box("wing", 0.14, 0.03, 0.01, m, (x + side * 0.06, y, z))
        w.rotation_euler = (0, side * math.radians(-22), 0)


def windmill(x, y, z, white, red, dark, wood):
    cyl("wm_tower", 0.26, 0.18, 1.0, white, (x, y, z), seg=8, smooth=False)
    cyl("wm_roof", 0.24, 0.0, 0.35, red, (x, y, z + 1.0), seg=8, smooth=False)
    box("wm_door", 0.12, 0.04, 0.2, dark, (x, y - 0.25, z))
    hub = Vector((x, y - 0.26, z + 0.95))
    sphere("wm_hub", 0.05, dark, hub, seg=10)
    for k in range(4):
        a = k * math.pi / 2 + 0.35
        b = box("blade", 0.1, 0.01, 0.62, wood, (hub.x, hub.y - 0.02, hub.z))
        b.rotation_euler = (0, a, 0)


def volcano(x, y, z, rock, glow_m, flow_m):
    """A ridged volcano: a displaced, subdivided cone with a glowing crater and lava streams."""
    def build(bm):
        rings, segs, R, Hh = 14, 48, 1.7, 2.1
        rows = []
        for j in range(rings + 1):
            t = j / rings
            row = []
            for i in range(segs):
                a = i * 2 * math.pi / segs
                rr = R * (1 - t) + 0.42 * t
                ridge = 0.12 * math.sin(a * 7 + t * 3) * (1 - t) + 0.08 * noise.noise(Vector((math.cos(a) * 2, math.sin(a) * 2, t * 3)))
                rr += ridge
                zz = Hh * (t ** 0.85)
                row.append(bm.verts.new((math.cos(a) * rr, math.sin(a) * rr, zz)))
            rows.append(row)
        for j in range(rings):
            for i in range(segs):
                bm.faces.new((rows[j][i], rows[j][(i + 1) % segs], rows[j + 1][(i + 1) % segs], rows[j + 1][i]))

    obj_from("volcano", build, rock, (x, y, z), smooth=True)
    cyl("crater", 0.4, 0.3, 0.04, glow_m, (x, y, z + 2.08), seg=24)
    for k in range(3):
        a = -math.pi / 2 + (k - 1) * 0.55
        for j in range(16):
            t = j / 16
            rr = 0.45 + 1.15 * t
            px = x + math.cos(a + 0.12 * math.sin(t * 6)) * rr
            py = y + math.sin(a + 0.12 * math.sin(t * 6)) * rr
            pz = z + 2.1 * ((1 - t) ** 0.85) + 0.02
            sphere("flow", 0.075 * (1 - 0.4 * t), flow_m, (px, py, pz), seg=8)


def auto_frame(sc, cam, pts, xr=(0.04, 0.96), yr=(0.30, 0.95)):
    """Move the camera until every must-see point is inside the frame (bottom kept for UI)."""
    fov = cam.data.angle
    for _ in range(80):
        bpy.context.view_layer.update()
        vs = [world_to_camera_view(sc, cam, p) for p in pts]
        minx, maxx = min(v.x for v in vs), max(v.x for v in vs)
        miny, maxy = min(v.y for v in vs), max(v.y for v in vs)
        q = cam.matrix_world.to_quaternion()
        fwd, right, up = q @ Vector((0, 0, -1)), q @ Vector((1, 0, 0)), q @ Vector((0, 1, 0))
        d = sum(v.z for v in vs) / len(vs)
        span = 2 * d * math.tan(fov / 2)
        cx = (minx + maxx) / 2 - (xr[0] + xr[1]) / 2
        cy = (miny + maxy) / 2 - (yr[0] + yr[1]) / 2
        fits = minx >= xr[0] and maxx <= xr[1] and miny >= yr[0] and maxy <= yr[1]
        cam.location += right * cx * span * 0.6 + up * cy * span * 0.6 * (9 / 16)
        if not fits:
            cam.location -= fwd * 0.3
        elif (maxx - minx) < 0.85 * (xr[1] - xr[0]) and (maxy - miny) < 0.85 * (yr[1] - yr[0]):
            cam.location += fwd * 0.15
        elif abs(cx) < 0.01 and abs(cy) < 0.01:
            break


# ------------------------------------------------------------------ layouts
def islet(p, cx, cy, rx, ry, off):
    wob = noise.noise(Vector((p.x * 0.6, p.y * 0.6, cx)) + off) * 0.14
    d = math.sqrt(((p.x - cx) / rx) ** 2 + ((p.y - cy) / ry) ** 2) + wob
    return smoothstep(1.05, 0.62, d)


def make_layout(wi, off):
    """Returns (nodes, height(x, y) -> float | None, extras dict)."""
    if wi == 0:  # archipelago
        nodes = [(-5.0, -1.7), (-3.3, -0.1), (-0.9, 1.1), (0.9, -0.5), (3.2, 0.7), (4.9, -0.9)]
        isles = [(-4.1, -0.9, 1.9, 1.5), (0.0, 0.3, 1.8, 1.45), (4.1, -0.1, 1.85, 1.5),
                 (-8.6, 4.5, 2.4, 1.6), (8.5, 3.6, 2.2, 1.4), (7.4, -4.6, 1.6, 1.1), (-2.2, 6.6, 2.6, 1.3), (-8.4, -4.4, 1.3, 0.9)]

        def h(x, y):
            p = Vector((x, y, 0))
            m = max(islet(p, *i, off) for i in isles)
            n = noise.noise(Vector((x * 0.6, y * 0.6, 0)) + off)
            return -0.5 + m * (1.05 + 0.25 * max(0.0, n + 0.1))

        return nodes, h, dict(sea=True, grid=(30, 22, 0, 2.5), cam=((0, -10.0, 8.2), (0, 0.4, 0), 34))
    if wi == 1:  # valley
        nodes = [(-4.9, -2.3), (-2.9, -0.9), (-0.5, -1.9), (1.3, -0.4), (2.5, 1.4), (4.3, 2.4)]
        river = [Vector(v) for v in ((-7.5, 4.0, 0), (-3.6, 1.6, 0), (-1.85, 0.2, 0), (-1.65, -1.5, 0), (-2.3, -3.6, 0), (-3.0, -7.0, 0))]

        def h(x, y):
            p = Vector((x, y, 0))
            n = noise.noise(Vector((x * 0.32, y * 0.32, 0)) + off)
            base = 0.62 + 0.45 * max(0.0, n) + 0.12 * noise.noise(Vector((x * 1.1, y * 1.1, 3)) + off)
            if y > 2.8:
                ridge = abs(noise.noise(Vector((x * 0.28, 9.0, 0)) + off))
                base += (y - 2.8) ** 1.35 * (0.7 + 1.1 * ridge)
            hill = math.exp(-((x - 4.3) ** 2 + (y - 2.4) ** 2) / 2.2) * 0.55
            base += hill
            d = poly_dist(p, river)
            carve = smoothstep(0.62, 0.2, d)
            return base * (1 - carve) + (WL - 0.22) * carve

        return nodes, h, dict(sea=True, river=river, grid=(34, 30, 0, 4.0), cam=((0, -9.4, 8.6), (0, 0.8, 0), 32))
    if wi == 2:  # moon crater
        nodes = [(-4.4, -2.2), (-3.9, 1.2), (-1.2, 2.7), (2.2, 2.1), (3.0, -0.6), (0.3, 0.2)]
        center = Vector((0.0, 0.4, 0))
        small = [(-8.0, -3.5, 1.3), (7.6, -3.8, 1.0), (8.5, 3.5, 1.8), (-7.0, 5.5, 1.5)]

        def h(x, y):
            p = Vector((x, y, 0))
            n = noise.noise(Vector((x * 0.5, y * 0.5, 0)) + off) * 0.12
            r = (p - center).length
            z = 0.55 + n
            z += 0.75 * math.exp(-((r - 5.0) / 0.75) ** 2)
            if r < 5.0:
                z -= 0.5 * (1 - (r / 5.0) ** 2)
            z += 0.32 * math.exp(-(r / 1.0) ** 2)
            for cx, cy, cr in small:
                rr = math.hypot(x - cx, y - cy)
                z += 0.3 * math.exp(-((rr - cr) / 0.3) ** 2) - 0.3 * max(0.0, 1 - (rr / cr) ** 2)
            return z

        return nodes, h, dict(sea=False, grid=(36, 30, 0, 4.0), cam=((0, -10.4, 6.6), (0, 1.6, 0), 32), center=center)
    if wi == 3:  # desert bazaar
        nodes = [(-5.0, 0.9), (-3.2, -1.5), (-1.0, -0.3), (1.0, -1.7), (2.7, 0.2), (4.5, 1.8)]
        oasis = Vector((-2.2, 1.6, 0))

        def h(x, y):
            p = Vector((x, y, 0))
            dune = 0.22 * math.sin(0.85 * x + 0.7 * math.sin(0.6 * y)) * (0.6 + 0.4 * noise.noise(Vector((x * 0.3, y * 0.3, 1)) + off))
            z = 0.66 + dune + 0.1 * noise.noise(Vector((x * 0.9, y * 0.9, 2)) + off)
            d = (p - oasis).length
            z = z * (1 - smoothstep(1.15, 0.65, d)) + (WL - 0.15) * smoothstep(1.15, 0.65, d)
            coast = x - 0.55 * y - 7.6
            z = z * (1 - smoothstep(-0.8, 0.6, coast)) + (-0.45) * smoothstep(-0.8, 0.6, coast)
            return z

        return nodes, h, dict(sea=True, grid=(34, 28, 0, 3.5), cam=((0, -9.8, 8.0), (0, 0.4, 0), 34), oasis=oasis)
    # sky islands
    nodes = [(-4.9, -1.4), (-2.5, 0.0), (-0.3, -1.9), (0.1, 1.2), (2.7, -0.2), (4.7, 1.5)]
    isles = [(-4.5, -1.1, 1.35, 1.05), (-2.4, 0.05, 0.85, 0.75), (-0.2, -1.85, 1.0, 0.8), (0.3, 1.25, 1.45, 1.05), (2.75, -0.2, 1.0, 0.85), (4.75, 1.45, 1.35, 1.1),
             (-7.6, 3.4, 1.0, 0.7), (7.6, -2.8, 0.9, 0.6), (6.3, 4.6, 0.8, 0.6), (-6.9, -4.3, 0.7, 0.5)]

    def h(x, y):
        p = Vector((x, y, 0))
        m = max(islet(p, *i, off) for i in isles)
        if m < 0.35:
            return None
        return 0.35 + 0.35 * m + 0.08 * noise.noise(Vector((x, y, 4)) + off)

    return nodes, h, dict(sea=False, sky=True, isles=isles, grid=(28, 20, 0, 1.5), cam=((0, -10.6, 5.6), (0, 1.4, -0.2), 33))


THEMES = [
    dict(name="reef", top=0x7FC8F8, bottom=0xCFEFFF, sea=0x1F6FD0, sea_light=0x2C86E0, shallow=0x45D2D8, sand=0xF4DFA6, grass=0x7DCB4F, grass2=0x5DB045,
         rock=0xB9A68A, snow=0xF6F8FF, path=0xFFF3D2, pad=0xF3ECDB, seed=1, sun=(52, -12, -30)),
    dict(name="valley", top=0x9ED6F2, bottom=0xE6F6E8, sea=0x2873C9, sea_light=0x3A92DA, shallow=0x4FD0C9, sand=0xEDDDA5, grass=0x86D056, grass2=0x62B24A,
         rock=0x9FA7B5, snow=0xF6F9FF, path=0xF6E7C2, pad=0xF3ECDB, seed=7, sun=(50, -10, -40)),
    dict(name="moon", top=0x1C2152, bottom=0x070818, sea=None, sea_light=None, shallow=None, sand=0x9CA2BD, grass=0xB9BED3, grass2=0xA4AAC2,
         rock=0xD4D8E6, snow=0xE6E9F4, path=0xF5D547, pad=0xE3E7F2, seed=3, sun=(40, -20, -50)),
    dict(name="bazaar", top=0xF7B98A, bottom=0xFFE6BF, sea=0x2266B8, sea_light=0x2F7FCC, shallow=0x48C8CF, sand=0xF2D49A, grass=0xEAC384, grass2=0xDDAE6C,
         rock=0xC48A5A, snow=0xF6E7CF, path=0xFFF1D6, pad=0xF5E7CF, seed=11, sun=(30, -10, -60)),
    dict(name="lava", top=0x1E0A18, bottom=0xC2462A, sea=None, sea_light=None, shallow=None, sand=0x6E5F66, grass=0x4C4254, grass2=0x3E3546,
         rock=0x7D5C5C, snow=0xFF9A4A, path=0xFFC37A, pad=0xE9DCD2, seed=5, sun=(38, -12, -50)),
]


def build_world(wi):
    th = THEMES[wi]
    name = th["name"]
    sc = T.eevee_scene(W, H, transparent=False, samples=48)
    T.world_backdrop(sc, th["top"], th["bottom"], ambient=0.22 if name not in ("moon", "lava") else 0.12)
    _LIB.clear()
    _MODELS.clear()
    _HOUSES.clear()
    rnd = random.Random(th["seed"])
    off = Vector((th["seed"] * 3.1, th["seed"] * 1.7, 0))
    node_xy, base_h, ex = make_layout(wi, off)
    nodes = [Vector((x, y, 0)) for x, y in node_xy]
    sky = ex.get("sky", False)

    path_pts = []
    for i in range(5):
        a, b = nodes[i], nodes[i + 1]
        mid = (a + b) / 2 + Vector((0, 0.45 if i % 2 else -0.45, 0))
        for k in range(16):
            t = k / 16
            path_pts.append((1 - t) ** 2 * a + 2 * (1 - t) * t * mid + t * t * b)
    path_pts.append(nodes[-1])

    def height(x, y):
        h = base_h(x, y)
        if h is None:
            return None
        p = Vector((x, y, 0))
        dn = min((n - p).length for n in nodes)
        if dn < 0.9:  # every node sits on a level pad of land
            h = h * smoothstep(0.35, 0.9, dn) + PATH_H * (1 - smoothstep(0.35, 0.9, dn))
        if h > WL + 0.1:
            d = min((q - p).length for q in path_pts[::2])
            flat = smoothstep(0.8, 0.35, d) * 0.85
            h = h * (1 - flat) + max(h, PATH_H) * flat
        return h

    # ---- terrain: flat cel bands by zone; foam at the waterline
    def zone(h):
        if h < WL:
            return 0.3 * max(0.0, (h + 0.5) / (WL + 0.5))
        if h < WL + 0.025:
            return 0.31
        if h < 0.44:
            return 0.36
        if h < 1.5:
            return 0.62
        if h < 2.1:
            return 0.9
        return 0.97

    def terrain_color(nt):
        at = nt.nodes.new("ShaderNodeAttribute")
        at.attribute_name = "zone"
        nz = nt.nodes.new("ShaderNodeTexNoise")
        nz.inputs["Scale"].default_value = 2.0
        jit = nt.nodes.new("ShaderNodeMath")
        jit.operation = "MULTIPLY_ADD"
        jit.inputs[1].default_value = 0.09
        nt.links.new(nz.outputs["Fac"], jit.inputs[0])
        nt.links.new(at.outputs["Fac"], jit.inputs[2])
        sea = th["sea"] or 0x0A0C22
        shallow = th["shallow"] or 0x0A0C22
        stops = [
            (0.0, T.lin(sea)),
            (0.2, T.lin(shallow)),
            (0.3, T.lin(0xFFFFFF if th["sea"] else th["sand"])),
            (0.33, T.lin(th["sand"])),
            (0.6, T.lin(th["grass"])),
            (0.69, T.lin(th["grass2"])),
            (0.88, T.lin(th["rock"])),
            (0.985, T.lin(th["snow"])),
        ]
        r = T._ramp(nt, stops)
        nt.links.new(jit.outputs[0], r.inputs["Fac"])
        return r.outputs["Color"]

    gw, gh, gx, gy = ex["grid"]

    def terrain(bm):
        n = 300
        zl = bm.verts.layers.float.new("zone")
        grid = []
        for j in range(n + 1):
            row = []
            for i in range(n + 1):
                x = gx - gw / 2 + gw * i / n
                y = gy - gh / 2 + gh * j / n
                hh = height(x, y)
                v = bm.verts.new((x, y, hh if hh is not None else -3.0))
                v[zl] = zone(hh) if hh is not None else 0.0
                row.append((v, hh is not None))
            grid.append(row)
        for j in range(n):
            for i in range(n):
                q = (grid[j][i], grid[j][i + 1], grid[j + 1][i + 1], grid[j + 1][i])
                if all(ok for _, ok in q):
                    bm.faces.new([v for v, _ in q])

    obj_from("terrain", terrain, T.toon("ground", 0, base_socket_fn=terrain_color, spec=0.0, rim=0.08, threshold=0.25))

    if ex.get("sea"):
        obj_from("sea", lambda bm: bmesh.ops.create_grid(bm, x_segments=1, y_segments=1, size=60), sea_material(th), (0, 0, WL), smooth=False)
        obj_from("deep", lambda bm: bmesh.ops.create_grid(bm, x_segments=1, y_segments=1, size=60), T.toon("deep", th["sea"], spec=0.0, rim=0.0), (0, 0, -0.6), smooth=False)

    rockm = T.toon("rock", th["rock"], spec=0.15, rim=0.2)
    leaf = T.toon("leaf", th["grass2"] if name != "bazaar" else 0x6FBF4A, spec=0.25, rim=0.25)
    leaf2 = T.toon("leaf2", 0x9BDB52, spec=0.25, rim=0.25)
    trunk = T.toon("trunk", 0x9A6A43, spec=0.1)
    wood = T.toon("wood", 0xB07A4A, spec=0.15)
    rope = T.toon("rope", 0x6B4A2E, spec=0.0, rim=0.0)

    # ---- path: CC0 path stones on land, rope-rail plank bridges over water and the void
    stone_paths = [mk("RockPath_Round_Small_1"), mk("RockPath_Round_Small_2"), mk("RockPath_Round_Small_3")]
    path_tint = {"moon": 0xFFE27A, "lava": 0xFFC37A, "bazaar": 0xFFF1D6}.get(name)
    for k, p in enumerate(path_pts):
        if any((p - n).length < 0.5 for n in nodes):
            continue
        hh = height(p.x, p.y)
        if hh is not None and hh > WL + 0.08:
            if k % 2 == 0:
                put(stone_paths[k % 3], p.x, p.y, hh - 0.005, 0.32, rot=k * 1.3, by_height=False, tint=path_tint)
        else:
            nxt = path_pts[min(k + 1, len(path_pts) - 1)]
            prv = path_pts[max(k - 1, 0)]
            ang = math.atan2(nxt.y - prv.y, nxt.x - prv.x)
            z = PATH_H - 0.08
            box("plank", 0.12, 0.42, 0.04, wood, (p.x, p.y, z), rot=(0, 0, ang))
            if k % 3 == 0:
                for side in (-1, 1):
                    px = p.x - math.sin(ang) * 0.22 * side
                    py = p.y + math.cos(ang) * 0.22 * side
                    cyl("post", 0.02, 0.02, 0.62 if not sky else 0.25, rope, (px, py, z - (0.42 if not sky else 0.05)), seg=6)

    medal_c = {"reef": 0x2E9BD6, "valley": 0x5DB045, "moon": 0xF5D547, "bazaar": 0xD9414E, "lava": 0xFF7A22}[name]
    stone = T.toon("padstone", th["pad"], spec=0.3, rim=0.2, ao=0.3)
    stone2 = T.toon("padstone2", 0x8C93A8 if name != "lava" else 0x4C4254, spec=0.15, rim=0.2)
    medal = T.toon("medal", medal_c, spec=0.8, spec_size=0.5, rim=0.3)
    for i, n in enumerate(nodes):
        node_pad(i, n.x, n.y, height(n.x, n.y), stone, stone2, medal)

    def clear_spot(x, y, r=0.65):
        p = Vector((x, y, 0))
        return all((q - p).length > r for q in path_pts) and all((q - p).length > 0.85 for q in nodes)

    def scatter(fn, count, zmin=WL + 0.12, zmax=9.0, rr=0.65, region=None):
        made = tries = 0
        while made < count and tries < count * 80:
            tries += 1
            if region:
                x, y = rnd.uniform(region[0], region[1]), rnd.uniform(region[2], region[3])
            else:
                x, y = rnd.uniform(gx - gw / 2.6, gx + gw / 2.6), rnd.uniform(gy - gh / 2.4, gy + gh / 2.6)
            z = height(x, y)
            if z is not None and zmin <= z <= zmax and clear_spot(x, y, rr):
                fn(x, y, z, rnd)
                made += 1

    n6 = nodes[5]
    lx, ly = n6.x - 0.1, n6.y + 0.95
    cm = T.toon("cloud", 0xFFFFFF, shadow=(0.78, 0.84, 0.97), spec=0.0, rim=0.15, threshold=0.2)
    must = [Vector((n.x, n.y, (height(n.x, n.y) or PATH_H) + 0.3)) for n in nodes]
    fog = dict(color=th["bottom"], start=8.0, depth=26.0, strength=0.3)

    def mpick(lst):
        return lst[rnd.randrange(len(lst))]

    trees_mk = [mk("CommonTree_%d" % i) for i in range(1, 6)]
    pines_mk = [mk("Pine_%d" % i) for i in range(1, 6)]
    twisted_mk = [mk("TwistedTree_%d" % i) for i in range(1, 6)]
    dead_mk = [mk("DeadTree_%d" % i) for i in range(1, 6)]
    bushes_mk = [mk("Bush_Common"), mk("Bush_Common_Flowers"), mk("Plant_1_Big"), mk("Plant_7_Big"), mk("Fern_1")]
    flowers_mk = [mk("Flower_3_Group"), mk("Flower_4_Group"), mk("Grass_Wispy_Tall"), mk("Grass_Common_Tall"), mk("Clover_1")]
    rocks_mk = [mk("Rock_Medium_1"), mk("Rock_Medium_2"), mk("Rock_Medium_3")]
    palms_kn = [kn("tree_palmTall"), kn("tree_palmDetailedTall"), kn("tree_palmBend"), kn("tree_palm")]

    if name == "reef":
        scatter(lambda x, y, z, r: put(mpick(palms_kn), x, y, z, r.uniform(0.75, 1.05), r.uniform(0, 6)), 26, zmax=0.8)
        scatter(lambda x, y, z, r: put(mpick(trees_mk), x, y, z, r.uniform(0.8, 1.05), r.uniform(0, 6)), 6, zmin=0.5)
        scatter(lambda x, y, z, r: put(mpick(bushes_mk[2:]), x, y, z, r.uniform(0.12, 0.2), r.uniform(0, 6)), 18, rr=0.5)
        scatter(lambda x, y, z, r: put(mpick(rocks_mk), x, y, z, r.uniform(0.18, 0.32), r.uniform(0, 6)), 12, zmin=WL, zmax=0.45, rr=0.5)
        lx, ly = n6.x + 0.8, n6.y + 0.55
        lz = height(lx, ly)
        lighthouse(lx, ly, lz)
        must += [Vector((lx, ly, lz)), Vector((lx, ly, lz + 2.25))]
        hullm, sailm, stripe = T.toon("hull", 0x8A5A3B, spec=0.2), T.toon("sail", 0xFFFDF5, spec=0.0, rim=0.2), T.toon("stripe", 0xD9414E)
        for bx, by in ((-1.9, -2.6), (2.2, -2.4), (6.4, 1.6), (-6.8, 1.2)):
            sailboat(bx, by, WL, rnd, hullm, sailm, stripe)
        put(kn("canoe"), -2.0, 1.9, WL - 0.02, 0.55, 0.7, by_height=False)
        gm = T.toon("gull", 0xFFFFFF, spec=0.0, rim=0.0)
        for k in range(7):
            gull(rnd.uniform(-5, 5), rnd.uniform(0.5, 5), rnd.uniform(1.6, 2.8), gm)
        T.fog_volume("seamist", (0, 9, WL + 0.3), (40, 8, 0.8), 0xE8F6FF, 0.25, WL + 1.0)
        for cx in (-6.5, -1.5, 4.0):
            cloud(cx + rnd.uniform(-1, 1), 8.5 + rnd.uniform(0, 2), 2.2 + rnd.uniform(0, 0.8), rnd, cm)
        fog.update(color=0xCFEFFF, start=10.0, depth=22.0, strength=0.35)
    elif name == "valley":
        scatter(lambda x, y, z, r: put(mpick(trees_mk), x, y, z, r.uniform(0.8, 1.15), r.uniform(0, 6)), 30, zmax=1.4)
        scatter(lambda x, y, z, r: put(mpick(pines_mk), x, y, z, r.uniform(0.9, 1.3), r.uniform(0, 6)), 34, zmin=1.0, zmax=2.4)
        scatter(lambda x, y, z, r: put(mpick(bushes_mk), x, y, z, r.uniform(0.18, 0.3), r.uniform(0, 6)), 30, rr=0.5, zmax=1.4)
        scatter(lambda x, y, z, r: put(mpick(flowers_mk), x, y, z, r.uniform(0.1, 0.18), r.uniform(0, 6)), 70, rr=0.4, zmax=1.4)
        scatter(lambda x, y, z, r: put(mpick(rocks_mk), x, y, z, r.uniform(0.2, 0.4), r.uniform(0, 6)), 8)
        for k, p in enumerate(ex["river"][1:-1]):
            put(kn("lily_large" if k % 2 else "lily_small"), p.x + 0.12, p.y, WL + 0.005, 0.22, k * 1.7, by_height=False)
        # farm: a windmill, a house, a fence and crops
        fx, fy = -4.7, 0.6
        fz = height(fx, fy)
        windmill(fx, fy, fz, T.toon("wmw", 0xF6F2EA, spec=0.3), T.toon("wmr", 0xD9414E, spec=0.4), T.toon("wmd", 0x4A3428), T.toon("wmb", 0xC9A27A))
        must += [Vector((fx, fy, fz + 1.4))]
        put_house(0, fx + 1.1, fy - 0.2, height(fx + 1.1, fy - 0.2), 0.75, 0.3)
        for k in range(6):
            put(kn("crops_wheatStageB"), fx + 0.2 + k * 0.22, fy + 1.0, height(fx + 0.2 + k * 0.22, fy + 1.0), 0.22, 0)
        petal = T.toon("petal", 0xF06CC4, spec=0.4)
        lz = height(lx, ly)
        cyl("stem", 0.07, 0.06, 1.1, T.toon("stemg", 0x5DB045), (lx, ly, lz))
        for k in range(8):
            a = k * 2 * math.pi / 8
            pp = sphere("pet", 0.24, petal, (lx + math.cos(a) * 0.3, ly + math.sin(a) * 0.3, lz + 1.15), (1.25, 0.6, 0.25), seg=16)
            pp.rotation_euler = (0, 0, a)
        sphere("core", 0.17, T.toon("core", 0xFFE15C, spec=0.5), (lx, ly, lz + 1.2))
        must += [Vector((lx, ly, lz)), Vector((lx, ly, lz + 1.45))]
        petals = [mk("Petal_%d" % i) for i in range(1, 6)]
        for k in range(40):
            put(mpick(petals), rnd.uniform(-6, 6), rnd.uniform(-3, 5), rnd.uniform(0.9, 2.4), 0.06, rnd.uniform(0, 6), by_height=False)
        T.fog_volume("rivermist", (-2.2, -0.5, WL + 0.25), (2.4, 7.0, 0.6), 0xFFFFFF, 0.9, WL + 0.6)
        for cx in (-7, 0.5, 6.5):
            cloud(cx, 10.5 + rnd.uniform(0, 1.5), 4.2 + rnd.uniform(0, 0.8), rnd, cm, 1.3)
        fog.update(color=0xE6F6E8, start=9.0, depth=24.0, strength=0.4)
    elif name == "moon":
        for nm, cnt, s0, s1 in (("moon_rock_01", 12, 0.25, 0.5), ("moon_rock_03", 14, 0.15, 0.3), ("moon_rock_05", 7, 0.35, 0.7)):
            scatter(lambda x, y, z, r, nm=nm, s0=s0, s1=s1: place_rock(nm, rockm, x, y, z, r.uniform(s0, s1), r.uniform(0, 6)), cnt, zmin=-9)
        spires = [kn("rock_tallA"), kn("rock_tallC"), kn("rock_tallF"), kn("rock_tallH")]
        scatter(lambda x, y, z, r: put(mpick(spires), x, y, z, r.uniform(0.5, 1.0), r.uniform(0, 6), tint=0xC9D0E8), 10, zmin=-9, rr=0.8)
        glows = [T.glow("g%d" % i, c, 2.0) for i, c in enumerate([0xF5D547, 0xE8475A, 0x4C7BF2])]
        scatter(lambda x, y, z, r: cyl("pill", 0.07, 0.07, 0.26, glows[r.randrange(3)], (x, y, z + 0.08), rot=(math.radians(90), 0, r.random() * 3)), 16, zmin=-9)
        c = ex["center"]
        lx, ly = c.x + 1.0, c.y - 0.3
        lz = height(lx, ly)
        dome = T.toon("dome", 0xBFE9FF, spec=0.8, alpha=0.55)
        hullw = T.toon("module", 0xEEF2FA, spec=0.5)
        torus("domering", 0.7, 0.06, T.toon("dring", 0x8C93A8, spec=0.3), (lx, ly, lz + 0.03))
        sphere("dome", 0.7, dome, (lx, ly, lz), (1, 1, 0.75), seg=32)
        for k in range(3):
            a = k * 2.1 + 0.6
            mx, my = lx + math.cos(a) * 1.05, ly + math.sin(a) * 0.95
            cyl("mod", 0.14, 0.14, 0.42, hullw, (mx, my, height(mx, my) + 0.14), rot=(0, math.radians(90), a), seg=16)
        cyl("antenna", 0.015, 0.01, 0.9, T.toon("ant", 0x5A6070), (lx + 0.45, ly + 0.35, lz + 0.3), seg=6)
        sphere("beacon", 0.07, glows[1], (lx + 0.45, ly + 0.35, lz + 1.22))
        must += [Vector((lx, ly, lz)), Vector((lx + 0.45, ly + 0.35, lz + 1.3))]
        star = T.glow("star", 0xFFFFFF, 4.0)
        for k in range(220):
            sphere("star", rnd.uniform(0.02, 0.045), star, (rnd.uniform(-30, 30), rnd.uniform(14, 40), rnd.uniform(1, 14)), seg=6)
        shoot = T.glow("shoot", 0xBFE6FF, 3.0)
        for k in range(3):
            sx, sy, sz = rnd.uniform(-12, 12), rnd.uniform(18, 26), rnd.uniform(6, 12)
            box("meteor", 1.6, 0.03, 0.03, shoot, (sx, sy, sz), rot=(0, math.radians(-18), math.radians(rnd.uniform(10, 40))))
        planet = T.toon("planet", 0xF2A35E, shadow=(0.45, 0.4, 0.75), spec=0.2, rim=0.35)
        sphere("planet", 2.6, planet, (-7.0, 24.0, 4.5), seg=48)
        ring = torus("pring", 4.0, 0.14, T.toon("pring", 0xFFD9A0, spec=0.0), (-7.0, 24.0, 4.5))
        ring.scale = (1, 1, 0.25)
        ring.rotation_euler = (math.radians(70), math.radians(-20), 0)
        T.fog_volume("dust", (0, 0.5, 0.25), (24, 18, 0.7), 0xB8C0E8, 0.35, 0.7)
        fog.update(color=0x1C2152, start=12.0, depth=30.0, strength=0.3)
    elif name == "bazaar":
        warm = 0xFFE2B8
        scatter(lambda x, y, z, r: put_house(r.randrange(4), x, y, z, r.uniform(0.55, 0.7), r.uniform(-0.3, 0.3) + math.pi * r.randrange(2), tint=warm), 14, region=(1.4, 6.4, -0.8, 3.8), rr=0.65)
        walls = T.toon("wall", 0xEBD0A6, spec=0.1, ao=0.3)
        for k in range(26):
            a = k * 2 * math.pi / 26
            wx, wy = 3.7 + math.cos(a) * 2.9, 1.2 + math.sin(a) * 2.3
            wz = height(wx, wy)
            if wz is not None and wz > WL + 0.1 and clear_spot(wx, wy, 0.35):
                box("wall", 0.75, 0.16, 0.32, walls, (wx, wy, wz - 0.05), rot=(0, 0, a + math.pi / 2))
                box("merlon", 0.16, 0.16, 0.1, walls, (wx, wy, wz + 0.27), rot=(0, 0, a))
        tents = [kn("tent_detailedOpen"), kn("tent_smallOpen")]
        scatter(lambda x, y, z, r: put(mpick(tents), x, y, z, r.uniform(0.3, 0.42), r.uniform(0, 6), by_height=False), 10, region=(-1.5, 2.5, -2.5, 1.5), rr=0.5)
        scatter(lambda x, y, z, r: put(kn(r.choice(["cactus_tall", "cactus_short"])), x, y, z, r.uniform(0.25, 0.45), r.uniform(0, 6)), 16)
        scatter(lambda x, y, z, r: put(kn(r.choice(["pot_large", "pot_small"])), x, y, z, r.uniform(0.1, 0.16), r.uniform(0, 6)), 18, region=(0.5, 6.5, -1.5, 4.0), rr=0.4)
        put(vl("Prop_Wagon"), -0.4, 1.3, height(-0.4, 1.3), 0.5, 0.8, by_height=False, tint=warm)
        o = ex["oasis"]
        for k in range(9):
            a = k * 2 * math.pi / 9 + 0.3
            px, py = o.x + math.cos(a) * 1.35, o.y + math.sin(a) * 1.2
            pz = height(px, py)
            if pz is not None and pz > WL + 0.1 and clear_spot(px, py, 0.4):
                put(mpick(palms_kn), px, py, pz, rnd.uniform(0.8, 1.1), rnd.uniform(0, 6))
        put(kn("lily_large"), o.x, o.y, WL + 0.005, 0.25, 0.3, by_height=False)
        lz = height(lx, ly)
        mwall = T.toon("minw", 0xF6E9CF, spec=0.15, ao=0.3)
        cyl("minaret", 0.28, 0.22, 1.5, mwall, (lx, ly, lz), seg=12)
        cyl("ring", 0.3, 0.3, 0.06, T.toon("minr", 0xD9414E, spec=0.4), (lx, ly, lz + 1.1), seg=16)
        sphere("onion", 0.3, T.toon("onion", 0x9B63E0, spec=0.5), (lx, ly, lz + 1.62), (1, 1, 1.2))
        must += [Vector((lx, ly, lz)), Vector((lx, ly, lz + 2.05))]
        sandm = T.toon("sandgrain", 0xF2D49A, spec=0.0, rim=0.0)
        for k in range(260):
            bx = rnd.uniform(-8, 8)
            box("sand", 0.12, 0.012, 0.012, sandm, (bx, rnd.uniform(-4, 7), rnd.uniform(0.6, 2.2)), rot=(0, 0, math.radians(-8)))
        T.fog_volume("heat", (0, 2, 0.6), (26, 20, 1.2), 0xFFD9A8, 0.22, 1.4)
        fog.update(color=0xFFE6BF, start=7.0, depth=22.0, strength=0.45)
    else:  # lava
        lavam = bpy.data.materials.new("lava")
        lavam.use_nodes = True
        lnt = lavam.node_tree
        for nd in list(lnt.nodes):
            lnt.nodes.remove(nd)
        lout = lnt.nodes.new("ShaderNodeOutputMaterial")
        lem = lnt.nodes.new("ShaderNodeEmission")
        lem.inputs["Strength"].default_value = 1.6
        tc = lnt.nodes.new("ShaderNodeTexCoord")
        nz = lnt.nodes.new("ShaderNodeTexNoise")
        nz.inputs["Scale"].default_value = 0.8
        warp = lnt.nodes.new("ShaderNodeVectorMath")
        warp.operation = "MULTIPLY_ADD"
        warp.inputs[1].default_value = (1.6, 1.6, 1.6)
        lnt.links.new(nz.outputs["Color"], warp.inputs[0])
        lnt.links.new(tc.outputs["Object"], warp.inputs[2])
        v = lnt.nodes.new("ShaderNodeTexVoronoi")
        v.feature = "DISTANCE_TO_EDGE"
        v.inputs["Scale"].default_value = 0.8
        lnt.links.new(warp.outputs[0], v.inputs["Vector"])
        heat = lnt.nodes.new("ShaderNodeTexNoise")
        heat.inputs["Scale"].default_value = 0.6
        lnt.links.new(tc.outputs["Object"], heat.inputs["Vector"])
        mixd = lnt.nodes.new("ShaderNodeMath")
        mixd.operation = "MULTIPLY_ADD"
        mixd.inputs[1].default_value = 0.22
        lnt.links.new(heat.outputs["Fac"], mixd.inputs[0])
        lnt.links.new(v.outputs["Distance"], mixd.inputs[2])
        sub = lnt.nodes.new("ShaderNodeMath")
        sub.operation = "SUBTRACT"
        sub.inputs[1].default_value = 0.1
        lnt.links.new(mixd.outputs[0], sub.inputs[0])
        rr_ = T._ramp(lnt, [(0.0, T.lin(0xFFE27A)), (0.035, T.lin(0xFF7A22)), (0.1, T.lin(0xB8361A)), (0.22, T.lin(0x5A1C14)), (0.4, T.lin(0x2E1210))], interp="LINEAR")
        lnt.links.new(sub.outputs[0], rr_.inputs["Fac"])
        lnt.links.new(rr_.outputs["Color"], lem.inputs["Color"])
        lnt.links.new(lem.outputs[0], lout.inputs["Surface"])
        obj_from("lava", lambda bm: bmesh.ops.create_grid(bm, x_segments=1, y_segments=1, size=70), lavam, (0, 0, -0.8), smooth=False)
        basalt = T.toon("basalt", 0x3A3140, shadow=(0.5, 0.42, 0.62), spec=0.1, rim=0.35, ao=0.3)
        for k, (cx, cy, rx, ry) in enumerate(ex["isles"]):
            r0 = min(rx, ry) * 0.95
            cyl("pillar%d" % k, r0 * 0.8, r0 * 0.95, 1.15, basalt, (cx, cy, -0.85), seg=7, smooth=False)
            for j in range(4):
                a = j * 1.6 + k
                cyl("col", r0 * 0.22, r0 * 0.28, 0.9 + 0.2 * math.sin(a), basalt, (cx + math.cos(a) * r0 * 0.75, cy + math.sin(a) * r0 * 0.6, -0.85), seg=6, smooth=False)
        fall = T.glow("lavafall", 0xFF8A2A, 1.8)
        for wx, wy in ((0.45, 0.2), (-4.3, -1.9), (4.9, 0.6)):
            box("lavafall", 0.16, 0.05, 1.25, fall, (wx, wy, -0.8))
        obsidian = T.toon("obsidian", 0x5B3B8C, shadow=(0.4, 0.35, 0.7), spec=0.8, rim=0.4)
        scatter(lambda x, y, z, r: cyl("shard", 0.07, 0.0, r.uniform(0.25, 0.45), obsidian, (x, y, z - 0.02), seg=5, smooth=False, rot=(r.uniform(-0.3, 0.3), r.uniform(-0.3, 0.3), 0)), 20, rr=0.45, zmin=0.3)
        scatter(lambda x, y, z, r: put(mpick(dead_mk), x, y, z, r.uniform(0.6, 0.9), r.uniform(0, 6), tint=0x6A5560), 12, zmin=0.3)
        scatter(lambda x, y, z, r: put(mpick(twisted_mk), x, y, z, r.uniform(0.6, 0.85), r.uniform(0, 6), tint=0xB06A50), 5, zmin=0.3)
        scatter(lambda x, y, z, r: put(kn("statue_obelisk"), x, y, z, r.uniform(0.45, 0.6), r.uniform(0, 6), tint=0x6B5A70), 3, zmin=0.3)
        vx, vy = lx + 0.3, ly + 1.4
        vz = (height(lx, ly) or PATH_H) - 0.35
        volcano(vx, vy, vz, T.toon("cone", 0x4A3A44, shadow=(0.5, 0.42, 0.62), spec=0.05, rim=0.35, ao=0.3), T.glow("crater", 0xFFB050, 3.0), fall)
        smoke = T.toon("smoke", 0x4A3A44, shadow=(0.6, 0.55, 0.7), spec=0.0, rim=0.25, threshold=0.2)
        for k in range(6):
            cloud(vx + rnd.uniform(-0.3, 0.5) + k * 0.15, vy + 0.3 + k * 0.25, vz + 2.4 + k * 0.42, rnd, smoke, 0.8 + k * 0.28)
        must += [Vector((vx, vy, vz)), Vector((vx, vy, vz + 2.2)), Vector((vx + 0.6, vy + 1.5, vz + 4.2))]
        ember = T.glow("ember", 0xFFB050, 3.0)
        for k in range(110):
            sphere("ember", rnd.uniform(0.015, 0.035), ember, (rnd.uniform(-9, 9), rnd.uniform(-4, 7), rnd.uniform(-0.8, 3.2)), seg=6)
        ash = T.toon("ash", 0x6E6470, spec=0.0, rim=0.0)
        for k in range(160):
            box("ash", 0.04, 0.04, 0.006, ash, (rnd.uniform(-9, 9), rnd.uniform(-4, 8), rnd.uniform(0, 3.5)), rot=(rnd.uniform(0, 3), rnd.uniform(0, 3), 0))
        T.fog_volume("smokefog", (0, 2, -0.3), (30, 22, 1.2), 0x6A2A24, 0.35, 0.6)
        for k in range(6):
            cloud(rnd.uniform(-11, 11), rnd.uniform(8, 14), rnd.uniform(1.5, 3.5), rnd, smoke, 1.6)
        fog.update(color=0x7A2A20, start=7.0, depth=24.0, strength=0.4)

    T.sun(sc, th["sun"], energy=4.5 if name not in ("moon", "lava") else 3.2, color={"bazaar": 0xFFD9B8, "lava": 0xFFB48A}.get(name, 0xFFF1DA))
    (cloc, ctgt, lens) = ex["cam"]
    cd = bpy.data.cameras.new("cam")
    cd.lens = lens
    cam = bpy.data.objects.new("cam", cd)
    cam.location = cloc
    cam.rotation_euler = (Vector(ctgt) - Vector(cloc)).to_track_quat("-Z", "Y").to_euler()
    sc.collection.objects.link(cam)
    sc.camera = cam
    auto_frame(sc, cam, must)
    T.post(sc, fog=fog["color"], fog_start=fog["start"], fog_depth=fog["depth"], fog_strength=fog["strength"], bloom_threshold=0.92, bloom_strength=0.3)
    try:
        sc.eevee.volumetric_tile_size = "4"
        sc.eevee.volumetric_samples = 48
    except Exception:
        pass
    bpy.context.view_layer.update()

    with open(os.path.join(OUT, "world%d.txt" % wi), "w") as f:
        for n in nodes:
            v = world_to_camera_view(sc, cam, Vector((n.x, n.y, height(n.x, n.y) + 0.16)))
            f.write("%d %d\n" % (round(v.x * W), round((1 - v.y) * H)))
    sc.render.filepath = os.path.join(OUT, "world%d.tga" % wi)
    bpy.ops.render.render(write_still=True)
    print("world", wi, "done", flush=True)


for wi in range(5):
    if ONLY is None or ONLY == wi:
        build_world(wi)
