use crate::{Error, Result};

const TAPS_PER_GROUP: usize = 4;
const MAX_GROUPS: usize = 16;
const SLOT_COUNT: usize = 3 + 2 * MAX_GROUPS;
const SLOT_STRIDE: u64 = 256;
const PARAMS_SIZE: u64 = 96;
const MAX_GLOW_SIZE: u32 = 2048;
const MAX_VOLUME_SIZE: u32 = 64;
const GLOW_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GlowTap {
    pub offset: f32,
    pub weight: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct Glow<'a> {
    pub bright_offset: [f32; 3],
    pub texture_size: u32,
    pub taps: &'a [GlowTap],
    pub intensity: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ColorVolumeHandle(pub(crate) usize);

#[derive(Debug, Clone, Copy)]
pub struct ColorLut {
    pub volume: ColorVolumeHandle,
    pub blend: f32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PostEffects<'a> {
    pub glow: Option<Glow<'a>>,
    pub lut: Option<ColorLut>,
}

struct Pipelines {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    copy: wgpu::RenderPipeline,
    bright: wgpu::RenderPipeline,
    blur_replace: wgpu::RenderPipeline,
    blur_add: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    lut: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    placeholder_volume: wgpu::TextureView,
    format: wgpu::TextureFormat,
}

struct FrameCopy {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
}

struct GlowTargets {
    size: u32,
    g: wgpu::TextureView,
    c: wgpu::TextureView,
}

#[derive(Default)]
pub(crate) struct PostState {
    pipelines: Option<Pipelines>,
    frame: Option<FrameCopy>,
    glow: Option<GlowTargets>,
    volumes: Vec<wgpu::TextureView>,
}

#[derive(Clone, Copy)]
enum Step {
    Copy,
    Bright,
    BlurReplace,
    BlurAdd,
    Composite,
    Lut,
}

#[derive(Clone, Copy, Default)]
struct Params {
    offsets: [[f32; 4]; 4],
    weights: [f32; 4],
    c0: [f32; 4],
}

impl Params {
    fn bytes(&self) -> Vec<u8> {
        self.offsets
            .iter()
            .flatten()
            .chain(&self.weights)
            .chain(&self.c0)
            .flat_map(|v| v.to_le_bytes())
            .collect()
    }
}

struct Draw<'a> {
    step: Step,
    slot: usize,
    params: Params,
    source: &'a wgpu::TextureView,
    target: &'a wgpu::TextureView,
    volume: Option<&'a wgpu::TextureView>,
}

impl PostState {
    pub(crate) fn upload_volume(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        edge: u32,
        rgba: &[u8],
    ) -> Result<ColorVolumeHandle> {
        let cells = usize::try_from(edge).map_err(|_| Error::EmptyTexture)?;
        let expected = cells
            .checked_pow(3)
            .and_then(|n| n.checked_mul(4))
            .ok_or(Error::EmptyTexture)?;
        if edge == 0 || edge > MAX_VOLUME_SIZE || rgba.len() != expected {
            return Err(Error::EmptyTexture);
        }
        let extent = wgpu::Extent3d {
            width: edge,
            height: edge,
            depth_or_array_layers: edge,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mint colour volume"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(edge.saturating_mul(4)),
                rows_per_image: Some(edge),
            },
            extent,
        );
        self.volumes
            .push(texture.create_view(&wgpu::TextureViewDescriptor::default()));
        Ok(ColorVolumeHandle(self.volumes.len().saturating_sub(1)))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn record(
        &mut self,
        device: &wgpu::Device,
        cache: Option<&wgpu::PipelineCache>,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::Texture,
        target: &wgpu::TextureView,
        color_format: wgpu::TextureFormat,
        (width, height): (u32, u32),
        post: &PostEffects<'_>,
    ) -> Result<()> {
        let lut = match post.lut {
            Some(lut) => Some((
                self.volumes
                    .get(lut.volume.0)
                    .ok_or(Error::UnknownHandle)?
                    .clone(),
                lut.blend,
            )),
            None => None,
        };
        let glow = if lut.is_some() {
            None
        } else {
            post.glow
                .filter(|g| g.texture_size > 0 && g.texture_size <= MAX_GLOW_SIZE)
                .filter(|g| !g.taps.is_empty())
        };
        if lut.is_none() && glow.is_none() {
            return Ok(());
        }
        if self
            .pipelines
            .as_ref()
            .is_none_or(|p| p.format != color_format)
        {
            self.pipelines = Some(Pipelines::new(device, cache, color_format));
        }
        if self
            .frame
            .as_ref()
            .is_none_or(|f| f.width != width || f.height != height)
        {
            self.frame = Some(FrameCopy::new(device, color_format, width, height));
        }
        if let Some(glow) = &glow {
            if self
                .glow
                .as_ref()
                .is_none_or(|t| t.size != glow.texture_size)
            {
                self.glow = Some(GlowTargets::new(device, glow.texture_size));
            }
        }
        let (Some(pipelines), Some(frame)) = (&self.pipelines, &self.frame) else {
            return Ok(());
        };
        encoder.copy_texture_to_texture(
            wgpu::TexelCopyTextureInfo {
                texture: source,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyTextureInfo {
                texture: &frame.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );

        let mut draws: Vec<Draw<'_>> = Vec::new();
        if let Some((volume, blend)) = &lut {
            let params = Params {
                c0: [*blend, 0.0, 0.0, 0.0],
                ..Params::default()
            };
            draws.push(Draw {
                step: Step::Lut,
                slot: 0,
                params,
                source: &frame.view,
                target,
                volume: Some(volume),
            });
        } else if let (Some(glow), Some(targets)) = (glow, &self.glow) {
            glow_draws(&mut draws, &glow, &frame.view, targets, target);
        }

        for draw in &draws {
            let offset = (draw.slot as u64).saturating_mul(SLOT_STRIDE);
            queue.write_buffer(&pipelines.uniforms, offset, &draw.params.bytes());
        }
        for draw in &draws {
            pipelines.draw(device, encoder, draw);
        }
        Ok(())
    }
}

fn glow_draws<'a>(
    draws: &mut Vec<Draw<'a>>,
    glow: &Glow<'_>,
    frame: &'a wgpu::TextureView,
    targets: &'a GlowTargets,
    target: &'a wgpu::TextureView,
) {
    let size = glow.texture_size as f32;
    let mut add = |step, params, source, target| {
        let slot = draws.len();
        draws.push(Draw {
            step,
            slot,
            params,
            source,
            target,
            volume: None,
        });
    };
    add(Step::Copy, Params::default(), frame, &targets.c);
    let [r, g, b] = glow.bright_offset;
    let bright = Params {
        c0: [r, g, b, 0.0],
        ..Params::default()
    };
    add(Step::Bright, bright, &targets.c, &targets.g);
    for (horizontal, source, destination) in [
        (true, &targets.g, &targets.c),
        (false, &targets.c, &targets.g),
    ] {
        for (group, taps) in glow
            .taps
            .chunks(TAPS_PER_GROUP)
            .take(MAX_GROUPS)
            .enumerate()
        {
            let mut params = Params::default();
            for ((offset, weight), tap) in params
                .offsets
                .iter_mut()
                .zip(params.weights.iter_mut())
                .zip(taps)
            {
                let shift = tap.offset / size;
                *offset = if horizontal {
                    [shift, 0.0, 0.0, 0.0]
                } else {
                    [0.0, shift, 0.0, 0.0]
                };
                *weight = tap.weight * glow.intensity;
            }
            let step = if group == 0 {
                Step::BlurReplace
            } else {
                Step::BlurAdd
            };
            add(step, params, source, destination);
        }
    }
    add(Step::Composite, Params::default(), &targets.g, target);
}

impl FrameCopy {
    fn new(device: &wgpu::Device, format: wgpu::TextureFormat, width: u32, height: u32) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("mint post frame copy"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: format.remove_srgb_suffix(),
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            texture,
            view,
            width,
            height,
        }
    }
}

impl GlowTargets {
    fn new(device: &wgpu::Device, size: u32) -> Self {
        let make = |label| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d {
                        width: size,
                        height: size,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: GLOW_FORMAT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        Self {
            size,
            g: make("mint glow G"),
            c: make("mint glow C"),
        }
    }
}

fn layout_entry(binding: u32, ty: wgpu::BindingType) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty,
        count: None,
    }
}

fn texture_binding(dimension: wgpu::TextureViewDimension) -> wgpu::BindingType {
    wgpu::BindingType::Texture {
        sample_type: wgpu::TextureSampleType::Float { filterable: true },
        view_dimension: dimension,
        multisampled: false,
    }
}

impl Pipelines {
    fn new(
        device: &wgpu::Device,
        cache: Option<&wgpu::PipelineCache>,
        format: wgpu::TextureFormat,
    ) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mint post"),
            source: wgpu::ShaderSource::Wgsl(include_str!("post.wgsl").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mint post"),
            entries: &[
                layout_entry(
                    0,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(PARAMS_SIZE),
                    },
                ),
                layout_entry(1, texture_binding(wgpu::TextureViewDimension::D2)),
                layout_entry(
                    2,
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                ),
                layout_entry(3, texture_binding(wgpu::TextureViewDimension::D3)),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mint post"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry: &'static str,
                        target: wgpu::TextureFormat,
                        blend: Option<wgpu::BlendState>,
                        mask: wgpu::ColorWrites| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs_post"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some(entry),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: target,
                        blend,
                        write_mask: mask,
                    })],
                }),
                multiview_mask: None,
                cache,
            })
        };
        let one_inv_src_color = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrc,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
        };
        let all = wgpu::ColorWrites::ALL;
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("mint post"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            ..Default::default()
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mint post params"),
            size: SLOT_STRIDE.saturating_mul(SLOT_COUNT as u64),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let placeholder_volume = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("mint post placeholder volume"),
                size: wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D3,
                format: wgpu::TextureFormat::Rgba8Unorm,
                usage: wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&wgpu::TextureViewDescriptor::default());
        Self {
            copy: pipeline("fs_copy", GLOW_FORMAT, None, all),
            bright: pipeline("fs_bright", GLOW_FORMAT, None, all),
            blur_replace: pipeline("fs_blur", GLOW_FORMAT, None, all),
            blur_add: pipeline("fs_blur", GLOW_FORMAT, Some(one_inv_src_color), all),
            composite: pipeline(
                "fs_copy",
                format,
                Some(one_inv_src_color),
                wgpu::ColorWrites::COLOR,
            ),
            lut: pipeline("fs_lut", format, None, all),
            layout,
            sampler,
            uniforms,
            placeholder_volume,
            format,
        }
    }

    fn draw(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, draw: &Draw<'_>) {
        let pipeline = match draw.step {
            Step::Copy => &self.copy,
            Step::Bright => &self.bright,
            Step::BlurReplace => &self.blur_replace,
            Step::BlurAdd => &self.blur_add,
            Step::Composite => &self.composite,
            Step::Lut => &self.lut,
        };
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mint post"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &self.uniforms,
                        offset: 0,
                        size: wgpu::BufferSize::new(PARAMS_SIZE),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(draw.source),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(
                        draw.volume.unwrap_or(&self.placeholder_volume),
                    ),
                },
            ],
        });
        let dynamic_offset =
            u32::try_from((draw.slot as u64).saturating_mul(SLOT_STRIDE)).unwrap_or(0);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("mint post pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: draw.target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[dynamic_offset]);
        pass.draw(0..3, 0..1);
    }
}
