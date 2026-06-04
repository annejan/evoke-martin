#!/usr/bin/env bash
# ============================================================================
# bundle.sh — build martin as ONE self-contained binary with its show + assets
# baked in. A thin wrapper around `cargo build --release --features bundle`
# (build.rs does the real work: read bundle.toml, auto-collect the .ply/PNG the
# show references, lz4-compress them into the executable). At runtime the binary
# self-extracts to a temp dir and plays the baked-in show — no asset files, no
# env vars needed (env still overrides for debugging).
#
# Usage:   ./pipeline/bundle.sh [bundle.toml]
# Example: ./pipeline/bundle.sh                # uses ./bundle.toml
#          ./pipeline/bundle.sh shows/live.toml
#
# The .ply assets the manifest references must be present locally (they're
# git-ignored). Edit bundle.toml to choose the show.
# ============================================================================
set -euo pipefail

MANIFEST="${1:-bundle.toml}"
[ -f "$MANIFEST" ] || { echo "manifest not found: $MANIFEST"; exit 1; }

echo "==> bundling from $MANIFEST"
MARTIN_BUNDLE="$MANIFEST" cargo +nightly build --release --features bundle

BIN="target/release/martin"
echo
echo "============================================================"
echo "DONE.  Single self-contained binary: $BIN  ($(du -h "$BIN" | cut -f1))"
echo "Run it anywhere — no assets, no env needed:   $BIN"
echo "(env vars still override the baked-in show, e.g. MARTIN_LOOP=1)"
echo "============================================================"
