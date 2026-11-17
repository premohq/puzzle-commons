"""Puzzle Commons cast animation: renders each character's motion clips (idle, talk, attack,
hit, worried, win, lose) from the rigged VRoid models, in the same two passes the stills use
(unlit albedo + half-size normal/material map, lit by the game; see light.rs).

  blender -b --factory-startup --python tools/blender/anim3d.py -- <repo> <third_party dir> [ids] [clip:name] [frames:N]

Body motion is motion-captured-style keyframe animation from Quaternius's Universal Animation
Library 1 and 2 (CC0, third_party/quaternius_ual), retargeted onto each VRoid skeleton: every
bone (fingers too) takes the same world-space turn away from its T-pose that the library's bone
takes away from the library's T-pose. Faces (VRM blend shapes: blinks, mouth shapes,
expressions) are keyed here per clip. Hair, skirts and sleeves are not keyed: they are simulated
with each model's own VRM spring-bone settings (stiffness, drag, gravity, colliders), the same
algorithm VRM viewers run, so they swing and settle as the body moves. Looping clips are
simulated for a few loops first and the last loop is kept, so they join up.

Frames go to target/anim_frames/<id>/<clip>_<k>.png (+ _n.png); tools/blender/anim_pack.py
packs them into assets/anim/<id>.anim.
"""
import bpy
import math
import os
import sys
from mathutils import Matrix, Vector, Quaternion

ARGV = sys.argv[sys.argv.index("--") + 1:]
REPO = ARGV[0]
TP = ARGV[1]
VRM_SRC = os.path.join(TP, "vroid_cc0", "vrm")
UAL = {
    1: os.path.join(TP, "quaternius_ual", "x1", "Animation Library[Standard]", "Godot", "AnimationLibrary_Godot_Standard.glb"),
    2: os.path.join(TP, "quaternius_ual", "x2", "Universal Animation Library 2 [Standard]", "Unreal-Godot", "UAL2_Standard.glb"),
}
REST = ARGV[2:]
# cast3d reads its own arguments: <repo> <mode> [ids]; give it a mode it ignores
sys.argv = sys.argv[: sys.argv.index("--") + 1] + [REPO, "anim"]
sys.path.insert(0, os.path.join(REPO, "tools", "blender"))
import cast3d as C  # noqa: E402
import toon_lib as T  # noqa: E402

C.VRM_DIR = VRM_SRC
C.WORK = os.path.join(REPO, "target", "anim_work")
os.makedirs(C.WORK, exist_ok=True)
ONLY = set(a for a in REST if ":" not in a)
CLIP_ONLY = set(a[5:] for a in REST if a.startswith("clip:"))
FRAMES_ONLY = next((int(a[7:]) for a in REST if a.startswith("frames:")), None)
OUT = os.path.join(REPO, "target", "anim_frames")

# The still's framing is 448 x 960; clips get a margin all round (same pixel density) so
# raised arms and hops stay in the picture. The game lines the two up with these numbers.
STILL_W, STILL_H = 448, 960
M_SIDE, M_TOP, M_BOT = 64, 64, 0
AW, AH = STILL_W + 2 * M_SIDE, STILL_H + M_TOP + M_BOT
FPS = 15
SIM_HZ = 60


# ------------------------------------------------------------------ forward kinematics
class Skel:
    """A pose we compute ourselves (Blender only evaluates it when rendering): bone matrices
    in armature space from each bone's basis, parents first."""

    def __init__(self, arm, place=None):
        self.arm = arm
        self.order = []
        self.parent = {}
        self.rest_rel = {}
        self.rest = {}

        def walk(b):
            self.order.append(b.name)
            self.parent[b.name] = b.parent.name if b.parent else None
            self.rest[b.name] = b.matrix_local.copy()
            self.rest_rel[b.name] = (b.parent.matrix_local.inverted() @ b.matrix_local) if b.parent else b.matrix_local.copy()
            for c in b.children:
                walk(c)

        for b in arm.data.bones:
            if b.parent is None:
                walk(b)
        self.basis = {n: Matrix.Identity(4) for n in self.order}
        # where this armature sits in the main armature's space (a callable of the main pose)
        self.place = place
        self.mats = {}

    def fk(self, root=None):
        root = root if root is not None else Matrix.Identity(4)
        m = self.mats
        for n in self.order:
            p = self.parent[n]
            m[n] = (m[p] if p else root) @ self.rest_rel[n] @ self.basis[n]
        return m

    def apply(self):
        for n in self.order:
            pb = self.arm.pose.bones.get(n)
            if pb is not None:
                pb.matrix_basis = self.basis[n]


# ------------------------------------------------------------------ spring bones (VRM 0.x)
class Spring:
    def __init__(self, skel, name, stiff, drag, grav, hit, colliders):
        self.skel, self.name = skel, name
        self.stiff, self.drag, self.grav, self.hit = stiff, drag, grav, hit
        self.colliders = colliders
        self.length = skel.arm.data.bones[name].length
        self.cur = None
        self.prev = None
        self.extra = 1.0  # how much wind it catches


def vrm_springs(skel, vrm, gltf, wind_gain, colliders=None):
    """Spring joints (root first) and their collider spheres for one armature."""
    nodes = gltf["nodes"]
    sa = vrm.get("secondaryAnimation", {})
    groups = []
    for cg in sa.get("colliderGroups", []):
        bname = nodes[cg["node"]]["name"]
        spheres = []
        for c in cg.get("colliders", []):
            o = c.get("offset", {})
            # VRM 0.x offsets are Unity coordinates (left-handed, Y up); Blender's: X, Z, Y
            spheres.append((Vector((o.get("x", 0.0), o.get("z", 0.0), o.get("y", 0.0))), c.get("radius", 0.05)))
        groups.append((skel, bname, spheres))
    out = []
    seen = set()
    for bg in sa.get("boneGroups", []):
        stiff = bg.get("stiffiness", bg.get("stiffness", 1.0))
        drag = bg.get("dragForce", 0.4)
        gd = bg.get("gravityDir", {"x": 0, "y": -1, "z": 0})
        grav = Vector((gd.get("x", 0), gd.get("z", 0), gd.get("y", -1))) * bg.get("gravityPower", 0.0)
        hit = bg.get("hitRadius", 0.02)
        cols = colliders if colliders is not None else [groups[i] for i in bg.get("colliderGroups", []) if i < len(groups)]
        comment = (bg.get("comment") or "").lower()
        for ni in bg.get("bones", []):
            root = nodes[ni]["name"]
            if root not in skel.arm.data.bones:
                continue
            stack = [skel.arm.data.bones[root]]
            while stack:
                b = stack.pop(0)
                if b.name in seen:
                    continue
                seen.add(b.name)
                s = Spring(skel, b.name, stiff, drag, grav, hit, cols)
                s.extra = wind_gain.get("bust" if "bust" in comment else "skirt" if "skirt" in comment else "sleeve" if "sleeve" in comment else "hair", 1.0)
                out.append(s)
                stack.extend(b.children)
    # parents before children
    order = {n: i for i, n in enumerate(skel.order)}
    out.sort(key=lambda s: order[s.name])
    return out, groups


def sim_step(skel, springs, root, dt, wind):
    """One step of VRM's spring-bone logic for the springs of one armature (in main armature
    space). Updates the springs' basis and the armature's matrices in place."""
    m = skel.mats
    yv = Vector((0, 1, 0))
    # collider spheres where the bones are now
    cache = {}

    def spheres(group):
        gsk, bname, sph = group
        key = (id(gsk), bname)
        if key not in cache:
            gm = gsk.mats
            if bname in gm:
                M = gm[bname] @ gsk.rest[bname].inverted()
                cache[key] = [(M @ (gsk.rest[bname].translation + off), r) for off, r in sph]
            else:
                cache[key] = []
        return cache[key]

    for s in springs:
        n = s.name
        p = skel.parent[n]
        P = (m[p] if p else root) @ skel.rest_rel[n]
        head = P.translation
        R = P.to_3x3()
        d0 = (R @ yv).normalized()
        if s.cur is None:
            s.cur = head + d0 * s.length
            s.prev = s.cur.copy()
        nxt = s.cur + (s.cur - s.prev) * (1.0 - s.drag) + d0 * (s.stiff * dt) + s.grav * dt + wind * (s.extra * dt)
        nxt = head + (nxt - head).normalized() * s.length
        for g in s.colliders:
            for c, r in spheres(g):
                rr = r + s.hit
                if (nxt - c).length_squared < rr * rr:
                    pc = c + (nxt - c).normalized() * rr
                    nxt = head + (pc - head).normalized() * s.length
        s.prev = s.cur
        s.cur = nxt
        loc = (R.inverted() @ (nxt - head)).normalized()
        q = yv.rotation_difference(loc)
        skel.basis[n] = q.to_matrix().to_4x4()
        m[n] = P @ skel.basis[n]
        # children of a spring that are not springs follow along
    return m


# ------------------------------------------------------------------ the library's rigs
def _ual_map(lib):
    m = {}
    if lib == 1:
        core = {"hips": "DEF-hips", "spine": "DEF-spine.001", "chest": "DEF-spine.002", "upperChest": "DEF-spine.003", "neck": "DEF-neck", "head": "DEF-head"}
        for side, s in (("left", "L"), ("right", "R")):
            core.update({side + "Shoulder": "DEF-shoulder." + s, side + "UpperArm": "DEF-upper_arm." + s, side + "LowerArm": "DEF-forearm." + s, side + "Hand": "DEF-hand." + s,
                         side + "UpperLeg": "DEF-thigh." + s, side + "LowerLeg": "DEF-shin." + s, side + "Foot": "DEF-foot." + s, side + "Toes": "DEF-toe." + s})
            for vf, uf in (("Thumb", "thumb"), ("Index", "f_index"), ("Middle", "f_middle"), ("Ring", "f_ring"), ("Little", "f_pinky")):
                for k, seg in enumerate(("Proximal", "Intermediate", "Distal")):
                    core[side + vf + seg] = "DEF-%s.%02d.%s" % (uf, k + 1, s)
        m = core
    else:
        core = {"hips": "pelvis", "spine": "spine_01", "chest": "spine_02", "upperChest": "spine_03", "neck": "neck_01", "head": "Head"}
        for side, s in (("left", "l"), ("right", "r")):
            core.update({side + "Shoulder": "clavicle_" + s, side + "UpperArm": "upperarm_" + s, side + "LowerArm": "lowerarm_" + s, side + "Hand": "hand_" + s,
                         side + "UpperLeg": "thigh_" + s, side + "LowerLeg": "calf_" + s, side + "Foot": "foot_" + s, side + "Toes": "ball_" + s})
            for vf, uf in (("Thumb", "thumb"), ("Index", "index"), ("Middle", "middle"), ("Ring", "ring"), ("Little", "pinky")):
                for k, seg in enumerate(("Proximal", "Intermediate", "Distal")):
                    core[side + vf + seg] = "%s_%02d_%s" % (uf, k + 1, s)
        m = core
    return m


def _set_action(arm, act):
    if arm.animation_data is None:
        arm.animation_data_create()
    arm.animation_data.action = act
    try:
        slots = getattr(act, "slots", None)
        if slots:
            arm.animation_data.action_slot = slots[0]
    except Exception:
        pass


def _forward(arm, foot, toe):
    """The way a rig faces, from heel to toe, flattened."""
    a = arm.matrix_world @ arm.pose.bones[foot].head
    b = arm.matrix_world @ arm.pose.bones[toe].head
    d = Vector((b.x - a.x, b.y - a.y, 0.0))
    return d.normalized()


SAMPLES = {}


def sample_library(needed):
    """Every clip we use, sampled at SIM_HZ: per humanoid bone, the world-space turn away from
    the library's T-pose (already turned to face the way VRoid models face, +Y), and the hips'
    move as a fraction of hip height."""
    by_lib = {}
    for name, (lib, act, *_r) in needed.items():
        by_lib.setdefault(lib, []).append((name, act))
    for lib, items in by_lib.items():
        bpy.ops.wm.read_factory_settings(use_empty=True)
        sc = bpy.context.scene
        bpy.ops.import_scene.gltf(filepath=UAL[lib])
        arm = next(o for o in bpy.data.objects if o.type == "ARMATURE")
        fps = sc.render.fps / sc.render.fps_base
        mp = {hb: b for hb, b in _ual_map(lib).items() if b in arm.pose.bones}
        acts = {a.name: a for a in bpy.data.actions}

        def grab():
            bpy.context.view_layer.update()
            return {hb: (arm.matrix_world @ arm.pose.bones[b].matrix) for hb, b in mp.items()}

        _set_action(arm, acts["A_TPose"])
        sc.frame_set(0)
        ref = grab()
        fwd = _forward(arm, mp["leftFoot"], mp["leftToes"])
        # turn the library's facing onto +Y
        turn = Vector((fwd.x, fwd.y, 0)).to_track_quat("Y", "Z").to_matrix().inverted()
        hip_h = ref["hips"].translation.z - min(ref["leftFoot"].translation.z, ref["rightFoot"].translation.z)
        ref_rot = {hb: M.to_3x3().normalized() for hb, M in ref.items()}
        for name, an in items:
            act = acts[an]
            _set_action(arm, act)
            f0, f1 = act.frame_range
            secs = (f1 - f0) / fps
            n = max(2, int(round(secs * SIM_HZ)))
            frames = []
            for k in range(n + 1):
                f = f0 + (f1 - f0) * k / n
                sc.frame_set(int(math.floor(f)), subframe=f - math.floor(f))
                cur = grab()
                d = {}
                for hb, M in cur.items():
                    D = M.to_3x3().normalized() @ ref_rot[hb].inverted()
                    d[hb] = turn @ D @ turn.inverted()
                off = turn @ (cur["hips"].translation - ref["hips"].translation)
                frames.append((d, off / max(hip_h, 1e-6)))
            SAMPLES[name] = dict(secs=secs, frames=frames)
            print("sampled %s <- %s (%.2fs)" % (name, an, secs))


def library_pose(name, t, loop):
    """Interpolated library pose at time t (seconds)."""
    s = SAMPLES[name]
    fr = s["frames"]
    n = len(fr) - 1
    x = (t / s["secs"]) * n
    if loop:
        x = x % n
    x = max(0.0, min(n - 1e-6, x))
    i = int(x)
    u = x - i
    a, b = fr[i], fr[min(i + 1, n)]
    d = {}
    for hb in a[0]:
        qa = a[0][hb].to_quaternion()
        qb = b[0][hb].to_quaternion()
        d[hb] = qa.slerp(qb, u).to_matrix()
    off = a[1].lerp(b[1], u)
    return d, off


class Retarget:
    def __init__(self, skel, bones):
        self.skel = skel
        self.hb_of = {b: hb for hb, b in bones.items()}
        self.rest_rot = {n: skel.rest[n].to_3x3().normalized() for n in skel.order}
        hips = bones["hips"]
        feet = [skel.rest[bones[f]].translation.z for f in ("leftFoot", "rightFoot") if f in bones]
        self.hip_h = skel.rest[hips].translation.z - (min(feet) if feet else 0.0)
        self.hips = hips

    def set(self, delta, off, extra=None, hip_scale=(1.0, 1.0, 1.0)):
        """delta: humanoid -> world turn from T-pose; extra: humanoid -> extra arm-space turn
        that also carries the bone's children (for small per-character touches)."""
        sk = self.skel
        mats = {}
        ecum = {}
        for n in sk.order:
            p = sk.parent[n]
            P = (mats[p] if p else Matrix.Identity(4)) @ sk.rest_rel[n]
            hb = self.hb_of.get(n)
            E = ecum.get(p, Matrix.Identity(3))
            if extra and hb in extra:
                E = E @ extra[hb]
            ecum[n] = E
            if hb in delta:
                Wr = E @ delta[hb] @ self.rest_rot[n]
                B = (P.to_3x3().normalized().inverted() @ Wr).to_4x4()
                if n == self.hips:
                    o = Vector((off.x * hip_scale[0], off.y * hip_scale[1], off.z * hip_scale[2])) * self.hip_h
                    B.translation = P.to_3x3().normalized().inverted() @ o
                sk.basis[n] = B
            else:
                # unmapped bones (springs, eyes, the root) keep their rest turn on their parent
                sk.basis[n] = Matrix.Identity(4)
            mats[n] = P @ sk.basis[n]
        sk.mats = mats
        return mats


def axis_rot(ax, deg):
    return Matrix.Rotation(math.radians(deg), 3, ax)


def turn(pitch=0.0, roll=0.0, yaw=0.0):
    """An arm-space turn for a model facing +Y: pitch + bows forward, roll + leans to the
    screen's right, yaw + turns to the screen's right."""
    return axis_rot("Z", yaw) @ axis_rot("Y", -roll) @ axis_rot("X", -pitch)


# ------------------------------------------------------------------ timing helpers
def ease_in_out(x):
    x = max(0.0, min(1.0, x))
    return 4 * x * x * x if x < 0.5 else 1 - (-2 * x + 2) ** 3 / 2


def env(t, a, b, c, d):
    """0 before a, rises to 1 at b, holds, falls to 0 from c to d (smooth)."""
    if t <= a or t >= d:
        return 0.0
    if t < b:
        return ease_in_out((t - a) / (b - a))
    if t <= c:
        return 1.0
    return 1.0 - ease_in_out((t - c) / (d - c))


def osc(t, period, phase=0.0):
    return math.sin(2 * math.pi * (t / period + phase))


def blink(t, at, dur=0.16):
    """Eyelids for a blink starting at `at`: quick close, a beat shut, slower open."""
    x = t - at
    if x < 0 or x > dur:
        return 0.0
    c = dur * 0.35
    return min(1.0, x / c) if x < c else max(0.0, 1.0 - (x - c) / (dur - c)) if x > c + 0.03 else 1.0


def blinks(t, times):
    return max([blink(t, a) for a in times] + [0.0])


def speech(t, rate=1.0):
    """A mouth that talks: syllables at an uneven rhythm, with short pauses."""
    v = 0.5 + 0.5 * math.sin(t * 2 * math.pi * 4.2 * rate) * math.sin(t * 2 * math.pi * 1.3 * rate + 0.7)
    v = max(0.0, v * 1.4 - 0.25)
    pause = 0.5 + 0.5 * math.sin(t * 2 * math.pi * 0.37 + 1.0)
    return min(1.0, v) * (0.35 + 0.65 * pause)


def wind(t, L, s=1.0):
    # a soft breeze that comes and goes (periodic with the loop), mostly across the picture
    g = 0.5 + 0.5 * osc(t, L, 0.1)
    return Vector((0.010 * (0.4 + g) * osc(t, L / 2, 0.3), -0.004 * g, 0.002 * osc(t, L / 3))) * s


# ------------------------------------------------------------------ the clips
# Each clip: library actions played back to back, then (for one-shots) a blend back into the
# first frame of the character's idle, so the game can drop straight into idle afterwards.
# face(t, secs) gives the VRM expression weights; extra(t, secs) small arm-space turns.
def f_idle(t, L):
    b = blinks(t, (0.7 * L,))
    return {"neutral": 1.0, "fun": 0.22 * (1 - b), "blink": b}


def f_proud(t, L):
    b = blinks(t, (0.55 * L,))
    return {"fun": 0.45 * (1 - b), "blink_l": 0.25 * (1 - b), "blink": b}


def f_talk(t, L):
    b = blinks(t, (0.35 * L, 0.82 * L))
    m = speech(t)
    return {"fun": 0.35 * (1 - b), "a": 0.55 * m, "o": 0.15 * m * (0.5 + 0.5 * math.sin(t * 7.0)), "blink": b}


def f_attack(t, L):
    # a breath in, then a shout on the blow
    k = env(t, 0.05 * L, 0.3 * L, 0.55 * L, 0.85 * L)
    return {"angry": 0.35 + 0.45 * k, "a": 0.7 * k, "fun": 0.25 * (1 - k)}


def f_cast(t, L):
    k = env(t, 0.1 * L, 0.35 * L, 0.6 * L, 0.85 * L)
    return {"fun": 0.4 * (1 - k), "joy": 0.6 * k, "a": 0.35 * k}


def f_hit(t, L):
    k = env(t, 0.0, 0.08 * L, 0.35 * L, 0.85 * L)
    return {"sorrow": 0.7 * k, "blink": 0.85 * k, "o": 0.45 * k, "neutral": 1 - k}


def f_worried(t, L):
    b = blinks(t, (0.3 * L, 0.42 * L))
    return {"sorrow": 0.75, "o": 0.18 + 0.08 * osc(t, L / 2), "blink": b}


def f_win(t, L):
    return {"joy": 1.0, "a": 0.25 + 0.15 * osc(t, L / 2)}


def f_lose(t, L):
    b = 0.35 + 0.3 * env(t % L, 0.3 * L, 0.45 * L, 0.6 * L, 0.75 * L)
    return {"sorrow": 1.0, "blink": b}


STANCE = 3.0  # the library stands wide; our cast stands a little narrower


def x_stance(t, L):
    return {"leftUpperLeg": turn(roll=STANCE), "leftFoot": turn(roll=-STANCE),
            "rightUpperLeg": turn(roll=-STANCE), "rightFoot": turn(roll=STANCE)}


def x_worried(t, L):
    x = x_stance(t, L)
    # hunched, peering up at the stack
    x.update({"upperChest": turn(pitch=4.0), "head": turn(pitch=-3.0, roll=2.0 * osc(t, L, 0.2))})
    return x


def x_lose(t, L):
    x = x_stance(t, L)
    x.update({"head": turn(pitch=12.0), "upperChest": turn(pitch=7.0), "leftShoulder": turn(roll=-3.0), "rightShoulder": turn(roll=3.0)})
    return x


# name: ([(library, action), ...], loop?, face, extra, wind, recover seconds)
CLIPS = {
    "idle": ([(1, "Idle_Loop")], True, f_idle, x_stance, 1.0, 0.0),
    "talk": ([(1, "Idle_Talking_Loop")], True, f_talk, x_stance, 0.8, 0.0),
    "attack": ([(1, "Punch_Cross")], False, f_attack, x_stance, 0.3, 0.4),
    "cast": ([(1, "Spell_Simple_Enter"), (1, "Spell_Simple_Shoot"), (1, "Spell_Simple_Exit")], False, f_cast, x_stance, 0.3, 0.35),
    "hit": ([(1, "Hit_Chest")], False, f_hit, x_stance, 0.3, 0.45),
    "hit_big": ([(1, "Hit_Head")], False, f_hit, x_stance, 0.3, 0.45),
    "worried": ([(2, "Idle_No_Loop")], True, f_worried, x_worried, 0.6, 0.0),
    "win": ([(1, "Dance_Loop")], True, f_win, x_stance, 0.8, 0.0),
    "lose": ([(1, "Idle_Loop")], True, f_lose, x_lose, 0.4, 0.0),
}
# characters who stand differently: arms folded, a knowing look
PROUD = {"kaede", "jun", "toma"}
# who throws a punch when they send garbage, and who casts it
CASTERS = {"yuki", "hana", "aoi", "kiko", "mei", "luna", "kaede", "saki", "toma"}


def clips_for(cid):
    out = {}
    for name, c in CLIPS.items():
        if CLIP_ONLY and name not in CLIP_ONLY:
            continue
        if name == "attack" and cid in CASTERS or name == "cast" and cid not in CASTERS:
            continue
        srcs, loop, face, extra, ws, rec = c
        if cid in PROUD and name in ("idle", "lose"):
            srcs = [(2, "Idle_FoldArms_Loop")]
            if name == "idle":
                face = f_proud
        out[name] = (srcs, loop, face, extra, ws, rec)
    out["_idle"] = out.get("idle") or (([(2, "Idle_FoldArms_Loop")] if cid in PROUD else [(1, "Idle_Loop")]), True, f_idle, x_stance, 1.0, 0.0)
    return out


def clip_pose(srcs, idle_key, t, loop, rec):
    """The library pose of a clip at time t: its actions back to back, then the blend home."""
    keys = ["%d:%s" % s for s in srcs]
    total = sum(SAMPLES[k]["secs"] for k in keys)
    if loop:
        t = t % total
    if not loop and t >= total:
        d0, o0 = library_pose(keys[-1], SAMPLES[keys[-1]]["secs"], False)
        d1, o1 = library_pose(idle_key, 0.0, True)
        u = ease_in_out((t - total) / rec) if rec > 0 else 1.0
        d = {hb: d0[hb].to_quaternion().slerp(d1[hb].to_quaternion(), u).to_matrix() for hb in d0 if hb in d1}
        return d, o0.lerp(o1, u)
    for k in keys:
        s = SAMPLES[k]["secs"]
        if t <= s or k == keys[-1]:
            return library_pose(k, min(t, s), loop and len(keys) == 1)
        t -= s


# ------------------------------------------------------------------ rendering
def anim_camera(sc, full):
    """The still's camera, widened by the margins at the same pixel size."""
    cd = full.data.copy()
    s = full.data.ortho_scale / STILL_H  # world units per pixel (height-fitted, H > W)
    cd.ortho_scale = s * AH
    # shift is a fraction of the larger side: move the picture's centre so the still's area
    # sits M_TOP from the top and M_BOT from the bottom
    cd.shift_y = ((M_TOP - M_BOT) / 2.0) / AH
    cd.shift_x = 0.0
    cam = bpy.data.objects.new("cam_anim", cd)
    cam.location = full.location.copy()
    cam.rotation_euler = full.rotation_euler.copy()
    sc.collection.objects.link(cam)
    return cam


def render_char(entry):
    cid, src, tints, _role = entry[:4]
    donor = entry[4] if len(entry) > 4 else None
    C.CAM_AXES.clear()
    sc = T.eevee_scene(AW, AH, transparent=True, samples=16)
    sc.render.image_settings.file_format = "PNG"
    sc.render.image_settings.color_mode = "RGBA"
    sc.render.image_settings.compression = 15
    arm, meshes, vrm, gltf = C.import_vrm(os.path.join(C.VRM_DIR, src + ".vrm"))
    bones = C.bone_map(vrm, gltf)
    donor_arm = None
    if donor:
        before = set(o for o in bpy.data.objects if o.type == "ARMATURE")
        C.transplant_hair(arm, bones, meshes, donor)
        donor_arm = next((o for o in bpy.data.objects if o.type == "ARMATURE" and o not in before), None)
    C.outline(meshes)
    slots = C.pass_materials(meshes, tints, gltf)
    # the stills' framing (bounds over every still pose), then ours around it
    lo = Vector((1e9, 1e9, 1e9))
    hi = Vector((-1e9, -1e9, -1e9))
    for ex, (weights, pz) in C.EXPRS.items():
        C.reset_pose(arm)
        C.relax_hands(arm, bones, 0.6 if pz == "angry" else 0.0)
        C.pose(arm, bones, pz)
        a, b = C.posed_bounds(meshes)
        lo = Vector((min(lo.x, a.x), min(lo.y, a.y), min(lo.z, a.z)))
        hi = Vector((max(hi.x, b.x), max(hi.y, b.y), max(hi.z, b.z)))
    C.reset_pose(arm)
    full = C.frame_bounds(sc, lo, hi, STILL_W, STILL_H, front=1.0, yaw=14)
    sc.camera = full
    cam = anim_camera(sc, full)
    sc.camera = cam

    skel = Skel(arm)
    rt = Retarget(skel, bones)
    springs, groups = vrm_springs(skel, vrm, gltf, {"hair": 1.0, "skirt": 0.6, "sleeve": 0.5, "bust": 0.0})
    dskel = None
    dsprings = []
    if donor_arm is not None:
        dj = C.vrm_json(os.path.join(C.VRM_DIR, donor + ".vrm"))
        dskel = Skel(donor_arm)
        # the donor's own body stays in its T-pose, unseen: the hair collides with ours
        dsprings, _ = vrm_springs(dskel, dj.get("extensions", {}).get("VRM", {}), dj, {"hair": 1.0}, colliders=groups)
        # Child Of: donor = head(t) @ head(rest)^-1 @ donor's own place (all in our armature space)
        head_rest = arm.data.bones[bones["head"]].matrix_local
        D0 = arm.matrix_world.inverted() @ donor_arm.matrix_basis
        dskel.place = lambda mats: mats[bones["head"]] @ head_rest.inverted() @ D0
    os.makedirs(os.path.join(OUT, cid), exist_ok=True)
    cl = clips_for(cid)
    idle_key = "%d:%s" % cl["_idle"][0][0]
    for name, (srcs, loop, face, extra, ws, rec) in cl.items():
        if name.startswith("_"):
            continue
        secs = sum(SAMPLES["%d:%s" % s]["secs"] for s in srcs) + (0.0 if loop else rec)
        n = FRAMES_ONLY or max(2, round(secs * FPS))
        dt = 1.0 / SIM_HZ
        pre = 1.5 + (2 * secs if loop else 0.0)
        t = -pre
        for s in springs + dsprings:
            s.cur = None
        frame_times = [k * secs / (n if loop else max(1, n - 1)) for k in range(n)]
        k = 0
        while k < n:
            tt = (t % secs) if loop else max(0.0, min(secs, t))
            d, off = clip_pose(srcs, idle_key, tt, loop, rec)
            mats = rt.set(d, off, extra(tt, secs))
            w = wind(tt, secs, ws)
            sim_step(skel, springs, Matrix.Identity(4), dt, w)
            if dskel is not None:
                dskel.fk(dskel.place(mats))
                sim_step(dskel, dsprings, dskel.place(mats), dt, w)
            if t >= frame_times[k] - 1e-6:
                skel.apply()
                if dskel is not None:
                    dskel.apply()
                C.set_expression(vrm, gltf, meshes, face(tt, secs))
                for which, suffix in (("albedo", ""), ("data", "_n")):
                    C.use_pass(sc, slots, which)
                    sc.render.resolution_x, sc.render.resolution_y = AW, AH
                    C.render(sc, os.path.join(OUT, cid, "%s_%02d%s.png" % (name, k, suffix)))
                k += 1
            t += dt
        with open(os.path.join(OUT, cid, "%s.txt" % name), "w") as f:
            f.write("frames %d\nsecs %.4f\nloop %d\nsource %s\n" % (n, secs, 1 if loop else 0, "+".join("%d:%s" % s for s in srcs)))


needed = {}
for entry in C.HUMANS:
    if ONLY and entry[0] not in ONLY:
        continue
    for name, c in clips_for(entry[0]).items():
        for lib, act in c[0]:
            needed["%d:%s" % (lib, act)] = (lib, act)
sample_library(needed)
for entry in C.HUMANS:
    if ONLY and entry[0] not in ONLY:
        continue
    render_char(entry)
