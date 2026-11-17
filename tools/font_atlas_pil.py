"""Bake a glyph atlas (ASCII 32..126) for a display face, in the same layout as
tools/blender/font_atlas.py: 16 x 6 cells of 1.25 em at 64 px per em, the baseline 0.32 em
above each cell's bottom, the pen 0.1 em in from its left. White ink, coverage in alpha.
Written for the faces that need no Blender pass (Mochiy Pop One, M PLUS Rounded 1c; SIL OFL 1.1).

  python3 tools/font_atlas_pil.py assets/fonts/MochiyPopOne-Regular.ttf assets/sprites display

The metrics file lists each glyph's ink left and its advance minus the engine's tracking, so
font.rs (which spaces glyphs by that number plus TRACK) lays text out with the face's own advances.
"""
import os
import struct
import sys

from PIL import Image, ImageFont

TTF, OUT, NAME = sys.argv[1], sys.argv[2], sys.argv[3]
SS = 4  # supersampling
CELL, PPU, BASE, ORIGIN = 1.25, 64, 0.32, 0.1
TRACK = 0.045
COLS, ROWS = 16, 6
cell = int(CELL * PPU)
font = ImageFont.truetype(TTF, PPU * SS)
big = Image.new("L", (COLS * cell * SS, ROWS * cell * SS), 0)
lines = []
from PIL import ImageDraw

d = ImageDraw.Draw(big)
for k in range(95):
    ch = chr(32 + k)
    col, row = k % COLS, k // COLS
    adv = font.getlength(ch) / (PPU * SS)
    if ch == " ":
        lines.append("32 0 %.4f" % max(adv - TRACK, 0.2))
        continue
    ox = (col * cell + ORIGIN * PPU) * SS
    by = ((row + 1) * cell - BASE * PPU) * SS
    d.text((ox, by), ch, font=font, fill=255, anchor="ls")
    l, _, _, _ = font.getbbox(ch, anchor="ls")
    lines.append("%d %.4f %.4f" % (32 + k, l / (PPU * SS), adv - TRACK))
small = big.resize((COLS * cell, ROWS * cell), Image.Resampling.BOX)
w, h = small.size
a = small.tobytes()
hdr = struct.pack("<BBBHHBHHHHBB", 0, 0, 2, 0, 0, 0, 0, 0, w, h, 32, 0x28)
body = bytearray(w * h * 4)
body[0::4] = b"\xff" * (w * h)
body[1::4] = b"\xff" * (w * h)
body[2::4] = b"\xff" * (w * h)
body[3::4] = a
with open(os.path.join(OUT, "font_%s.tga" % NAME), "wb") as f:
    f.write(hdr + bytes(body))
with open(os.path.join(OUT, "font_%s.txt" % NAME), "w") as f:
    f.write("cell %.3f ppu %d base %.3f origin %.1f\n" % (CELL, PPU, BASE, ORIGIN))
    f.write("\n".join(lines) + "\n")
print("atlas", NAME, w, h)
