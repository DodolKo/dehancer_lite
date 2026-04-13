//! Film-halation model with physically-motivated controls.

use image::{Rgba, RgbaImage};

use crate::color::{acescg_to_linear, linear_to_acescg, linear_to_srgb, srgb_to_linear};

/// Reference calibration presets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(non_camel_case_types)]
pub enum FilmPreset {
    /// Public, reproducible baseline for Kodak Vision3 500T style halation behavior.
    KodakVision3_500T_PublicCalibV1,
    /// Lower-intensity variant for subtle halation tests.
    KodakVision3_500T_Subtle,
    /// Higher-intensity variant for stronger halation tests.
    KodakVision3_500T_Strong,
}

/// Tunable parameters for the halation simulation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HalationParams {
    pub threshold: f32,
    pub knee: f32,
    pub core_radius: f32,
    pub tail_radius: f32,
    pub core_weight: f32,
    pub tail_weight: f32,
    pub absorption_rgb: [f32; 3],
    pub optical_depth: f32,
    pub chroma_bias: [f32; 3],
    pub intensity: f32,
    pub soft_clip: f32,
    pub grain_amount: f32,
    pub grain_size: f32,
    pub haze_strength: f32,
    pub lens_distortion: f32,
}

impl FilmPreset {
    /// Baseline values derived from public documentation and visual calibration.
    pub fn defaults(self) -> HalationParams {
        match self {
            Self::KodakVision3_500T_PublicCalibV1 => HalationParams {
                threshold: 0.72,
                knee: 0.18,
                core_radius: 7.0,
                tail_radius: 26.0,
                core_weight: 0.78,
                tail_weight: 0.42,
                absorption_rgb: [0.95, 1.15, 1.45],
                optical_depth: 0.9,
                chroma_bias: [0.30, 0.02, -0.06],
                intensity: 1.0,
                soft_clip: 0.55,
                grain_amount: 0.0,
                grain_size: 1.0,
                haze_strength: 0.0,
                lens_distortion: 0.0,
            },
            Self::KodakVision3_500T_Subtle => HalationParams {
                threshold: 0.76,
                knee: 0.16,
                core_radius: 5.0,
                tail_radius: 18.0,
                core_weight: 0.58,
                tail_weight: 0.28,
                absorption_rgb: [1.05, 1.24, 1.55],
                optical_depth: 0.92,
                chroma_bias: [0.22, 0.01, -0.06],
                intensity: 0.68,
                soft_clip: 0.72,
                grain_amount: 0.0,
                grain_size: 1.0,
                haze_strength: 0.0,
                lens_distortion: 0.0,
            },
            Self::KodakVision3_500T_Strong => HalationParams {
                threshold: 0.66,
                knee: 0.24,
                core_radius: 9.0,
                tail_radius: 34.0,
                core_weight: 0.95,
                tail_weight: 0.62,
                absorption_rgb: [0.88, 1.06, 1.30],
                optical_depth: 0.84,
                chroma_bias: [0.42, 0.04, -0.04],
                intensity: 1.28,
                soft_clip: 0.46,
                grain_amount: 0.0,
                grain_size: 1.0,
                haze_strength: 0.0,
                lens_distortion: 0.0,
            },
        }
    }
}

impl Default for HalationParams {
    fn default() -> Self {
        FilmPreset::KodakVision3_500T_PublicCalibV1.defaults()
    }
}

impl HalationParams {
    /// Clamp inputs to safe, deterministic ranges.
    pub fn validated(self) -> Self {
        fn sane(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
            if value.is_finite() {
                value.clamp(min, max)
            } else {
                fallback
            }
        }

        let d = HalationParams::default();
        Self {
            threshold: sane(self.threshold, 0.0, 8.0, d.threshold),
            knee: sane(self.knee, 0.0, 4.0, d.knee),
            core_radius: sane(self.core_radius, 0.0, 128.0, d.core_radius),
            tail_radius: sane(self.tail_radius, 0.0, 256.0, d.tail_radius),
            core_weight: sane(self.core_weight, 0.0, 4.0, d.core_weight),
            tail_weight: sane(self.tail_weight, 0.0, 4.0, d.tail_weight),
            absorption_rgb: [
                sane(self.absorption_rgb[0], 0.0, 8.0, d.absorption_rgb[0]),
                sane(self.absorption_rgb[1], 0.0, 8.0, d.absorption_rgb[1]),
                sane(self.absorption_rgb[2], 0.0, 8.0, d.absorption_rgb[2]),
            ],
            optical_depth: sane(self.optical_depth, 0.0, 8.0, d.optical_depth),
            chroma_bias: [
                sane(self.chroma_bias[0], -0.95, 3.0, d.chroma_bias[0]),
                sane(self.chroma_bias[1], -0.95, 3.0, d.chroma_bias[1]),
                sane(self.chroma_bias[2], -0.95, 3.0, d.chroma_bias[2]),
            ],
            intensity: sane(self.intensity, 0.0, 8.0, d.intensity),
            soft_clip: sane(self.soft_clip, 0.0, 4.0, d.soft_clip),
            grain_amount: sane(self.grain_amount, 0.0, 0.5, d.grain_amount),
            grain_size: sane(self.grain_size, 1.0, 16.0, d.grain_size),
            haze_strength: sane(self.haze_strength, 0.0, 4.0, d.haze_strength),
            lens_distortion: sane(self.lens_distortion, -1.0, 1.0, d.lens_distortion),
        }
    }

    pub fn core_radius_px(self) -> usize {
        self.core_radius.round().clamp(0.0, 128.0) as usize
    }

    pub fn tail_radius_px(self) -> usize {
        self.tail_radius.round().clamp(0.0, 256.0) as usize
    }
}

/// Public API: apply halation to an image.
pub fn apply_halation(input: &RgbaImage, params: &HalationParams, preset: FilmPreset) -> RgbaImage {
    let mut resolved = params.validated();
    let preset_defaults = preset.defaults();

    // Keep physically meaningful attenuation even under aggressive user tuning.
    for i in 0..3 {
        resolved.absorption_rgb[i] =
            resolved.absorption_rgb[i].max(preset_defaults.absorption_rgb[i] * 0.25);
    }

    let width = input.width() as usize;
    let height = input.height() as usize;
    let len = width * height;

    let mut original_aces = vec![[0.0_f32; 3]; len];
    let mut extracted = vec![[0.0_f32; 3]; len];

    let attenuation = [
        (-resolved.absorption_rgb[0] * resolved.optical_depth).exp(),
        (-resolved.absorption_rgb[1] * resolved.optical_depth).exp(),
        (-resolved.absorption_rgb[2] * resolved.optical_depth).exp(),
    ];

    let chroma_mul = [
        (1.0 + resolved.chroma_bias[0]).max(0.0),
        (1.0 + resolved.chroma_bias[1]).max(0.0),
        (1.0 + resolved.chroma_bias[2]).max(0.0),
    ];

    for (idx, pixel) in input.pixels().enumerate() {
        let srgb = [
            pixel[0] as f32 / 255.0,
            pixel[1] as f32 / 255.0,
            pixel[2] as f32 / 255.0,
        ];

        let linear = [
            srgb_to_linear(srgb[0]),
            srgb_to_linear(srgb[1]),
            srgb_to_linear(srgb[2]),
        ];
        let aces = linear_to_acescg(linear);
        original_aces[idx] = aces;

        let luma = 0.272_228_72 * aces[0] + 0.674_081_74 * aces[1] + 0.053_689_517 * aces[2];
        let mask = highlight_mask(luma, resolved.threshold, resolved.knee);

        extracted[idx] = [
            aces[0] * mask * attenuation[0] * chroma_mul[0],
            aces[1] * mask * attenuation[1] * chroma_mul[1],
            aces[2] * mask * attenuation[2] * chroma_mul[2],
        ];
    }

    let core = blur_rgb(
        &extracted,
        width,
        height,
        resolved.core_radius_px(),
        radius_to_sigma(resolved.core_radius_px()),
    );
    let tail = blur_rgb(
        &extracted,
        width,
        height,
        resolved.tail_radius_px(),
        radius_to_sigma(resolved.tail_radius_px()),
    );

    let mut out = RgbaImage::new(input.width(), input.height());
    for y in 0..height {
        for x in 0..width {
            let idx = y * width + x;
            let mut composed = [
                original_aces[idx][0]
                    + resolved.intensity
                        * (resolved.core_weight * core[idx][0]
                            + resolved.tail_weight * tail[idx][0]),
                original_aces[idx][1]
                    + resolved.intensity
                        * (resolved.core_weight * core[idx][1]
                            + resolved.tail_weight * tail[idx][1]),
                original_aces[idx][2]
                    + resolved.intensity
                        * (resolved.core_weight * core[idx][2]
                            + resolved.tail_weight * tail[idx][2]),
            ];

            let haze = resolved.haze_strength
                * (0.272_228_72 * tail[idx][0]
                    + 0.674_081_74 * tail[idx][1]
                    + 0.053_689_517 * tail[idx][2]);
            composed[0] += haze * 1.05;
            composed[1] += haze;
            composed[2] += haze * 0.92;

            composed[0] = composed[0] / (1.0 + resolved.soft_clip * composed[0].max(0.0));
            composed[1] = composed[1] / (1.0 + resolved.soft_clip * composed[1].max(0.0));
            composed[2] = composed[2] / (1.0 + resolved.soft_clip * composed[2].max(0.0));

            let linear = acescg_to_linear(composed);
            let srgb = [
                (linear_to_srgb(linear[0]) * 255.0)
                    .round()
                    .clamp(0.0, 255.0) as u8,
                (linear_to_srgb(linear[1]) * 255.0)
                    .round()
                    .clamp(0.0, 255.0) as u8,
                (linear_to_srgb(linear[2]) * 255.0)
                    .round()
                    .clamp(0.0, 255.0) as u8,
            ];

            out.put_pixel(
                x as u32,
                y as u32,
                Rgba([srgb[0], srgb[1], srgb[2], input[(x as u32, y as u32)][3]]),
            );
        }
    }

    let out = apply_lens_distortion(&out, resolved.lens_distortion);
    apply_grain(&out, resolved.grain_amount, resolved.grain_size)
}

fn highlight_mask(luma: f32, threshold: f32, knee: f32) -> f32 {
    if knee <= 1e-6 {
        return (luma > threshold) as u8 as f32;
    }

    let edge0 = threshold - knee;
    let edge1 = threshold + knee;
    let t = ((luma - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn radius_to_sigma(radius: usize) -> f32 {
    if radius == 0 {
        1.0
    } else {
        (radius as f32 * 0.5).max(1.0)
    }
}

pub(crate) fn gaussian_kernel(radius: usize, sigma: f32) -> Vec<f32> {
    if radius == 0 {
        return vec![1.0];
    }

    let sigma = sigma.max(1e-4);
    let two_sigma_sq = 2.0 * sigma * sigma;
    let size = radius * 2 + 1;
    let mut kernel = vec![0.0; size];

    for (idx, weight) in kernel.iter_mut().enumerate() {
        let x = idx as isize - radius as isize;
        *weight = (-(x as f32 * x as f32) / two_sigma_sq).exp();
    }

    let sum: f32 = kernel.iter().sum();
    if sum > 0.0 {
        for weight in &mut kernel {
            *weight /= sum;
        }
    }

    kernel
}

fn blur_rgb(
    src: &[[f32; 3]],
    width: usize,
    height: usize,
    radius: usize,
    sigma: f32,
) -> Vec<[f32; 3]> {
    if radius == 0 {
        return src.to_vec();
    }

    let kernel = gaussian_kernel(radius, sigma);
    let mut tmp = vec![[0.0_f32; 3]; src.len()];
    let mut dst = vec![[0.0_f32; 3]; src.len()];

    for y in 0..height {
        for x in 0..width {
            let mut acc = [0.0_f32; 3];
            for k in 0..kernel.len() {
                let offset = k as isize - radius as isize;
                let sx = (x as isize + offset).clamp(0, (width - 1) as isize) as usize;
                let sample = src[y * width + sx];
                let w = kernel[k];
                acc[0] += sample[0] * w;
                acc[1] += sample[1] * w;
                acc[2] += sample[2] * w;
            }
            tmp[y * width + x] = acc;
        }
    }

    for y in 0..height {
        for x in 0..width {
            let mut acc = [0.0_f32; 3];
            for k in 0..kernel.len() {
                let offset = k as isize - radius as isize;
                let sy = (y as isize + offset).clamp(0, (height - 1) as isize) as usize;
                let sample = tmp[sy * width + x];
                let w = kernel[k];
                acc[0] += sample[0] * w;
                acc[1] += sample[1] * w;
                acc[2] += sample[2] * w;
            }
            dst[y * width + x] = acc;
        }
    }

    dst
}

fn apply_lens_distortion(input: &RgbaImage, distortion: f32) -> RgbaImage {
    if distortion.abs() <= 1e-6 || input.width() == 0 || input.height() == 0 {
        return input.clone();
    }

    let width = input.width();
    let height = input.height();
    let wf = width as f32;
    let hf = height as f32;
    let aspect = wf / hf;
    let max_x = wf - 1.0;
    let max_y = hf - 1.0;
    let mut out = RgbaImage::new(width, height);

    for y in 0..height {
        for x in 0..width {
            let centered_x = ((x as f32 + 0.5) / wf) * 2.0 - 1.0;
            let centered_y = ((y as f32 + 0.5) / hf) * 2.0 - 1.0;
            let corrected_x = centered_x * aspect;
            let r2 = corrected_x * corrected_x + centered_y * centered_y;
            let factor = 1.0 + distortion * r2;
            let sample_centered_x = (corrected_x * factor) / aspect;
            let sample_centered_y = centered_y * factor;
            let sx = (((sample_centered_x + 1.0) * 0.5) * wf - 0.5).clamp(0.0, max_x);
            let sy = (((sample_centered_y + 1.0) * 0.5) * hf - 0.5).clamp(0.0, max_y);

            out.put_pixel(x, y, sample_bilinear(input, sx, sy));
        }
    }

    out
}

fn sample_bilinear(input: &RgbaImage, x: f32, y: f32) -> Rgba<u8> {
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = (x0 + 1).min(input.width().saturating_sub(1));
    let y1 = (y0 + 1).min(input.height().saturating_sub(1));
    let tx = x - x0 as f32;
    let ty = y - y0 as f32;

    let p00 = input[(x0, y0)];
    let p10 = input[(x1, y0)];
    let p01 = input[(x0, y1)];
    let p11 = input[(x1, y1)];

    let mut out = [0_u8; 4];
    for channel in 0..4 {
        let top = p00[channel] as f32 * (1.0 - tx) + p10[channel] as f32 * tx;
        let bottom = p01[channel] as f32 * (1.0 - tx) + p11[channel] as f32 * tx;
        out[channel] = (top * (1.0 - ty) + bottom * ty).round().clamp(0.0, 255.0) as u8;
    }

    Rgba(out)
}

fn apply_grain(input: &RgbaImage, amount: f32, size: f32) -> RgbaImage {
    if amount <= 1e-6 {
        return input.clone();
    }

    let mut out = RgbaImage::new(input.width(), input.height());
    let cell_size = size.max(1.0);

    for y in 0..input.height() {
        for x in 0..input.width() {
            let pixel = input[(x, y)];
            let rgb = [
                pixel[0] as f32 / 255.0,
                pixel[1] as f32 / 255.0,
                pixel[2] as f32 / 255.0,
            ];
            let luma = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
            let amp = amount * (0.35 + 0.65 * (1.0 - luma).clamp(0.0, 1.0));
            let cell_x = (x as f32 / cell_size).floor() as u32;
            let cell_y = (y as f32 / cell_size).floor() as u32;

            let grained = [
                rgb[0] + grain_noise(cell_x, cell_y, 0) * amp,
                rgb[1] + grain_noise(cell_x, cell_y, 1) * amp,
                rgb[2] + grain_noise(cell_x, cell_y, 2) * amp,
            ];

            out.put_pixel(
                x,
                y,
                Rgba([
                    (grained[0].clamp(0.0, 1.0) * 255.0).round() as u8,
                    (grained[1].clamp(0.0, 1.0) * 255.0).round() as u8,
                    (grained[2].clamp(0.0, 1.0) * 255.0).round() as u8,
                    pixel[3],
                ]),
            );
        }
    }

    out
}

fn grain_noise(x: u32, y: u32, channel: u32) -> f32 {
    let mut n = x
        .wrapping_mul(1973)
        .wrapping_add(y.wrapping_mul(9277))
        .wrapping_add(channel.wrapping_mul(26699))
        .wrapping_add(0x68bc_21eb);
    n ^= n >> 15;
    n = n.wrapping_mul(2246822519);
    n ^= n >> 13;
    n = n.wrapping_mul(3266489917);
    n ^= n >> 16;
    (n as f32 / u32::MAX as f32) * 2.0 - 1.0
}

#[cfg(test)]
mod tests {
    use image::{Rgba, RgbaImage};

    use super::{apply_halation, gaussian_kernel, FilmPreset, HalationParams};

    #[test]
    fn kernel_is_normalized() {
        let kernel = gaussian_kernel(11, 5.0);
        let sum: f32 = kernel.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5, "kernel sum = {sum}");
    }

    #[test]
    fn validation_clamps_nan_and_ranges() {
        let params = HalationParams {
            threshold: f32::NAN,
            knee: -2.0,
            core_radius: 999.0,
            tail_radius: -5.0,
            core_weight: f32::INFINITY,
            tail_weight: -10.0,
            absorption_rgb: [f32::NAN, -1.0, 99.0],
            optical_depth: f32::NEG_INFINITY,
            chroma_bias: [f32::NAN, -99.0, 99.0],
            intensity: -5.0,
            soft_clip: f32::INFINITY,
            grain_amount: f32::NAN,
            grain_size: 0.0,
            haze_strength: f32::NEG_INFINITY,
            lens_distortion: f32::INFINITY,
        }
        .validated();

        assert!(params.threshold.is_finite());
        assert!(params.knee >= 0.0);
        assert!(params.core_radius <= 128.0);
        assert!(params.tail_radius >= 0.0);
        assert!(params.absorption_rgb.iter().all(|v| *v >= 0.0 && *v <= 8.0));
        assert!(params.chroma_bias.iter().all(|v| *v >= -0.95 && *v <= 3.0));
        assert!(params.grain_amount.is_finite());
        assert!(params.grain_size >= 1.0);
        assert!(params.haze_strength >= 0.0);
        assert!(params.lens_distortion <= 1.0);
    }

    #[test]
    fn scene_headlight_halo_grows() {
        let mut img = RgbaImage::new(128, 128);
        for y in 0..128 {
            for x in 0..128 {
                img.put_pixel(x, y, Rgba([4, 4, 5, 255]));
            }
        }
        img.put_pixel(64, 64, Rgba([255, 252, 240, 255]));

        let out = apply_halation(
            &img,
            &HalationParams::default(),
            FilmPreset::KodakVision3_500T_PublicCalibV1,
        );
        let ring = ring_mean_channel(&out, 64, 64, 8.0, 18.0, 0);
        assert!(ring > 0.015, "expected visible red halo ring, got {ring}");
    }

    #[test]
    fn scene_neon_red_bias_is_present() {
        let mut img = RgbaImage::new(192, 96);
        for y in 0..96 {
            for x in 0..192 {
                img.put_pixel(x, y, Rgba([3, 3, 6, 255]));
            }
        }
        for x in 24..168 {
            img.put_pixel(x, 48, Rgba([255, 255, 255, 255]));
        }

        let out = apply_halation(
            &img,
            &HalationParams::default(),
            FilmPreset::KodakVision3_500T_PublicCalibV1,
        );
        let red = band_mean_channel(&out, 48, 5, 0);
        let blue = band_mean_channel(&out, 48, 5, 2);
        assert!(
            red > blue,
            "expected red-biased halation: red={red}, blue={blue}"
        );
    }

    #[test]
    fn scene_window_energy_increase_is_bounded() {
        let mut img = RgbaImage::new(160, 120);
        for y in 0..120 {
            for x in 0..160 {
                img.put_pixel(x, y, Rgba([10, 10, 12, 255]));
            }
        }
        for y in 32..88 {
            for x in 52..108 {
                img.put_pixel(x, y, Rgba([240, 240, 240, 255]));
            }
        }

        let out = apply_halation(
            &img,
            &HalationParams::default(),
            FilmPreset::KodakVision3_500T_PublicCalibV1,
        );
        let in_energy = mean_luma(&img);
        let out_energy = mean_luma(&out);
        assert!(
            out_energy > in_energy,
            "halation should increase energy in highlights"
        );
        assert!(
            out_energy < in_energy * 2.1,
            "soft clip should keep energy bounded"
        );
    }

    fn ring_mean_channel(
        img: &RgbaImage,
        cx: i32,
        cy: i32,
        r0: f32,
        r1: f32,
        channel: usize,
    ) -> f32 {
        let mut acc = 0.0_f32;
        let mut count = 0_u32;
        let r0_sq = r0 * r0;
        let r1_sq = r1 * r1;

        for y in 0..img.height() as i32 {
            for x in 0..img.width() as i32 {
                let dx = (x - cx) as f32;
                let dy = (y - cy) as f32;
                let d2 = dx * dx + dy * dy;
                if d2 >= r0_sq && d2 <= r1_sq {
                    acc += img[(x as u32, y as u32)][channel] as f32 / 255.0;
                    count += 1;
                }
            }
        }

        if count == 0 {
            0.0
        } else {
            acc / count as f32
        }
    }

    fn band_mean_channel(img: &RgbaImage, y: i32, half_width: i32, channel: usize) -> f32 {
        let y0 = (y - half_width).max(0) as u32;
        let y1 = (y + half_width).min(img.height() as i32 - 1) as u32;
        let mut acc = 0.0_f32;
        let mut count = 0_u32;

        for yy in y0..=y1 {
            for x in 0..img.width() {
                acc += img[(x, yy)][channel] as f32 / 255.0;
                count += 1;
            }
        }

        acc / count as f32
    }

    fn mean_luma(img: &RgbaImage) -> f32 {
        let mut acc = 0.0_f32;
        let mut count = 0_u32;
        for pixel in img.pixels() {
            let r = pixel[0] as f32 / 255.0;
            let g = pixel[1] as f32 / 255.0;
            let b = pixel[2] as f32 / 255.0;
            acc += 0.2126 * r + 0.7152 * g + 0.0722 * b;
            count += 1;
        }
        acc / count as f32
    }
}
