#!/usr/bin/env python3
"""Replace the two low-poly cylinders ('discs') in defeest.glb with a single
smooth high-segment unit cylinder. Text meshes, materials and node matrices are
left untouched; both cylinder primitives are repointed at the new geometry.

Pure-stdlib GLB surgery: new vertex/index data is appended to the BIN chunk and
new bufferViews/accessors are added; the old cylinder accessors are left
orphaned (harmless unused bytes)."""
import json, struct, math, sys

N = 128  # segments around the rim (was 32)
SRC = DST = "defeest.glb"

def build_cylinder(n):
    """Unit cylinder: radius 1 in XY, z in [-1,1]. Smooth normals on the side,
    flat normals on the caps. Returns (positions, normals, uvs, indices)."""
    pos, nrm, uv, idx = [], [], [], []
    # --- side: two rings of n verts, smooth radial normals ---
    for i in range(n):
        a = 2 * math.pi * i / n
        c, s = math.cos(a), math.sin(a)
        u = i / n
        for z in (-1.0, 1.0):                # bottom then top
            pos.append((c, s, z)); nrm.append((c, s, 0.0))
            uv.append((u, 0.0 if z < 0 else 1.0))
    for i in range(n):
        b0 = 2 * i; t0 = 2 * i + 1
        b1 = 2 * ((i + 1) % n); t1 = 2 * ((i + 1) % n) + 1
        idx += [b0, b1, t1,  b0, t1, t0]     # two CCW tris, outward
    base = len(pos)
    # --- top cap (z=+1), flat normal +Z, fan from center ---
    cidx = len(pos); pos.append((0.0, 0.0, 1.0)); nrm.append((0.0, 0.0, 1.0)); uv.append((0.5, 0.5))
    rim = []
    for i in range(n):
        a = 2 * math.pi * i / n
        rim.append(len(pos))
        pos.append((math.cos(a), math.sin(a), 1.0)); nrm.append((0.0, 0.0, 1.0))
        uv.append((0.5 + 0.5 * math.cos(a), 0.5 + 0.5 * math.sin(a)))
    for i in range(n):
        idx += [cidx, rim[i], rim[(i + 1) % n]]
    # --- bottom cap (z=-1), flat normal -Z ---
    cidx = len(pos); pos.append((0.0, 0.0, -1.0)); nrm.append((0.0, 0.0, -1.0)); uv.append((0.5, 0.5))
    rim = []
    for i in range(n):
        a = 2 * math.pi * i / n
        rim.append(len(pos))
        pos.append((math.cos(a), math.sin(a), -1.0)); nrm.append((0.0, 0.0, -1.0))
        uv.append((0.5 + 0.5 * math.cos(a), 0.5 + 0.5 * math.sin(a)))
    for i in range(n):
        idx += [cidx, rim[(i + 1) % n], rim[i]]
    return pos, nrm, uv, idx

# --- read GLB ---
with open(SRC, "rb") as f:
    data = f.read()
magic, ver, total = struct.unpack("<III", data[:12])
off = 12
jlen, jtype = struct.unpack("<II", data[off:off+8]); off += 8
js = json.loads(data[off:off+jlen]); off += jlen
blen, btype = struct.unpack("<II", data[off:off+8]); off += 8
bindata = bytearray(data[off:off+blen])

pos, nrm, uv, idx = build_cylinder(N)

def pad4(buf):
    while len(buf) % 4: buf.append(0)

# pack new data, 4-byte aligned bufferViews
pos_off = len(bindata)
for v in pos: bindata += struct.pack("<3f", *v)
nrm_off = len(bindata)
for v in nrm: bindata += struct.pack("<3f", *v)
uv_off = len(bindata)
for v in uv: bindata += struct.pack("<2f", *v)
pad4(bindata)
idx_off = len(bindata)
for i in idx: bindata += struct.pack("<H", i)
pad4(bindata)

bvs = js["bufferViews"]; accs = js["accessors"]
ARRAY_BUFFER, ELEMENT_ARRAY_BUFFER = 34962, 34963
def add_bv(byte_off, byte_len, target):
    bvs.append({"buffer": 0, "byteOffset": byte_off, "byteLength": byte_len, "target": target})
    return len(bvs) - 1
bv_pos = add_bv(pos_off, len(pos)*12, ARRAY_BUFFER)
bv_nrm = add_bv(nrm_off, len(nrm)*12, ARRAY_BUFFER)
bv_uv  = add_bv(uv_off,  len(uv)*8,  ARRAY_BUFFER)
bv_idx = add_bv(idx_off, len(idx)*2, ELEMENT_ARRAY_BUFFER)

xs=[v[0] for v in pos]; ys=[v[1] for v in pos]; zs=[v[2] for v in pos]
def add_acc(bv, comp, typ, count, mn=None, mx=None):
    a = {"bufferView": bv, "componentType": comp, "count": count, "type": typ}
    if mn is not None: a["min"] = mn; a["max"] = mx
    accs.append(a); return len(accs) - 1
a_pos = add_acc(bv_pos, 5126, "VEC3", len(pos), [min(xs),min(ys),min(zs)], [max(xs),max(ys),max(zs)])
a_nrm = add_acc(bv_nrm, 5126, "VEC3", len(nrm))
a_uv  = add_acc(bv_uv,  5126, "VEC2", len(uv))
a_idx = add_acc(bv_idx, 5123, "SCALAR", len(idx))

# repoint both cylinder meshes at the new shared geometry
n_patched = 0
for m in js["meshes"]:
    if "Cylinder" in m.get("name", ""):
        p = m["primitives"][0]
        p["attributes"] = {"POSITION": a_pos, "NORMAL": a_nrm, "TEXCOORD_0": a_uv}
        p["indices"] = a_idx
        n_patched += 1
js["buffers"][0]["byteLength"] = len(bindata)
assert n_patched == 2, f"expected 2 cylinders, patched {n_patched}"

# --- write GLB ---
jbytes = json.dumps(js, separators=(",", ":")).encode()
while len(jbytes) % 4: jbytes += b" "
while len(bindata) % 4: bindata.append(0)
out = bytearray()
out += struct.pack("<III", 0x46546C67, 2, 12 + 8 + len(jbytes) + 8 + len(bindata))
out += struct.pack("<II", len(jbytes), 0x4E4F534A) + jbytes
out += struct.pack("<II", len(bindata), 0x004E4942) + bindata
with open(DST, "wb") as f: f.write(out)
print(f"patched {n_patched} cylinders -> {N} segments; {len(pos)} verts, {len(idx)//3} tris each")
