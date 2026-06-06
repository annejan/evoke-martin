#!/usr/bin/env python3
# ============================================================================
# gen-demo-splats.py — generate a batch of procedural Gaussian-splat .ply files
# (clean, colourful shapes) to showcase martin's morph / swarm / deform engine.
# They all morph beautifully into one another (similar counts, vivid colours).
#
# Usage:  python3 pipeline/gen-demo-splats.py [out_dir] [points_per_shape]
#         (default: assets/demo  140000)
#
# Output is martin's sh0 .ply layout (the format Brush exports / martin loads):
#   x y z  scale_0..2 (log)  opacity (logit)  rot_0..3 (wxyz)  f_dc_0..2 (SH0).
# ============================================================================
import sys
import os
import numpy as np

OUT = sys.argv[1] if len(sys.argv) > 1 else "assets/demo"
N = int(sys.argv[2]) if len(sys.argv) > 2 else 140_000
os.makedirs(OUT, exist_ok=True)
rng = np.random.default_rng(0xDEFEE5)

SPLAT = 0.02       # splat radius (shapes span ~±1)
ALPHA = 0.92       # opacity


def hsv(h, s, v):
    """h,s,v in [0,1] arrays -> rgb (N,3)."""
    h = (h % 1.0) * 6.0
    i = np.floor(h).astype(int)
    f = h - i
    p, q, t = v * (1 - s), v * (1 - s * f), v * (1 - s * (1 - f))
    i = i % 6
    r = np.choose(i, [v, q, p, p, t, v])
    g = np.choose(i, [t, v, v, q, p, p])
    b = np.choose(i, [p, p, t, v, v, q])
    return np.stack([r, g, b], 1)


def write_ply(name, pos, rgb):
    pos = pos.astype(np.float32)
    n = len(pos)
    scale = np.full((n, 3), np.log(SPLAT), np.float32)
    opacity = np.full((n, 1), np.log(ALPHA / (1 - ALPHA)), np.float32)
    rot = np.tile(np.array([1, 0, 0, 0], np.float32), (n, 1))  # identity wxyz
    f_dc = ((rgb - 0.5) / 0.2820948).astype(np.float32)        # SH degree-0 dc
    data = np.concatenate([pos, scale, opacity, rot, f_dc], 1).astype("<f4")
    header = (
        "ply\nformat binary_little_endian 1.0\n"
        f"comment martin demo splat: {name}\n"
        f"element vertex {n}\n"
        "property float x\nproperty float y\nproperty float z\n"
        "property float scale_0\nproperty float scale_1\nproperty float scale_2\n"
        "property float opacity\n"
        "property float rot_0\nproperty float rot_1\nproperty float rot_2\nproperty float rot_3\n"
        "property float f_dc_0\nproperty float f_dc_1\nproperty float f_dc_2\n"
        "end_header\n"
    ).encode("ascii")
    with open(os.path.join(OUT, name + ".ply"), "wb") as f:
        f.write(header)
        f.write(data.tobytes())
    print(f"  {name}.ply  ({n} splats)")


def sphere():
    d = rng.normal(size=(N, 3))
    d /= np.linalg.norm(d, axis=1, keepdims=True)
    r = rng.random(N) ** (1 / 3)
    pos = d * r[:, None]
    rgb = hsv((np.arctan2(d[:, 2], d[:, 0]) / (2 * np.pi)) + 0.5, 0.85, 1.0)
    return pos, rgb


def cube():
    pos = rng.uniform(-1, 1, (N, 3))
    rgb = (pos + 1) / 2  # position-as-colour: a cube of the RGB gamut
    return pos, rgb


def torus():
    u = rng.uniform(0, 2 * np.pi, N)
    v = rng.uniform(0, 2 * np.pi, N)
    R, r = 0.72, 0.3
    pos = np.stack([(R + r * np.cos(v)) * np.cos(u),
                    r * np.sin(v),
                    (R + r * np.cos(v)) * np.sin(u)], 1)
    return pos, hsv(u / (2 * np.pi), 0.9, 1.0)


def helix():
    t = rng.uniform(0, 6 * np.pi, N)
    strand = rng.integers(0, 2, N)
    phase = strand * np.pi
    jit = rng.normal(0, 0.03, (N, 3))
    pos = np.stack([0.45 * np.cos(t + phase),
                    t / (3 * np.pi) - 1.0,
                    0.45 * np.sin(t + phase)], 1) + jit
    rgb = np.where(strand[:, None] == 0, np.array([0.1, 0.9, 1.0]), np.array([1.0, 0.2, 0.8]))
    return pos, rgb


def galaxy():
    arms = 3
    r = rng.random(N) ** 0.7
    arm = rng.integers(0, arms, N)
    theta = arm * (2 * np.pi / arms) + r * 5.0 + rng.normal(0, 0.25, N)
    pos = np.stack([r * np.cos(theta),
                    rng.normal(0, 0.04, N) * (1.2 - r),
                    r * np.sin(theta)], 1)
    rgb = hsv(0.6 + r * 0.35, 0.8, 1.0)  # warm core -> cool rim
    return pos, rgb


def star():
    spikes = rng.normal(size=(24, 3))
    spikes /= np.linalg.norm(spikes, axis=1, keepdims=True)
    pick = rng.integers(0, 24, N)
    r = rng.random(N) ** 0.5
    perp = rng.normal(0, 1, (N, 3)) * (0.12 * (1 - r))[:, None]
    pos = spikes[pick] * r[:, None] + perp
    return pos, hsv(0.05 + r * 0.12, 0.95, 1.0)  # yellow core -> red tips


def wave():
    x = rng.uniform(-1, 1, N)
    z = rng.uniform(-1, 1, N)
    y = 0.35 * np.sin(3.2 * x) * np.cos(3.2 * z)
    pos = np.stack([x, y, z], 1)
    return pos, hsv(0.55 + y, 0.85, 1.0)


def ring():
    u = rng.uniform(0, 2 * np.pi, N)
    v = rng.uniform(0, 2 * np.pi, N)
    R, r = 0.92, 0.08
    pos = np.stack([(R + r * np.cos(v)) * np.cos(u),
                    r * np.sin(v),
                    (R + r * np.cos(v)) * np.sin(u)], 1)
    return pos, hsv(u / (2 * np.pi), 1.0, 1.0)


def knot():
    # trefoil knot tube
    t = rng.uniform(0, 2 * np.pi, N)
    pos = np.stack([np.sin(t) + 2 * np.sin(2 * t),
                    np.cos(t) - 2 * np.cos(2 * t),
                    -np.sin(3 * t)], 1) / 3.2
    pos += rng.normal(0, 0.035, (N, 3))  # tube thickness
    return pos, hsv(t / (2 * np.pi), 0.9, 1.0)


def mobius():
    u = rng.uniform(0, 2 * np.pi, N)
    half = rng.uniform(-1, 1, N) * 0.4
    pos = np.stack([(1 + half * np.cos(u / 2)) * np.cos(u),
                    (1 + half * np.cos(u / 2)) * np.sin(u),
                    half * np.sin(u / 2)], 1) * 0.82
    return pos, hsv(u / (2 * np.pi), 0.85, 1.0)


def supershape():
    # 3D superformula — one organic "bloom" of many lobes.
    def sf(a, m, n1, n2, n3):
        t = m * a / 4.0
        r = (np.abs(np.cos(t)) ** n2 + np.abs(np.sin(t)) ** n3 + 1e-9) ** (-1.0 / n1)
        return np.clip(np.nan_to_num(r), 0.0, 3.0)
    th = rng.uniform(-np.pi / 2, np.pi / 2, N)
    ph = rng.uniform(-np.pi, np.pi, N)
    r1 = sf(th, 7, 0.3, 1.7, 1.7)
    r2 = sf(ph, 7, 0.3, 1.7, 1.7)
    pos = np.stack([r1 * np.cos(th) * r2 * np.cos(ph),
                    r2 * np.sin(th),
                    r1 * np.cos(th) * r2 * np.sin(ph)], 1)
    pos = np.nan_to_num(pos)
    pos /= max(np.max(np.abs(pos)), 1e-6)
    return pos, hsv(0.55 + 0.4 * np.sin(2 * ph), 0.9, 1.0)


print(f"generating demo splats -> {OUT}/ ({N} each)")
for name, fn in [("sphere", sphere), ("cube", cube), ("torus", torus), ("helix", helix),
                 ("galaxy", galaxy), ("star", star), ("wave", wave), ("ring", ring),
                 ("knot", knot), ("mobius", mobius), ("supershape", supershape)]:
    write_ply(name, *fn())
print("done — morph through them with MARTIN_SEQ (see assets/demo-show.seq).")
