"""Puzzle Commons 3D cast: anime characters (pixiv's CC0 VRoid sample avatars, VRM = glTF 2.0)
and cute creatures (Quaternius, CC0), toon shaded with an ink outline, posed and rendered to
sprites for the game.

  blender -b --factory-startup --python tools/blender/cast3d.py -- <repo dir> <mode> [ids...]

modes: preview   one neutral full-body render per source model -> third_party/.../preview
       cast      every cast member x expression -> assets/cast/<id>_<expr>.tga + assets/cast/cast.txt
"""
import bpy
import json
import math
import os
import shutil
import struct
import sys
from mathutils import Matrix, Vector

ARGS = sys.argv[sys.argv.index("--") + 1:]
ROOT = ARGS[0]
MODE = ARGS[1] if len(ARGS) > 1 else "preview"
ONLY = set(ARGS[2:])
sys.path.insert(0, os.path.join(ROOT, "tools", "blender"))
import toon_lib as T  # noqa: E402

VRM_DIR = os.path.join(ROOT, "third_party", "vroid_cc0", "vrm")
MON_DIR = os.path.join(ROOT, "third_party", "quaternius_cute_monsters")
WORK = os.path.join(ROOT, "third_party", "vroid_cc0", "work")
os.makedirs(WORK, exist_ok=True)


# ------------------------------------------------------------------ VRM helpers
def vrm_json(path):
    b = open(path, "rb").read()
    ln = struct.unpack("<I", b[12:16])[0]
    return json.loads(b[20:20 + ln])


def import_vrm(path):
    """Import a VRM 0.x file through Blender's glTF importer; returns (armature, meshes, vrm ext, gltf json)."""
    glb = os.path.join(WORK, os.path.basename(path)[:-4] + ".glb")
    if not os.path.exists(glb):
        shutil.copyfile(path, glb)
    before = set(bpy.data.objects)
    bpy.ops.import_scene.gltf(filepath=glb, import_shading="NORMALS")
    new = [o for o in bpy.data.objects if o not in before]
    arm = next((o for o in new if o.type == "ARMATURE"), None)
    meshes = [o for o in new if o.type == "MESH"]
    j = vrm_json(path)
    return arm, meshes, j.get("extensions", {}).get("VRM", {}), j


def bone_map(vrm, gltf):
    nodes = gltf["nodes"]
    return {hb["bone"]: nodes[hb["node"]]["name"] for hb in vrm.get("humanoid", {}).get("humanBones", []) if hb.get("node", -1) >= 0}


def set_expression(vrm, gltf, meshes, weights):
    """weights: {preset: 0..1}. VRM blend shape groups bind (mesh index, morph index, weight 0..100)."""
    # reset every shape key
    for m in meshes:
        if m.data.shape_keys:
            for kb in m.data.shape_keys.key_blocks[1:]:
                kb.value = 0.0
    gmeshes = gltf.get("meshes", [])
    by_name = {}
    for m in meshes:
        by_name.setdefault(m.data.name, []).append(m)
        by_name.setdefault(m.name, []).append(m)
    for g in vrm.get("blendShapeMaster", {}).get("blendShapeGroups", []):
        preset = g.get("presetName") or g.get("name", "").lower()
        w = weights.get(preset, 0.0)
        if w <= 0:
            continue
        for bind in g.get("binds", []):
            mi, ti, bw = bind.get("mesh"), bind.get("index"), bind.get("weight", 100) / 100.0
            if mi is None or mi >= len(gmeshes):
                continue
            gm = gmeshes[mi]
            names = gm.get("extras", {}).get("targetNames") or gm["primitives"][0].get("extras", {}).get("targetNames")
            key_name = names[ti] if names and ti < len(names) else None
            for o in meshes:
                if not o.data.shape_keys:
                    continue
                kbs = o.data.shape_keys.key_blocks
                kb = kbs.get(key_name) if key_name else None
                if kb is None and ti + 1 < len(kbs) and (gm.get("name") in (o.data.name, o.name)):
                    kb = kbs[ti + 1]
                if kb is not None:
                    kb.value = min(1.0, kb.value + w * bw)


def rotate_bone(arm, bname, axis, deg):
    """Rotate a pose bone about its head, in armature space."""
    if bname not in arm.pose.bones:
        return
    pb = arm.pose.bones[bname]
    bpy.context.view_layer.update()
    head = pb.head.copy()
    R = Matrix.Rotation(math.radians(deg), 4, axis)
    pb.matrix = Matrix.Translation(head) @ R @ Matrix.Translation(-head) @ pb.matrix
    bpy.context.view_layer.update()


# poses: (humanoid bone, axis, degrees) in armature space. Character faces -Y; +X is their left.
POSES = {
    "stand": [("leftUpperArm", "Y", 70), ("rightUpperArm", "Y", -70), ("leftUpperArm", "X", -6), ("rightUpperArm", "X", -6), ("leftLowerArm", "Z", 16), ("rightLowerArm", "Z", -16), ("leftHand", "Z", 40), ("rightHand", "Z", -40)],
    "cheer": [("leftUpperArm", "Y", -50), ("rightUpperArm", "Y", 50), ("leftLowerArm", "Y", -26), ("rightLowerArm", "Y", 26), ("head", "X", -6)],
    "sad": [("leftUpperArm", "Y", 76), ("rightUpperArm", "Y", -76), ("leftUpperArm", "X", -14), ("rightUpperArm", "X", -14), ("leftLowerArm", "Z", 34), ("rightLowerArm", "Z", -34), ("head", "X", 14), ("spine", "X", 5)],
    "angry": [("leftUpperArm", "Y", 74), ("rightUpperArm", "Y", -74), ("leftUpperArm", "X", -12), ("rightUpperArm", "X", -12), ("head", "X", 7), ("spine", "X", 4)],
    "think": [("leftUpperArm", "Y", 68), ("rightUpperArm", "Y", -40), ("rightUpperArm", "Z", 30), ("rightLowerArm", "Z", -110), ("head", "Z", -8), ("head", "Y", 6)],
    "wave": [("leftUpperArm", "Y", 68), ("rightUpperArm", "Y", -10), ("rightLowerArm", "Y", 75), ("head", "Y", -5)],
}


def rotate_bone_axis(arm, bname, axis, deg):
    """Rotate a pose bone about its head around an armature-space axis vector."""
    if bname not in arm.pose.bones or axis.length < 1e-6:
        return
    pb = arm.pose.bones[bname]
    bpy.context.view_layer.update()
    head = pb.head.copy()
    R = Matrix.Rotation(math.radians(deg), 4, axis.normalized())
    pb.matrix = Matrix.Translation(head) @ R @ Matrix.Translation(-head) @ pb.matrix
    bpy.context.view_layer.update()


# relaxed hand: degrees of curl per segment (proximal, intermediate, distal)
FINGER_CURL = {"Index": (26, 40, 26), "Middle": (32, 46, 28), "Ring": (36, 50, 30), "Little": (42, 54, 32), "Thumb": (12, 20, 14)}


def relax_hands(arm, bones, fist=0.0):
    """Curl every finger toward the palm (the T-pose leaves them straight and spread, which
    reads as thin claws once the arms come down). `fist` adds extra curl (0..1)."""
    for side in ("left", "right"):
        for finger, curls in FINGER_CURL.items():
            for seg, deg in zip(("Proximal", "Intermediate", "Distal"), curls):
                hb = side + finger + seg
                if hb not in bones or bones[hb] not in arm.pose.bones:
                    continue
                pb = arm.pose.bones[bones[hb]]
                d = (pb.tail - pb.head)
                if d.length < 1e-6:
                    continue
                d = d.normalized()
                if finger == "Thumb":
                    # thumbs fold in across the palm
                    axis = d.cross(Vector((0, 0, 1)))
                else:
                    # fingers bend toward the palm (down, in the T-pose)
                    axis = d.cross(Vector((0, 0, -1)))
                rotate_bone_axis(arm, bones[hb], axis, deg * (1.0 + 1.6 * fist))
        # close the spread a little: fingers drift toward the middle finger
        for finger, ang in (("Index", 4), ("Ring", -4), ("Little", -8)):
            hb = side + finger + "Proximal"
            if hb in bones and bones[hb] in arm.pose.bones:
                rotate_bone_axis(arm, bones[hb], Vector((0, 0, 1)), ang if side == "left" else -ang)


def posed_bounds(meshes):
    """World bounds of the meshes as deformed by the current pose (not the rest pose)."""
    dg = bpy.context.evaluated_depsgraph_get()
    lo = Vector((1e9, 1e9, 1e9))
    hi = Vector((-1e9, -1e9, -1e9))
    for o in meshes:
        if o.type != "MESH" or len(o.data.vertices) < 200 or o.name.lower().startswith("icosphere"):
            continue
        eo = o.evaluated_get(dg)
        me = eo.to_mesh()
        mw = eo.matrix_world
        for v in me.vertices:
            p = mw @ v.co
            lo = Vector((min(lo.x, p.x), min(lo.y, p.y), min(lo.z, p.z)))
            hi = Vector((max(hi.x, p.x), max(hi.y, p.y), max(hi.z, p.z)))
        eo.to_mesh_clear()
    return lo, hi


def pose(arm, bones, name):
    # VRM 0.x models face +Y once imported; the poses are written for a model facing -Y, which
    # mirrors rotations about X and Y. The armature object is then turned to face the camera.
    for hb, ax, deg in POSES.get(name, []):
        if hb in bones:
            rotate_bone(arm, bones[hb], ax, -deg if ax in "XY" else deg)


def outline(meshes, color=0x2A1E38, width=0.0035):
    """Anime ink line: inverted hull (solidify with flipped normals, back faces only)."""
    mat = bpy.data.materials.get("ink") or bpy.data.materials.new("ink")
    mat.use_nodes = True
    nt = mat.node_tree
    for n in list(nt.nodes):
        nt.nodes.remove(n)
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    em = nt.nodes.new("ShaderNodeEmission")
    em.inputs[0].default_value = T.lin(color)
    em.inputs[1].default_value = 1.0
    nt.links.new(em.outputs[0], out.inputs[0])
    mat.use_backface_culling = True
    for m in meshes:
        if len(m.data.vertices) < 50:
            continue
        m.data.materials.append(mat)
        mod = m.modifiers.new("ink", "SOLIDIFY")
        mod.thickness = width
        mod.offset = 1.0
        mod.use_flip_normals = True
        mod.use_rim = False
        mod.material_offset = len(m.data.materials) - 1


def first_image(mat):
    if mat is None or mat.node_tree is None:
        return None
    for n in mat.node_tree.nodes:
        if n.bl_idname == "ShaderNodeTexImage" and n.image is not None:
            return n.image
    return None


def toon_meshes(meshes, face_soft=True, tints=None):
    """Cel shade with each material's own texture. Eye and face-detail layers (iris, highlight,
    brows, lashes, mouth) stay unlit, as anime faces are drawn."""
    done = {}
    for m in meshes:
        for slot in m.material_slots:
            mat = slot.material
            if mat is None or mat.name == "ink":
                continue
            low = mat.name.lower()
            if low.endswith("_eye") or low.endswith("_face"):
                continue
            if mat.name in done:
                slot.material = done[mat.name]
                continue
            img = first_image(mat)
            blend = getattr(mat, "blend_method", "OPAQUE")
            cut = blend in ("BLEND", "HASHED", "CLIP")
            skin = any(k in low for k in ("skin", "body"))
            holder = {}

            tint = None
            for key, col in (tints or {}).items():
                if key.lower() in low:
                    tint = col

            def base_fn(nt, img=img, holder=holder, tint=tint):
                if img is None:
                    rgb = nt.nodes.new("ShaderNodeRGB")
                    rgb.outputs[0].default_value = (0.85, 0.85, 0.85, 1)
                    return rgb.outputs[0]
                t = nt.nodes.new("ShaderNodeTexImage")
                t.image = img
                holder["t"] = t
                out = t.outputs["Color"]
                if tint is not None:
                    # recolor from the texture's brightness: tint x (0.5 + 1.1 x luminance), so dark
                    # and pale hair alike take the new color and keep their strands and shine
                    bw = nt.nodes.new("ShaderNodeRGBToBW")
                    nt.links.new(out, bw.inputs[0])
                    ma = nt.nodes.new("ShaderNodeMath")
                    ma.operation = "MULTIPLY_ADD"
                    nt.links.new(bw.outputs[0], ma.inputs[0])
                    ma.inputs[1].default_value = 1.1
                    ma.inputs[2].default_value = 0.5
                    mul = nt.nodes.new("ShaderNodeVectorMath")
                    mul.operation = "SCALE"
                    mul.inputs[0].default_value = T.lin(tint)[:3]
                    nt.links.new(ma.outputs[0], mul.inputs[3])
                    out = mul.outputs[0]
                return out

            alpha = (lambda nt, holder=holder: holder["t"].outputs["Alpha"]) if (img is not None and cut) else None
            shadow = (0.93, 0.86, 0.90) if (skin and face_soft) else (0.70, 0.70, 0.88)
            new = T.toon(mat.name + "_toon", 0, shadow=shadow, base_socket_fn=base_fn, spec=0.10 if not skin else 0.0, rim=0.25,
                         alpha=alpha, alpha_mode="DITHERED", ao=0.0, threshold=0.35)
            done[mat.name] = new
            slot.material = new


def frame_camera(sc, objs, mode="full", w=512, h=768, front=1.0, yaw=0.0):
    """Orthographic camera on the side the model faces (+Y for VRM, -Y for the creatures),
    turned `yaw` degrees around the model for a three-quarter view."""
    pts = []
    for o in objs:
        # skip helper shapes (the glTF importer's bone-display icosphere and the like)
        if o.type != "MESH" or len(o.data.vertices) < 200 or o.name.lower().startswith("icosphere"):
            continue
        for c in o.bound_box:
            pts.append(o.matrix_world @ Vector(c))
    xs = [p.x for p in pts]
    zs = [p.z for p in pts]
    top, bot = max(zs), min(zs)
    cx = (max(xs) + min(xs)) / 2
    hgt = top - bot
    cam_d = bpy.data.cameras.new("cam")
    cam_d.type = "ORTHO"
    if mode == "full":
        cam_d.ortho_scale = hgt * 1.06 * max(1.0, w / h)
        cz = bot + hgt * 0.5
    else:  # bust: head and shoulders
        cam_d.ortho_scale = hgt * 0.30
        cz = top - hgt * 0.15
    ys = [p.y for p in pts]
    cy = (max(ys) + min(ys)) / 2
    cam = bpy.data.objects.new("cam", cam_d)
    a = math.radians(yaw)
    # direction from the model to the camera
    dx, dy = math.sin(a) * front, front * math.cos(a)
    cam.location = (cx + dx * 10, cy + dy * 10, cz)
    cam.rotation_euler = (math.radians(90), 0, math.atan2(dx, -dy))
    sc.collection.objects.link(cam)
    sc.camera = cam
    return cam


def frame_bounds(sc, lo, hi, w, h, front=1.0, yaw=0.0):
    """Orthographic full-body camera fitted to world bounds (height-limited, small margin)."""
    cam_d = bpy.data.cameras.new("camf")
    cam_d.type = "ORTHO"
    hgt = hi.z - lo.z
    wid = max(hi.x - lo.x, hi.y - lo.y)
    # fit height, or width if the pose is wider than the picture's shape
    cam_d.ortho_scale = max(hgt * 1.04, wid * 1.04 * h / w)
    cx, cy, cz = (lo.x + hi.x) / 2, (lo.y + hi.y) / 2, (lo.z + hi.z) / 2
    cam = bpy.data.objects.new("camf", cam_d)
    a = math.radians(yaw)
    dx, dy = math.sin(a) * front, front * math.cos(a)
    cam.location = (cx + dx * 10, cy + dy * 10, cz)
    cam.rotation_euler = (math.radians(90), 0, math.atan2(dx, -dy))
    sc.collection.objects.link(cam)
    return cam


def lights(sc):
    T.sun(sc, rot_deg=(62, 0, -28), energy=3.4, color=0xFFF6EA, angle=6)
    # a soft fill from the other side so faces never go dark
    fill = bpy.data.lights.new("fill", "SUN")
    fill.energy = 1.1
    fill.color = (0.78, 0.85, 1.0)
    fo = bpy.data.objects.new("fill", fill)
    fo.rotation_euler = (math.radians(70), 0, math.radians(40))
    sc.collection.objects.link(fo)


def render(sc, path):
    sc.render.filepath = path
    bpy.ops.render.render(write_still=True)


# ------------------------------------------------------------------ creatures
def import_monster(name):
    path = os.path.join(MON_DIR, "blends", name + ".blend")
    # load as a data library (never opened as a file, so nothing inside it runs)
    with bpy.data.libraries.load(path, link=False) as (src, dst):
        dst.objects = list(src.objects)
    objs = [o for o in dst.objects if o is not None]
    for o in objs:
        bpy.context.scene.collection.objects.link(o)
    # cleaned copies (softened grain, richer color: see the cast step in README)
    tex = os.path.join(MON_DIR, "textures_clean", name + "_Texture.png")
    if not os.path.exists(tex):
        tex = os.path.join(MON_DIR, "textures", name + "_Texture.png")
    img = bpy.data.images.load(tex, check_existing=True) if os.path.exists(tex) else None
    for o in objs:
        if o.type == "MESH":
            for slot in o.material_slots:
                m = slot.material
                if m and m.node_tree and img is not None:
                    for n in m.node_tree.nodes:
                        if n.bl_idname == "ShaderNodeTexImage":
                            n.image = img
    return objs


# ------------------------------------------------------------------ modes
def preview():
    out = os.path.join(ROOT, "third_party", "vroid_cc0", "preview")
    os.makedirs(out, exist_ok=True)
    for f in sorted(os.listdir(VRM_DIR)):
        if not f.endswith(".vrm") or (ONLY and f[:-4] not in ONLY):
            continue
        sc = T.eevee_scene(512, 768, transparent=True, samples=32)
        sc.render.image_settings.file_format = "PNG"
        lights(sc)
        arm, meshes, vrm, gltf = import_vrm(os.path.join(VRM_DIR, f))
        bones = bone_map(vrm, gltf)
        pose(arm, bones, "stand")
        set_expression(vrm, gltf, meshes, {"joy": 1.0})
        toon_meshes(meshes)
        outline(meshes)
        frame_camera(sc, meshes, front=1.0, yaw=14)
        render(sc, os.path.join(out, f[:-4] + ".png"))
    mout = os.path.join(ROOT, "third_party", "quaternius_cute_monsters", "preview")
    os.makedirs(mout, exist_ok=True)
    for f in sorted(os.listdir(os.path.join(MON_DIR, "blends"))):
        name = f[:-6]
        if ONLY and name not in ONLY:
            continue
        sc = T.eevee_scene(512, 512, transparent=True, samples=32)
        sc.render.image_settings.file_format = "PNG"
        lights(sc)
        objs = import_monster(name)
        meshes = [o for o in objs if o.type == "MESH"]
        toon_meshes(meshes, face_soft=False)
        outline(meshes, width=0.01)
        frame_camera(sc, meshes, w=512, h=512, front=-1.0, yaw=-24)
        render(sc, os.path.join(mout, name + ".png"))


# ------------------------------------------------------------------ the cast
# id, source model, tints by material keyword, role (only for reading this table)
HUMANS = [
    ("mira", "hairsample_female", {"HAIR": 0xFFA0CF}, "host and hero"),
    ("kai", "hairsample_male", {}, "Mira's friend and sparring rival"),
    ("nami", "avatarsample_f", {}, "rival of world 1, Quad Reef"),
    ("hana", "avatarsample_e", {}, "rival of world 2, Bloom Valley"),
    ("yuki", "avatarsample_g", {}, "rival of world 3, Clinic Moon"),
    ("rin", "sendagaya_shino", {"HAIR": 0xF2B640}, "rival of world 4, Tile Bazaar"),
    ("kaede", "avatarsample_d_darkness", {"HAIR": 0xD8344A}, "queen of world 5, Cinder Crown"),
    ("sora", "avatarsample_d_0", {"HAIR": 0x6EC8FF}, "versus regular"),
    ("ren", "sakurada_fumiriya", {"HAIR": 0x8A5634}, "versus regular"),
    ("taro", "base_male", {"HAIR": 0x2E6E58}, "versus regular"),
    ("emi", "base_female", {"HAIR": 0xFF9A3C}, "versus regular"),
]
# The creatures were tried and left out of the game (the anime cast reads better together);
# render them with `cast3d.py <repo> creatures` if they are wanted again.
CREATURES_ALL = [
    ("boo", "Ghost"),
    ("pip", "Bat"),
    ("bumble", "Bee"),
    ("pinch", "Crab"),
    ("morel", "Mushroom"),
    ("ember", "YellowDragon"),
    ("spike", "Cactus"),
    ("waddle", "Penguin"),
    ("zib", "Alien"),
    ("squish", "Cthulhu"),
]
CREATURES = CREATURES_ALL if MODE == "creatures" else []
# expression: (VRM blend shape weights, pose)
EXPRS = {
    "normal": ({"neutral": 1.0}, "stand"),
    "smile": ({"fun": 1.0}, "stand"),
    "delighted": ({"joy": 1.0}, "cheer"),
    "angry": ({"angry": 1.0}, "angry"),
    "sad": ({"sorrow": 1.0}, "sad"),
    "shocked": ({"o": 1.0, "sorrow": 0.25}, "stand"),
    "smug": ({"fun": 0.55, "blink_l": 0.85}, "think"),
    "sleepy": ({"blink": 0.72, "neutral": 0.5}, "sad"),
}
# creature expression: (action name prefix, frame)
CREATURE_EXPRS = {"smile": ("Idle", 20), "delighted": ("Jump", 6), "sad": ("HitRecieve", 5), "angry": ("Bite_InPlace", 8), "shocked": ("No", 5)}

OUT = os.path.join(ROOT, "assets", "cast")


def reset_pose(arm):
    for pb in arm.pose.bones:
        pb.matrix_basis = Matrix.Identity(4)
    bpy.context.view_layer.update()


def cast():
    os.makedirs(OUT, exist_ok=True)
    for cid, src, tints, _role in HUMANS:
        if ONLY and cid not in ONLY:
            continue
        sc = T.eevee_scene(448, 960, transparent=True, samples=48)
        sc.render.image_settings.file_format = "PNG"
        sc.render.image_settings.color_mode = "RGBA"
        sc.render.image_settings.compression = 90
        lights(sc)
        arm, meshes, vrm, gltf = import_vrm(os.path.join(VRM_DIR, src + ".vrm"))
        bones = bone_map(vrm, gltf)
        toon_meshes(meshes, tints=tints)
        outline(meshes)
        # one framing for all eight poses (so sizes match between expressions), measured on
        # the posed meshes so raised hands stay inside the picture
        lo = Vector((1e9, 1e9, 1e9))
        hi = Vector((-1e9, -1e9, -1e9))
        for ex, (weights, pz) in EXPRS.items():
            reset_pose(arm)
            relax_hands(arm, bones, 0.6 if pz == "angry" else 0.0)
            pose(arm, bones, pz)
            a, b = posed_bounds(meshes)
            lo = Vector((min(lo.x, a.x), min(lo.y, a.y), min(lo.z, a.z)))
            hi = Vector((max(hi.x, b.x), max(hi.y, b.y), max(hi.z, b.z)))
        full = frame_bounds(sc, lo, hi, 448, 960, front=1.0, yaw=14)
        reset_pose(arm)
        bust = frame_camera(sc, meshes, "bust", 640, 640, front=1.0, yaw=10)
        for ex, (weights, pz) in EXPRS.items():
            reset_pose(arm)
            relax_hands(arm, bones, 0.6 if pz == "angry" else 0.0)
            pose(arm, bones, pz)
            set_expression(vrm, gltf, meshes, weights)
            sc.camera = full
            sc.render.resolution_x, sc.render.resolution_y = 448, 960
            render(sc, os.path.join(OUT, "%s_%s_full.png" % (cid, ex)))
            sc.camera = bust
            sc.render.resolution_x, sc.render.resolution_y = 640, 640
            render(sc, os.path.join(OUT, "%s_%s_bust.png" % (cid, ex)))
    for cid, name in CREATURES:
        if ONLY and cid not in ONLY:
            continue
        sc = T.eevee_scene(512, 512, transparent=True, samples=48)
        sc.render.image_settings.file_format = "PNG"
        sc.render.image_settings.color_mode = "RGBA"
        sc.render.image_settings.compression = 90
        lights(sc)
        objs = import_monster(name)
        meshes = [o for o in objs if o.type == "MESH"]
        arm = next((o for o in objs if o.type == "ARMATURE"), None)
        toon_meshes(meshes, face_soft=False)
        outline(meshes, width=0.01)
        cam = frame_camera(sc, meshes, w=512, h=512, front=-1.0, yaw=-24)
        cam.data.ortho_scale *= 1.6
        cam.location.z += cam.data.ortho_scale * 0.05
        acts = []
        with bpy.data.libraries.load(os.path.join(MON_DIR, "blends", name + ".blend"), link=False) as (s_, d_):
            d_.actions = list(s_.actions)
        acts = [a for a in d_.actions if a is not None]
        for ex, (prefix, frame) in CREATURE_EXPRS.items():
            act = next((a for a in acts if a.name.startswith(prefix)), None) or next((a for a in acts if a.name.startswith("Idle")), None) or (acts[0] if acts else None)
            if arm is not None and act is not None:
                if arm.animation_data is None:
                    arm.animation_data_create()
                arm.animation_data.action = act
                try:
                    slots = getattr(act, "slots", None)
                    if slots:
                        arm.animation_data.action_slot = slots[0]
                except Exception:
                    pass
                sc.frame_set(int(min(frame, act.frame_range[1])))
            render(sc, os.path.join(OUT, "%s_%s.png" % (cid, ex)))
    # the roster the game reads
    with open(os.path.join(OUT, "cast.txt"), "w") as f:
        for cid, src, _t, role in HUMANS:
            f.write("human %s %s\n" % (cid, src))
        for cid, name in CREATURES:
            f.write("creature %s %s\n" % (cid, name))


if MODE == "preview":
    preview()
elif MODE in ("cast", "creatures"):
    cast()
