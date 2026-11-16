"""DR-PURTRIS cast export (the only art export).

Builds every piece as a small 3D model and renders it with Cycles to a transparent,
uncompressed RGBA TGA that the engine embeds. Faces and colorblind patterns are NOT baked:
the engine draws them as a decoration layer so they can be toggled and never hide a color.

Run (headless):
  blender -b --factory-startup --python tools/blender/export_cast.py -- assets/sprites
  flatpak run org.blender.Blender -b --factory-startup --python tools/blender/export_cast.py -- assets/sprites
"""

import bpy
import bmesh
sys_path_added = None
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import toon_lib as T  # noqa: E402

OUT = sys.argv[sys.argv.index("--") + 1] if "--" in sys.argv else "assets/sprites"
OUT = os.path.abspath(OUT)
os.makedirs(OUT, exist_ok=True)
SIZE = 256
SAMPLES = 64

PALETTE = {
    "red": 0xE8475A,
    "orange": 0xF59A3B,
    "yellow": 0xF5D547,
    "green": 0x5BD07A,
    "cyan": 0x47D1E8,
    "blue": 0x4C7BF2,
    "purple": 0xA66CF0,
    "pink": 0xF06CC4,
    "gray": 0x8C93A8,
    "white": 0xD8DCE8,
}


def srgb_to_linear(c):
    c = c / 255.0
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def rgba(hexv, a=1.0):
    return (srgb_to_linear((hexv >> 16) & 255), srgb_to_linear((hexv >> 8) & 255), srgb_to_linear(hexv & 255), a)


def setinput(node, names, value):
    for n in names:
        if n in node.inputs:
            node.inputs[n].default_value = value
            return


def reset():
    """Toon pieces: EEVEE, transparent film, orthographic camera looking down, key sun from the top-left."""
    sc = T.eevee_scene(SIZE, SIZE, transparent=True, samples=96)
    T.world_backdrop(sc, 0x000000, 0x000000, ambient=0.25)
    cam_data = bpy.data.cameras.new("cam")
    cam_data.type = "ORTHO"
    cam_data.ortho_scale = 2.0
    cam = bpy.data.objects.new("cam", cam_data)
    cam.location = (0, 0, 10)
    sc.collection.objects.link(cam)
    sc.camera = cam
    T.sun(sc, (-35, -28, 0), energy=4.0, color=0xFFF6E8, angle=2.0)


def vec_to_euler(d):
    from mathutils import Vector

    return Vector(d).to_track_quat("-Z", "Y").to_euler()


def material(name, hexv, rough=0.28, coat=0.6, sss=0.0, emit=0.0, metal=0.0, alpha=1.0):
    spec = 0.75 if rough < 0.25 else 0.5
    if emit >= 1.0:
        return T.glow(name, hexv, emit)
    return T.toon(name, hexv, spec=spec, spec_size=0.55, rim=0.3, alpha=None if alpha >= 1.0 else alpha, threshold=0.3)


def link(obj, mat=None):
    bpy.context.scene.collection.objects.link(obj)
    if mat is not None:
        if obj.data.materials:
            obj.data.materials[0] = mat
        else:
            obj.data.materials.append(mat)
    return obj


def smooth(obj):
    for poly in obj.data.polygons:
        poly.use_smooth = True


def mesh_obj(name, build):
    me = bpy.data.meshes.new(name)
    bm = bmesh.new()
    build(bm)
    bm.to_mesh(me)
    bm.free()
    return bpy.data.objects.new(name, me)


def cube(name, sx, sy, sz):
    return mesh_obj(name, lambda bm: (bmesh.ops.create_cube(bm, size=1.0), bmesh.ops.scale(bm, vec=(sx, sy, sz), verts=bm.verts)))


def sphere(name, r, seg=48, rings=24):
    return mesh_obj(name, lambda bm: bmesh.ops.create_uvsphere(bm, u_segments=seg, v_segments=rings, radius=r))


def cylinder(name, r, depth, seg=48):
    return mesh_obj(name, lambda bm: bmesh.ops.create_cone(bm, cap_ends=True, segments=seg, radius1=r, radius2=r, depth=depth))


def bevel(obj, width, segments=5):
    mod = obj.modifiers.new("bevel", "BEVEL")
    mod.width = width
    mod.segments = segments
    mod.limit_method = "ANGLE"
    smooth(obj)
    try:
        obj.data.set_sharp_from_angle(angle=math.radians(40))
    except Exception:
        pass


def render(name):
    sc = bpy.context.scene
    path = os.path.join(OUT, name + ".tga")
    sc.render.filepath = path
    bpy.ops.render.render(write_still=True)
    print("rendered", path)


# ------------------------------------------------------------------ pieces


def cushion_cut(bm):
    """A square cushion-cut jewel: beveled girdle, faceted crown, flat table."""
    import bmesh as _b
    def ring(half, z, inset=0.0):
        return [bm.verts.new((x * (half - inset), y * (half - inset), z)) for x, y in ((-1, -1), (1, -1), (1, 1), (-1, 1))]
    def oct_ring(half, cut, z):
        """Chamfered square, counter-clockwise."""
        h, c = half, cut
        pts = [(-h + c, -h), (h - c, -h), (h, -h + c), (h, h - c), (h - c, h), (-h + c, h), (-h, h - c), (-h, -h + c)]
        return [bm.verts.new((px, py, z)) for px, py in pts]
    bottom = oct_ring(0.86, 0.16, -0.32)
    girdle_lo = oct_ring(0.9, 0.17, -0.08)
    girdle_hi = oct_ring(0.9, 0.17, 0.12)
    crown = oct_ring(0.7, 0.13, 0.38)
    table = oct_ring(0.42, 0.08, 0.48)
    rings = [bottom, girdle_lo, girdle_hi, crown, table]
    for ri, (r0, r1) in enumerate(zip(rings, rings[1:])):
        for i in range(8):
            f = bm.faces.new((r0[i], r0[(i + 1) % 8], r1[(i + 1) % 8], r1[i]))
            # 0 crown facets, 1 table, 2 girdle; alternate crown facets get a second tone
            f.material_index = 2 if ri < 2 else (0 if (i + ri) % 2 == 0 else 3)
    bm.faces.new(list(reversed(bottom))).material_index = 2
    bm.faces.new(table).material_index = 1


def gem(color):
    """Quad gem: a cushion-cut jewel with a bright table, two-tone crown facets, a deep girdle."""
    reset()
    c = PALETTE[color]

    def tone(hexv, k):
        r, g, b = (hexv >> 16) & 255, (hexv >> 8) & 255, hexv & 255
        if k >= 0:
            f = lambda v: int(v + (255 - v) * k)
        else:
            f = lambda v: int(v * (1 + k))
        return (f(r) << 16) | (f(g) << 8) | f(b)

    crown = T.toon("crown_" + color, c, spec=0.5, spec_size=0.7, rim=0.4, threshold=0.25)
    table = T.toon("table_" + color, tone(c, 0.38), spec=0.9, spec_size=0.86, rim=0.2, threshold=0.2, emit_boost=1.05)
    girdle = T.toon("girdle_" + color, tone(c, -0.28), spec=0.2, rim=0.3)
    crown2 = T.toon("crown2_" + color, tone(c, 0.16), spec=0.5, spec_size=0.7, rim=0.4, threshold=0.25)
    o = link(mesh_obj("gem", cushion_cut))
    for m in (crown, table, girdle, crown2):
        o.data.materials.append(m)
    for poly in o.data.polygons:
        poly.use_smooth = False
    o.scale = (1.06, 1.06, 1.0)
    # a small four-point sparkle on the table
    glint = T.glow("glint", 0xFFFFFF, 1.0)
    for ang, ln in ((0, 0.2), (90, 0.2), (45, 0.08), (135, 0.08)):
        sp = link(sphere("glint", 1.0), glint)
        sp.scale = (ln, 0.022, 0.01)
        sp.location = (-0.2, 0.2, 0.5)
        sp.rotation_euler = (0, 0, math.radians(ang))
    render("gem_" + color)


TILE_SYMBOL = {"red": "heart", "green": "triangle", "cyan": "cross", "yellow": "star", "purple": "circle", "blue": "diamond"}
SHAPES = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", "third_party", "phosphor", "shapes"))


def emboss_symbol(name, mat, size=0.95, z=0.3, depth=0.07):
    """Import a Phosphor (MIT) shape and extrude it into a beveled relief."""
    path = os.path.join(SHAPES, name + ".svg")
    if not os.path.exists(path):
        return
    try:
        bpy.ops.preferences.addon_enable(module="io_curve_svg")
    except Exception:
        pass
    before = set(bpy.data.objects)
    bpy.ops.import_curve.svg(filepath=path)
    new = [o for o in bpy.data.objects if o not in before]
    from mathutils import Vector
    lo, hi = Vector((1e9, 1e9, 0)), Vector((-1e9, -1e9, 0))
    for o in new:
        for v in o.bound_box:
            w = o.matrix_world @ Vector(v)
            lo = Vector((min(lo.x, w.x), min(lo.y, w.y), 0))
            hi = Vector((max(hi.x, w.x), max(hi.y, w.y), 0))
    sz = max(hi.x - lo.x, hi.y - lo.y, 1e-6)
    k = size / sz
    cx, cy = (lo.x + hi.x) / 2, (lo.y + hi.y) / 2
    for o in new:
        o.data.extrude = depth / k / 2
        o.data.bevel_depth = depth / k / 4
        o.data.bevel_resolution = 3
        o.data.fill_mode = "BOTH"
        o.data.materials.clear()
        o.data.materials.append(mat)
        o.location = ((o.location.x - cx) * k, (o.location.y - cy) * k, z)
        o.scale = (o.scale.x * k, o.scale.y * k, o.scale.z * k)


def tile(color):
    """Panel charm tile: a rounded slab with an inset face and an embossed symbol."""
    reset()
    base = PALETTE[color]
    o = link(cube("tile", 1.84, 1.84, 0.5), T.toon("tile_" + color, base, spec=0.55, rim=0.35, threshold=0.3))
    bevel(o, 0.24, 6)
    inset = link(cube("inset", 1.36, 1.36, 0.08), T.toon("inset_" + color, base, shadow=(0.7, 0.72, 0.9), spec=0.3, rim=0.2))
    inset.location = (0, 0, 0.24)
    bevel(inset, 0.22, 5)
    lighter = 0xFFFFFF if color != "yellow" else 0xFFF6D0
    emboss_symbol(TILE_SYMBOL.get(color, "circle"), T.toon("sym_" + color, lighter, shadow=(0.78, 0.8, 0.95), spec=0.7, spec_size=0.5, rim=0.3), size=0.9, z=0.3)
    render("tile_" + color)


def stone():
    reset()
    m = material("stone", 0x6E7489, rough=0.8, coat=0.05)
    o = link(cube("stone", 1.9, 1.9, 0.6), m)
    bevel(o, 0.18)
    render("stone")


def star_mesh(bm, r_out, r_in, depth):
    verts = []
    for i in range(10):
        a = math.pi / 2 + i * math.pi / 5
        r = r_out if i % 2 == 0 else r_in
        verts.append(bm.verts.new((math.cos(a) * r, math.sin(a) * r, 0)))
    face = bm.faces.new(verts)
    ext = bmesh.ops.extrude_face_region(bm, geom=[face])
    vs = [e for e in ext["geom"] if isinstance(e, bmesh.types.BMVert)]
    bmesh.ops.translate(bm, vec=(0, 0, depth), verts=vs)


def host():
    """Nova: an original round comet-sprite. Body only; the engine draws the face per pose."""
    reset()
    bpy.context.scene.render.resolution_x = 256
    bpy.context.scene.render.resolution_y = 256
    bpy.context.scene.camera.data.ortho_scale = 2.6
    body = link(sphere("body", 0.62), material("nova", 0xFFE08A, rough=0.3, coat=0.6, sss=0.35, emit=0.15))
    body.location = (0.15, -0.1, 0)
    for i in range(5):
        t = link(sphere("tail", 0.42 - i * 0.07, 32, 16), material("tail%d" % i, 0xFFB86B, rough=0.4, emit=1.2, alpha=0.55 - i * 0.08))
        t.location = (-0.4 - i * 0.2, -0.28 - i * 0.05, -0.3)
    ant = link(cylinder("antenna", 0.035, 0.45, 16), material("ant", 0xE89A3C, rough=0.4))
    ant.rotation_euler = (math.radians(-90), math.radians(20), 0)
    ant.location = (0.3, 0.68, 0.1)
    st = link(mesh_obj("star", lambda bm: star_mesh(bm, 0.2, 0.09, 0.08)), material("star", 0xFFF4B0, rough=0.2, emit=2.0))
    st.location = (0.38, 0.92, 0.1)
    render("host")


def main():
    only = sys.argv[sys.argv.index('--') + 2] if len(sys.argv) > sys.argv.index('--') + 2 else None
    if only == 'gem':
        for c in ('red', 'orange', 'yellow', 'green', 'cyan', 'blue', 'purple', 'gray'):
            gem(c)
        return
    for c in ("red", "orange", "yellow", "green", "cyan", "blue", "purple", "gray"):
        gem(c)
    for c in ("red", "green", "blue", "yellow", "purple", "white"):
        blob(c)
    for c in ("yellow", "red", "blue"):
        capsule_half(c)
        virus(c, 0)
        virus(c, 1)
    for c in ("red", "green", "cyan", "yellow", "purple", "blue"):
        tile(c)
    stone()
    host()


main()
