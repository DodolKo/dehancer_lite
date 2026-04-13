use image::RgbaImage;
use serde::{Deserialize, Serialize};

use crate::effects::utils::{
    aces_luma, aces_to_image, alpha_plane, blur_rgb, highlight_mask, image_to_aces, mix_saturation,
    radius_to_sigma, sane, smoothstep,
};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BloomParams {
    pub enabled: bool,
    pub threshold: f32,
    pub knee: f32,
    pub radius: f32,
    pub amplify: f32,
    pub saturation: f32,
    pub save_lights: f32,
    pub source_limiter: f32,
    pub details: f32,
    pub veiling_glare_floor: f32,
}

impl Default for BloomParams {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: 4.0,
            knee: 1.2,
            radius: 48.0,
            amplify: 0.35,
            saturation: 0.85,
            save_lights: 0.70,
            source_limiter: 0.2,
            details: 0.2,
            veiling_glare_floor: 0.01,
        }
    }
}

impl BloomParams {
    pub fn validated(self) -> Self {
        let d = Self::default();
        Self {
            enabled: self.enabled,
            threshold: sane(self.threshold, 0.01, 24.0, d.threshold),
            knee: sane(self.knee, 0.0, 8.0, d.knee),
            radius: sane(self.radius, 0.0, 320.0, d.radius),
            amplify: sane(self.amplify, 0.0, 6.0, d.amplify),
            saturation: sane(self.saturation, 0.0, 2.0, d.saturation),
            save_lights: sane(self.save_lights, 0.0, 1.0, d.save_lights),
            source_limiter: sane(self.source_limiter, 0.0, 1.0, d.source_limiter),
            details: sane(self.details, 0.0, 1.0, d.details),
            veiling_glare_floor: sane(self.veiling_glare_floor, 0.0, 0.2, d.veiling_glare_floor),
        }
    }

    fn radius_px(self) -> usize {
        self.radius.round().clamp(0.0, 320.0) as usize
    }
}

pub fn apply_bloom(input: &RgbaImage, params: &BloomParams) -> RgbaImage {
    let params = params.validated();
    if !params.enabled || params.amplify <= 1e-6 || params.radius_px() == 0 {
        return input.clone();
    }

    let width = input.width() as usize;
    let height = input.height() as usize;
    let len = width * height;
    let alpha = alpha_plane(input);

    let base = image_to_aces(input);
    let mut extracted = vec![[0.0_f32; 3]; len];

    for idx in 0..len {
        let luma = aces_luma(base[idx]);
        let mut mask = highlight_mask(luma, params.threshold, params.knee);
        let limiter = 1.0
            - params.source_limiter
                * smoothstep(params.threshold * 1.25, params.threshold * 2.0, luma);
        mask *= limiter.max(0.0);
        extracted[idx] = [
            base[idx][0] * mask,
            base[idx][1] * mask,
            base[idx][2] * mask,
        ];
    }

    let blurred = blur_rgb(
        &extracted,
        width,
        height,
        params.radius_px(),
        radius_to_sigma(params.radius),
    );

    let mut composed = vec![[0.0_f32; 3]; len];
    for idx in 0..len {
        let luma = aces_luma(base[idx]);
        let protect = 1.0
            - params.save_lights
                * smoothstep(params.threshold, params.threshold + params.knee, luma);
        let detail_gate =
            1.0 - params.details * smoothstep(params.threshold * 0.4, params.threshold, luma);

        let mut bloom = [
            blurred[idx][0] * params.amplify * protect * detail_gate,
            blurred[idx][1] * params.amplify * protect * detail_gate,
            blurred[idx][2] * params.amplify * protect * detail_gate,
        ];

        let floor_boost = params.veiling_glare_floor * params.amplify;
        bloom[0] += floor_boost;
        bloom[1] += floor_boost;
        bloom[2] += floor_boost;

        let bloom = mix_saturation(bloom, params.saturation);
        composed[idx] = [
            (base[idx][0] + bloom[0]).max(0.0),
            (base[idx][1] + bloom[1]).max(0.0),
            (base[idx][2] + bloom[2]).max(0.0),
        ];
    }

    aces_to_image(&composed, input.width(), input.height(), &alpha)
}

pub fn apply_bloom_gpu_reference(input: &RgbaImage, params: &BloomParams) -> RgbaImage {
    // MVP reference path: this mirrors CPU behavior until dedicated compute kernels are wired.
    apply_bloom(input, params)
}

#[cfg(test)]
mod tests {
    use image::{Rgba, RgbaImage};

    use super::{apply_bloom, BloomParams};

    #[test]
    fn bloom_is_visible_off_core_and_preserves_core() {
        let mut img = RgbaImage::new(128, 128);
        for y in 0..128 {
            for x in 0..128 {
                img.put_pixel(x, y, Rgba([6, 6, 6, 255]));
            }
        }
        for y in 56..72 {
            for x in 56..72 {
                img.put_pixel(x, y, Rgba([255, 255, 255, 255]));
            }
        }

        let out = apply_bloom(
            &img,
            &BloomParams {
                threshold: 0.7,
                knee: 0.2,
                radius: 16.0,
                amplify: 0.8,
                ..BloomParams::default()
            },
        );

        let ring = mean_ring(&out, 64, 64, 16.0, 28.0);
        let core = mean_box(&out, 58, 58, 12, 12);
        assert!(
            ring > 0.09,
            "expected brighter ring around core, got {ring}"
        );
        assert!(core > ring, "core should stay brighter than the halo ring");
    }

    #[test]
    fn disabled_bloom_is_noop() {
        let mut img = RgbaImage::new(32, 32);
        for y in 0..32 {
            for x in 0..32 {
                img.put_pixel(x, y, Rgba([40, 30, 20, 255]));
            }
        }

        let out = apply_bloom(
            &img,
            &BloomParams {
                enabled: false,
                ..BloomParams::default()
            },
        );

        assert_eq!(img.as_raw(), out.as_raw());
    }

    fn mean_ring(img: &RgbaImage, cx: i32, cy: i32, r0: f32, r1: f32) -> f32 {
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
                    let p = img[(x as u32, y as u32)];
                    acc += (p[0] as f32 + p[1] as f32 + p[2] as f32) / (3.0 * 255.0);
                    count += 1;
                }
            }
        }

        acc / count.max(1) as f32
    }

    fn mean_box(img: &RgbaImage, x0: u32, y0: u32, w: u32, h: u32) -> f32 {
        let mut acc = 0.0_f32;
        let mut count = 0_u32;
        for y in y0..(y0 + h) {
            for x in x0..(x0 + w) {
                let p = img[(x, y)];
                acc += (p[0] as f32 + p[1] as f32 + p[2] as f32) / (3.0 * 255.0);
                count += 1;
            }
        }
        acc / count.max(1) as f32
    }
}
