#[cfg(not(target_arch = "wasm32"))]
use std::path::Path;

use anyhow::{Context, Result};
use eframe::egui;
use image::RgbaImage;

use crate::pipeline::{
    preset_library, FilmPipeline, FilmPresetV1, HalationReferencePreset, RenderBackend,
    RenderRequest,
};

#[cfg(target_arch = "wasm32")]
const PRESET_STORAGE_KEY: &str = "dehancer_lite.preset.v1";

pub struct HalationApp {
    ctx: egui::Context,
    control_mode: ControlMode,
    render_backend: RenderBackend,
    preset_library: Vec<FilmPresetV1>,
    selected_library_preset: Option<usize>,
    preset: FilmPresetV1,
    source: Option<LoadedImage>,
    processed: Option<RgbaImage>,
    preview_texture: Option<egui::TextureHandle>,
    preview_size: [usize; 2],
    dirty: bool,
    status: String,
    show_preset_json: bool,
    preset_json_buffer: String,
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
            "Renderer backend ready.".to_owned()
        } else {
            "Renderer backend unavailable in this runtime.".to_owned()
        };

        let library = preset_library();
        let preset = library.first().cloned().unwrap_or_default();

        #[allow(unused_mut)]
        let mut app = Self {
            ctx: cc.egui_ctx.clone(),
            control_mode: ControlMode::Simple,
            render_backend: RenderBackend::GpuPreferred,
            selected_library_preset: Some(0),
            preset_library: library,
            preset,
            source: None,
            processed: None,
            preview_texture: None,
            preview_size: [1, 1],
            dirty: false,
            status,
            show_preset_json: false,
            preset_json_buffer: String::new(),
        };

        #[cfg(target_arch = "wasm32")]
        app.restore_preset_from_browser_storage();

        app
    }

    fn process_if_needed(&mut self) {
        if !self.dirty {
            return;
        }

        let Some(source) = &self.source else {
            self.dirty = false;
            return;
        };

        let source_name = source.name.clone();
        let source_width = source.rgba.width();
        let source_height = source.rgba.height();
        let request = RenderRequest {
            input: &source.rgba,
            preset: &self.preset,
            render_backend: self.render_backend,
        };
        let processed = FilmPipeline::render(&request);

        match processed {
            Ok(processed) => {
                self.preview_size = [processed.width() as usize, processed.height() as usize];
                self.update_preview_texture(&processed);
                self.processed = Some(processed);
                self.status = format!(
                    "Processed {} ({}x{}) · {:?}",
                    source_name, source_width, source_height, self.render_backend
                );
                self.dirty = false;
            }
            Err(err) => {
                self.status = format!("Pipeline processing failed: {err:#}");
            }
        }
    }

    fn update_preview_texture(&mut self, image: &RgbaImage) {
        let color_image = egui::ColorImage::from_rgba_unmultiplied(
            [image.width() as usize, image.height() as usize],
            image.as_raw(),
        );

        if let Some(texture) = self.preview_texture.as_mut() {
            texture.set(color_image, egui::TextureOptions::LINEAR);
        } else {
            self.preview_texture = Some(self.ctx.load_texture(
                "film-stack-preview",
                color_image,
                egui::TextureOptions::LINEAR,
            ));
        }
    }

    fn controls_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;

        ui.heading("Film Stack MVP");
        ui.label("ACEScg working space · fixed order: Bloom → Halation → Compression → Grain");

        ui.horizontal(|ui| {
            ui.label("Render backend");
            egui::ComboBox::from_id_salt("render-backend")
                .selected_text(match self.render_backend {
                    RenderBackend::GpuPreferred => "GPU Preferred",
                    RenderBackend::CpuReference => "CPU Reference",
                })
                .show_ui(ui, |ui| {
                    changed |= ui
                        .selectable_value(
                            &mut self.render_backend,
                            RenderBackend::GpuPreferred,
                            "GPU Preferred",
                        )
                        .changed();
                    changed |= ui
                        .selectable_value(
                            &mut self.render_backend,
                            RenderBackend::CpuReference,
                            "CPU Reference",
                        )
                        .changed();
                });
        });

        ui.separator();

        ui.horizontal(|ui| {
            ui.label("Preset library");
            egui::ComboBox::from_id_salt("preset-library")
                .selected_text(self.current_library_label())
                .show_ui(ui, |ui| {
                    for (idx, preset) in self.preset_library.iter().enumerate() {
                        if ui
                            .selectable_label(
                                self.selected_library_preset == Some(idx),
                                &preset.name,
                            )
                            .clicked()
                        {
                            self.selected_library_preset = Some(idx);
                            self.preset = preset.clone().validated();
                            changed = true;
                        }
                    }
                });

            if ui.button("Reset").clicked() {
                self.preset = FilmPresetV1::default();
                self.selected_library_preset = self.find_library_match();
                changed = true;
            }
        });

        #[cfg(not(target_arch = "wasm32"))]
        {
            ui.horizontal(|ui| {
                if ui.button("Load Preset JSON").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter("JSON", &["json"])
                        .pick_file()
                    {
                        match self.load_preset_from_path(&path) {
                            Ok(()) => {
                                changed = true;
                            }
                            Err(err) => {
                                self.status = format!("Preset import failed: {err:#}");
                            }
                        }
                    }
                }

                if ui.button("Save Preset JSON").clicked() {
                    if let Err(err) = self.save_preset_to_path_dialog() {
                        self.status = format!("Preset export failed: {err:#}");
                    }
                }
            });
        }

        ui.horizontal(|ui| {
            if ui.button("Show JSON").clicked() {
                self.show_preset_json = !self.show_preset_json;
                if self.show_preset_json && self.preset_json_buffer.is_empty() {
                    if let Ok(json) = self.preset.to_json_pretty() {
                        self.preset_json_buffer = json;
                    }
                }
            }

            if ui.button("Refresh JSON From Preset").clicked() {
                match self.preset.to_json_pretty() {
                    Ok(json) => {
                        self.preset_json_buffer = json;
                    }
                    Err(err) => {
                        self.status = format!("Cannot serialize preset: {err:#}");
                    }
                }
            }

            if ui.button("Apply JSON Buffer").clicked() {
                match FilmPresetV1::from_json_str(&self.preset_json_buffer) {
                    Ok(preset) => {
                        self.preset = preset;
                        self.selected_library_preset = self.find_library_match();
                        changed = true;
                        self.status = "Preset JSON applied.".to_owned();
                    }
                    Err(err) => {
                        self.status = format!("Preset JSON invalid: {err:#}");
                    }
                }
            }
        });

        if self.show_preset_json {
            ui.add(
                egui::TextEdit::multiline(&mut self.preset_json_buffer)
                    .desired_rows(12)
                    .desired_width(f32::INFINITY),
            );
        }

        ui.separator();

        ui.horizontal_wrapped(|ui| {
            changed |= ui
                .checkbox(&mut self.preset.effects.bloom.enabled, "Bloom")
                .changed();
            changed |= ui
                .checkbox(&mut self.preset.effects.halation.enabled, "Halation")
                .changed();
            changed |= ui
                .checkbox(
                    &mut self.preset.effects.film_compression.enabled,
                    "Film Compression",
                )
                .changed();
            changed |= ui
                .checkbox(&mut self.preset.effects.grain.enabled, "Grain")
                .changed();
            changed |= ui
                .checkbox(&mut self.preset.effects.finish.enabled, "Finish (legacy)")
                .changed();
        });

        ui.separator();

        ui.collapsing("Bloom", |ui| {
            changed |= self.bloom_ui(ui);
        });

        ui.collapsing("Halation", |ui| {
            changed |= self.halation_ui(ui);
        });

        ui.collapsing("Film Compression", |ui| {
            changed |= self.compression_ui(ui);
        });

        ui.collapsing("Grain", |ui| {
            changed |= self.grain_ui(ui);
        });

        ui.collapsing("Finish (legacy mapped to halation)", |ui| {
            changed |= self.finish_ui(ui);
        });

        #[cfg(not(target_arch = "wasm32"))]
        {
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Open Image").clicked() {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter(
                            "Image",
                            &["png", "jpg", "jpeg", "tif", "tiff", "bmp", "webp"],
                        )
                        .pick_file()
                    {
                        match self.load_image_from_path(&path) {
                            Ok(()) => {
                                changed = true;
                            }
                            Err(err) => {
                                self.status = format!("Open image failed: {err:#}");
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

        changed
    }

    fn bloom_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        let bloom = &mut self.preset.effects.bloom;
        changed |= ui
            .add(egui::Slider::new(&mut bloom.threshold, 0.1..=12.0).text("threshold"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut bloom.knee, 0.0..=4.0).text("knee"))
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut bloom.radius, 0.0..=220.0)
                    .step_by(1.0)
                    .text("diffusion radius"),
            )
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut bloom.amplify, 0.0..=4.0).text("amplify"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut bloom.saturation, 0.0..=2.0).text("saturation"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut bloom.save_lights, 0.0..=1.0).text("save lights"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut bloom.source_limiter, 0.0..=1.0).text("source limiter"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut bloom.details, 0.0..=1.0).text("details preserve"))
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut bloom.veiling_glare_floor, 0.0..=0.08)
                    .text("veiling glare floor"),
            )
            .changed();
        changed
    }

    fn halation_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        let halation = &mut self.preset.effects.halation;

        ui.horizontal(|ui| {
            ui.label("Mode");
            ui.selectable_value(&mut self.control_mode, ControlMode::Simple, "Simple");
            ui.selectable_value(&mut self.control_mode, ControlMode::Complex, "Complex");
        });

        egui::ComboBox::from_label("Reference")
            .selected_text(halation_reference_label(halation.reference))
            .show_ui(ui, |ui| {
                for ref_preset in [
                    HalationReferencePreset::Vision3PublicCalibV1,
                    HalationReferencePreset::Vision3Subtle,
                    HalationReferencePreset::Vision3Strong,
                ] {
                    changed |= ui
                        .selectable_value(
                            &mut halation.reference,
                            ref_preset,
                            halation_reference_label(ref_preset),
                        )
                        .changed();
                }
            });

        match self.control_mode {
            ControlMode::Simple => {
                changed |= ui
                    .add(egui::Slider::new(&mut halation.intensity, 0.0..=3.0).text("halation"))
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut halation.tail_radius, 0.0..=120.0)
                            .step_by(1.0)
                            .text("halo size"),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut halation.threshold, 0.0..=4.0)
                            .text("highlight pickup"),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut halation.soft_clip, 0.0..=2.0)
                            .text("highlight rolloff"),
                    )
                    .changed();
            }
            ControlMode::Complex => {
                changed |= ui
                    .add(egui::Slider::new(&mut halation.threshold, 0.0..=4.0).text("threshold"))
                    .changed();
                changed |= ui
                    .add(egui::Slider::new(&mut halation.knee, 0.0..=1.5).text("knee"))
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut halation.core_radius, 0.0..=48.0)
                            .step_by(1.0)
                            .text("core radius px"),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut halation.tail_radius, 0.0..=140.0)
                            .step_by(1.0)
                            .text("tail radius px"),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut halation.core_weight, 0.0..=2.5).text("core weight"),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut halation.tail_weight, 0.0..=2.5).text("tail weight"),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut halation.absorption_rgb[0], 0.0..=4.0)
                            .text("absorption R"),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut halation.absorption_rgb[1], 0.0..=4.0)
                            .text("absorption G"),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut halation.absorption_rgb[2], 0.0..=4.0)
                            .text("absorption B"),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut halation.optical_depth, 0.0..=3.0)
                            .text("optical depth"),
                    )
                    .changed();
                changed |= ui
                    .add(egui::Slider::new(&mut halation.intensity, 0.0..=3.0).text("intensity"))
                    .changed();
                changed |= ui
                    .add(egui::Slider::new(&mut halation.soft_clip, 0.0..=2.0).text("soft clip"))
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut halation.chroma_bias[0], -0.5..=1.2)
                            .text("chroma bias R"),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut halation.chroma_bias[1], -0.5..=1.2)
                            .text("chroma bias G"),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::Slider::new(&mut halation.chroma_bias[2], -0.5..=1.2)
                            .text("chroma bias B"),
                    )
                    .changed();
            }
        }

        changed
    }

    fn compression_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        let compression = &mut self.preset.effects.film_compression;
        changed |= ui
            .add(egui::Slider::new(&mut compression.impact, 0.0..=1.0).text("impact"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut compression.white_point, 0.25..=2.5).text("white point"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut compression.tonal_range, 0.0..=1.0).text("tonal range"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut compression.color_density, 0.0..=2.0).text("color density"))
            .changed();
        changed
    }

    fn grain_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        let grain = &mut self.preset.effects.grain;
        changed |= ui
            .add(egui::Slider::new(&mut grain.size_px, 0.25..=4.0).text("size px"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut grain.amount, 0.0..=1.0).text("amount"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut grain.shadows, 0.0..=2.5).text("shadows"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut grain.midtones, 0.0..=2.5).text("midtones"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut grain.highlights, 0.0..=2.5).text("highlights"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut grain.chroma, 0.0..=1.0).text("chroma"))
            .changed();

        let seed_resp = ui.add(
            egui::DragValue::new(&mut grain.seed)
                .speed(1.0)
                .prefix("seed "),
        );
        changed |= seed_resp.changed();

        changed
    }

    fn finish_ui(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        let finish = &mut self.preset.effects.finish;
        ui.label("Legacy controls mapped to halation internals for compatibility.");
        changed |= ui
            .add(
                egui::Slider::new(&mut finish.legacy_grain_amount, 0.0..=0.25)
                    .text("legacy grain amount"),
            )
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut finish.legacy_grain_size, 1.0..=8.0)
                    .step_by(1.0)
                    .text("legacy grain size"),
            )
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut finish.haze_strength, 0.0..=2.0).text("haze"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut finish.lens_distortion, -0.5..=0.5).text("lens distortion"))
            .changed();
        changed
    }

    fn preview_ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Preview");

        let Some(source) = &self.source else {
            ui.label("Load or drop an image to start.");
            return;
        };

        ui.label(format!(
            "Source: {} ({}x{})",
            source.name,
            source.rgba.width(),
            source.rgba.height()
        ));

        if let Some(texture) = &self.preview_texture {
            let available = ui.available_size();
            let size = fit_size(self.preview_size, available);
            ui.image((texture.id(), size));
        } else {
            ui.label("No processed frame yet.");
        }
    }

    fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let dropped_files = ctx.input(|i| i.raw.dropped_files.clone());
        for dropped in dropped_files {
            if let Some(bytes) = dropped.bytes {
                let name = if dropped.name.is_empty() {
                    "dropped-file".to_owned()
                } else {
                    dropped.name
                };

                if name.ends_with(".json") {
                    if let Ok(text) = std::str::from_utf8(bytes.as_ref()) {
                        match FilmPresetV1::from_json_str(text) {
                            Ok(preset) => {
                                self.preset = preset;
                                self.selected_library_preset = self.find_library_match();
                                self.status = "Preset loaded from dropped JSON.".to_owned();
                                self.dirty = true;
                            }
                            Err(err) => {
                                self.status = format!("Dropped preset invalid: {err:#}");
                            }
                        }
                        return;
                    }
                }

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
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PNG", &["png"])
            .set_file_name("film-stack-output.png")
            .save_file()
        else {
            return Ok(());
        };

        let rendered = if let Some(processed) = &self.processed {
            processed.clone()
        } else if let Some(source) = &self.source {
            FilmPipeline::render(&RenderRequest {
                input: &source.rgba,
                preset: &self.preset,
                render_backend: self.render_backend,
            })?
        } else {
            self.status = "No source image loaded.".to_owned();
            return Ok(());
        };

        rendered
            .save(&path)
            .with_context(|| format!("failed to save {}", path.display()))?;

        self.status = format!("Saved {}", path.display());
        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn load_preset_from_path(&mut self, path: &Path) -> Result<()> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read preset {}", path.display()))?;
        self.preset = FilmPresetV1::from_json_str(&text)?;
        self.selected_library_preset = self.find_library_match();
        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn save_preset_to_path_dialog(&self) -> Result<()> {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("JSON", &["json"])
            .set_file_name("film-preset.json")
            .save_file()
        else {
            return Ok(());
        };

        let json = self.preset.to_json_pretty()?;
        std::fs::write(&path, json)
            .with_context(|| format!("failed to write preset {}", path.display()))
    }

    fn current_library_label(&self) -> String {
        if let Some(idx) = self.selected_library_preset {
            if let Some(preset) = self.preset_library.get(idx) {
                return preset.name.clone();
            }
        }
        "Custom".to_owned()
    }

    fn find_library_match(&self) -> Option<usize> {
        self.preset_library.iter().position(|preset| {
            preset.effects == self.preset.effects && preset.name == self.preset.name
        })
    }

    #[cfg(target_arch = "wasm32")]
    fn restore_preset_from_browser_storage(&mut self) {
        let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) else {
            return;
        };

        let Ok(Some(raw)) = storage.get_item(PRESET_STORAGE_KEY) else {
            return;
        };

        match FilmPresetV1::from_json_str(&raw) {
            Ok(preset) => {
                self.preset = preset;
                self.selected_library_preset = self.find_library_match();
                self.status = "Preset restored from browser storage.".to_owned();
            }
            Err(err) => {
                self.status = format!("Stored preset ignored: {err:#}");
            }
        }
    }

    #[cfg(target_arch = "wasm32")]
    fn persist_preset_to_browser_storage(&self) {
        let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) else {
            return;
        };

        let Ok(raw) = self.preset.to_json_pretty() else {
            return;
        };

        let _ = storage.set_item(PRESET_STORAGE_KEY, &raw);
    }
}

impl eframe::App for HalationApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_dropped_files(ui.ctx());
        self.process_if_needed();

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
            self.preset.effects = self.preset.effects.validated();
            self.selected_library_preset = self.find_library_match();
            self.dirty = true;

            #[cfg(target_arch = "wasm32")]
            self.persist_preset_to_browser_storage();

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

fn halation_reference_label(preset: HalationReferencePreset) -> &'static str {
    match preset {
        HalationReferencePreset::Vision3PublicCalibV1 => "Vision3 PublicCalibV1",
        HalationReferencePreset::Vision3Subtle => "Vision3 Subtle",
        HalationReferencePreset::Vision3Strong => "Vision3 Strong",
    }
}
