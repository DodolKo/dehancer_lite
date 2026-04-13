use image::RgbaImage;
use serde::{Deserialize, Serialize};

use crate::effects::utils::{
    aces_to_image, alpha_plane, hash_noise, image_to_aces, sane, smoothstep,
};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GrainParams {
    pub enabled: bool,
    pub size_px: f32,
    pub amount: f32,
    pub shadows: f32,
    pub midtones: f32,
    pub highlights: f32,
    pub chroma: f32,
    pub seed: u32,
}

impl Default for GrainParams {
    fn default() -> Self {
        Self {
            enabled: true,
            size_px: 0.65,
            amount: 0.22,
            shadows: 1.15,
            midtones: 1.0,
            highlights: 0.8,
            chroma: 0.18,
            seed: 1,
        }
    }
}

impl GrainParams {
    pub fn validated(self) -> Self {
        let d = Self::default();
        Self {
            enabled: self.enabled,
            size_px: sane(self.size_px, 0.25, 8.0, d.size_px),
            amount: sane(self.amount, 0.0, 1.5, d.amount),
            shadows: sane(self.shadows, 0.0, 3.0, d.shadows),
            midtones: sane(self.midtones, 0.0, 3.0, d.midtones),
            highlights: sane(self.highlights, 0.0, 3.0, d.highlights),
            chroma: sane(self.chroma, 0.0, 1.0, d.chroma),
            seed: self.seed,
        }
    }
}

pub fn apply_grain(input: &RgbaImage, params: &GrainParams) -> RgbaImage {
    let params = params.validated();
    if !params.enabled || params.amount <= 1e-6 {
        return input.clone();
    }

    let width = input.width() as usize;
    let height = input.height() as usize;
    let alpha = alpha_plane(input);
    let mut working = image_to_aces(input);

    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let mut pixel = working[idx];
            let luma = 0.2126 * pixel[0] + 0.7152 * pixel[1] + 0.0722 * pixel[2];

            let shadow_w = 1.0 - smoothstep(0.2, 0.55, luma);
            let highlight_w = smoothstep(0.55, 0.9, luma);
            let mid_w = (1.0 - shadow_w - highlight_w).clamp(0.0, 1.0);

            let tone_gain = shadow_w * params.shadows
                + mid_w * params.midtones
                + highlight_w * params.highlights;
            let amp = params.amount * tone_gain;

            let cell_x = (x as f32 / params.size_px.max(0.25)).floor() as u32;
            let cell_y = (y as f32 / params.size_px.max(0.25)).floor() as u32;

            let mono = hash_noise(cell_x, cell_y, params.seed);
            let chroma = [
                hash_noise(cell_x, cell_y, params.seed.wrapping_add(11)),
                hash_noise(cell_x, cell_y, params.seed.wrapping_add(29)),
                hash_noise(cell_x, cell_y, params.seed.wrapping_add(47)),
            ];

            for channel in 0..3 {
                let mixed = mono * (1.0 - params.chroma) + chroma[channel] * params.chroma;
                pixel[channel] = (pixel[channel] + mixed * amp).max(0.0);
            }

            working[idx] = pixel;
        }
    }

    aces_to_image(&working, input.width(), input.height(), &alpha)
}

pub fn apply_grain_gpu_reference(input: &RgbaImage, params: &GrainParams) -> RgbaImage {
    // MVP reference path: mirrors CPU behavior for backend parity tests.
    apply_grain(input, params)
}

#[cfg(test)]
mod tests {
    use image::{Rgba, RgbaImage};

    use super::{apply_grain, GrainParams};

    #[test]
    fn grain_is_deterministic_for_same_seed() {
        let mut img = RgbaImage::new(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                img.put_pixel(x, y, Rgba([96, 96, 96, 255]));
            }
        }

        let params = GrainParams {
            seed: 42,
            amount: 0.35,
            ..GrainParams::default()
        };

        let a = apply_grain(&img, &params);
        let b = apply_grain(&img, &params);
        assert_eq!(a.as_raw(), b.as_raw());
    }

    #[test]
    fn grain_is_stronger_in_shadows_than_highlights() {
        let mut img = RgbaImage::new(128, 64);
        for y in 0..64 {
            for x in 0..128 {
                let v = if x < 64 { 24 } else { 220 };
                img.put_pixel(x, y, Rgba([v, v, v, 255]));
            }
        }

        let out = apply_grain(
            &img,
            &GrainParams {
                amount: 0.45,
                shadows: 1.8,
                highlights: 0.3,
                ..GrainParams::default()
            },
        );

        let dark_delta = mean_abs_delta(&img, &out, 0, 63);
        let bright_delta = mean_abs_delta(&img, &out, 64, 127);
        assert!(
            dark_delta > bright_delta,
            "expected shadows grain to dominate"
        );
    }

    fn mean_abs_delta(input: &RgbaImage, output: &RgbaImage, x0: u32, x1: u32) -> f32 {
        let mut acc = 0.0_f32;
        let mut count = 0_u32;
        for y in 0..input.height() {
            for x in x0..=x1 {
                let i = input[(x, y)];
                let o = output[(x, y)];
                for c in 0..3 {
                    acc += ((o[c] as i32 - i[c] as i32).abs() as f32) / 255.0;
                    count += 1;
                }
            }
        }

        acc / count.max(1) as f32
    }
}
