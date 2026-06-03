//! Splat-image: sample a PNG's opaque pixels into flat (z=0) coloured gaussians, so a logo (or
//! any image) is just another morph source — it can ball-assemble, sparkle in, hold, and morph
//! into the next part exactly like splat-text. Built **Y-DOWN** so the entity's
//! `cloud_base_rotation` flips it upright, matching text and the Y-down `.ply` splats.

use bevy_gaussian_splatting::{Gaussian3d, SphericalHarmonicCoefficients};

/// 3DGS degree-0 encode: rendered colour ≈ 0.5 + 0.2820948·dc, so invert for a target linear.
fn dc(c: f32) -> f32 {
    (c - 0.5) / 0.282_094_79
}

/// Sample the opaque pixels of `png` (every `stride`-th pixel) into colored gaussians spanning
/// `world_width`, centred at the origin, flat on z=0. `alpha_thresh` drops near-transparent
/// pixels (clean edges); `gain` (<1) keeps bright logos from blooming into a blob. Deterministic
/// jitter (no rng) keeps record mode reproducible.
pub fn build_image_gaussians(
    png: &[u8],
    world_width: f32,
    stride: usize,
    splat: f32,
    alpha_thresh: f32,
    gain: f32,
) -> Vec<Gaussian3d> {
    let img = match image::load_from_memory(png) {
        Ok(i) => i.to_rgba8(),
        Err(_) => return Vec::new(),
    };
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 {
        return Vec::new();
    }
    let stride = stride.max(1);
    let scale = world_width / w as f32;
    let (cx, cy) = (w as f32 * 0.5, h as f32 * 0.5);

    let mut out = Vec::new();
    let mut i: u32 = 0;
    for yy in (0..h).step_by(stride) {
        for xx in (0..w).step_by(stride) {
            let px = img.get_pixel(xx, yy).0;
            let a = px[3] as f32 / 255.0;
            if a < alpha_thresh {
                continue; // opaque pixels only → the logo shape, clean edges
            }
            let mut sh = SphericalHarmonicCoefficients::default();
            sh.set(0, dc(px[0] as f32 / 255.0 * gain));
            sh.set(1, dc(px[1] as f32 / 255.0 * gain));
            sh.set(2, dc(px[2] as f32 / 255.0 * gain));
            // cheap deterministic jitter inside the cell (mirrors text.rs; no rng dep)
            let j = |k: u32| ((k.wrapping_mul(2_654_435_761) >> 8) & 0xff) as f32 / 255.0 - 0.5;
            let gx = (xx as f32 + j(i) * stride as f32 - cx) * scale;
            let gy = (yy as f32 + j(i ^ 0x9e37) * stride as f32 - cy) * scale; // Y-DOWN
            i = i.wrapping_add(1);
            out.push(Gaussian3d {
                position_visibility: [gx, gy, 0.0, 1.0].into(),
                spherical_harmonic: sh,
                rotation: [0.0, 0.0, 0.0, 1.0].into(),
                scale_opacity: [splat, splat, splat, a].into(),
            });
        }
    }
    out
}
