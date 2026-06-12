//! The *instrument* layer: each voice is a FunDSP graph (a 0-input → 1-output unit) the renderer
//! plays into the bed. The distortion-heavy ones (saw/tanh stacks) optionally 2× oversample via the
//! crate's `oversampling()` flag. The *score* (what they play) lives in `crate::score`.

use fundsp::prelude32::*;

use super::oversampling;

/// Snare: high-passed noise crack + a short tone body + a clap layer (low-passed noise with slower
/// decay for that back-breaking demoscene crack — the clap fills the low-mids the crack misses).
pub(super) fn snare() -> Box<dyn AudioUnit> {
    Box::new(
        ((noise() >> highpass_hz(1200.0, 0.7)) * envelope(|t: f32| (-t * 26.0).exp())
            + sine_hz(190.0) * envelope(|t: f32| (-t * 24.0).exp()) * 0.5
            + (noise() >> highpass_hz(280.0, 0.6)) * envelope(|t: f32| (-t * 16.0).exp()) * 0.35)
            * 0.75,
    )
}

/// Hat: bright high-passed noise crack + a body layer (lower, slower) so it has a "tick" body
/// behind the sizzle — without it every hat is a wisp, not a percussion hit.
pub(super) fn hat() -> Box<dyn AudioUnit> {
    Box::new(
        ((noise() >> highpass_hz(7000.0, 0.7)) * envelope(|t: f32| (-t * 80.0).exp()) * 0.55
            + (noise() >> highpass_hz(3500.0, 0.5)) * envelope(|t: f32| (-t * 40.0).exp()) * 0.45)
            * 0.9,
    )
}

/// Stab: one chord note as a saw through a low-pass with a plucky attack (rendered per triad note
/// so the three can be panned wide).
pub(super) fn stab(freq: f32) -> Box<dyn AudioUnit> {
    Box::new(
        (saw_hz(freq) >> lowpass_hz(1600.0, 0.8) >> highpass_hz(180.0, 0.7))
            * envelope(|t: f32| {
                let a = 0.01;
                if t < a { t / a } else { (-(t - a) * 7.0).exp() }
            })
            * 0.3,
    )
}

/// Pad: one chord note an octave down through a soft low-pass, slow swell, high-passed off the
/// low-mids so it stops stacking into the same band as everything else (body/warmth, not honk).
pub(super) fn pad(freq: f32) -> Box<dyn AudioUnit> {
    Box::new(
        (saw_hz(freq * 0.5) >> lowpass_hz(900.0, 0.6) >> highpass_hz(150.0, 0.7))
            * envelope(|t: f32| (t * 2.0).min(1.0))
            * 0.22,
    )
}

/// Bass: a moving Reese — a sub sine + two ±8-cent-detuned saws (the phasing growl) through a
/// resonant low-pass that drops from ~1.4 kHz to ~900 Hz, with per-VOICE tanh drive so the grit
/// lives on the bass itself, not smeared across the whole bus.
pub(super) fn bass(freq: f32, vel: f32) -> Box<dyn AudioUnit> {
    let mk = move || {
        let saws = sine_hz(freq)
            + saw_hz(freq) * 0.6
            + saw_hz(freq * 1.008) * 0.5
            + saw_hz(freq * 0.992) * 0.5;
        let cut = envelope(|t: f32| 900.0 + 500.0 * (-t * 3.0).exp());
        let drive = 1.8 + 0.8 * vel; // harder notes growl harder
        ((saws | cut) >> lowpass_q(1.4) >> shape_fn(move |x| (x * drive).tanh()))
            * envelope(|t: f32| {
                let a = 0.005;
                if t < a { t / a } else { (-(t - a) * 4.0).exp() }
            })
            * 0.46
    };
    if oversampling() {
        Box::new(oversample(mk()))
    } else {
        Box::new(mk())
    }
}

/// Wooz-bass: thick + dark in the low-mids, a slow GROWL that develops AFTER the hit, and a
/// slightly-detuned, woozy quality — the pitch never quite settles. How each trait is built:
///   • dark low-mid body — a sub sine for weight + two detuned saws, all through a RESONANT low-pass
///     parked low (~220 Hz, Q≈3.2) so it sits in the low-mids and never gets bright.
///   • growl-after-the-hit — the low-pass cutoff is WOBBLED by a ~5.5 Hz LFO whose depth ramps IN
///     over ~0.4 s, so the note lands clean and the growl only opens up as it sustains.
///   • woozy/unstable pitch — a ~4 Hz vibrato + a slower ~0.6 Hz drift on EACH oscillator (at
///     different rates/phases) on top of ±12-cent detuning, so the three voices beat against each
///     other and the pitch drifts. Best on HELD notes (it needs time to develop). A palette voice —
///     wire it into the score where a sustained woozy sub fits (`set woozbass=1` swaps it into the
///     bass note-lane; audition the voice alone with the `woozbass_demo` test).
pub(super) fn woozbass(freq: f32) -> Box<dyn AudioUnit> {
    use std::f32::consts::TAU;
    // independent vibrato + slow drift per oscillator → they never lock, so the pitch feels unstable.
    let f_sub = lfo(move |t: f32| {
        freq * (1.0 + 0.006 * (t * 4.3 * TAU).sin() + 0.004 * (t * 0.6 * TAU).sin())
    });
    let f_up = lfo(move |t: f32| freq * 1.007 * (1.0 + 0.006 * (t * 4.1 * TAU + 1.0).sin()));
    let f_dn = lfo(move |t: f32| freq * 0.993 * (1.0 + 0.005 * (t * 3.7 * TAU + 2.0).sin()));
    let oscs = (f_sub >> sine()) * 0.7 + (f_up >> saw()) * 0.45 + (f_dn >> saw()) * 0.45;
    // the developing growl: a resonant-LPF cutoff wobble whose depth eases in over ~0.4 s.
    let cut = lfo(move |t: f32| {
        let grow = (t / 0.4).min(1.0);
        220.0 + grow * 230.0 * ((t * 5.5 * TAU).sin() * 0.5 + 0.5)
    });
    Box::new(
        ((oscs | cut | constant(3.2)) >> lowpass())
            * envelope(|t: f32| {
                let a = 0.008;
                if t < a {
                    t / a // quick, clean attack...
                } else {
                    0.6 + 0.4 * (-(t - a) * 0.6).exp() // ...then a long sustain so the growl can bloom
                }
            })
            * 0.5,
    )
}

/// Lead: a 5-saw detuned stack with a per-note FILTER ENVELOPE — the cutoff sweeps down from ~4.9 kHz
/// to ~700 Hz so every note plucks/opens and settles instead of droning through a fixed cutoff (a
/// static cutoff on a saw is literally an organ). Softsign drive for brass bite; no sub-octave (that
/// read as an organ pipe).
pub(super) fn lead(freq: f32, vel: f32) -> Box<dyn AudioUnit> {
    use std::f32::consts::TAU;
    let mk = move || {
        // a gentle vibrato that SWELLS IN over the note — the lead leans into the note like a singer
        // instead of one static, ethereal tone. Each saw gets its own phase (lush, decorrelated).
        let vib = move |mult: f32, ph: f32| {
            lfo(move |t: f32| {
                let depth = 0.005 * (t * 1.4).min(1.0);
                freq * mult * (1.0 + depth * (t * 5.0 * TAU + ph).sin())
            })
        };
        let saws = ((vib(1.0, 0.0) >> saw())
            + (vib(1.007, 1.0) >> saw())
            + (vib(0.993, 2.0) >> saw())
            + (vib(1.014, 3.0) >> saw())
            + (vib(0.986, 4.0) >> saw()))
            * 0.18;
        // a higher floor + a slower sweep so the note stays PRESENT and bright (it SINGS) instead of
        // closing down to a thin/ethereal whisper; the sweep PEAK still tracks velocity.
        let top = 2400.0 + 2600.0 * vel;
        let cut = envelope(move |t: f32| 1550.0 + top * (-t * 3.4).exp());
        ((saws | cut) >> lowpass_q(0.8) >> shape(Softsign(0.4 + 0.4 * vel)))
            * envelope(|t: f32| {
                let a = 0.02;
                if t < a {
                    t / a
                } else {
                    0.55 + 0.45 * (-(t - a) * 0.85).exp() // high sustain floor → the note holds + SINGS
                }
            })
            * 0.8
    };
    if oversampling() {
        Box::new(oversample(mk()))
    } else {
        Box::new(mk())
    }
}

/// Arp: short filtered pluck. Lower and quieter than the old bright square arp so it reads as
/// motion in the groove, not late-90s game melody.
pub(super) fn arp(freq: f32, vel: f32) -> Box<dyn AudioUnit> {
    let osc = saw_hz(freq) * 0.7 + square_hz(freq) * 0.15;
    let top = 2500.0 + 2500.0 * vel;
    let cut = envelope(move |t: f32| 600.0 + top * (-t * 22.0).exp());
    Box::new(
        ((osc | cut) >> lowpass_q(0.9) >> shape(Atan(0.5)))
            * envelope(|t: f32| {
                let a = 0.008;
                if t < a { t / a } else { (-(t - a) * 7.5).exp() }
            })
            * 0.24,
    )
}

/// Supersaw: 7 detuned saws + a sub-octave saw through a bright-ish filter, slow swell — the wide
/// "epic" chord wall for the drop/climax. Held a full bar per chord note (panned wide by chord_spread).
pub(super) fn supersaw(freq: f32) -> Box<dyn AudioUnit> {
    let mk = move || {
        let saws = (saw_hz(freq)
            + saw_hz(freq * 1.006)
            + saw_hz(freq * 0.994)
            + saw_hz(freq * 1.013)
            + saw_hz(freq * 0.987)
            + saw_hz(freq * 1.020)
            + saw_hz(freq * 0.980))
            * 0.13;
        let cut = envelope(|t: f32| 1300.0 + 3200.0 * (t * 1.0).min(1.0)); // filter swells open
        // HP off the sub, then DRIVE it (rawstyle screech grit) — a hard wall, not a polite pad.
        ((saws | cut) >> lowpass_q(0.7) >> highpass_hz(180.0, 0.7) >> shape(Tanh(1.8)))
            * envelope(|t: f32| (t * 3.0).min(1.0))
            * 0.42
    };
    if oversampling() {
        Box::new(oversample(mk()))
    } else {
        Box::new(mk())
    }
}

/// Choir / ensemble pad: a wide bank of detuned saws + a sub-octave sine body through a soft filter
/// with a slow swell — lush grandeur layered UNDER the supersaw wall in the big sections (it carries
/// the warmth/size while the supersaw carries the bright edge). The new diffuse reverb makes it bloom.
pub(super) fn choir(freq: f32) -> Box<dyn AudioUnit> {
    let saws = (saw_hz(freq)
        + saw_hz(freq * 1.004)
        + saw_hz(freq * 0.996)
        + saw_hz(freq * 1.009)
        + saw_hz(freq * 0.991)
        + sine_hz(freq * 0.5) * 0.6)
        * 0.15;
    Box::new((saws >> lowpass_hz(2600.0, 0.7)) * envelope(|t: f32| (t * 1.0).min(1.0)) * 0.3)
}

/// Donk: a bright, plucky detuned-saw chord stab — the euphoric off-beat "donk" of happy-hardcore /
/// house / party music. Snappy filter pluck + an octave partial + a touch of drive so it cuts and
/// bounces on the up-beats.
pub(super) fn donk(freq: f32) -> Box<dyn AudioUnit> {
    let mk = move || {
        let saws =
            (saw_hz(freq) + saw_hz(freq * 1.01) + saw_hz(freq * 0.99) + saw_hz(freq * 2.0) * 0.4)
                * 0.2;
        let cut = envelope(|t: f32| 900.0 + 3600.0 * (-t * 16.0).exp());
        ((saws | cut) >> lowpass_q(1.0) >> shape(Tanh(1.4)))
            * envelope(|t: f32| {
                let a = 0.003;
                if t < a {
                    t / a
                } else {
                    (-(t - a) * 12.0).exp()
                }
            })
            * 0.4
    };
    if oversampling() {
        Box::new(oversample(mk()))
    } else {
        Box::new(mk())
    }
}

/// House organ stab: the classic early-90s "M1 organ" rave/house chord stab — Haddaway "What Is Love",
/// Snap!, Cappella. A drawbar-organ tone (fundamental + octave + the nasal fifth + 2-octave partial,
/// like organ drawbars) with a hair of detuned saw for bite, a percussive pluck attack and a short
/// sustain through a bright resonant filter. Hollow + euphoric, but the drive + minor chords keep the
/// dark edge. Rendered per triad note so the chord can be panned wide.
pub(super) fn houseorg(freq: f32) -> Box<dyn AudioUnit> {
    let mk = move || {
        let organ = (sine_hz(freq)              // 16' fundamental
            + sine_hz(freq * 2.0) * 0.7         // 8'  octave
            + sine_hz(freq * 3.0) * 0.5         // 5⅓' fifth — the nasal organ honk
            + sine_hz(freq * 4.0) * 0.32        // 4'  two octaves up
            + saw_hz(freq * 1.005) * 0.28       // detuned saw pair = the "zaag" bite + width
            + saw_hz(freq * 0.995) * 0.28)
            * 0.17;
        let cut = envelope(|t: f32| 1200.0 + 3600.0 * (-t * 8.5).exp()); // bright pluck, settles fast
        ((organ | cut) >> lowpass_q(1.1) >> shape(Tanh(1.3)))
            * envelope(|t: f32| {
                let a = 0.004;
                if t < a {
                    t / a
                } else {
                    0.22 + 0.78 * (-(t - a) * 7.0).exp() // percussive attack → a short organ sustain
                }
            })
            * 0.44
    };
    if oversampling() {
        Box::new(oversample(mk()))
    } else {
        Box::new(mk())
    }
}

/// CASIO / electric-piano: a tine-ish voice (sine carrier + a bell "ting" harmonic + a hair of saw
/// cheese) with a pluck-to-light-sustain envelope — the kitschy Ome-Henk keyboard comping.
pub(super) fn casio(freq: f32) -> Box<dyn AudioUnit> {
    let body = (sine_hz(freq)
        + sine_hz(freq * 2.01) * 0.45
        + sine_hz(freq * 4.02) * 0.18 // a slightly inharmonic bell "ting" (not a pure organ partial)
        + saw_hz(freq) * 0.07) // a hair of plastic cheese
        * 0.3;
    let cut = envelope(|t: f32| 800.0 + 3000.0 * (-t * 11.0).exp());
    Box::new(
        ((body | cut) >> lowpass_q(0.8) >> shape(Atan(0.4)))
            * envelope(|t: f32| {
                let a = 0.004;
                if t < a {
                    t / a
                } else {
                    0.12 + 0.88 * (-(t - a) * 6.5).exp() // a real pluck now, no organ sustain plateau
                }
            })
            * 0.5,
    )
}
