//! Music score — the *composition*, data-driven. Ported from Cinder's (Kristian Vlaardingerbroek,
//! deFEEST) `term-demo` (MIT, Outline 2026): the BPM→beat→bar grid, the section timeline
//! (intro→build→drop→breakdown→climax→outro), the drum patterns and the per-section dynamics that
//! the synth (`audio.rs`) and the visual `@@anchor`s both read.
//!
//! The score is no longer hard-coded: `Score::builtin()` is the default, but `MARTIN_SCORE=<file>`
//! loads a **tracker-DSL** score (see `from_str` / `USAGE.md`), and `MARTIN_SCORE_DUMP=<file>`
//! exports the built-in as an editable starting point. The *instrument* (how a kick/stab sounds)
//! stays in `audio.rs` — this file is purely the score. 16 steps per bar (16th notes).

const SLOTS_PER_BAR: i64 = 16;
const BEATS_PER_BAR: f32 = 4.0;

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
    fn of(phases: &[[bool; 16]], fill: [bool; 16]) -> Self {
        Self {
            phases: phases.to_vec(),
            fill,
        }
    }
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
            start_bar: 0,
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
        if self.fill {
            let total: u32 = self.phases.iter().sum::<u32>() + 1;
            if into >= total.saturating_sub(1) {
                return 255;
            }
        }
        let mut acc = 0;
        for (i, &p) in self.phases.iter().enumerate() {
            acc += p;
            if into < acc {
                return i as u8;
            }
        }
        self.phases.len().saturating_sub(1) as u8
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
    pub sections: Vec<Section>,
    total_bars: u32,
}

impl Score {
    /// Lay out the sections (cumulative `start_bar`, total length) — the single place section
    /// timing is derived, so the file and the built-in agree.
    fn new(bpm: f32, mut sections: Vec<Section>) -> Self {
        let mut bar = 0;
        for s in &mut sections {
            s.start_bar = bar;
            bar += s.bars;
        }
        Self {
            bpm,
            sections,
            total_bars: bar,
        }
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

    /// The time of the most recent `inst` hit at/just-before `t` (None if there isn't one within a
    /// few bars). Drives the per-voice envelopes in the synth.
    pub fn last_hit(&self, inst: Inst, t: f32) -> Option<f32> {
        if t < 0.0 {
            return None;
        }
        let sl = self.slot_len();
        let mut slot = ((t + sl * 1e-3) / sl).floor() as i64;
        if slot >= 0 && (slot as f32) * sl > t {
            slot -= 1;
        }
        for _ in 0..(SLOTS_PER_BAR * 4) {
            if slot < 0 {
                return None;
            }
            let kt = slot as f32 * sl;
            if self.lane_hits(inst, kt)[slot.rem_euclid(SLOTS_PER_BAR) as usize] {
                return Some(kt);
            }
            slot -= 1;
        }
        None
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
        let Ok(path) = std::env::var("MARTIN_SCORE") else {
            return Score::builtin();
        };
        match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| Score::from_str(&t))
        {
            Ok(s) => {
                eprintln!(
                    "score: loaded {path} ({} sections, {:.1}s)",
                    s.sections.len(),
                    s.demo_len()
                );
                s
            }
            Err(e) => {
                eprintln!("score: {path}: {e} — using built-in");
                Score::builtin()
            }
        }
    }

    /// Parse a tracker-DSL score (see `to_dsl` for the shape / `USAGE.md` for the grammar).
    pub fn from_str(text: &str) -> Result<Score, String> {
        let mut bpm = 140.0_f32;
        let mut sections: Vec<Section> = Vec::new();
        let find = |sections: &[Section], name: &str| sections.iter().position(|s| s.name == name);

        for (n, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let ln = n + 1;
            let first = line.split_whitespace().next().unwrap();

            // pattern line: `<section>.<inst> p<N>|fill: <16 steps>`
            if first.contains('.') {
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
                let grid = parse_pattern(pat)
                    .ok_or_else(|| format!("line {ln}: pattern must be 16 of x/."))?;
                let lane = sections[si]
                    .lane_mut(inst)
                    .ok_or_else(|| format!("line {ln}: unknown instrument `{inst}`"))?;
                if phase_tok.eq_ignore_ascii_case("fill") {
                    lane.fill = grid;
                } else {
                    let p: usize = phase_tok
                        .trim_start_matches('p')
                        .parse()
                        .map_err(|_| format!("line {ln}: bad phase `{phase_tok}`"))?;
                    if lane.phases.len() <= p {
                        lane.phases.resize(p + 1, [false; 16]);
                    }
                    lane.phases[p] = grid;
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
                other => return Err(format!("line {ln}: unknown keyword `{other}`")),
            }
        }
        if sections.is_empty() {
            return Err("no sections defined".into());
        }
        Ok(Score::new(bpm, sections))
    }

    /// Serialize back to the tracker DSL — `MARTIN_SCORE_DUMP` writes the built-in this way for a
    /// ready-to-edit starting file (and it round-trips through `from_str`).
    pub fn to_dsl(&self) -> String {
        let mut o = String::new();
        o.push_str("# martin score — tracker DSL. Edit + load with MARTIN_SCORE=<this file>.\n");
        o.push_str(&format!("bpm {}\n\n", fnum(self.bpm)));
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

    /// The default score (six-section 140-BPM arc) — identical to the pre-data-driven hard-coded
    /// version, so behaviour is unchanged when `MARTIN_SCORE` is unset.
    pub fn builtin() -> Score {
        let sec = |name,
                   bars,
                   phases: &[u32],
                   fill,
                   gain: (f32, f32),
                   sub: (f32, f32),
                   mids: (f32, f32),
                   k,
                   sn,
                   h,
                   st| Section {
            name: String::from(name),
            bars,
            phases: phases.to_vec(),
            fill,
            gain: Ramp::new(gain.0, gain.1),
            sub: Ramp::new(sub.0, sub.1),
            mids: Ramp::new(mids.0, mids.1),
            kick: k,
            snare: sn,
            hat: h,
            stab: st,
            start_bar: 0,
        };
        let sections = vec![
            sec(
                "intro",
                4,
                &[4],
                false,
                (0.5, 0.5),
                (0.25, 0.25),
                (0.5, 0.5),
                Lane::of(&[KICK_INTRO], EMPTY),
                Lane::of(&[SNARE_INTRO], EMPTY),
                Lane::of(&[HAT_INTRO], EMPTY),
                Lane::of(&[STAB_INTRO], EMPTY),
            ),
            sec(
                "build",
                10,
                &[4, 5],
                true,
                (0.85, 0.85),
                (0.25, 0.8),
                (0.7, 0.7),
                Lane::of(&[KICK_BUILD_P0, KICK_BUILD_P1], KICK_BUILD_FILL),
                Lane::of(&[SNARE_BUILD_P0, SNARE_BUILD_P1], SNARE_BUILD_FILL),
                Lane::of(&[HAT_BUILD_P0, HAT_BUILD_P1], HAT_BUILD_FILL),
                Lane::of(&[STAB_BUILD_P0, STAB_BUILD_P1], STAB_BUILD_FILL),
            ),
            sec(
                "drop",
                10,
                &[4, 5],
                true,
                (1.0, 1.0),
                (1.0, 1.0),
                (0.9, 0.9),
                Lane::of(&[KICK_DROP_P0, KICK_DROP_P1], KICK_DROP_FILL),
                Lane::of(&[SNARE_DROP_P0, SNARE_DROP_P1], SNARE_DROP_FILL),
                Lane::of(&[HAT_DROP_P0, HAT_DROP_P1], HAT_DROP_FILL),
                Lane::of(&[STAB_DROP_P0, STAB_DROP_P1], STAB_DROP_FILL),
            ),
            sec(
                "breakdown",
                6,
                &[3, 2],
                true,
                (0.6, 0.6),
                (0.15, 0.15),
                (0.6, 0.6),
                Lane::of(&[KICK_BREAKDOWN_P0, KICK_BREAKDOWN_P1], KICK_BREAKDOWN_FILL),
                Lane::of(
                    &[SNARE_BREAKDOWN_P0, SNARE_BREAKDOWN_P1],
                    SNARE_BREAKDOWN_FILL,
                ),
                Lane::of(&[HAT_BREAKDOWN_P0, HAT_BREAKDOWN_P1], HAT_BREAKDOWN_FILL),
                Lane::of(&[STAB_BREAKDOWN_P0, STAB_BREAKDOWN_P1], STAB_BREAKDOWN_FILL),
            ),
            sec(
                "climax",
                18,
                &[6, 6, 5],
                true,
                (1.0, 1.0),
                (0.9, 0.9),
                (1.0, 1.0),
                Lane::of(
                    &[KICK_CLIMAX_P0, KICK_CLIMAX_P1, KICK_CLIMAX_P2],
                    KICK_CLIMAX_FILL,
                ),
                Lane::of(
                    &[SNARE_CLIMAX_P0, SNARE_CLIMAX_P1, SNARE_CLIMAX_P2],
                    SNARE_CLIMAX_FILL,
                ),
                Lane::of(
                    &[HAT_CLIMAX_P0, HAT_CLIMAX_P1, HAT_CLIMAX_P2],
                    HAT_CLIMAX_FILL,
                ),
                Lane::of(
                    &[STAB_CLIMAX_P0, STAB_CLIMAX_P1, STAB_CLIMAX_P2],
                    STAB_CLIMAX_FILL,
                ),
            ),
            sec(
                "outro",
                6,
                &[5],
                true,
                (0.7, 0.7),
                (0.4, 0.4),
                (0.45, 0.45),
                Lane::of(&[KICK_OUTRO_P0], KICK_OUTRO_FILL),
                Lane::of(&[SNARE_OUTRO_P0], SNARE_OUTRO_FILL),
                Lane::of(&[HAT_OUTRO_P0], HAT_OUTRO_FILL),
                Lane::of(&[STAB_OUTRO_P0], STAB_OUTRO_FILL),
            ),
        ];
        Score::new(140.0, sections)
    }
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

// ---- built-in patterns (16 slots per bar; X = hit) -----------------------------------------
const F: bool = false;
const X: bool = true;
const EMPTY: [bool; 16] = [F; 16];

const KICK_INTRO: [bool; 16] = [F; 16];
const KICK_BUILD_P0: [bool; 16] = [X, F, F, F, F, F, F, F, F, F, F, F, X, F, F, F];
const KICK_BUILD_P1: [bool; 16] = [X, F, F, F, F, F, X, F, F, F, F, F, X, F, F, F];
const KICK_BUILD_FILL: [bool; 16] = [X, F, F, F, F, F, F, F, F, F, F, F, X, F, F, F];
const KICK_DROP_P0: [bool; 16] = [X, F, F, F, F, F, X, F, F, F, X, F, F, F, F, F];
const KICK_DROP_P1: [bool; 16] = [X, F, F, F, F, F, F, F, X, F, F, F, X, F, F, X];
const KICK_DROP_FILL: [bool; 16] = [X, F, F, F, F, F, F, F, F, F, F, F, X, F, F, F];
const KICK_BREAKDOWN_P0: [bool; 16] = [X, F, F, F, F, F, F, F, F, F, F, F, F, F, F, F];
const KICK_BREAKDOWN_P1: [bool; 16] = [X, F, F, F, F, F, F, F, F, F, F, F, X, F, F, F];
const KICK_BREAKDOWN_FILL: [bool; 16] = [X, F, F, F, F, F, F, F, F, F, F, F, F, F, F, F];
const KICK_CLIMAX_P0: [bool; 16] = [X, F, F, X, F, F, X, F, F, F, X, F, X, F, F, F];
const KICK_CLIMAX_P1: [bool; 16] = [X, F, F, F, X, F, F, X, F, X, F, F, X, F, F, X];
const KICK_CLIMAX_P2: [bool; 16] = [X, F, X, F, X, F, X, F, X, F, X, F, X, F, X, F];
const KICK_CLIMAX_FILL: [bool; 16] = [X, F, F, F, F, F, F, F, F, F, F, F, X, F, F, F];
const KICK_OUTRO_P0: [bool; 16] = [X, F, F, F, F, F, F, F, X, F, F, F, F, F, F, F];
const KICK_OUTRO_FILL: [bool; 16] = [X, F, F, F, F, F, F, F, F, F, F, F, F, F, F, F];

const SNARE_INTRO: [bool; 16] = [F; 16];
const SNARE_BUILD_P0: [bool; 16] = [F; 16];
const SNARE_BUILD_P1: [bool; 16] = [F, F, F, F, X, F, F, F, F, F, F, F, X, F, F, F];
const SNARE_BUILD_FILL: [bool; 16] = [F, F, F, F, X, F, X, F, X, F, X, F, X, X, X, X];
const SNARE_DROP_P0: [bool; 16] = [F, F, F, F, X, F, F, X, F, F, F, F, X, F, F, X];
const SNARE_DROP_P1: [bool; 16] = [F, F, X, F, F, F, F, F, F, F, X, F, X, F, F, F];
const SNARE_DROP_FILL: [bool; 16] = [F, F, F, F, X, F, X, X, F, X, X, X, X, X, X, X];
const SNARE_BREAKDOWN_P0: [bool; 16] = [F; 16];
const SNARE_BREAKDOWN_P1: [bool; 16] = [F, F, F, F, F, F, F, F, F, F, F, F, X, F, F, F];
const SNARE_BREAKDOWN_FILL: [bool; 16] = [F, F, X, X, F, X, X, X, F, X, X, X, X, X, X, X];
const SNARE_CLIMAX_P0: [bool; 16] = [F, F, F, F, X, F, F, X, F, F, F, F, X, F, X, X];
const SNARE_CLIMAX_P1: [bool; 16] = [F, F, X, F, F, F, X, F, F, X, F, F, F, F, X, X];
const SNARE_CLIMAX_P2: [bool; 16] = [X, F, X, F, X, F, X, F, X, F, X, F, X, F, X, X];
const SNARE_CLIMAX_FILL: [bool; 16] = [F, F, F, F, X, X, X, X, X, X, X, X, X, X, X, X];
const SNARE_OUTRO_P0: [bool; 16] = [F, F, F, F, X, F, F, F, F, F, F, F, X, F, F, F];
const SNARE_OUTRO_FILL: [bool; 16] = [F, F, F, F, X, F, F, F, F, F, F, F, F, F, F, F];

const HAT_INTRO: [bool; 16] = [F; 16];
const HAT_BUILD_P0: [bool; 16] = [X, F, F, F, X, F, F, F, X, F, F, F, X, F, F, F];
const HAT_BUILD_P1: [bool; 16] = [X, F, X, F, X, F, X, F, X, F, X, F, X, F, X, F];
const HAT_BUILD_FILL: [bool; 16] = [X; 16];
const HAT_DROP_P0: [bool; 16] = [X; 16];
const HAT_DROP_P1: [bool; 16] = [X; 16];
const HAT_DROP_FILL: [bool; 16] = [X; 16];
const HAT_BREAKDOWN_P0: [bool; 16] = [F; 16];
const HAT_BREAKDOWN_P1: [bool; 16] = [X, F, F, F, X, F, F, F, X, F, F, F, X, F, F, F];
const HAT_BREAKDOWN_FILL: [bool; 16] = [X; 16];
const HAT_CLIMAX_P0: [bool; 16] = [X; 16];
const HAT_CLIMAX_P1: [bool; 16] = [X; 16];
const HAT_CLIMAX_P2: [bool; 16] = [X; 16];
const HAT_CLIMAX_FILL: [bool; 16] = [X; 16];
const HAT_OUTRO_P0: [bool; 16] = [X, F, X, F, X, F, X, F, X, F, X, F, X, F, X, F];
const HAT_OUTRO_FILL: [bool; 16] = [F; 16];

const STAB_INTRO: [bool; 16] = [F; 16];
const STAB_BUILD_P0: [bool; 16] = [F; 16];
const STAB_BUILD_P1: [bool; 16] = [F, F, F, F, F, F, X, F, F, F, F, F, F, F, F, F];
const STAB_BUILD_FILL: [bool; 16] = [F, F, F, F, F, F, F, F, F, F, F, F, F, F, F, X];
const STAB_DROP_P0: [bool; 16] = [F, F, F, F, F, F, X, F, F, F, F, F, F, F, X, F];
const STAB_DROP_P1: [bool; 16] = [F, F, F, F, X, F, F, F, F, F, F, F, X, F, F, F];
const STAB_DROP_FILL: [bool; 16] = [F, F, F, F, F, F, F, F, F, F, F, F, F, F, F, X];
const STAB_BREAKDOWN_P0: [bool; 16] = [F, F, F, F, F, F, F, F, X, F, F, F, F, F, F, F];
const STAB_BREAKDOWN_P1: [bool; 16] = [F; 16];
const STAB_BREAKDOWN_FILL: [bool; 16] = [F; 16];
const STAB_CLIMAX_P0: [bool; 16] = [F, F, F, F, F, F, X, F, F, F, X, F, F, F, X, F];
const STAB_CLIMAX_P1: [bool; 16] = [F, X, F, F, F, F, X, F, F, X, F, F, F, F, X, F];
const STAB_CLIMAX_P2: [bool; 16] = [X, F, F, X, F, X, X, F, X, F, X, F, X, F, X, X];
const STAB_CLIMAX_FILL: [bool; 16] = [F, F, F, F, F, F, F, F, F, F, F, F, F, F, F, X];
const STAB_OUTRO_P0: [bool; 16] = [F, F, F, F, F, F, X, F, F, F, F, F, F, F, F, F];
const STAB_OUTRO_FILL: [bool; 16] = [F, F, F, F, F, F, X, F, F, F, F, F, F, F, F, F];
