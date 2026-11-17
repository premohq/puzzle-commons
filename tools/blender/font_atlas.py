"""Render an anti-aliased glyph atlas (ASCII 32..126) for each UI weight, plus metrics.
Fonts: Lato (SIL Open Font License 1.1, see assets/fonts/LICENSE-OFL.txt) for body text,
M PLUS Rounded 1c Black (SIL OFL 1.1, see assets/fonts/LICENSE-OFL-MPLUSRounded1c.txt) for
display headings.

  blender -b --factory-startup --python tools/blender/font_atlas.py -- <fonts dir> <out dir> [weight ...]
"""
import bpy
import os
import sys

args = sys.argv[sys.argv.index("--") + 1:]
FONTS, OUT = os.path.abspath(args[0]), os.path.abspath(args[1])
os.makedirs(OUT, exist_ok=True)
CELL = 1.25
PPU = 64
BASE = 0.32
COLS, ROWS = 16, 6
WEIGHTS = [("regular", "Lato-Medium.ttf"), ("bold", "Lato-Bold.ttf"), ("black", "Lato-Black.ttf"), ("display", "MPLUSRounded1c-Black.ttf")]
if len(args) > 2:
    WEIGHTS = [w for w in WEIGHTS if w[0] in args[2:]]


def scene():
    bpy.ops.wm.read_factory_settings(use_empty=True)
    sc = bpy.context.scene
    sc.render.engine = "CYCLES"
    sc.cycles.device = "CPU"
    sc.cycles.samples = 8
    try:
        sc.cycles.use_denoising = False
    except Exception:
        pass
    sc.render.film_transparent = True
    sc.render.resolution_x = int(COLS * CELL * PPU)
    sc.render.resolution_y = int(ROWS * CELL * PPU)
    sc.render.image_settings.file_format = "TARGA_RAW"
    sc.render.image_settings.color_mode = "RGBA"
    sc.view_settings.view_transform = "Standard"
    cd = bpy.data.cameras.new("cam")
    cd.type = "ORTHO"
    cd.ortho_scale = COLS * CELL
    cam = bpy.data.objects.new("cam", cd)
    cam.location = (COLS * CELL / 2, -ROWS * CELL / 2, 10)
    sc.collection.objects.link(cam)
    sc.camera = cam
    m = bpy.data.materials.new("ink")
    m.use_nodes = True
    nt = m.node_tree
    for n in list(nt.nodes):
        nt.nodes.remove(n)
    em = nt.nodes.new("ShaderNodeEmission")
    em.inputs[0].default_value = (1, 1, 1, 1)
    em.inputs[1].default_value = 1.0
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    nt.links.new(em.outputs[0], out.inputs[0])
    return sc, m


for weight, fname in WEIGHTS:
    sc, mat = scene()
    font = bpy.data.fonts.load(os.path.join(FONTS, fname))
    lines = []
    objs = []
    for k in range(95):
        ch = chr(32 + k)
        col, row = k % COLS, k // COLS
        cu = bpy.data.curves.new("g%d" % k, type="FONT")
        cu.body = ch if ch != " " else "."
        cu.font = font
        cu.size = 1.0
        ob = bpy.data.objects.new("g%d" % k, cu)
        ob.location = (col * CELL + 0.1, -(row + 1) * CELL + BASE, 0)
        cu.materials.append(mat)
        sc.collection.objects.link(ob)
        if ch == " ":
            ob.hide_render = True
        objs.append((k, ob))
    bpy.context.view_layer.update()
    dg = bpy.context.evaluated_depsgraph_get()
    for k, ob in objs:
        ev = ob.evaluated_get(dg)
        bb = [v[:] for v in ev.bound_box]
        xs = [v[0] for v in bb]
        if 32 + k == 32:
            lines.append("%d 0 0.26" % (32 + k))
        else:
            lines.append("%d %.4f %.4f" % (32 + k, min(xs), max(xs)))
    sc.render.filepath = os.path.join(OUT, "font_%s.tga" % weight)
    bpy.ops.render.render(write_still=True)
    with open(os.path.join(OUT, "font_%s.txt" % weight), "w") as f:
        f.write("cell %.3f ppu %d base %.3f origin 0.1\n" % (CELL, PPU, BASE))
        f.write("\n".join(lines) + "\n")
    print("atlas", weight)
