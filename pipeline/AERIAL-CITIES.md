<!--
SPDX-FileCopyrightText: 2026 Anne Jan Brouwer
SPDX-License-Identifier: MIT
-->
# Aerial cities → Gaussian splats → flythrough demo

Turn a **street address** into a 3D-Gaussian-splat flythrough, entirely on a CUDA-free
AMD box. Google's [Aerial View API](https://developers.google.com/maps/documentation/aerial-view/generate-video)
renders a cinematic orbit video of the address; that video goes straight through the
existing photogrammetry pipeline (COLMAP → Brush) into a splat, which martin flies a
camera through. The "Austin" reference demo (`renders/austin_fly.mp4`) was built exactly
this way from `600 Montgomery St`-style US addresses.

## ⚠️ Read this first — what third parties may and may not do

The **scripts in `pipeline/` are MIT** — reuse them freely.

The **output is not yours to redistribute.** A splat or video derived from Google imagery
is Google Maps Content: their terms forbid downloading/caching, derivative works, and
redistribution (see the project notes — verified across the Aerial View + Maps Platform
terms; the EEA terms are stricter still). So:

- ✅ **Reproduce it yourself** with your own API key — generate, view, experiment **locally**.
- ❌ **Do not ship** the `.ply`, the `*_fly.mp4`, or a bundled binary containing them. They
  are git-ignored here for exactly this reason; keep it that way.
- 🪧 If you ever display a frame, Google requires the **"Imagery ©Google"** attribution.
- 🎯 Want something **shippable**? Run the *same* pipeline on your **own footage** (a phone
  orbit) or **CC0/CC-BY** photogrammetry / open data (e.g. NL: [3DBAG](https://3dbag.nl)).
  Identical aesthetic, clean SPDX tag, zero exposure.

This file is the *recipe* so others can rebuild the demo — not a license to pass the asset around.

## Prerequisites

1. **Splat toolchain** — `./pipeline/splat-setup.sh` (builds CPU COLMAP + Vulkan Brush).
2. **sh3 martin build** — `cargo b-sh3` (captures are degree-3; the default sh0 build renders them black).
3. **Google Cloud key** with the **Aerial View API** enabled + billing on the project
   (the API itself is free; billing is just the gate). See the project's key-safety notes:
   use a throwaway project, restrict the key to Aerial View, set a billing cap.

## One city, by hand

```bash
# 1. address → cinematic MP4 (US addresses only; new ones render 1–several hours, async)
GOOGLE_MAPS_API_KEY=… ./pipeline/fetch-aerial.sh "233 S Wacker Dr, Chicago, IL 60606" chicago.mp4

# 2. MP4 → splat → cropped dense core → flythrough clip  (renders/chicago_fly.mp4)
./pipeline/city-splat.sh chicago.mp4 chicago
```

`city-splat.sh` runs `splat.sh` (COLMAP+Brush), then `crop-splat.py` (drops the NaN/inf
splat + sky/ground floaters + oversized spike gaussians → a clean dense core), then renders
a generic dive→low-pass→climb flythrough headless. Tunables: `TRAIN_ITERS` (Brush length,
default 12000 — captures plateau ~16k), `KEEP_PCT`/`OPACITY_MIN`/`SCALE_PCT` (crop tightness),
`FLY_SECS`, `FLY_JSON` (hand-tuned camera path in normalized space, cloud ≈ [-1,1], y up).

## A whole demo, unattended

```bash
GOOGLE_MAPS_API_KEY=… ./pipeline/overnight-demo.sh
```

Fetches the cities listed at the top of the script (parallel server-side renders), splats
each serially on the one iGPU, reuses Austin if already built, and stitches every clip
(+ the martin synth soundtrack) into `renders/demo.mp4`. Resilient: a city with no Aerial
coverage or a failed step is skipped and the demo is built from whatever succeeded.

## Why the gotchas exist (so the next city Just Works)

- **Black render** = a single non-finite (`NaN`/`inf`) splat poisons martin's `normalize_to`
  centroid → the whole cloud moves to NaN → nothing draws. `crop-splat.py` removes it; the
  engine should also skip non-finite splats (see project TODO).
- **Spiky white glint** = oversized elongated gaussians (floaters), *not* sh3 view-dependence.
  The `SCALE_PCT` crop drops them.
- **Melted close-ups** = Aerial View is itself a render of Google's photogrammetry mesh, so
  you're doing SfM on a render — quality is capped. Fine for set-dressing / fast flythroughs,
  not for inspecting a single building. Keep the camera clear of the buildings.
