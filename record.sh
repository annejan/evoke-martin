#!/usr/bin/env bash
# Render the dogdemo explosion to an mp4 (headless deterministic frame capture + ffmpeg).
# Usage: ./record.sh [output.mp4]   (env: FRAMES=220)
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
OUT="${1:-$HERE/aegg_explosion.mp4}"
FRAMES="${FRAMES:-220}"
FR="$(mktemp -d)"
export DISPLAY="${DISPLAY:-:0}"

echo "==> building dogdemo"
cargo +nightly build --manifest-path "$HERE/dogdemo/Cargo.toml"
BIN="$(find "$HERE/dogdemo/target/debug" -maxdepth 1 -type f -executable -name dogdemo | head -n1)"

echo "==> recording $FRAMES frames -> $FR"
DOGDEMO_RECORD="$FR" DOGDEMO_FRAMES="$FRAMES" BEVY_ASSET_ROOT="$HERE/dogdemo" "$BIN"

echo "==> assembling $OUT"
ffmpeg -y -framerate 30 -start_number 0 -i "$FR/frame_%05d.png" \
  -c:v libx264 -pix_fmt yuv420p -crf 18 -movflags +faststart "$OUT"
rm -rf "$FR"
echo "==> wrote $OUT"
