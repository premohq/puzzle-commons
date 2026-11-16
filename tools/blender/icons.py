"""Render the UI icon atlas from Phosphor Icons (MIT, third_party/phosphor/LICENSE).

  blender -b --factory-startup --python tools/blender/icons.py -- <svg dir> <out dir>
"""
import bpy
import os
import sys
from mathutils import Vector

args = sys.argv[sys.argv.index("--") + 1:]
SRC, OUT = os.path.abspath(args[0]), os.path.abspath(args[1])
names = sorted(f[:-4] for f in os.listdir(SRC) if f.endswith(".svg"))
COLS, CELL = 8, 128
rows = (len(names) + COLS - 1) // COLS
bpy.ops.wm.read_factory_settings(use_empty=True)
try:
    bpy.ops.preferences.addon_enable(module="io_curve_svg")
except Exception:
    pass
sc = bpy.context.scene
sc.render.engine = "BLENDER_EEVEE"
sc.render.film_transparent = True
sc.render.resolution_x = COLS * CELL
sc.render.resolution_y = rows * CELL
sc.render.image_settings.file_format = "TARGA_RAW"
sc.render.image_settings.color_mode = "RGBA"
sc.view_settings.view_transform = "Standard"
ink = bpy.data.materials.new("ink")
ink.use_nodes = True
nt = ink.node_tree
for n in list(nt.nodes):
    nt.nodes.remove(n)
em = nt.nodes.new("ShaderNodeEmission")
em.inputs[0].default_value = (1, 1, 1, 1)
out = nt.nodes.new("ShaderNodeOutputMaterial")
nt.links.new(em.outputs[0], out.inputs[0])
for k, nm in enumerate(names):
    before = set(bpy.data.objects)
    bpy.ops.import_curve.svg(filepath=os.path.join(SRC, nm + ".svg"))
    new = [o for o in bpy.data.objects if o not in before]
    lo, hi = Vector((1e9, 1e9, 0)), Vector((-1e9, -1e9, 0))
    for o in new:
        o.data.materials.clear()
        o.data.materials.append(ink)
        o.data.fill_mode = "BOTH"
        for v in o.bound_box:
            w = o.matrix_world @ Vector(v)
            lo = Vector((min(lo.x, w.x), min(lo.y, w.y), 0))
            hi = Vector((max(hi.x, w.x), max(hi.y, w.y), 0))
    size = max(hi.x - lo.x, hi.y - lo.y, 1e-6)
    s = 0.84 / size
    cx, cy = (lo.x + hi.x) / 2, (lo.y + hi.y) / 2
    col, row = k % COLS, k // COLS
    tx, ty = col + 0.5, -(row + 0.5)
    for o in new:
        o.location = ((o.location.x - cx) * s + tx, (o.location.y - cy) * s + ty, 0)
        o.scale = (o.scale.x * s, o.scale.y * s, 1)
cd = bpy.data.cameras.new("cam")
cd.type = "ORTHO"
cd.ortho_scale = COLS
cam = bpy.data.objects.new("cam", cd)
cam.location = (COLS / 2, -rows / 2, 10)
sc.collection.objects.link(cam)
sc.camera = cam
sc.render.filepath = os.path.join(OUT, "icons.tga")
bpy.ops.render.render(write_still=True)
with open(os.path.join(OUT, "icons.txt"), "w") as f:
    f.write("\n".join(names) + "\n")
print("icons", len(names))
