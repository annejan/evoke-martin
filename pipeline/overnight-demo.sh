#!/usr/bin/env bash
# ============================================================================
# overnight-demo.sh — unattended: fetch N aerial cities, splat each, render a
# flythrough per city, stitch them (+ music) into one demo. Run it, go to bed.
#
#   ./pipeline/overnight-demo.sh        (needs GOOGLE_MAPS_API_KEY in env)
#
# Austin is reused (already splatted). The other cities are POSTed to Google
# first (renders run in PARALLEL server-side, ~1-3h each), then splatted SERIALLY
# (one iGPU). Resilient: a city with no coverage / a failed step is skipped and
# the demo is built from whatever succeeded.
#
# Output: renders/demo.mp4
# Tunables (env): TRAIN_ITERS (default 12000), city list below.
# ============================================================================
set -uo pipefail
HERE="$(cd "$(dirname "$0")/.." && pwd)"
cd "$HERE"
export TRAIN_ITERS="${TRAIN_ITERS:-12000}"
LOG="$HERE/renders/overnight.log"
mkdir -p "$HERE/renders"
say(){ echo "[$(date '+%H:%M:%S')] $*" | tee -a "$LOG"; }

: "${GOOGLE_MAPS_API_KEY:?set GOOGLE_MAPS_API_KEY (the fetch needs it)}"
[ -x "$HERE/target/sh3/release/martin" ] || { say "sh3 binary missing — run: cargo b-sh3"; exit 1; }

# name | "postal address"  — edit freely. (Austin handled separately, reused.)
CITIES=(
  "nyc|350 5th Ave, New York, NY 10118"
  "seattle|400 Broad St, Seattle, WA 98109"
  "chicago|233 S Wacker Dr, Chicago, IL 60606"
)

# --- 1. kick off all Google renders in parallel (server-side) --------------
say "fetching ${#CITIES[@]} cities (parallel Google renders, this can take hours)…"
pids=()
for c in "${CITIES[@]}"; do
  name="${c%%|*}"; addr="${c#*|}"
  ( MAX_WAIT="${MAX_WAIT:-21600}" "$HERE/pipeline/fetch-aerial.sh" "$addr" "$HERE/${name}.mp4" \
      > "$HERE/renders/${name}_fetch.log" 2>&1 ) &
  pids+=("$!")
  say "  → $name ($addr) [pid $!]"
done
for p in "${pids[@]}"; do wait "$p" || true; done
say "all fetches finished (check renders/<name>_fetch.log for any coverage misses)"

# --- 2. splat + flythrough each, SERIALLY (single iGPU) --------------------
CLIPS=()
process(){ # name  mp4
  local name="$1" mp4="$2"
  if [ ! -f "$mp4" ]; then say "  skip $name — no mp4 (coverage miss or fetch failed)"; return; fi
  say "splatting + flythrough: $name"
  if "$HERE/pipeline/city-splat.sh" "$mp4" "$name" >> "$LOG" 2>&1; then
    CLIPS+=("$HERE/renders/${name}_fly.mp4"); say "  ✓ $name → renders/${name}_fly.mp4"
  else
    say "  ✗ $name failed (see $LOG)"
  fi
}
# Austin first (reuses its existing splat; the aerial.mp4 arg just satisfies the signature).
[ -f "$HERE/aerial.mp4" ] && process austin "$HERE/aerial.mp4"
for c in "${CITIES[@]}"; do name="${c%%|*}"; process "$name" "$HERE/${name}.mp4"; done

[ "${#CLIPS[@]}" -gt 0 ] || { say "no clips produced — nothing to stitch. Bail."; exit 4; }
say "have ${#CLIPS[@]} clips: ${CLIPS[*]##*/}"

# --- 3. render the demo soundtrack (martin synth → wav), non-fatal ---------
WAV="$HERE/renders/demo_track.wav"
say "rendering synth soundtrack…"
MARTIN_SYNTH_WAV="$WAV" BEVY_ASSET_ROOT="$HERE" "$HERE/target/sh3/release/martin" >/dev/null 2>&1 \
  || say "  (synth render failed — demo will be silent)"

# --- 4. stitch all clips (scaled to a common size) + loop music -----------
say "stitching → renders/demo.mp4"
ins=(); fc=""; i=0
for clip in "${CLIPS[@]}"; do ins+=(-i "$clip"); fc+="[$i:v]scale=1280:720:force_original_aspect_ratio=decrease,pad=1280:720:-1:-1:color=black,setsar=1[v$i];"; i=$((i+1)); done
maps=""; for ((j=0;j<i;j++)); do maps+="[v$j]"; done
fc+="${maps}concat=n=$i:v=1:a=0[v]"
if [ -s "$WAV" ]; then
  ffmpeg -y -hide_banner -loglevel error "${ins[@]}" -stream_loop -1 -i "$WAV" \
    -filter_complex "$fc" -map "[v]" -map "${i}:a" -shortest \
    -c:v libx264 -pix_fmt yuv420p -crf 18 -c:a aac -movflags +faststart "$HERE/renders/demo.mp4"
else
  ffmpeg -y -hide_banner -loglevel error "${ins[@]}" \
    -filter_complex "$fc" -map "[v]" \
    -c:v libx264 -pix_fmt yuv420p -crf 18 -movflags +faststart "$HERE/renders/demo.mp4"
fi
say "DONE → renders/demo.mp4 ($(du -h "$HERE/renders/demo.mp4" 2>/dev/null | cut -f1)) · ${#CLIPS[@]} cities"
