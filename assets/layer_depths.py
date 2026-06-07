#!/usr/bin/env python3
"""Make the four logo parts a uniform Z-thickness and stack them in even steps.
Only the Z scale + Z translation of each node matrix change; X/Y is untouched
(world X/Y don't read those matrix elements). Patches both defeest.glb and
defeest.dae so the live .dae and the canonical .glb stay in sync."""
import json, struct, re

# Concentric layers all CENTRED on z=0, so the object is mirror-symmetric: the
# same logo reads from the front AND the back (the parts are solid slabs with
# two caps, so no duplicate geometry is needed). Each layer is progressively
# thicker - yellow thinnest, then blue, then the letters - so its caps step out
# 0.06 ahead of the next on both faces (a shared cap plane would blend to mush).
# local depth: cylinders span z[-1,1] -> 2.0 ; text z[-0.11,0.11] -> 0.22
PARTS = {
    # node name : (front_z, thickness, sign, local_depth)   centre = front + thickness/2 = 0
    "de":         (-0.18, 0.36, +1, 0.22),   # letters, thickest, outermost caps
    "FEEST":      (-0.18, 0.36, +1, 0.22),
    "Blue_cil":   (-0.12, 0.24, +1, 2.0),    # middle plate
    "Yellow_cil": (-0.03, 0.06, -1, 2.0),    # base plate, thinnest, innermost (keep neg sign)
}

def scale_for(name):
    front, thick, sign, local = PARTS[name]
    scale_z = sign * (thick / local)
    center_z = front + thick / 2.0
    return scale_z, center_z

# ---- defeest.glb (node.matrix is column-major in the JSON chunk) ----
with open("defeest.glb", "rb") as f:
    data = f.read()
off = 12
jl, _ = struct.unpack("<II", data[off:off+8]); off += 8
js = json.loads(data[off:off+jl]); rest = data[off+jl:]
for n in js["nodes"]:
    nm = n.get("name")
    if nm in PARTS:
        m = n["matrix"]
        sz, tz = scale_for(nm)
        m[10] = sz   # column 2, row 2 = Z scale
        m[14] = tz   # column 3, row 2 = Z translation
jb = json.dumps(js, separators=(",", ":")).encode()
while len(jb) % 4: jb += b" "
out = bytearray()
head = 12 + 8 + len(jb) + len(rest)
out += struct.pack("<III", 0x46546C67, 2, head)
out += struct.pack("<II", len(jb), 0x4E4F534A) + jb
out += rest
with open("defeest.glb", "wb") as f:
    f.write(out)

# ---- defeest.dae (matrix is row-major text: idx 10 = Z scale, 11 = Z trans) ----
with open("defeest.dae") as f:
    dae = f.read()
for nm in PARTS:
    sz, tz = scale_for(nm)
    # find  <node id="NM" ...> ... <matrix ...>v0 .. v15</matrix>
    pat = re.compile(r'(<node id="' + re.escape(nm) + r'"[^>]*>\s*<matrix[^>]*>)([^<]+)(</matrix>)')
    def repl(mo):
        vals = mo.group(2).split()
        vals[10] = "%g" % sz
        vals[11] = "%g" % tz
        return mo.group(1) + " ".join(vals) + mo.group(3)
    dae, n = pat.subn(repl, dae)
    assert n == 1, f"{nm}: matched {n} nodes in dae"
with open("defeest.dae", "w") as f:
    f.write(dae)

print("stacked layers (front->back):",
      ", ".join(f"{n}=thick{PARTS[n][1]}" for n in PARTS))
