#!/usr/bin/env bash
# ============================================================================
# release.sh — build martin as ONE self-contained release binary (the show +
# all its assets baked in via --features bundle) and verify it self-extracts +
# plays. Thin wrapper around the bundling pipeline (build.rs reads bundle.toml).
#
# Usage:   ./pipeline/release.sh [bundle.toml]
#
# The .ply the manifest references must exist locally (git-ignored); the demo
# shapes are generated on demand. Cross-OS release binaries: run this on each OS
# (or use GitHub Actions with the assets present) — the bundling itself is the
# same `cargo build --features bundle` everywhere.
# ============================================================================
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$HERE"
MANIFEST="${1:-bundle.toml}"

echo "==> demo shapes (generate once if missing)"
if [ ! -f assets/sphere.ply ]; then
  python3 pipeline/gen-demo-splats.py assets/demo 140000
  cp assets/demo/*.ply assets/
fi

echo "==> building the bundled release binary from $MANIFEST"
MARTIN_BUNDLE="$MANIFEST" cargo +nightly build --release --features bundle

BIN="$HERE/target/release/martin"
echo "==> verifying it self-extracts + builds the baked-in show (headless)"
env -u DISPLAY -u WAYLAND_DISPLAY MARTIN_BENCH=90 timeout 180 "$BIN" 2>&1 \
  | grep -iE "bundle:|sequence built|bench:" | head

echo
echo "============================================================"
echo "RELEASE BINARY:  $BIN   ($(du -h "$BIN" | cut -f1))"
echo "Self-contained — run it anywhere (no assets, no env):  $BIN"
echo "Publish:  gh release create vX.Y -t 'martin vX.Y' && gh release upload vX.Y \"$BIN\""
echo "============================================================"
