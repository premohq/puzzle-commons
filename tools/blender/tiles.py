"""Letter tiles in the logo's style (the "PUZZLE" row of logo.py): a colored rounded tile with
an ink border, a drop shadow, a white shine bar and a white letter outlined in ink. Every
character in every tile color, in one atlas the game spells titles with (ui::block_title).
Font: Mochiy Pop One (SIL OFL 1.1).

  blender -b --factory-startup --python tools/blender/tiles.py -- <fonts dir> <out dir>
"""
import math
import os
import sys

import bpy

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
args = sys.argv[sys.argv.index("--") + 1:]
FONTS, OUT = os.path.abspath(args[0]), os.path.abspath(args[1])
sys.argv = [sys.argv[0], "--", FONTS, OUT]
src = open(os.path.join(HERE, "logo.py")).read()
# reuse logo.py's helpers (flat, rrect, text), not its scene
exec(src[: src.index("bpy.ops.wm.read_factory_settings")])

CHARS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!?&'-.:/+"
COLORS = [0xE8475A, 0xF59A3B, 0xF5D547, 0x5BD07A, 0x47D1E8, 0xA66CF0]
TILTS = [-8, 5, -4, 7, -6, 4]
CELL = 1.6
PX = 144

bpy.ops.wm.read_factory_settings(use_empty=True)
sc = bpy.context.scene
sc.render.engine = "BLENDER_EEVEE"
sc.render.film_transparent = True
sc.render.resolution_x, sc.render.resolution_y = PX * len(CHARS), PX * len(COLORS)
sc.render.image_settings.file_format = "PNG"
sc.render.image_settings.color_mode = "RGBA"
sc.render.image_settings.compression = 100
sc.view_settings.view_transform = "Standard"
try:
    sc.eevee.taa_render_samples = 64
except Exception:
    pass
FONT = bpy.data.fonts.load(os.path.join(FONTS, "MochiyPopOne-Regular.ttf"))
globals()["FONT"] = FONT

W = CELL * len(CHARS)
H = CELL * len(COLORS)
for row, (col, rot) in enumerate(zip(COLORS, TILTS)):
    for k, ch in enumerate(CHARS):
        x = -W / 2 + CELL * (k + 0.5)
        y = H / 2 - CELL * (row + 0.5) + 0.06
        rrect("tshadow", 1.36, 1.36, 0.32, 0x241838, x, y - 0.12, 0.06, rot)
        rrect("touter", 1.36, 1.36, 0.32, INK, x, y, 0.07, rot)
        rrect("tile", 1.18, 1.18, 0.24, col, x, y, 0.08, rot)
        rrect("tshine", 0.7, 0.1, 0.05, 0xFFFFFF, x, y + 0.45, 0.09, rot)
        text(ch, 1.08, 0xFFFFFF, x, y - 0.07, 0.12, rot, outline=(INK, 0.05))

cd = bpy.data.cameras.new("cam")
cd.type = "ORTHO"
cd.ortho_scale = W
cam = bpy.data.objects.new("cam", cd)
cam.location = (0, 0, 10)
sc.collection.objects.link(cam)
sc.camera = cam
sc.render.filepath = os.path.join(OUT, "tiles.png")
bpy.ops.render.render(write_still=True)
with open(os.path.join(OUT, "tiles.txt"), "w") as f:
    f.write("%d %d %s\n" % (PX, len(COLORS), CHARS))
print("tiles done")
