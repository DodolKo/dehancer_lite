use dehancer_lite::halation::{apply_halation, FilmPreset, HalationParams};
use image::{Rgba, RgbaImage};

#[test]
fn regression_public_scene_01_specular_headlight() {
    let scene = scene_headlight();
    let output = apply_halation(
        &scene,
        &HalationParams::default(),
        FilmPreset::KodakVision3_500T_PublicCalibV1,
    );

    let halo_strength = ring_mean_channel(&output, 96, 72, 10.0, 22.0, 0);
    assert!(halo_strength > 0.055, "halo too weak: {halo_strength}");
}

#[test]
fn regression_public_scene_02_neon_strip() {
    let scene = scene_neon();
    let output = apply_halation(
        &scene,
        &HalationParams::default(),
        FilmPreset::KodakVision3_500T_PublicCalibV1,
    );

    let red_band = horizontal_band_mean(&output, 64, 6, 0);
    let blue_band = horizontal_band_mean(&output, 64, 6, 2);
    assert!(red_band > blue_band, "expected warm tint in halo region");
}

#[test]
fn regression_public_scene_03_backlit_window() {
    let scene = scene_window();
    let output = apply_halation(
        &scene,
        &HalationParams::default(),
        FilmPreset::KodakVision3_500T_PublicCalibV1,
    );

    let in_luma = mean_luma(&scene);
    let out_luma = mean_luma(&output);
    assert!(out_luma > in_luma);
    assert!(out_luma < in_luma * 2.2);
}

fn scene_headlight() -> RgbaImage {
    let mut img = RgbaImage::new(192, 144);
    for y in 0..144 {
        for x in 0..192 {
            img.put_pixel(x, y, Rgba([5, 5, 7, 255]));
        }
    }

    for y in 64..80 {
        for x in 88..104 {
            let dx = x as i32 - 96;
            let dy = y as i32 - 72;
            if dx * dx + dy * dy < 30 {
                img.put_pixel(x, y, Rgba([255, 252, 245, 255]));
            }
        }
    }
    img
}

fn scene_neon() -> RgbaImage {
    let mut img = RgbaImage::new(224, 128);
    for y in 0..128 {
        for x in 0..224 {
            img.put_pixel(x, y, Rgba([4, 4, 9, 255]));
        }
    }

    for x in 20..204 {
        img.put_pixel(x, 64, Rgba([255, 255, 255, 255]));
    }
    img
}

fn scene_window() -> RgbaImage {
    let mut img = RgbaImage::new(200, 150);
    for y in 0..150 {
        for x in 0..200 {
            img.put_pixel(x, y, Rgba([12, 12, 16, 255]));
        }
    }

    for y in 40..112 {
        for x in 56..144 {
            img.put_pixel(x, y, Rgba([238, 238, 236, 255]));
        }
    }

    img
}

fn ring_mean_channel(img: &RgbaImage, cx: i32, cy: i32, r0: f32, r1: f32, channel: usize) -> f32 {
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

    acc / count.max(1) as f32
}

fn horizontal_band_mean(img: &RgbaImage, y: i32, half_width: i32, channel: usize) -> f32 {
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

    acc / count.max(1) as f32
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

    acc / count.max(1) as f32
}
