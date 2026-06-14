# SHOWBOOK — CITIES (deFEEST)

> The storyboard is the **source artefact** of this production: design here first (minutes), render
> afterwards (tens of minutes). Engine work doesn't live here — if a scene needs something the engine
> can't yet do, it goes in *Engine questions* at the bottom and the scene stays described as intended.

**The hook:** four aerial city splats (Austin · New York · Chicago · Seattle), each captured from Google
Aerial View → COLMAP → Brush, floaters trimmed (`*_tight.ply`). The whole show is **one continuous
morph** — skyline flows straight into skyline, no cut, while the camera flies around each one. The
transition *is* the content. The technique is the star: a coherent, ghostly, colour-matched morph
(`pair=match`) instead of the demoscene "disperse-to-ball".

**Music:** the built-in score (6 sections, 89 s) — `productions/intro/score.txt`. The city cuts land on
`@@drop / @@breakdown / @@climax / @@outro`.
**Sections:** intro 0 · drop · breakdown · climax · outro · end (~89 s).
**Show-file:** `cities.show` (`kind=demo`; **LOCAL captures only** — Google Maps Content, gitignored,
never shipped; "Imagery ©Google" credit required — see `pipeline/AERIAL-CITIES.md`).

## The arc (decided 2026-06-14)

Skyline → skyline → skyline → skyline → signature. No story beyond *the morph itself*: a field of grey
roofs becomes a different field of roofs; towers slide into towers; streets re-route. The emotional pull
is purely the uncanny flow — you recognise each city as it locks in, then watch it liquefy into the next.
Closes on the deFEEST signature + the ©Google credit.

Rule per scene: **one city, one orbit, one straight morph into the next.** Nothing static; every skyline
arrives by morph and leaves by morph. The camera never cuts — it reaches each city as that city assembles.

## The five scenes

| # | section (time) | city | what you see | camera | status |
|---|---|---|---|---|---|
| 1 | intro (0–~18) | **Austin** | Black. Austin assembles from the intro ball, high overhead — grid of low downtown blocks, the river. The camera starts its orbit. Starfield behind throughout. | high, begin the orbit + descend | ◪ worked out |
| 2 | drop (~18–~36) | **New York** | Austin **flows straight into** Manhattan — low blocks rise into dense towers, the grid tightens. Post-morph the camera dives and banks across the island. | dive in, bank across Manhattan | ◪ worked out |
| 3 | breakdown (~36–~52) | **Chicago** | NYC → Chicago: the Loop and the lakefront slide in. Quick orbit. | quick orbit | ◪ worked out |
| 4 | climax (~52–~72) | **Seattle** | Chicago → Seattle, the big one — **this is the cut that used to ball** (strongest beat). Now a clean straight slide; the Space Needle resolves out of the morph. Dive + counter-bank. | dive, then counter-bank | ◪ worked out |
| 5 | outro (~72–89) | **credits** | Pull back + level. `deFEEST · CITIES` pen-writes in; `Imagery ©Google` fades in and evaporates. Gentle pull-out to black. | pull back, hold frontal, drift out | ◪ worked out |

*Status trap: □ idea → ◪ worked out (shots + timing below) → ▣ built (in cities.show) → ★ approved.*

## Why `pair=match` (the whole reason this show exists)

A morph is a straight per-particle lerp, so **pairing** is everything. The default rank pairing (Morton
Z-order) flows between *similar* shapes but pinches *dissimilar* ones — two different cities — through a
centre **ball**: distant rank-pairs cross at the centroid. `pair=match` reorders each city so every splat
slides to a nearby, similar-colour splat of the previous city (grey roof → grey roof, green park → green
park, dark street → dark street): short moves, no collapse. Set in `[settings]`:

```
pair = match            # nearest same-colour pairing → straight morph, no ball
budget = 1200000        # crisp; tight files hold up to ~2M (budget=0 = max)
```

`MARTIN_PAIR_COLOR` (default 0.5) tunes colour-vs-position weight. See `DOMAIN.md` → *Pairing*. The
second, sneakier ball — the beat kick's mid-morph `bulge` punch, loudest at `@@climax` — is auto-suppressed
under `pair=match`. Full background: `[[martin-pair-match-morph]]` and commit `7aced0b`.

## Rendering this show

Full quality is **1.2M splats @ 60 fps** — a full 60 fps dump is ~5300 PNGs (~10 GB) and **overflows the
RAM-backed `/tmp`**, so point the scratch dir at a real disk:

```
TMPDIR=/home/<you>/.cache/martin-render MARTIN_SHOW=productions/cities/cities.show ./record.sh cities.mp4
```

Fast preview: `MARTIN_PREVIEW_FPS=8 MARTIN_MORPH_COUNT=250000 …` (fewer frames, fewer splats; fits `/tmp`).
`match_reorder` runs once at build (~31 s at 1.2M). See `AGENTS.md` → *Build & Render*.

## Engine questions

- *(none open)* — the show renders end-to-end. `pair=match` + the beat-pulse gate landed; the
  `match_reorder` ring search is bounded O(n).

## Provenance / licensing

The four `*_tight.ply` are **Google Maps Content** (Aerial View captures): local-only, gitignored, never
shipped, and the show carries an **"Imagery ©Google"** credit in the outro. The tight files are symlinked
into `austin_run/exports/` so one asset root resolves all four; each lives in its own gitignored
`<city>_run/exports/`. See `pipeline/AERIAL-CITIES.md`.
