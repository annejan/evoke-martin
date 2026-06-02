# martin — CUDA-free Gaussian Splatting on AMD / openSUSE

Tooling for building 3D Gaussian splats **without CUDA or ROCm** — everything
runs on CPU + Vulkan (Mesa RADV), targeting an AMD Ryzen AI 7 PRO 350 /
Radeon 860M (gfx1152) on openSUSE Tumbleweed.

## Tools

| Script | What it does |
|---|---|
| `splat-setup.sh` | One-time: installs COLMAP build deps via `zypper`, builds **COLMAP** (CUDA off) and **Brush** (wgpu/Vulkan), symlinks `~/.local/bin/brush`. |
| `splat.sh` | Pipeline: `video \| image-dir` → ffmpeg frames → COLMAP CPU SfM + undistort → **Brush** training → `.ply`. |

### Usage

```bash
./splat-setup.sh                 # once
./splat.sh my_video.mp4          # or:  ./splat.sh ./photos/
VIEWER=1 ./splat.sh ./photos/    # watch training live in Brush's window
```

Tunables (env): `FPS`, `MAX_SIZE`, `EXPORT_EVERY`, `VIEWER`.
View / clean / compress the resulting `.ply` at <https://superspl.at/editor>.

## `dogdemo/` — standalone splat demo (Bevy + Vulkan, no CUDA)

A standalone executable that loads a `.ply` Gaussian splat, orbits a camera
around it, and **explodes it** — each Gaussian flies apart via a closed-form
ballistic displacement injected into the renderer's WGSL (vendored fork in
`dogdemo/vendor/`), with HDR bloom on black. Built on Bevy 0.18 +
`bevy_gaussian_splatting` 7.0.1, wgpu → Vulkan (nightly toolchain, pinned).

```bash
cd dogdemo && cargo run            # window: orbiting splat
#   ↑/↓ zoom · ←/→ raise/lower · Space = detonate / reset
./record.sh                        # render the explosion to ./aegg_explosion.mp4
```

The splat loads from `dogdemo/assets/aegg.ply` (symlink to the project-root
`.ply`). **Export uncompressed/standard PLY from SuperSplat** — the loader
rejects SuperSplat's *compressed* format (`missing required properties`).
Linux build deps: `systemd-devel` (libudev) + alsa (and a Vulkan/RADV driver).

## Note on git

Splats, captures, run outputs, and the external COLMAP/Brush checkouts are
**git-ignored** (multi-GB binaries). Only source/tools are tracked.
