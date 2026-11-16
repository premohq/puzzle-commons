"""Shared toon (cel) look for every Puzzle Commons render: EEVEE, hard light bands with a cool
shadow tint, a specular highlight band, a rim light, flat saturated color, optional bloom.
Used by export_cast.py (pieces), overworld.py (story maps) and backdrop.py (scenes).
"""
import math
import bpy


def lin(hexv):
    def f(c):
        c /= 255.0
        return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4

    return (f((hexv >> 16) & 255), f((hexv >> 8) & 255), f(hexv & 255), 1.0)


def eevee_scene(w, h, transparent=False, samples=32):
    bpy.ops.wm.read_factory_settings(use_empty=True)
    sc = bpy.context.scene
    sc.render.engine = "BLENDER_EEVEE"
    try:
        sc.eevee.taa_render_samples = samples
    except Exception:
        pass
    for attr, val in (("use_shadows", True), ("shadow_ray_count", 2), ("shadow_step_count", 8)):
        try:
            setattr(sc.eevee, attr, val)
        except Exception:
            pass
    sc.render.resolution_x = w
    sc.render.resolution_y = h
    sc.render.resolution_percentage = 100
    sc.render.film_transparent = transparent
    sc.render.image_settings.file_format = "TARGA_RAW"
    sc.render.image_settings.color_mode = "RGBA" if transparent else "RGB"
    sc.view_settings.view_transform = "Standard"
    sc.view_settings.look = "None"
    world = bpy.data.worlds.new("world")
    sc.world = world
    return sc


def world_backdrop(sc, top, bottom, ambient=0.18):
    """Camera sees a vertical gradient; everything else gets a weak neutral ambient so the
    toon ramp is driven by the sun."""
    world = sc.world
    world.use_nodes = True
    nt = world.node_tree
    for n in list(nt.nodes):
        nt.nodes.remove(n)
    out = nt.nodes.new("ShaderNodeOutputWorld")
    amb = nt.nodes.new("ShaderNodeBackground")
    amb.inputs[0].default_value = (0.7, 0.78, 1.0, 1)
    amb.inputs[1].default_value = ambient
    tc = nt.nodes.new("ShaderNodeTexCoord")
    sep = nt.nodes.new("ShaderNodeSeparateXYZ")
    nt.links.new(tc.outputs["Window"], sep.inputs[0])
    ramp = nt.nodes.new("ShaderNodeValToRGB")
    ramp.color_ramp.elements[0].color = lin(bottom)
    ramp.color_ramp.elements[1].color = lin(top)
    nt.links.new(sep.outputs["Y"], ramp.inputs["Fac"])
    cam = nt.nodes.new("ShaderNodeBackground")
    nt.links.new(ramp.outputs["Color"], cam.inputs["Color"])
    lp = nt.nodes.new("ShaderNodeLightPath")
    mix = nt.nodes.new("ShaderNodeMixShader")
    nt.links.new(lp.outputs["Is Camera Ray"], mix.inputs[0])
    nt.links.new(amb.outputs[0], mix.inputs[1])
    nt.links.new(cam.outputs[0], mix.inputs[2])
    nt.links.new(mix.outputs[0], out.inputs["Surface"])


def sun(sc, rot_deg=(50, -15, -35), energy=4.0, color=0xFFF4E0, angle=2.5):
    ld = bpy.data.lights.new("sun", "SUN")
    ld.energy = energy
    ld.color = lin(color)[:3]
    ld.angle = math.radians(angle)
    for attr, val in (("shadow_maximum_resolution", 0.002), ("shadow_softness_factor", 1.0), ("use_shadow_jitter", True)):
        try:
            setattr(ld, attr, val)
        except Exception:
            pass
    o = bpy.data.objects.new("sun", ld)
    o.rotation_euler = tuple(math.radians(a) for a in rot_deg)
    sc.collection.objects.link(o)
    return o


def _ramp(nt, stops, interp="CONSTANT"):
    r = nt.nodes.new("ShaderNodeValToRGB")
    r.color_ramp.interpolation = interp
    els = r.color_ramp.elements
    els[0].position, els[0].color = stops[0]
    els[1].position, els[1].color = stops[-1]
    for pos, col in stops[1:-1]:
        e = els.new(pos)
        e.color = col
    return r


def _mix(nt, mode, fac, a, b):
    m = nt.nodes.new("ShaderNodeMix")
    m.data_type = "RGBA"
    m.blend_type = mode
    if isinstance(fac, (int, float)):
        m.inputs["Factor"].default_value = fac
    else:
        nt.links.new(fac, m.inputs["Factor"])
    for idx, v in ((6, a), (7, b)):
        if isinstance(v, tuple):
            m.inputs[idx].default_value = v
        else:
            nt.links.new(v, m.inputs[idx])
    return m.outputs[2]


def toon(name, base, shadow=(0.58, 0.64, 0.92), spec=0.55, spec_size=0.62, rim=0.22, base_socket_fn=None,
         emit_boost=1.0, alpha=None, threshold=0.32, ao=0.0, alpha_mode="BLENDED"):
    """A cel material. `base` is a hex color, or base_socket_fn(nt) returns a color socket."""
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    for n in list(nt.nodes):
        nt.nodes.remove(n)
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    diff = nt.nodes.new("ShaderNodeBsdfDiffuse")
    diff.inputs[0].default_value = (1, 1, 1, 1)
    s2r = nt.nodes.new("ShaderNodeShaderToRGB")
    nt.links.new(diff.outputs[0], s2r.inputs[0])
    lit = _ramp(nt, [(0.0, (0, 0, 0, 1)), (threshold, (1, 1, 1, 1))])
    nt.links.new(s2r.outputs["Color"], lit.inputs["Fac"])
    if base_socket_fn is not None:
        base_sock = base_socket_fn(nt)
    else:
        rgb = nt.nodes.new("ShaderNodeRGB")
        rgb.outputs[0].default_value = lin(base)
        base_sock = rgb.outputs[0]
    shade = _mix(nt, "MULTIPLY", 1.0, base_sock, (shadow[0], shadow[1], shadow[2], 1))
    col = _mix(nt, "MIX", lit.outputs["Color"], shade, base_sock)
    if spec > 0:
        gl = nt.nodes.new("ShaderNodeBsdfGlossy")
        gl.inputs["Roughness"].default_value = 0.3
        s2 = nt.nodes.new("ShaderNodeShaderToRGB")
        nt.links.new(gl.outputs[0], s2.inputs[0])
        band = _ramp(nt, [(0.0, (0, 0, 0, 1)), (spec_size, (spec, spec, spec, 1))])
        nt.links.new(s2.outputs["Color"], band.inputs["Fac"])
        col = _mix(nt, "ADD", 1.0, col, band.outputs["Color"])
    if rim > 0:
        lw = nt.nodes.new("ShaderNodeLayerWeight")
        lw.inputs["Blend"].default_value = 0.35
        rr = _ramp(nt, [(0.0, (0, 0, 0, 1)), (0.72, (rim, rim, rim, 1))])
        nt.links.new(lw.outputs["Facing"], rr.inputs["Fac"])
        rim_lit = _mix(nt, "MULTIPLY", 1.0, rr.outputs["Color"], lit.outputs["Color"])
        col = _mix(nt, "ADD", 1.0, col, rim_lit)
    if ao > 0:
        aon = nt.nodes.new("ShaderNodeAmbientOcclusion")
        aon.inputs["Distance"].default_value = 0.35
        rr = nt.nodes.new("ShaderNodeMapRange")
        rr.inputs["To Min"].default_value = 1.0 - ao
        nt.links.new(aon.outputs["AO"], rr.inputs["Value"])
        aoc = nt.nodes.new("ShaderNodeCombineColor")
        for k in ("Red", "Green", "Blue"):
            nt.links.new(rr.outputs["Result"], aoc.inputs[k])
        col = _mix(nt, "MULTIPLY", 1.0, col, aoc.outputs[0])
    em = nt.nodes.new("ShaderNodeEmission")
    em.inputs["Strength"].default_value = emit_boost
    nt.links.new(col, em.inputs["Color"])
    if alpha is None:
        nt.links.new(em.outputs[0], out.inputs["Surface"])
    else:
        tr = nt.nodes.new("ShaderNodeBsdfTransparent")
        mx = nt.nodes.new("ShaderNodeMixShader")
        if isinstance(alpha, (int, float)):
            mx.inputs[0].default_value = alpha
        else:
            nt.links.new(alpha(nt), mx.inputs[0])
        nt.links.new(tr.outputs[0], mx.inputs[1])
        nt.links.new(em.outputs[0], mx.inputs[2])
        nt.links.new(mx.outputs[0], out.inputs["Surface"])
        for attr, val in (("surface_render_method", alpha_mode), ("blend_method", "BLEND" if alpha_mode == "BLENDED" else "HASHED")):
            try:
                setattr(m, attr, val)
            except Exception:
                pass
    return m


def glow(name, hexv, strength=4.0):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    for n in list(nt.nodes):
        nt.nodes.remove(n)
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    em = nt.nodes.new("ShaderNodeEmission")
    em.inputs[0].default_value = lin(hexv)
    em.inputs[1].default_value = strength
    nt.links.new(em.outputs[0], out.inputs["Surface"])
    return m


def bloom(sc, threshold=0.9, size=7, mix=-0.75):
    """Soft bloom through the compositor. Silently skipped if this Blender's API differs."""
    try:
        if hasattr(sc, "compositing_node_group"):
            tree = bpy.data.node_groups.new("comp", "CompositorNodeTree")
            sc.compositing_node_group = tree
            gin = tree.nodes.new("CompositorNodeRLayers")
            gout = tree.nodes.new("NodeGroupOutput")
            tree.interface.new_socket("Image", in_out="OUTPUT", socket_type="NodeSocketColor")
        else:
            sc.use_nodes = True
            tree = sc.node_tree
            for n in list(tree.nodes):
                tree.nodes.remove(n)
            gin = tree.nodes.new("CompositorNodeRLayers")
            gout = tree.nodes.new("CompositorNodeComposite")
        g = tree.nodes.new("CompositorNodeGlare")
        if "Type" in g.inputs:
            g.inputs["Type"].default_value = "Bloom"
            g.inputs["Threshold"].default_value = threshold
            g.inputs["Strength"].default_value = max(0.05, 1.0 + mix) if mix < 0 else 0.4
            g.inputs["Size"].default_value = min(1.0, size / 10.0)
            g.inputs["Quality"].default_value = "High"
        else:
            g.glare_type = "FOG_GLOW"
            g.threshold = threshold
            g.size = size
            g.mix = mix
        tree.links.new(gin.outputs["Image"], g.inputs["Image"])
        tree.links.new(g.outputs["Image"], gout.inputs[0])
        return True
    except Exception as e:
        print("bloom skipped:", e)
        return False


def find_image(socket, depth=0):
    """Walk back from a socket to the first image texture node, if any."""
    if not socket.is_linked or depth > 6:
        return None
    n = socket.links[0].from_node
    if n.bl_idname == "ShaderNodeTexImage":
        return n.image
    for inp in n.inputs:
        if inp.is_linked:
            im = find_image(inp, depth + 1)
            if im is not None:
                return im
    return None


def toonify(mat, tint=None, shadow=(0.58, 0.64, 0.92), spec=0.18, rim=0.22, ao=0.3):
    """Convert an imported PBR material to the cel look, keeping its texture and alpha cutout."""
    img, col, cut = None, (0.8, 0.8, 0.8, 1.0), False
    p = None
    if mat is not None and mat.node_tree is not None:
        p = next((n for n in mat.node_tree.nodes if n.bl_idname == "ShaderNodeBsdfPrincipled"), None)
    if p is not None:
        img = find_image(p.inputs["Base Color"])
        if img is None:
            col = tuple(p.inputs["Base Color"].default_value)
        cut = p.inputs["Alpha"].is_linked
    holder = {}

    def base_fn(nt):
        if img is not None:
            t = nt.nodes.new("ShaderNodeTexImage")
            t.image = img
            holder["t"] = t
            out = t.outputs["Color"]
        else:
            rgb = nt.nodes.new("ShaderNodeRGB")
            rgb.outputs[0].default_value = col
            out = rgb.outputs[0]
        if tint is not None:
            out = _mix(nt, "MULTIPLY", 1.0, out, lin(tint))
        return out

    alpha = (lambda nt: holder["t"].outputs["Alpha"]) if (img is not None and cut) else None
    return toon((mat.name if mat else "m") + "_toon", 0, shadow=shadow, base_socket_fn=base_fn, spec=spec, rim=rim,
                alpha=alpha, alpha_mode="DITHERED", ao=ao, threshold=0.3)


def post(sc, fog=None, fog_start=6.0, fog_depth=30.0, fog_strength=0.35, bloom_threshold=0.95, bloom_strength=0.25):
    """Compositor: depth mist (mist pass blended to a fog color), then soft bloom."""
    try:
        vl = sc.view_layers[0]
        vl.use_pass_mist = True
        if sc.world is not None:
            sc.world.mist_settings.start = fog_start
            sc.world.mist_settings.depth = fog_depth
            sc.world.mist_settings.falloff = "QUADRATIC"
        tree = bpy.data.node_groups.new("comp", "CompositorNodeTree")
        sc.compositing_node_group = tree
        rl = tree.nodes.new("CompositorNodeRLayers")
        gout = tree.nodes.new("NodeGroupOutput")
        tree.interface.new_socket("Image", in_out="OUTPUT", socket_type="NodeSocketColor")
        img = rl.outputs["Image"]
        if fog is not None and "Mist" in rl.outputs:
            mul = tree.nodes.new("ShaderNodeMath")
            mul.operation = "MULTIPLY"
            mul.inputs[1].default_value = fog_strength
            tree.links.new(rl.outputs["Mist"], mul.inputs[0])
            mx = tree.nodes.new("ShaderNodeMix")
            mx.data_type = "RGBA"
            tree.links.new(mul.outputs[0], mx.inputs["Factor"])
            tree.links.new(img, mx.inputs[6])
            mx.inputs[7].default_value = lin(fog)
            img = mx.outputs[2]
        g = tree.nodes.new("CompositorNodeGlare")
        g.inputs["Type"].default_value = "Bloom"
        g.inputs["Threshold"].default_value = bloom_threshold
        g.inputs["Strength"].default_value = bloom_strength
        g.inputs["Size"].default_value = 0.6
        tree.links.new(img, g.inputs["Image"])
        tree.links.new(g.outputs["Image"], gout.inputs[0])
        return True
    except Exception as e:
        print("post skipped:", e)
        return False


def fog_volume(name, center, size, color, density, top):
    """A box of ground fog whose density fades out with height (top = world Z where it vanishes)."""
    me = bpy.data.meshes.new(name)
    import bmesh
    bm = bmesh.new()
    bmesh.ops.create_cube(bm, size=1.0)
    bm.to_mesh(me)
    bm.free()
    ob = bpy.data.objects.new(name, me)
    ob.location = center
    ob.scale = size
    bpy.context.scene.collection.objects.link(ob)
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    nt = m.node_tree
    for n in list(nt.nodes):
        nt.nodes.remove(n)
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    vol = nt.nodes.new("ShaderNodeVolumePrincipled")
    vol.inputs["Color"].default_value = lin(color)
    tc = nt.nodes.new("ShaderNodeTexCoord")
    sep = nt.nodes.new("ShaderNodeSeparateXYZ")
    nt.links.new(tc.outputs["Object"], sep.inputs[0])
    nz = nt.nodes.new("ShaderNodeTexNoise")
    nz.inputs["Scale"].default_value = 2.5
    nt.links.new(tc.outputs["Object"], nz.inputs["Vector"])
    fall = nt.nodes.new("ShaderNodeMapRange")
    fall.inputs["From Min"].default_value = -0.5
    fall.inputs["From Max"].default_value = (top - center[2]) / max(size[2], 1e-3)
    fall.inputs["To Min"].default_value = density
    fall.inputs["To Max"].default_value = 0.0
    nt.links.new(sep.outputs["Z"], fall.inputs["Value"])
    d = nt.nodes.new("ShaderNodeMath")
    d.operation = "MULTIPLY"
    nt.links.new(fall.outputs["Result"], d.inputs[0])
    nt.links.new(nz.outputs["Fac"], d.inputs[1])
    nt.links.new(d.outputs[0], vol.inputs["Density"])
    nt.links.new(vol.outputs[0], out.inputs["Volume"])
    me.materials.append(m)
    return ob
