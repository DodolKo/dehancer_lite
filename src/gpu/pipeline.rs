use anyhow::Result;
use bytemuck::{Pod, Zeroable};
use eframe::{egui, egui_wgpu, wgpu};
use image::RgbaImage;

use crate::halation::HalationParams;

const WORKGROUP_SIZE_X: u32 = 8;
const WORKGROUP_SIZE_Y: u32 = 8;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct HalationUniform {
    threshold: f32,
    knee: f32,
    intensity: f32,
    soft_clip: f32,
    core_weight: f32,
    tail_weight: f32,
    optical_depth: f32,
    _pad0: f32,
    absorption: [f32; 4],
    chroma_bias: [f32; 4],
    finish: [f32; 4],
}

impl From<HalationParams> for HalationUniform {
    fn from(params: HalationParams) -> Self {
        Self {
            threshold: params.threshold,
            knee: params.knee,
            intensity: params.intensity,
            soft_clip: params.soft_clip,
            core_weight: params.core_weight,
            tail_weight: params.tail_weight,
            optical_depth: params.optical_depth,
            _pad0: 0.0,
            absorption: [
                params.absorption_rgb[0],
                params.absorption_rgb[1],
                params.absorption_rgb[2],
                0.0,
            ],
            chroma_bias: [
                params.chroma_bias[0],
                params.chroma_bias[1],
                params.chroma_bias[2],
                0.0,
            ],
            finish: [
                params.grain_amount,
                params.grain_size,
                params.haze_strength,
                params.lens_distortion,
            ],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct BlurUniform {
    direction: [f32; 2],
    radius: f32,
    sigma: f32,
}

struct Texture2d {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

impl Texture2d {
    fn new(
        device: &wgpu::Device,
        label: &str,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        usage: wgpu::TextureUsages,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self { texture, view }
    }
}

struct RenderTargets {
    source: Texture2d,
    extract: Texture2d,
    core_ping: Texture2d,
    core: Texture2d,
    tail_ping: Texture2d,
    tail: Texture2d,
    output: Texture2d,
}

pub struct GpuHalationProcessor {
    width: u32,
    height: u32,

    extract_layout: wgpu::BindGroupLayout,
    blur_layout: wgpu::BindGroupLayout,
    composite_layout: wgpu::BindGroupLayout,

    extract_pipeline: wgpu::ComputePipeline,
    blur_pipeline: wgpu::ComputePipeline,
    composite_pipeline: wgpu::ComputePipeline,

    params_buffer: wgpu::Buffer,
    blur_buffers: [wgpu::Buffer; 4],

    targets: RenderTargets,
    preview_texture_id: Option<egui::TextureId>,
}

impl GpuHalationProcessor {
    pub fn new(render_state: &egui_wgpu::RenderState, width: u32, height: u32) -> Self {
        let device = &render_state.device;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("halation-compute-shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("halation.wgsl").into()),
        });

        let extract_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("halation-extract-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let blur_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("halation-blur-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba16Float,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let composite_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("halation-composite-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::StorageTexture {
                        access: wgpu::StorageTextureAccess::WriteOnly,
                        format: wgpu::TextureFormat::Rgba8Unorm,
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::COMPUTE,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        let extract_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("halation-extract-pipeline-layout"),
                bind_group_layouts: &[Some(&extract_layout)],
                immediate_size: 0,
            });

        let blur_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("halation-blur-pipeline-layout"),
            bind_group_layouts: &[Some(&blur_layout)],
            immediate_size: 0,
        });

        let composite_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("halation-composite-pipeline-layout"),
                bind_group_layouts: &[Some(&composite_layout)],
                immediate_size: 0,
            });

        let extract_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("halation-extract-pipeline"),
            layout: Some(&extract_pipeline_layout),
            module: &shader,
            entry_point: Some("extract_highlights"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let blur_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("halation-blur-pipeline"),
            layout: Some(&blur_pipeline_layout),
            module: &shader,
            entry_point: Some("blur_pass"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let composite_pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("halation-composite-pipeline"),
            layout: Some(&composite_pipeline_layout),
            module: &shader,
            entry_point: Some("composite_pass"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        });

        let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("halation-params-buffer"),
            size: std::mem::size_of::<HalationUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let blur_buffers = [
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("halation-blur-buffer-core-h"),
                size: std::mem::size_of::<BlurUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("halation-blur-buffer-core-v"),
                size: std::mem::size_of::<BlurUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("halation-blur-buffer-tail-h"),
                size: std::mem::size_of::<BlurUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("halation-blur-buffer-tail-v"),
                size: std::mem::size_of::<BlurUniform>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        ];

        let (width, height) = (width.max(1), height.max(1));
        let targets = Self::create_targets(device, width, height);

        Self {
            width,
            height,
            extract_layout,
            blur_layout,
            composite_layout,
            extract_pipeline,
            blur_pipeline,
            composite_pipeline,
            params_buffer,
            blur_buffers,
            targets,
            preview_texture_id: None,
        }
    }

    pub fn output_size(&self) -> [usize; 2] {
        [self.width as usize, self.height as usize]
    }

    pub fn process(
        &mut self,
        render_state: &egui_wgpu::RenderState,
        source: &RgbaImage,
        params: HalationParams,
    ) -> Result<egui::TextureId> {
        let width = source.width().max(1);
        let height = source.height().max(1);

        if width != self.width || height != self.height {
            self.width = width;
            self.height = height;
            self.targets = Self::create_targets(&render_state.device, width, height);
        }

        let params = params.validated();
        render_state.queue.write_buffer(
            &self.params_buffer,
            0,
            bytemuck::bytes_of(&HalationUniform::from(params)),
        );

        let core_radius = params.core_radius_px() as f32;
        let tail_radius = params.tail_radius_px() as f32;

        let blur_uniforms = [
            BlurUniform {
                direction: [1.0, 0.0],
                radius: core_radius,
                sigma: radius_to_sigma(core_radius),
            },
            BlurUniform {
                direction: [0.0, 1.0],
                radius: core_radius,
                sigma: radius_to_sigma(core_radius),
            },
            BlurUniform {
                direction: [1.0, 0.0],
                radius: tail_radius,
                sigma: radius_to_sigma(tail_radius),
            },
            BlurUniform {
                direction: [0.0, 1.0],
                radius: tail_radius,
                sigma: radius_to_sigma(tail_radius),
            },
        ];

        for (buffer, uniform) in self.blur_buffers.iter().zip(blur_uniforms.iter()) {
            render_state
                .queue
                .write_buffer(buffer, 0, bytemuck::bytes_of(uniform));
        }

        render_state.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.targets.source.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            source.as_raw(),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4 * width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        let extract_bg = render_state
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("halation-extract-bg"),
                layout: &self.extract_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.targets.source.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&self.targets.extract.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: self.params_buffer.as_entire_binding(),
                    },
                ],
            });

        let core_h_bg = self.create_blur_bg(
            &render_state.device,
            &self.targets.extract.view,
            &self.targets.core_ping.view,
            &self.blur_buffers[0],
            "halation-core-h-bg",
        );
        let core_v_bg = self.create_blur_bg(
            &render_state.device,
            &self.targets.core_ping.view,
            &self.targets.core.view,
            &self.blur_buffers[1],
            "halation-core-v-bg",
        );
        let tail_h_bg = self.create_blur_bg(
            &render_state.device,
            &self.targets.extract.view,
            &self.targets.tail_ping.view,
            &self.blur_buffers[2],
            "halation-tail-h-bg",
        );
        let tail_v_bg = self.create_blur_bg(
            &render_state.device,
            &self.targets.tail_ping.view,
            &self.targets.tail.view,
            &self.blur_buffers[3],
            "halation-tail-v-bg",
        );

        let composite_bg = render_state
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("halation-composite-bg"),
                layout: &self.composite_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&self.targets.source.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&self.targets.core.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&self.targets.tail.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::TextureView(&self.targets.output.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: self.params_buffer.as_entire_binding(),
                    },
                ],
            });

        let dispatch_x = width.div_ceil(WORKGROUP_SIZE_X);
        let dispatch_y = height.div_ceil(WORKGROUP_SIZE_Y);

        let mut encoder =
            render_state
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("halation-command-encoder"),
                });

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("halation-extract-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.extract_pipeline);
            pass.set_bind_group(0, &extract_bg, &[]);
            pass.dispatch_workgroups(dispatch_x, dispatch_y, 1);
        }

        for bind_group in [&core_h_bg, &core_v_bg, &tail_h_bg, &tail_v_bg] {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("halation-blur-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.blur_pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.dispatch_workgroups(dispatch_x, dispatch_y, 1);
        }

        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("halation-composite-pass"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.composite_pipeline);
            pass.set_bind_group(0, &composite_bg, &[]);
            pass.dispatch_workgroups(dispatch_x, dispatch_y, 1);
        }

        render_state.queue.submit(Some(encoder.finish()));

        let mut renderer = render_state.renderer.write();
        let texture_id = if let Some(texture_id) = self.preview_texture_id {
            renderer.update_egui_texture_from_wgpu_texture(
                &render_state.device,
                &self.targets.output.view,
                wgpu::FilterMode::Linear,
                texture_id,
            );
            texture_id
        } else {
            let texture_id = renderer.register_native_texture(
                &render_state.device,
                &self.targets.output.view,
                wgpu::FilterMode::Linear,
            );
            self.preview_texture_id = Some(texture_id);
            texture_id
        };

        Ok(texture_id)
    }

    fn create_blur_bg(
        &self,
        device: &wgpu::Device,
        input: &wgpu::TextureView,
        output: &wgpu::TextureView,
        blur_uniform: &wgpu::Buffer,
        label: &str,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(label),
            layout: &self.blur_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(input),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(output),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: blur_uniform.as_entire_binding(),
                },
            ],
        })
    }

    fn create_targets(device: &wgpu::Device, width: u32, height: u32) -> RenderTargets {
        let sampled_storage_usage =
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::STORAGE_BINDING;

        RenderTargets {
            source: Texture2d::new(
                device,
                "halation-source",
                width,
                height,
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            ),
            extract: Texture2d::new(
                device,
                "halation-extract",
                width,
                height,
                wgpu::TextureFormat::Rgba16Float,
                sampled_storage_usage,
            ),
            core_ping: Texture2d::new(
                device,
                "halation-core-ping",
                width,
                height,
                wgpu::TextureFormat::Rgba16Float,
                sampled_storage_usage,
            ),
            core: Texture2d::new(
                device,
                "halation-core",
                width,
                height,
                wgpu::TextureFormat::Rgba16Float,
                sampled_storage_usage,
            ),
            tail_ping: Texture2d::new(
                device,
                "halation-tail-ping",
                width,
                height,
                wgpu::TextureFormat::Rgba16Float,
                sampled_storage_usage,
            ),
            tail: Texture2d::new(
                device,
                "halation-tail",
                width,
                height,
                wgpu::TextureFormat::Rgba16Float,
                sampled_storage_usage,
            ),
            output: Texture2d::new(
                device,
                "halation-output",
                width,
                height,
                wgpu::TextureFormat::Rgba8Unorm,
                wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
            ),
        }
    }
}

fn radius_to_sigma(radius: f32) -> f32 {
    if radius <= 0.0 {
        1.0
    } else {
        (radius * 0.5).max(1.0)
    }
}
