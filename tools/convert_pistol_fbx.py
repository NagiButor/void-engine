#!/usr/bin/env python3
"""Convert Armpist.fbx + textures into assets/models/fps_pistol.glb and assets/textures/weapons/pistol/*.png."""

from __future__ import annotations

import json
import math
import struct
import zlib
from pathlib import Path
import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
SRC_DIR = Path(r"E:\Downloads\fps-pistol-animations")
FBX_PATH = SRC_DIR / "source" / "Armpist.fbx"
TEX_SRC = SRC_DIR / "textures"
OUT_GLB = ROOT / "assets" / "models" / "fps_pistol.glb"
OUT_TEX = ROOT / "assets" / "textures" / "weapons" / "pistol"

FBX_TIME_SECOND = 46186158000.0

ANIM_RENAMES = {
    "Armature|FPS_Pistol_Idle": "idle",
    "Armature|FPS_Pistol_Walk": "walk",
    "Armature|FPS_Pistol_Fire": "fire_0",
    "Armature|FPS_Pistol_Fire.001": "fire_1",
    "Armature|FPS_Pistol_Fire.002": "fire_2",
    "Armature|FPS_Pistol_Reload_easy": "reload_easy",
    "Armature|FPS_Pistol_Reload_full": "reload_full",
}

MESH_ORDER = [
    ("Mesh", "arms", 0),
    ("10_low_4:003.002", "pistol_frame", 1),
    ("10_low_4:003.005", "pistol_slide", 2),
    ("10_low_4:003.006", "pistol_mag", 3),
]


def read_fbx(path: Path):
    data = path.read_bytes()
    if not data.startswith(b"Kaydara FBX Binary  \x00"):
        raise ValueError("Not a binary FBX file")
    ver = struct.unpack("<I", data[23:27])[0]
    pos = 27
    is_64 = ver >= 7500

    def read_prop():
        nonlocal pos
        t = chr(data[pos])
        pos += 1
        if t == "Y":
            v = struct.unpack("<h", data[pos : pos + 2])[0]
            pos += 2
            return v
        if t == "C":
            v = data[pos] != 0
            pos += 1
            return v
        if t == "I":
            v = struct.unpack("<i", data[pos : pos + 4])[0]
            pos += 4
            return v
        if t == "F":
            v = struct.unpack("<f", data[pos : pos + 4])[0]
            pos += 4
            return v
        if t == "D":
            v = struct.unpack("<d", data[pos : pos + 8])[0]
            pos += 8
            return v
        if t == "L":
            v = struct.unpack("<q", data[pos : pos + 8])[0]
            pos += 8
            return v
        if t in ("f", "d", "l", "i", "b"):
            arr_len, enc, comp_len = struct.unpack("<III", data[pos : pos + 12])
            pos += 12
            raw = data[pos : pos + comp_len]
            pos += comp_len
            if enc == 1:
                raw = zlib.decompress(raw)
            sz = {"f": 4, "d": 8, "l": 8, "i": 4, "b": 1}[t]
            code = {"f": "f", "d": "d", "l": "q", "i": "i", "b": "B"}[t]
            return list(struct.unpack("<" + str(len(raw) // sz) + code, raw))
        if t in ("S", "R"):
            slen = struct.unpack("<I", data[pos : pos + 4])[0]
            pos += 4
            val = data[pos : pos + slen]
            pos += slen
            return val.decode("utf-8", "replace") if t == "S" else val
        raise ValueError(f"Unknown FBX property type {t!r}")

    def read_node():
        nonlocal pos
        if is_64:
            end_off, num_props, _ = struct.unpack("<QQQ", data[pos : pos + 24])
            pos += 24
        else:
            end_off, num_props, _ = struct.unpack("<III", data[pos : pos + 12])
            pos += 12
        nlen = data[pos]
        pos += 1
        if end_off == 0:
            return None
        name = data[pos : pos + nlen].decode("utf-8", "replace")
        pos += nlen
        props = [read_prop() for _ in range(num_props)]
        children = []
        null_size = 25 if is_64 else 13
        while pos < end_off:
            if end_off - pos == null_size and data[pos:end_off] == b"\x00" * null_size:
                pos = end_off
                break
            ch = read_node()
            if ch is None:
                break
            children.append(ch)
        pos = end_off
        return (name, props, children)

    nodes = []
    while pos < len(data):
        n = read_node()
        if n is None:
            break
        nodes.append(n)
    return nodes


def euler_xyz_quat(rx_deg: float, ry_deg: float, rz_deg: float) -> list[float]:
    """Convert FBX XYZ Euler angles (degrees) into glTF [x, y, z, w] quaternion."""
    hx, hy, hz = math.radians(rx_deg) * 0.5, math.radians(ry_deg) * 0.5, math.radians(rz_deg) * 0.5
    cx, sx = math.cos(hx), math.sin(hx)
    cy, sy = math.cos(hy), math.sin(hy)
    cz, sz = math.cos(hz), math.sin(hz)
    # R = Rz * Ry * Rx
    qx = sx * cy * cz - cx * sy * sz
    qy = cx * sy * cz + sx * cy * sz
    qz = cx * cy * sz - sx * sy * cz
    qw = cx * cy * cz + sx * sy * sz
    n = math.sqrt(qx * qx + qy * qy + qz * qz + qw * qw) or 1.0
    return [qx / n, qy / n, qz / n, qw / n]


def mat4_to_trs(m: np.ndarray):
    t = m[:3, 3].tolist()
    sx = float(np.linalg.norm(m[:3, 0]))
    sy = float(np.linalg.norm(m[:3, 1]))
    sz = float(np.linalg.norm(m[:3, 2]))
    r = np.zeros((3, 3))
    r[:, 0] = m[:3, 0] / max(sx, 1e-8)
    r[:, 1] = m[:3, 1] / max(sy, 1e-8)
    r[:, 2] = m[:3, 2] / max(sz, 1e-8)
    tr = r[0, 0] + r[1, 1] + r[2, 2]
    if tr > 0:
        s = math.sqrt(tr + 1.0) * 2.0
        qw = 0.25 * s
        qx = (r[2, 1] - r[1, 2]) / s
        qy = (r[0, 2] - r[2, 0]) / s
        qz = (r[1, 0] - r[0, 1]) / s
    elif r[0, 0] > r[1, 1] and r[0, 0] > r[2, 2]:
        s = math.sqrt(1.0 + r[0, 0] - r[1, 1] - r[2, 2]) * 2.0
        qw = (r[2, 1] - r[1, 2]) / s
        qx = 0.25 * s
        qy = (r[0, 1] + r[1, 0]) / s
        qz = (r[0, 2] + r[2, 0]) / s
    elif r[1, 1] > r[2, 2]:
        s = math.sqrt(1.0 + r[1, 1] - r[0, 0] - r[2, 2]) * 2.0
        qw = (r[0, 2] - r[2, 0]) / s
        qx = (r[0, 1] + r[1, 0]) / s
        qy = 0.25 * s
        qz = (r[1, 2] + r[2, 1]) / s
    else:
        s = math.sqrt(1.0 + r[2, 2] - r[0, 0] - r[1, 1]) * 2.0
        qw = (r[1, 0] - r[0, 1]) / s
        qx = (r[0, 2] + r[2, 0]) / s
        qy = (r[1, 2] + r[2, 1]) / s
        qz = 0.25 * s
    return t, [qx, qy, qz, qw], [sx, sy, sz]


def get_model_trs(model_node):
    t, r, s = [0.0, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]
    for ch in model_node[2]:
        if ch[0] == "Properties70":
            for p in ch[2]:
                if p[1][0] == "Lcl Translation":
                    t = [float(x) for x in p[1][4:7]]
                elif p[1][0] == "Lcl Rotation":
                    r = [float(x) for x in p[1][4:7]]
                elif p[1][0] == "Lcl Scaling":
                    s = [float(x) for x in p[1][4:7]]
    return t, r, s


def build_textures():
    OUT_TEX.mkdir(parents=True, exist_ok=True)

    def resize_open(name: str, mode: str):
        im = Image.open(TEX_SRC / name).convert(mode)
        if im.size != (1024, 1024):
            im = im.resize((1024, 1024), Image.Resampling.LANCZOS)
        return im

    # Arms: color * AO, roughness, normal
    arm_col = np.asarray(resize_open("armColor.png", "RGB"), dtype=np.float32)
    arm_ao = np.asarray(resize_open("armAO.png", "L"), dtype=np.float32)[..., None] / 255.0
    arm_col_baked = np.clip(arm_col * (0.25 + 0.75 * arm_ao), 0, 255).astype(np.uint8)
    Image.fromarray(arm_col_baked, mode="RGB").save(OUT_TEX / "arms_albedo.png")
    resize_open("armRoughness.png", "L").save(OUT_TEX / "arms_roughness.png")
    resize_open("armNormal.png", "RGB").save(OUT_TEX / "arms_normal.png")

    # Pistol: diffuse, 255 - glossiness -> roughness, normal
    resize_open("for_texturing_None.001_Diffuse.png", "RGB").save(OUT_TEX / "pistol_albedo.png")
    gloss = np.asarray(resize_open("for_texturing_None.001_Glossiness.png", "L"), dtype=np.uint8)
    rough = 255 - gloss
    Image.fromarray(rough, mode="L").save(OUT_TEX / "pistol_roughness.png")
    resize_open("for_texturing_None.001_Normal.png", "RGB").save(OUT_TEX / "pistol_normal.png")
    print(f"Saved 1024x1024 pistol & arms PBR textures to {OUT_TEX}")


def convert_fbx_to_glb():
    nodes = read_fbx(FBX_PATH)
    by_name = {n[0]: n for n in nodes}
    objs = by_name["Objects"][2]
    conns = by_name["Connections"][2]
    id_map = {o[1][0]: o for o in objs if o[1]}

    children_of: dict[int, list[int]] = {}
    for c in conns:
        if c[1][0] == "OO":
            a, b = c[1][1], c[1][2]
            if a in id_map and id_map[a][0] == "Model":
                children_of.setdefault(b, []).append(a)

    # Collect skeleton hierarchy rooted at Armature
    armature_id = [o[1][0] for o in objs if o[0] == "Model" and o[1][1].startswith("Armature\x00")][0]
    joint_ids: list[int] = []

    def walk_joints(nid: int):
        joint_ids.append(nid)
        for ch in children_of.get(nid, []):
            walk_joints(ch)

    walk_joints(armature_id)
    joint_index = {nid: idx for idx, nid in enumerate(joint_ids)}

    # Collect Cluster inverse bind transforms (in FBX Blender export, Cluster.Transform = inv(bone_bind) @ mesh_bind)
    ibm_by_joint = [np.eye(4, dtype=np.float32) for _ in joint_ids]
    geom_skin_map: dict[int, tuple[list[list[int]], list[list[float]]]] = {}

    for geom_name, _, _ in MESH_ORDER:
        g = [o for o in objs if o[0] == "Geometry" and o[1][1].startswith(geom_name + "\x00")][0]
        g_id = g[1][0]
        gch = {c[0]: c for c in g[2]}
        num_verts = len(gch["Vertices"][1][0]) // 3
        influences: list[list[tuple[int, float]]] = [[] for _ in range(num_verts)]

        skins = [c[1][1] for c in conns if c[1][0] == "OO" and c[1][2] == g_id and id_map[c[1][1]][0] == "Deformer"]
        for sid in skins:
            for c in conns:
                if c[1][0] == "OO" and c[1][2] == sid:
                    cl = id_map[c[1][1]]
                    cch = {x[0]: x for x in cl[2]}
                    if "Indexes" not in cch:
                        continue
                    bone_ids = [
                        bc[1][1]
                        for bc in conns
                        if bc[1][0] == "OO" and bc[1][2] == cl[1][0] and id_map[bc[1][1]][0] == "Model"
                    ]
                    if not bone_ids:
                        continue
                    bid = bone_ids[0]
                    jidx = joint_index[bid]
                    T = np.array(cch["Transform"][1][0], dtype=np.float32).reshape(4, 4).T
                    ibm_by_joint[jidx] = T
                    idxs = cch["Indexes"][1][0]
                    wts = cch["Weights"][1][0]
                    for vi, w in zip(idxs, wts):
                        if w > 1e-6:
                            influences[vi].append((jidx, float(w)))

        v_joints = []
        v_weights = []
        for inf in influences:
            inf.sort(key=lambda x: x[1], reverse=True)
            inf = inf[:4]
            total = sum(w for _, w in inf)
            if total <= 1e-6:
                inf = [(0, 1.0)]
                total = 1.0
            j4 = [0, 0, 0, 0]
            w4 = [0.0, 0.0, 0.0, 0.0]
            for k, (ji, w) in enumerate(inf):
                j4[k] = ji
                w4[k] = w / total
            v_joints.append(j4)
            v_weights.append(w4)
        geom_skin_map[g_id] = (v_joints, v_weights)

    # glTF nodes:
    # Node 0 = ViewRoot (converts FBX Armature world cm/+Z-fwd/Head_Cam -> VOID meters/-Z-fwd/eye-origin)
    # Wait: Head_Cam is at (0, 21.58354 cm, 0) in Root space -> (0, 0.2158354 m, 0) in VOID meters.
    # Armature has Lcl Rotation=(-90,0,0), Lcl Scaling=(100,100,100).
    # In VOID view-space: x_void = -x_fbx * 0.01, y_void = y_fbx * 0.01 - 0.2158354, z_void = -z_fbx * 0.01.
    # Let Node 0 (ViewRoot) carry translation=(0, -0.2158354, 0), rotation=rot_y(180deg)=[0,1,0,0], scale=[0.01,0.01,0.01].
    # And Nodes 1..len(joint_ids) correspond to joint_ids[0..].
    # And Nodes after joints correspond to the 4 skinned mesh nodes (parented to ViewRoot).
    head_cam_id = [o[1][0] for o in objs if o[0] == "Model" and o[1][1].startswith("Head_Cam\x00")][0]
    head_t, _, _ = get_model_trs(id_map[head_cam_id])
    head_y_m = float(head_t[1])

    gltf_nodes = [
        {
            "name": "ViewRoot",
            "translation": [0.0, -head_y_m, 0.0],
            "rotation": [0.0, 1.0, 0.0, 0.0],
            "scale": [0.01, 0.01, 0.01],
            "children": [1],
        }
    ]
    for nid in joint_ids:
        m = id_map[nid]
        t, r, s = get_model_trs(m)
        ch_nodes = [1 + joint_index[c] for c in children_of.get(nid, []) if c in joint_index]
        node_obj = {
            "name": m[1][1].split("\x00")[0],
            "translation": t,
            "rotation": euler_xyz_quat(*r),
            "scale": s,
        }
        if ch_nodes:
            node_obj["children"] = ch_nodes
        gltf_nodes.append(node_obj)

    joint_node_indices = [1 + i for i in range(len(joint_ids))]

    # Binary buffer builder
    bin_buf = bytearray()
    buffer_views = []
    accessors = []

    def push_data(raw: bytes, target: int | None = None) -> int:
        while len(bin_buf) % 4 != 0:
            bin_buf.append(0)
        off = len(bin_buf)
        bin_buf.extend(raw)
        bv = {"buffer": 0, "byteOffset": off, "byteLength": len(raw)}
        if target is not None:
            bv["target"] = target
        buffer_views.append(bv)
        return len(buffer_views) - 1

    def add_accessor(bv: int, comp_type: int, count: int, acc_type: str, min_v=None, max_v=None) -> int:
        acc = {
            "bufferView": bv,
            "componentType": comp_type,
            "count": count,
            "type": acc_type,
        }
        if min_v is not None:
            acc["min"] = min_v
        if max_v is not None:
            acc["max"] = max_v
        accessors.append(acc)
        return len(accessors) - 1

    # Inverse bind matrices (column-major float32 4x4)
    ibm_raw = bytearray()
    for M in ibm_by_joint:
        ibm_raw.extend(M.T.astype("<f4").tobytes())
    ibm_bv = push_data(bytes(ibm_raw))
    ibm_acc = add_accessor(ibm_bv, 5126, len(joint_ids), "MAT4")

    # Build 4 meshes
    gltf_meshes = []
    mesh_node_indices = []
    for geom_name, out_name, mat_idx in MESH_ORDER:
        g = [o for o in objs if o[0] == "Geometry" and o[1][1].startswith(geom_name + "\x00")][0]
        g_id = g[1][0]
        gch = {c[0]: c for c in g[2]}
        raw_verts = gch["Vertices"][1][0]
        poly_idx = gch["PolygonVertexIndex"][1][0]
        norm_ch = {c[0]: c for c in gch["LayerElementNormal"][2]}
        raw_norms = norm_ch["Normals"][1][0]
        uv_ch = {c[0]: c for c in gch["LayerElementUV"][2]}
        raw_uvs = uv_ch["UV"][1][0]
        uv_indices = uv_ch["UVIndex"][1][0]
        v_joints, v_weights = geom_skin_map[g_id]

        # Triangulate polygons and deduplicate vertices
        vert_map: dict[tuple, int] = {}
        pos_list: list[tuple[float, float, float]] = []
        norm_list: list[tuple[float, float, float]] = []
        uv_list: list[tuple[float, float]] = []
        jnt_list: list[list[int]] = []
        wt_list: list[list[float]] = []
        tri_indices: list[int] = []

        poly_start = 0
        for pvi, raw_vi in enumerate(poly_idx):
            is_end = raw_vi < 0
            if is_end:
                poly_end = pvi + 1
                # Fan triangulate poly_start..poly_end
                corners = []
                for k in range(poly_start, poly_end):
                    vi = poly_idx[k]
                    if vi < 0:
                        vi = ~vi
                    nx = float(raw_norms[k * 3])
                    ny = float(raw_norms[k * 3 + 1])
                    nz = float(raw_norms[k * 3 + 2])
                    nlen = math.sqrt(nx * nx + ny * ny + nz * nz) or 1.0
                    nx, ny, nz = nx / nlen, ny / nlen, nz / nlen
                    ui = uv_indices[k]
                    u = float(raw_uvs[ui * 2])
                    v = 1.0 - float(raw_uvs[ui * 2 + 1])
                    key = (
                        vi,
                        round(nx, 4),
                        round(ny, 4),
                        round(nz, 4),
                        round(u, 5),
                        round(v, 5),
                    )
                    idx = vert_map.get(key)
                    if idx is None:
                        idx = len(pos_list)
                        vert_map[key] = idx
                        pos_list.append(
                            (
                                float(raw_verts[vi * 3]),
                                float(raw_verts[vi * 3 + 1]),
                                float(raw_verts[vi * 3 + 2]),
                            )
                        )
                        norm_list.append((nx, ny, nz))
                        uv_list.append((u, v))
                        jnt_list.append(v_joints[vi])
                        wt_list.append(v_weights[vi])
                    corners.append(idx)
                for t in range(1, len(corners) - 1):
                    tri_indices.extend((corners[0], corners[t], corners[t + 1]))
                poly_start = poly_end

        pos_arr = np.array(pos_list, dtype="<f4")
        norm_arr = np.array(norm_list, dtype="<f4")
        uv_arr = np.array(uv_list, dtype="<f4")
        jnt_arr = np.array(jnt_list, dtype="<u1")
        wt_arr = np.array(wt_list, dtype="<f4")
        idx_arr = np.array(tri_indices, dtype="<u4")

        p_acc = add_accessor(
            push_data(pos_arr.tobytes(), 34962),
            5126,
            len(pos_arr),
            "VEC3",
            pos_arr.min(axis=0).tolist(),
            pos_arr.max(axis=0).tolist(),
        )
        n_acc = add_accessor(push_data(norm_arr.tobytes(), 34962), 5126, len(norm_arr), "VEC3")
        u_acc = add_accessor(push_data(uv_arr.tobytes(), 34962), 5126, len(uv_arr), "VEC2")
        j_acc = add_accessor(push_data(jnt_arr.tobytes(), 34962), 5121, len(jnt_arr), "VEC4")
        w_acc = add_accessor(push_data(wt_arr.tobytes(), 34962), 5126, len(wt_arr), "VEC4")
        i_acc = add_accessor(push_data(idx_arr.tobytes(), 34963), 5125, len(idx_arr), "SCALAR")

        mesh_idx = len(gltf_meshes)
        gltf_meshes.append(
            {
                "name": out_name,
                "primitives": [
                    {
                        "attributes": {
                            "POSITION": p_acc,
                            "NORMAL": n_acc,
                            "TEXCOORD_0": u_acc,
                            "JOINTS_0": j_acc,
                            "WEIGHTS_0": w_acc,
                        },
                        "indices": i_acc,
                        "material": mat_idx,
                    }
                ],
            }
        )
        mnode_idx = len(gltf_nodes)
        gltf_nodes.append({"name": out_name, "mesh": mesh_idx, "skin": 0})
        mesh_node_indices.append(mnode_idx)

    # Animations
    gltf_anims = []

    def eval_curve_1d(curve_obj, t_fbx: float, default_val: float) -> float:
        cch = {c[0]: c for c in curve_obj[2]}
        kt = cch["KeyTime"][1][0]
        kv = cch["KeyValueFloat"][1][0]
        if not kt:
            return default_val
        if t_fbx <= kt[0]:
            return float(kv[0])
        if t_fbx >= kt[-1]:
            return float(kv[-1])
        # binary search
        lo, hi = 0, len(kt) - 1
        while lo + 1 < hi:
            mid = (lo + hi) // 2
            if kt[mid] <= t_fbx:
                lo = mid
            else:
                hi = mid
        dt = kt[hi] - kt[lo]
        alpha = (t_fbx - kt[lo]) / dt if dt > 0 else 0.0
        return float(kv[lo] * (1.0 - alpha) + kv[hi] * alpha)

    for o in objs:
        if o[0] != "AnimationStack":
            continue
        raw_name = o[1][1].split("\x00")[0]
        if raw_name not in ANIM_RENAMES:
            continue
        anim_name = ANIM_RENAMES[raw_name]
        stop_ktime = 0
        for ch in o[2]:
            if ch[0] == "Properties70":
                for p in ch[2]:
                    if p[1][0] == "LocalStop":
                        stop_ktime = int(p[1][4])
        duration = max(stop_ktime / FBX_TIME_SECOND, 1.0 / 60.0)
        # Sample uniformly at 60 Hz for smooth, exact quaternion slerp with zero gimbal issues
        num_samples = max(2, int(round(duration * 60.0)) + 1)
        times = np.linspace(0.0, duration, num_samples, dtype="<f4")
        t_acc = add_accessor(
            push_data(times.tobytes()),
            5126,
            num_samples,
            "SCALAR",
            [float(times[0])],
            [float(times[-1])],
        )

        layer = [id_map[c[1][1]] for c in conns if c[1][0] == "OO" and c[1][2] == o[1][0]][0]
        # Map node_id -> { 'Lcl Translation': {d|X: curve, ...}, ... }
        node_curves: dict[int, dict[str, dict[str, object]]] = {}
        for c in conns:
            if c[1][0] == "OO" and c[1][2] == layer[1][0]:
                cn_id = c[1][1]
                targets = [tc for tc in conns if tc[1][0] == "OP" and tc[1][1] == cn_id]
                curves = [cc for cc in conns if cc[1][0] == "OP" and cc[1][2] == cn_id]
                for tc in targets:
                    mid = tc[1][2]
                    if mid not in joint_index:
                        continue
                    prop = tc[1][3]
                    cmap = node_curves.setdefault(mid, {}).setdefault(prop, {})
                    for cc in curves:
                        cmap[cc[1][3]] = id_map[cc[1][1]]

        samplers = []
        channels = []
        for mid, props in node_curves.items():
            node_idx = 1 + joint_index[mid]
            def_t, def_r, def_s = get_model_trs(id_map[mid])

            if "Lcl Translation" in props:
                cm = props["Lcl Translation"]
                arr = np.zeros((num_samples, 3), dtype="<f4")
                for si, t_sec in enumerate(times):
                    tk = float(t_sec) * FBX_TIME_SECOND
                    arr[si, 0] = eval_curve_1d(cm["d|X"], tk, def_t[0]) if "d|X" in cm else def_t[0]
                    arr[si, 1] = eval_curve_1d(cm["d|Y"], tk, def_t[1]) if "d|Y" in cm else def_t[1]
                    arr[si, 2] = eval_curve_1d(cm["d|Z"], tk, def_t[2]) if "d|Z" in cm else def_t[2]
                out_acc = add_accessor(push_data(arr.tobytes()), 5126, num_samples, "VEC3")
                s_idx = len(samplers)
                samplers.append({"input": t_acc, "interpolation": "LINEAR", "output": out_acc})
                channels.append({"sampler": s_idx, "target": {"node": node_idx, "path": "translation"}})

            if "Lcl Rotation" in props:
                cm = props["Lcl Rotation"]
                arr = np.zeros((num_samples, 4), dtype="<f4")
                prev_q = None
                for si, t_sec in enumerate(times):
                    tk = float(t_sec) * FBX_TIME_SECOND
                    rx = eval_curve_1d(cm["d|X"], tk, def_r[0]) if "d|X" in cm else def_r[0]
                    ry = eval_curve_1d(cm["d|Y"], tk, def_r[1]) if "d|Y" in cm else def_r[1]
                    rz = eval_curve_1d(cm["d|Z"], tk, def_r[2]) if "d|Z" in cm else def_r[2]
                    q = np.array(euler_xyz_quat(rx, ry, rz), dtype=np.float32)
                    if prev_q is not None and float(np.dot(prev_q, q)) < 0.0:
                        q = -q
                    prev_q = q
                    arr[si] = q
                out_acc = add_accessor(push_data(arr.tobytes()), 5126, num_samples, "VEC4")
                s_idx = len(samplers)
                samplers.append({"input": t_acc, "interpolation": "LINEAR", "output": out_acc})
                channels.append({"sampler": s_idx, "target": {"node": node_idx, "path": "rotation"}})

            if "Lcl Scaling" in props:
                cm = props["Lcl Scaling"]
                arr = np.zeros((num_samples, 3), dtype="<f4")
                for si, t_sec in enumerate(times):
                    tk = float(t_sec) * FBX_TIME_SECOND
                    arr[si, 0] = eval_curve_1d(cm["d|X"], tk, def_s[0]) if "d|X" in cm else def_s[0]
                    arr[si, 1] = eval_curve_1d(cm["d|Y"], tk, def_s[1]) if "d|Y" in cm else def_s[1]
                    arr[si, 2] = eval_curve_1d(cm["d|Z"], tk, def_s[2]) if "d|Z" in cm else def_s[2]
                out_acc = add_accessor(push_data(arr.tobytes()), 5126, num_samples, "VEC3")
                s_idx = len(samplers)
                samplers.append({"input": t_acc, "interpolation": "LINEAR", "output": out_acc})
                channels.append({"sampler": s_idx, "target": {"node": node_idx, "path": "scale"}})

        gltf_anims.append({"name": anim_name, "channels": channels, "samplers": samplers})
        print(f"  animation '{anim_name}': {duration:.3f}s ({num_samples} samples, {len(channels)} channels)")

    doc = {
        "asset": {"version": "2.0", "generator": "void-engine convert_pistol_fbx.py"},
        "scene": 0,
        "scenes": [{"nodes": [0] + mesh_node_indices}],
        "nodes": gltf_nodes,
        "meshes": gltf_meshes,
        "materials": [
            {"name": "arms"},
            {"name": "pistol_frame"},
            {"name": "pistol_slide"},
            {"name": "pistol_mag"},
        ],
        "skins": [
            {
                "name": "Armature",
                "inverseBindMatrices": ibm_acc,
                "skeleton": 1,
                "joints": joint_node_indices,
            }
        ],
        "animations": gltf_anims,
        "buffers": [{"byteLength": len(bin_buf)}],
        "bufferViews": buffer_views,
        "accessors": accessors,
    }

    json_bytes = json.dumps(doc, separators=(",", ":")).encode("utf-8")
    while len(json_bytes) % 4 != 0:
        json_bytes += b" "
    while len(bin_buf) % 4 != 0:
        bin_buf.append(0)

    total_len = 12 + 8 + len(json_bytes) + 8 + len(bin_buf)
    OUT_GLB.parent.mkdir(parents=True, exist_ok=True)
    with OUT_GLB.open("wb") as f:
        f.write(struct.pack("<III", 0x46546C67, 2, total_len))
        f.write(struct.pack("<II", len(json_bytes), 0x4E4F534A))
        f.write(json_bytes)
        f.write(struct.pack("<II", len(bin_buf), 0x004E4942))
        f.write(bin_buf)
    print(f"Wrote {OUT_GLB} ({OUT_GLB.stat().st_size / 1024:.1f} KiB, {len(gltf_meshes)} meshes, {len(joint_ids)} joints)")


if __name__ == "__main__":
    build_textures()
    convert_fbx_to_glb()
