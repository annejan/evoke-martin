#!/usr/bin/env bash
# ============================================================================
# city-splat.sh — one aerial MP4 → a clean flythrough clip, fully unattended.
#
#   mp4 → splat.sh (COLMAP+Brush) → crop-splat.py (dense core) → sh3 flythrough render
#
# Usage:   ./pipeline/city-splat.sh <aerial.mp4> <name>
# Example: ./pipeline/city-splat.sh sf.mp4 sf
# Output:  renders/<name>_fly.mp4   +   <name>_run/exports/<name>_tight.ply
#
# Tunables (env): TRAIN_ITERS (Brush length, default 12000 for batch speed),
#   FPS (frame sample, default 4), KEEP_PCT/OPACITY_MIN/SCALE_PCT (crop), FLY_SECS,
#   PREVIEW_FPS (render fps), FLY_JSON (override the camera path).
# ============================================================================
set -euo pipefail
HERE="$(cd "$(dirname "$0")/.." && pwd)"
MP4="${1:?Usage: ./pipeline/city-splat.sh <aerial.mp4> <name>}"
NAME="${2:?need a <name>}"
[ -f "$MP4" ] || { echo "city-splat: no such mp4: $MP4"; exit 1; }

WORK="$HERE/${NAME}_run"
TIGHT="$WORK/exports/${NAME}_tight.ply"
OUT="$HERE/renders/${NAME}_fly.mp4"
mkdir -p "$HERE/renders"

SH3="$HERE/target/sh3/release/martin"
[ -x "$SH3" ] || { echo "city-splat: sh3 binary missing — run: cargo b-sh3"; exit 1; }

# --- 1. video → splat (skip if already trained) ----------------------------
if ls "$WORK"/exports/export_*.ply >/dev/null 2>&1; then
  echo "==> [$NAME] splat exists, skipping train"
else
  echo "==> [$NAME] training splat (TRAIN_ITERS=${TRAIN_ITERS:-12000})"
  FPS="${FPS:-4}" TRAIN_ITERS="${TRAIN_ITERS:-12000}" "$HERE/pipeline/splat.sh" "$MP4" "$WORK"
fi

# pick the highest-step checkpoint as the source
SRC="$(ls -1 "$WORK"/exports/export_*.ply 2>/dev/null | sort -t_ -k2 -n | tail -1)"
[ -n "$SRC" ] || { echo "city-splat: no checkpoint produced for $NAME (COLMAP/Brush failed?)"; exit 2; }

# --- 2. crop: drop NaN/inf, far floaters, spike + haze → dense core ---------
echo "==> [$NAME] cropping $(basename "$SRC") → ${NAME}_tight.ply"
KEEP_PCT="${KEEP_PCT:-80}" OPACITY_MIN="${OPACITY_MIN:--2}" SCALE_PCT="${SCALE_PCT:-1.5}" \
  python3 "$HERE/pipeline/crop-splat.py" "$SRC" "$TIGHT"

# --- 3. flythrough render (offscreen, headless) ----------------------------
# Generic dive→low-pass→climb path in NORMALIZED space (cloud ~[-1,1], y up), so it works for any
# city. Override with FLY_JSON=<file> to hand-tune one.
FLY="${FLY_JSON:-}"
if [ -z "$FLY" ]; then
  FLY="$WORK/fly.json"
  cat > "$FLY" <<'JSON'
[
  {"target":[-0.70,0.10,-0.50],"dist":1.80,"yaw":0.15,"pitch":0.56},
  {"target":[-0.45,0.05,-0.25],"dist":1.05,"yaw":0.70,"pitch":0.40},
  {"target":[-0.18,0.00,-0.02],"dist":0.58,"yaw":1.35,"pitch":0.26},
  {"target":[0.05,0.00,0.12],"dist":0.40,"yaw":2.05,"pitch":0.18},
  {"target":[0.28,0.00,0.16],"dist":0.40,"yaw":2.70,"pitch":0.18},
  {"target":[0.50,0.05,0.02],"dist":0.72,"yaw":3.20,"pitch":0.30},
  {"target":[0.72,0.10,-0.28],"dist":1.55,"yaw":3.60,"pitch":0.52}
]
JSON
fi

FR="$(mktemp -d)"
PFPS="${PREVIEW_FPS:-30}"
echo "==> [$NAME] rendering flythrough → frames (${PFPS}fps, FLY_SECS=${FLY_SECS:-10})"
MARTIN_PLY="$TIGHT" MARTIN_WAYPOINTS="$FLY" MARTIN_FLY="${FLY_SECS:-10}" \
  MARTIN_RECORD="$FR" MARTIN_PREVIEW_FPS="$PFPS" MARTIN_MUTE=1 BEVY_ASSET_ROOT="$HERE" \
  "$SH3" >/dev/null 2>&1 || true
N="$(ls "$FR"/frame_*.png 2>/dev/null | wc -l)"
[ "$N" -gt 0 ] || { echo "city-splat: render produced no frames for $NAME"; rm -rf "$FR"; exit 3; }

ffmpeg -y -hide_banner -loglevel error -framerate "$PFPS" -start_number 0 -i "$FR/frame_%05d.png" \
  -c:v libx264 -pix_fmt yuv420p -crf 18 -movflags +faststart "$OUT"
rm -rf "$FR"
echo "==> [$NAME] DONE → $OUT ($(du -h "$OUT" | cut -f1), $N frames)"
