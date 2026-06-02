# dogdemo — usage & env vars

`dogdemo` loads Gaussian splats and flies a camera around them while they morph,
explode, and reassemble — driven entirely by environment variables. There's no
config file: you compose effects by combining env vars on the command line.

```bash
cd dogdemo
cargo +nightly run --release        # nightly toolchain is pinned (rust-toolchain.toml)
```

With no env vars it loads `assets/aegg.ply` and orbits/explodes it.

---

## Modes at a glance

The demo picks a mode from the env vars, in this **precedence order** (first match wins):

| If you set… | Mode |
|---|---|
| `DOGDEMO_SEQ` | **Sequence** — a timeline of beats that morph into one another (the big one) |
| `DOGDEMO_TEXT` | **Splat-text** — a title assembles out of a ball cloud |
| `DOGDEMO_REFORM` | **Morph** — the source splat(s) turn into a target splat |
| `DOGDEMO_PLY2` (no reform) | **Two splats** collapse inward together |
| *(nothing)* | **Single splat** explodes inward |

Examples:

```bash
# Single splat (your own .ply)
DOGDEMO_PLY=~/Projects/martin/martin.ply cargo +nightly run --release

# Two Martins morph into a dog
DOGDEMO_PLY=~/Projects/martin/martin-peace.ply DOGDEMO_PLY2=martin.ply \
DOGDEMO_REFORM=doggo.ply cargo +nightly run --release

# A glowing title that assembles from particles
DOGDEMO_TEXT="MARTIN GAUS" cargo +nightly run --release

# A whole show (see "Sequences" below)
DOGDEMO_PLY=~/Projects/martin/doggo.ply \
DOGDEMO_SEQ="text:MARTIN GAUS; splat:doggo.ply; text:GREETINGS; text:CODE ANNEJAN" \
cargo +nightly run --release
```

---

## Where files are loaded from

`DOGDEMO_PLY` takes an **absolute path**, and its **parent folder becomes the asset
root**. Every other splat reference (`DOGDEMO_PLY2`, `DOGDEMO_REFORM`, and `splat:` beats
in a sequence) is then just a **filename in that same folder**:

```bash
DOGDEMO_PLY=/home/you/splats/martin.ply   # → asset root = /home/you/splats
DOGDEMO_PLY2=martin-peace.ply              # → /home/you/splats/martin-peace.ply
DOGDEMO_REFORM=doggo.ply                   # → /home/you/splats/doggo.ply
```

In **sequence mode** the splat referenced by `DOGDEMO_PLY` isn't shown; it's only used to
set the asset root, so point it at any `.ply` in the folder your beats live in.

> **Export uncompressed / standard PLY** (e.g. from [SuperSplat](https://superspl.at/editor)).
> The loader rejects SuperSplat's *compressed* format (`missing required properties`).

---

## Full env var reference

| Env var | Default | What it does |
|---|---|---|
| `DOGDEMO_PLY` | `assets/aegg.ply` | Primary splat (absolute path); sets the asset folder for the rest. |
| `DOGDEMO_PLY2` | — | A second splat, placed beside the first. |
| `DOGDEMO_REFORM` | — | Morph target: the source splat(s) turn into this one. |
| `DOGDEMO_TEXT` | — | Splat-text: this string assembles out of a ball cloud (glowing). |
| `DOGDEMO_SEQ` | — | A timeline of beats (see [Sequences](#sequences)). Highest precedence. |
| `DOGDEMO_BULGE` | `0.9` | Ball-cloud size at a morph's midpoint, in object-radii. `0` = clean "puzzle-box" reorder (no explosion); `~0.9` = a ball roughly the object's size. (In sequences this is the per-beat 3rd timing number instead.) |
| `DOGDEMO_MORPH_COUNT` | `0` (morph) / `200000` (seq) | Gaussian budget. `0` = max input count (~1.15M, crisp, ~20 fps on the iGPU). Lower = faster: **250k ≈ locked 60 fps, 500k ≈ 40 fps.** |
| `DOGDEMO_YAW` | — (gentle sway) | Pin the camera to a fixed orbit angle in **radians** (e.g. `1.57` ≈ head-on). Handy for inspecting a splat. |
| `DOGDEMO_FPS` | off | `=1` logs smoothed FPS / frame-time (and the morph clock) every ~0.5 s. |
| `DOGDEMO_RECORD` | — | Directory to dump one PNG per frame into (used by `record.sh`). |
| `DOGDEMO_FRAMES` | `220` | Frames to record (non-sequence modes). `record.sh` overrides via `FRAMES=`. Sequences compute their own length. |
| `DOGDEMO_SHOT` | — | Capture a single headless screenshot to this path, then exit ~2 s later. |
| `DOGDEMO_SHOT_AT` | `4.5` | When (seconds) to take the `DOGDEMO_SHOT`. |
| `DOGDEMO_EXPLODE` | off | `=1` auto-triggers the explosion/morph at t≈2 s (so headless captures don't need a keypress). |

---

## Live keyboard controls

When running in a window (not recording):

| Key | Action |
|---|---|
| `Space` | Trigger / reset the explosion or morph |
| `↑` / `↓` | Zoom in / out |
| `←` / `→` | Lower / raise the camera |

The camera only **sways across the front** of the subject — single-image splats (e.g.
from TRELLIS) have a hollow back, so a full 360° orbit would show the inside of the head.
Use `DOGDEMO_YAW` to inspect a fixed angle. Splats captured from all sides (COLMAP→Brush)
can be orbited freely.

---

## Sequences

`DOGDEMO_SEQ` is the composable mode: a list of **beats** that morph into one another,
each transition flowing through a ball cloud. It's either a `;`-separated string **or a
path to a file** with one beat per line (`#` starts a comment, blank lines are skipped).

**Beat grammar:**

```
text:STRING                      # splat-text (glowing)
splat:name.ply                   # a splat (filename in the asset folder)
splat:a.ply+b.ply                # several splats, auto-arranged side by side
…any of the above… @hold,morph,bulge
```

The optional trailing `@hold,morph,bulge` sets, in **seconds** (and ball amount):
- **hold** — how long to rest on this beat once it arrives (default `1.5`)
- **morph** — how long the morph *into* this beat takes (default `3.0`)
- **bulge** — ball-cloud explosiveness of that morph, `0`–`~1.4` (default `0.9`)

(For the first beat, `morph`/`bulge` are ignored — there's nothing to morph from.)

**Inline example — a full show:**

```bash
DOGDEMO_PLY=~/Projects/martin/doggo.ply \
DOGDEMO_SEQ="text:MARTIN GAUS @2,2.5,0; splat:doggo.ply @2,3,0.9; text:GREETINGS @1.5,2.5,0.9; text:DEFEEST CINDER @1.5,2.5,0.7; text:CODE ANNEJAN @2,2.5,0.6" \
cargo +nightly run --release
```

**File example** — put this in `show.seq`:

```
# Martin Gaus — Evoke
text:MARTIN GAUS @2.5,3,0
splat:martin.ply+martin-peace.ply @2,3,0.6   # the two Martins, side by side
splat:doggo.ply @2,3.5,0.9                    # …become the dog
text:GREETINGS @1.5,2.5,0.9
text:CODE ANNEJAN @2.5,3,0.6
```

…and run it:

```bash
DOGDEMO_PLY=~/Projects/martin/doggo.ply DOGDEMO_SEQ=~/show.seq cargo +nightly run --release
```

All beats are resampled to one gaussian count (`DOGDEMO_MORPH_COUNT`, default 200k in
sequences) and the camera is framed once over everything, so it never pops between beats.

---

## Recording to video

`record.sh` (in the repo root) builds the demo, renders frames headlessly, and runs
ffmpeg. It inherits all the `DOGDEMO_*` env vars:

```bash
# from the repo root
DOGDEMO_PLY=~/Projects/martin/doggo.ply \
DOGDEMO_SEQ="text:MARTIN GAUS; splat:doggo.ply; text:CODE ANNEJAN" \
./record.sh my_show.mp4
```

For the non-sequence modes, `FRAMES=420 ./record.sh out.mp4` sets the clip length;
sequences compute their own duration from the beats.

To grab a single still instead:

```bash
DOGDEMO_TEXT="MARTIN GAUS" DOGDEMO_EXPLODE=1 \
DOGDEMO_SHOT=/tmp/title.png DOGDEMO_SHOT_AT=6 cargo +nightly run --release
```

---

## Performance notes (Radeon 860M iGPU, Vulkan)

It's fill-rate bound and the depth sort scales with gaussian count:

| `DOGDEMO_MORPH_COUNT` | Frame rate |
|---|---|
| `250000` | locked 60 fps |
| `500000` | ~40 fps |
| `0` (max, ~1.15M) | ~20 fps — crisp, best for offline video / a beefier machine |

Use the lower counts for a smooth **live** demo and `0` for the final **rendered** video.
Run `--release`: the debug build is for fast iteration only.
