#!/usr/bin/env bash
# ============================================================================
# fetch-demo-assets.sh — download the large external splat demo assets into
# assets/. These are Mitchell Mosure's reference clouds (the upstream
# bevy_gaussian_splatting author): a multi-view photogrammetry go-board (SH3)
# and a TRELLIS single-image→3DGS garden trellis (KHR_gaussian_splatting glb).
#
# They each exceed GitHub's 100 MB file limit, so they're gitignored, not
# committed — run this to (re)fetch them. Idempotent: re-runs resume/skip.
#
#   ./pipeline/fetch-demo-assets.sh
#   MARTIN_PLY=assets/go_trimmed.ply cargo r-sh3      # SH3 photogrammetry (glints)
#   MARTIN_GLB=assets/trellis.glb MARTIN_GLB_DIST=2.6 cargo r-sh0   # KHR splat scene
# ============================================================================
set -euo pipefail
HERE="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$HERE/assets"

fetch() {
    local url="$1" out="$DEST/$2"
    if [ -s "$out" ]; then
        echo "==> have $2 ($(du -h "$out" | cut -f1)), skipping"
    else
        echo "==> fetching $2 from $url"
        curl -fSL --retry 3 -C - "$url" -o "$out"
        echo "    got $(du -h "$out" | cut -f1)"
    fi
}

fetch https://mitchell.mosure.me/go_trimmed.ply go_trimmed.ply   # ~422 MB, SH3 go-board
fetch https://mitchell.mosure.me/trellis.glb    trellis.glb      # ~113 MB, KHR_gaussian_splatting
echo "==> demo assets ready in $DEST"
