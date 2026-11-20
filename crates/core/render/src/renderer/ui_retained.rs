use std::collections::HashMap;

use super::{
    content_stencil_state, screen_stencil_state, wgpu_blend_state, Blend, Error, MaterialSlot,
    Renderer, Result, UiGeometry,
};

const VERTEX_SIZE: u64 = 8;

const INSTANCE_SIZE: u64 = 112;

const FIRST_ARENA_VERTICES: usize = 1 << 15;

const ARENA_LIMIT_VERTICES: usize = 1 << 22;

const FIRST_INSTANCES: u32 = 64;

const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 1] = [wgpu::VertexAttribute {
    format: wgpu::VertexFormat::Float32x2,
    offset: 0,
    shader_location: 0,
}];

#[derive(Clone, Copy)]
struct Shape {
    triangles: (u32, u32),
    lines: (u32, u32),
    triangle_len: usize,
    line_len: usize,
}

impl Shape {
    fn matches(&self, geometry: &UiGeometry<'_>) -> bool {
        self.triangle_len == geometry.triangles.len() && self.line_len == geometry.lines.len()
    }
}

#[derive(Clone, Copy)]
pub(super) struct RetainedRun {
    triangles: Option<(u32, u32)>,
    lines: Option<(u32, u32)>,
    material: Option<MaterialSlot>,
    instance: u32,
}

impl RetainedRun {
    pub(super) fn is_empty(&self) -> bool {
        self.triangles.is_none() && self.lines.is_none()
    }
}

pub(super) struct RetainedUi {
    layout: wgpu::BindGroupLayout,
    solid_triangles: wgpu::RenderPipeline,
    solid_lines: wgpu::RenderPipeline,
    textured: wgpu::RenderPipeline,
    arena: wgpu::Buffer,
    arena_capacity: usize,
    arena_used: usize,
    shapes: HashMap<u64, Shape>,
    instances: wgpu::Buffer,
    instance_capacity: u32,
    bind_group: wgpu::BindGroup,
    vertex_scratch: Vec<u8>,
    instance_scratch: Vec<u8>,
}

impl RetainedUi {
    pub(super) fn new(
        device: &wgpu::Device,
        viewport_layout: &wgpu::BindGroupLayout,
        material_layout: &wgpu::BindGroupLayout,
        color_format: wgpu::TextureFormat,
        cache: Option<&wgpu::PipelineCache>,
    ) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mint ui retained instances"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(INSTANCE_SIZE),
                },
                count: None,
            }],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mint ui retained"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../ui_retained.wgsl").into()),
        });
        let solid_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mint ui retained solid layout"),
            bind_group_layouts: &[Some(viewport_layout), Some(&layout)],
            immediate_size: 0,
        });
        let textured_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mint ui retained textured layout"),
            bind_group_layouts: &[Some(viewport_layout), Some(material_layout), Some(&layout)],
            immediate_size: 0,
        });
        let buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: VERTEX_SIZE,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &VERTEX_ATTRIBUTES,
        })];
        let build = |label: &str,
                     pipeline_layout: &wgpu::PipelineLayout,
                     topology: wgpu::PrimitiveTopology,
                     vertex: &str,
                     fragment: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vertex),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &buffers,
                },
                primitive: wgpu::PrimitiveState {
                    topology,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: screen_stencil_state(content_stencil_state()),
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: color_format,
                        blend: wgpu_blend_state(Blend::AlphaBlend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache,
            })
        };
        let solid_triangles = build(
            "mint ui retained (triangles)",
            &solid_layout,
            wgpu::PrimitiveTopology::TriangleList,
            "vs_solid",
            "fs_solid",
        );
        let solid_lines = build(
            "mint ui retained (lines)",
            &solid_layout,
            wgpu::PrimitiveTopology::LineList,
            "vs_solid",
            "fs_solid",
        );
        let textured = build(
            "mint ui retained (textured triangles)",
            &textured_layout,
            wgpu::PrimitiveTopology::TriangleList,
            "vs_textured",
            "fs_textured",
        );
        let instances = create_instance_buffer(device, FIRST_INSTANCES);
        let bind_group = create_instance_bind_group(device, &layout, &instances);
        Self {
            arena: create_arena(device, FIRST_ARENA_VERTICES),
            arena_capacity: FIRST_ARENA_VERTICES,
            arena_used: 0,
            layout,
            solid_triangles,
            solid_lines,
            textured,
            shapes: HashMap::new(),
            instances,
            instance_capacity: FIRST_INSTANCES,
            bind_group,
            vertex_scratch: Vec::new(),
            instance_scratch: Vec::new(),
        }
    }

    pub(super) fn plan(&mut self, device: &wgpu::Device, entries: &[&UiGeometry<'_>]) -> bool {
        if !self.vertex_scratch.is_empty() {
            self.shapes.clear();
            self.arena_used = 0;
            self.vertex_scratch.clear();
        }
        self.instance_scratch.clear();
        let mut total = 0usize;
        let mut missing = 0usize;
        for geometry in entries {
            let Some(key) = geometry.retain_key else {
                continue;
            };
            let vertices = usable_vertices(geometry);
            total = total.saturating_add(vertices);
            if !self
                .shapes
                .get(&key)
                .is_some_and(|shape| shape.matches(geometry))
            {
                missing = missing.saturating_add(vertices);
            }
        }
        if total == 0 || total > ARENA_LIMIT_VERTICES {
            return false;
        }
        if self.arena_used.saturating_add(missing) > self.arena_capacity {
            let capacity = total
                .saturating_mul(2)
                .clamp(FIRST_ARENA_VERTICES, ARENA_LIMIT_VERTICES);
            self.arena = create_arena(device, capacity);
            self.arena_capacity = capacity;
            self.arena_used = 0;
            self.shapes.clear();
        }
        true
    }

    pub(super) fn add(
        &mut self,
        geometry: &UiGeometry<'_>,
        material: Option<(MaterialSlot, (u32, u32))>,
    ) -> Result<Option<RetainedRun>> {
        let Some(key) = geometry.retain_key else {
            return Ok(None);
        };
        let shape = match self
            .shapes
            .get(&key)
            .copied()
            .filter(|shape| shape.matches(geometry))
        {
            Some(shape) => shape,
            None => {
                let Some(shape) = self.insert(geometry)? else {
                    return Ok(None);
                };
                self.shapes.insert(key, shape);
                shape
            }
        };
        let instance =
            u32::try_from(self.instance_scratch.len() / INSTANCE_SIZE_USIZE).map_err(|_| {
                Error::TooManyInstances {
                    count: self.instance_scratch.len(),
                }
            })?;
        write_instance(
            geometry,
            material.map(|(_, size)| size),
            &mut self.instance_scratch,
        );
        Ok(Some(RetainedRun {
            triangles: (shape.triangles.1 > 0).then_some(shape.triangles),
            lines: (shape.lines.1 > 0).then_some(shape.lines),
            material: material.map(|(slot, _)| slot),
            instance,
        }))
    }

    fn insert(&mut self, geometry: &UiGeometry<'_>) -> Result<Option<Shape>> {
        let pending = self.vertex_scratch.len() / VERTEX_SIZE_USIZE;
        let first = self.arena_used.saturating_add(pending);
        if first.saturating_add(usable_vertices(geometry)) > self.arena_capacity {
            return Ok(None);
        }
        for corner in geometry.triangles.chunks_exact(3).flatten() {
            push_position(*corner, &mut self.vertex_scratch);
        }
        let after_triangles = self
            .arena_used
            .saturating_add(self.vertex_scratch.len() / VERTEX_SIZE_USIZE);
        for endpoint in geometry.lines.chunks_exact(2).flatten() {
            push_position(*endpoint, &mut self.vertex_scratch);
        }
        let end = self
            .arena_used
            .saturating_add(self.vertex_scratch.len() / VERTEX_SIZE_USIZE);
        Ok(Some(Shape {
            triangles: range(first, after_triangles)?,
            lines: range(after_triangles, end)?,
            triangle_len: geometry.triangles.len(),
            line_len: geometry.lines.len(),
        }))
    }

    pub(super) fn flush(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if !self.vertex_scratch.is_empty() {
            let offset = u64::try_from(self.arena_used)
                .unwrap_or(u64::MAX)
                .saturating_mul(VERTEX_SIZE);
            queue.write_buffer(&self.arena, offset, &self.vertex_scratch);
            self.arena_used = self
                .arena_used
                .saturating_add(self.vertex_scratch.len() / VERTEX_SIZE_USIZE);
            self.vertex_scratch.clear();
        }
        if self.instance_scratch.is_empty() {
            return;
        }
        let needed =
            u32::try_from(self.instance_scratch.len() / INSTANCE_SIZE_USIZE).unwrap_or(u32::MAX);
        if needed > self.instance_capacity {
            let capacity = needed.saturating_mul(2).max(FIRST_INSTANCES);
            self.instances = create_instance_buffer(device, capacity);
            self.bind_group = create_instance_bind_group(device, &self.layout, &self.instances);
            self.instance_capacity = capacity;
        }
        queue.write_buffer(&self.instances, 0, &self.instance_scratch);
    }
}

const VERTEX_SIZE_USIZE: usize = 8;
const INSTANCE_SIZE_USIZE: usize = 112;

fn usable_vertices(geometry: &UiGeometry<'_>) -> usize {
    geometry
        .triangles
        .chunks_exact(3)
        .len()
        .saturating_mul(3)
        .saturating_add(geometry.lines.chunks_exact(2).len().saturating_mul(2))
}

fn range(first: usize, end: usize) -> Result<(u32, u32)> {
    let first_u32 = u32::try_from(first).map_err(|_| Error::TooManyInstances { count: first })?;
    let end_u32 = u32::try_from(end).map_err(|_| Error::TooManyInstances { count: end })?;
    Ok((first_u32, end_u32.saturating_sub(first_u32)))
}

fn push_position(position: [f32; 2], out: &mut Vec<u8>) {
    for value in position {
        out.extend_from_slice(&value.to_le_bytes());
    }
}

fn write_instance(geometry: &UiGeometry<'_>, texture_size: Option<(u32, u32)>, out: &mut Vec<u8>) {
    let [[pm00, pm01], [pm10, pm11]] = geometry.affine;
    let [ptx, pty] = geometry.translation;
    let (uv_affine, uv_translation) = match texture_size {
        Some((width, height)) => {
            let [[tm00, tm01], [tm10, tm11]] = geometry.texture_transform;
            let [ttx, tty] = geometry.texture_translation;
            let inv_w = f64::from(width).recip();
            let inv_h = f64::from(height).recip();
            #[allow(clippy::cast_possible_truncation)]
            let (inv_w, inv_h) = (inv_w as f32, inv_h as f32);
            (
                [tm00 * inv_w, tm01 * inv_w, tm10 * inv_h, tm11 * inv_h],
                [ttx * inv_w, tty * inv_h, 0.0, 0.0],
            )
        }
        None => ([0.0; 4], [0.0; 4]),
    };
    for vector in [
        [pm00, pm01, pm10, pm11],
        [ptx, pty, 0.0, 0.0],
        uv_affine,
        uv_translation,
        geometry.color,
        geometry.color_add,
        geometry.clip,
    ] {
        for value in vector {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
}

fn create_arena(device: &wgpu::Device, vertices: usize) -> wgpu::Buffer {
    let size = u64::try_from(vertices)
        .unwrap_or(u64::MAX)
        .saturating_mul(VERTEX_SIZE);
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mint ui retained vertices"),
        size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_instance_buffer(device: &wgpu::Device, capacity: u32) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mint ui retained instances"),
        size: u64::from(capacity).saturating_mul(INSTANCE_SIZE),
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_instance_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("mint ui retained instances"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer,
                offset: 0,
                size: None,
            }),
        }],
    })
}

impl Renderer {
    pub(super) fn record_retained_run(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        run: &RetainedRun,
    ) -> u32 {
        let retained = &self.ui_retained;
        let instances = run.instance..run.instance.saturating_add(1);
        let mut draw_calls = 0u32;
        let triangles = |pass: &mut wgpu::RenderPass<'_>| -> u32 {
            let Some((first, count)) = run.triangles else {
                return 0;
            };
            match run.material {
                None => {
                    pass.set_pipeline(&retained.solid_triangles);
                    pass.set_bind_group(0, &self.ui_geometry_bind_group, &[]);
                    pass.set_bind_group(1, &retained.bind_group, &[]);
                }
                Some(material) => {
                    let Some(group) = self.material_groups.get(material) else {
                        return 0;
                    };
                    pass.set_pipeline(&retained.textured);
                    pass.set_bind_group(0, &self.ui_geometry_bind_group, &[]);
                    pass.set_bind_group(1, group, &[]);
                    pass.set_bind_group(2, &retained.bind_group, &[]);
                }
            }
            pass.set_vertex_buffer(0, retained.arena.slice(..));
            pass.draw(first..first.saturating_add(count), instances.clone());
            1
        };
        let lines = |pass: &mut wgpu::RenderPass<'_>| -> u32 {
            let Some((first, count)) = run.lines else {
                return 0;
            };
            pass.set_pipeline(&retained.solid_lines);
            pass.set_bind_group(0, &self.ui_geometry_bind_group, &[]);
            pass.set_bind_group(1, &retained.bind_group, &[]);
            pass.set_vertex_buffer(0, retained.arena.slice(..));
            pass.draw(first..first.saturating_add(count), instances.clone());
            1
        };
        if run.material.is_none() {
            draw_calls = draw_calls.saturating_add(triangles(pass));
            draw_calls = draw_calls.saturating_add(lines(pass));
        } else {
            draw_calls = draw_calls.saturating_add(lines(pass));
            draw_calls = draw_calls.saturating_add(triangles(pass));
        }
        draw_calls
    }
}
