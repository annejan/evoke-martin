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

## `dogdemo/` (WIP)

A standalone Bevy + `bevy_gaussian_splatting` executable: fly a camera around a
splat and (eventually) explode it. Rust + wgpu → Vulkan, no CUDA.

## Note on git

Splats, captures, run outputs, and the external COLMAP/Brush checkouts are
**git-ignored** (multi-GB binaries). Only source/tools are tracked.
