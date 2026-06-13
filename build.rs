//! Build-time asset prep:
//!  1. ALWAYS: synthesize the procedural demo splats the DEFAULT show references, if missing — so a
//!     fresh `git clone && cargo run` plays the intro with no python/numpy step (the .ply are
//!     gitignored, 38 MB of regenerable blobs). This is the Rust port of `pipeline/gen-demo-splats.py`.
//!  2. `--features bundle`: read `bundle.toml`, auto-collect the `.ply`/PNG assets the baked-in show
//!     references, lz4-compress them into one archive embedded in the binary, emit the show config as
//!     Rust consts. At runtime `src/bundle.rs` self-extracts + plays the show.

use std::io::Write;
use std::path::{Path, PathBuf};

/// The show whose `.ply` get auto-generated for a bare `cargo run` (the default + CI binary).
const DEFAULT_SHOW: &str = "productions/intro/intro.show";

fn main() {
    println!("cargo:rerun-if-changed=bundle.toml");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={DEFAULT_SHOW}");

    // (1) Ensure the default show's procedural splats exist (idempotent — skips any already present).
    if let Ok(src) = std::fs::read_to_string(DEFAULT_SHOW) {
        ensure_splats(Path::new("assets"), &referenced_assets(&src));
    }

    // Only do the bundle work for a bundled build (cargo sets CARGO_FEATURE_<NAME> per enabled feature).
    if std::env::var("CARGO_FEATURE_BUNDLE").is_err() {
        return;
    }

    let manifest = std::env::var("MARTIN_BUNDLE").unwrap_or_else(|_| "bundle.toml".to_string());
    println!("cargo:rerun-if-changed={manifest}");
    let toml = std::fs::read_to_string(&manifest)
        .unwrap_or_else(|e| panic!("bundle: cannot read {manifest}: {e}"));
    let cfg = parse_kv(&toml);

    let get = |k: &str| {
        cfg.iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.as_str())
    };
    let asset_dir = PathBuf::from(get("asset_dir").unwrap_or("assets"));
    let (kind, show_spec) = match (get("show"), get("seq"), get("compose")) {
        (Some(s), _, _) => ("show", s),
        (_, Some(s), _) => ("seq", s),
        (_, _, Some(c)) => ("compose", c),
        _ => panic!("bundle: bundle.toml needs a `show = …`, `seq = …` or `compose = …`"),
    };
    // The show spec is a file path (read its content) or an inline string — same rule martin uses.
    // Re-run if the show FILE changes (else an incremental bundle build bakes in a STALE show), and
    // if MARTIN_BUNDLE selects a different manifest.
    println!("cargo:rerun-if-env-changed=MARTIN_BUNDLE");
    if Path::new(show_spec).is_file() {
        println!("cargo:rerun-if-changed={show_spec}");
    }
    let show_src = read_or_inline(show_spec);

    // Auto-collect: every `splat:`/`image:`/`mesh:` filename the show references, + the logo.
    let mut names = referenced_assets(&show_src);
    let logo = get("logo").unwrap_or("").to_string();
    if !logo.is_empty() && !names.contains(&logo) {
        names.push(logo.clone());
    }

    // Synthesize any referenced procedural splats that aren't present (same as the default-build step,
    // for the bundle's own show + asset_dir) — so a bundle build needs no python/numpy pre-step either.
    ensure_splats(&asset_dir, &names);

    // The archive: per entry [u32 name_len][name][u32 data_len][lz4 data]; prefixed by [u32 count].
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for name in &names {
        let path = asset_dir.join(name);
        println!("cargo:rerun-if-changed={}", path.display());
        // a `.mtl` sibling is optional — an .obj without one just renders with the flat fallback.
        if name.ends_with(".mtl") && !path.exists() {
            continue;
        }
        let bytes = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("bundle: missing asset {}: {e}", path.display()));
        files.push((name.clone(), bytes));
    }
    // The score: bake the named file (else the default assets/score.txt) so the bundle is self-contained.
    let score_path = get("score")
        .map(PathBuf::from)
        .unwrap_or_else(|| asset_dir.join("score.txt"));
    let score_name = "score.txt".to_string();
    if let Ok(bytes) = std::fs::read(&score_path) {
        println!("cargo:rerun-if-changed={}", score_path.display());
        files.push((score_name.clone(), bytes));
    }

    // Pre-rendered music: bake it in so the bundle plays the track INSTANTLY + in sync. (The live
    // synth render takes ~30s — far too slow for a bundled demo, which is why it played silent.)
    let music_name = match get("music") {
        Some(m) => {
            let mp = PathBuf::from(m);
            let ext = mp.extension().and_then(|e| e.to_str()).unwrap_or("wav");
            let name = format!("music.{ext}");
            let bytes = std::fs::read(&mp)
                .unwrap_or_else(|e| panic!("bundle: missing music {}: {e}", mp.display()));
            println!("cargo:rerun-if-changed={}", mp.display());
            files.push((name.clone(), bytes));
            name
        }
        None => String::new(),
    };

    let root_ply = names
        .iter()
        .find(|n| n.ends_with(".ply"))
        .cloned()
        .unwrap_or_default();

    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    write_archive(&out.join("bundle.bin"), &files);

    let mut raw_total = 0usize;
    let mut comp_total = 0usize;
    let mut code = String::new();
    code.push_str(&format!("pub const SHOW_KIND: &str = {kind:?};\n"));
    code.push_str(&format!("pub const SHOW_SRC: &str = {show_src:?};\n"));
    code.push_str(&format!("pub const SCORE_NAME: &str = {score_name:?};\n"));
    code.push_str(&format!("pub const ROOT_PLY: &str = {root_ply:?};\n"));
    code.push_str(&format!("pub const LOGO: &str = {logo:?};\n"));
    code.push_str(&format!("pub const MUSIC_NAME: &str = {music_name:?};\n"));
    code.push_str(&format!(
        "pub const MORPH_COUNT: &str = {:?};\n",
        get("morph_count").unwrap_or("")
    ));
    std::fs::write(out.join("bundle_config.rs"), code).expect("write bundle_config.rs");

    for (_, b) in &files {
        raw_total += b.len();
        comp_total += lz4_flex::compress_prepend_size(b).len();
    }
    println!(
        "cargo:warning=bundle: {} assets, {} KiB raw -> {} KiB compressed (show: {kind})",
        files.len(),
        raw_total / 1024,
        comp_total / 1024
    );
}

/// Minimal `key = value` parser (`#` comments, optional quotes) — no toml dependency needed.
fn parse_kv(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in s.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = v.trim().trim_matches('"').trim_matches('\'');
        out.push((k.trim().to_string(), v.to_string()));
    }
    out
}

/// A file path → its contents, else the spec used verbatim as an inline string.
fn read_or_inline(spec: &str) -> String {
    std::fs::read_to_string(spec).unwrap_or_else(|_| spec.to_string())
}

/// Every asset filename a show spec references — `splat:`/`image:`/`mesh:`/`glb:`/`gltf:`/`model:`
/// (the same token grammar martin parses), plus the sibling `.mtl` of any `.obj` (Wavefront
/// references its material by name from inside the file, so it must ship alongside).
fn referenced_assets(spec: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut push = |n: &str| {
        let n = n.trim();
        if n.is_empty() || names.contains(&n.to_string()) {
            return;
        }
        names.push(n.to_string());
        if let Some(stem) = n.strip_suffix(".obj") {
            names.push(format!("{stem}.mtl")); // ship the material beside the .obj
        }
    };
    for line in spec.split([';', '\n']) {
        let line = line.split('#').next().unwrap_or("");
        for tok in line.split_whitespace() {
            if let Some(p) = tok.strip_prefix("splat:") {
                p.split('+').for_each(&mut push);
            } else if let Some(p) = tok.strip_prefix("image:") {
                push(p);
            } else if let Some(p) = tok.strip_prefix("svg:") {
                push(p);
            } else if let Some(p) = tok.strip_prefix("mesh:") {
                push(p);
            } else if let Some(p) = tok.strip_prefix("glb:") {
                push(p);
            } else if let Some(p) = tok.strip_prefix("gltf:") {
                push(p);
            } else if let Some(p) = tok.strip_prefix("model:") {
                push(p);
            }
        }
    }
    names
}

// ───────────────────────── procedural demo splats (port of pipeline/gen-demo-splats.py) ─────────────
// Not bit-exact with the python (different RNG) — it doesn't need to be: martin morton-resamples each
// cloud on load, so only the SHAPE + colour matter, not point order. Deterministic per shape (fixed
// seed) so rebuilds are stable. sh0 .ply layout: x y z | scale_0..2 (log) | opacity (logit) |
// rot wxyz (identity) | f_dc_0..2 (SH0). ~140k splats/shape.

const GEN_N: usize = 140_000;
const GEN_SPLAT: f32 = 0.02; // splat radius
const GEN_ALPHA: f32 = 0.92; // opacity

/// For each referenced `*.ply` that's missing and is a shape we know how to synthesize, generate it.
fn ensure_splats(asset_dir: &Path, names: &[String]) {
    for n in names {
        let Some(stem) = n.strip_suffix(".ply") else {
            continue;
        };
        let path = asset_dir.join(n);
        if path.exists() {
            continue;
        }
        let Some((pos, rgb)) = gen_shape(stem) else {
            continue; // not a procedural shape (a real capture) — leave it to fail loudly downstream
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        write_ply(&path, stem, &pos, &rgb);
        println!(
            "cargo:warning=gen: synthesized {} ({GEN_N} splats)",
            path.display()
        );
    }
}

/// A tiny splitmix64 PRNG — enough for uniform/normal/integer draws, no rand crate in build deps.
struct Rng(u64);
impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn unit(&mut self) -> f32 {
        // [0,1)
        ((self.next_u64() >> 11) as f64 / (1u64 << 53) as f64) as f32
    }
    fn uniform(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.unit()
    }
    fn int(&mut self, k: u64) -> u64 {
        self.next_u64() % k
    }
    fn normal(&mut self, mu: f32, sigma: f32) -> f32 {
        // Box–Muller (one of the pair); cheap + plenty for jitter.
        let u1 = self.unit().max(1e-7);
        let u2 = self.unit();
        let z = (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos();
        mu + sigma * z
    }
}

/// h,s,v in [0,1] → (r,g,b) in [0,1] (matches the python `hsv`).
fn hsv(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h6 = (h.rem_euclid(1.0)) * 6.0;
    let i = h6.floor() as i32;
    let f = h6 - i as f32;
    let (p, q, t) = (v * (1.0 - s), v * (1.0 - s * f), v * (1.0 - s * (1.0 - f)));
    match i.rem_euclid(6) {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    }
}

type Cloud = (Vec<[f32; 3]>, Vec<[f32; 3]>);

/// Synthesize a named shape (the subset the shipped shows use; others → None).
fn gen_shape(stem: &str) -> Option<Cloud> {
    let mut rng = Rng(0xDEFEE5);
    let n = GEN_N;
    let pi = std::f32::consts::PI;
    let tau = std::f32::consts::TAU;
    let (mut pos, mut rgb) = (Vec::with_capacity(n), Vec::with_capacity(n));
    match stem {
        "galaxy" => {
            let arms = 3u64;
            for _ in 0..n {
                let r = rng.unit().powf(0.7);
                let arm = rng.int(arms) as f32;
                let theta = arm * (tau / arms as f32) + r * 5.0 + rng.normal(0.0, 0.25);
                let y = rng.normal(0.0, 0.04) * (1.2 - r);
                pos.push([r * theta.cos(), y, r * theta.sin()]);
                rgb.push(hsv(0.6 + r * 0.35, 0.8, 1.0));
            }
        }
        "torus" => {
            let (rad, tube) = (0.72, 0.3);
            for _ in 0..n {
                let u = rng.uniform(0.0, tau);
                let v = rng.uniform(0.0, tau);
                pos.push([
                    (rad + tube * v.cos()) * u.cos(),
                    tube * v.sin(),
                    (rad + tube * v.cos()) * u.sin(),
                ]);
                rgb.push(hsv(u / tau, 0.9, 1.0));
            }
        }
        "helix" => {
            for _ in 0..n {
                let t = rng.uniform(0.0, 6.0 * pi);
                let strand = rng.int(2);
                let phase = strand as f32 * pi;
                let j = [
                    rng.normal(0.0, 0.03),
                    rng.normal(0.0, 0.03),
                    rng.normal(0.0, 0.03),
                ];
                pos.push([
                    0.45 * (t + phase).cos() + j[0],
                    t / (3.0 * pi) - 1.0 + j[1],
                    0.45 * (t + phase).sin() + j[2],
                ]);
                rgb.push(if strand == 0 {
                    [0.1, 0.9, 1.0]
                } else {
                    [1.0, 0.2, 0.8]
                });
            }
        }
        "knot" => {
            for _ in 0..n {
                let t = rng.uniform(0.0, tau);
                let p = [
                    (t.sin() + 2.0 * (2.0 * t).sin()) / 3.2 + rng.normal(0.0, 0.035),
                    (t.cos() - 2.0 * (2.0 * t).cos()) / 3.2 + rng.normal(0.0, 0.035),
                    (-(3.0 * t).sin()) / 3.2 + rng.normal(0.0, 0.035),
                ];
                pos.push(p);
                rgb.push(hsv(t / tau, 0.9, 1.0));
            }
        }
        "supershape" => {
            let sf = |a: f32| {
                let t = 7.0 * a / 4.0;
                let r = (t.cos().abs().powf(1.7) + t.sin().abs().powf(1.7) + 1e-9).powf(-1.0 / 0.3);
                r.clamp(0.0, 3.0)
            };
            let mut raw = Vec::with_capacity(n);
            let mut maxv = 1e-6f32;
            for _ in 0..n {
                let th = rng.uniform(-pi / 2.0, pi / 2.0);
                let ph = rng.uniform(-pi, pi);
                let (r1, r2) = (sf(th), sf(ph));
                let p = [
                    r1 * th.cos() * r2 * ph.cos(),
                    r2 * th.sin(),
                    r1 * th.cos() * r2 * ph.sin(),
                ];
                let p = p.map(|c| if c.is_finite() { c } else { 0.0 });
                maxv = maxv.max(p[0].abs()).max(p[1].abs()).max(p[2].abs());
                raw.push((p, ph));
            }
            for (p, ph) in raw {
                pos.push([p[0] / maxv, p[1] / maxv, p[2] / maxv]);
                rgb.push(hsv(0.55 + 0.4 * (2.0 * ph).sin(), 0.9, 1.0));
            }
        }
        _ => return None,
    }
    Some((pos, rgb))
}

/// Write a cloud as martin's sh0 binary .ply.
fn write_ply(path: &Path, name: &str, pos: &[[f32; 3]], rgb: &[[f32; 3]]) {
    let scale = GEN_SPLAT.ln();
    let opacity = (GEN_ALPHA / (1.0 - GEN_ALPHA)).ln();
    let header = format!(
        "ply\nformat binary_little_endian 1.0\ncomment martin demo splat: {name}\n\
         element vertex {}\n\
         property float x\nproperty float y\nproperty float z\n\
         property float scale_0\nproperty float scale_1\nproperty float scale_2\n\
         property float opacity\n\
         property float rot_0\nproperty float rot_1\nproperty float rot_2\nproperty float rot_3\n\
         property float f_dc_0\nproperty float f_dc_1\nproperty float f_dc_2\nend_header\n",
        pos.len()
    );
    let mut buf = Vec::with_capacity(header.len() + pos.len() * 56);
    buf.extend_from_slice(header.as_bytes());
    let mut put = |v: f32| buf.extend_from_slice(&v.to_le_bytes());
    for (p, c) in pos.iter().zip(rgb) {
        put(p[0]);
        put(p[1]);
        put(p[2]);
        put(scale);
        put(scale);
        put(scale);
        put(opacity);
        put(1.0);
        put(0.0);
        put(0.0);
        put(0.0); // identity rot wxyz
        put((c[0] - 0.5) / 0.282_094_8);
        put((c[1] - 0.5) / 0.282_094_8);
        put((c[2] - 0.5) / 0.282_094_8);
    }
    std::fs::write(path, &buf).unwrap_or_else(|e| panic!("gen: write {}: {e}", path.display()));
}

fn write_archive(path: &Path, files: &[(String, Vec<u8>)]) {
    let mut buf = Vec::new();
    buf.extend_from_slice(&(files.len() as u32).to_le_bytes());
    for (name, bytes) in files {
        let comp = lz4_flex::compress_prepend_size(bytes);
        buf.extend_from_slice(&(name.len() as u32).to_le_bytes());
        buf.extend_from_slice(name.as_bytes());
        buf.extend_from_slice(&(comp.len() as u32).to_le_bytes());
        buf.extend_from_slice(&comp);
    }
    let mut f = std::fs::File::create(path).expect("write bundle.bin");
    f.write_all(&buf).expect("write bundle.bin");
}
