use image::{Rgba, RgbaImage};

use crate::color::{acescg_to_linear, linear_to_acescg, linear_to_srgb, srgb_to_linear};

pub(crate) fn sane(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

pub(crate) fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let denom = (edge1 - edge0).abs().max(1e-6);
    let t = ((x - edge0) / denom).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub(crate) fn highlight_mask(luma: f32, threshold: f32, knee: f32) -> f32 {
    if knee <= 1e-6 {
        return (luma > threshold) as u8 as f32;
    }

    let edge0 = threshold - knee;
    let edge1 = threshold + knee;
    smoothstep(edge0, edge1, luma)
}

pub(crate) fn aces_luma(rgb: [f32; 3]) -> f32 {
    0.272_228_72 * rgb[0] + 0.674_081_74 * rgb[1] + 0.053_689_517 * rgb[2]
}

pub(crate) fn radius_to_sigma(radius: f32) -> f32 {
    if radius <= 0.0 {
        1.0
    } else {
        (radius * 0.5).max(1.0)
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

pub(crate) fn blur_rgb(
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
            for (k, weight) in kernel.iter().enumerate() {
                let offset = k as isize - radius as isize;
                let sx = (x as isize + offset).clamp(0, (width - 1) as isize) as usize;
                let sample = src[y * width + sx];
                acc[0] += sample[0] * *weight;
                acc[1] += sample[1] * *weight;
                acc[2] += sample[2] * *weight;
            }
            tmp[y * width + x] = acc;
        }
    }

    for y in 0..height {
        for x in 0..width {
            let mut acc = [0.0_f32; 3];
            for (k, weight) in kernel.iter().enumerate() {
                let offset = k as isize - radius as isize;
                let sy = (y as isize + offset).clamp(0, (height - 1) as isize) as usize;
                let sample = tmp[sy * width + x];
                acc[0] += sample[0] * *weight;
                acc[1] += sample[1] * *weight;
                acc[2] += sample[2] * *weight;
            }
            dst[y * width + x] = acc;
        }
    }

    dst
}

pub(crate) fn mix_saturation(rgb: [f32; 3], saturation: f32) -> [f32; 3] {
    let luma = aces_luma(rgb);
    let sat = saturation.max(0.0);
    [
        luma + (rgb[0] - luma) * sat,
        luma + (rgb[1] - luma) * sat,
        luma + (rgb[2] - luma) * sat,
    ]
}

pub(crate) fn hash_noise(x: u32, y: u32, salt: u32) -> f32 {
    let mut n = x
        .wrapping_mul(1_973)
        .wrapping_add(y.wrapping_mul(9_277))
        .wrapping_add(salt.wrapping_mul(26_699))
        .wrapping_add(0x68bc_21eb);
    n ^= n >> 15;
    n = n.wrapping_mul(2_246_822_519);
    n ^= n >> 13;
    n = n.wrapping_mul(3_266_489_917);
    n ^= n >> 16;
    (n as f32 / u32::MAX as f32) * 2.0 - 1.0
}

pub(crate) fn image_to_aces(input: &RgbaImage) -> Vec<[f32; 3]> {
    let mut out = Vec::with_capacity((input.width() * input.height()) as usize);
    for pixel in input.pixels() {
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
        out.push(linear_to_acescg(linear));
    }
    out
}

pub(crate) fn aces_to_image(
    input: &[[f32; 3]],
    width: u32,
    height: u32,
    alpha: &[u8],
) -> RgbaImage {
    let mut out = RgbaImage::new(width, height);
    for y in 0..height as usize {
        for x in 0..width as usize {
            let idx = y * width as usize + x;
            let linear = acescg_to_linear(input[idx]);
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
                Rgba([srgb[0], srgb[1], srgb[2], alpha[idx]]),
            );
        }
    }

    out
}

pub(crate) fn alpha_plane(input: &RgbaImage) -> Vec<u8> {
    input.pixels().map(|p| p[3]).collect()
}
