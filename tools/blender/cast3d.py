"""Puzzle Commons 3D cast: anime characters (pixiv's CC0 VRoid sample avatars, VRM = glTF 2.0)
and cute creatures (Quaternius, CC0), toon shaded with an ink outline, posed and rendered to
sprites for the game.

  blender -b --factory-startup --python tools/blender/cast3d.py -- <repo dir> <mode> [ids...]

modes: preview   one neutral full-body render per source model -> third_party/.../preview
       passes    what the game uses: every expression and menu pose as UNLIT albedo plus a
                 half-size lighting map (<name>_n.png: normals + material), lit in-engine by
                 light.rs. Filter with ids and expr:<name> (expr:p- = menu poses only).
       cast      old look: toon light baked in (do not use for assets/cast any more)
       poses     menu poses (MENU_POSES), baked light; prefer passes
       posetest  a quick contact sheet of POSE_CANDIDATES on one model -> target/posework/
"""
import bpy
import json
import math
import os
import shutil
import struct
import sys
from mathutils import Matrix, Quaternion, Vector

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


def is_hair(o):
    # VRoid exports the hairstyle as its own "Hair..." object (body materials can mention hair too)
    return o.type == "MESH" and o.name.lower().startswith("hair")


def transplant_hair(arm, bones, meshes, donor):
    """Give a character another model's hairstyle: delete their own hair meshes, import the
    donor, keep only its hair, line its head up with ours, and make the donor's skeleton follow
    our head bone (Child Of), so the hair moves with every pose."""
    for o in list(meshes):
        if is_hair(o):
            meshes.remove(o)
            bpy.data.objects.remove(o, do_unlink=True)
    arm_b, meshes_b, vrm_b, gltf_b = import_vrm(os.path.join(VRM_DIR, donor + ".vrm"))
    bones_b = bone_map(vrm_b, gltf_b)
    keep = []
    for o in meshes_b:
        if is_hair(o):
            keep.append(o)
        else:
            bpy.data.objects.remove(o, do_unlink=True)
    bpy.context.view_layer.update()
    ha = arm.matrix_world @ arm.pose.bones[bones["head"]].head
    hb = arm_b.matrix_world @ arm_b.pose.bones[bones_b["head"]].head
    arm_b.location = arm_b.location + (ha - hb)
    bpy.context.view_layer.update()
    con = arm_b.constraints.new("CHILD_OF")
    con.target = arm
    con.subtarget = bones["head"]
    pb = arm.pose.bones[bones["head"]]
    con.inverse_matrix = (arm.matrix_world @ pb.matrix).inverted()
    bpy.context.view_layer.update()
    meshes.extend(keep)
    return keep


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
# Each entry: id, body model, tints by material keyword, role, and optionally a hair donor model.
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
    # new cast members built from the same CC0 parts: hair swaps and recolors
    ("aoi", "base_female", {"HAIR": 0x2FB7A8, "CLOTH": 0x7A2340}, "versus regular", "sendagaya_shino"),
    ("haru", "base_male", {"HAIR": 0xD8C07A, "CLOTH": 0x2F5D3A}, "versus regular", "hairsample_male"),
    ("mei", "avatarsample_g", {"HAIR": 0x1C1A24, "CLOTH": 0x8E1E34}, "versus regular", "avatarsample_d_0"),
    ("toma", "sakurada_fumiriya", {"HAIR": 0xE8ECF4, "CLOTH": 0x3A3F4A}, "versus regular", "hairsample_male"),
    ("luna", "avatarsample_f", {"HAIR": 0x8A5CF0, "CLOTH": 0x5B3BB0}, "versus regular", "sendagaya_shino"),
    ("kiko", "hairsample_female", {"HAIR": 0x7FE0B8, "CLOTH": 0xF2D45C}, "versus regular", "avatarsample_e"),
    ("jun", "base_male", {"HAIR": 0x23262E, "CLOTH": 0xE6E2D8}, "versus regular", "sakurada_fumiriya"),
    ("saki", "avatarsample_d_darkness", {"HAIR": 0xF4EEFF, "CLOTH": 0x26407A}, "versus regular", "avatarsample_g"),
    # the courier as a boy (the player picks; the game draws him wherever "mira" stands):
    # a pink hoodie and pink hair, so he reads as the courier, not as Kai
    ("hero_m", "hairsample_male", {"HAIR": 0xFF9ED8, "Tops": 0xAB6B8A}, "the courier, boy model", "sakurada_fumiriya"),
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
# A pose is a name in POSES (authored by hand) or (library, action, frame) from the Universal
# Animation Library, retargeted (see Retarget below): the calm expressions stand naturally.
EXPRS = {
    "normal": ({"neutral": 1.0}, (1, "Idle_Talking_Loop", 30)),
    "smile": ({"fun": 1.0}, (1, "Idle_Loop", 0)),
    "delighted": ({"joy": 1.0}, "cheer"),
    "angry": ({"angry": 1.0}, "angry"),
    "sad": ({"sorrow": 1.0}, "sad"),
    "shocked": ({"o": 1.0, "sorrow": 0.25}, (1, "Idle_Loop", 30)),
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


def poser(arm, bones):
    """put_pose(pz): a name in POSES (posed by hand) or (library, action, frame) from the
    Universal Animation Library, retargeted onto this armature (libraries load on first use)."""
    libs = {}

    def put_pose(pz):
        reset_pose(arm)
        if isinstance(pz, tuple):
            if pz[0] not in libs:
                src_arm, acts = import_ual(pz[0])
                libs[pz[0]] = (Retarget(arm, bones, pz[0], src_arm, acts), acts)
            r, acts = libs[pz[0]]
            r.apply(acts[pz[1]], pz[2])
        else:
            relax_hands(arm, bones, 0.6 if pz == "angry" else 0.0)
            pose(arm, bones, pz)

    return put_pose


def cast():
    os.makedirs(OUT, exist_ok=True)
    for entry in HUMANS:
        cid, src, tints, _role = entry[:4]
        donor = entry[4] if len(entry) > 4 else None
        if ONLY and cid not in ONLY:
            continue
        sc = T.eevee_scene(448, 960, transparent=True, samples=48)
        sc.render.image_settings.file_format = "PNG"
        sc.render.image_settings.color_mode = "RGBA"
        sc.render.image_settings.compression = 90
        lights(sc)
        arm, meshes, vrm, gltf = import_vrm(os.path.join(VRM_DIR, src + ".vrm"))
        bones = bone_map(vrm, gltf)
        if donor:
            transplant_hair(arm, bones, meshes, donor)
        toon_meshes(meshes, tints=tints)
        outline(meshes)
        put_pose = poser(arm, bones)

        only_ex = set(filter(None, os.environ.get("EXPR_ONLY", "").split(",")))
        # one framing for all eight poses (so sizes match between expressions), measured on
        # the posed meshes so raised hands stay inside the picture
        lo = Vector((1e9, 1e9, 1e9))
        hi = Vector((-1e9, -1e9, -1e9))
        for ex, (weights, pz) in EXPRS.items():
            put_pose(pz)
            a, b = posed_bounds(meshes)
            lo = Vector((min(lo.x, a.x), min(lo.y, a.y), min(lo.z, a.z)))
            hi = Vector((max(hi.x, b.x), max(hi.y, b.y), max(hi.z, b.z)))
        full = frame_bounds(sc, lo, hi, 448, 960, front=1.0, yaw=14)
        reset_pose(arm)
        bust = frame_camera(sc, meshes, "bust", 640, 640, front=1.0, yaw=10)
        for ex, (weights, pz) in EXPRS.items():
            if only_ex and ex not in only_ex:
                continue
            put_pose(pz)
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
        for entry in HUMANS:
            f.write("human %s %s\n" % (entry[0], entry[1]))
        for cid, name in CREATURES:
            f.write("creature %s %s\n" % (cid, name))


# ------------------------------------------------------------------ lighting passes
# The game lights the cast itself (crates/purtris-client/src/light.rs), so each scene's sun,
# sky, bounce and rim light fall on the characters. Every pose is rendered twice:
#   <id>_<expr>_<frame>.png    albedo: the textures' own color, unlit, with the ink line
#   <id>_<expr>_<frame>_n.png  half size: camera-space normal in R,G (x, y; z is implied)
#                              and the material class in B (see CLASSES)
CAM_AXES = []  # (axis index, socket) of every normal projection, aimed at the current camera
CLASSES = {"unlit": 0.0, "face": 0.25, "skin": 0.5, "cloth": 0.75, "hair": 1.0}


def material_class(name):
    low = name.lower()
    if low == "ink" or low.endswith("_eye") or (low.endswith("_face") and "skin" not in low):
        return "unlit"
    if "face" in low and "skin" in low:
        return "face"
    if "skin" in low or "body" in low:
        return "skin"
    if "hair" in low:
        return "hair"
    return "cloth"


def _emission_mat(name, color_fn, alpha_fn=None, blended=False):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    for n in list(nt.nodes):
        nt.nodes.remove(n)
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    em = nt.nodes.new("ShaderNodeEmission")
    em.inputs["Strength"].default_value = 1.0
    nt.links.new(color_fn(nt), em.inputs["Color"])
    if alpha_fn is None:
        nt.links.new(em.outputs[0], out.inputs["Surface"])
    else:
        tr = nt.nodes.new("ShaderNodeBsdfTransparent")
        mx = nt.nodes.new("ShaderNodeMixShader")
        nt.links.new(alpha_fn(nt), mx.inputs[0])
        nt.links.new(tr.outputs[0], mx.inputs[1])
        nt.links.new(em.outputs[0], mx.inputs[2])
        nt.links.new(mx.outputs[0], out.inputs["Surface"])
        mode = "BLENDED" if blended else "DITHERED"
        for attr, val in (("surface_render_method", mode), ("blend_method", "BLEND" if blended else "HASHED")):
            try:
                setattr(m, attr, val)
            except Exception:
                pass
    return m


def _normal_socket(nt, cls):
    """Camera-space shading normal (flipped on back faces), packed to 0..1, class in B."""
    geo = nt.nodes.new("ShaderNodeNewGeometry")
    flip = nt.nodes.new("ShaderNodeVectorMath")
    flip.operation = "SCALE"
    nt.links.new(geo.outputs["Normal"], flip.inputs[0])
    fac = nt.nodes.new("ShaderNodeMath")
    fac.operation = "MULTIPLY_ADD"
    nt.links.new(geo.outputs["Backfacing"], fac.inputs[0])
    fac.inputs[1].default_value = -2.0
    fac.inputs[2].default_value = 1.0
    nt.links.new(fac.outputs[0], flip.inputs["Scale"])
    # project onto the camera's axes (set per render by use_pass: right, up, toward the viewer)
    comb = nt.nodes.new("ShaderNodeCombineXYZ")
    for i in range(2):
        dot = nt.nodes.new("ShaderNodeVectorMath")
        dot.operation = "DOT_PRODUCT"
        nt.links.new(flip.outputs[0], dot.inputs[0])
        CAM_AXES.append((i, dot.inputs[1]))
        ma = nt.nodes.new("ShaderNodeMath")
        ma.operation = "MULTIPLY_ADD"
        nt.links.new(dot.outputs["Value"], ma.inputs[0])
        ma.inputs[1].default_value = 0.5
        ma.inputs[2].default_value = 0.5
        nt.links.new(ma.outputs[0], comb.inputs[i])
    comb.inputs[2].default_value = CLASSES[cls]
    return comb.outputs[0]


def gltf_factor(gltf, name):
    """The glTF baseColorFactor of a material (VRoid tints lashes, brows and eye lines with it)."""
    base = name.split(".")[0]
    for m in gltf.get("materials", []):
        if m.get("name") == base:
            return tuple(m.get("pbrMetallicRoughness", {}).get("baseColorFactor", [1, 1, 1, 1]))
    return (1, 1, 1, 1)


def pass_materials(meshes, tints=None, gltf=None):
    """For every material slot: (slot, albedo material, data material)."""
    made = {}
    slots = []
    for m in meshes:
        for slot in m.material_slots:
            mat = slot.material
            if mat is None:
                continue
            if mat.name not in made:
                cls = material_class(mat.name)
                if mat.name == "ink":
                    ink = lambda nt: _rgb(nt, T.lin(0x2A1E38))
                    alb = _emission_mat("ink_alb", ink)
                    dat = _emission_mat("ink_dat", lambda nt: _rgb(nt, (0.5, 0.5, 0.0, 1.0)))
                    for x in (alb, dat):
                        x.use_backface_culling = True
                else:
                    img = first_image(mat)
                    low = mat.name.lower()
                    tint = None
                    for key, col in (tints or {}).items():
                        if key.lower() in low:
                            tint = col
                    blend = getattr(mat, "blend_method", "OPAQUE")
                    cut = img is not None and blend in ("BLEND", "HASHED", "CLIP")
                    # face layers (lashes, brows, mouth, highlights) are blended over the skin
                    blended = cut and cls == "unlit" and blend == "BLEND"
                    holder = {}

                    fac = gltf_factor(gltf or {}, mat.name)

                    def color_fn(nt, img=img, tint=tint, holder=holder, fac=fac):
                        out = base_color(nt, img, tint, holder)
                        if max(abs(c - 1.0) for c in fac[:3]) > 1e-3:
                            out = T._mix(nt, "MULTIPLY", 1.0, out, (fac[0], fac[1], fac[2], 1.0))
                        return out

                    def alpha_fn(nt, holder=holder, fac=fac):
                        a = holder["t"].outputs["Alpha"]
                        if fac[3] < 0.999:
                            m = nt.nodes.new("ShaderNodeMath")
                            m.operation = "MULTIPLY"
                            nt.links.new(a, m.inputs[0])
                            m.inputs[1].default_value = fac[3]
                            a = m.outputs[0]
                        return a

                    def data_fn(nt, cls=cls, img=img, holder=holder):
                        if img is not None:
                            t = nt.nodes.new("ShaderNodeTexImage")
                            t.image = img
                            holder["t"] = t
                        return _normal_socket(nt, cls)

                    if "eyeextra" in low:
                        # VRoid's special-expression eye overlay ("><", hearts): never wanted here
                        alpha_fn = lambda nt, holder=None: _value(nt, 0.0)  # noqa: E731
                        cut = True
                    alb = _emission_mat(mat.name + "_alb", color_fn, alpha_fn if cut else None, blended)
                    holder2 = {}
                    dat = _emission_mat(mat.name + "_dat", lambda nt, f=data_fn, h=holder2: f(nt, holder=h),
                                        (lambda nt, h=holder2, af=alpha_fn: af(nt, holder=h)) if cut else None, blended)
                # keep the original's culling: VRoid hides layers (the "><" eye overlay, inner hair
                # shells) behind back faces
                for x in (alb, dat):
                    if mat.name != "ink":
                        x.use_backface_culling = mat.use_backface_culling
                made[mat.name] = (alb, dat)
            slots.append((slot, made[mat.name][0], made[mat.name][1]))
    return slots


def _value(nt, v):
    n = nt.nodes.new("ShaderNodeValue")
    n.outputs[0].default_value = v
    return n.outputs[0]


def _rgb(nt, col):
    rgb = nt.nodes.new("ShaderNodeRGB")
    rgb.outputs[0].default_value = col
    return rgb.outputs[0]


def base_color(nt, img, tint, holder):
    """The texture color, recolored like toon_meshes does for tinted hair and clothes."""
    if img is None:
        return _rgb(nt, (0.85, 0.85, 0.85, 1))
    t = nt.nodes.new("ShaderNodeTexImage")
    t.image = img
    holder["t"] = t
    out = t.outputs["Color"]
    if tint is not None:
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


def use_pass(sc, slots, which):
    for slot, alb, dat in slots:
        slot.material = alb if which == "albedo" else dat
    # data is written as is (no view transform, no dither); albedo keeps the texture's sRGB
    sc.view_settings.view_transform = "Raw" if which == "data" else "Standard"
    m3 = sc.camera.matrix_world.to_3x3()
    axes = (m3.col[0].normalized(), m3.col[1].normalized())
    for i, sock in CAM_AXES:
        sock.default_value = tuple(axes[i])
    sc.render.dither_intensity = 0.0
    sc.render.resolution_percentage = 50 if which == "data" else 100


def passes():
    os.makedirs(OUT, exist_ok=True)
    for entry in HUMANS:
        cid, src, tints, _role = entry[:4]
        donor = entry[4] if len(entry) > 4 else None
        if ONLY and cid not in ONLY:
            continue
        CAM_AXES.clear()  # the scene reset below frees the previous character's nodes
        sc = T.eevee_scene(448, 960, transparent=True, samples=24)
        sc.render.image_settings.file_format = "PNG"
        sc.render.image_settings.color_mode = "RGBA"
        sc.render.image_settings.compression = 100
        arm, meshes, vrm, gltf = import_vrm(os.path.join(VRM_DIR, src + ".vrm"))
        bones = bone_map(vrm, gltf)
        if donor:
            transplant_hair(arm, bones, meshes, donor)
        outline(meshes)
        slots = pass_materials(meshes, tints, gltf)
        put_pose = poser(arm, bones)
        lo = Vector((1e9, 1e9, 1e9))
        hi = Vector((-1e9, -1e9, -1e9))
        for ex, (weights, pz) in EXPRS.items():
            put_pose(pz)
            a, b = posed_bounds(meshes)
            lo = Vector((min(lo.x, a.x), min(lo.y, a.y), min(lo.z, a.z)))
            hi = Vector((max(hi.x, b.x), max(hi.y, b.y), max(hi.z, b.z)))
        full = frame_bounds(sc, lo, hi, 448, 960, front=1.0, yaw=14)
        reset_pose(arm)
        bust = frame_camera(sc, meshes, "bust", 640, 640, front=1.0, yaw=10)
        exprs = [e for e in EXPRS if not EXPR_ONLY or e in EXPR_ONLY]

        def both(cam, w, h, name):
            # the camera first: use_pass aims the normal projection at the current camera
            sc.camera = cam
            for which, suffix in (("albedo", ""), ("data", "_n")):
                use_pass(sc, slots, which)
                sc.render.resolution_x, sc.render.resolution_y = w, h
                render(sc, os.path.join(OUT, "%s%s.png" % (name, suffix)))

        for ex in exprs:
            weights, pz = EXPRS[ex]
            put_pose(pz)
            set_expression(vrm, gltf, meshes, weights)
            both(full, 448, 960, "%s_%s_full" % (cid, ex))
            both(bust, 640, 640, "%s_%s_bust" % (cid, ex))
        # menu and rival poses (see MENU_POSES), each framed on its own posed bounds
        if EXPR_ONLY and "p-" not in EXPR_ONLY:
            continue
        for name, lib, act, f, expr, yaw in MENU_POSES.get(cid, []):
            put_pose((lib, act, f))
            set_expression(vrm, gltf, meshes, EXPRS[expr][0])
            lo, hi = posed_bounds(meshes)
            cam = frame_bounds(sc, lo, hi, 720, 960, front=1.0, yaw=yaw)
            both(cam, 720, 960, "%s_p-%s_full" % (cid, name))


EXPR_ONLY = set(x[5:] for x in ONLY if x.startswith("expr:"))
ONLY = set(x for x in ONLY if not x.startswith("expr:"))


# ------------------------------------------------------------------ menu poses
# Natural poses for the menus: one frame of a real animation from Quaternius's Universal
# Animation Library (CC0, third_party/quaternius_ual), copied bone by bone onto the VRoid rig.
# Each bone gets the same world-space turn away from its T-pose as the source bone, so rigs
# with different bone rolls and proportions still match.
UAL_DIR = os.path.join(ROOT, "third_party", "quaternius_ual")
UAL = {
    1: os.path.join(UAL_DIR, "x1", "Animation Library[Standard]", "Godot", "AnimationLibrary_Godot_Standard.glb"),
    2: os.path.join(UAL_DIR, "x2", "Universal Animation Library 2 [Standard]", "Unreal-Godot", "UAL2_Standard.glb"),
}


def ual_map(lib):
    """VRM humanoid bone -> UAL bone name."""
    m = {}
    if lib == 1:
        m.update({"hips": "DEF-hips", "spine": "DEF-spine.001", "chest": "DEF-spine.002", "upperChest": "DEF-spine.003", "neck": "DEF-neck", "head": "DEF-head"})
        for side, s in (("left", "L"), ("right", "R")):
            for hb, b in (("Shoulder", "shoulder"), ("UpperArm", "upper_arm"), ("LowerArm", "forearm"), ("Hand", "hand"), ("UpperLeg", "thigh"), ("LowerLeg", "shin"), ("Foot", "foot"), ("Toes", "toe")):
                m[side + hb] = "DEF-%s.%s" % (b, s)
            for f, g in (("Index", "f_index"), ("Middle", "f_middle"), ("Ring", "f_ring"), ("Little", "f_pinky"), ("Thumb", "thumb")):
                for i, seg in enumerate(("Proximal", "Intermediate", "Distal")):
                    m[side + f + seg] = "DEF-%s.%02d.%s" % (g, i + 1, s)
    else:
        m.update({"hips": "pelvis", "spine": "spine_01", "chest": "spine_02", "upperChest": "spine_03", "neck": "neck_01", "head": "Head"})
        for side, s in (("left", "l"), ("right", "r")):
            for hb, b in (("Shoulder", "clavicle"), ("UpperArm", "upperarm"), ("LowerArm", "lowerarm"), ("Hand", "hand"), ("UpperLeg", "thigh"), ("LowerLeg", "calf"), ("Foot", "foot"), ("Toes", "ball")):
                m[side + hb] = "%s_%s" % (b, s)
            for f, g in (("Index", "index"), ("Middle", "middle"), ("Ring", "ring"), ("Little", "pinky"), ("Thumb", "thumb")):
                for i, seg in enumerate(("Proximal", "Intermediate", "Distal")):
                    m[side + f + seg] = "%s_%02d_%s" % (g, i + 1, s)
    return m


def import_ual(lib):
    """Import one animation library, hidden from renders. Returns (armature, {action name: action})."""
    before_o, before_a = set(bpy.data.objects), set(bpy.data.actions)
    bpy.ops.import_scene.gltf(filepath=UAL[lib])
    objs = [o for o in bpy.data.objects if o not in before_o]
    for o in objs:
        o.hide_render = True
    arm = next(o for o in objs if o.type == "ARMATURE")
    acts = {}
    for a in bpy.data.actions:
        if a not in before_a:
            name = a.name
            if len(name) > 4 and name[-4] == "." and name[-3:].isdigit():
                name = name[:-4]
            acts[name] = a
    if arm.animation_data is None:
        arm.animation_data_create()
    for t in list(arm.animation_data.nla_tracks):
        arm.animation_data.nla_tracks.remove(t)
    return arm, acts


def ual_frame(arm, act, frame):
    arm.animation_data.action = act
    try:
        slots = getattr(act, "slots", None)
        if slots:
            arm.animation_data.action_slot = slots[0]
    except Exception:
        pass
    bpy.context.scene.frame_set(int(frame))
    bpy.context.view_layer.update()


def world_q(arm, pb):
    return (arm.matrix_world @ pb.matrix).to_quaternion()


def world_head(arm, pb):
    return arm.matrix_world @ pb.head


def facing(arm, lname, rname):
    """Horizontal direction the character faces: its left-to-right shoulder line crossed with up."""
    v = world_head(arm, arm.pose.bones[lname]) - world_head(arm, arm.pose.bones[rname])
    v.z = 0
    v.normalize()
    return v.cross(Vector((0, 0, 1)))


def depth(pb):
    d = 0
    while pb.parent is not None:
        d += 1
        pb = pb.parent
    return d


class Retarget:
    """Copies UAL frames onto one VRM armature. The source's T-pose action is its reference."""

    def __init__(self, tgt, bones, lib, src, acts):
        self.tgt, self.bones, self.src = tgt, bones, src
        smap = ual_map(lib)
        self.map = {hb: smap[hb] for hb in bones if hb in smap and smap[hb] in src.pose.bones and bones[hb] in tgt.pose.bones}
        # the reference must be a true T-pose (arms level): library 1's T-pose action is one,
        # library 2's is an A-pose, so take whichever of it and the rest pose has level arms
        def arm_tilt():
            pb = src.pose.bones[self.map["leftUpperArm"]]
            d = (src.matrix_world @ pb.tail) - (src.matrix_world @ pb.head)
            return abs(d.normalized().z)

        ual_frame(src, acts["A_TPose"], 0)
        tilt_action = arm_tilt()
        src.animation_data.action = None
        reset_pose(src)
        tilt_rest = arm_tilt()
        if tilt_action < tilt_rest:
            ual_frame(src, acts["A_TPose"], 0)
        print("retarget lib %d reference: %s (arm tilt %.2f vs rest %.2f)" % (lib, "T-pose action" if tilt_action < tilt_rest else "rest", tilt_action, tilt_rest))
        self.src_ref = {hb: world_q(src, src.pose.bones[n]) for hb, n in self.map.items()}
        self.src_hips = world_head(src, src.pose.bones[self.map["hips"]])
        reset_pose(tgt)
        self.tgt_rest = {hb: world_q(tgt, tgt.pose.bones[bones[hb]]) for hb in self.map}
        self.tgt_hips = world_head(tgt, tgt.pose.bones[bones["hips"]])
        fs = facing(src, self.map["leftUpperArm"], self.map["rightUpperArm"])
        ft = facing(tgt, bones["leftUpperArm"], bones["rightUpperArm"])
        # a turn about the vertical only (rotation_difference picks an arbitrary axis when the
        # two facings are exactly opposite, which they usually are)
        self.q = Quaternion((0.0, 0.0, 1.0), math.atan2(fs.cross(ft).z, fs.dot(ft)))
        print("retarget facing src %s tgt %s turn %.1f deg; arm obj rot %s scale %s" % (tuple(round(v, 2) for v in fs), tuple(round(v, 2) for v in ft), math.degrees(self.q.angle), tuple(round(math.degrees(v), 1) for v in tgt.matrix_world.to_euler()), tuple(round(v, 3) for v in tgt.matrix_world.to_scale())))

        def leg(arm, a, b):
            return (world_head(arm, arm.pose.bones[a]) - world_head(arm, arm.pose.bones[b])).length

        self.hip_k = leg(tgt, bones["leftUpperLeg"], bones["leftFoot"]) / max(1e-6, leg(src, self.map["leftUpperLeg"], self.map["leftFoot"]))
        self.order = sorted(self.map, key=lambda hb: depth(tgt.pose.bones[bones[hb]]))

    def apply(self, act, frame):
        src, tgt = self.src, self.tgt
        ual_frame(src, act, frame)
        now = {hb: world_q(src, src.pose.bones[n]) for hb, n in self.map.items()}
        hips_now = world_head(src, src.pose.bones[self.map["hips"]])
        reset_pose(tgt)
        qi = self.q.inverted()
        for hb in self.order:
            d = now[hb] @ self.src_ref[hb].inverted()
            want = self.q @ d @ qi @ self.tgt_rest[hb]
            pb = tgt.pose.bones[self.bones[hb]]
            cur = tgt.matrix_world @ pb.matrix
            loc = cur.translation.copy()
            if hb == "hips":
                loc = self.tgt_hips + self.q @ ((hips_now - self.src_hips) * self.hip_k)
            m = Matrix.Translation(loc) @ want.to_matrix().to_4x4() @ Matrix.Diagonal(cur.to_scale().to_4d())
            pb.matrix = tgt.matrix_world.inverted() @ m
            bpy.context.view_layer.update()


# Candidates for choosing frames: (lib, action, frames)
POSE_CANDIDATES = [
    (1, "Idle_Loop", (0,)),
    (1, "Idle_Talking_Loop", (30, 60)),
    (1, "Interact", (30,)),
    (1, "Jog_Fwd_Loop", (15,)),
    (1, "Spell_Simple_Shoot", (4,)),
    (1, "Spell_Simple_Enter", (8,)),
    (1, "Punch_Cross", (8,)),
    (1, "Punch_Enter", (16,)),
    (1, "Sword_Idle", (20,)),
    (1, "Pistol_Idle_Loop", (20,)),
    (1, "Walk_Formal_Loop", (8,)),
    (1, "Jump_Land", (20,)),
    (2, "Idle_FoldArms_Loop", (10,)),
    (2, "Yes", (20,)),
    (2, "Idle_Rail_Call", (30,)),
    (2, "Idle_TalkingPhone_Loop", (30,)),
    (2, "NinjaJump_Idle_Loop", (10, 30)),
    (2, "OverhandThrow", (12,)),
    (2, "Chest_Open", (20,)),
    (2, "Idle_No_Loop", (20,)),
    (2, "Consume", (16,)),
    (2, "Farm_Watering", (40,)),
]

# Menu poses: id -> [(pose name, lib, action, frame, expression, camera yaw)]. Frames that
# lift a skirt (jumps, crouches, sitting, high knees) are left out on purpose.
_COURIER = [
    ("present", 1, "Spell_Simple_Shoot", 4, "smile", 14),
    ("talk", 1, "Idle_Talking_Loop", 30, "smile", 14),
]
MENU_POSES = {
    "mira": _COURIER,
    "hero_m": _COURIER,
    "kai": [("run", 1, "Jog_Fwd_Loop", 15, "delighted", 30), ("thumb", 2, "Yes", 20, "delighted", 14)],
    "kaede": [("fight", 1, "Punch_Cross", 8, "smug", 20)],
    "sora": [("thumb", 2, "Yes", 20, "delighted", 14)],
    "toma": [("look", 2, "Idle_Rail_Call", 30, "normal", 14)],
    "jun": [("arms", 2, "Idle_FoldArms_Loop", 10, "smug", 14)],
    "yuki": [("tinker", 1, "Interact", 30, "smile", 14)],
}
# Every rival's signature pose for the story (the map's rival card, character select):
# chosen for who they are.
RIVAL_POSES = {
    "kai": (1, "Punch_Cross", 8, "smug"),            # sparring partner, fists up
    "nami": (1, "Spell_Simple_Shoot", 4, "smug"),    # points the challenge at you
    "hana": (2, "Yes", 20, "delighted"),             # cheerful thumbs up
    "yuki": (1, "Spell_Simple_Idle_Loop", 20, "smug"),
    "rin": (2, "Idle_TalkingPhone_Loop", 30, "smug"),  # always on a deal
    "kaede": (2, "Idle_FoldArms_Loop", 10, "smug"),  # the queen, unimpressed
    "sora": (1, "Spell_Simple_Shoot", 4, "delighted"),
    "ren": (2, "Idle_FoldArms_Loop", 40, "normal"),
    "taro": (2, "Consume", 16, "smile"),             # a snack from the night train
    "emi": (1, "Punch_Cross", 8, "delighted"),
    "aoi": (2, "Yes", 20, "smile"),
    "haru": (2, "Idle_Rail_Call", 30, "smile"),      # looking out over the water
    "mei": (1, "Spell_Simple_Idle_Loop", 20, "smug"),
    "toma": (2, "Idle_FoldArms_Loop", 10, "normal"),
    "luna": (1, "Spell_Simple_Shoot", 4, "smug"),    # Mira's loud rival, pointing
    "kiko": (1, "Idle_Talking_Loop", 50, "delighted"),
    "jun": (1, "Punch_Enter", 16, "smug"),           # guard up, champion
    "saki": (1, "Sword_Idle", 20, "smug"),
}
for _cid, (_lib, _act, _f, _ex) in RIVAL_POSES.items():
    MENU_POSES.setdefault(_cid, []).append(("rival", _lib, _act, _f, _ex, 20))


def cast_model(cid):
    """Import and shade one cast member; returns (arm, meshes, vrm, gltf, bones)."""
    entry = next(e for e in HUMANS if e[0] == cid)
    src, tints = entry[1], entry[2]
    donor = entry[4] if len(entry) > 4 else None
    arm, meshes, vrm, gltf = import_vrm(os.path.join(VRM_DIR, src + ".vrm"))
    bones = bone_map(vrm, gltf)
    if donor:
        transplant_hair(arm, bones, meshes, donor)
    toon_meshes(meshes, tints=tints)
    outline(meshes)
    return arm, meshes, vrm, gltf, bones


def render_pose(sc, meshes, path, w, h, yaw):
    lo, hi = posed_bounds(meshes)
    sc.camera = frame_bounds(sc, lo, hi, w, h, front=1.0, yaw=yaw)
    sc.render.resolution_x, sc.render.resolution_y = w, h
    render(sc, path)


def poses(test):
    out = os.path.join(ROOT, "target", "posework") if test else OUT
    os.makedirs(out, exist_ok=True)
    ids = sorted(ONLY) if ONLY else sorted(MENU_POSES)
    for cid in ids:
        w, h = (320, 480) if test else (720, 960)
        sc = T.eevee_scene(w, h, transparent=True, samples=8 if test else 48)
        sc.render.image_settings.file_format = "PNG"
        sc.render.image_settings.color_mode = "RGBA"
        sc.render.image_settings.compression = 90
        lights(sc)
        arm, meshes, vrm, gltf, bones = cast_model(cid)
        libs = {}

        def rt(lib):
            if lib not in libs:
                src, acts = import_ual(lib)
                libs[lib] = (Retarget(arm, bones, lib, src, acts), acts)
            return libs[lib]

        if test:
            only_acts = set(filter(None, os.environ.get("POSE_ONLY", "").split(",")))
            for lib, act, frames in POSE_CANDIDATES:
                if only_acts and act not in only_acts:
                    continue
                r, acts = rt(lib)
                if act not in acts:
                    print("missing action", act)
                    continue
                for f in frames:
                    r.apply(acts[act], f)
                    set_expression(vrm, gltf, meshes, {"fun": 1.0})
                    render_pose(sc, meshes, os.path.join(out, "%s__%d_%s_%03d.png" % (cid, lib, act, f)), w, h, 14)
        else:
            only_poses = set(filter(None, os.environ.get("POSE_NAMES", "").split(",")))
            for name, lib, act, f, expr, yaw in MENU_POSES[cid]:
                if only_poses and name not in only_poses:
                    continue
                r, acts = rt(lib)
                r.apply(acts[act], f)
                set_expression(vrm, gltf, meshes, EXPRS[expr][0])
                render_pose(sc, meshes, os.path.join(out, "%s_p-%s_full.png" % (cid, name)), w, h, yaw)


if MODE == "preview":
    preview()
elif MODE in ("cast", "creatures"):
    cast()
elif MODE == "passes":
    passes()
elif MODE in ("poses", "posetest"):
    poses(MODE == "posetest")
