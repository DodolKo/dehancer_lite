use anyhow::{anyhow, Result};
use image::RgbaImage;
use serde::{Deserialize, Serialize};

use crate::effects::bloom::{apply_bloom, apply_bloom_gpu_reference, BloomParams};
use crate::effects::film_compression::{
    apply_film_compression, apply_film_compression_gpu_reference, FilmCompressionParams,
};
use crate::effects::finish::FinishParams;
use crate::effects::grain::{apply_grain, apply_grain_gpu_reference, GrainParams};
use crate::halation::{apply_halation, FilmPreset, HalationParams};

pub const PRESET_SCHEMA: &str = "dehancer-lite.preset";
pub const PRESET_VERSION: u32 = 1;

const PIPELINE_ORDER: [&str; 4] = ["bloom", "halation", "film_compression", "grain"];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderBackend {
    GpuPreferred,
    CpuReference,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HalationReferencePreset {
    Vision3PublicCalibV1,
    Vision3Subtle,
    Vision3Strong,
}

impl Default for HalationReferencePreset {
    fn default() -> Self {
        Self::Vision3PublicCalibV1
    }
}

impl From<FilmPreset> for HalationReferencePreset {
    fn from(value: FilmPreset) -> Self {
        match value {
            FilmPreset::KodakVision3_500T_PublicCalibV1 => Self::Vision3PublicCalibV1,
            FilmPreset::KodakVision3_500T_Subtle => Self::Vision3Subtle,
            FilmPreset::KodakVision3_500T_Strong => Self::Vision3Strong,
        }
    }
}

impl From<HalationReferencePreset> for FilmPreset {
    fn from(value: HalationReferencePreset) -> Self {
        match value {
            HalationReferencePreset::Vision3PublicCalibV1 => {
                FilmPreset::KodakVision3_500T_PublicCalibV1
            }
            HalationReferencePreset::Vision3Subtle => FilmPreset::KodakVision3_500T_Subtle,
            HalationReferencePreset::Vision3Strong => FilmPreset::KodakVision3_500T_Strong,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct HalationEffectParams {
    pub enabled: bool,
    pub reference: HalationReferencePreset,
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
}

impl Default for HalationEffectParams {
    fn default() -> Self {
        Self::from_legacy(
            FilmPreset::KodakVision3_500T_PublicCalibV1,
            FilmPreset::KodakVision3_500T_PublicCalibV1.defaults(),
        )
    }
}

impl HalationEffectParams {
    pub fn validated(self) -> Self {
        fn sane(value: f32, min: f32, max: f32, fallback: f32) -> f32 {
            if value.is_finite() {
                value.clamp(min, max)
            } else {
                fallback
            }
        }

        let d = Self::default();
        Self {
            enabled: self.enabled,
            reference: self.reference,
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
        }
    }

    pub fn from_legacy(reference: FilmPreset, params: HalationParams) -> Self {
        Self {
            enabled: true,
            reference: reference.into(),
            threshold: params.threshold,
            knee: params.knee,
            core_radius: params.core_radius,
            tail_radius: params.tail_radius,
            core_weight: params.core_weight,
            tail_weight: params.tail_weight,
            absorption_rgb: params.absorption_rgb,
            optical_depth: params.optical_depth,
            chroma_bias: params.chroma_bias,
            intensity: params.intensity,
            soft_clip: params.soft_clip,
        }
    }

    pub fn to_legacy_with_finish(self, finish: FinishParams) -> (HalationParams, FilmPreset) {
        let self_v = self.validated();
        let finish_v = finish.validated();

        let mut params = HalationParams {
            threshold: self_v.threshold,
            knee: self_v.knee,
            core_radius: self_v.core_radius,
            tail_radius: self_v.tail_radius,
            core_weight: self_v.core_weight,
            tail_weight: self_v.tail_weight,
            absorption_rgb: self_v.absorption_rgb,
            optical_depth: self_v.optical_depth,
            chroma_bias: self_v.chroma_bias,
            intensity: self_v.intensity,
            soft_clip: self_v.soft_clip,
            grain_amount: if finish_v.enabled {
                finish_v.legacy_grain_amount
            } else {
                0.0
            },
            grain_size: if finish_v.enabled {
                finish_v.legacy_grain_size
            } else {
                1.0
            },
            haze_strength: if finish_v.enabled {
                finish_v.haze_strength
            } else {
                0.0
            },
            lens_distortion: if finish_v.enabled {
                finish_v.lens_distortion
            } else {
                0.0
            },
        }
        .validated();

        if !self_v.enabled {
            params.intensity = 0.0;
            params.core_weight = 0.0;
            params.tail_weight = 0.0;
        }

        (params, self_v.reference.into())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct EffectStackParams {
    #[serde(default)]
    pub bloom: BloomParams,
    #[serde(default)]
    pub halation: HalationEffectParams,
    #[serde(default, rename = "film_compression")]
    pub film_compression: FilmCompressionParams,
    #[serde(default)]
    pub grain: GrainParams,
    #[serde(default)]
    pub finish: FinishParams,
}

impl Default for EffectStackParams {
    fn default() -> Self {
        Self {
            bloom: BloomParams::default(),
            halation: HalationEffectParams::default(),
            film_compression: FilmCompressionParams::default(),
            grain: GrainParams::default(),
            finish: FinishParams::default(),
        }
    }
}

impl EffectStackParams {
    pub fn validated(self) -> Self {
        Self {
            bloom: self.bloom.validated(),
            halation: self.halation.validated(),
            film_compression: self.film_compression.validated(),
            grain: self.grain.validated(),
            finish: self.finish.validated(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FilmPresetMetadata {
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FilmPresetV1 {
    pub schema: String,
    pub version: u32,
    pub name: String,
    pub pipeline_order: Vec<String>,
    pub effects: EffectStackParams,
    #[serde(default)]
    pub metadata: FilmPresetMetadata,
}

impl FilmPresetV1 {
    pub fn new(name: impl Into<String>, effects: EffectStackParams) -> Self {
        Self {
            schema: PRESET_SCHEMA.to_owned(),
            version: PRESET_VERSION,
            name: name.into(),
            pipeline_order: pipeline_order_strings(),
            effects: effects.validated(),
            metadata: FilmPresetMetadata::default(),
        }
    }

    pub fn validated(mut self) -> Self {
        self.schema = PRESET_SCHEMA.to_owned();
        self.version = PRESET_VERSION;
        self.pipeline_order = pipeline_order_strings();
        self.effects = self.effects.validated();
        self
    }

    pub fn from_json_str(json: &str) -> Result<Self> {
        let preset: FilmPresetV1 =
            serde_json::from_str(json).map_err(|err| anyhow!("invalid preset json: {err}"))?;
        if preset.schema != PRESET_SCHEMA {
            return Err(anyhow!(
                "unsupported preset schema `{}` (expected `{}`)",
                preset.schema,
                PRESET_SCHEMA
            ));
        }
        if preset.version != PRESET_VERSION {
            return Err(anyhow!(
                "unsupported preset version `{}` (expected `{}`)",
                preset.version,
                PRESET_VERSION
            ));
        }

        Ok(preset.validated())
    }

    pub fn to_json_pretty(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .map_err(|err| anyhow!("failed to serialize preset json: {err}"))
    }

    pub fn from_legacy_halation(
        name: impl Into<String>,
        film_preset: FilmPreset,
        params: HalationParams,
    ) -> Self {
        let effects = EffectStackParams {
            bloom: BloomParams {
                enabled: false,
                ..BloomParams::default()
            },
            halation: HalationEffectParams::from_legacy(film_preset, params),
            film_compression: FilmCompressionParams {
                enabled: false,
                ..FilmCompressionParams::default()
            },
            grain: GrainParams {
                enabled: false,
                ..GrainParams::default()
            },
            finish: FinishParams {
                enabled: params.grain_amount > 1e-6
                    || params.haze_strength > 1e-6
                    || params.lens_distortion.abs() > 1e-6,
                legacy_grain_amount: params.grain_amount,
                legacy_grain_size: params.grain_size,
                haze_strength: params.haze_strength,
                lens_distortion: params.lens_distortion,
            },
        };

        Self {
            schema: PRESET_SCHEMA.to_owned(),
            version: PRESET_VERSION,
            name: name.into(),
            pipeline_order: pipeline_order_strings(),
            effects: effects.validated(),
            metadata: FilmPresetMetadata {
                tags: vec!["legacy-halation".to_owned()],
                note: Some("Migrated from legacy halation parameters".to_owned()),
                source: Some("legacy-v0".to_owned()),
            },
        }
    }
}

impl Default for FilmPresetV1 {
    fn default() -> Self {
        preset_library()
            .into_iter()
            .next()
            .unwrap_or_else(|| FilmPresetV1::new("Vision3 50D Clean", EffectStackParams::default()))
    }
}

pub fn preset_library() -> Vec<FilmPresetV1> {
    vec![
        preset_vision3_50d_clean(),
        preset_vision3_250d_balanced(),
        preset_vision3_500t_night(),
        preset_no_remjet_strong_halo(),
        preset_kodak_2383_printish(),
        preset_portra_400_photo(),
    ]
}

fn pipeline_order_strings() -> Vec<String> {
    PIPELINE_ORDER.iter().map(|s| (*s).to_owned()).collect()
}

fn base_preset(name: &str, tags: &[&str]) -> FilmPresetV1 {
    let mut preset = FilmPresetV1::new(name, EffectStackParams::default());
    preset.metadata.tags = tags.iter().map(|v| (*v).to_owned()).collect();
    preset
}

fn preset_vision3_50d_clean() -> FilmPresetV1 {
    let mut p = base_preset("Vision3 50D Clean", &["cinema", "daylight", "clean"]);
    p.effects.bloom = BloomParams {
        threshold: 5.2,
        knee: 1.0,
        radius: 42.0,
        amplify: 0.20,
        saturation: 0.82,
        ..BloomParams::default()
    };
    p.effects.halation.reference = HalationReferencePreset::Vision3Subtle;
    p.effects.halation.intensity = 0.70;
    p.effects.halation.tail_radius = 20.0;
    p.effects.film_compression = FilmCompressionParams {
        impact: 0.30,
        color_density: 0.82,
        ..FilmCompressionParams::default()
    };
    p.effects.grain = GrainParams {
        amount: 0.14,
        size_px: 0.52,
        shadows: 1.08,
        midtones: 0.95,
        highlights: 0.65,
        ..GrainParams::default()
    };
    p.metadata.source = Some("dehancer-lite".to_owned());
    p.validated()
}

fn preset_vision3_250d_balanced() -> FilmPresetV1 {
    let mut p = base_preset("Vision3 250D Balanced", &["cinema", "daylight", "balanced"]);
    p.effects.bloom = BloomParams {
        threshold: 4.8,
        amplify: 0.28,
        radius: 48.0,
        ..BloomParams::default()
    };
    p.effects.halation.reference = HalationReferencePreset::Vision3PublicCalibV1;
    p.effects.halation.intensity = 0.95;
    p.effects.film_compression = FilmCompressionParams {
        impact: 0.35,
        tonal_range: 0.48,
        color_density: 0.88,
        ..FilmCompressionParams::default()
    };
    p.effects.grain = GrainParams {
        amount: 0.20,
        size_px: 0.62,
        ..GrainParams::default()
    };
    p.metadata.source = Some("dehancer-lite".to_owned());
    p.validated()
}

fn preset_vision3_500t_night() -> FilmPresetV1 {
    let mut p = base_preset("Vision3 500T Night", &["cinema", "tungsten", "night"]);
    p.effects.bloom = BloomParams {
        threshold: 4.0,
        amplify: 0.40,
        radius: 56.0,
        save_lights: 0.76,
        ..BloomParams::default()
    };
    p.effects.halation.reference = HalationReferencePreset::Vision3PublicCalibV1;
    p.effects.halation.intensity = 1.20;
    p.effects.halation.tail_radius = 30.0;
    p.effects.film_compression = FilmCompressionParams {
        impact: 0.42,
        tonal_range: 0.52,
        color_density: 0.92,
        ..FilmCompressionParams::default()
    };
    p.effects.grain = GrainParams {
        amount: 0.28,
        size_px: 0.72,
        shadows: 1.25,
        ..GrainParams::default()
    };
    p.metadata.source = Some("dehancer-lite".to_owned());
    p.validated()
}

fn preset_no_remjet_strong_halo() -> FilmPresetV1 {
    let mut p = base_preset(
        "No-Remjet Strong Halo",
        &["cinema", "no-remjet", "stylized"],
    );
    p.effects.bloom = BloomParams {
        threshold: 3.8,
        amplify: 0.46,
        radius: 62.0,
        saturation: 0.78,
        ..BloomParams::default()
    };
    p.effects.halation.reference = HalationReferencePreset::Vision3Strong;
    p.effects.halation.intensity = 1.42;
    p.effects.halation.tail_radius = 42.0;
    p.effects.halation.chroma_bias = [0.55, 0.08, -0.08];
    p.effects.film_compression = FilmCompressionParams {
        impact: 0.33,
        tonal_range: 0.44,
        ..FilmCompressionParams::default()
    };
    p.effects.grain = GrainParams {
        amount: 0.25,
        size_px: 0.66,
        ..GrainParams::default()
    };
    p.effects.finish = FinishParams {
        enabled: true,
        legacy_grain_amount: 0.04,
        legacy_grain_size: 1.0,
        haze_strength: 0.12,
        lens_distortion: 0.03,
    };
    p.metadata.source = Some("dehancer-lite".to_owned());
    p.validated()
}

fn preset_kodak_2383_printish() -> FilmPresetV1 {
    let mut p = base_preset("Kodak 2383 Print-ish", &["print", "contrast", "projection"]);
    p.effects.bloom = BloomParams {
        threshold: 5.0,
        amplify: 0.22,
        radius: 40.0,
        ..BloomParams::default()
    };
    p.effects.halation.reference = HalationReferencePreset::Vision3Subtle;
    p.effects.halation.intensity = 0.62;
    p.effects.film_compression = FilmCompressionParams {
        impact: 0.52,
        tonal_range: 0.38,
        color_density: 1.08,
        ..FilmCompressionParams::default()
    };
    p.effects.grain = GrainParams {
        amount: 0.16,
        size_px: 0.58,
        ..GrainParams::default()
    };
    p.metadata.source = Some("dehancer-lite".to_owned());
    p.validated()
}

fn preset_portra_400_photo() -> FilmPresetV1 {
    let mut p = base_preset("Portra 400 Photo", &["photo", "negative", "soft"]);
    p.effects.bloom = BloomParams {
        threshold: 5.4,
        amplify: 0.14,
        radius: 36.0,
        saturation: 0.9,
        ..BloomParams::default()
    };
    p.effects.halation.reference = HalationReferencePreset::Vision3Subtle;
    p.effects.halation.intensity = 0.48;
    p.effects.halation.tail_radius = 16.0;
    p.effects.film_compression = FilmCompressionParams {
        impact: 0.28,
        tonal_range: 0.58,
        color_density: 0.94,
        ..FilmCompressionParams::default()
    };
    p.effects.grain = GrainParams {
        amount: 0.19,
        size_px: 0.60,
        shadows: 1.05,
        midtones: 0.95,
        highlights: 0.72,
        ..GrainParams::default()
    };
    p.metadata.source = Some("dehancer-lite".to_owned());
    p.validated()
}

pub struct RenderRequest<'a> {
    pub input: &'a RgbaImage,
    pub preset: &'a FilmPresetV1,
    pub render_backend: RenderBackend,
}

pub struct FilmPipeline;

impl FilmPipeline {
    pub fn render(request: &RenderRequest<'_>) -> Result<RgbaImage> {
        let effects = request.preset.effects.validated();
        match request.render_backend {
            RenderBackend::CpuReference => Ok(Self::render_cpu(request.input, effects)),
            RenderBackend::GpuPreferred => Ok(Self::render_gpu_reference(request.input, effects)),
        }
    }

    fn render_cpu(input: &RgbaImage, effects: EffectStackParams) -> RgbaImage {
        let mut frame = input.clone();

        if effects.bloom.enabled {
            frame = apply_bloom(&frame, &effects.bloom);
        }

        if effects.halation.enabled || effects.finish.has_any_effect() {
            let (halation_params, halation_preset) =
                effects.halation.to_legacy_with_finish(effects.finish);
            frame = apply_halation(&frame, &halation_params, halation_preset);
        }

        if effects.film_compression.enabled {
            frame = apply_film_compression(&frame, &effects.film_compression);
        }

        if effects.grain.enabled {
            frame = apply_grain(&frame, &effects.grain);
        }

        frame
    }

    fn render_gpu_reference(input: &RgbaImage, effects: EffectStackParams) -> RgbaImage {
        let mut frame = input.clone();

        if effects.bloom.enabled {
            frame = apply_bloom_gpu_reference(&frame, &effects.bloom);
        }

        if effects.halation.enabled || effects.finish.has_any_effect() {
            let (halation_params, halation_preset) =
                effects.halation.to_legacy_with_finish(effects.finish);
            frame = apply_halation(&frame, &halation_params, halation_preset);
        }

        if effects.film_compression.enabled {
            frame = apply_film_compression_gpu_reference(&frame, &effects.film_compression);
        }

        if effects.grain.enabled {
            frame = apply_grain_gpu_reference(&frame, &effects.grain);
        }

        frame
    }
}

#[cfg(test)]
mod tests {
    use image::{Rgba, RgbaImage};

    use super::{
        preset_library, EffectStackParams, FilmPipeline, FilmPresetV1, RenderBackend, RenderRequest,
    };
    use crate::halation::{FilmPreset, HalationParams};

    #[test]
    fn preset_json_roundtrip() {
        let preset = preset_library().remove(0);
        let json = preset.to_json_pretty().expect("must serialize");
        let restored = FilmPresetV1::from_json_str(&json).expect("must deserialize");
        assert_eq!(preset, restored);
    }

    #[test]
    fn legacy_migration_maps_finish_fields() {
        let mut legacy = HalationParams::default();
        legacy.grain_amount = 0.12;
        legacy.grain_size = 2.0;
        legacy.haze_strength = 0.2;
        legacy.lens_distortion = 0.05;

        let migrated = FilmPresetV1::from_legacy_halation(
            "legacy",
            FilmPreset::KodakVision3_500T_Strong,
            legacy,
        );

        assert!(migrated.effects.finish.enabled);
        assert_eq!(migrated.effects.finish.legacy_grain_amount, 0.12);
        assert_eq!(migrated.effects.finish.legacy_grain_size, 2.0);
        assert_eq!(migrated.effects.finish.haze_strength, 0.2);
        assert_eq!(migrated.effects.finish.lens_distortion, 0.05);
    }

    #[test]
    fn pipeline_follows_fixed_order() {
        let mut img = RgbaImage::new(96, 96);
        for y in 0..96 {
            for x in 0..96 {
                let v = if (40..56).contains(&x) && (40..56).contains(&y) {
                    245
                } else {
                    10
                };
                img.put_pixel(x, y, Rgba([v, v, v, 255]));
            }
        }

        let preset = FilmPresetV1 {
            name: "order-test".to_owned(),
            effects: EffectStackParams::default(),
            ..FilmPresetV1::default()
        }
        .validated();

        let pipeline_out = FilmPipeline::render(&RenderRequest {
            input: &img,
            preset: &preset,
            render_backend: RenderBackend::CpuReference,
        })
        .expect("pipeline render should succeed");

        let mut manual = crate::effects::bloom::apply_bloom(&img, &preset.effects.bloom);
        let (h_params, h_preset) = preset
            .effects
            .halation
            .to_legacy_with_finish(preset.effects.finish);
        manual = crate::halation::apply_halation(&manual, &h_params, h_preset);
        manual = crate::effects::film_compression::apply_film_compression(
            &manual,
            &preset.effects.film_compression,
        );
        manual = crate::effects::grain::apply_grain(&manual, &preset.effects.grain);

        assert_eq!(pipeline_out.as_raw(), manual.as_raw());
    }

    #[test]
    fn cpu_gpu_reference_parity() {
        let mut img = RgbaImage::new(96, 64);
        for y in 0..64 {
            for x in 0..96 {
                let v = ((x + y) % 255) as u8;
                img.put_pixel(
                    x,
                    y,
                    Rgba([v, v.saturating_add(10), v.saturating_add(20), 255]),
                );
            }
        }

        let preset = FilmPresetV1::default();

        let cpu = FilmPipeline::render(&RenderRequest {
            input: &img,
            preset: &preset,
            render_backend: RenderBackend::CpuReference,
        })
        .expect("cpu render must succeed");

        let gpu = FilmPipeline::render(&RenderRequest {
            input: &img,
            preset: &preset,
            render_backend: RenderBackend::GpuPreferred,
        })
        .expect("gpu render must succeed");

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
}
