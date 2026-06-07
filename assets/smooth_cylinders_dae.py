#!/usr/bin/env python3
"""Replace the two low-poly cylinder <geometry> blocks in defeest.dae with a
smooth high-segment unit cylinder (radius 1 in XY, z in [-1,1]). The node
matrices in the visual scene are untouched, so placement/scale is unchanged."""
import math, re

N = 128
DAE = "defeest.dae"

def build_cylinder(n):
    pos, nrm, uv, idx = [], [], [], []
    for i in range(n):                       # side rings, smooth radial normals
        a = 2 * math.pi * i / n; c, s = math.cos(a), math.sin(a); u = i / n
        for z in (-1.0, 1.0):
            pos.append((c, s, z)); nrm.append((c, s, 0.0)); uv.append((u, 0.0 if z < 0 else 1.0))
    for i in range(n):
        b0 = 2*i; t0 = 2*i+1; b1 = 2*((i+1) % n); t1 = 2*((i+1) % n)+1
        idx += [b0, b1, t1, b0, t1, t0]
    c = len(pos); pos.append((0,0,1)); nrm.append((0,0,1)); uv.append((0.5,0.5))  # top cap
    rim = []
    for i in range(n):
        a = 2*math.pi*i/n; rim.append(len(pos))
        pos.append((math.cos(a), math.sin(a), 1.0)); nrm.append((0,0,1))
        uv.append((0.5+0.5*math.cos(a), 0.5+0.5*math.sin(a)))
    for i in range(n): idx += [c, rim[i], rim[(i+1) % n]]
    c = len(pos); pos.append((0,0,-1)); nrm.append((0,0,-1)); uv.append((0.5,0.5))  # bottom cap
    rim = []
    for i in range(n):
        a = 2*math.pi*i/n; rim.append(len(pos))
        pos.append((math.cos(a), math.sin(a), -1.0)); nrm.append((0,0,-1))
        uv.append((0.5+0.5*math.cos(a), 0.5+0.5*math.sin(a)))
    for i in range(n): idx += [c, rim[(i+1) % n], rim[i]]
    return pos, nrm, uv, idx

def fmt(vals):
    return " ".join(("%g" % x) for v in vals for x in v)

pos, nrm, uv, idx = build_cylinder(N)
V, T = len(pos), len(idx)//3

def geometry(gid, name, material):
    p_indices = " ".join(str(i) for i in idx)              # one shared index per corner
    vcount = "3 " * T
    return f'''    <geometry id="{gid}" name="{name}">
      <mesh>
        <source id="{gid}-positions">
          <float_array id="{gid}-positions-array" count="{V*3}">{fmt(pos)}</float_array>
          <technique_common>
            <accessor source="#{gid}-positions-array" count="{V}" stride="3">
              <param name="X" type="float"/>
              <param name="Y" type="float"/>
              <param name="Z" type="float"/>
            </accessor>
          </technique_common>
        </source>
        <source id="{gid}-normals">
          <float_array id="{gid}-normals-array" count="{V*3}">{fmt(nrm)}</float_array>
          <technique_common>
            <accessor source="#{gid}-normals-array" count="{V}" stride="3">
              <param name="X" type="float"/>
              <param name="Y" type="float"/>
              <param name="Z" type="float"/>
            </accessor>
          </technique_common>
        </source>
        <source id="{gid}-map-0">
          <float_array id="{gid}-map-0-array" count="{V*2}">{fmt(uv)}</float_array>
          <technique_common>
            <accessor source="#{gid}-map-0-array" count="{V}" stride="2">
              <param name="S" type="float"/>
              <param name="T" type="float"/>
            </accessor>
          </technique_common>
        </source>
        <vertices id="{gid}-vertices">
          <input semantic="POSITION" source="#{gid}-positions"/>
        </vertices>
        <polylist material="{material}" count="{T}">
          <input semantic="VERTEX" source="#{gid}-vertices" offset="0"/>
          <input semantic="NORMAL" source="#{gid}-normals" offset="0"/>
          <input semantic="TEXCOORD" source="#{gid}-map-0" offset="0" set="0"/>
          <vcount>{vcount}</vcount>
          <p>{p_indices}</p>
        </polylist>
      </mesh>
    </geometry>'''

with open(DAE) as f:
    src = f.read()

for gid, name, mat in [("Cylinder-mesh", "Cylinder", "Yellow-material"),
                       ("Cylinder_001-mesh", "Cylinder.001", "Blue-material")]:
    pat = re.compile(r'    <geometry id="' + re.escape(gid) + r'".*?</geometry>', re.DOTALL)
    new, n = pat.subn(lambda m: geometry(gid, name, mat), src)
    assert n == 1, f"expected 1 match for {gid}, got {n}"
    src = new

with open(DAE, "w") as f:
    f.write(src)
print(f"defeest.dae cylinders -> {N} segments; {V} verts, {T} tris each")
