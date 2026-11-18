#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FadeBlend {
    Add,
    Subtract,
    Saturate,
    Multiply,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenFade {
    pub blend: FadeBlend,
    pub value: f32,
}

impl FadeBlend {
    const ALL: [Self; 4] = [Self::Add, Self::Subtract, Self::Saturate, Self::Multiply];

    fn index(self) -> usize {
        match self {
            Self::Add => 0,
            Self::Subtract => 1,
            Self::Saturate => 2,
            Self::Multiply => 3,
        }
    }

    fn state(self) -> wgpu::BlendState {
        use wgpu::{BlendComponent, BlendFactor as F, BlendOperation as Op};
        let component = match self {
            Self::Add => BlendComponent {
                src_factor: F::One,
                dst_factor: F::One,
                operation: Op::Add,
            },
            Self::Subtract => BlendComponent {
                src_factor: F::One,
                dst_factor: F::One,
                operation: Op::ReverseSubtract,
            },
            Self::Saturate => BlendComponent {
                src_factor: F::Dst,
                dst_factor: F::Src,
                operation: Op::Add,
            },
            Self::Multiply => BlendComponent {
                src_factor: F::Zero,
                dst_factor: F::Src,
                operation: Op::Add,
            },
        };
        wgpu::BlendState {
            color: component,
            alpha: BlendComponent::REPLACE,
        }
    }

    fn draws(self) -> u32 {
        if self == Self::Saturate {
            2
        } else {
            1
        }
    }
}

pub(crate) struct FadePass {
    pipelines: [wgpu::RenderPipeline; 4],
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl FadePass {
    pub(crate) fn new(
        device: &wgpu::Device,
        cache: Option<&wgpu::PipelineCache>,
        color_format: wgpu::TextureFormat,
        depth_stencil: &impl Fn() -> Option<wgpu::DepthStencilState>,
    ) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mint fade colour"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mint fade layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mint fade group"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mint fade pipeline layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mint fade"),
            source: wgpu::ShaderSource::Wgsl(include_str!("fade.wgsl").into()),
        });
        let pipelines = FadeBlend::ALL.map(|blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("mint fade"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: depth_stencil(),
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: color_format,
                        blend: Some(blend.state()),
                        write_mask: wgpu::ColorWrites::COLOR,
                    })],
                }),
                multiview_mask: None,
                cache,
            })
        });
        Self {
            pipelines,
            buffer,
            bind_group,
        }
    }

    pub(crate) fn write(&self, queue: &wgpu::Queue, fade: &ScreenFade) {
        let grey = fade.value.clamp(0.0, 1.0);
        let mut bytes = [0u8; 16];
        for (chunk, value) in bytes.chunks_exact_mut(4).zip([grey, grey, grey, 1.0]) {
            chunk.copy_from_slice(&value.to_le_bytes());
        }
        queue.write_buffer(&self.buffer, 0, &bytes);
    }

    pub(crate) fn record(&self, pass: &mut wgpu::RenderPass<'_>, blend: FadeBlend) -> u32 {
        let Some(pipeline) = self.pipelines.get(blend.index()) else {
            return 0;
        };
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        for _ in 0..blend.draws() {
            pass.draw(0..3, 0..1);
        }
        blend.draws()
    }
}
