use dehancer_lite::halation::{FilmPreset, HalationParams};
use dehancer_lite::pipeline::{FilmPipeline, FilmPresetV1, RenderBackend, RenderRequest};
use image::{Rgba, RgbaImage};

#[test]
fn legacy_halation_preset_migration_is_strict() {
    let scene = scene_with_speculars();

    let mut legacy = HalationParams::default();
    legacy.threshold = 0.7;
    legacy.knee = 0.18;
    legacy.core_radius = 6.0;
    legacy.tail_radius = 30.0;
    legacy.core_weight = 0.9;
    legacy.tail_weight = 0.5;
    legacy.grain_amount = 0.1;
    legacy.grain_size = 2.0;
    legacy.haze_strength = 0.15;
    legacy.lens_distortion = 0.04;

    let reference = dehancer_lite::halation::apply_halation(
        &scene,
        &legacy,
        FilmPreset::KodakVision3_500T_PublicCalibV1,
    );

    let migrated = FilmPresetV1::from_legacy_halation(
        "legacy-regression",
        FilmPreset::KodakVision3_500T_PublicCalibV1,
        legacy,
    );

    let output = FilmPipeline::render(&RenderRequest {
        input: &scene,
        preset: &migrated,
        render_backend: RenderBackend::CpuReference,
    })
    .expect("pipeline render should succeed");

    assert_eq!(reference.as_raw(), output.as_raw());
}

#[test]
fn backend_parity_cpu_vs_gpu_reference() {
    let scene = scene_with_speculars();
    let preset = FilmPresetV1::default();

    let cpu = FilmPipeline::render(&RenderRequest {
        input: &scene,
        preset: &preset,
        render_backend: RenderBackend::CpuReference,
    })
    .expect("cpu render should succeed");

    let gpu = FilmPipeline::render(&RenderRequest {
        input: &scene,
        preset: &preset,
        render_backend: RenderBackend::GpuPreferred,
    })
    .expect("gpu render should succeed");

    let max_delta = cpu
        .pixels()
        .zip(gpu.pixels())
        .map(|(a, b)| {
            let dr = (a[0] as i16 - b[0] as i16).unsigned_abs() as u8;
            let dg = (a[1] as i16 - b[1] as i16).unsigned_abs() as u8;
            let db = (a[2] as i16 - b[2] as i16).unsigned_abs() as u8;
            dr.max(dg).max(db)
        })
        .max()
        .unwrap_or(0);

    assert!(max_delta <= 1, "backend delta too high: {max_delta}");
}

fn scene_with_speculars() -> RgbaImage {
    let mut img = RgbaImage::new(200, 140);

    for y in 0..img.height() {
        for x in 0..img.width() {
            let base = ((x + y) % 23) as u8;
            img.put_pixel(
                x,
                y,
                Rgba([
                    8_u8.saturating_add(base),
                    9_u8.saturating_add(base),
                    12_u8.saturating_add(base),
                    255,
                ]),
            );
        }
    }

    for y in 52..90 {
        for x in 72..128 {
            img.put_pixel(x, y, Rgba([245, 245, 242, 255]));
        }
    }

    for y in 30..36 {
        for x in 34..170 {
            img.put_pixel(x, y, Rgba([255, 255, 255, 255]));
        }
    }

    img
}
