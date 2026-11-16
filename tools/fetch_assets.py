"""Fetch the CC0 source assets used to build Puzzle Commons art (Poly Haven, public domain).
Models, textures and skies land in third_party/polyhaven/<id>/. The game ships only the images
that tools/blender/*.py render from them, so these files are build inputs, not runtime data.

  python3 tools/fetch_assets.py
"""
import json
import os
import sys
import urllib.request

UA = {"User-Agent": "PuzzleCommons-asset-fetch/1.0"}
ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "third_party", "polyhaven")
MODELS = ["island_tree_01", "island_tree_02", "shrub_01", "shrub_03", "flower_gazania", "coast_rocks_01", "rock_moss_set_01",
          "moon_rock_01", "moon_rock_03", "moon_rock_05", "boulder_01", "lambis_shell", "treasure_chest", "street_lamp_01",
          "barrel_03", "brass_diya_lantern", "quiver_tree_01", "stone_01"]
TEXTURES = ["aerial_grass_rock", "aerial_beach_01", "aerial_rocks_02", "gray_rocks", "rocky_trail", "sandstone_cracks"]
HDRIS = ["kloofendal_48d_partly_cloudy_puresky", "belfast_sunset_puresky", "moonless_golf"]


def api(path):
    with urllib.request.urlopen(urllib.request.Request("https://api.polyhaven.com/" + path, headers=UA), timeout=60) as r:
        return json.load(r)


def fetch(url, dest):
    if os.path.exists(dest) and os.path.getsize(dest) > 0:
        return
    os.makedirs(os.path.dirname(dest), exist_ok=True)
    with urllib.request.urlopen(urllib.request.Request(url, headers=UA), timeout=300) as r, open(dest + ".part", "wb") as f:
        while True:
            b = r.read(1 << 20)
            if not b:
                break
            f.write(b)
    os.replace(dest + ".part", dest)


def main():
    for m in MODELS:
        g = api("files/" + m)["gltf"]["1k"]["gltf"]
        d = os.path.join(ROOT, m)
        fetch(g["url"], os.path.join(d, m + ".gltf"))
        for rel, v in g.get("include", {}).items():
            fetch(v["url"], os.path.join(d, rel))
        print("model", m, flush=True)
    for t in TEXTURES:
        f = api("files/" + t)
        for key, name in (("Diffuse", "diff"), ("nor_gl", "nor"), ("Rough", "rough")):
            if key in f:
                fetch(f[key]["1k"]["jpg"]["url"], os.path.join(ROOT, t, "%s_%s.jpg" % (t, name)))
        print("texture", t, flush=True)
    for h in HDRIS:
        fetch(api("files/" + h)["hdri"]["2k"]["hdr"]["url"], os.path.join(ROOT, h, h + ".hdr"))
        print("hdri", h, flush=True)
    with open(os.path.join(ROOT, "LICENSE.txt"), "w") as f:
        f.write("All files in this folder come from Poly Haven (https://polyhaven.com) under CC0 1.0 (public domain).\n")
        f.write("Assets: " + ", ".join(MODELS + TEXTURES + HDRIS) + "\n")


if __name__ == "__main__":
    sys.exit(main())
