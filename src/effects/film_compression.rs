use image::RgbaImage;
use serde::{Deserialize, Serialize};

use crate::effects::utils::{aces_luma, aces_to_image, alpha_plane, image_to_aces, sane};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct FilmCompressionParams {
    pub enabled: bool,
    pub impact: f32,
    pub white_point: f32,
    pub tonal_range: f32,
    pub color_density: f32,
}

impl Default for FilmCompressionParams {
    fn default() -> Self {
        Self {
            enabled: true,
            impact: 0.35,
            white_point: 1.0,
            tonal_range: 0.45,
            color_density: 0.85,
        }
    }
}

impl FilmCompressionParams {
    pub fn validated(self) -> Self {
        let d = Self::default();
        Self {
            enabled: self.enabled,
            impact: sane(self.impact, 0.0, 1.0, d.impact),
            white_point: sane(self.white_point, 0.25, 4.0, d.white_point),
            tonal_range: sane(self.tonal_range, 0.0, 1.0, d.tonal_range),
            color_density: sane(self.color_density, 0.0, 2.0, d.color_density),
        }
    }
}

pub fn apply_film_compression(input: &RgbaImage, params: &FilmCompressionParams) -> RgbaImage {
    let params = params.validated();
    if !params.enabled || params.impact <= 1e-6 {
        return input.clone();
    }

    let width = input.width();
    let height = input.height();
    let alpha = alpha_plane(input);
    let mut working = image_to_aces(input);

    let density = (1.0 + (params.color_density - 1.0) * params.impact).max(0.0);

    for pixel in &mut working {
        let in_luma = aces_luma(*pixel);

        let mut compressed = [0.0_f32; 3];
        for channel in 0..3 {
            compressed[channel] = compress_channel(pixel[channel], params);
        }

        let out_luma = aces_luma(compressed);
        let chroma = [
            compressed[0] - out_luma,
            compressed[1] - out_luma,
            compressed[2] - out_luma,
        ];

        let luma_anchor = out_luma.max(in_luma * (1.0 - params.impact * 0.2));
        *pixel = [
            (luma_anchor + chroma[0] * density).max(0.0),
            (luma_anchor + chroma[1] * density).max(0.0),
            (luma_anchor + chroma[2] * density).max(0.0),
        ];
    }

    aces_to_image(&working, width, height, &alpha)
}

pub fn apply_film_compression_gpu_reference(
    input: &RgbaImage,
    params: &FilmCompressionParams,
) -> RgbaImage {
    // MVP reference path: mirrors CPU behavior for backend parity tests.
    apply_film_compression(input, params)
}

pub(crate) fn compress_channel(value: f32, params: FilmCompressionParams) -> f32 {
    let white = params.white_point.max(1e-4);
    let shoulder_start = white * (1.0 - 0.85 * params.tonal_range).clamp(0.05, 1.0);

    if value <= shoulder_start {
        return value.max(0.0);
    }

    let d = value - shoulder_start;
    let k = 1.0 + params.impact * 6.0;
    shoulder_start + d / (1.0 + (params.impact * d / (k * white)).max(0.0))
}

#[cfg(test)]
mod tests {
    use image::{Rgba, RgbaImage};

    use super::{apply_film_compression, compress_channel, FilmCompressionParams};

    #[test]
    fn compression_rolls_off_high_values() {
        let params = FilmCompressionParams::default().validated();
        let c1 = compress_channel(1.2, params);
        let c2 = compress_channel(4.0, params);
        assert!(c1 < 1.2);
        assert!(c2 < 4.0);
        assert!(c2 > c1);
    }

    #[test]
    fn compression_avoids_hard_clip() {
        let mut img = RgbaImage::new(64, 64);
        for y in 0..64 {
            for x in 0..64 {
                let v = if (20..44).contains(&x) && (20..44).contains(&y) {
                    255
                } else {
                    180
                };
                img.put_pixel(x, y, Rgba([v, v, v, 255]));
            }
        }

        let out = apply_film_compression(
            &img,
            &FilmCompressionParams {
                impact: 0.9,
                tonal_range: 0.8,
                white_point: 0.9,
                ..FilmCompressionParams::default()
            },
        );

        let in_peak = img.pixels().map(|p| p[0]).max().unwrap_or(0);
        let out_peak = out.pixels().map(|p| p[0]).max().unwrap_or(0);
        assert!(out_peak <= in_peak);
        assert!(out_peak > 200);
    }
}
