#!/usr/bin/env bash
# ============================================================================
# mesh-splat.sh — a textured MESH -> a "proper" 3D Gaussian Splat, all offline,
# all-AMD (Blender render + Brush train, no CUDA).
#
#   Blender (EEVEE) renders the model from ~150 orbital views with KNOWN poses
#   -> a synthetic transforms.json dataset -> Brush trains it on Vulkan -> .ply.
#
# Because the poses are exact (we placed every camera), there's no COLMAP step
# and no SfM failure mode — the trainer just fits the splat to clean renders.
# This is the "bake offline, ship a cheap .ply" path: far better than martin's
# in-engine mesh sampler (src/mesh.rs) when a mesh really matters.
#
# Usage:   ./mesh-splat.sh <mesh> [workspace-dir]
# Example: ./mesh-splat.sh model.obj
#          ./mesh-splat.sh badge.dae ./badge_run
#
# Tunables (env vars):
#   VIEWS=150          camera viewpoints (more = cleaner, slower)
#   RES=800            square render resolution (px)
#   SAMPLES=48         Cycles samples per render (denoised; lower = faster)
#   ITERS=15000        Brush training iterations
#   SH_DEGREE=0        spherical-harmonic degree of the output (0 = matches martin's sh0 build)
#   MAX_SPLATS=        cap the splat count (Brush --max-splats; empty = default)
#   EXPORT_EVERY=5000  how often Brush writes a .ply checkpoint
#   BLENDER=blender-5.0   the Blender binary
#   VIEWER=1           open Brush's live training window
#
# Rendering runs on Cycles/CPU (headless-safe — EEVEE needs a GL context that a
# windowless box doesn't have). COLLADA .dae / other non-native formats are
# auto-converted to .glb with `assimp` first.
# ============================================================================
set -euo pipefail

MESH="${1:?Usage: ./mesh-splat.sh <mesh.obj|.dae|.stl|.ply|.glb> [workspace-dir]}"
WORK="${2:-./mesh_splat_run}"
VIEWS="${VIEWS:-150}"
RES="${RES:-800}"
SAMPLES="${SAMPLES:-48}"
ITERS="${ITERS:-15000}"
SH_DEGREE="${SH_DEGREE:-0}"
EXPORT_EVERY="${EXPORT_EVERY:-5000}"
BLENDER="${BLENDER:-blender-5.0}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

[ -f "$MESH" ] || { echo "mesh not found: $MESH"; exit 1; }
command -v "$BLENDER" >/dev/null || { echo "$BLENDER not found (set BLENDER=… to your Blender binary)"; exit 1; }
command -v brush      >/dev/null || { echo "brush not found — run ./pipeline/splat-setup.sh (and put ~/.local/bin on PATH)"; exit 1; }

mkdir -p "$WORK"

echo "==> [Blender] rendering $VIEWS orbital views @ ${RES}px (Cycles/CPU, ${SAMPLES} samples, transparent film)"
SAMPLES="$SAMPLES" "$BLENDER" -b -P "$SCRIPT_DIR/render_orbit.py" -- "$MESH" "$WORK" "$VIEWS" "$RES"
[ -f "$WORK/transforms.json" ] || { echo "ERROR: Blender wrote no transforms.json"; exit 1; }

# Brush resolves a RELATIVE --export-path against the dataset's parent, so make it absolute.
EXPORT_DIR="$(cd "$WORK" && pwd)/exports"
echo "==> [Brush] training on Vulkan (Radeon 860M / RADV) — known poses, no COLMAP"
echo "    (sh-degree $SH_DEGREE for martin's sh0 loader; .ply every ${EXPORT_EVERY} steps -> $EXPORT_DIR/)"
ARGS=(--export-path "$EXPORT_DIR/" --export-every "$EXPORT_EVERY" --total-train-iters "$ITERS" --sh-degree "$SH_DEGREE")
[ -n "${MAX_SPLATS:-}" ] && ARGS+=(--max-splats "$MAX_SPLATS")
[ "${VIEWER:-0}" = "1" ] && { ARGS+=(--with-viewer); echo "    VIEWER=1 -> opening live training window"; }
brush "$WORK" "${ARGS[@]}"

echo
echo "============================================================"
echo "DONE.  Splat files: $EXPORT_DIR/*.ply"
echo "Drop the newest .ply into martin:  MARTIN_PLY=…/exports/<file>.ply cargo +nightly run --release"
echo "(Brush exports Y-up + centred — use MARTIN_ROT=180,0,0 (or similar) to stand it upright if needed.)"
echo "View / clean / compress: https://superspl.at/editor"
echo "============================================================"
