//! Sound-design effects + the reverb: the ping-pong delay, the section-transition risers / jet
//! whooshes / impacts, the hardstyle kick, the snare-roll, the atmosphere bed, and the spread reverb
//! send. These are hand-written DSP (sample loops), not FunDSP voices — the voices are in `voices`.

use super::voices::snare;
use super::{SAMPLE_RATE, add_stereo, pseudo_noise, render_into};

/// Ping-pong delay: a stereo delay line where each tap alternates L-R-L-R so the delayed
/// repeats bounce across the stereo field. Used on the arp to give it motion and space.
pub(super) fn render_pingpong(buf: &mut [f32], delay_s: f32, feedback: f32, wet: f32) {
    let sr = SAMPLE_RATE as f32;
    let d = (delay_s * sr) as usize;
    if d < 2 {
        return;
    }
    let frames = buf.len() / 2;
    let mut line = vec![0f32; d * 2];
    let mut w = 0usize;
    let mut alt = 0u32;
    for i in 0..frames {
        let l = buf[2 * i];
        let r = buf[2 * i + 1];
        let mono = l + r;
        let dl = line[2 * w];
        let dr = line[2 * w + 1];
        buf[2 * i] += dl * wet;
        buf[2 * i + 1] += dr * wet;
        let fb = mono * feedback * 0.5;
        if alt & 1 == 0 {
            line[2 * w] = fb * 0.45;
            line[2 * w + 1] = fb;
        } else {
            line[2 * w] = fb;
            line[2 * w + 1] = fb * 0.45;
        }
        alt = alt.wrapping_add(1);
        w = (w + 1) % d;
    }
}

/// Noise + tone sweep into a section boundary. This is intentionally simple and deterministic:
/// enough to make the arrangement breathe without turning the score DSL into an effects tracker.
pub(super) fn render_riser(buf: &mut [f32], start_t: f32, dur: f32, amp: f32, pan: f32) {
    use std::f32::consts::TAU;
    let sr = SAMPLE_RATE as f32;
    let start = (start_t.max(0.0) * sr) as usize;
    let n = (dur.max(0.0) * sr) as usize;
    let mut phase = 0.0f32;
    let mut hp = 0.0f32;
    let denom = std::cmp::max(n, 1) as f32;
    for i in 0..n {
        let p = i as f32 / denom;
        let frame = start + i;
        let hz = 180.0 + 2400.0 * p * p;
        phase = (phase + TAU * hz / sr) % TAU;
        let noise = pseudo_noise(i + start);
        hp += 0.08 * (noise - hp);
        let bright = noise - hp;
        let gate = (p * 16.0).sin().abs() * 0.35 + 0.65;
        let env = p * p * (1.0 - (p - 0.98).max(0.0) * 50.0).clamp(0.0, 1.0);
        add_stereo(
            buf,
            frame,
            (phase.sin() * 0.35 + bright * 0.65) * env * gate * amp,
            pan,
        );
    }
}

/// Atmospheric texture bed under the WHOLE track: a soft band-limited noise floor + sparse vinyl
/// crackle. Game/chiptune music is dead-silent between notes; produced trip-hop/downtempo records
/// (Massive Attack / Portishead) always sit on a dusty textured floor — that bed is a big part of
/// what reads as "a record" instead of "a bright synth preset". Kept low + slightly decorrelated L/R.
pub(super) fn render_atmosphere(bed: &mut [f32], sr: f32, start_t: f32, amt: f32) {
    use std::f32::consts::TAU;
    if amt <= 0.0 {
        return; // `set atmosphere=0` → no floor (e.g. a clean chiptune or a different genre)
    }
    let total = bed.len() / 2;
    let start = (start_t.max(0.0) * sr) as usize;
    let fade = (1.5 * sr) as usize; // ease the floor in over ~1.5 s so it doesn't just switch on
    let (mut lp, mut hp) = (0.0f32, 0.0f32);
    let a = 1.0 - (-TAU * 2000.0 / sr).exp();
    let ah = 1.0 - (-TAU * 350.0 / sr).exp();
    for i in start..total {
        let g = ((i - start) as f32 / fade as f32).min(1.0);
        let n = pseudo_noise(i * 2 + 7);
        lp += a * (n - lp); // low-pass...
        hp += ah * (lp - hp); // ...minus a high-pass = a soft ~350-2000 Hz band (warm hiss, no fizz)
        let floor = (lp - hp) * 0.008;
        let crackle = if pseudo_noise(i * 3 + 1) > 0.9996 {
            pseudo_noise(i * 7) * 0.03 // sparser, quieter dust clicks
        } else {
            0.0
        };
        let v = (floor + crackle) * g * amt;
        bed[2 * i] += v;
        bed[2 * i + 1] += v * 0.92;
    }
}

/// Modern hardstyle / rawstyle KICK, tuned per hit to the chord root: a tight click transient → a
/// heavily DISTORTED pitch-swept body (sine + a saw partial driven through tanh then hard-clipped =
/// the "zaag"/gabber grit) → a pitched tonal TAIL on the root pitch-class (the "piep" — the kick is
/// melodic and sings the progression). This is the centre of a modern hard production, not a soft
/// 90s drum-machine thud.
pub(super) fn render_hardkick(buf: &mut [f32], t: f32, root: f32, amp: f32) {
    use std::f32::consts::TAU;
    let sr = SAMPLE_RATE as f32;
    let start = (t.max(0.0) * sr) as usize;
    let n = (0.5 * sr) as usize;
    // pitch the tonal tail to the root pitch-class in a punchy 55-90 Hz window
    let mut tail_hz = root;
    while tail_hz > 90.0 {
        tail_hz *= 0.5;
    }
    while tail_hz < 55.0 {
        tail_hz *= 2.0;
    }
    let (mut ph_b, mut ph_t) = (0.0f32, 0.0f32);
    for i in 0..n {
        let tt = i as f32 / sr;
        let frame = start + i;
        // body: a fast pitch sweep from ~300 Hz down to the tail pitch over ~13 ms
        let body_hz = tail_hz + (300.0 - tail_hz) * (-tt * 75.0).exp();
        ph_b = (ph_b + TAU * body_hz / sr) % TAU;
        let raw = ph_b.sin() + ((ph_b / TAU) * 2.0 - 1.0) * 0.5; // sine + saw partial (the "zaag")
        let driven = (raw * 5.0).tanh(); // overdrive
        let body = (driven * 1.6).clamp(-1.0, 1.0) * (-tt * 9.0).exp(); // + hard-clip edge, fast decay
        // tonal tail: the pitched "piep", distorted, slower decay
        ph_t = (ph_t + TAU * tail_hz / sr) % TAU;
        let tail = (ph_t.sin() * 3.0).tanh() * (-tt * 5.0).exp();
        // click transient: bright noise blip for the attack snap
        let click = pseudo_noise(i + start * 11) * (-tt * 300.0).exp() * 0.6;
        add_stereo(buf, frame, (body * 0.95 + tail * 0.45 + click) * amp, 0.0);
    }
}

/// Jet-engine flyby: band-limited noise (a sweeping band-pass built from two one-pole low-passes, so
/// it can't self-oscillate) + a sweeping turbine whine, with a swell-to-flyby-then-away amplitude
/// envelope and a left→right doppler pan. Rips into a section like an afterburner pass.
pub(super) fn render_jet(buf: &mut [f32], start_t: f32, dur: f32, amp: f32) {
    use std::f32::consts::TAU;
    let sr = SAMPLE_RATE as f32;
    let start = (start_t.max(0.0) * sr) as usize;
    let n = (dur.max(0.0) * sr) as usize;
    let denom = std::cmp::max(n, 1) as f32;
    let (mut lp1, mut lp2, mut lp3) = (0.0f32, 0.0f32, 0.0f32);
    let (mut ph1, mut ph2) = (0.0f32, 0.0f32);
    for i in 0..n {
        let p = i as f32 / denom;
        let frame = start + i;
        let nz = pseudo_noise(i + start * 7);
        // a RESONANT noise band whose centre rises across the pass (faster near the end) — an uplifter
        // "whoosh", not a polite sweep. Two overlapping band-passes stack into a richer scream than the
        // old single 1-pole band did.
        let cut = 350.0 + 3200.0 * p * p;
        let a_lo = 1.0 - (-TAU * cut / sr).exp();
        let a_hi = 1.0 - (-TAU * (cut * 2.2) / sr).exp();
        let a_n = 1.0 - (-TAU * (cut * 1.4) / sr).exp();
        lp1 += a_lo * (nz - lp1);
        lp2 += a_hi * (nz - lp2);
        lp3 += a_n * (nz - lp3);
        let band = (lp2 - lp1) * 2.5 + (lp2 - lp3) * 2.0;
        // a DETUNED-saw turbine pair (not a clean sine — that was the synthetic tell) rising into the
        // hit, low under the noise: pitch motion without the cheesy pure-tone whine.
        let whz = 500.0 + 2200.0 * p;
        ph1 = (ph1 + TAU * whz / sr) % TAU;
        ph2 = (ph2 + TAU * whz * 1.011 / sr) % TAU;
        let saw = |ph: f32| (ph / TAU) * 2.0 - 1.0;
        let turbine = (saw(ph1) + saw(ph2)) * 0.06;
        let env = (1.0 - (2.0 * p - 1.0).abs()).powf(1.3); // swell → flyby → away
        let v = ((band + turbine) * env).tanh() * amp; // soft drive → grit, not a clean sweep
        add_stereo(buf, frame, v, (2.0 * p - 1.0) * 0.8);
    }
}

/// Low boom + short noisy crack at a downbeat.
pub(super) fn render_impact(buf: &mut [f32], t: f32, dur: f32, amp: f32) {
    use std::f32::consts::TAU;
    let sr = SAMPLE_RATE as f32;
    let start = (t.max(0.0) * sr) as usize;
    let n = (dur.max(0.0) * sr) as usize;
    let mut phase = 0.0f32;
    let denom = std::cmp::max(n, 1) as f32;
    for i in 0..n {
        let p = i as f32 / denom;
        let frame = start + i;
        let hz = 92.0 * (1.0 - p).powf(2.0) + 32.0;
        phase = (phase + TAU * hz / sr) % TAU;
        let boom = phase.sin() * (-p * 4.5).exp();
        let crack = pseudo_noise(i + start * 3) * (-p * 38.0).exp();
        add_stereo(buf, frame, (boom * 0.9 + crack * 0.25) * amp, 0.0);
    }
}

/// Accelerating, rising snare roll over `[start, start+dur]` — the build-up tension into a drop.
pub(super) fn render_snare_roll(buf: &mut [f32], start: f32, dur: f32, beat: f32) {
    let mut t = 0.0;
    let mut step = beat;
    while t < dur {
        let p = (t / dur).clamp(0.0, 1.0);
        render_into(buf, start + t, 0.16, 0.10 + 0.5 * p, 0.0, snare());
        step = (step * 0.86).max(beat * 0.12); // tighten toward the drop
        t += step;
    }
}

/// Spread reverb send: a mono sum of the stereo bed through 3 damped feedback combs per channel,
/// with slightly different delays L vs R → a wide, decorrelated room tail (dry excluded).
pub(super) fn reverb_send(bed: &[f32], sr: f32) -> Vec<f32> {
    let frames = bed.len() / 2;
    let damp = 0.25_f32;
    // mono sum of the bed, HIGH-PASSED at ~300 Hz before the combs so the tail is air/space, not a
    // low-mid wash that welds the voices together (the reverb was a big part of the "organ" blanket).
    let mut mono: Vec<f32> = (0..frames)
        .map(|i| 0.5 * (bed[2 * i] + bed[2 * i + 1]))
        .collect();
    let a = 1.0 - (-std::f32::consts::TAU * 300.0 / sr).exp();
    let mut hp = 0.0f32;
    for s in mono.iter_mut() {
        hp += a * (*s - hp);
        *s -= hp;
    }
    // ~22 ms pre-delay: the gap before the tail that makes the space read as a big, real hall.
    let pre = (0.022 * sr) as usize;
    // 6 prime-length feedback combs per tank — the modes interleave into a smooth dense tail instead
    // of a few resonant metallic rings. Two decorrelated delay sets feed the L and R tanks (width).
    let comb = |delays: &[usize]| -> Vec<f32> {
        let mut wet = vec![0f32; frames];
        for &d in delays {
            let mut line = vec![0f32; frames];
            let mut lp = 0f32;
            for i in 0..frames {
                let src = if i >= pre { mono[i - pre] } else { 0.0 };
                let fb_in = if i >= d { line[i - d] } else { 0.0 };
                lp += damp * (fb_in - lp);
                line[i] = src + 0.88 * lp;
                wet[i] += 0.88 * lp;
            }
        }
        for w in wet.iter_mut() {
            *w *= 0.5; // 6 combs sum hot — tame before diffusion so the wet doesn't pump the limiter
        }
        wet
    };
    // in-place series all-pass diffuser: smears the comb echoes into a smooth, diffuse tail.
    let allpass = |x: &mut [f32], d: usize, g: f32| {
        let mut buf = vec![0f32; x.len()];
        for i in 0..x.len() {
            let dl = if i >= d { buf[i - d] } else { 0.0 };
            let y = -g * x[i] + dl;
            buf[i] = x[i] + g * y;
            x[i] = y;
        }
    };
    let mut wl = comb(&[1117, 1188, 1277, 1356, 1422, 1491]);
    let mut wr = comb(&[1129, 1213, 1291, 1373, 1447, 1499]);
    for &d in &[0.0051f32, 0.0167, 0.0097] {
        allpass(&mut wl, (d * sr) as usize, 0.7);
    }
    for &d in &[0.0047f32, 0.0153, 0.0089] {
        allpass(&mut wr, (d * sr) as usize, 0.7);
    }
    // darken the wet return (~6.5 kHz one-pole LP) so the tail sits behind the mix like a real room.
    let ad = 1.0 - (-std::f32::consts::TAU * 6500.0 / sr).exp();
    let (mut dl, mut dr) = (0.0f32, 0.0f32);
    let mut out = vec![0f32; bed.len()];
    for i in 0..frames {
        dl += ad * (wl[i] - dl);
        dr += ad * (wr[i] - dr);
        out[2 * i] = dl;
        out[2 * i + 1] = dr;
    }
    out
}
