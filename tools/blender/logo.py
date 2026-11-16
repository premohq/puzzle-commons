"""Puzzle Commons logo: a cream bubble with a chunky rounded wordmark, a row of tilted letter
tiles above it, and an "Open source!" pill badge. Font: Mochiy Pop One (SIL OFL 1.1).

  blender -b --factory-startup --python tools/blender/logo.py -- <fonts dir> <out dir>
"""
import bmesh
import bpy
import math
import os
import sys

args = sys.argv[sys.argv.index("--") + 1:]
FONTS, OUT = os.path.abspath(args[0]), os.path.abspath(args[1])
W, H = 1400, 700
INK = 0x3B2A57


def lin(h):
    def f(c):
        c /= 255.0
        return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4
    return (f((h >> 16) & 255), f((h >> 8) & 255), f(h & 255), 1.0)


_M = {}


def flat(h):
    if h in _M:
        return _M[h]
    m = bpy.data.materials.new("c%06x" % h)
    m.use_nodes = True
    nt = m.node_tree
    for n in list(nt.nodes):
        nt.nodes.remove(n)
    e = nt.nodes.new("ShaderNodeEmission")
    e.inputs[0].default_value = lin(h)
    o = nt.nodes.new("ShaderNodeOutputMaterial")
    nt.links.new(e.outputs[0], o.inputs[0])
    _M[h] = m
    return m


def rrect(name, w, h, r, color, x, y, z, rot=0.0):
    me = bpy.data.meshes.new(name)
    bm = bmesh.new()
    pts = []
    r = min(r, w / 2, h / 2)
    for cx, cy, a0 in ((w / 2 - r, h / 2 - r, 0), (-w / 2 + r, h / 2 - r, 90), (-w / 2 + r, -h / 2 + r, 180), (w / 2 - r, -h / 2 + r, 270)):
        for k in range(13):
            a = math.radians(a0 + k * 90 / 12)
            pts.append(bm.verts.new((cx + math.cos(a) * r, cy + math.sin(a) * r, 0)))
    bm.faces.new(pts)
    bm.to_mesh(me)
    bm.free()
    ob = bpy.data.objects.new(name, me)
    ob.location = (x, y, z)
    ob.rotation_euler = (0, 0, math.radians(rot))
    me.materials.append(flat(color))
    bpy.context.scene.collection.objects.link(ob)
    return ob


FONT = None


def text(body, size, color, x, y, z, rot=0.0, outline=None, outline2=None):
    def one(col, off, zz):
        cu = bpy.data.curves.new("t", type="FONT")
        cu.body = body
        cu.font = FONT
        cu.size = size
        cu.align_x = "CENTER"
        cu.align_y = "CENTER"
        cu.offset = off
        ob = bpy.data.objects.new("t", cu)
        ob.location = (x, y, zz)
        ob.rotation_euler = (0, 0, math.radians(rot))
        cu.materials.append(flat(col))
        bpy.context.scene.collection.objects.link(ob)
    if outline2:
        one(outline2[0], outline2[1], z - 0.02)
    if outline:
        one(outline[0], outline[1], z - 0.01)
    one(color, 0.0, z)


bpy.ops.wm.read_factory_settings(use_empty=True)
sc = bpy.context.scene
sc.render.engine = "BLENDER_EEVEE"
sc.render.film_transparent = True
sc.render.resolution_x, sc.render.resolution_y = W, H
sc.render.image_settings.file_format = "TARGA_RAW"
sc.render.image_settings.color_mode = "RGBA"
sc.view_settings.view_transform = "Standard"
try:
    sc.eevee.taa_render_samples = 64
except Exception:
    pass
FONT = bpy.data.fonts.load(os.path.join(FONTS, "MochiyPopOne-Regular.ttf"))

# the bubble: shadow, ink outline, cream fill, top highlight
rrect("shadow", 10.4, 3.2, 1.45, 0x241838, 0.0, -0.9, 0.0)
rrect("outline", 10.4, 3.2, 1.45, INK, 0, -0.7, 0.01)
rrect("fill", 9.8, 2.62, 1.18, 0xFFF6D6, 0, -0.7, 0.02)
rrect("shine", 8.2, 0.26, 0.13, 0xFFFFFF, 0, 0.3, 0.03)
text("Commons", 2.2, INK, 0.0, -0.82, 0.05)

# the tile row: one colored tile per letter, gently tilted
colors = [0xE8475A, 0xF59A3B, 0xF5D547, 0x5BD07A, 0x47D1E8, 0xA66CF0]
for k, ch in enumerate("PUZZLE"):
    x = -4.05 + k * 1.42
    y = 1.32 + (0.08 if k % 2 else -0.04)
    rot = [-8, 5, -4, 7, -6, 4][k]
    rrect("tshadow", 1.36, 1.36, 0.32, 0x241838, x, y - 0.12, 0.06, rot)
    rrect("touter", 1.36, 1.36, 0.32, INK, x, y, 0.07, rot)
    rrect("tile", 1.18, 1.18, 0.24, colors[k], x, y, 0.08, rot)
    rrect("tshine", 0.7, 0.1, 0.05, 0xFFFFFF, x, y + 0.45, 0.09, rot)
    text(ch, 1.08, 0xFFFFFF, x, y - 0.07, 0.12, rot, outline=(INK, 0.05))

# the sticker
# a pill badge (solid shapes, no font-offset outlines, so the edges stay clean)
SX, SY, SR = 3.35, -2.25, -6
rrect("sshadow", 3.3, 0.86, 0.43, 0x241838, SX + 0.05, SY - 0.1, 0.18, SR)
rrect("souter", 3.3, 0.86, 0.43, INK, SX, SY, 0.19, SR)
rrect("sfill", 3.06, 0.64, 0.32, 0xE8475A, SX, SY, 0.20, SR)
rrect("sshine", 2.3, 0.07, 0.035, 0xFFFFFF, SX, SY + 0.2, 0.21, SR)
text("Open source!", 0.42, 0xFFFFFF, SX, SY - 0.02, 0.22, SR)

cd = bpy.data.cameras.new("cam")
cd.type = "ORTHO"
cd.ortho_scale = 13.0
cam = bpy.data.objects.new("cam", cd)
cam.location = (0, 0, 10)
sc.collection.objects.link(cam)
sc.camera = cam
sc.render.filepath = os.path.join(OUT, "logo.tga")
bpy.ops.render.render(write_still=True)
print("logo done")
