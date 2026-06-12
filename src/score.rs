//! Music score — the *composition*, data-driven. Ported from Cinder's (Kristian Vlaardingerbroek,
//! deFEEST) `term-demo` (MIT, Outline 2026): the BPM→beat→bar grid, the section timeline
//! (intro→build→drop→breakdown→climax→outro), the drum patterns and the per-section dynamics that
//! the synth (`audio.rs`) and the visual `@@anchor`s both read.
//!
//! The music lives in a **text file**, not in code: `assets/score.txt` (a tracker-DSL score) is
//! loaded by default — edit it, no recompile — and `include_str!`'d as the embedded fallback for a
//! bundled binary (so the notes/patterns/chords are not duplicated in Rust). `MARTIN_SCORE=<file>`
//! overrides it; `MARTIN_SCORE_DUMP=<file>` writes a copy. The *instrument* (how a kick/stab
//! sounds) stays in `audio.rs`. 16 steps per bar (16th notes).

const SLOTS_PER_BAR: i64 = 16;
const BEATS_PER_BAR: f32 = 4.0;

/// The editable default score, loaded from disk when present (so editing it needs no recompile).
/// The same file is `include_str!`'d as the embedded fallback — the music lives here, not in code.
const DEFAULT_SCORE: &str = "assets/score.txt";

/// Crossfade window (seconds) smoothing per-section dynamics steps at boundaries — long enough to
/// kill the click, short enough not to smear the musical transition.
pub const SECTION_FADE: f32 = 0.12;

/// The four sequenced drum/voice lanes (the *instrument* synthesis lives in `audio.rs`).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Inst {
    Kick,
    Snare,
    Hat,
    Stab,
}

/// A per-section value that ramps linearly `a → b` across the section (`a == b` = constant). Used
/// for gain / sub / mids so a section can build (e.g. the riser into the drop) instead of stepping.
#[derive(Clone, Copy)]
pub struct Ramp {
    pub a: f32,
    pub b: f32,
}

impl Ramp {
    fn new(a: f32, b: f32) -> Self {
        Self { a, b }
    }
    fn c(v: f32) -> Self {
        Self { a: v, b: v }
    }
    /// value at progress `p` (0..1) through the section.
    fn at(&self, p: f32) -> f32 {
        self.a + (self.b - self.a) * p
    }
}

/// One instrument's pattern within a section: a 16-step grid per phase, plus the fill-bar grid.
#[derive(Clone, Default)]
pub struct Lane {
    pub phases: Vec<[bool; 16]>,
    pub fill: [bool; 16],
}

impl Lane {
    /// the grid for `phase` (255 = fill). An undefined phase is **silent** — lanes only carry the
    /// phases that have hits, so this keeps `MARTIN_SCORE_DUMP` → reload faithful and makes
    /// "didn't write a pattern" mean "doesn't play" (not "repeat the previous one").
    fn at(&self, phase: u8) -> [bool; 16] {
        if phase == 255 {
            self.fill
        } else {
            self.phases
                .get(phase as usize)
                .copied()
                .unwrap_or([false; 16])
        }
    }
    fn any(&self, p: &[bool; 16]) -> bool {
        p.iter().any(|&b| b)
    }
}

/// A melodic note lane: a frequency (Hz) per 16-step slot (`None` = rest) — same phase/fill shape
/// as `Lane`, but pitched. This is the `lead` (melody) the synth plays.
#[derive(Clone, Default)]
pub struct NoteLane {
    /// Each phase is a melodic **phrase**: a sequence of 1+ bars that plays out and loops every N
    /// bars (a 1-bar phrase = the old per-bar repeat). So a section can carry a real multi-bar
    /// melody — a through-composed line — not just one looping bar.
    pub phases: Vec<Vec<[Option<f32>; 16]>>,
    pub fill: Vec<[Option<f32>; 16]>,
}

impl NoteLane {
    /// The 16-slot grid `into` bars into the section. The melodic phrase (the primary `p0` line)
    /// loops CONTINUOUSLY across the whole section — independent of the drum phases and the fill
    /// bar — so a through-composed line plays as one uninterrupted statement (and breathes over a
    /// drum fill) instead of being chopped/restarted at every phase boundary.
    fn bar(&self, into: u32) -> [Option<f32>; 16] {
        let phrase = self.phases.first().map(Vec::as_slice).unwrap_or(&[]);
        if phrase.is_empty() {
            [None; 16]
        } else {
            phrase[into as usize % phrase.len()]
        }
    }
    fn any(phrase: &[[Option<f32>; 16]]) -> bool {
        phrase.iter().flatten().any(|n| n.is_some())
    }
}

/// A chord: a root frequency + major/minor quality. Cycles per bar (the `chords` line) and drives
/// the bass + stab, so the harmony moves under the melody.
#[derive(Clone, Copy)]
pub struct Chord {
    pub root: f32,
    pub minor: bool,
}

impl Chord {
    /// (root, third, fifth) triad frequencies.
    pub fn triad(&self) -> [f32; 3] {
        let third = if self.minor { 3.0 } else { 4.0 };
        [self.root, self.root * semis(third), self.root * semis(7.0)]
    }
}

/// Frequency ratio of `n` semitones.
fn semis(n: f32) -> f32 {
    2f32.powf(n / 12.0)
}

/// The built-in, name-based FX/layer gating — the behaviour a section gets when it has NO explicit
/// `<section>.fx:` line (so the shipped op-de-camping score, which has none, is unchanged). The layer
/// names (`wall`/`shimmer`/`donk`/`house`/`casio`) and transition accents (`riser`/`jet`/`impact`/
/// `bang`) each fire in the sections the synth used to hard-code.
fn default_fx(name: &str, token: &str) -> bool {
    let any = |names: &[&str]| names.contains(&name);
    match token {
        "wall" => any(&["drop", "climax", "outro"]),
        "shimmer" => any(&["climax", "outro"]),
        "donk" => any(&["drop", "climax"]),
        "house" => any(&["drop", "climax", "outro"]),
        "casio" => any(&["outro"]),
        "riser" => any(&["build", "drop", "climax", "outro"]),
        "jet" => any(&["drop", "climax"]),
        "impact" => any(&["drop", "breakdown", "climax"]),
        "bang" => any(&["outro"]),
        _ => false,
    }
}

/// One section of the arrangement: a span of `bars` divided into `phases` (bars per phase) with an
/// optional fill bar, its dynamics curves, and its four drum lanes.
#[derive(Clone)]
pub struct Section {
    pub name: String,
    pub bars: u32,
    pub phases: Vec<u32>, // bars per phase; if `fill`, the final bar of the section is the fill
    pub fill: bool,
    pub gain: Ramp,
    pub sub: Ramp,
    pub mids: Ramp,
    pub kick: Lane,
    pub snare: Lane,
    pub hat: Lane,
    pub stab: Lane,
    pub lead: NoteLane,     // melody (one note per slot); empty = no lead
    pub arp: NoteLane,      // a second melodic line (the plucky counter-melody); empty = no arp
    pub bass: NoteLane, // an articulated bassline (one note per slot); empty = chord-root sub only
    pub chords: Vec<Chord>, // per-section chord override (cycles within the section); empty = global
    /// Per-section mix/fx knob overrides (`<section>.set key=value`): when set, `param_at` returns
    /// these inside this section instead of the global `set` value — e.g. a louder house organ in the
    /// drop without touching the climax. Empty = use the global knob.
    pub params: std::collections::HashMap<String, f32>,
    /// Per-section FX/layer selection (`<section>.fx: wall jet …`). `None` = use the built-in
    /// name-based defaults (so the shipped demo is unchanged); `Some` = exactly these accents, letting
    /// a different genre opt out of e.g. the demoscene jets without renaming its sections. See `fx_on`.
    pub fx: Option<Vec<String>>,
    pub start_bar: u32, // computed by Score::new
}

impl Section {
    fn empty(name: String, bars: u32, phases: Vec<u32>, fill: bool) -> Self {
        Self {
            name,
            bars,
            phases,
            fill,
            gain: Ramp::c(0.85),
            sub: Ramp::c(0.5),
            mids: Ramp::c(0.6),
            kick: Lane::default(),
            snare: Lane::default(),
            hat: Lane::default(),
            stab: Lane::default(),
            lead: NoteLane::default(),
            arp: NoteLane::default(),
            bass: NoteLane::default(),
            chords: Vec::new(),
            params: std::collections::HashMap::new(),
            fx: None,
            start_bar: 0,
        }
    }

    /// Whether this section gets the FX/layer `token` (`wall`/`shimmer`/`donk`/`house`/`casio` layers,
    /// `riser`/`jet`/`impact`/`bang` transitions). An explicit `<section>.fx:` list is authoritative;
    /// otherwise the built-in name-based default fires (so a score with no `fx:` lines is unchanged).
    pub fn fx_on(&self, token: &str) -> bool {
        match &self.fx {
            Some(list) => list.iter().any(|t| t == token),
            None => default_fx(&self.name, token),
        }
    }

    fn lane(&self, inst: Inst) -> &Lane {
        match inst {
            Inst::Kick => &self.kick,
            Inst::Snare => &self.snare,
            Inst::Hat => &self.hat,
            Inst::Stab => &self.stab,
        }
    }

    fn lane_mut(&mut self, inst: &str) -> Option<&mut Lane> {
        match inst {
            "kick" => Some(&mut self.kick),
            "snare" => Some(&mut self.snare),
            "hat" => Some(&mut self.hat),
            "stab" => Some(&mut self.stab),
            _ => None,
        }
    }

    /// Which phase a bar `into` this section is in: the trailing bar is the fill (255) when the
    /// section has one; otherwise the phase whose cumulative bar-span contains `into`.
    fn phase_at(&self, into: u32) -> u8 {
        self.phase_and_offset(into).0
    }

    /// The phase index AND how many bars into that phase `into` is — so a multi-bar melodic phrase
    /// knows which of its bars to play. The trailing fill bar is `(255, 0)`.
    fn phase_and_offset(&self, into: u32) -> (u8, u32) {
        if self.fill {
            let total: u32 = self.phases.iter().sum::<u32>() + 1;
            if into >= total.saturating_sub(1) {
                return (255, 0);
            }
        }
        let mut acc = 0;
        for (i, &p) in self.phases.iter().enumerate() {
            if into < acc + p {
                return (i as u8, into - acc);
            }
            acc += p;
        }
        // past the defined phases → the last phase, offset from its start.
        let last = self.phases.len().saturating_sub(1);
        let before: u32 = self.phases.iter().take(last).sum();
        (last as u8, into.saturating_sub(before))
    }
}

/// The enveloped sub-bass / mids levels at a moment — the synth reads these for its osc + stab
/// amplitudes.
#[derive(Clone, Copy)]
pub struct Levels {
    pub sub_bass: f32,
    pub mids: f32,
}

/// A whole score: tempo + an ordered list of sections (which carry their own patterns + dynamics).
#[derive(Clone)]
pub struct Score {
    pub bpm: f32,
    pub chords: Vec<Chord>, // per-bar chord progression (cycles); drives bass + stab
    pub sections: Vec<Section>,
    total_bars: u32,
    /// Free-form mix/fx knobs from `set <key>=<value>` lines — the synth reads these (with built-in
    /// defaults) so the SOUND can be tuned by editing the score file (no recompile), not the engine.
    params: std::collections::HashMap<String, f32>,
}

impl Score {
    /// Lay out the sections (cumulative `start_bar`, total length) — the single place section
    /// timing is derived, so the file and the built-in agree.
    fn new(bpm: f32, chords: Vec<Chord>, mut sections: Vec<Section>) -> Self {
        let mut bar = 0;
        for s in &mut sections {
            s.start_bar = bar;
            bar += s.bars;
        }
        // a score with no `chords` line still needs harmony — default to a single A-minor.
        let chords = if chords.is_empty() {
            vec![Chord {
                root: note_freq("A3").unwrap(),
                minor: true,
            }]
        } else {
            chords
        };
        Self {
            bpm,
            chords,
            sections,
            total_bars: bar,
            params: std::collections::HashMap::new(),
        }
    }

    /// A mix/fx knob (`set <key>=<value>` in the score), or `default` if unset — the single hook the
    /// synth uses so its levels/sends live in the score file, tunable without recompiling the engine.
    pub fn param(&self, key: &str, default: f32) -> f32 {
        self.params.get(key).copied().unwrap_or(default)
    }

    /// A mix/fx knob honouring a per-section override: if the section active at `t` has a
    /// `<section>.set key=…` for `key`, return that; otherwise fall back to the global `param`. The
    /// synth uses this for the knobs it reads per-onset (so a section can be louder/quieter without a
    /// recompile and without touching the others — e.g. `drop.set house=0.18`).
    pub fn param_at(&self, t: f32, key: &str, default: f32) -> f32 {
        let s = &self.sections[self.section_index_at(t)];
        s.params
            .get(key)
            .copied()
            .unwrap_or_else(|| self.param(key, default))
    }

    /// Whether the section named `name` gets FX/layer `token` (see `Section::fx_on`). Unknown section
    /// → false. The synth gates its accents on this so a section's FX live in the score, not in code.
    pub fn fx_on(&self, name: &str, token: &str) -> bool {
        self.sections
            .iter()
            .find(|s| s.name == name)
            .is_some_and(|s| s.fx_on(token))
    }

    // --- grid ---------------------------------------------------------------------------------
    pub fn beat(&self) -> f32 {
        60.0 / self.bpm
    }
    pub fn bar(&self) -> f32 {
        BEATS_PER_BAR * self.beat()
    }
    fn slot_len(&self) -> f32 {
        self.beat() / 4.0
    }
    pub fn demo_len(&self) -> f32 {
        self.total_bars as f32 * self.bar()
    }

    fn abs_slot(&self, t: f32) -> i64 {
        let sl = self.slot_len();
        ((t + sl * 1e-3) / sl).floor() as i64
    }
    fn bar_idx_at(&self, t: f32) -> u32 {
        (self.abs_slot(t).max(0) / SLOTS_PER_BAR) as u32
    }

    // --- sections -----------------------------------------------------------------------------
    fn section_index_at(&self, t: f32) -> usize {
        let b = self.bar_idx_at(t);
        let mut idx = 0;
        for (i, s) in self.sections.iter().enumerate() {
            if b >= s.start_bar {
                idx = i;
            } else {
                break;
            }
        }
        idx
    }
    pub fn section_start_secs(&self, idx: usize) -> f32 {
        self.sections[idx].start_bar as f32 * self.bar()
    }

    // --- patterns -----------------------------------------------------------------------------
    fn lane_hits(&self, inst: Inst, t: f32) -> [bool; 16] {
        let i = self.section_index_at(t);
        let s = &self.sections[i];
        let into = (self.bar_idx_at(t) as i64 - s.start_bar as i64).max(0) as u32;
        s.lane(inst).at(s.phase_at(into))
    }

    /// Every hit time (s) for `inst` across the whole track, in order — the synth builds a voice at
    /// each. Forward enumeration: walk every 16th-note slot and keep the ones that fire.
    pub fn hits(&self, inst: Inst) -> Vec<f32> {
        let sl = self.slot_len();
        let slots = self.total_bars as i64 * SLOTS_PER_BAR;
        (0..slots)
            .filter_map(|s| {
                let t = s as f32 * sl;
                self.lane_hits(inst, t)[(s % SLOTS_PER_BAR) as usize].then_some(t)
            })
            .collect()
    }

    // --- harmony + melody ---------------------------------------------------------------------
    /// The chord active at `t` (per-bar, cycling). A section with its own `chords:` line cycles
    /// through *that* progression (counted from the section start) — e.g. a G-minor verse under a
    /// G-major chorus; otherwise the global progression applies.
    pub fn chord_at(&self, t: f32) -> Chord {
        let bar = self.bar_idx_at(t) as usize;
        let s = &self.sections[self.section_index_at(t)];
        if !s.chords.is_empty() {
            return s.chords[(bar - s.start_bar as usize) % s.chords.len()];
        }
        self.chords[bar % self.chords.len()]
    }

    fn note_grid(&self, t: f32, pick: fn(&Section) -> &NoteLane) -> [Option<f32>; 16] {
        let i = self.section_index_at(t);
        let s = &self.sections[i];
        let into = (self.bar_idx_at(t) as i64 - s.start_bar as i64).max(0) as u32;
        pick(s).bar(into)
    }

    /// Every note of a note-lane as (time, freq) across the whole track — the synth builds a voice
    /// at each onset.
    fn note_line(&self, pick: fn(&Section) -> &NoteLane) -> Vec<(f32, f32)> {
        let sl = self.slot_len();
        let slots = self.total_bars as i64 * SLOTS_PER_BAR;
        (0..slots)
            .filter_map(|s| {
                let t = s as f32 * sl;
                self.note_grid(t, pick)[(s % SLOTS_PER_BAR) as usize].map(|f| (t, f))
            })
            .collect()
    }

    /// The `lead` (foreground melody) onsets.
    pub fn lead_notes(&self) -> Vec<(f32, f32)> {
        self.note_line(|s| &s.lead)
    }

    /// The `arp` (second melodic line) onsets.
    pub fn arp_notes(&self) -> Vec<(f32, f32)> {
        self.note_line(|s| &s.arp)
    }

    /// The `bass` (articulated bassline) onsets — empty unless the score writes a `bass` lane.
    pub fn bass_notes(&self) -> Vec<(f32, f32)> {
        self.note_line(|s| &s.bass)
    }

    // --- dynamics -----------------------------------------------------------------------------
    fn section_value<F: Fn(&Section) -> Ramp>(&self, t: f32, pick: &F) -> f32 {
        let i = self.section_index_at(t);
        let s = &self.sections[i];
        let dur = (s.bars as f32 * self.bar()).max(1e-3);
        let p = ((t - self.section_start_secs(i)) / dur).clamp(0.0, 1.0);
        pick(s).at(p)
    }

    /// Crossfade a section value across its start boundary (`SECTION_FADE`) to remove the step.
    fn smooth<F: Fn(&Section) -> Ramp>(&self, t: f32, pick: F) -> f32 {
        let b = self.section_start_secs(self.section_index_at(t));
        let cur = self.section_value(t, &pick);
        if b > 0.0 && t - b < SECTION_FADE {
            let prev = self.section_value(b - 1e-3, &pick);
            prev + (cur - prev) * ((t - b) / SECTION_FADE)
        } else {
            cur
        }
    }

    /// Master gain (per-section, crossfaded) the synth multiplies the mix by.
    pub fn gain_at(&self, t: f32) -> f32 {
        self.smooth(t, |s| s.gain)
    }

    /// Sub-bass + mids levels: the per-section depth (crossfaded) under a slow LFO breath/swell.
    pub fn levels(&self, t: f32) -> Levels {
        use std::f32::consts::TAU;
        let breath = (t / (2.0 * self.beat()) * TAU).sin() * 0.5 + 0.5;
        let swell = (t / (8.0 * self.beat()) * TAU).sin() * 0.5 + 0.5;
        Levels {
            sub_bass: (breath * self.smooth(t, |s| s.sub)).clamp(0.0, 1.0),
            mids: (swell * self.smooth(t, |s| s.mids)).clamp(0.0, 1.0),
        }
    }

    // --- visual anchoring ---------------------------------------------------------------------
    /// Resolve a `@@anchor` token to an absolute time (s): a section name, `bar<N>`/`bar:N`,
    /// `beat<N>`/`beat:N`, `start`, or raw seconds. Lets a part lock to the music.
    pub fn anchor_seconds(&self, s: &str) -> Option<f32> {
        let s = s.trim().to_ascii_lowercase();
        if s == "start" {
            return Some(0.0);
        }
        if let Some(i) = self.sections.iter().position(|x| x.name == s) {
            return Some(self.section_start_secs(i));
        }
        if let Some(n) = s.strip_prefix("bar") {
            return n
                .trim_start_matches(':')
                .parse::<f32>()
                .ok()
                .map(|b| b * self.bar());
        }
        if let Some(n) = s.strip_prefix("beat") {
            return n
                .trim_start_matches(':')
                .parse::<f32>()
                .ok()
                .map(|b| b * self.beat());
        }
        s.parse::<f32>().ok()
    }

    // --- loading ------------------------------------------------------------------------------
    /// `MARTIN_SCORE=<file>` loads a tracker-DSL score; on any error we log + fall back to the
    /// built-in, so a bad score file never stops the show.
    pub fn from_env() -> Score {
        // MARTIN_SCORE override, else the editable default file (edit it → no recompile), else the
        // embedded built-in (a bundled binary with no assets/ folder).
        let path = std::env::var("MARTIN_SCORE")
            .ok()
            .filter(|p| !p.is_empty())
            .or_else(|| {
                std::path::Path::new(DEFAULT_SCORE)
                    .exists()
                    .then(|| DEFAULT_SCORE.to_string())
            });
        let Some(path) = path else {
            return Score::builtin();
        };
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| Score::from_str(&t))
        {
            Ok(s) => {
                eprintln!(
                    "score: {path} ({} sections, {:.0}s)",
                    s.sections.len(),
                    s.demo_len()
                );
                // structural lint: surface the DSL's silent traps (phase/bar mismatch, a pattern on a
                // phase the section lacks, an ignored melodic p1+). Warnings don't fail the load — the
                // show still plays — UNLESS `MARTIN_SCORE_STRICT` is set (authoring / CI), then they're
                // fatal so a broken score can't slip through.
                let warnings = validate(&s.sections);
                for w in &warnings {
                    eprintln!("score: warning: {w}");
                }
                if !warnings.is_empty() && strict_scores() {
                    eprintln!(
                        "score: {} warning(s) with MARTIN_SCORE_STRICT — aborting",
                        warnings.len()
                    );
                    std::process::exit(1);
                }
                s
            }
            Err(e) => {
                eprintln!("score: {path}: {e} — using embedded built-in");
                Score::builtin()
            }
        }
    }

    /// Parse a tracker-DSL score (see `to_dsl` for the shape / `USAGE.md` for the grammar).
    pub fn from_str(text: &str) -> Result<Score, String> {
        let mut bpm = 140.0_f32;
        let mut chords: Vec<Chord> = Vec::new();
        let mut sections: Vec<Section> = Vec::new();
        let mut params: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
        let find = |sections: &[Section], name: &str| sections.iter().position(|s| s.name == name);

        for (n, raw) in text.lines().enumerate() {
            // Strip a `# comment`, but only when `#` starts the line or follows whitespace — so a
            // sharp note like `F#6` (mid-token `#`) is NOT mistaken for a comment.
            let bytes = raw.as_bytes();
            let cut = raw
                .char_indices()
                .find(|&(i, c)| c == '#' && (i == 0 || bytes[i - 1].is_ascii_whitespace()))
                .map(|(i, _)| i)
                .unwrap_or(raw.len());
            let line = raw[..cut].trim();
            if line.is_empty() {
                continue;
            }
            let ln = n + 1;
            let first = line.split_whitespace().next().unwrap();

            // pattern line: `<section>.<inst> p<N>|fill: <16 steps>`
            if first.contains('.') {
                // per-section knob override: `<section>.set key=value …` (no colon — mirrors global
                // `set`; the synth reads it via `param_at` inside that section only).
                if let Some((sec, "set")) = first.split_once('.') {
                    let si = find(&sections, sec)
                        .ok_or_else(|| format!("line {ln}: unknown section `{sec}`"))?;
                    for tok in line.split_whitespace().skip(1) {
                        let (k, v) = tok.split_once('=').ok_or_else(|| {
                            format!("line {ln}: `{sec}.set` needs key=value, got `{tok}`")
                        })?;
                        let val = v
                            .parse()
                            .map_err(|_| format!("line {ln}: bad set value `{tok}`"))?;
                        sections[si].params.insert(k.to_string(), val);
                    }
                    continue;
                }
                let (head, pat) = line
                    .split_once(':')
                    .ok_or_else(|| format!("line {ln}: pattern needs a ':'"))?;
                let mut h = head.split_whitespace();
                let target = h.next().unwrap();
                let phase_tok = h.next().unwrap_or("p0");
                let (sec, inst) = target
                    .split_once('.')
                    .ok_or_else(|| format!("line {ln}: expected `section.inst`, got `{target}`"))?;
                let si = find(&sections, sec)
                    .ok_or_else(|| format!("line {ln}: unknown section `{sec}`"))?;
                let phase: Option<usize> = if phase_tok.eq_ignore_ascii_case("fill") {
                    None
                } else {
                    Some(
                        phase_tok
                            .trim_start_matches('p')
                            .parse()
                            .map_err(|_| format!("line {ln}: bad phase `{phase_tok}`"))?,
                    )
                };
                if inst == "chords" {
                    // per-section chord override: `<section>.chords: G Am Bb D` (cycles in-section).
                    let mut cs = Vec::new();
                    for tok in pat.split_whitespace() {
                        cs.push(
                            parse_chord(tok)
                                .ok_or_else(|| format!("line {ln}: bad chord `{tok}`"))?,
                        );
                    }
                    sections[si].chords = cs;
                } else if inst == "fx" {
                    // per-section FX/layer selection: `<section>.fx: wall jet impact` (overrides the
                    // built-in name-based defaults for that section). An empty list = no FX at all.
                    sections[si].fx = Some(pat.split_whitespace().map(|s| s.to_string()).collect());
                } else if inst == "lead" || inst == "arp" || inst == "bass" {
                    // pitched note lane: a phrase of 1+ bars (16 note tokens each, `A4`/`C#5`/`.`).
                    let grid = parse_notes(pat).ok_or_else(|| {
                        format!("line {ln}: {inst} needs 16 notes/rests (or a multiple of 16)")
                    })?;
                    let lane = match inst {
                        "arp" => &mut sections[si].arp,
                        "bass" => &mut sections[si].bass,
                        _ => &mut sections[si].lead,
                    };
                    match phase {
                        None => lane.fill = grid,
                        Some(p) => {
                            if lane.phases.len() <= p {
                                lane.phases.resize(p + 1, Vec::new());
                            }
                            lane.phases[p] = grid;
                        }
                    }
                } else {
                    let grid = parse_pattern(pat)
                        .ok_or_else(|| format!("line {ln}: pattern must be 16 of x/."))?;
                    let lane = sections[si]
                        .lane_mut(inst)
                        .ok_or_else(|| format!("line {ln}: unknown instrument `{inst}`"))?;
                    match phase {
                        None => lane.fill = grid,
                        Some(p) => {
                            if lane.phases.len() <= p {
                                lane.phases.resize(p + 1, [false; 16]);
                            }
                            lane.phases[p] = grid;
                        }
                    }
                }
                continue;
            }

            let mut it = line.split_whitespace();
            let kw = it.next().unwrap();
            match kw {
                "bpm" => {
                    bpm = it
                        .next()
                        .and_then(pf)
                        .ok_or_else(|| format!("line {ln}: bpm needs a number"))?;
                }
                "section" => {
                    let name = it
                        .next()
                        .ok_or_else(|| format!("line {ln}: section needs a name"))?
                        .to_string();
                    let bars: u32 = it
                        .next()
                        .and_then(|x| x.parse().ok())
                        .ok_or_else(|| format!("line {ln}: section needs a bar count"))?;
                    let mut phases = vec![bars];
                    let mut fill = false;
                    for tok in it {
                        if tok.eq_ignore_ascii_case("fill") {
                            fill = true;
                        } else {
                            let ph: Vec<u32> =
                                tok.split(',').filter_map(|x| x.parse().ok()).collect();
                            if !ph.is_empty() {
                                phases = ph;
                            }
                        }
                    }
                    sections.push(Section::empty(name, bars, phases, fill));
                }
                "chords" => {
                    for tok in it {
                        chords.push(
                            parse_chord(tok)
                                .ok_or_else(|| format!("line {ln}: bad chord `{tok}`"))?,
                        );
                    }
                }
                "gain" | "sub" | "mids" => {
                    let toks: Vec<&str> = it.collect();
                    for pair in toks.chunks(2) {
                        let [name, val] = pair else { break };
                        let si = find(&sections, name)
                            .ok_or_else(|| format!("line {ln}: unknown section `{name}`"))?;
                        let r = parse_ramp(val)
                            .ok_or_else(|| format!("line {ln}: bad value `{val}`"))?;
                        match kw {
                            "gain" => sections[si].gain = r,
                            "sub" => sections[si].sub = r,
                            _ => sections[si].mids = r,
                        }
                    }
                }
                // `set lead=0.82 reverb=0.35 ...` — free-form mix/fx knobs the synth reads (with its
                // own defaults). Lets the SOUND be tuned by editing the score, not recompiling.
                "set" => {
                    for tok in it {
                        let (k, v) = tok.split_once('=').ok_or_else(|| {
                            format!("line {ln}: `set` needs key=value, got `{tok}`")
                        })?;
                        let val = v
                            .parse()
                            .map_err(|_| format!("line {ln}: bad set value `{tok}`"))?;
                        params.insert(k.to_string(), val);
                    }
                }
                other => return Err(format!("line {ln}: unknown keyword `{other}`")),
            }
        }
        if sections.is_empty() {
            return Err("no sections defined".into());
        }
        let mut score = Score::new(bpm, chords, sections);
        score.params = params;
        Ok(score)
    }

    /// Serialize back to the tracker DSL — `MARTIN_SCORE_DUMP` writes the built-in this way for a
    /// ready-to-edit starting file (and it round-trips through `from_str`).
    pub fn to_dsl(&self) -> String {
        let mut o = String::new();
        o.push_str("# martin score — tracker DSL. Edit + load with MARTIN_SCORE=<this file>.\n");
        o.push_str(&format!("bpm {}\n", fnum(self.bpm)));
        o.push_str(&format!(
            "chords {}\n\n",
            self.chords
                .iter()
                .map(chord_str)
                .collect::<Vec<_>>()
                .join(" ")
        ));
        if !self.params.is_empty() {
            let mut kv: Vec<_> = self.params.iter().collect();
            kv.sort_by(|a, b| a.0.cmp(b.0));
            o.push_str("# mix/fx knobs (tune the SOUND here, no recompile — synth reads these).\n");
            o.push_str("set ");
            o.push_str(
                &kv.iter()
                    .map(|(k, v)| format!("{k}={}", fnum(**v)))
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            o.push_str("\n\n");
        }
        o.push_str("# section <name> <bars> <phase-bars,csv> [fill]\n");
        for s in &self.sections {
            let ph = s
                .phases
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join(",");
            o.push_str(&format!(
                "section {} {} {}{}\n",
                s.name,
                s.bars,
                ph,
                if s.fill { " fill" } else { "" }
            ));
        }
        for s in &self.sections {
            if !s.chords.is_empty() {
                o.push_str(&format!(
                    "{}.chords: {}\n",
                    s.name,
                    s.chords.iter().map(chord_str).collect::<Vec<_>>().join(" ")
                ));
            }
        }
        for s in &self.sections {
            if !s.params.is_empty() {
                let mut kv: Vec<_> = s.params.iter().collect();
                kv.sort_by(|a, b| a.0.cmp(b.0));
                o.push_str(&format!(
                    "{}.set {}\n",
                    s.name,
                    kv.iter()
                        .map(|(k, v)| format!("{k}={}", fnum(**v)))
                        .collect::<Vec<_>>()
                        .join(" ")
                ));
            }
        }
        for s in &self.sections {
            if let Some(fx) = &s.fx {
                o.push_str(&format!("{}.fx: {}\n", s.name, fx.join(" ")));
            }
        }
        o.push_str(
            "\n# patterns: <section>.<kick|snare|hat|stab> p<N>|fill: 16 steps (x=hit .=rest)\n",
        );
        for s in &self.sections {
            for (inst, name) in [
                (Inst::Kick, "kick"),
                (Inst::Snare, "snare"),
                (Inst::Hat, "hat"),
                (Inst::Stab, "stab"),
            ] {
                let lane = s.lane(inst);
                for (p, grid) in lane.phases.iter().enumerate() {
                    if lane.any(grid) {
                        o.push_str(&format!("{}.{name} p{p}: {}\n", s.name, pat_str(grid)));
                    }
                }
                if s.fill && lane.any(&lane.fill) {
                    o.push_str(&format!(
                        "{}.{name} fill: {}\n",
                        s.name,
                        pat_str(&lane.fill)
                    ));
                }
            }
        }
        o.push_str(
            "\n# melody: <section>.lead p<N>|fill: 16 note slots (A4 C#5 . E5 …; . = rest)\n",
        );
        for s in &self.sections {
            for (p, grid) in s.lead.phases.iter().enumerate() {
                if NoteLane::any(grid) {
                    o.push_str(&format!("{}.lead p{p}: {}\n", s.name, notes_phrase(grid)));
                }
            }
            if NoteLane::any(&s.lead.fill) {
                o.push_str(&format!(
                    "{}.lead fill: {}\n",
                    s.name,
                    notes_phrase(&s.lead.fill)
                ));
            }
        }
        o.push_str("\n# arp: <section>.arp p<N>|fill — a second melodic line, same note grammar\n");
        for s in &self.sections {
            for (p, grid) in s.arp.phases.iter().enumerate() {
                if NoteLane::any(grid) {
                    o.push_str(&format!("{}.arp p{p}: {}\n", s.name, notes_phrase(grid)));
                }
            }
            if NoteLane::any(&s.arp.fill) {
                o.push_str(&format!(
                    "{}.arp fill: {}\n",
                    s.name,
                    notes_phrase(&s.arp.fill)
                ));
            }
        }
        o.push_str(
            "\n# bass: <section>.bass p<N>|fill — an articulated bassline, same note grammar\n",
        );
        for s in &self.sections {
            for (p, grid) in s.bass.phases.iter().enumerate() {
                if NoteLane::any(grid) {
                    o.push_str(&format!("{}.bass p{p}: {}\n", s.name, notes_phrase(grid)));
                }
            }
            if NoteLane::any(&s.bass.fill) {
                o.push_str(&format!(
                    "{}.bass fill: {}\n",
                    s.name,
                    notes_phrase(&s.bass.fill)
                ));
            }
        }
        o.push_str(
            "\n# dynamics 0..1 per section (`v` constant or `a>b` ramp across the section)\n",
        );
        for (kw, pick) in [
            ("gain", &(|s: &Section| s.gain) as &dyn Fn(&Section) -> Ramp),
            ("sub", &(|s: &Section| s.sub)),
            ("mids", &(|s: &Section| s.mids)),
        ] {
            o.push_str(kw);
            for s in &self.sections {
                o.push_str(&format!(" {} {}", s.name, ramp_str(&pick(s))));
            }
            o.push('\n');
        }
        o
    }

    /// The default score: the **embedded** `assets/score.txt`, so the notes / patterns / chords
    /// live in the editable text file, not in code. `from_env` prefers the on-disk copy when it's
    /// present (edit it → no recompile); this embedded copy is the fallback a bundled binary ships.
    pub fn builtin() -> Score {
        Score::from_str(include_str!("../assets/score.txt"))
            .expect("embedded assets/score.txt must parse")
    }
}

// ---- validation ----------------------------------------------------------------------------

/// `MARTIN_SCORE_STRICT` set (and not `0`/empty) → treat score warnings as fatal (for authoring + CI,
/// so a phase/bar typo can't silently ship). Unset → warnings are logged but the show still plays.
fn strict_scores() -> bool {
    std::env::var("MARTIN_SCORE_STRICT")
        .map(|v| !v.is_empty() && v != "0")
        .unwrap_or(false)
}

/// Structural lint of the parsed sections — returns human-readable WARNINGS (not errors: a bad score
/// should never stop the show). It catches the silent traps the DSL otherwise hides:
///   • a phase/bar-count mismatch — the classic one (`section x 8 4,4 fill` → 4+4+1≠8): the extra or
///     missing bars just repeat the last phase, producing dead/duplicated bars with no error.
///   • a drum pattern on a phase the section doesn't have (`x.kick p5` when x has 3 phases) — it
///     never plays.
///   • a melodic `p1`+ phrase — note lanes loop `p0` CONTINUOUSLY across the section (see
///     `NoteLane::bar`), so any `p1`+ is silently ignored, and a lane with ONLY a `p1` is dead silent.
fn validate(sections: &[Section]) -> Vec<String> {
    let mut w = Vec::new();
    for s in sections {
        let declared = s.phases.iter().sum::<u32>() + u32::from(s.fill);
        if declared != s.bars {
            w.push(format!(
                "section `{}`: {} bars but phases{} sum to {} — the extra/missing bars repeat the \
                 last phase (likely a typo)",
                s.name,
                s.bars,
                if s.fill { " + fill" } else { "" },
                declared
            ));
        }
        for (inst, lane) in [
            ("kick", &s.kick),
            ("snare", &s.snare),
            ("hat", &s.hat),
            ("stab", &s.stab),
        ] {
            if lane.phases.len() > s.phases.len() {
                w.push(format!(
                    "`{}.{inst}`: defines p{} but section `{}` has only {} phase(s) — that pattern \
                     never plays",
                    s.name,
                    lane.phases.len() - 1,
                    s.name,
                    s.phases.len()
                ));
            }
        }
        for (lname, lane) in [("lead", &s.lead), ("arp", &s.arp), ("bass", &s.bass)] {
            if lane.phases.iter().skip(1).any(|p| NoteLane::any(p)) {
                w.push(format!(
                    "`{}.{lname}`: p1+ phrases are ignored — melodic lanes loop p0 continuously \
                     across the section",
                    s.name
                ));
            }
            if lane.phases.len() > 1 && lane.phases.first().is_some_and(|p| !NoteLane::any(p)) {
                w.push(format!(
                    "`{}.{lname}`: no p0 phrase (only p1+), so the lane is SILENT — melodic lanes \
                     play p0; rename your phrase to p0",
                    s.name
                ));
            }
        }
    }
    w
}

// ---- parsing helpers -----------------------------------------------------------------------

/// Leading-dot-tolerant float parse (`.85` → 0.85).
fn pf(s: &str) -> Option<f32> {
    let s = s.trim();
    s.parse().ok().or_else(|| format!("0{s}").parse().ok())
}

fn parse_ramp(s: &str) -> Option<Ramp> {
    match s.split_once('>') {
        Some((a, b)) => Some(Ramp::new(pf(a)?, pf(b)?)),
        None => Some(Ramp::c(pf(s)?)),
    }
}

/// Parse a note name → frequency (Hz): letter `A`–`G`, optional `#`/`b`, octave (`A4` = 440 Hz).
fn note_freq(name: &str) -> Option<f32> {
    let mut chars = name.chars();
    let base: i32 = match chars.next()?.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let mut semi = base;
    let mut rest = chars.as_str();
    match rest.chars().next() {
        Some('#') => {
            semi += 1;
            rest = &rest[1..];
        }
        Some('b') => {
            semi -= 1;
            rest = &rest[1..];
        }
        _ => {}
    }
    let octave: i32 = rest.parse().ok()?;
    let midi = (octave + 1) * 12 + semi;
    Some(440.0 * 2f32.powf((midi as f32 - 69.0) / 12.0))
}

/// Parse a chord token: a note (letter + optional `#`/`b`) + optional trailing `m` for minor
/// (`Am`, `F`, `C#`, `Ebm`). The root is taken at octave 3.
fn parse_chord(s: &str) -> Option<Chord> {
    let s = s.trim();
    let (note, minor) = match s.strip_suffix('m') {
        Some(p) if !p.is_empty() => (p, true),
        _ => (s, false),
    };
    Some(Chord {
        root: note_freq(&format!("{note}3"))?,
        minor,
    })
}

/// Parse whitespace-separated note tokens (`A4`/`C#5`/… or `.`/`-`/`_` = rest) into a melodic
/// phrase: one or more bars of 16 slots each (so a 32/48/… token line is a 2/3/…-bar phrase). The
/// token count must be a positive multiple of 16.
fn parse_notes(s: &str) -> Option<Vec<[Option<f32>; 16]>> {
    let toks: Vec<&str> = s.split_whitespace().collect();
    if toks.is_empty() || !toks.len().is_multiple_of(16) {
        return None;
    }
    let mut bars = Vec::with_capacity(toks.len() / 16);
    for chunk in toks.chunks(16) {
        let mut bar = [None; 16];
        for (i, t) in chunk.iter().enumerate() {
            bar[i] = match *t {
                "." | "-" | "_" => None,
                n => Some(note_freq(n)?),
            };
        }
        bars.push(bar);
    }
    Some(bars)
}

const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

fn freq_to_midi(freq: f32) -> i32 {
    (69.0 + 12.0 * (freq / 440.0).log2()).round() as i32
}

/// Nearest note name (with octave) for a frequency — for `to_dsl`.
fn note_name(freq: f32) -> String {
    let midi = freq_to_midi(freq);
    format!(
        "{}{}",
        NOTE_NAMES[midi.rem_euclid(12) as usize],
        midi.div_euclid(12) - 1
    )
}

fn notes_str(g: &[Option<f32>; 16]) -> String {
    let toks: Vec<String> = g
        .iter()
        .map(|n| n.map(note_name).unwrap_or_else(|| ".".into()))
        .collect();
    toks.chunks(4)
        .map(|c| c.join(" "))
        .collect::<Vec<_>>()
        .join("  ")
}

/// A whole melodic phrase (1+ bars) on one line — each bar's `notes_str`, joined by 3 spaces.
fn notes_phrase(phrase: &[[Option<f32>; 16]]) -> String {
    phrase.iter().map(notes_str).collect::<Vec<_>>().join("   ")
}

fn chord_str(c: &Chord) -> String {
    let name = note_name(c.root); // e.g. "A3"
    let letter: String = name.chars().take_while(|ch| !ch.is_ascii_digit()).collect();
    format!("{letter}{}", if c.minor { "m" } else { "" })
}

/// Parse a 16-step grid: `x`/`X` = hit, `.`/`-`/`_` = rest; spaces / `|` group separators ignored.
fn parse_pattern(s: &str) -> Option<[bool; 16]> {
    let mut out = [false; 16];
    let mut i = 0;
    for c in s.chars() {
        match c {
            ' ' | '\t' | '|' => {}
            'x' | 'X' => {
                *out.get_mut(i)? = true;
                i += 1;
            }
            '.' | '-' | '_' => {
                *out.get_mut(i)? = false;
                i += 1;
            }
            _ => return None,
        }
    }
    (i == 16).then_some(out)
}

fn pat_str(p: &[bool; 16]) -> String {
    let mut s = String::with_capacity(19);
    for (i, &b) in p.iter().enumerate() {
        if i > 0 && i % 4 == 0 {
            s.push(' ');
        }
        s.push(if b { 'x' } else { '.' });
    }
    s
}

fn fnum(v: f32) -> String {
    format!("{v}")
}

fn ramp_str(r: &Ramp) -> String {
    if (r.a - r.b).abs() < 1e-6 {
        fnum(r.a)
    } else {
        format!("{}>{}", fnum(r.a), fnum(r.b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_is_consistent() {
        let s = Score::builtin();
        assert!(s.bpm > 0.0);
        assert!(s.beat() > 0.0);
        assert!((s.bar() - BEATS_PER_BAR * s.beat()).abs() < 1e-6);
        assert!(s.demo_len() > 0.0);
        assert!(!s.sections.is_empty());
    }

    #[test]
    fn section_starts_are_monotonic_and_first_is_zero() {
        let s = Score::builtin();
        assert_eq!(s.sections[0].start_bar, 0);
        let mut prev = 0;
        for sec in &s.sections {
            assert!(sec.start_bar >= prev, "sections must be in order");
            prev = sec.start_bar;
        }
        assert_eq!(s.section_start_secs(0), 0.0);
    }

    #[test]
    fn anchor_seconds_resolves_every_form() {
        let s = Score::builtin();
        assert_eq!(s.anchor_seconds("start"), Some(0.0));
        // a real section name (the first) resolves to its start.
        let first = s.sections[0].name.clone();
        assert_eq!(s.anchor_seconds(&first), Some(0.0));
        // bar / beat forms, with and without the colon.
        assert_eq!(s.anchor_seconds("bar1"), Some(s.bar()));
        assert_eq!(s.anchor_seconds("bar:2"), Some(2.0 * s.bar()));
        assert_eq!(s.anchor_seconds("beat4"), Some(4.0 * s.beat()));
        // a plain number is seconds; whitespace + case are tolerated.
        assert_eq!(s.anchor_seconds("  2.5 "), Some(2.5));
        assert_eq!(s.anchor_seconds("nope"), None);
    }

    #[test]
    fn sharp_notes_parse_and_real_comments_still_strip() {
        // `#` is the comment char, but a mid-token `#` (the note F#5 / chord F#) must NOT be eaten —
        // and a genuine trailing `# comment` must still be stripped (else the lead line mis-counts).
        let dsl = "bpm 120\n\
                   chords F#\n\
                   section a 4 4\n\
                   a.lead p0: F#5 . . .  . . . .  . . . .  . . . .   # trailing comment with # inside\n";
        assert!(
            Score::from_str(dsl).is_ok(),
            "F#5 + a trailing comment should parse"
        );
        // a leading-`#` line is a comment (ignored); a bad note still errors.
        assert!(Score::from_str(
            "bpm 120\nchords G\nsection a 4 4\na.lead p0: Z9 . . . . . . . . . . . . . . .\n"
        )
        .is_err());
    }

    #[test]
    fn per_section_chords_override_the_global_progression() {
        // global = G major everywhere; the `verse` section flips to a G-minor `chords:` line. The
        // global chord at the verse's time must be the section's (minor), not the global (major).
        let dsl = "bpm 120\nchords G\n\
                   section intro 2 2\nsection verse 2 2\n\
                   verse.chords: Am\n";
        let s = Score::from_str(dsl).unwrap();
        let intro = s.chord_at(0.1); // intro → global G major
        let verse = s.chord_at(s.section_start_secs(1) + 0.1); // verse → section A minor
        assert!(!intro.minor, "intro uses the global G major");
        assert!(verse.minor, "verse uses its own A-minor override");
        assert!(
            (verse.root - note_freq("A3").unwrap()).abs() < 1.0,
            "verse root is A, not the global G"
        );
    }

    #[test]
    fn multi_bar_lead_phrase_advances_and_loops() {
        // a 2-bar phrase (32 tokens): bar0 has C5 at slot 0, bar1 has E5 at slot 0. Over a 4-bar
        // section the lead should play C5, E5, C5, E5 at each bar's downbeat (the phrase loops).
        let dsl = "bpm 120\nchords C\nsection a 4 4\n\
                   a.lead p0: C5 . . . . . . . . . . . . . . .  E5 . . . . . . . . . . . . . . .\n";
        let s = Score::from_str(dsl).unwrap();
        let notes = s.lead_notes();
        assert_eq!(notes.len(), 4, "4 downbeat notes over 4 bars");
        let c5 = note_freq("C5").unwrap();
        let e5 = note_freq("E5").unwrap();
        let pitch = |f: f32| {
            if (f - c5).abs() < 1.0 {
                'C'
            } else if (f - e5).abs() < 1.0 {
                'E'
            } else {
                '?'
            }
        };
        let seq: String = notes.iter().map(|&(_, f)| pitch(f)).collect();
        assert_eq!(seq, "CECE", "the phrase advances per bar and loops");
        // and the bar times line up with the grid.
        assert!((notes[1].0 - s.bar()).abs() < 1e-3);
    }

    #[test]
    fn single_bar_lead_still_repeats_every_bar() {
        // backward-compat: a 1-bar phrase plays the same bar every bar (the old behaviour).
        let dsl = "bpm 120\nchords C\nsection a 3 3\na.lead p0: G5 . . . . . . . . . . . . . . .\n";
        let s = Score::from_str(dsl).unwrap();
        assert_eq!(s.lead_notes().len(), 3); // G5 on every one of the 3 bars
    }

    #[test]
    fn validate_flags_phase_mismatch_and_ignored_melodic_phases() {
        // 8-bar section but phases 4,4 + fill = 9 (mismatch); a lead phrase written as p1 (no p0) is
        // both "p1+ ignored" AND "silent". All are WARNINGS — the score still parses.
        let dsl = "bpm 120\nchords C\nsection a 8 4,4 fill\n\
                   a.kick p5: x... .... .... ....\n\
                   a.lead p1: C5 . . . . . . . . . . . . . . .\n";
        let s = Score::from_str(dsl).expect("warnings, not errors — still parses");
        let w = validate(&s.sections);
        assert!(
            w.iter().any(|m| m.contains("bars but phases")),
            "phase/bar mismatch flagged: {w:?}"
        );
        assert!(
            w.iter().any(|m| m.contains(".kick`: defines p5")),
            "out-of-range drum phase flagged: {w:?}"
        );
        assert!(
            w.iter().any(|m| m.contains("p1+ phrases are ignored")),
            "ignored melodic phase flagged: {w:?}"
        );
        assert!(
            w.iter().any(|m| m.contains("SILENT")),
            "silent (p1-only) melodic lane flagged: {w:?}"
        );
        // and a clean score yields NO warnings.
        assert!(
            validate(&Score::builtin().sections).is_empty(),
            "the built-in score must be warning-clean"
        );
    }

    #[test]
    fn per_section_set_overrides_the_global_knob_and_round_trips() {
        let dsl = "bpm 120\nchords C\nsection intro 2 2\nsection drop 2 2\n\
                   set house=0.1\ndrop.set house=0.3 lead=0.9\n";
        let s = Score::from_str(dsl).unwrap();
        let drop_t = s.section_start_secs(1) + 0.1;
        assert!(
            (s.param("house", 0.0) - 0.1).abs() < 1e-6,
            "global house = 0.1"
        );
        assert!(
            (s.param_at(0.1, "house", 0.0) - 0.1).abs() < 1e-6,
            "intro falls back to the global 0.1"
        );
        assert!(
            (s.param_at(drop_t, "house", 0.0) - 0.3).abs() < 1e-6,
            "drop overrides house to 0.3"
        );
        assert!(
            (s.param_at(drop_t, "lead", 0.0) - 0.9).abs() < 1e-6,
            "drop also overrides lead"
        );
        // the override survives a to_dsl → from_str round-trip.
        let s2 = Score::from_str(&s.to_dsl()).unwrap();
        assert!(
            (s2.param_at(drop_t, "house", 0.0) - 0.3).abs() < 1e-6,
            "the per-section override round-trips through to_dsl"
        );
    }

    #[test]
    fn per_section_fx_list_overrides_the_name_defaults_and_round_trips() {
        // no `fx:` line → the built-in name-based defaults (a drop gets the wall + jet, not casio).
        let plain = Score::from_str("bpm 120\nchords C\nsection drop 4 4\n").unwrap();
        assert!(plain.fx_on("drop", "wall"));
        assert!(plain.fx_on("drop", "jet"));
        assert!(!plain.fx_on("drop", "casio"));
        // an explicit `<section>.fx:` line is authoritative: keep the wall, drop the jet.
        let custom =
            Score::from_str("bpm 120\nchords C\nsection drop 4 4\ndrop.fx: wall house\n").unwrap();
        assert!(custom.fx_on("drop", "wall"));
        assert!(custom.fx_on("drop", "house"));
        assert!(
            !custom.fx_on("drop", "jet"),
            "explicit fx list omits the jet"
        );
        // round-trips through to_dsl.
        let s2 = Score::from_str(&custom.to_dsl()).unwrap();
        assert!(s2.fx_on("drop", "wall") && !s2.fx_on("drop", "jet"));
    }

    #[test]
    fn drum_hits_are_ordered_and_in_range() {
        let s = Score::builtin();
        let hits = s.hits(Inst::Kick);
        assert!(!hits.is_empty(), "the built-in track should kick");
        assert!(hits.windows(2).all(|w| w[0] <= w[1]), "hits in time order");
        assert!(hits.iter().all(|&t| t >= 0.0 && t <= s.demo_len()));
    }
}
