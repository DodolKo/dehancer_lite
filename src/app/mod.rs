#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

use anyhow::{Context, Result};
use eframe::egui;
use image::RgbaImage;

use crate::gpu::GpuHalationProcessor;
#[cfg(not(target_arch = "wasm32"))]
use crate::halation::apply_halation;
use crate::halation::{FilmPreset, HalationParams};

pub struct HalationApp {
    ctx: egui::Context,
    active_preset: FilmPreset,
    control_mode: ControlMode,
    params: HalationParams,
    source: Option<LoadedImage>,
    gpu: Option<GpuHalationProcessor>,
    preview_texture_id: Option<egui::TextureId>,
    preview_size: [usize; 2],
    dirty: bool,
    status: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ControlMode {
    Simple,
    Complex,
}

struct LoadedImage {
    name: String,
    rgba: RgbaImage,
}

impl HalationApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let status = if cc.wgpu_render_state.is_some() {
            "WebGPU backend ready.".to_owned()
        } else {
            "WebGPU backend unavailable. This MVP requires WGPU/WebGPU.".to_owned()
        };

        Self {
            ctx: cc.egui_ctx.clone(),
            active_preset: FilmPreset::KodakVision3_500T_PublicCalibV1,
            control_mode: ControlMode::Simple,
            params: FilmPreset::KodakVision3_500T_PublicCalibV1.defaults(),
            source: None,
            gpu: None,
            preview_texture_id: None,
            preview_size: [1, 1],
            dirty: false,
            status,
        }
    }

    fn process_if_needed(&mut self, frame: &mut eframe::Frame) {
        if !self.dirty {
            return;
        }

        let Some(source) = &self.source else {
            self.dirty = false;
            return;
        };

        let Some(render_state) = frame.wgpu_render_state() else {
            self.status =
                "Cannot process image: WebGPU renderer state is not available in this runtime."
                    .to_owned();
            return;
        };

        let processor = self.gpu.get_or_insert_with(|| {
            GpuHalationProcessor::new(render_state, source.rgba.width(), source.rgba.height())
        });

        match processor.process(render_state, &source.rgba, self.params) {
            Ok(texture_id) => {
                self.preview_texture_id = Some(texture_id);
                self.preview_size = processor.output_size();
                self.status = format!(
                    "Processed {} ({}x{})",
                    source.name,
                    source.rgba.width(),
                    source.rgba.height()
                );
                self.dirty = false;
            }
            Err(err) => {
                self.status = format!("GPU processing failed: {err:#}");
            }
        }
    }

    fn controls_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;

        ui.heading("Halation Physique MVP");
        ui.label("Calibration: Kodak Vision3 500T family");
        ui.label("Working space: ACEScg, compute pipeline WebGPU");
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("Mode");
            ui.selectable_value(&mut self.control_mode, ControlMode::Simple, "Simple");
            ui.selectable_value(&mut self.control_mode, ControlMode::Complex, "Complex");
        });

        egui::ComboBox::from_label("Preset")
            .selected_text(preset_label(self.active_preset))
            .show_ui(ui, |ui| {
                for preset in [
                    FilmPreset::KodakVision3_500T_PublicCalibV1,
                    FilmPreset::KodakVision3_500T_Subtle,
                    FilmPreset::KodakVision3_500T_Strong,
                ] {
                    if ui
                        .selectable_label(self.active_preset == preset, preset_label(preset))
                        .clicked()
                    {
                        self.active_preset = preset;
                        self.params = preset.defaults();
                        changed = true;
                    }
                }
            });

        if ui.button("Reset Preset").clicked() {
            self.params = self.active_preset.defaults();
            changed = true;
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            ui.horizontal(|ui| {
                if ui.button("Open Image").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("Image", &["png", "jpg", "jpeg", "tif", "tiff", "bmp"])
                        .pick_file()
                    {
                        match self.load_image_from_path(&path) {
                            Ok(()) => {
                                changed = true;
                            }
                            Err(err) => {
                                self.status = format!("Open failed: {err:#}");
                            }
                        }
                    }
                }

                if ui.button("Export Processed PNG").clicked() {
                    if let Err(err) = self.export_processed_image() {
                        self.status = format!("Export failed: {err:#}");
                    }
                }
            });
        }

        #[cfg(target_arch = "wasm32")]
        {
            ui.label("Web build: drop an image file into the window.");
            ui.label("WebGPU only; no WebGL/CPU fallback in this MVP.");
        }

        ui.separator();
        match self.control_mode {
            ControlMode::Simple => {
                changed |= self.simple_controls_ui(ui);
            }
            ControlMode::Complex => {
                changed |= self.complex_controls_ui(ui);
            }
        }

        changed
    }

    fn simple_controls_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;

        ui.label("Essentials");
        changed |= ui
            .add(egui::Slider::new(&mut self.params.intensity, 0.0..=3.0).text("halation"))
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.params.tail_radius, 0.0..=120.0)
                    .step_by(1.0)
                    .text("halo size"),
            )
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut self.params.threshold, 0.0..=4.0).text("highlight pickup"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut self.params.soft_clip, 0.0..=2.0).text("highlight rolloff"))
            .changed();

        ui.separator();
        ui.label("Texture and lens");
        changed |= ui
            .add(egui::Slider::new(&mut self.params.grain_amount, 0.0..=0.18).text("grain"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut self.params.haze_strength, 0.0..=1.5).text("haze"))
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.params.lens_distortion, -0.35..=0.35)
                    .text("lens distortion"),
            )
            .changed();

        changed
    }

    fn complex_controls_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;

        ui.label("Highlight Extraction");
        changed |= ui
            .add(egui::Slider::new(&mut self.params.threshold, 0.0..=4.0).text("threshold"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut self.params.knee, 0.0..=1.5).text("knee"))
            .changed();

        ui.separator();
        ui.label("Scattering");
        changed |= ui
            .add(
                egui::Slider::new(&mut self.params.core_radius, 0.0..=48.0)
                    .step_by(1.0)
                    .text("core radius px"),
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.params.tail_radius, 0.0..=120.0)
                    .step_by(1.0)
                    .text("tail radius px"),
            )
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut self.params.core_weight, 0.0..=2.5).text("core weight"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut self.params.tail_weight, 0.0..=2.5).text("tail weight"))
            .changed();

        ui.separator();
        ui.label("Absorption (Beer-Lambert-inspired)");
        changed |= ui
            .add(
                egui::Slider::new(&mut self.params.absorption_rgb[0], 0.0..=4.0)
                    .text("absorption R"),
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.params.absorption_rgb[1], 0.0..=4.0)
                    .text("absorption G"),
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.params.absorption_rgb[2], 0.0..=4.0)
                    .text("absorption B"),
            )
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut self.params.optical_depth, 0.0..=3.0).text("optical depth"))
            .changed();

        ui.separator();
        ui.label("Composite");
        changed |= ui
            .add(egui::Slider::new(&mut self.params.intensity, 0.0..=3.0).text("intensity"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut self.params.soft_clip, 0.0..=2.0).text("soft clip"))
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.params.chroma_bias[0], -0.5..=1.2)
                    .text("chroma bias R"),
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.params.chroma_bias[1], -0.5..=1.2)
                    .text("chroma bias G"),
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.params.chroma_bias[2], -0.5..=1.2)
                    .text("chroma bias B"),
            )
            .changed();

        ui.separator();
        ui.label("Texture and lens");
        changed |= ui
            .add(egui::Slider::new(&mut self.params.grain_amount, 0.0..=0.25).text("grain amount"))
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.params.grain_size, 1.0..=8.0)
                    .step_by(1.0)
                    .text("grain size"),
            )
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut self.params.haze_strength, 0.0..=2.0).text("haze"))
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.params.lens_distortion, -0.5..=0.5)
                    .text("lens distortion"),
            )
            .changed();

        changed
    }

    fn preview_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Preview");

        let Some(source) = &self.source else {
            ui.label("Load or drop an image to start the halation simulation.");
            return;
        };

        ui.label(format!(
            "Source: {} ({}x{})",
            source.name,
            source.rgba.width(),
            source.rgba.height()
        ));

        if let Some(texture_id) = self.preview_texture_id {
            let available = ui.available_size();
            let size = fit_size(self.preview_size, available);
            ui.image((texture_id, size));
        } else {
            ui.label("No processed frame yet.");
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped_files = ctx.input(|i| i.raw.dropped_files.clone());
        for dropped in dropped_files {
            if let Some(bytes) = dropped.bytes {
                let name = if dropped.name.is_empty() {
                    "dropped-image".to_owned()
                } else {
                    dropped.name
                };
                match self.load_image_from_bytes(&name, bytes.as_ref()) {
                    Ok(()) => {
                        self.dirty = true;
                    }
                    Err(err) => {
                        self.status = format!("Drop failed: {err:#}");
                    }
                }
                return;
            }

            #[cfg(not(target_arch = "wasm32"))]
            if let Some(path) = dropped.path {
                match self.load_image_from_path(&path) {
                    Ok(()) => {
                        self.dirty = true;
                    }
                    Err(err) => {
                        self.status = format!("Drop failed: {err:#}");
                    }
                }
                return;
            }
        }
    }

    fn load_image_from_bytes(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        let decoded = image::load_from_memory(bytes)
            .with_context(|| format!("unsupported image content for {name}"))?
            .to_rgba8();

        self.source = Some(LoadedImage {
            name: name.to_owned(),
            rgba: decoded,
        });
        self.dirty = true;
        self.ctx.request_repaint();
        Ok(())
    }

    pub fn load_image_from_web(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        self.load_image_from_bytes(name, bytes)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn load_image_from_path(&mut self, path: &Path) -> Result<()> {
        let bytes = std::fs::read(path)
            .with_context(|| format!("failed to read image file {}", path.display()))?;
        let name = path
            .file_name()
            .map(|v| v.to_string_lossy().to_string())
            .unwrap_or_else(|| "image".to_owned());
        self.load_image_from_bytes(&name, &bytes)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn export_processed_image(&mut self) -> Result<()> {
        let Some(source) = &self.source else {
            self.status = "No source image loaded.".to_owned();
            return Ok(());
        };

        let Some(path) = rfd::FileDialog::new()
            .add_filter("PNG", &["png"])
            .set_file_name("halation-output.png")
            .save_file()
        else {
            return Ok(());
        };

        let rendered = apply_halation(&source.rgba, &self.params, self.active_preset);
        rendered
            .save(&path)
            .with_context(|| format!("failed to save {}", path.display()))?;

        self.status = format!("Saved {}", path.display());
        Ok(())
    }
}

impl eframe::App for HalationApp {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.handle_dropped_files(ui.ctx());
        self.process_if_needed(frame);

        ui.horizontal(|ui| {
            ui.label("Status:");
            ui.monospace(&self.status);
        });
        ui.separator();

        let mut changed = false;
        ui.columns(2, |columns| {
            changed = self.controls_ui(&mut columns[0]);
            columns[0].separator();
            self.preview_ui(&mut columns[1]);
        });

        if changed {
            self.params = self.params.validated();
            self.dirty = true;
            ui.ctx().request_repaint();
        }

        if self.dirty {
            ui.ctx().request_repaint();
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

fn fit_size(image_size: [usize; 2], available: egui::Vec2) -> egui::Vec2 {
    let src_w = image_size[0].max(1) as f32;
    let src_h = image_size[1].max(1) as f32;

    let sx = if src_w > 0.0 {
        available.x / src_w
    } else {
        1.0
    };
    let sy = if src_h > 0.0 {
        available.y / src_h
    } else {
        1.0
    };
    let scale = sx.min(sy).clamp(0.05, 1.0);

    egui::vec2(src_w * scale, src_h * scale)
}

fn preset_label(preset: FilmPreset) -> &'static str {
    match preset {
        FilmPreset::KodakVision3_500T_PublicCalibV1 => "Vision3 PublicCalibV1",
        FilmPreset::KodakVision3_500T_Subtle => "Vision3 Subtle",
        FilmPreset::KodakVision3_500T_Strong => "Vision3 Strong",
    }
}
