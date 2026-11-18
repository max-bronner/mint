use std::collections::BTreeMap;
use std::future::Future;
use std::path::Path;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

use crate::blend::Blend;
use crate::camera::Camera;
use crate::custom::{
    aligned_uniform_size, CustomMaterialHandle, CustomMaterialSource, CustomShader,
    CustomShaderHandle, MAX_CUSTOM_TEXTURE_SLOTS, MAX_CUSTOM_UNIFORM_SIZE,
};
use crate::depth::{DepthDirection, DepthTest};
use crate::detail::{DetailAlphaCombine, DetailCombine};
use crate::envmap::EnvMap;
use crate::fade::{FadeBlend, FadePass, ScreenFade};
use crate::frustum::{transformed_bounds, Frustum};
use crate::math::{Mat4, Vec3};
use crate::mesh::{Aabb, MeshData, Vertex};
use crate::modulate::Modulate2x;
use crate::pipeline_cache::{self, PipelineCacheState};
use crate::post::{ColorVolumeHandle, PostEffects, PostState};
use crate::shadow::{
    ContactShadows, ShadowEdge, ShadowMapHandle, ShadowSample, MAX_SHADOW_CASCADES,
};
use crate::texture::{AddressMode, PixelFormat, TextureData};
use crate::ui::{ScreenDraw, UiGeometry, UiQuad};
use crate::{Error, Result};
use ui_retained::{RetainedRun, RetainedUi};

const BLEND_MODE_COUNT: usize = 9;

const DEPTH_TEST_COUNT: usize = 2;

const DEPTH_DIRECTION_COUNT: usize = 2;

const MERGE_PART_COUNT: usize = 2;

const GRID_BLENDS: [Blend; BLEND_MODE_COUNT] = [
    Blend::Opaque,
    Blend::AlphaBlend,
    Blend::Additive,
    Blend::AlphaAdditive,
    Blend::AlphaBlendWritesDepth,
    Blend::AddScaledByAlpha,
    Blend::AddScaledByColor,
    Blend::AddInverseScaledByColor,
    Blend::Multiply,
];

const MERGE_BLENDS: [Blend; MERGE_PART_COUNT] = [Blend::MergeMask, Blend::MergeAlphaBlend];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ColorKey {
    cull_mode: Option<wgpu::Face>,
    skinned: bool,
    direction: DepthDirection,
    depth_test: DepthTest,
    blend: Blend,
}

fn grid_keys(
    cull_mode: Option<wgpu::Face>,
    skinned: bool,
    blends: &[Blend],
) -> impl Iterator<Item = ColorKey> + '_ {
    [DepthDirection::Standard, DepthDirection::Reversed]
        .into_iter()
        .flat_map(move |direction| {
            [DepthTest::Nearer, DepthTest::NearerOrEqual]
                .into_iter()
                .flat_map(move |depth_test| {
                    blends.iter().map(move |&blend| ColorKey {
                        cull_mode,
                        skinned,
                        direction,
                        depth_test,
                        blend,
                    })
                })
        })
}

fn grid_index(
    direction: DepthDirection,
    depth_test: DepthTest,
    slot: usize,
    slots: usize,
) -> Option<usize> {
    if slot >= slots {
        return None;
    }
    direction
        .index()
        .checked_mul(DEPTH_TEST_COUNT)?
        .checked_add(depth_test.index())?
        .checked_mul(slots)?
        .checked_add(slot)
}

struct PipelineSet {
    pipelines: Vec<wgpu::RenderPipeline>,
    slots: usize,
}

impl PipelineSet {
    fn take(built: &mut impl Iterator<Item = wgpu::RenderPipeline>, slots: usize) -> Self {
        Self {
            pipelines: built
                .take((DEPTH_DIRECTION_COUNT * DEPTH_TEST_COUNT).saturating_mul(slots))
                .collect(),
            slots,
        }
    }

    fn get(
        &self,
        direction: DepthDirection,
        depth_test: DepthTest,
        slot: usize,
    ) -> Option<&wgpu::RenderPipeline> {
        self.pipelines
            .get(grid_index(direction, depth_test, slot, self.slots)?)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ShadowKey {
    alpha: bool,
    skinned: bool,
    culled: bool,
}

fn shadow_keys() -> impl Iterator<Item = ShadowKey> {
    [false, true].into_iter().flat_map(|alpha| {
        [false, true].into_iter().flat_map(move |skinned| {
            [false, true].into_iter().map(move |culled| ShadowKey {
                alpha,
                skinned,
                culled,
            })
        })
    })
}

fn shadow_index(key: ShadowKey) -> usize {
    usize::from(key.alpha)
        .saturating_mul(4)
        .saturating_add(usize::from(key.skinned).saturating_mul(2))
        .saturating_add(usize::from(key.culled))
}

#[derive(Debug, Clone, Copy)]
enum PipelineJob {
    Color(ColorKey),
    Shadow(ShadowKey),
}

pub const MAX_DIRECTIONAL_LIGHTS: usize = 3;

impl Blend {
    fn merge_index(self) -> Option<usize> {
        match self {
            Self::MergeMask => Some(0),
            Self::MergeAlphaBlend => Some(1),
            _ => None,
        }
    }

    fn index(self) -> usize {
        match self {
            Self::Opaque => 0,
            Self::AlphaBlend => 1,
            Self::Additive => 2,
            Self::AlphaAdditive => 3,
            Self::AlphaBlendWritesDepth => 4,
            Self::AddScaledByAlpha => 5,
            Self::AddScaledByColor => 6,
            Self::AddInverseScaledByColor => 7,
            Self::Multiply => 8,
            Self::MergeMask => 9,
            Self::MergeAlphaBlend => 10,
        }
    }
}

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32FloatStencil8;

const FALLBACK_DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24PlusStencil8;

const MERGE_STENCIL: u32 = 1;

fn depth_format(device: &wgpu::Device) -> wgpu::TextureFormat {
    if device
        .features()
        .contains(wgpu::Features::DEPTH32FLOAT_STENCIL8)
    {
        DEPTH_FORMAT
    } else {
        FALLBACK_DEPTH_FORMAT
    }
}

const STENCIL_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Stencil8;

fn screen_stencil_state(stencil: wgpu::StencilState) -> Option<wgpu::DepthStencilState> {
    Some(wgpu::DepthStencilState {
        format: STENCIL_FORMAT,
        depth_write_enabled: None,
        depth_compare: None,
        stencil,
        bias: wgpu::DepthBiasState::default(),
    })
}

fn content_stencil_state() -> wgpu::StencilState {
    let face = wgpu::StencilFaceState {
        compare: wgpu::CompareFunction::Equal,
        fail_op: wgpu::StencilOperation::Keep,
        depth_fail_op: wgpu::StencilOperation::Keep,
        pass_op: wgpu::StencilOperation::Keep,
    };
    wgpu::StencilState {
        front: face,
        back: face,
        read_mask: 0xFF,
        write_mask: 0,
    }
}

fn mask_stencil_state(pass_op: wgpu::StencilOperation) -> wgpu::StencilState {
    let face = wgpu::StencilFaceState {
        compare: wgpu::CompareFunction::Equal,
        fail_op: wgpu::StencilOperation::Keep,
        depth_fail_op: wgpu::StencilOperation::Keep,
        pass_op,
    };
    wgpu::StencilState {
        front: face,
        back: face,
        read_mask: 0xFF,
        write_mask: 0xFF,
    }
}

const UNIFORM_STRIDE: u64 = UNIFORM_SIZE;

const UNIFORM_SIZE: u64 = 416;

const UNIFORM_FLOATS: usize = 104;

const FRAME_UNIFORM_SIZE: u64 = 1264;

const FRAME_SLOT_STRIDE: u64 = frame::slot_stride(FRAME_UNIFORM_SIZE);

const LIGHT_SLOT_STRIDE: u64 = frame::slot_stride(SHADOW_UNIFORM_SIZE);

const FRAME_UNIFORM_FLOATS: usize = 316;

const UI_INSTANCE_SIZE: u64 = 112;

const GEOMETRY_VERTEX_SIZE: u64 = 56;

const UI_GEOMETRY_TEXTURED_TRANSFORM_SIZE: u64 = 64;

const SHADOW_UNIFORM_SIZE: u64 = 112;

const MAX_BONE_BASE: u32 = 1 << 24;

const COPY_ROW_ALIGNMENT: usize = 256;

pub const MAX_ANISOTROPY: u16 = 16;

const DEFAULT_ANISOTROPY: u16 = MAX_ANISOTROPY;

const MAX_POLLS: u32 = 100_000;

pub trait SurfaceTarget:
    wgpu::rwh::HasWindowHandle + wgpu::rwh::HasDisplayHandle + Send + Sync + 'static
{
}

impl<T> SurfaceTarget for T where
    T: wgpu::rwh::HasWindowHandle + wgpu::rwh::HasDisplayHandle + Send + Sync + 'static
{
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MeshHandle(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextureHandle(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RenderTargetHandle(usize);

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Instance {
    pub mesh: MeshHandle,
    pub texture: Option<TextureHandle>,
    pub normal_map: Option<TextureHandle>,
    pub transform: Mat4,
    pub tint: [f32; 4],
    pub alpha_cutoff: f32,
    pub blend: Blend,
    pub height_shear: [f32; 4],
    pub cull: bool,
    pub uv_linear: [[f32; 2]; 2],
    pub uv_offset: [f32; 2],
    pub material_ambient: [f32; 3],
    pub material_diffuse: [f32; 3],
    pub material_emissive: [f32; 3],
    pub light_add: [f32; 3],
    pub light_set: u8,
    pub texture2: Option<TextureHandle>,
    pub detail_combine: DetailCombine,
    pub house_color: [f32; 4],
    pub house_color_extra: [[f32; 4]; 2],
    pub detail_alpha_combine: DetailAlphaCombine,
    pub uv2_linear: [[f32; 2]; 2],
    pub uv2_offset: [f32; 2],
    pub env_map0: Option<EnvMap>,
    pub env_map1: Option<EnvMap>,
    pub bump_offset: Option<[[f32; 2]; 2]>,
    pub sort_level: Option<u8>,
    pub surface_overlay: bool,
    pub unlit: bool,
    pub ambient_from_vertex_color: bool,
    pub diffuse_from_vertex_color: bool,
    pub dcg_lighting_disabled: bool,
    pub dcg_multiplies_result: bool,
    pub modulate2x: Modulate2x,
    pub visible: bool,
    pub casts_shadow: bool,
    pub depth_test: DepthTest,
    pub vertex_alpha: bool,
    pub custom_material: Option<CustomMaterialHandle>,
    pub custom_shader: Option<CustomShaderHandle>,
    pub bounds: Option<Aabb>,
    pub bone_base: Option<u32>,
    pub fog: bool,
    pub visibility_mask: bool,
    pub visibility_mask_alpha: bool,
}

impl Instance {
    #[must_use]
    pub fn new(mesh: MeshHandle) -> Self {
        Self {
            mesh,
            texture: None,
            normal_map: None,
            transform: Mat4::IDENTITY,
            tint: [1.0, 1.0, 1.0, 1.0],
            alpha_cutoff: 0.0,
            blend: Blend::Opaque,
            cull: false,
            uv_linear: [[1.0, 0.0], [0.0, 1.0]],
            uv_offset: [0.0, 0.0],
            material_ambient: [1.0, 1.0, 1.0],
            material_diffuse: [1.0, 1.0, 1.0],
            material_emissive: [0.0, 0.0, 0.0],
            light_add: [0.0, 0.0, 0.0],
            light_set: 0,
            height_shear: [0.0; 4],
            texture2: None,
            detail_combine: DetailCombine::Disable,
            house_color: [1.0, 1.0, 1.0, 1.0],
            house_color_extra: [[0.0; 4]; 2],
            detail_alpha_combine: DetailAlphaCombine::Disable,
            uv2_linear: [[1.0, 0.0], [0.0, 1.0]],
            uv2_offset: [0.0, 0.0],
            env_map0: None,
            env_map1: None,
            bump_offset: None,
            sort_level: None,
            surface_overlay: false,
            unlit: false,
            ambient_from_vertex_color: false,
            diffuse_from_vertex_color: false,
            dcg_lighting_disabled: false,
            dcg_multiplies_result: false,
            modulate2x: Modulate2x::Off,
            visible: true,
            casts_shadow: true,
            depth_test: DepthTest::Nearer,
            vertex_alpha: false,
            custom_material: None,
            custom_shader: None,
            bounds: None,
            bone_base: None,
            fog: true,
            visibility_mask: false,
            visibility_mask_alpha: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DirectionalLight {
    pub direction: Vec3,
    pub color: [f32; 3],
}

pub const LIGHT_SETS: u8 = 4;

const LIGHT_SET_COUNT: usize = LIGHT_SETS as usize;

const SET_LIGHTS: usize = LIGHT_SET_COUNT * MAX_DIRECTIONAL_LIGHTS;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightSet {
    pub lights: [DirectionalLight; MAX_DIRECTIONAL_LIGHTS],
    pub ambient_color: [f32; 3],
    pub cookie: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightCookie {
    pub texture: TextureHandle,
    pub transform: [f32; 4],
    pub tint: [f32; 3],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisibilityMask {
    pub texture: TextureHandle,
    pub transform: [f32; 4],
    pub strength: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DistanceFog {
    pub color: [f32; 3],
    pub start: f32,
    pub end: f32,
}

impl Default for DirectionalLight {
    fn default() -> Self {
        Self {
            direction: Vec3::ZERO,
            color: [0.0, 0.0, 0.0],
        }
    }
}

#[derive(Debug, Clone)]
pub struct Scene<'a> {
    pub camera: Camera,
    pub lights: [DirectionalLight; MAX_DIRECTIONAL_LIGHTS],
    pub ambient_color: [f32; 3],
    pub lit_ceiling: Option<f32>,
    pub modulate2x_scene: bool,
    pub post: PostEffects<'a>,
    pub clear_color: [f64; 4],
    pub instances: &'a [Instance],
    pub clip_plane: Option<[f32; 4]>,
    pub shadow: Option<ShadowSample>,
    pub light_sets: [Option<LightSet>; LIGHT_SET_COUNT],
    pub light_cookie: Option<LightCookie>,
    pub contact_shadows: Option<ContactShadows>,
    pub ui_quads: &'a [UiQuad],
    pub ui_geometry: &'a [UiGeometry<'a>],
    pub ordered_screen: &'a [ScreenDraw<'a>],
    pub screen_fade: Option<ScreenFade>,
    pub fog: Option<DistanceFog>,
    pub visibility_mask: Option<VisibilityMask>,
}

impl<'a> Scene<'a> {
    #[must_use]
    pub fn new(camera: Camera) -> Self {
        Self {
            camera,
            lights: [
                DirectionalLight {
                    direction: Vec3::new(-0.4, -0.6, -0.7),
                    color: [1.0, 1.0, 1.0],
                },
                DirectionalLight::default(),
                DirectionalLight::default(),
            ],
            ambient_color: [0.3, 0.3, 0.3],
            lit_ceiling: Some(2.0),
            modulate2x_scene: false,
            post: PostEffects::default(),
            clear_color: [0.2478, 0.2717, 0.3133, 1.0],
            instances: &[],
            clip_plane: None,
            shadow: None,
            light_sets: [None; LIGHT_SET_COUNT],
            light_cookie: None,
            contact_shadows: None,
            ui_quads: &[],
            ui_geometry: &[],
            ordered_screen: &[],
            screen_fade: None,
            fog: None,
            visibility_mask: None,
        }
    }

    #[must_use]
    pub fn with_instances(mut self, instances: &'a [Instance]) -> Self {
        self.instances = instances;
        self
    }

    #[must_use]
    pub fn with_ui_quads(mut self, ui_quads: &'a [UiQuad]) -> Self {
        self.ui_quads = ui_quads;
        self
    }

    #[must_use]
    pub fn with_ui_geometry(mut self, ui_geometry: &'a [UiGeometry<'a>]) -> Self {
        self.ui_geometry = ui_geometry;
        self
    }

    #[must_use]
    pub fn with_screen_fade(mut self, fade: ScreenFade) -> Self {
        self.screen_fade = Some(fade);
        self
    }

    #[must_use]
    pub fn with_ordered_screen(mut self, ordered_screen: &'a [ScreenDraw<'a>]) -> Self {
        self.ordered_screen = ordered_screen;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceInfo {
    pub adapter: String,
    pub backend: String,
    pub driver: String,
    pub block_compression: bool,
}

enum Target {
    Window {
        surface: wgpu::Surface<'static>,
        config: wgpu::SurfaceConfiguration,
        view_format: wgpu::TextureFormat,
    },
    Offscreen {
        format: wgpu::TextureFormat,
        texture: wgpu::Texture,
        view: wgpu::TextureView,
    },
}

struct GpuMesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    vertex_count: usize,
    skin: Option<(wgpu::Buffer, u32)>,
}

struct GpuTexture {
    texture: wgpu::Texture,
    format: Option<PixelFormat>,
    view: wgpu::TextureView,
    address_u: AddressMode,
    address_v: AddressMode,
    width: u32,
    height: u32,
}

type MaterialKey = (Option<usize>, Option<usize>, Option<usize>);

type MaterialSlot = usize;

#[derive(Debug, Clone, Copy)]
struct UiTextureMaterial {
    handle: TextureHandle,
    slot: MaterialSlot,
    size: (u32, u32),
}

struct CustomShaderEntry {
    layout: wgpu::BindGroupLayout,
    pipelines_culled: PipelineSet,
    pipelines_double_sided: PipelineSet,
    texture_slots: usize,
    uniform_size: usize,
    scene_color: bool,
    frame_inputs: usize,
    label: String,
    builtin_material: bool,
    scene_layout: Option<wgpu::BindGroupLayout>,
    scene_buffer: Option<wgpu::Buffer>,
    scene_group: Option<wgpu::BindGroup>,
    scene_textures: Vec<Option<usize>>,
    scene_uniform_size: usize,
    scene_texture_slots: usize,
}

struct CustomMaterialEntry {
    shader: usize,
    buffer: Option<wgpu::Buffer>,
    bind_group: wgpu::BindGroup,
    textures: Vec<Option<usize>>,
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    target: Target,
    info: DeviceInfo,

    depth: wgpu::TextureView,
    depth_sampled: wgpu::TextureView,
    screen_stencil: wgpu::TextureView,
    frame_input_layouts: [wgpu::BindGroupLayout; 4],
    frame_input_groups: [Option<wgpu::BindGroup>; 4],
    contact_shadow_pipeline: wgpu::RenderPipeline,
    scene_color: Option<SceneColor>,
    post: PostState,
    can_capture_scene_color: bool,
    width: u32,
    height: u32,

    pipelines_culled: PipelineSet,
    pipelines_double_sided: PipelineSet,
    pipelines_skinned_culled: PipelineSet,
    pipelines_skinned_double_sided: PipelineSet,
    merge_pipelines: [PipelineSet; 2],
    pipeline_cache: Option<pipeline_cache::DiskCache>,
    pipeline_cache_state: PipelineCacheState,
    bone_buffer: wgpu::Buffer,
    bone_capacity: u32,
    bone_count: usize,
    bone_scratch: Vec<u8>,
    depth_direction: DepthDirection,
    cull_override: Option<bool>,

    uniform_layout: wgpu::BindGroupLayout,
    material_layout: wgpu::BindGroupLayout,
    samplers: SamplerSet,
    anisotropy: u16,

    uniform_buffer: wgpu::Buffer,
    frame_uniform_buffer: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    shadow_uniform_layout: wgpu::BindGroupLayout,
    shadow_uniform_group: wgpu::BindGroup,
    shadow_instance_buffer: wgpu::Buffer,
    shadow_view_proj_buffer: wgpu::Buffer,
    shadow_scratch: Vec<u8>,
    shadow_pipelines: Vec<wgpu::RenderPipeline>,
    shadow_sampler: wgpu::Sampler,
    shadow_placeholder: wgpu::TextureView,
    cookie_placeholder: GpuTexture,
    cookie_sampler: wgpu::Sampler,
    bound_cookie: Option<TextureHandle>,
    mask_sampler: wgpu::Sampler,
    bound_mask: Option<TextureHandle>,
    bound_shadow_map: Option<usize>,
    shadow_maps: Vec<ShadowMapEntry>,
    uniform_scratch: Vec<u8>,

    blank_texture: GpuTexture,
    flat_normal_texture: GpuTexture,
    meshes: Vec<GpuMesh>,
    textures: Vec<GpuTexture>,
    materials: BTreeMap<MaterialKey, MaterialSlot>,
    material_groups: Vec<wgpu::BindGroup>,

    ui_uniform_layout: wgpu::BindGroupLayout,
    ui_pipeline: wgpu::RenderPipeline,
    fade_pass: FadePass,
    ui_instance_buffer: wgpu::Buffer,
    ui_instance_capacity: u32,
    ui_viewport_buffer: wgpu::Buffer,
    ui_bind_group: wgpu::BindGroup,
    ui_scratch: Vec<u8>,

    ui_geometry_triangle_pipeline: wgpu::RenderPipeline,
    ui_geometry_line_pipeline: wgpu::RenderPipeline,
    ui_mask_push_pipeline: wgpu::RenderPipeline,
    ui_mask_pop_pipeline: wgpu::RenderPipeline,
    ui_geometry_triangle_buffer: wgpu::Buffer,
    ui_geometry_triangle_capacity: u32,
    ui_geometry_line_buffer: wgpu::Buffer,
    ui_geometry_line_capacity: u32,
    ui_geometry_bind_group: wgpu::BindGroup,
    ui_geometry_triangle_scratch: Vec<u8>,
    ui_geometry_line_scratch: Vec<u8>,

    ui_geometry_textured_transform_layout: wgpu::BindGroupLayout,
    ui_geometry_textured_pipeline: wgpu::RenderPipeline,
    ui_geometry_textured_buffer: wgpu::Buffer,
    ui_geometry_textured_capacity: u32,
    ui_geometry_textured_transform_buffer: wgpu::Buffer,
    ui_geometry_textured_transform_capacity: u32,
    ui_geometry_textured_transform_bind_group: wgpu::BindGroup,
    ui_geometry_textured_scratch: Vec<u8>,
    ui_geometry_textured_transform_scratch: Vec<u8>,
    ui_retained: RetainedUi,

    custom_shaders: Vec<CustomShaderEntry>,
    custom_materials: Vec<CustomMaterialEntry>,
    render_targets: Vec<RenderTarget>,
    color_format: wgpu::TextureFormat,

    draw_calls: u32,
    present_wait: std::time::Duration,
    capture_next: bool,
    captured: Option<(u32, u32, Vec<u8>)>,
    drawn_instances: u32,

    shadow_draw_calls: u32,
    shadow_drawn_instances: u32,
    shadow_alpha_instances: u32,
    shadow_alpha_draw_calls: u32,

    target_draw_calls: u32,
    target_drawn_instances: u32,

    frustum_culling: bool,
    shadow_alpha_test: bool,
    culled_instances: u32,
    shadow_culled_instances: u32,
    target_culled_instances: u32,
    frame: frame::FrameEncoder,
}

impl Renderer {
    pub fn for_window<T: SurfaceTarget>(target: Arc<T>, width: u32, height: u32) -> Result<Self> {
        Self::for_window_with_pipeline_cache(target, width, height, None)
    }

    pub fn for_window_with_pipeline_cache<T: SurfaceTarget>(
        target: Arc<T>,
        width: u32,
        height: u32,
        cache_dir: Option<&Path>,
    ) -> Result<Self> {
        let (surface, adapter) = open_adapter(|instance| {
            instance
                .create_surface(Arc::clone(&target))
                .map(Some)
                .map_err(|e| Error::backend("creating a surface for the window", e))
        })?;
        let surface = surface.ok_or(Error::SurfaceUnsupported)?;
        let (device, queue, info, cache_key) = request_device(&adapter)?;

        let (width, height) = clamp_size(width, height);
        let mut config = surface
            .get_default_config(&adapter, width, height)
            .ok_or(Error::SurfaceUnsupported)?;
        let view_format = config.format.remove_srgb_suffix();
        if view_format != config.format {
            config.view_formats.push(view_format);
        }
        let can_capture_scene_color = surface
            .get_capabilities(&adapter)
            .usages
            .contains(wgpu::TextureUsages::COPY_SRC);
        if can_capture_scene_color {
            config.usage |= wgpu::TextureUsages::COPY_SRC;
        }
        surface.configure(&device, &config);

        Self::assemble(
            Opened {
                device,
                queue,
                info,
                cache_key,
            },
            cache_dir,
            Target::Window {
                surface,
                config,
                view_format,
            },
            view_format,
            width,
            height,
            can_capture_scene_color,
        )
    }

    pub fn offscreen(width: u32, height: u32) -> Result<Self> {
        Self::offscreen_with_pipeline_cache(width, height, None)
    }

    pub fn offscreen_with_pipeline_cache(
        width: u32,
        height: u32,
        cache_dir: Option<&Path>,
    ) -> Result<Self> {
        let opened = offscreen_device()?;
        let device = &opened.device;

        let (width, height) = clamp_size(width, height);
        let format = wgpu::TextureFormat::Rgba8Unorm;
        let (texture, view) = create_color_target(device, format, width, height);

        Self::assemble(
            opened,
            cache_dir,
            Target::Offscreen {
                format,
                texture,
                view,
            },
            format,
            width,
            height,
            true,
        )
    }

    fn assemble(
        opened: Opened,
        cache_dir: Option<&Path>,
        target: Target,
        color_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
        can_capture_scene_color: bool,
    ) -> Result<Self> {
        let Opened {
            device,
            queue,
            info,
            cache_key,
        } = opened;
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mint textured-lit"),
            source: wgpu::ShaderSource::Wgsl(
                concat!(
                    include_str!("instance_block.wgsl"),
                    include_str!("shader.wgsl")
                )
                .into(),
            ),
        });

        let instance_buffer_entry = wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Storage { read_only: true },
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(UNIFORM_SIZE),
            },
            count: None,
        };
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mint uniforms"),
            entries: &[
                instance_buffer_entry,
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Depth,
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Comparison),
                    count: None,
                },
                bone_layout_entry(3),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: wgpu::BufferSize::new(FRAME_UNIFORM_SIZE),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 6,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 7,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 8,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let shadow_uniform_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("mint shadow uniforms"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(SHADOW_UNIFORM_SIZE),
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: true,
                            min_binding_size: wgpu::BufferSize::new(SHADOW_UNIFORM_SIZE),
                        },
                        count: None,
                    },
                    bone_layout_entry(2),
                ],
            });

        let texture_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let sampler_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        };
        let material_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mint material"),
            entries: &[
                texture_entry(0),
                sampler_entry(1),
                texture_entry(2),
                texture_entry(3),
                sampler_entry(4),
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mint pipeline layout"),
            bind_group_layouts: &[Some(&uniform_layout), Some(&material_layout)],
            immediate_size: 0,
        });

        let shadow_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mint shadow depth"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shadow.wgsl").into()),
        });
        let shadow_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("mint shadow pipeline layout"),
                bind_group_layouts: &[Some(&shadow_uniform_layout)],
                immediate_size: 0,
            });
        let shadow_alpha_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("mint shadow alpha pipeline layout"),
                bind_group_layouts: &[Some(&shadow_uniform_layout), Some(&material_layout)],
                immediate_size: 0,
            });

        let (disk_cache, pipeline_cache_state) =
            pipeline_cache::open(&device, cache_key.as_deref(), cache_dir);
        let cache = disk_cache.as_ref().map(|disk| &disk.cache);
        let jobs: Vec<PipelineJob> = [
            (Some(wgpu::Face::Back), false),
            (None, false),
            (Some(wgpu::Face::Back), true),
            (None, true),
        ]
        .into_iter()
        .flat_map(|(cull_mode, skinned)| grid_keys(cull_mode, skinned, &GRID_BLENDS))
        .chain(
            [Some(wgpu::Face::Back), None]
                .into_iter()
                .flat_map(|cull_mode| grid_keys(cull_mode, false, &MERGE_BLENDS)),
        )
        .map(PipelineJob::Color)
        .chain(shadow_keys().map(PipelineJob::Shadow))
        .collect();
        let shadow_layouts = ShadowLayouts {
            plain: &shadow_pipeline_layout,
            alpha: &shadow_alpha_pipeline_layout,
        };
        let mut built = mint_pool::map_ordered(&jobs, |job| match *job {
            PipelineJob::Color(key) => build_pipeline_masked(
                &device,
                cache,
                &pipeline_layout,
                &shader,
                color_format,
                key,
                wgpu::ColorWrites::ALL,
            ),
            PipelineJob::Shadow(key) => {
                build_shadow_pipeline(&device, cache, shadow_layouts, &shadow_shader, key)
            }
        })
        .into_iter();
        let pipelines_culled = PipelineSet::take(&mut built, BLEND_MODE_COUNT);
        let pipelines_double_sided = PipelineSet::take(&mut built, BLEND_MODE_COUNT);
        let pipelines_skinned_culled = PipelineSet::take(&mut built, BLEND_MODE_COUNT);
        let pipelines_skinned_double_sided = PipelineSet::take(&mut built, BLEND_MODE_COUNT);
        let merge_pipelines = [
            PipelineSet::take(&mut built, MERGE_PART_COUNT),
            PipelineSet::take(&mut built, MERGE_PART_COUNT),
        ];
        let shadow_pipelines: Vec<wgpu::RenderPipeline> = built.collect();

        let samplers = SamplerSet::build(&device, DEFAULT_ANISOTROPY);

        let shadow_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("mint shadow comparison"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Nearest,
            compare: Some(wgpu::CompareFunction::LessEqual),
            ..Default::default()
        });
        let (shadow_placeholder, _) = create_shadow_texture(&device, 1, 1);

        let capacity = 64u32;
        let bone_buffer = create_bone_buffer(&device, capacity);
        let uniform_buffer = create_uniform_buffer(&device, capacity);
        let frame_slots = 8u32;
        let light_slots = 4u32;
        let frame_uniform_buffer = create_slot_buffer(
            &device,
            "mint frame uniforms",
            FRAME_SLOT_STRIDE,
            frame_slots,
        );
        let cookie_placeholder = TextureData::rgba8(1, 1, vec![255, 255, 255, 255])
            .ok_or(Error::EmptyTexture)
            .and_then(|data| upload_texture(&device, &queue, &data, info.block_compression))?;
        let cookie_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("mint light cookie"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let mask_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("mint visibility mask"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniform_bind_group = create_uniform_bind_group(
            &device,
            &uniform_layout,
            &uniform_buffer,
            (&shadow_placeholder, &shadow_sampler),
            &bone_buffer,
            &frame_uniform_buffer,
            [
                (&cookie_placeholder.view, &cookie_sampler),
                (&cookie_placeholder.view, &mask_sampler),
            ],
        );
        let shadow_instance_buffer = create_shadow_instance_buffer(&device, capacity);
        let shadow_view_proj_buffer = create_slot_buffer(
            &device,
            "mint shadow view-projection",
            LIGHT_SLOT_STRIDE,
            light_slots,
        );
        let shadow_uniform_group = create_shadow_uniform_group(
            &device,
            &shadow_uniform_layout,
            &shadow_instance_buffer,
            &shadow_view_proj_buffer,
            &bone_buffer,
        );

        let (depth, depth_sampled) = create_depth_views(&device, width, height);
        let screen_stencil = create_screen_stencil_view(&device, width, height);
        let frame_input_layouts =
            std::array::from_fn(|index| create_frame_input_layout(&device, index));
        let frame_input_groups = std::array::from_fn(|index| {
            frame_input_layouts.get(index).and_then(|layout| {
                create_frame_input_group(&device, layout, index, &depth_sampled, None)
            })
        });
        let contact_shadow_pipeline = create_contact_shadow_pipeline(
            &device,
            cache,
            &uniform_layout,
            frame_input_layouts.get(frame_input_index(true, false)),
            color_format,
        );

        let ui_uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("mint ui uniforms"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(UI_INSTANCE_SIZE),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(16),
                    },
                    count: None,
                },
            ],
        });
        let ui_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mint ui pipeline layout"),
            bind_group_layouts: &[Some(&ui_uniform_layout), Some(&material_layout)],
            immediate_size: 0,
        });
        let ui_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mint ui"),
            source: wgpu::ShaderSource::Wgsl(include_str!("ui.wgsl").into()),
        });
        let ui_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mint ui screen pass"),
            layout: Some(&ui_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &ui_shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: screen_stencil_state(content_stencil_state()),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &ui_shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache,
        });
        let fade_pass = FadePass::new(&device, cache, color_format, &|| {
            screen_stencil_state(content_stencil_state())
        });
        let ui_instance_capacity = 64u32;
        let ui_instance_buffer = create_ui_instance_buffer(&device, ui_instance_capacity);
        let ui_viewport_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mint ui viewport"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let ui_bind_group = create_ui_bind_group(
            &device,
            &ui_uniform_layout,
            &ui_instance_buffer,
            &ui_viewport_buffer,
        );

        let ui_geometry_uniform_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("mint ui geometry uniforms"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(16),
                    },
                    count: None,
                }],
            });
        let ui_geometry_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("mint ui geometry pipeline layout"),
                bind_group_layouts: &[Some(&ui_geometry_uniform_layout)],
                immediate_size: 0,
            });
        let ui_geometry_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("mint ui geometry"),
            source: wgpu::ShaderSource::Wgsl(include_str!("ui_geometry.wgsl").into()),
        });
        let ui_geometry_vertex_buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: GEOMETRY_VERTEX_SIZE,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &GEOMETRY_VERTEX_ATTRIBUTES,
        })];
        let build_geometry_pipeline =
            |topology: wgpu::PrimitiveTopology,
             label: &str,
             stencil: wgpu::StencilState,
             color_writes: wgpu::ColorWrites| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some(label),
                    layout: Some(&ui_geometry_pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &ui_geometry_shader,
                        entry_point: Some("vs_main"),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        buffers: &ui_geometry_vertex_buffers,
                    },
                    primitive: wgpu::PrimitiveState {
                        topology,
                        front_face: wgpu::FrontFace::Ccw,
                        cull_mode: None,
                        ..Default::default()
                    },
                    depth_stencil: screen_stencil_state(stencil),
                    multisample: wgpu::MultisampleState::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &ui_geometry_shader,
                        entry_point: Some("fs_main"),
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format: color_format,
                            blend: wgpu_blend_state(Blend::AlphaBlend),
                            write_mask: color_writes,
                        })],
                    }),
                    multiview_mask: None,
                    cache,
                })
            };
        let ui_geometry_triangle_pipeline = build_geometry_pipeline(
            wgpu::PrimitiveTopology::TriangleList,
            "mint ui geometry (triangles)",
            content_stencil_state(),
            wgpu::ColorWrites::ALL,
        );
        let ui_geometry_line_pipeline = build_geometry_pipeline(
            wgpu::PrimitiveTopology::LineList,
            "mint ui geometry (lines)",
            content_stencil_state(),
            wgpu::ColorWrites::ALL,
        );
        let ui_mask_push_pipeline = build_geometry_pipeline(
            wgpu::PrimitiveTopology::TriangleList,
            "mint ui mask push",
            mask_stencil_state(wgpu::StencilOperation::IncrementClamp),
            wgpu::ColorWrites::empty(),
        );
        let ui_mask_pop_pipeline = build_geometry_pipeline(
            wgpu::PrimitiveTopology::TriangleList,
            "mint ui mask pop",
            mask_stencil_state(wgpu::StencilOperation::DecrementClamp),
            wgpu::ColorWrites::empty(),
        );
        let ui_geometry_triangle_capacity = 64u32;
        let ui_geometry_triangle_buffer =
            create_ui_geometry_buffer(&device, ui_geometry_triangle_capacity);
        let ui_geometry_line_capacity = 64u32;
        let ui_geometry_line_buffer = create_ui_geometry_buffer(&device, ui_geometry_line_capacity);
        let ui_geometry_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mint ui geometry uniforms"),
            layout: &ui_geometry_uniform_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &ui_viewport_buffer,
                    offset: 0,
                    size: None,
                }),
            }],
        });

        let ui_geometry_textured_transform_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("mint ui geometry textured transforms"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            UI_GEOMETRY_TEXTURED_TRANSFORM_SIZE,
                        ),
                    },
                    count: None,
                }],
            });
        let ui_geometry_textured_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("mint ui geometry textured pipeline layout"),
                bind_group_layouts: &[
                    Some(&ui_geometry_uniform_layout),
                    Some(&material_layout),
                    Some(&ui_geometry_textured_transform_layout),
                ],
                immediate_size: 0,
            });
        let ui_geometry_textured_pipeline =
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("mint ui geometry (textured triangles)"),
                layout: Some(&ui_geometry_textured_pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &ui_geometry_shader,
                    entry_point: Some("vs_textured"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &ui_geometry_vertex_buffers,
                },
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: screen_stencil_state(content_stencil_state()),
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &ui_geometry_shader,
                    entry_point: Some("fs_textured"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: color_format,
                        blend: wgpu_blend_state(Blend::AlphaBlend),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache,
            });
        let ui_geometry_textured_capacity = 64u32;
        let ui_geometry_textured_buffer =
            create_ui_geometry_buffer(&device, ui_geometry_textured_capacity);
        let ui_geometry_textured_transform_capacity = 16u32;
        let ui_geometry_textured_transform_buffer = create_ui_geometry_textured_transform_buffer(
            &device,
            ui_geometry_textured_transform_capacity,
        );
        let ui_geometry_textured_transform_bind_group =
            create_ui_geometry_textured_transform_bind_group(
                &device,
                &ui_geometry_textured_transform_layout,
                &ui_geometry_textured_transform_buffer,
            );

        let ui_retained = RetainedUi::new(
            &device,
            &ui_geometry_uniform_layout,
            &material_layout,
            color_format,
            cache,
        );

        let blank = TextureData::rgba8(1, 1, vec![255, 255, 255, 255])
            .ok_or(Error::EmptyTexture)
            .and_then(|data| upload_texture(&device, &queue, &data, info.block_compression))?;

        let flat_normal =
            TextureData::single(PixelFormat::Rgba8Unorm, 1, 1, vec![128, 128, 255, 255]);
        let flat_normal = upload_texture(&device, &queue, &flat_normal, info.block_compression)?;

        Ok(Self {
            device,
            queue,
            target,
            info,
            depth,
            depth_sampled,
            screen_stencil,
            frame_input_layouts,
            frame_input_groups,
            contact_shadow_pipeline,
            scene_color: None,
            post: PostState::default(),
            can_capture_scene_color,
            width,
            height,
            pipelines_culled,
            pipelines_double_sided,
            pipelines_skinned_culled,
            pipelines_skinned_double_sided,
            merge_pipelines,
            pipeline_cache: disk_cache,
            pipeline_cache_state,
            bone_buffer,
            bone_capacity: capacity,
            bone_count: 0,
            bone_scratch: Vec::new(),
            depth_direction: DepthDirection::default(),
            cull_override: None,
            uniform_layout,
            material_layout,
            samplers,
            anisotropy: DEFAULT_ANISOTROPY,
            uniform_buffer,
            frame_uniform_buffer,
            uniform_bind_group,
            shadow_uniform_layout,
            shadow_uniform_group,
            shadow_instance_buffer,
            shadow_view_proj_buffer,
            shadow_scratch: Vec::new(),
            shadow_pipelines,
            shadow_sampler,
            shadow_placeholder,
            bound_shadow_map: None,
            cookie_placeholder,
            cookie_sampler,
            bound_cookie: None,
            mask_sampler,
            bound_mask: None,
            shadow_maps: Vec::new(),
            uniform_scratch: Vec::new(),
            blank_texture: blank,
            flat_normal_texture: flat_normal,
            meshes: Vec::new(),
            textures: Vec::new(),
            materials: BTreeMap::new(),
            material_groups: Vec::new(),
            ui_uniform_layout,
            ui_pipeline,
            fade_pass,
            ui_instance_buffer,
            ui_instance_capacity,
            ui_viewport_buffer,
            ui_bind_group,
            ui_scratch: Vec::new(),
            ui_geometry_triangle_pipeline,
            ui_geometry_line_pipeline,
            ui_mask_push_pipeline,
            ui_mask_pop_pipeline,
            ui_geometry_triangle_buffer,
            ui_geometry_triangle_capacity,
            ui_geometry_line_buffer,
            ui_geometry_line_capacity,
            ui_geometry_bind_group,
            ui_geometry_triangle_scratch: Vec::new(),
            ui_geometry_line_scratch: Vec::new(),
            ui_geometry_textured_transform_layout,
            ui_geometry_textured_pipeline,
            ui_geometry_textured_buffer,
            ui_geometry_textured_capacity,
            ui_geometry_textured_transform_buffer,
            ui_geometry_textured_transform_capacity,
            ui_geometry_textured_transform_bind_group,
            ui_geometry_textured_scratch: Vec::new(),
            ui_geometry_textured_transform_scratch: Vec::new(),
            ui_retained,
            custom_shaders: Vec::new(),
            custom_materials: Vec::new(),
            render_targets: Vec::new(),
            color_format,
            draw_calls: 0,
            present_wait: std::time::Duration::ZERO,
            capture_next: false,
            captured: None,
            drawn_instances: 0,
            shadow_draw_calls: 0,
            shadow_drawn_instances: 0,
            shadow_alpha_instances: 0,
            shadow_alpha_draw_calls: 0,
            target_draw_calls: 0,
            target_drawn_instances: 0,
            frustum_culling: true,
            shadow_alpha_test: true,
            culled_instances: 0,
            shadow_culled_instances: 0,
            target_culled_instances: 0,
            frame: frame::FrameEncoder::new(capacity, capacity, frame_slots, light_slots),
        })
    }

    #[must_use]
    pub fn info(&self) -> &DeviceInfo {
        &self.info
    }

    #[must_use]
    pub fn pipeline_cache_state(&self) -> PipelineCacheState {
        self.pipeline_cache_state
    }

    pub fn save_pipeline_cache(&mut self) -> Result<bool> {
        self.pipeline_cache
            .as_mut()
            .map_or(Ok(false), pipeline_cache::DiskCache::save)
    }

    #[must_use]
    pub fn supports_block_compression(&self) -> bool {
        self.info.block_compression
    }

    #[must_use]
    pub fn draw_calls(&self) -> u32 {
        self.draw_calls
    }

    #[must_use]
    pub fn present_wait(&self) -> std::time::Duration {
        self.present_wait
    }

    #[must_use]
    pub fn submits(&self) -> (u32, u32) {
        self.frame.submits()
    }

    #[must_use]
    pub fn passes(&self) -> u32 {
        self.frame.passes()
    }

    #[must_use]
    pub fn drawn_instances(&self) -> u32 {
        self.drawn_instances
    }

    #[must_use]
    pub fn shadow_draw_calls(&self) -> u32 {
        self.shadow_draw_calls
    }

    #[must_use]
    pub fn shadow_drawn_instances(&self) -> u32 {
        self.shadow_drawn_instances
    }

    #[must_use]
    pub fn shadow_alpha_instances(&self) -> (u32, u32) {
        (self.shadow_alpha_instances, self.shadow_alpha_draw_calls)
    }

    #[must_use]
    pub fn target_draw_calls(&self) -> u32 {
        self.target_draw_calls
    }

    #[must_use]
    pub fn target_drawn_instances(&self) -> u32 {
        self.target_drawn_instances
    }

    #[must_use]
    pub fn frustum_culling(&self) -> bool {
        self.frustum_culling
    }

    pub fn set_frustum_culling(&mut self, enabled: bool) {
        self.frustum_culling = enabled;
    }

    #[must_use]
    pub fn shadow_alpha_test(&self) -> bool {
        self.shadow_alpha_test
    }

    pub fn set_shadow_alpha_test(&mut self, enabled: bool) {
        self.shadow_alpha_test = enabled;
    }

    #[must_use]
    pub fn culled_instances(&self) -> u32 {
        self.culled_instances
    }

    #[must_use]
    pub fn shadow_culled_instances(&self) -> u32 {
        self.shadow_culled_instances
    }

    #[must_use]
    pub fn target_culled_instances(&self) -> u32 {
        self.target_culled_instances
    }

    fn color_view_proj(&self, scene: &Scene<'_>, aspect: f32) -> Mat4 {
        self.camera_view_proj(&scene.camera, aspect)
    }

    fn camera_view_proj(&self, camera: &Camera, aspect: f32) -> Mat4 {
        camera
            .projection_with_depth(self.depth_direction, aspect)
            .mul_mat4(camera.view())
    }

    #[must_use]
    pub fn view_frustum(
        &self,
        camera: &Camera,
        target: Option<RenderTargetHandle>,
    ) -> Option<Frustum> {
        let aspect = match target {
            None => self.aspect_ratio(),
            Some(target) => {
                let t = self.render_targets.get(target.0)?;
                f64_ratio(t.width, t.height)
            }
        };
        Some(Frustum::from_view_proj(
            self.camera_view_proj(camera, aspect),
        ))
    }

    #[must_use]
    pub fn samples_texture(&self, instance: &Instance, texture: TextureHandle) -> bool {
        let wanted = Some(texture);
        if instance.texture == wanted
            || instance.texture2 == wanted
            || instance.normal_map == wanted
        {
            return true;
        }
        let index = Some(texture.0);
        instance
            .custom_material
            .and_then(|handle| self.custom_materials.get(handle.0))
            .is_some_and(|material| material.textures.contains(&index))
            || self
                .custom_shader_for(instance)
                .is_some_and(|shader| shader.scene_textures.contains(&index))
    }

    fn custom_shader_for(&self, instance: &Instance) -> Option<&CustomShaderEntry> {
        match (instance.custom_material, instance.custom_shader) {
            (Some(handle), _) => self
                .custom_materials
                .get(handle.0)
                .and_then(|material| self.custom_shaders.get(material.shader)),
            (None, Some(handle)) => self.custom_shaders.get(handle.0),
            (None, None) => None,
        }
    }

    fn in_frustum(&self, instance: &Instance, frustum: &Frustum) -> bool {
        if !self.frustum_culling {
            return true;
        }
        match instance.bounds {
            Some(bounds) => frustum.intersects(&transformed_bounds(&bounds, instance.transform)),
            None => true,
        }
    }

    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    #[must_use]
    pub fn aspect_ratio(&self) -> f32 {
        if self.height == 0 {
            1.0
        } else {
            f64_ratio(self.width, self.height)
        }
    }

    #[must_use]
    pub fn backface_culling(&self) -> bool {
        self.cull_override.unwrap_or(false)
    }

    pub fn set_backface_culling(&mut self, enabled: bool) {
        self.cull_override = Some(enabled);
    }

    #[must_use]
    pub fn cull_override(&self) -> Option<bool> {
        self.cull_override
    }

    pub fn set_cull_override(&mut self, over: Option<bool>) {
        self.cull_override = over;
    }

    #[must_use]
    pub fn anisotropy(&self) -> u16 {
        self.anisotropy
    }

    pub fn set_anisotropy(&mut self, samples: u16) {
        let samples = samples.clamp(1, MAX_ANISOTROPY);
        if samples == self.anisotropy {
            return;
        }
        self.anisotropy = samples;
        self.samplers = SamplerSet::build(&self.device, samples);
        self.materials.clear();
        self.material_groups.clear();
    }

    #[must_use]
    pub fn depth_direction(&self) -> DepthDirection {
        self.depth_direction
    }

    pub fn set_depth_direction(&mut self, direction: DepthDirection) {
        self.depth_direction = direction;
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let (width, height) = clamp_size(width, height);
        if width == self.width && height == self.height {
            return;
        }
        self.width = width;
        self.height = height;
        (self.depth, self.depth_sampled) = create_depth_views(&self.device, width, height);
        self.screen_stencil = create_screen_stencil_view(&self.device, width, height);
        if self.scene_color.is_some() {
            self.scene_color = Some(create_scene_color(
                &self.device,
                self.color_format,
                width,
                height,
            ));
        }
        self.rebuild_frame_inputs();

        match &mut self.target {
            Target::Window {
                surface, config, ..
            } => {
                config.width = width;
                config.height = height;
                surface.configure(&self.device, config);
            }
            Target::Offscreen {
                format,
                texture,
                view,
            } => {
                let (new_texture, new_view) =
                    create_color_target(&self.device, *format, width, height);
                *texture = new_texture;
                *view = new_view;
            }
        }
    }

    fn rebuild_frame_inputs(&mut self) {
        let device = &self.device;
        let depth = &self.depth_sampled;
        let color = self.scene_color.as_ref();
        let layouts = &self.frame_input_layouts;
        self.frame_input_groups = std::array::from_fn(|index| {
            layouts
                .get(index)
                .and_then(|layout| create_frame_input_group(device, layout, index, depth, color))
        });
    }

    pub fn upload_mesh(&mut self, mesh: &MeshData) -> Result<MeshHandle> {
        if mesh.vertices.is_empty() || mesh.indices.is_empty() {
            return Err(Error::EmptyMesh);
        }
        if let Some(at) = mesh.validate() {
            return Err(Error::BadIndex {
                at,
                vertices: mesh.vertices.len(),
            });
        }
        let index_count = u32::try_from(mesh.indices.len()).map_err(|_| Error::TooManyIndices {
            count: mesh.indices.len(),
        })?;

        let vertices = create_buffer_init(
            &self.device,
            &self.queue,
            "mint vertices",
            wgpu::BufferUsages::VERTEX,
            &mesh.vertex_bytes(),
        );
        let indices = create_buffer_init(
            &self.device,
            &self.queue,
            "mint indices",
            wgpu::BufferUsages::INDEX,
            &mesh.index_bytes(),
        );

        let handle = MeshHandle(self.meshes.len());
        self.meshes.push(GpuMesh {
            vertices,
            indices,
            index_count,
            vertex_count: mesh.vertices.len(),
            skin: None,
        });
        Ok(handle)
    }

    pub fn upload_skinned_mesh(
        &mut self,
        mesh: &MeshData,
        skin: &[crate::SkinVertex],
    ) -> Result<MeshHandle> {
        if skin.len() != mesh.vertices.len() {
            return Err(Error::SkinLengthMismatch {
                vertices: mesh.vertices.len(),
                skin: skin.len(),
            });
        }
        let handle = self.upload_mesh(mesh)?;
        let span = skin
            .iter()
            .map(crate::SkinVertex::bone_span)
            .max()
            .unwrap_or(0);
        let buffer = create_buffer_init(
            &self.device,
            &self.queue,
            "mint skin",
            wgpu::BufferUsages::VERTEX,
            &crate::skin::skin_bytes(skin),
        );
        if let Some(gpu) = self.meshes.get_mut(handle.0) {
            gpu.skin = Some((buffer, span));
        }
        Ok(handle)
    }

    pub fn set_bones(&mut self, bones: &[Mat4]) -> Result<()> {
        let needed = u32::try_from(bones.len())
            .map_err(|_| Error::TooManyInstances { count: bones.len() })?;
        let grown = needed > self.bone_capacity;
        if grown {
            let capacity = needed.saturating_mul(2).max(64);
            self.bone_buffer = create_bone_buffer(&self.device, capacity);
            self.bone_capacity = capacity;
            self.uniform_bind_group = create_uniform_bind_group(
                &self.device,
                &self.uniform_layout,
                &self.uniform_buffer,
                (self.bound_shadow_view(), &self.shadow_sampler),
                &self.bone_buffer,
                &self.frame_uniform_buffer,
                self.bound_scene_textures(),
            );
            self.shadow_uniform_group = create_shadow_uniform_group(
                &self.device,
                &self.shadow_uniform_layout,
                &self.shadow_instance_buffer,
                &self.shadow_view_proj_buffer,
                &self.bone_buffer,
            );
        }
        if grown {
            let mut scratch = std::mem::take(&mut self.bone_scratch);
            scratch.clear();
            scratch.reserve(bones.len().saturating_mul(crate::skin::BONE_SIZE));
            for bone in bones {
                crate::skin::write_bone(bone, &mut scratch);
            }
            self.queue.write_buffer(&self.bone_buffer, 0, &scratch);
            self.bone_scratch = scratch;
        } else {
            self.frame.write_buffer_with(
                &self.device,
                &self.queue,
                &self.bone_buffer,
                0,
                bones.len().saturating_mul(crate::skin::BONE_SIZE),
                |palette| {
                    let (slots, _) = palette.into_chunks::<{ crate::skin::BONE_SIZE }>();
                    for (slot, bone) in slots.into_iter().zip(bones) {
                        slot.write(crate::skin::packed_bone(bone));
                    }
                },
            );
        }
        self.bone_count = bones.len();
        Ok(())
    }

    fn resolve_materials(
        &mut self,
        instances: &[Instance],
        drawn: &[usize],
    ) -> Result<Vec<MaterialSlot>> {
        for instance in instances {
            self.check_bones(instance)?;
            for handle in [instance.texture, instance.normal_map, instance.texture2]
                .into_iter()
                .flatten()
            {
                self.textures.get(handle.0).ok_or(Error::UnknownHandle)?;
            }
        }
        let mut materials = vec![0; instances.len()];
        let mut last: Option<(MaterialKey, MaterialSlot)> = None;
        for &index in drawn {
            let Some(instance) = instances.get(index) else {
                continue;
            };
            let key: MaterialKey = (
                instance.texture.map(|h| h.0),
                instance.normal_map.map(|h| h.0),
                instance.texture2.map(|h| h.0),
            );
            let slot = match last {
                Some((previous, slot)) if previous == key => slot,
                _ => {
                    let slot = self.ensure_material(
                        instance.texture,
                        instance.normal_map,
                        instance.texture2,
                    )?;
                    last = Some((key, slot));
                    slot
                }
            };
            if let Some(entry) = materials.get_mut(index) {
                *entry = slot;
            }
        }
        Ok(materials)
    }

    fn check_bones(&self, instance: &Instance) -> Result<()> {
        let Some((_, span)) = self
            .meshes
            .get(instance.mesh.0)
            .and_then(|m| m.skin.as_ref())
        else {
            return Ok(());
        };
        let base = instance.bone_base;
        let end = u64::from(base.unwrap_or(0)).saturating_add(u64::from(*span));
        let fits = u64::try_from(self.bone_count).is_ok_and(|count| end <= count);
        if base.is_none() || !fits || base.is_some_and(|b| b >= MAX_BONE_BASE) {
            return Err(Error::BoneRangeOutOfBounds {
                base: base.unwrap_or(0),
                end,
                palette: self.bone_count,
            });
        }
        if instance.custom_material.is_some() || instance.custom_shader.is_some() {
            return Err(Error::SkinnedCustomShader);
        }
        Ok(())
    }

    pub fn update_mesh_vertices(&mut self, mesh: MeshHandle, data: &MeshData) -> Result<()> {
        let gpu = self.meshes.get(mesh.0).ok_or(Error::UnknownHandle)?;
        if data.vertices.len() != gpu.vertex_count {
            return Err(Error::VertexCountMismatch {
                original: gpu.vertex_count,
                supplied: data.vertices.len(),
            });
        }
        self.frame.write_buffer(
            &self.device,
            &self.queue,
            &gpu.vertices,
            0,
            &data.vertex_bytes(),
        );
        Ok(())
    }

    pub fn update_mesh_vertex_prefix(&mut self, mesh: MeshHandle, data: &MeshData) -> Result<()> {
        let gpu = self.meshes.get(mesh.0).ok_or(Error::UnknownHandle)?;
        if data.vertices.len() > gpu.vertex_count {
            return Err(Error::VertexCountMismatch {
                original: gpu.vertex_count,
                supplied: data.vertices.len(),
            });
        }
        if !data.vertices.is_empty() {
            self.frame.write_buffer(
                &self.device,
                &self.queue,
                &gpu.vertices,
                0,
                &data.vertex_bytes(),
            );
        }
        Ok(())
    }

    pub fn upload_texture(&mut self, texture: &TextureData) -> Result<TextureHandle> {
        let uploaded = upload_texture(
            &self.device,
            &self.queue,
            texture,
            self.info.block_compression,
        )?;
        let handle = TextureHandle(self.textures.len());
        self.textures.push(uploaded);
        Ok(handle)
    }

    pub fn update_texture(&mut self, handle: TextureHandle, data: &TextureData) -> Result<()> {
        let gpu = self.textures.get(handle.0).ok_or(Error::UnknownHandle)?;
        let (width, height) = (data.width(), data.height());
        if gpu.format != Some(data.format) || width != gpu.width || height != gpu.height {
            return Err(Error::TextureShapeMismatch {
                width: gpu.width,
                height: gpu.height,
                supplied_width: width,
                supplied_height: height,
            });
        }
        let level = data.levels.first().ok_or(Error::EmptyTexture)?;
        let bytes_per_row = data
            .format
            .bytes_per_row(level.width)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or(Error::EmptyTexture)?;
        let expected = data
            .format
            .level_size(level.width, level.height)
            .ok_or(Error::EmptyTexture)?;
        if level.data.len() != expected {
            return Err(Error::TextureShapeMismatch {
                width: gpu.width,
                height: gpu.height,
                supplied_width: width,
                supplied_height: height,
            });
        }
        let rows = level
            .height
            .checked_add(data.format.block_edge().saturating_sub(1))
            .and_then(|n| n.checked_div(data.format.block_edge()))
            .ok_or(Error::EmptyTexture)?;
        self.frame.flush(&self.queue);
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &gpu.texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &level.data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(rows),
            },
            wgpu::Extent3d {
                width: level.width,
                height: level.height,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    }

    pub fn zeroed_texture(
        &mut self,
        width: u32,
        height: u32,
        format: PixelFormat,
    ) -> Result<TextureHandle> {
        if format.is_block_compressed() {
            return Err(Error::BlockCompressionUnsupported);
        }
        if width == 0 || height == 0 {
            return Err(Error::EmptyTexture);
        }
        let texture = new_texture(
            &self.device,
            (width, height),
            format,
            1,
            (AddressMode::Repeat, AddressMode::Repeat),
        )?;
        let handle = TextureHandle(self.textures.len());
        self.textures.push(texture);
        Ok(handle)
    }

    pub fn update_texture_rows(
        &mut self,
        handle: TextureHandle,
        first_row: u32,
        rows: &[u8],
    ) -> Result<()> {
        let gpu = self.textures.get(handle.0).ok_or(Error::UnknownHandle)?;
        let mismatch = Error::TextureShapeMismatch {
            width: gpu.width,
            height: gpu.height,
            supplied_width: gpu.width,
            supplied_height: 0,
        };
        let single_level = gpu.texture.mip_level_count() == 1;
        let Some(format) = gpu
            .format
            .filter(|format| format.block_edge() == 1 && single_level)
        else {
            return Err(mismatch);
        };
        let Some(bytes_per_row) = format.bytes_per_row(gpu.width).filter(|&n| n > 0) else {
            return Err(mismatch);
        };
        let count = rows.len().checked_div(bytes_per_row).unwrap_or(0);
        let fits = rows.len().checked_rem(bytes_per_row) == Some(0)
            && u32::try_from(count)
                .ok()
                .and_then(|count| first_row.checked_add(count))
                .is_some_and(|end| end <= gpu.height);
        let (Ok(count), Ok(bytes_per_row), true) =
            (u32::try_from(count), u32::try_from(bytes_per_row), fits)
        else {
            return Err(mismatch);
        };
        if count == 0 {
            return Ok(());
        }
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &gpu.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: first_row,
                    z: 0,
                },
                aspect: wgpu::TextureAspect::All,
            },
            rows,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(count),
            },
            wgpu::Extent3d {
                width: gpu.width,
                height: count,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    }

    pub fn upload_color_volume(&mut self, edge: u32, rgba: &[u8]) -> Result<ColorVolumeHandle> {
        self.post
            .upload_volume(&self.device, &self.queue, edge, rgba)
    }

    fn ensure_material(
        &mut self,
        diffuse: Option<TextureHandle>,
        normal: Option<TextureHandle>,
        stage2: Option<TextureHandle>,
    ) -> Result<MaterialSlot> {
        let key: MaterialKey = (
            diffuse.map(|h| h.0),
            normal.map(|h| h.0),
            stage2.map(|h| h.0),
        );
        for id in [key.0, key.1, key.2].into_iter().flatten() {
            self.textures.get(id).ok_or(Error::UnknownHandle)?;
        }
        if let Some(slot) = self.materials.get(&key) {
            return Ok(*slot);
        }
        let bind_group = self.material_bind_group(key)?;
        let slot = self.material_groups.len();
        self.material_groups.push(bind_group);
        self.materials.insert(key, slot);
        Ok(slot)
    }

    fn material_bind_group(&self, key: MaterialKey) -> Result<wgpu::BindGroup> {
        let (diffuse_view, sampler) = match key.0 {
            Some(id) => {
                let texture = self.textures.get(id).ok_or(Error::UnknownHandle)?;
                (
                    &texture.view,
                    self.samplers.get(texture.address_u, texture.address_v),
                )
            }
            None => (
                &self.blank_texture.view,
                self.samplers.get(AddressMode::Repeat, AddressMode::Repeat),
            ),
        };
        let normal_view = match key.1 {
            Some(id) => &self.textures.get(id).ok_or(Error::UnknownHandle)?.view,
            None => &self.flat_normal_texture.view,
        };
        let (stage2_view, stage2_sampler) = match key.2 {
            Some(id) => {
                let texture = self.textures.get(id).ok_or(Error::UnknownHandle)?;
                (
                    &texture.view,
                    self.samplers.get(texture.address_u, texture.address_v),
                )
            }
            None => (
                &self.blank_texture.view,
                self.samplers.get(AddressMode::Repeat, AddressMode::Repeat),
            ),
        };
        Ok(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mint material"),
            layout: &self.material_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(diffuse_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(normal_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(stage2_view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(stage2_sampler),
                },
            ],
        }))
    }

    fn texture_dimensions(&self, handle: TextureHandle) -> Result<(u32, u32)> {
        let texture = self.textures.get(handle.0).ok_or(Error::UnknownHandle)?;
        Ok((texture.width, texture.height))
    }

    fn ui_texture_material(
        &mut self,
        handle: TextureHandle,
        last: &mut Option<UiTextureMaterial>,
    ) -> Result<(MaterialSlot, (u32, u32))> {
        if let Some(UiTextureMaterial {
            handle: previous,
            slot,
            size,
        }) = *last
        {
            if previous == handle {
                return Ok((slot, size));
            }
        }
        let size = self.texture_dimensions(handle)?;
        if size.0 == 0 || size.1 == 0 {
            return Err(Error::EmptyTexture);
        }
        let slot = self.ensure_material(Some(handle), None, None)?;
        *last = Some(UiTextureMaterial { handle, slot, size });
        Ok((slot, size))
    }

    pub fn register_custom_shader(
        &mut self,
        shader: &CustomShader<'_>,
    ) -> Result<CustomShaderHandle> {
        if shader.texture_slots > MAX_CUSTOM_TEXTURE_SLOTS {
            return Err(Error::TooManyTextureSlots {
                asked: shader.texture_slots,
                limit: MAX_CUSTOM_TEXTURE_SLOTS,
            });
        }
        if shader.scene_texture_slots > MAX_CUSTOM_TEXTURE_SLOTS {
            return Err(Error::TooManyTextureSlots {
                asked: shader.scene_texture_slots,
                limit: MAX_CUSTOM_TEXTURE_SLOTS,
            });
        }
        if shader.uniform_size > MAX_CUSTOM_UNIFORM_SIZE {
            return Err(Error::CustomUniformTooLarge {
                asked: shader.uniform_size,
                limit: MAX_CUSTOM_UNIFORM_SIZE,
            });
        }
        if shader.scene_uniform_size > MAX_CUSTOM_UNIFORM_SIZE {
            return Err(Error::CustomUniformTooLarge {
                asked: shader.scene_uniform_size,
                limit: MAX_CUSTOM_UNIFORM_SIZE,
            });
        }
        let builtin_material = shader.material == CustomMaterialSource::Builtin;
        if builtin_material && (shader.texture_slots != 0 || shader.uniform_size != 0) {
            return Err(Error::CustomMaterialSourceMismatch {
                label: shader.label.to_owned(),
                problem: "takes mint's own material at bind group 1, so it may declare no texture slots and no uniform block of its own",
            });
        }
        let uniform_size =
            aligned_uniform_size(shader.uniform_size).ok_or(Error::CustomUniformTooLarge {
                asked: shader.uniform_size,
                limit: MAX_CUSTOM_UNIFORM_SIZE,
            })?;
        let scene_uniform_size = aligned_uniform_size(shader.scene_uniform_size).ok_or(
            Error::CustomUniformTooLarge {
                asked: shader.scene_uniform_size,
                limit: MAX_CUSTOM_UNIFORM_SIZE,
            },
        )?;

        let layout = self
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("mint custom material"),
                entries: &custom_group_entries(uniform_size, shader.texture_slots),
            });
        let scene_layout = if scene_uniform_size == 0 && shader.scene_texture_slots == 0 {
            None
        } else {
            Some(
                self.device
                    .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                        label: Some("mint custom scene"),
                        entries: &custom_group_entries(
                            scene_uniform_size,
                            shader.scene_texture_slots,
                        ),
                    }),
            )
        };
        let frame_inputs = frame_input_index(shader.scene_depth, shader.scene_color);
        if shader.scene_color {
            if !self.can_capture_scene_color {
                return Err(Error::SceneColorUnsupported);
            }
            if self.scene_color.is_none() {
                self.scene_color = Some(create_scene_color(
                    &self.device,
                    self.color_format,
                    self.width,
                    self.height,
                ));
                self.rebuild_frame_inputs();
            }
        }
        let frame_layout = self
            .frame_input_layouts
            .get(frame_inputs)
            .ok_or(Error::UnknownHandle)?;
        let group1 = if builtin_material {
            &self.material_layout
        } else {
            &layout
        };
        let mut groups: Vec<Option<&wgpu::BindGroupLayout>> =
            vec![Some(&self.uniform_layout), Some(group1)];
        if frame_inputs != 0 {
            groups.push(Some(frame_layout));
        } else if scene_layout.is_some() {
            groups.push(None);
        }
        if let Some(scene) = scene_layout.as_ref() {
            groups.push(Some(scene));
        }
        let pipeline_layout = self
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("mint custom pipeline layout"),
                bind_group_layouts: &groups,
                immediate_size: 0,
            });

        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = self
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("mint custom"),
                source: wgpu::ShaderSource::Wgsl(
                    format!(
                        "{}{}{}{}",
                        crate::CUSTOM_SHADER_PRELUDE,
                        if shader.scene_depth {
                            crate::SCENE_DEPTH_PRELUDE
                        } else {
                            ""
                        },
                        if shader.scene_color {
                            crate::SCENE_COLOR_PRELUDE
                        } else {
                            ""
                        },
                        shader.source
                    )
                    .into(),
                ),
            });
        let write_mask = if shader.writes_alpha {
            wgpu::ColorWrites::ALL
        } else {
            wgpu::ColorWrites::COLOR
        };
        if let Some(error) = pop_scope(scope, &self.device)? {
            return Err(Error::CustomShaderRejected {
                label: shader.label.to_owned(),
                message: error.to_string(),
            });
        }
        let device = &self.device;
        let color_format = self.color_format;
        let cache = self.pipeline_cache.as_ref().map(|disk| &disk.cache);
        let keys: Vec<ColorKey> = [Some(wgpu::Face::Back), None]
            .into_iter()
            .flat_map(|cull_mode| grid_keys(cull_mode, false, &GRID_BLENDS))
            .collect();
        let built = mint_pool::map_ordered(&keys, |key| {
            let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let pipeline = build_pipeline_masked(
                device,
                cache,
                &pipeline_layout,
                &module,
                color_format,
                *key,
                write_mask,
            );
            (pipeline, pop_scope(scope, device))
        });
        let mut pipelines = Vec::with_capacity(built.len());
        let mut first_error = None;
        for (pipeline, verdict) in built {
            if first_error.is_none() {
                first_error = verdict?;
            }
            pipelines.push(pipeline);
        }
        if let Some(error) = first_error {
            return Err(Error::CustomShaderRejected {
                label: shader.label.to_owned(),
                message: error.to_string(),
            });
        }
        let mut pipelines = pipelines.into_iter();
        let pipelines_culled = PipelineSet::take(&mut pipelines, BLEND_MODE_COUNT);
        let pipelines_double_sided = PipelineSet::take(&mut pipelines, BLEND_MODE_COUNT);

        let index = self.custom_shaders.len();
        self.custom_shaders.push(CustomShaderEntry {
            layout,
            pipelines_culled,
            pipelines_double_sided,
            texture_slots: usize::try_from(shader.texture_slots).unwrap_or(0),
            uniform_size: usize::try_from(uniform_size).unwrap_or(0),
            scene_color: shader.scene_color,
            frame_inputs,
            label: shader.label.to_owned(),
            builtin_material,
            scene_layout,
            scene_buffer: None,
            scene_group: None,
            scene_textures: vec![None; usize::try_from(shader.scene_texture_slots).unwrap_or(0)],
            scene_uniform_size: usize::try_from(scene_uniform_size).unwrap_or(0),
            scene_texture_slots: usize::try_from(shader.scene_texture_slots).unwrap_or(0),
        });
        self.rebuild_custom_scene(index)?;
        Ok(CustomShaderHandle(index))
    }

    pub fn create_custom_material(
        &mut self,
        shader: CustomShaderHandle,
        textures: &[Option<TextureHandle>],
        uniforms: &[u8],
    ) -> Result<CustomMaterialHandle> {
        let entry = self
            .custom_shaders
            .get(shader.0)
            .ok_or(Error::UnknownHandle)?;
        if entry.builtin_material {
            return Err(Error::CustomMaterialSourceMismatch {
                label: entry.label.clone(),
                problem: "takes mint's own material at bind group 1, so it has no material of its own to create - name it on `Instance::custom_shader`",
            });
        }
        if textures.len() != entry.texture_slots {
            return Err(Error::CustomMaterialMismatch {
                what: "texture slots",
                declared: entry.texture_slots,
                supplied: textures.len(),
            });
        }
        if uniforms.len() != entry.uniform_size {
            return Err(Error::CustomMaterialMismatch {
                what: "uniform bytes",
                declared: entry.uniform_size,
                supplied: uniforms.len(),
            });
        }

        let buffer = if uniforms.is_empty() {
            None
        } else {
            Some(create_buffer_init(
                &self.device,
                &self.queue,
                "mint custom material uniforms",
                wgpu::BufferUsages::UNIFORM,
                uniforms,
            ))
        };

        let slots: Vec<Option<usize>> = textures.iter().map(|t| t.map(|h| h.0)).collect();
        let bind_group =
            self.build_custom_material_group(&entry.layout, buffer.as_ref(), &slots)?;

        let index = self.custom_materials.len();
        self.custom_materials.push(CustomMaterialEntry {
            shader: shader.0,
            buffer,
            bind_group,
            textures: slots,
        });
        Ok(CustomMaterialHandle(index))
    }

    fn build_custom_material_group(
        &self,
        layout: &wgpu::BindGroupLayout,
        buffer: Option<&wgpu::Buffer>,
        slots: &[Option<usize>],
    ) -> Result<wgpu::BindGroup> {
        let mut views = Vec::with_capacity(slots.len());
        for slot in slots {
            match slot {
                Some(index) => {
                    let texture = self.textures.get(*index).ok_or(Error::UnknownHandle)?;
                    views.push((
                        &texture.view,
                        self.samplers.get(texture.address_u, texture.address_v),
                    ));
                }
                None => views.push((
                    &self.blank_texture.view,
                    self.samplers.get(AddressMode::Repeat, AddressMode::Repeat),
                )),
            }
        }

        let mut bindings: Vec<wgpu::BindGroupEntry<'_>> = Vec::new();
        if let Some(buffer) = buffer {
            bindings.push(wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            });
        }
        for (slot, (view, sampler)) in views.iter().enumerate() {
            let binding = u32::try_from(slot)
                .ok()
                .and_then(|slot| slot.checked_mul(2))
                .and_then(|slot| slot.checked_add(1))
                .ok_or(Error::UnknownHandle)?;
            bindings.push(wgpu::BindGroupEntry {
                binding,
                resource: wgpu::BindingResource::TextureView(view),
            });
            bindings.push(wgpu::BindGroupEntry {
                binding: binding.saturating_add(1),
                resource: wgpu::BindingResource::Sampler(sampler),
            });
        }
        Ok(self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("mint custom material"),
            layout,
            entries: &bindings,
        }))
    }

    pub fn set_custom_scene(
        &mut self,
        shader: CustomShaderHandle,
        textures: &[Option<TextureHandle>],
        uniforms: &[u8],
    ) -> Result<()> {
        let entry = self
            .custom_shaders
            .get(shader.0)
            .ok_or(Error::UnknownHandle)?;
        if textures.len() != entry.scene_texture_slots {
            return Err(Error::CustomMaterialMismatch {
                what: "scene texture slots",
                declared: entry.scene_texture_slots,
                supplied: textures.len(),
            });
        }
        if uniforms.len() != entry.scene_uniform_size {
            return Err(Error::CustomMaterialMismatch {
                what: "scene uniform bytes",
                declared: entry.scene_uniform_size,
                supplied: uniforms.len(),
            });
        }
        let slots: Vec<Option<usize>> = textures.iter().map(|t| t.map(|h| h.0)).collect();
        if let Some(entry) = self.custom_shaders.get_mut(shader.0) {
            entry.scene_textures = slots;
        }
        self.rebuild_custom_scene(shader.0)?;
        self.update_custom_scene(shader, uniforms)
    }

    pub fn update_custom_scene(
        &mut self,
        shader: CustomShaderHandle,
        uniforms: &[u8],
    ) -> Result<()> {
        let entry = self
            .custom_shaders
            .get(shader.0)
            .ok_or(Error::UnknownHandle)?;
        if uniforms.len() != entry.scene_uniform_size {
            return Err(Error::CustomMaterialMismatch {
                what: "scene uniform bytes",
                declared: entry.scene_uniform_size,
                supplied: uniforms.len(),
            });
        }
        if let Some(buffer) = &entry.scene_buffer {
            self.frame
                .write_buffer(&self.device, &self.queue, buffer, 0, uniforms);
        }
        Ok(())
    }

    fn rebuild_custom_scene(&mut self, index: usize) -> Result<()> {
        let entry = self.custom_shaders.get(index).ok_or(Error::UnknownHandle)?;
        if entry.scene_layout.is_none() {
            return Ok(());
        }
        let size = entry.scene_uniform_size;
        if entry.scene_buffer.is_none() && size > 0 {
            let buffer = create_buffer_init(
                &self.device,
                &self.queue,
                "mint custom scene uniforms",
                wgpu::BufferUsages::UNIFORM,
                &vec![0u8; size],
            );
            if let Some(entry) = self.custom_shaders.get_mut(index) {
                entry.scene_buffer = Some(buffer);
            }
        }
        let entry = self.custom_shaders.get(index).ok_or(Error::UnknownHandle)?;
        let Some(layout) = entry.scene_layout.as_ref() else {
            return Ok(());
        };
        let group = self.build_custom_material_group(
            layout,
            entry.scene_buffer.as_ref(),
            &entry.scene_textures,
        )?;
        if let Some(entry) = self.custom_shaders.get_mut(index) {
            entry.scene_group = Some(group);
        }
        Ok(())
    }

    pub fn update_custom_material(
        &mut self,
        material: CustomMaterialHandle,
        uniforms: &[u8],
    ) -> Result<()> {
        let entry = self
            .custom_materials
            .get(material.0)
            .ok_or(Error::UnknownHandle)?;
        let shader = self
            .custom_shaders
            .get(entry.shader)
            .ok_or(Error::UnknownHandle)?;
        if uniforms.len() != shader.uniform_size {
            return Err(Error::CustomMaterialMismatch {
                what: "uniform bytes",
                declared: shader.uniform_size,
                supplied: uniforms.len(),
            });
        }
        if let Some(buffer) = &entry.buffer {
            self.frame
                .write_buffer(&self.device, &self.queue, buffer, 0, uniforms);
        }
        Ok(())
    }

    pub fn create_render_target(&mut self, width: u32, height: u32) -> Result<RenderTargetHandle> {
        let (width, height) = clamp_size(width, height);
        let limit = self.device.limits().max_texture_dimension_2d;
        if width > limit || height > limit {
            return Err(Error::TextureTooLarge {
                width,
                height,
                limit,
            });
        }
        let (texture, view) =
            create_render_target_color(&self.device, self.color_format, width, height);
        let (depth, _) = create_depth_views(&self.device, width, height);
        let texture_index = self.textures.len();
        self.textures.push(GpuTexture {
            texture: texture.clone(),
            format: None,
            view: view.clone(),
            address_u: AddressMode::Clamp,
            address_v: AddressMode::Clamp,
            width,
            height,
        });
        let index = self.render_targets.len();
        self.render_targets.push(RenderTarget {
            texture,
            view,
            depth,
            width,
            height,
            texture_index,
        });
        Ok(RenderTargetHandle(index))
    }

    fn bound_scene_textures(&self) -> [(&wgpu::TextureView, &wgpu::Sampler); 2] {
        let view = |bound: Option<TextureHandle>| {
            bound
                .and_then(|handle| self.textures.get(handle.0))
                .map_or(&self.cookie_placeholder.view, |texture| &texture.view)
        };
        [
            (view(self.bound_cookie), &self.cookie_sampler),
            (view(self.bound_mask), &self.mask_sampler),
        ]
    }

    fn bound_shadow_view(&self) -> &wgpu::TextureView {
        self.bound_shadow_map
            .and_then(|index| self.shadow_maps.get(index))
            .map_or(&self.shadow_placeholder, |entry| &entry.view)
    }

    pub fn create_shadow_map(&mut self, size: u32) -> Result<ShadowMapHandle> {
        self.create_shadow_cascades(size, 1)
    }

    pub fn create_shadow_cascades(&mut self, size: u32, cascades: u32) -> Result<ShadowMapHandle> {
        let size = size.clamp(1, 16_384);
        let cascades = cascades.clamp(1, u32::try_from(MAX_SHADOW_CASCADES).unwrap_or(1));
        let limit = self.device.limits().max_texture_dimension_2d;
        if size > limit {
            return Err(Error::TextureTooLarge {
                width: size,
                height: size,
                limit,
            });
        }
        let (view, layers) = create_shadow_texture(&self.device, size, cascades);
        let index = self.shadow_maps.len();
        self.shadow_maps.push(ShadowMapEntry { view, layers, size });
        Ok(ShadowMapHandle(index))
    }

    #[must_use]
    pub fn shadow_map_size(&self, map: ShadowMapHandle) -> Option<u32> {
        self.shadow_maps.get(map.0).map(|entry| entry.size)
    }

    #[must_use]
    pub fn shadow_map_cascades(&self, map: ShadowMapHandle) -> Option<usize> {
        self.shadow_maps.get(map.0).map(|entry| entry.layers.len())
    }

    pub fn render_shadow_map(
        &mut self,
        map: ShadowMapHandle,
        view_proj: Mat4,
        scene: &Scene<'_>,
    ) -> Result<()> {
        self.render_shadow_cascade(map, 0, view_proj, scene)
    }

    pub fn render_shadow_cascade(
        &mut self,
        map: ShadowMapHandle,
        cascade: usize,
        view_proj: Mat4,
        scene: &Scene<'_>,
    ) -> Result<()> {
        if self
            .shadow_maps
            .get(map.0)
            .and_then(|entry| entry.layers.get(cascade))
            .is_none()
        {
            return Err(Error::UnknownHandle);
        }
        let frustum = Frustum::from_view_proj(view_proj);
        let mut casters = 0usize;
        let surviving: Vec<usize> = (0..scene.instances.len())
            .filter(|index| {
                scene.instances.get(*index).is_some_and(|instance| {
                    if !(instance.visible
                        && instance.casts_shadow
                        && instance.blend == Blend::Opaque)
                    {
                        return false;
                    }
                    casters = casters.saturating_add(1);
                    self.in_frustum(instance, &frustum)
                })
            })
            .collect();
        self.shadow_culled_instances =
            u32::try_from(casters.saturating_sub(surviving.len())).unwrap_or(u32::MAX);

        let alpha = self.shadow_alpha_test;
        let alpha_tested = |index: &usize| {
            alpha
                && scene
                    .instances
                    .get(*index)
                    .is_some_and(|i| i.alpha_cutoff > 0.0 && i.texture.is_some())
        };
        let mut order: Vec<usize> = surviving
            .iter()
            .copied()
            .filter(|i| !alpha_tested(i))
            .collect();
        let opaque_count = order.len();
        order.extend(surviving.iter().copied().filter(alpha_tested));

        let mut materials: Vec<MaterialSlot> = vec![0; scene.instances.len()];
        for index in order.iter().skip(opaque_count) {
            let diffuse = scene.instances.get(*index).and_then(|i| i.texture);
            let slot = self.ensure_material(diffuse, None, None)?;
            if let Some(entry) = materials.get_mut(*index) {
                *entry = slot;
            }
        }

        let sort_key = |index: &usize| {
            scene
                .instances
                .get(*index)
                .map_or((usize::MAX, usize::MAX, false), |i| {
                    (
                        materials.get(*index).copied().unwrap_or(0),
                        i.mesh.0,
                        self.cull_override.unwrap_or(i.cull),
                    )
                })
        };
        if let Some(head) = order.get_mut(..opaque_count) {
            head.sort_by_key(sort_key);
        }
        if let Some(tail) = order.get_mut(opaque_count..) {
            tail.sort_by_key(sort_key);
        }
        for index in &order {
            if let Some(instance) = scene.instances.get(*index) {
                self.check_bones(instance)?;
            }
        }
        let region = self.write_shadow_uniforms(scene, &order, view_proj)?;
        let opaque_slice = order.get(..opaque_count).unwrap_or(&[]);
        let alpha_slice = order.get(opaque_count..).unwrap_or(&[]);
        let batches = batch_draws(
            scene.instances,
            opaque_slice,
            &[],
            None,
            BatchScope::MeshOnly,
        );
        let alpha_batches = batch_draws(
            scene.instances,
            alpha_slice,
            &materials,
            None,
            BatchScope::MeshAndMaterial,
        );

        let mut encoder = self.frame.take(&self.device);
        let recorded: Result<()> = 'record: {
            let Some(layer) = self
                .shadow_maps
                .get(map.0)
                .and_then(|entry| entry.layers.get(cascade))
            else {
                break 'record Err(Error::UnknownHandle);
            };
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mint shadow pass"),
                color_attachments: &[],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: layer,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.shadow_uniform_group, &[region.slot_offset]);
            let mut bound: Option<(bool, bool, bool)> = None;
            let mut bound_material: Option<MaterialSlot> = None;
            for (list, offset, alpha) in [
                (&batches, 0usize, false),
                (&alpha_batches, opaque_count, true),
            ] {
                for batch in list {
                    let Some(instance) = scene.instances.get(batch.lead) else {
                        continue;
                    };
                    let Some(mesh) = self.meshes.get(instance.mesh.0) else {
                        break 'record Err(Error::UnknownHandle);
                    };
                    let cull = self.cull_override.unwrap_or(instance.cull);
                    let skin = mesh.skin.as_ref().map(|(buffer, _)| buffer);
                    if bound != Some((cull, alpha, skin.is_some())) {
                        let key = ShadowKey {
                            alpha,
                            skinned: skin.is_some(),
                            culled: cull,
                        };
                        let Some(pipeline) = self.shadow_pipelines.get(shadow_index(key)) else {
                            continue;
                        };
                        pass.set_pipeline(pipeline);
                        bound = Some((cull, alpha, skin.is_some()));
                    }
                    if let Some(skin) = skin {
                        pass.set_vertex_buffer(1, skin.slice(..));
                    }
                    if alpha && bound_material != Some(batch.material) {
                        let Some(group) = self.material_groups.get(batch.material) else {
                            break 'record Err(Error::UnknownHandle);
                        };
                        pass.set_bind_group(1, group, &[]);
                        bound_material = Some(batch.material);
                    }
                    let range = u32::try_from(batch.first.saturating_add(offset))
                        .ok()
                        .and_then(|first| first.checked_add(region.base))
                        .and_then(|first| Some(first..first.checked_add(batch.count)?));
                    let Some(range) = range else {
                        break 'record Err(Error::TooManyInstances { count: order.len() });
                    };
                    pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                    pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..mesh.index_count, 0, range);
                }
            }
            Ok(())
        };
        self.frame.keep(encoder);
        recorded?;
        self.shadow_draw_calls =
            u32::try_from(batches.len().saturating_add(alpha_batches.len())).unwrap_or(u32::MAX);
        self.shadow_drawn_instances = u32::try_from(order.len()).unwrap_or(u32::MAX);
        self.shadow_alpha_instances = u32::try_from(alpha_slice.len()).unwrap_or(u32::MAX);
        self.shadow_alpha_draw_calls = u32::try_from(alpha_batches.len()).unwrap_or(u32::MAX);

        if self.bound_shadow_map != Some(map.0) {
            self.bound_shadow_map = Some(map.0);
            self.rebuild_uniform_group();
        }
        Ok(())
    }

    #[must_use]
    pub fn render_target_texture(&self, target: RenderTargetHandle) -> Option<TextureHandle> {
        self.render_targets
            .get(target.0)
            .map(|t| TextureHandle(t.texture_index))
    }

    #[must_use]
    pub fn render_target_size(&self, target: RenderTargetHandle) -> Option<(u32, u32)> {
        self.render_targets
            .get(target.0)
            .map(|t| (t.width, t.height))
    }

    pub fn resize_render_target(
        &mut self,
        target: RenderTargetHandle,
        width: u32,
        height: u32,
    ) -> Result<()> {
        let (width, height) = clamp_size(width, height);
        let limit = self.device.limits().max_texture_dimension_2d;
        if width > limit || height > limit {
            return Err(Error::TextureTooLarge {
                width,
                height,
                limit,
            });
        }
        let Some(entry) = self.render_targets.get(target.0) else {
            return Err(Error::UnknownHandle);
        };
        if entry.width == width && entry.height == height {
            return Ok(());
        }
        let texture_index = entry.texture_index;
        let (texture, view) =
            create_render_target_color(&self.device, self.color_format, width, height);
        let (depth, _) = create_depth_views(&self.device, width, height);
        if let Some(slot) = self.textures.get_mut(texture_index) {
            slot.view = view.clone();
        }
        if let Some(entry) = self.render_targets.get_mut(target.0) {
            entry.texture = texture;
            entry.view = view;
            entry.depth = depth;
            entry.width = width;
            entry.height = height;
        }

        let stale: Vec<(MaterialKey, MaterialSlot)> = self
            .materials
            .iter()
            .filter(|(key, _)| [key.0, key.1, key.2].contains(&Some(texture_index)))
            .map(|(key, slot)| (*key, *slot))
            .collect();
        for (key, slot) in stale {
            let group = self.material_bind_group(key)?;
            if let Some(entry) = self.material_groups.get_mut(slot) {
                *entry = group;
            }
        }

        let mut rebuilt: Vec<(usize, wgpu::BindGroup)> = Vec::new();
        for (index, material) in self.custom_materials.iter().enumerate() {
            if !material.textures.contains(&Some(texture_index)) {
                continue;
            }
            let Some(shader) = self.custom_shaders.get(material.shader) else {
                continue;
            };
            let group = self.build_custom_material_group(
                &shader.layout,
                material.buffer.as_ref(),
                &material.textures,
            )?;
            rebuilt.push((index, group));
        }
        for (index, group) in rebuilt {
            if let Some(material) = self.custom_materials.get_mut(index) {
                material.bind_group = group;
            }
        }
        let stale: Vec<usize> = self
            .custom_shaders
            .iter()
            .enumerate()
            .filter(|(_, shader)| {
                shader.scene_group.is_some() && shader.scene_textures.contains(&Some(texture_index))
            })
            .map(|(index, _)| index)
            .collect();
        for index in stale {
            self.rebuild_custom_scene(index)?;
        }
        Ok(())
    }

    pub fn render_to_target(
        &mut self,
        target: RenderTargetHandle,
        scene: &Scene<'_>,
    ) -> Result<()> {
        let (width, height) = self
            .render_targets
            .get(target.0)
            .map(|t| (t.width, t.height))
            .ok_or(Error::UnknownHandle)?;

        let aspect = f64_ratio(width, height);
        let frustum = Frustum::from_view_proj(self.color_view_proj(scene, aspect));
        let before = scene.instances.iter().filter(|i| i.visible).count();
        let draw_order: Vec<usize> =
            sorted_draw_order(scene.instances, scene.camera.eye, |instance| {
                instance.visible && self.in_frustum(instance, &frustum)
            });
        self.target_culled_instances =
            u32::try_from(before.saturating_sub(draw_order.len())).unwrap_or(u32::MAX);
        let region = self.write_uniforms(scene, &draw_order, aspect)?;

        let materials = self.resolve_materials(scene.instances, &draw_order)?;

        let batches = batch_draws(
            scene.instances,
            &draw_order,
            &materials,
            self.cull_override,
            BatchScope::Surface,
        );
        let split = batches
            .iter()
            .position(|batch| !is_opaque_lead(scene.instances, batch))
            .unwrap_or(batches.len());
        let (opaque_batches, blended_batches) = batches.split_at(split);

        let Some(entry) = self.render_targets.get(target.0) else {
            return Err(Error::UnknownHandle);
        };
        let mut target_calls = 0u32;
        let mut encoder = self.frame.take(&self.device);
        let recorded: Result<()> = 'record: {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mint target opaque pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &entry.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: scene.clear_color[0],
                            g: scene.clear_color[1],
                            b: scene.clear_color[2],
                            a: scene.clear_color[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &entry.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.depth_direction.clear_value()),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            match self.record_batches(
                &mut pass,
                scene,
                opaque_batches,
                draw_order.len(),
                true,
                false,
                region,
            ) {
                Ok(calls) => target_calls = target_calls.saturating_add(calls),
                Err(error) => break 'record Err(error),
            }
            drop(pass);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mint target blended pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &entry.view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &entry.depth,
                    depth_ops: None,
                    stencil_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0),
                        store: wgpu::StoreOp::Discard,
                    }),
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_stencil_reference(MERGE_STENCIL);
            match self.record_batches(
                &mut pass,
                scene,
                blended_batches,
                draw_order.len(),
                false,
                false,
                region,
            ) {
                Ok(calls) => target_calls = target_calls.saturating_add(calls),
                Err(error) => break 'record Err(error),
            }
            Ok(())
        };
        self.frame.keep(encoder);
        recorded?;
        self.target_draw_calls = target_calls;
        self.target_drawn_instances = u32::try_from(draw_order.len()).unwrap_or(u32::MAX);
        Ok(())
    }

    pub fn render(&mut self, scene: &Scene<'_>) -> Result<()> {
        let aspect = self.aspect_ratio();
        let frustum = Frustum::from_view_proj(self.color_view_proj(scene, aspect));
        let before = scene.instances.iter().filter(|i| i.visible).count();
        let draw_order: Vec<usize> =
            sorted_draw_order(scene.instances, scene.camera.eye, |instance| {
                instance.visible && self.in_frustum(instance, &frustum)
            });
        self.culled_instances =
            u32::try_from(before.saturating_sub(draw_order.len())).unwrap_or(u32::MAX);
        let region = self.write_uniforms(scene, &draw_order, aspect)?;

        let materials = self.resolve_materials(scene.instances, &draw_order)?;
        if let Some(fade) = &scene.screen_fade {
            self.fade_pass.write(&self.queue, fade);
        }
        if !scene.ui_quads.is_empty()
            || !scene.ui_geometry.is_empty()
            || !scene.ordered_screen.is_empty()
        {
            self.queue.write_buffer(
                &self.ui_viewport_buffer,
                0,
                &ui_viewport_bytes(self.width, self.height),
            );
        }
        let ui_materials = self.prepare_ui_pass(scene)?;
        let UiGeometryPassOutput {
            runs: ui_geometry_runs,
            push_mask_ranges,
            pop_mask_ranges,
        } = self.prepare_ui_geometry_pass(scene)?;
        let flat_quad_materials = ui_materials
            .get(..scene.ui_quads.len())
            .unwrap_or(&ui_materials);
        let ordered_quad_materials = ui_materials.get(scene.ui_quads.len()..).unwrap_or(&[]);
        let flat_geometry_run_count = scene.ui_geometry.len().min(ui_geometry_runs.len());
        let (flat_geometry_runs, ordered_geometry_runs) =
            ui_geometry_runs.split_at(flat_geometry_run_count);
        let quad_base = u32::try_from(scene.ui_quads.len()).unwrap_or(u32::MAX);
        let ordered_draws = build_ordered_screen_draws(
            scene.ordered_screen,
            ordered_quad_materials,
            ordered_geometry_runs,
            &push_mask_ranges,
            &pop_mask_ranges,
            quad_base,
        );

        let acquire = std::time::Instant::now();
        let acquired = match &self.target {
            Target::Window { surface, .. } => Some(surface.get_current_texture()),
            Target::Offscreen { .. } => None,
        };
        self.present_wait = if acquired.is_some() {
            acquire.elapsed()
        } else {
            std::time::Duration::ZERO
        };
        let frame = match acquired {
            Some(acquired) => match acquired {
                wgpu::CurrentSurfaceTexture::Success(texture)
                | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => Some(texture),
                wgpu::CurrentSurfaceTexture::Timeout
                | wgpu::CurrentSurfaceTexture::Occluded
                | wgpu::CurrentSurfaceTexture::Validation => {
                    self.frame.submit_open(&self.queue);
                    self.frame.end_frame();
                    return Ok(());
                }
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                    self.frame.submit_open(&self.queue);
                    self.frame.end_frame();
                    let (width, height) = (self.width, self.height);
                    self.width = 0;
                    self.resize(width, height);
                    return Ok(());
                }
            },
            None => None,
        };

        let frame_view = frame.as_ref().map(|frame| {
            let format = match &self.target {
                Target::Window { view_format, .. } => Some(*view_format),
                Target::Offscreen { .. } => None,
            };
            frame.texture.create_view(&wgpu::TextureViewDescriptor {
                format,
                ..Default::default()
            })
        });
        let color_view = match (&frame_view, &self.target) {
            (Some(view), _) => view,
            (None, Target::Offscreen { view, .. }) => view,
            (None, Target::Window { .. }) => return Err(Error::SurfaceUnsupported),
        };

        let mut encoder = self.frame.take(&self.device);

        let batches = batch_draws(
            scene.instances,
            &draw_order,
            &materials,
            self.cull_override,
            BatchScope::Surface,
        );
        let split = batches
            .iter()
            .position(|batch| !is_opaque_lead(scene.instances, batch))
            .unwrap_or(batches.len());
        let (opaque_batches, blended_batches) = batches.split_at(split);
        let merge_end = blended_batches
            .iter()
            .position(|batch| {
                scene
                    .instances
                    .get(batch.lead)
                    .is_none_or(|instance| !instance.blend.merges())
            })
            .unwrap_or(blended_batches.len());
        let (merge_batches, blended_batches) = blended_batches.split_at(merge_end);

        let consumes_scene_color = |batch: &Batch| {
            scene
                .instances
                .get(batch.lead)
                .and_then(|instance| self.custom_shader_for(instance))
                .is_some_and(|shader| shader.scene_color)
        };
        let mut decal_batches: Vec<Batch> = Vec::new();
        let mut reading_batches: Vec<Batch> = Vec::new();
        for batch in blended_batches {
            if consumes_scene_color(batch) {
                reading_batches.push(*batch);
            } else {
                decal_batches.push(*batch);
            }
        }
        let snapshot_after_decals = !reading_batches.is_empty() && !decal_batches.is_empty();

        let mut draw_calls = 0u32;
        let recorded: Result<()> = 'record: {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("mint opaque pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: scene.clear_color[0],
                            g: scene.clear_color[1],
                            b: scene.clear_color[2],
                            a: scene.clear_color[3],
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(self.depth_direction.clear_value()),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            match self.record_batches(
                &mut pass,
                scene,
                opaque_batches,
                draw_order.len(),
                true,
                true,
                region,
            ) {
                Ok(calls) => draw_calls = draw_calls.saturating_add(calls),
                Err(error) => break 'record Err(error),
            }
            drop(pass);
            if let (Some(contact), Some(Some(depth_group))) = (
                scene.contact_shadows,
                self.frame_input_groups.get(frame_input_index(true, false)),
            ) {
                if contact.steps > 0 && contact.length > 0.0 {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("mint contact shadows"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: color_view,
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
                    pass.set_pipeline(&self.contact_shadow_pipeline);
                    pass.set_bind_group(0, &self.uniform_bind_group, &[region.slot_offset]);
                    pass.set_bind_group(2, depth_group, &[]);
                    pass.draw(0..3, 0..1);
                    draw_calls = draw_calls.saturating_add(1);
                }
            }
            if !merge_batches.is_empty() {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("mint merge pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: color_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth,
                        depth_ops: None,
                        stencil_ops: Some(wgpu::Operations {
                            load: wgpu::LoadOp::Clear(0),
                            store: wgpu::StoreOp::Discard,
                        }),
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                pass.set_stencil_reference(MERGE_STENCIL);
                match self.record_batches(
                    &mut pass,
                    scene,
                    merge_batches,
                    draw_order.len(),
                    false,
                    false,
                    region,
                ) {
                    Ok(calls) => draw_calls = draw_calls.saturating_add(calls),
                    Err(error) => break 'record Err(error),
                }
            }
            if snapshot_after_decals {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("mint decal pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: color_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth,
                        depth_ops: None,
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                match self.record_batches(
                    &mut pass,
                    scene,
                    &decal_batches,
                    draw_order.len(),
                    false,
                    true,
                    region,
                ) {
                    Ok(calls) => draw_calls = draw_calls.saturating_add(calls),
                    Err(error) => break 'record Err(error),
                }
            }
            if let Some(color) = &self.scene_color {
                let wanted = !reading_batches.is_empty();
                let source = match (&frame, &self.target) {
                    (Some(frame), _) => Some(&frame.texture),
                    (None, Target::Offscreen { texture, .. }) => Some(texture),
                    (None, Target::Window { .. }) => None,
                };
                if let (true, Some(source)) = (wanted, source) {
                    encoder.copy_texture_to_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: source,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::TexelCopyTextureInfo {
                            texture: &color.texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        wgpu::Extent3d {
                            width: self.width,
                            height: self.height,
                            depth_or_array_layers: 1,
                        },
                    );
                }
            }
            {
                let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("mint blended pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: color_view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                        view: &self.depth,
                        depth_ops: None,
                        stencil_ops: None,
                    }),
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                let rest: &[Batch] = if snapshot_after_decals {
                    &reading_batches
                } else {
                    blended_batches
                };
                match self.record_batches(
                    &mut pass,
                    scene,
                    rest,
                    draw_order.len(),
                    false,
                    true,
                    region,
                ) {
                    Ok(calls) => draw_calls = draw_calls.saturating_add(calls),
                    Err(error) => break 'record Err(error),
                }
            }
            if scene.post.glow.is_some() || scene.post.lut.is_some() {
                let source = match (&frame, &self.target) {
                    (Some(frame), _) => Some(&frame.texture),
                    (None, Target::Offscreen { texture, .. }) => Some(texture),
                    (None, Target::Window { .. }) => None,
                };
                if let Some(source) = source {
                    if let Err(error) = self.post.record(
                        &self.device,
                        self.pipeline_cache.as_ref().map(|disk| &disk.cache),
                        &self.queue,
                        &mut encoder,
                        source,
                        color_view,
                        self.color_format,
                        (self.width, self.height),
                        &scene.post,
                    ) {
                        break 'record Err(error);
                    }
                }
            }
            match self.record_ui_pass(
                &mut encoder,
                color_view,
                flat_quad_materials,
                flat_geometry_runs,
                &ordered_draws,
                scene.screen_fade.map(|fade| fade.blend),
            ) {
                Ok(calls) => draw_calls = draw_calls.saturating_add(calls),
                Err(error) => break 'record Err(error),
            }
            Ok(())
        };
        self.frame.submit(&self.queue, encoder);
        if let Err(error) = recorded {
            self.frame.end_frame();
            return Err(error);
        }
        self.draw_calls = draw_calls;
        self.drawn_instances = u32::try_from(draw_order.len()).unwrap_or(u32::MAX);
        if std::mem::take(&mut self.capture_next) {
            let copy = match (&frame, &self.target) {
                (Some(frame), Target::Window { config, .. }) => {
                    self.read_texture(&frame.texture).map(|mut pixels| {
                        if matches!(
                            config.format,
                            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
                        ) {
                            for texel in pixels.chunks_exact_mut(4) {
                                texel.swap(0, 2);
                            }
                        }
                        pixels
                    })
                }
                _ => self.read_pixels(),
            };
            self.captured = copy.ok().map(|pixels| (self.width, self.height, pixels));
        }
        self.frame.end_frame();
        if let Some(frame) = frame {
            drop(frame_view);
            let present = std::time::Instant::now();
            self.queue.present(frame);
            self.present_wait = self.present_wait.saturating_add(present.elapsed());
        }
        Ok(())
    }

    fn record_batches(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        scene: &Scene<'_>,
        batches: &[Batch],
        drawn: usize,
        opaque: bool,
        frame_inputs: bool,
        region: frame::PassRegion,
    ) -> Result<u32> {
        if batches.is_empty() {
            return Ok(0);
        }
        pass.set_bind_group(0, &self.uniform_bind_group, &[region.slot_offset]);

        let mut draw_calls = 0u32;
        let mut bound_scene: Option<Option<usize>> = None;
        let mut bound_material: Option<(Option<usize>, MaterialSlot)> = None;
        let mut bound_shader: Option<Option<usize>> = None;
        for batch in batches {
            let Some(instance) = scene.instances.get(batch.lead) else {
                continue;
            };
            let mesh = self
                .meshes
                .get(instance.mesh.0)
                .ok_or(Error::UnknownHandle)?;
            let custom = match (instance.custom_material, instance.custom_shader) {
                (None, None) => None,
                (Some(_), Some(_)) => {
                    let label = instance
                        .custom_shader
                        .and_then(|handle| self.custom_shaders.get(handle.0))
                        .map_or_else(String::new, |shader| shader.label.clone());
                    return Err(Error::CustomMaterialSourceMismatch {
                        label,
                        problem: "was named on an instance that also names a custom material - one instance, one bind group 1",
                    });
                }
                (Some(handle), None) => {
                    let material = self
                        .custom_materials
                        .get(handle.0)
                        .ok_or(Error::UnknownHandle)?;
                    let shader = self
                        .custom_shaders
                        .get(material.shader)
                        .ok_or(Error::UnknownHandle)?;
                    Some((Some(material), shader, material.shader))
                }
                (None, Some(handle)) => {
                    let shader = self
                        .custom_shaders
                        .get(handle.0)
                        .ok_or(Error::UnknownHandle)?;
                    if !shader.builtin_material {
                        return Err(Error::CustomMaterialSourceMismatch {
                            label: shader.label.clone(),
                            problem: "declares a bind group 1 of its own, so it must be named through a material rather than on `Instance::custom_shader`",
                        });
                    }
                    Some((None, shader, handle.0))
                }
            };
            let material = match custom {
                Some((Some(material), _, _)) => &material.bind_group,
                _ => self
                    .material_groups
                    .get(batch.material)
                    .ok_or(Error::UnknownHandle)?,
            };
            let first = u32::try_from(batch.first)
                .ok()
                .and_then(|first| first.checked_add(region.base))
                .ok_or(Error::TooManyInstances { count: drawn })?;
            let last = first
                .checked_add(batch.count)
                .ok_or(Error::TooManyInstances { count: drawn })?;
            let cull = self.cull_override.unwrap_or(instance.cull);
            let skin = mesh.skin.as_ref().map(|(buffer, _)| buffer);
            let (culled, double_sided) = match (custom, skin) {
                (_, Some(_)) => (
                    &self.pipelines_skinned_culled,
                    &self.pipelines_skinned_double_sided,
                ),
                (Some((_, shader, _)), None) => {
                    (&shader.pipelines_culled, &shader.pipelines_double_sided)
                }
                (None, None) => (&self.pipelines_culled, &self.pipelines_double_sided),
            };
            let pipelines = if cull { culled } else { double_sided };
            let pipeline = match (instance.blend.merge_index(), custom, skin) {
                (Some(part), None, None) => self
                    .merge_pipelines
                    .get(usize::from(!cull))
                    .and_then(|set| set.get(self.depth_direction, instance.depth_test, part)),
                (Some(_), _, _) => None,
                (None, _, _) => pipelines.get(
                    self.depth_direction,
                    instance.depth_test,
                    instance.blend.index(),
                ),
            };
            let Some(pipeline) = pipeline else {
                continue;
            };

            pass.set_pipeline(pipeline);
            let shader_id = custom.map(|(_, _, index)| index);
            if bound_shader != Some(shader_id) {
                bound_material = None;
                bound_scene = None;
                bound_shader = Some(shader_id);
            }
            let material_key = (
                instance.custom_material.map(|handle| handle.0),
                batch.material,
            );
            if bound_material != Some(material_key) {
                pass.set_bind_group(1, material, &[]);
                bound_material = Some(material_key);
            }
            if let Some((_, shader, _)) = custom.filter(|(_, shader, _)| shader.frame_inputs != 0) {
                if opaque {
                    return Err(Error::SceneDepthWritesDepth {
                        label: shader.label.clone(),
                    });
                }
                if !frame_inputs {
                    return Err(Error::FrameInputsOffTarget {
                        label: shader.label.clone(),
                    });
                }
                let Some(Some(group)) = self.frame_input_groups.get(shader.frame_inputs) else {
                    continue;
                };
                pass.set_bind_group(2, group, &[]);
            }
            if bound_scene != Some(shader_id) {
                if let Some(group) = custom.and_then(|(_, shader, _)| shader.scene_group.as_ref()) {
                    pass.set_bind_group(3, group, &[]);
                }
                bound_scene = Some(shader_id);
            }
            pass.set_vertex_buffer(0, mesh.vertices.slice(..));
            if let Some(skin) = skin {
                pass.set_vertex_buffer(1, skin.slice(..));
            }
            pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.index_count, 0, first..last);
            draw_calls = draw_calls.saturating_add(1);
        }
        Ok(draw_calls)
    }

    fn prepare_ui_pass(&mut self, scene: &Scene<'_>) -> Result<Vec<MaterialSlot>> {
        let ordered_quads = scene.ordered_screen.iter().filter_map(|item| match item {
            ScreenDraw::Quad(quad) => Some(quad),
            ScreenDraw::Geometry(_) | ScreenDraw::PushMask(_) | ScreenDraw::PopMask(_) => None,
        });
        let total = scene.ui_quads.len().saturating_add(
            scene
                .ordered_screen
                .iter()
                .filter(|item| matches!(item, ScreenDraw::Quad(_)))
                .count(),
        );
        if total == 0 {
            return Ok(Vec::new());
        }
        let needed = u32::try_from(total).map_err(|_| Error::TooManyInstances { count: total })?;
        if needed > self.ui_instance_capacity {
            let capacity = needed.saturating_mul(2).max(64);
            self.ui_instance_buffer = create_ui_instance_buffer(&self.device, capacity);
            self.ui_bind_group = create_ui_bind_group(
                &self.device,
                &self.ui_uniform_layout,
                &self.ui_instance_buffer,
                &self.ui_viewport_buffer,
            );
            self.ui_instance_capacity = capacity;
        }

        let mut materials = Vec::with_capacity(total);
        let mut last: Option<(Option<TextureHandle>, MaterialSlot)> = None;
        for quad in scene.ui_quads.iter().chain(ordered_quads.clone()) {
            let slot = match last {
                Some((texture, slot)) if texture == quad.texture => slot,
                _ => {
                    let slot = self.ensure_material(quad.texture, None, None)?;
                    last = Some((quad.texture, slot));
                    slot
                }
            };
            materials.push(slot);
        }

        let mut scratch = std::mem::take(&mut self.ui_scratch);
        scratch.clear();
        for quad in scene.ui_quads.iter().chain(ordered_quads) {
            write_ui_instance(quad, &mut scratch);
        }
        self.queue
            .write_buffer(&self.ui_instance_buffer, 0, &scratch);
        self.ui_scratch = scratch;

        Ok(materials)
    }

    fn prepare_ui_geometry_pass(&mut self, scene: &Scene<'_>) -> Result<UiGeometryPassOutput> {
        let mut triangle_scratch = std::mem::take(&mut self.ui_geometry_triangle_scratch);
        let mut line_scratch = std::mem::take(&mut self.ui_geometry_line_scratch);
        let mut textured_scratch = std::mem::take(&mut self.ui_geometry_textured_scratch);
        let mut transform_scratch =
            std::mem::take(&mut self.ui_geometry_textured_transform_scratch);
        triangle_scratch.clear();
        line_scratch.clear();
        textured_scratch.clear();
        transform_scratch.clear();

        let ordered_geometry = scene.ordered_screen.iter().filter_map(|item| match item {
            ScreenDraw::Geometry(geometry) => Some(geometry),
            ScreenDraw::Quad(_) | ScreenDraw::PushMask(_) | ScreenDraw::PopMask(_) => None,
        });
        let entries: Vec<&UiGeometry<'_>> =
            scene.ui_geometry.iter().chain(ordered_geometry).collect();
        let retain = self.ui_retained.plan(&self.device, &entries);
        let mut runs = Vec::with_capacity(entries.len());
        let mut triangle_vertices = 0usize;
        let mut line_vertices = 0usize;
        let mut textured_vertices = 0usize;
        let mut transform_count = 0usize;
        let mut last_texture: Option<UiTextureMaterial> = None;
        for geometry in entries {
            if retain && geometry.retain_key.is_some() {
                let material = match geometry.texture {
                    Some(handle) if !geometry.triangles.is_empty() => {
                        Some(self.ui_texture_material(handle, &mut last_texture)?)
                    }
                    _ => None,
                };
                if let Some(run) = self.ui_retained.add(geometry, material)? {
                    if !run.is_empty() {
                        runs.push(UiGeometryRun {
                            triangle_range: None,
                            line_range: None,
                            textured: None,
                            retained: Some(run),
                        });
                    }
                    continue;
                }
            }
            let textured = match geometry.texture {
                Some(handle) if !geometry.triangles.is_empty() => {
                    let (material, (width, height)) =
                        self.ui_texture_material(handle, &mut last_texture)?;
                    let textured_first = textured_vertices;
                    for corner in geometry.triangles.chunks_exact(3).flatten() {
                        write_geometry_vertex(
                            *corner,
                            geometry.color,
                            geometry.clip,
                            geometry.color_add,
                            &mut textured_scratch,
                        );
                        textured_vertices = textured_vertices.saturating_add(1);
                    }
                    let vertex_range = ui_geometry_vertex_range(textured_first, textured_vertices)?;
                    let transform_index =
                        u32::try_from(transform_count).map_err(|_| Error::TooManyInstances {
                            count: transform_count,
                        })?;
                    write_ui_geometry_textured_transform(
                        geometry,
                        width,
                        height,
                        &mut transform_scratch,
                    );
                    transform_count = transform_count.saturating_add(1);
                    Some(TexturedTriangleRun {
                        vertex_range,
                        transform_index,
                        material,
                    })
                }
                _ => None,
            };

            let triangle_first = triangle_vertices;
            if textured.is_none() {
                for corner in geometry.triangles.chunks_exact(3).flatten() {
                    let screen =
                        transform_ui_geometry_point(geometry.affine, geometry.translation, *corner);
                    write_geometry_vertex(
                        screen,
                        geometry.color,
                        geometry.clip,
                        geometry.color_add,
                        &mut triangle_scratch,
                    );
                    triangle_vertices = triangle_vertices.saturating_add(1);
                }
            }
            let line_first = line_vertices;
            for endpoint in geometry.lines.chunks_exact(2).flatten() {
                let screen =
                    transform_ui_geometry_point(geometry.affine, geometry.translation, *endpoint);
                write_geometry_vertex(
                    screen,
                    geometry.color,
                    geometry.clip,
                    geometry.color_add,
                    &mut line_scratch,
                );
                line_vertices = line_vertices.saturating_add(1);
            }
            let triangle_range = if triangle_vertices > triangle_first {
                Some(ui_geometry_vertex_range(triangle_first, triangle_vertices)?)
            } else {
                None
            };
            let line_range = if line_vertices > line_first {
                Some(ui_geometry_vertex_range(line_first, line_vertices)?)
            } else {
                None
            };
            if triangle_range.is_some() || line_range.is_some() || textured.is_some() {
                runs.push(UiGeometryRun {
                    triangle_range,
                    line_range,
                    textured,
                    retained: None,
                });
            }
        }

        let mut push_mask_ranges = Vec::new();
        let mut pop_mask_ranges = Vec::new();
        for item in scene.ordered_screen {
            let (shape, ranges) = match item {
                ScreenDraw::PushMask(shape) => (shape, &mut push_mask_ranges),
                ScreenDraw::PopMask(shape) => (shape, &mut pop_mask_ranges),
                ScreenDraw::Quad(_) | ScreenDraw::Geometry(_) => continue,
            };
            let first = triangle_vertices;
            for corner in shape.triangles.chunks_exact(3).flatten() {
                let screen = transform_ui_geometry_point(shape.affine, shape.translation, *corner);
                write_geometry_vertex(
                    screen,
                    [1.0, 1.0, 1.0, 1.0],
                    UiQuad::UNCLIPPED,
                    [0.0, 0.0, 0.0, 0.0],
                    &mut triangle_scratch,
                );
                triangle_vertices = triangle_vertices.saturating_add(1);
            }
            ranges.push(ui_geometry_vertex_range(first, triangle_vertices)?);
        }

        let triangle_count =
            u32::try_from(triangle_vertices).map_err(|_| Error::TooManyInstances {
                count: triangle_vertices,
            })?;
        let line_count = u32::try_from(line_vertices).map_err(|_| Error::TooManyInstances {
            count: line_vertices,
        })?;
        let textured_count =
            u32::try_from(textured_vertices).map_err(|_| Error::TooManyInstances {
                count: textured_vertices,
            })?;
        let transform_capacity_needed =
            u32::try_from(transform_count).map_err(|_| Error::TooManyInstances {
                count: transform_count,
            })?;

        if triangle_count > self.ui_geometry_triangle_capacity {
            let capacity = triangle_count.saturating_mul(2).max(64);
            self.ui_geometry_triangle_buffer = create_ui_geometry_buffer(&self.device, capacity);
            self.ui_geometry_triangle_capacity = capacity;
        }
        if line_count > self.ui_geometry_line_capacity {
            let capacity = line_count.saturating_mul(2).max(64);
            self.ui_geometry_line_buffer = create_ui_geometry_buffer(&self.device, capacity);
            self.ui_geometry_line_capacity = capacity;
        }
        if textured_count > self.ui_geometry_textured_capacity {
            let capacity = textured_count.saturating_mul(2).max(64);
            self.ui_geometry_textured_buffer = create_ui_geometry_buffer(&self.device, capacity);
            self.ui_geometry_textured_capacity = capacity;
        }
        if transform_capacity_needed > self.ui_geometry_textured_transform_capacity {
            let capacity = transform_capacity_needed.saturating_mul(2).max(16);
            self.ui_geometry_textured_transform_buffer =
                create_ui_geometry_textured_transform_buffer(&self.device, capacity);
            self.ui_geometry_textured_transform_bind_group =
                create_ui_geometry_textured_transform_bind_group(
                    &self.device,
                    &self.ui_geometry_textured_transform_layout,
                    &self.ui_geometry_textured_transform_buffer,
                );
            self.ui_geometry_textured_transform_capacity = capacity;
        }
        if retain {
            self.ui_retained.flush(&self.device, &self.queue);
        }
        if !triangle_scratch.is_empty() {
            self.queue
                .write_buffer(&self.ui_geometry_triangle_buffer, 0, &triangle_scratch);
        }
        if !line_scratch.is_empty() {
            self.queue
                .write_buffer(&self.ui_geometry_line_buffer, 0, &line_scratch);
        }
        if !textured_scratch.is_empty() {
            self.queue
                .write_buffer(&self.ui_geometry_textured_buffer, 0, &textured_scratch);
        }
        if !transform_scratch.is_empty() {
            self.queue.write_buffer(
                &self.ui_geometry_textured_transform_buffer,
                0,
                &transform_scratch,
            );
        }

        self.ui_geometry_triangle_scratch = triangle_scratch;
        self.ui_geometry_line_scratch = line_scratch;
        self.ui_geometry_textured_scratch = textured_scratch;
        self.ui_geometry_textured_transform_scratch = transform_scratch;
        Ok(UiGeometryPassOutput {
            runs,
            push_mask_ranges,
            pop_mask_ranges,
        })
    }

    fn record_geometry_run(&self, pass: &mut wgpu::RenderPass<'_>, run: &UiGeometryRun) -> u32 {
        if let Some(retained) = &run.retained {
            return self.record_retained_run(pass, retained);
        }
        let mut draw_calls = 0u32;
        if let Some((first, count)) = run.triangle_range {
            pass.set_pipeline(&self.ui_geometry_triangle_pipeline);
            pass.set_bind_group(0, &self.ui_geometry_bind_group, &[]);
            pass.set_vertex_buffer(0, self.ui_geometry_triangle_buffer.slice(..));
            pass.draw(first..first.saturating_add(count), 0..1);
            draw_calls = draw_calls.saturating_add(1);
        }
        if let Some((first, count)) = run.line_range {
            pass.set_pipeline(&self.ui_geometry_line_pipeline);
            pass.set_bind_group(0, &self.ui_geometry_bind_group, &[]);
            pass.set_vertex_buffer(0, self.ui_geometry_line_buffer.slice(..));
            pass.draw(first..first.saturating_add(count), 0..1);
            draw_calls = draw_calls.saturating_add(1);
        }
        if let Some(textured) = &run.textured {
            let Some(group) = self.material_groups.get(textured.material) else {
                return draw_calls;
            };
            pass.set_pipeline(&self.ui_geometry_textured_pipeline);
            pass.set_bind_group(0, &self.ui_geometry_bind_group, &[]);
            pass.set_bind_group(1, group, &[]);
            pass.set_bind_group(2, &self.ui_geometry_textured_transform_bind_group, &[]);
            pass.set_vertex_buffer(0, self.ui_geometry_textured_buffer.slice(..));
            let (first, count) = textured.vertex_range;
            let index = textured.transform_index;
            pass.draw(
                first..first.saturating_add(count),
                index..index.saturating_add(1),
            );
            draw_calls = draw_calls.saturating_add(1);
        }
        draw_calls
    }

    fn record_quad_run(&self, pass: &mut wgpu::RenderPass<'_>, run: &UiRun) -> u32 {
        let Some(group) = self.material_groups.get(run.material) else {
            return 0;
        };
        pass.set_pipeline(&self.ui_pipeline);
        pass.set_bind_group(0, &self.ui_bind_group, &[]);
        pass.set_bind_group(1, group, &[]);
        pass.draw(0..6, run.first..run.first.saturating_add(run.count));
        1
    }

    fn record_mask_run(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        pipeline: &wgpu::RenderPipeline,
        range: (u32, u32),
    ) -> u32 {
        let (first, count) = range;
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.ui_geometry_bind_group, &[]);
        pass.set_vertex_buffer(0, self.ui_geometry_triangle_buffer.slice(..));
        pass.draw(first..first.saturating_add(count), 0..1);
        1
    }

    fn record_ui_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        color_view: &wgpu::TextureView,
        materials: &[MaterialSlot],
        geometry_runs: &[UiGeometryRun],
        ordered_draws: &[OrderedScreenDraw],
        fade: Option<FadeBlend>,
    ) -> Result<u32> {
        if materials.is_empty()
            && geometry_runs.is_empty()
            && ordered_draws.is_empty()
            && fade.is_none()
        {
            return Ok(0);
        }
        let runs = ui_runs(materials)?;
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("mint screen pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: color_view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.screen_stencil,
                depth_ops: None,
                stencil_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(0),
                    store: wgpu::StoreOp::Discard,
                }),
            }),
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_stencil_reference(0);
        let mut draw_calls = 0u32;
        if let Some(blend) = fade {
            draw_calls = draw_calls.saturating_add(self.fade_pass.record(&mut pass, blend));
        }
        for run in geometry_runs {
            draw_calls = draw_calls.saturating_add(self.record_geometry_run(&mut pass, run));
        }
        if !materials.is_empty() {
            for run in &runs {
                draw_calls = draw_calls.saturating_add(self.record_quad_run(&mut pass, run));
            }
        }
        for draw in ordered_draws {
            draw_calls = draw_calls.saturating_add(match draw {
                OrderedScreenDraw::QuadRun { run, depth } => {
                    pass.set_stencil_reference(*depth);
                    self.record_quad_run(&mut pass, run)
                }
                OrderedScreenDraw::Geometry { run, depth } => {
                    pass.set_stencil_reference(*depth);
                    self.record_geometry_run(&mut pass, run)
                }
                OrderedScreenDraw::PushMask { range, reference } => {
                    pass.set_stencil_reference(*reference);
                    self.record_mask_run(&mut pass, &self.ui_mask_push_pipeline, *range)
                }
                OrderedScreenDraw::PopMask { range, reference } => {
                    pass.set_stencil_reference(*reference);
                    self.record_mask_run(&mut pass, &self.ui_mask_pop_pipeline, *range)
                }
            });
        }
        Ok(draw_calls)
    }

    pub fn read_pixels(&self) -> Result<Vec<u8>> {
        let Target::Offscreen { texture, .. } = &self.target else {
            return Err(Error::NotOffscreen);
        };
        self.read_texture(texture)
    }

    pub fn capture_next_frame(&mut self) {
        self.capture_next = true;
    }

    pub fn take_capture(&mut self) -> Option<(u32, u32, Vec<u8>)> {
        self.captured.take()
    }

    fn read_texture(&self, texture: &wgpu::Texture) -> Result<Vec<u8>> {
        let width = usize::try_from(self.width).map_err(|_| Error::NotOffscreen)?;
        let height = usize::try_from(self.height).map_err(|_| Error::NotOffscreen)?;
        let unpadded = width.checked_mul(4).ok_or(Error::NotOffscreen)?;
        let padded = align_up(unpadded, COPY_ROW_ALIGNMENT).ok_or(Error::NotOffscreen)?;
        let total = padded.checked_mul(height).ok_or(Error::NotOffscreen)?;

        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("mint readback"),
            size: u64::try_from(total).unwrap_or(u64::MAX),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("mint readback"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &staging,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(u32::try_from(padded).unwrap_or(u32::MAX)),
                    rows_per_image: Some(self.height),
                },
            },
            wgpu::Extent3d {
                width: self.width,
                height: self.height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(std::iter::once(encoder.finish()));

        let (sender, receiver) = std::sync::mpsc::channel();
        staging.map_async(wgpu::MapMode::Read, .., move |result| {
            let _ = sender.send(result.is_ok());
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| Error::backend("waiting for the readback copy", e))?;
        match receiver.recv() {
            Ok(true) => {}
            Ok(false) => return Err(Error::ReadbackFailed),
            Err(_) => return Err(Error::ReadbackFailed),
        }

        let mut pixels = Vec::with_capacity(unpadded.saturating_mul(height));
        {
            let view = staging
                .get_mapped_range(..)
                .map_err(|e| Error::backend("mapping the readback buffer", e))?;
            for row in 0..height {
                let start = row.checked_mul(padded).ok_or(Error::ReadbackFailed)?;
                let end = start.checked_add(unpadded).ok_or(Error::ReadbackFailed)?;
                pixels.extend_from_slice(view.get(start..end).ok_or(Error::ReadbackFailed)?);
            }
        }
        staging.unmap();
        Ok(pixels)
    }

    pub fn wait_idle(&self) -> Result<()> {
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map(|_| ())
            .map_err(|e| Error::backend("waiting for the queue to drain", e))
    }

    fn write_shadow_uniforms(
        &mut self,
        scene: &Scene<'_>,
        order: &[usize],
        view_proj: Mat4,
    ) -> Result<frame::PassRegion> {
        let count = order.len();
        let needed = u32::try_from(count).map_err(|_| Error::TooManyInstances { count })?;
        let base = match self.frame.casters.claim(needed) {
            Some(base) => base,
            None => {
                let capacity = self.frame.casters.grown(needed);
                self.shadow_instance_buffer = create_shadow_instance_buffer(&self.device, capacity);
                self.frame.casters.restart(capacity);
                self.rebuild_shadow_uniform_group();
                self.frame
                    .casters
                    .claim(needed)
                    .ok_or(Error::TooManyInstances { count })?
            }
        };
        let slot = match self.frame.light_slots.claim(1) {
            Some(slot) => slot,
            None => {
                let slots = self.frame.light_slots.grown(1);
                self.shadow_view_proj_buffer = create_slot_buffer(
                    &self.device,
                    "mint shadow view-projection",
                    LIGHT_SLOT_STRIDE,
                    slots,
                );
                self.frame.light_slots.restart(slots);
                self.rebuild_shadow_uniform_group();
                self.frame
                    .light_slots
                    .claim(1)
                    .ok_or(Error::TooManyInstances { count })?
            }
        };
        let slot_offset = u64::from(slot).saturating_mul(LIGHT_SLOT_STRIDE);
        let mut light = Vec::with_capacity(64);
        for column in view_proj.to_cols() {
            for value in column {
                light.extend_from_slice(&value.to_le_bytes());
            }
        }
        self.queue
            .write_buffer(&self.shadow_view_proj_buffer, slot_offset, &light);

        let mut scratch = std::mem::take(&mut self.shadow_scratch);
        scratch.clear();
        for index in order {
            let instance = scene.instances.get(*index);
            let model = instance.map_or(Mat4::IDENTITY, |instance| instance.transform);
            let linear = instance.map_or([[1.0, 0.0], [0.0, 1.0]], |i| i.uv_linear);
            let offset = instance.map_or([0.0, 0.0], |i| i.uv_offset);
            let cutoff = instance.map_or(0.0, |i| i.alpha_cutoff);
            let shear = instance.map_or([0.0; 4], |i| i.height_shear);
            let bone_base = instance.map_or(0.0, bone_base_f32);
            let [c0, c1, c2, c3] = model.to_cols();
            let floats: [f32; 28] = [
                c0[0],
                c0[1],
                c0[2],
                c0[3],
                c1[0],
                c1[1],
                c1[2],
                c1[3],
                c2[0],
                c2[1],
                c2[2],
                c2[3],
                c3[0],
                c3[1],
                c3[2],
                c3[3],
                linear[0][0],
                linear[0][1],
                linear[1][0],
                linear[1][1],
                offset[0],
                offset[1],
                cutoff,
                bone_base,
                shear[0],
                shear[1],
                shear[2],
                shear[3],
            ];
            let start = scratch.len();
            scratch.resize(start.saturating_add(floats.len().saturating_mul(4)), 0);
            if let Some(block) = scratch.get_mut(start..) {
                for (chunk, value) in block.chunks_exact_mut(4).zip(floats.iter()) {
                    chunk.copy_from_slice(&value.to_le_bytes());
                }
            }
        }
        if !scratch.is_empty() {
            self.queue.write_buffer(
                &self.shadow_instance_buffer,
                u64::from(base).saturating_mul(SHADOW_UNIFORM_SIZE),
                &scratch,
            );
        }
        self.shadow_scratch = scratch;
        Ok(frame::PassRegion {
            base,
            slot_offset: u32::try_from(slot_offset)
                .map_err(|_| Error::TooManyInstances { count })?,
        })
    }

    fn rebuild_shadow_uniform_group(&mut self) {
        self.shadow_uniform_group = create_shadow_uniform_group(
            &self.device,
            &self.shadow_uniform_layout,
            &self.shadow_instance_buffer,
            &self.shadow_view_proj_buffer,
            &self.bone_buffer,
        );
    }

    fn rebuild_uniform_group(&mut self) {
        self.uniform_bind_group = create_uniform_bind_group(
            &self.device,
            &self.uniform_layout,
            &self.uniform_buffer,
            (self.bound_shadow_view(), &self.shadow_sampler),
            &self.bone_buffer,
            &self.frame_uniform_buffer,
            self.bound_scene_textures(),
        );
    }

    fn write_uniforms(
        &mut self,
        scene: &Scene<'_>,
        order: &[usize],
        aspect: f32,
    ) -> Result<frame::PassRegion> {
        let cookie = scene
            .light_cookie
            .map(|c| c.texture)
            .filter(|handle| self.textures.get(handle.0).is_some());
        let mask = scene
            .visibility_mask
            .map(|m| m.texture)
            .filter(|handle| self.textures.get(handle.0).is_some());
        if cookie != self.bound_cookie || mask != self.bound_mask {
            self.bound_cookie = cookie;
            self.bound_mask = mask;
            self.uniform_bind_group = create_uniform_bind_group(
                &self.device,
                &self.uniform_layout,
                &self.uniform_buffer,
                (self.bound_shadow_view(), &self.shadow_sampler),
                &self.bone_buffer,
                &self.frame_uniform_buffer,
                self.bound_scene_textures(),
            );
        }
        let count = order.len();
        let needed = u32::try_from(count).map_err(|_| Error::TooManyInstances { count })?;
        let base = match self.frame.instances.claim(needed) {
            Some(base) => base,
            None => {
                let capacity = self.frame.instances.grown(needed);
                self.uniform_buffer = create_uniform_buffer(&self.device, capacity);
                self.frame.instances.restart(capacity);
                self.rebuild_uniform_group();
                self.frame
                    .instances
                    .claim(needed)
                    .ok_or(Error::TooManyInstances { count })?
            }
        };
        let slot = match self.frame.frame_slots.claim(1) {
            Some(slot) => slot,
            None => {
                let slots = self.frame.frame_slots.grown(1);
                self.frame_uniform_buffer = create_slot_buffer(
                    &self.device,
                    "mint frame uniforms",
                    FRAME_SLOT_STRIDE,
                    slots,
                );
                self.frame.frame_slots.restart(slots);
                self.rebuild_uniform_group();
                self.frame
                    .frame_slots
                    .claim(1)
                    .ok_or(Error::TooManyInstances { count })?
            }
        };
        let slot_offset = u64::from(slot).saturating_mul(FRAME_SLOT_STRIDE);

        let view = scene.camera.view();
        let view_proj = self.color_view_proj(scene, aspect);
        let clip_plane = scene.clip_plane.unwrap_or([0.0; 4]);
        let shadow = scene
            .shadow
            .filter(|sample| self.shadow_maps.get(sample.map.0).is_some());
        let (shadow_cascades, shadow_cascade_params, shadow_params, shadow_filter) =
            shadow_block(shadow, |sample| {
                self.shadow_maps
                    .get(sample.map.0)
                    .map_or((1, 0), |entry| (entry.size, entry.layers.len()))
            });
        let (set_directions, set_colors, set_ambient) = light_set_block(scene);
        let (cookie, cookie_tint) = scene
            .light_cookie
            .map_or(([0.0; 4], [1.0, 1.0, 1.0, 0.0]), |c| {
                (c.transform, [c.tint[0], c.tint[1], c.tint[2], 0.0])
            });
        let eye = scene.camera.eye;
        let [depth_a, depth_b, depth_c] = self
            .depth_direction
            .linearisation(scene.camera.near, scene.camera.far);
        let depth_params = [
            depth_a,
            depth_b,
            depth_c,
            self.depth_direction.clear_value(),
        ];
        let mut light_directions = [[0.0f32; 4]; MAX_DIRECTIONAL_LIGHTS];
        let mut light_colors = [[0.0f32; 4]; MAX_DIRECTIONAL_LIGHTS];
        for (slot, light) in scene.lights.iter().enumerate() {
            if let Some(direction) = light_directions.get_mut(slot) {
                *direction = [light.direction.x, light.direction.y, light.direction.z, 0.0];
            }
            if let Some(color) = light_colors.get_mut(slot) {
                *color = [light.color[0], light.color[1], light.color[2], 0.0];
            }
        }
        let ambient_color = [
            scene.ambient_color[0],
            scene.ambient_color[1],
            scene.ambient_color[2],
            0.0,
        ];
        let frame_uniforms = FrameUniforms {
            view_proj: view_proj.to_cols(),
            view: view.to_cols(),
            camera_eye: [eye.x, eye.y, eye.z, 0.0],
            light_directions,
            light_colors,
            ambient_color,
            depth_params,
            clip_plane,
            shadow_cascades,
            shadow_params,
            shadow_filter,
            shadow_cascade_params,
            set_directions,
            set_colors,
            set_ambient,
            cookie,
            cookie_tint,
            inv_view_proj: view_proj.inverse().unwrap_or(Mat4::IDENTITY).to_cols(),
            contact_shadow: contact_shadow_block(scene.contact_shadows),
            fog: fog_block(scene.fog),
            visibility_mask: scene.visibility_mask.map_or([[0.0; 4]; 2], |mask| {
                [
                    mask.transform,
                    [mask.strength.clamp(0.0, 1.0), 0.0, 0.0, 0.0],
                ]
            }),
        };
        self.queue.write_buffer(
            &self.frame_uniform_buffer,
            slot_offset,
            &frame_uniforms.to_bytes(),
        );
        let mut scratch = std::mem::take(&mut self.uniform_scratch);
        scratch.clear();
        for index in order {
            let Some(instance) = scene.instances.get(*index) else {
                let blank = usize::try_from(UNIFORM_STRIDE).unwrap_or(416);
                scratch.resize(scratch.len().saturating_add(blank), 0);
                continue;
            };
            let [[a, b], [c, d]] = instance.uv_linear;
            let [[a2, b2], [c2, d2]] = instance.uv2_linear;
            let [bump_du, bump_dv] = instance.bump_offset.unwrap_or([[0.0; 2]; 2]);
            let (detail_combine, detail_alpha_combine) = effective_detail_combine(instance);
            let unlit = instance.unlit;
            let uniforms = Uniforms {
                model: instance.transform.to_cols(),
                normal_matrix: instance.transform.normal_matrix().to_cols(),
                tint: instance.tint,
                params: [
                    scene.lit_ceiling.unwrap_or(f32::MAX),
                    instance.alpha_cutoff,
                    if instance.blend == Blend::Opaque {
                        1.0
                    } else {
                        0.0
                    },
                    if unlit { 1.0 } else { 0.0 },
                ],
                uv_linear: [a, b, c, d],
                uv_offset: [
                    instance.uv_offset[0],
                    instance.uv_offset[1],
                    bump_du[0],
                    bump_du[1],
                ],
                material_ambient: [
                    instance.material_ambient[0],
                    instance.material_ambient[1],
                    instance.material_ambient[2],
                    0.0,
                ],
                material_diffuse: [
                    instance.material_diffuse[0],
                    instance.material_diffuse[1],
                    instance.material_diffuse[2],
                    0.0,
                ],
                material_emissive: [
                    instance.material_emissive[0],
                    instance.material_emissive[1],
                    instance.material_emissive[2],
                    0.0,
                ],
                uv2_linear: [a2, b2, c2, d2],
                uv2_offset: [
                    instance.uv2_offset[0],
                    instance.uv2_offset[1],
                    bump_dv[0],
                    bump_dv[1],
                ],
                detail_combine: [
                    small_index_as_f32(detail_combine.index()),
                    small_index_as_f32(detail_alpha_combine.index()),
                    0.0,
                    0.0,
                ],
                house_color: instance.house_color,
                house_skin: instance.house_color_extra[0],
                house_paint: instance.house_color_extra[1],
                env_map: [
                    small_index_as_f32(instance.env_map0.map_or(0, EnvMap::index)),
                    small_index_as_f32(instance.env_map1.map_or(0, EnvMap::index)),
                    fog_mode(instance),
                    match (instance.visibility_mask, instance.visibility_mask_alpha) {
                        (false, _) => 0.0,
                        (true, false) => 1.0,
                        (true, true) => 2.0,
                    },
                ],
                color_source: [
                    if instance.ambient_from_vertex_color {
                        1.0
                    } else {
                        0.0
                    },
                    if instance.diffuse_from_vertex_color {
                        1.0
                    } else {
                        0.0
                    },
                    if instance.dcg_lighting_disabled {
                        1.0
                    } else {
                        0.0
                    },
                    if instance.dcg_multiplies_result {
                        1.0
                    } else {
                        0.0
                    },
                ],
                surface_flags: [
                    if instance.vertex_alpha { 1.0 } else { 0.0 },
                    bone_base_f32(instance),
                    if instance.bump_offset.is_some() {
                        1.0
                    } else {
                        0.0
                    },
                    match instance.modulate2x {
                        Modulate2x::On => 1.0,
                        Modulate2x::WithScene if scene.modulate2x_scene => 1.0,
                        Modulate2x::Off | Modulate2x::WithScene => 0.0,
                    },
                ],
                height_shear: instance.height_shear,
                light_add: [
                    instance.light_add[0],
                    instance.light_add[1],
                    instance.light_add[2],
                    f32::from(instance.light_set.min(LIGHT_SETS)),
                ],
            };
            uniforms.write_into(&mut scratch);
        }
        if !scratch.is_empty() {
            self.queue.write_buffer(
                &self.uniform_buffer,
                u64::from(base).saturating_mul(UNIFORM_STRIDE),
                &scratch,
            );
        }
        self.uniform_scratch = scratch;
        Ok(frame::PassRegion {
            base,
            slot_offset: u32::try_from(slot_offset)
                .map_err(|_| Error::TooManyInstances { count })?,
        })
    }
}

#[allow(clippy::cast_precision_loss)]
fn bone_base_f32(instance: &Instance) -> f32 {
    instance
        .bone_base
        .map_or(0.0, |base| base.min(MAX_BONE_BASE) as f32)
}

fn effective_detail_combine(instance: &Instance) -> (DetailCombine, DetailAlphaCombine) {
    if instance.texture2.is_some() {
        (instance.detail_combine, instance.detail_alpha_combine)
    } else {
        (DetailCombine::Disable, DetailAlphaCombine::Disable)
    }
}

struct Uniforms {
    model: [[f32; 4]; 4],
    normal_matrix: [[f32; 4]; 4],
    tint: [f32; 4],
    params: [f32; 4],
    uv_linear: [f32; 4],
    uv_offset: [f32; 4],
    material_ambient: [f32; 4],
    material_diffuse: [f32; 4],
    material_emissive: [f32; 4],
    uv2_linear: [f32; 4],
    uv2_offset: [f32; 4],
    detail_combine: [f32; 4],
    house_color: [f32; 4],
    house_skin: [f32; 4],
    house_paint: [f32; 4],
    env_map: [f32; 4],
    color_source: [f32; 4],
    surface_flags: [f32; 4],
    height_shear: [f32; 4],
    light_add: [f32; 4],
}

struct FrameUniforms {
    view_proj: [[f32; 4]; 4],
    view: [[f32; 4]; 4],
    camera_eye: [f32; 4],
    light_directions: [[f32; 4]; MAX_DIRECTIONAL_LIGHTS],
    light_colors: [[f32; 4]; MAX_DIRECTIONAL_LIGHTS],
    ambient_color: [f32; 4],
    depth_params: [f32; 4],
    clip_plane: [f32; 4],
    shadow_cascades: [[[f32; 4]; 4]; MAX_SHADOW_CASCADES],
    shadow_params: [f32; 4],
    shadow_filter: [f32; 4],
    shadow_cascade_params: [[f32; 4]; MAX_SHADOW_CASCADES],
    set_directions: [[f32; 4]; SET_LIGHTS],
    set_colors: [[f32; 4]; SET_LIGHTS],
    set_ambient: [[f32; 4]; LIGHT_SET_COUNT],
    cookie: [f32; 4],
    cookie_tint: [f32; 4],
    inv_view_proj: [[f32; 4]; 4],
    contact_shadow: [f32; 4],
    fog: [[f32; 4]; 2],
    visibility_mask: [[f32; 4]; 2],
}

fn fog_mode(instance: &Instance) -> f32 {
    if !instance.fog {
        return 0.0;
    }
    match instance.blend {
        Blend::Opaque
        | Blend::AlphaBlend
        | Blend::AlphaBlendWritesDepth
        | Blend::MergeMask
        | Blend::MergeAlphaBlend => 1.0,
        Blend::Additive
        | Blend::AlphaAdditive
        | Blend::AddScaledByAlpha
        | Blend::AddScaledByColor
        | Blend::AddInverseScaledByColor => 2.0,
        Blend::Multiply => 3.0,
    }
}

fn fog_block(fog: Option<DistanceFog>) -> [[f32; 4]; 2] {
    let Some(fog) = fog.filter(|f| f.end > f.start && f.start.is_finite() && f.end.is_finite())
    else {
        return [[0.0; 4]; 2];
    };
    let [r, g, b] = fog.color;
    [
        [r, g, b, 1.0],
        [fog.start, 1.0 / (fog.end - fog.start), 0.0, 0.0],
    ]
}

impl FrameUniforms {
    fn to_bytes(&self) -> [u8; FRAME_UNIFORM_FLOATS * 4] {
        let (floats, _) = self.floats();
        let mut bytes = [0u8; FRAME_UNIFORM_FLOATS * 4];
        for (chunk, value) in bytes.chunks_exact_mut(4).zip(floats.iter()) {
            chunk.copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    fn floats(&self) -> ([f32; FRAME_UNIFORM_FLOATS], usize) {
        let mut floats = [0.0f32; FRAME_UNIFORM_FLOATS];
        let mut written = 0usize;
        let mut put = |values: &[f32]| {
            for value in values {
                if let Some(slot) = floats.get_mut(written) {
                    *slot = *value;
                }
                written = written.saturating_add(1);
            }
        };
        for matrix in [&self.view_proj, &self.view] {
            for column in matrix {
                put(column);
            }
        }
        put(&self.camera_eye);
        for direction in &self.light_directions {
            put(direction);
        }
        for color in &self.light_colors {
            put(color);
        }
        for vector in [&self.ambient_color, &self.depth_params, &self.clip_plane] {
            put(vector);
        }
        for matrix in &self.shadow_cascades {
            for column in matrix {
                put(column);
            }
        }
        put(&self.shadow_params);
        put(&self.shadow_filter);
        for vector in &self.shadow_cascade_params {
            put(vector);
        }
        for vector in self
            .set_directions
            .iter()
            .chain(&self.set_colors)
            .chain(&self.set_ambient)
        {
            put(vector);
        }
        put(&self.cookie);
        put(&self.cookie_tint);
        for column in &self.inv_view_proj {
            put(column);
        }
        put(&self.contact_shadow);
        for vector in self.fog.iter().chain(&self.visibility_mask) {
            put(vector);
        }
        (floats, written)
    }
}

impl Uniforms {
    fn write_into(&self, out: &mut Vec<u8>) {
        let (floats, _) = self.floats();
        let start = out.len();
        out.resize(start.saturating_add(UNIFORM_FLOATS.saturating_mul(4)), 0);
        if let Some(block) = out.get_mut(start..) {
            for (chunk, value) in block.chunks_exact_mut(4).zip(floats.iter()) {
                chunk.copy_from_slice(&value.to_le_bytes());
            }
        }
    }

    fn floats(&self) -> ([f32; UNIFORM_FLOATS], usize) {
        let mut floats = [0.0f32; UNIFORM_FLOATS];
        let mut written = 0usize;
        let mut put = |values: &[f32]| {
            for value in values {
                if let Some(slot) = floats.get_mut(written) {
                    *slot = *value;
                }
                written = written.saturating_add(1);
            }
        };
        for matrix in [&self.model, &self.normal_matrix] {
            for column in matrix {
                put(column);
            }
        }
        for vector in [
            &self.tint,
            &self.params,
            &self.uv_linear,
            &self.uv_offset,
            &self.material_ambient,
            &self.material_diffuse,
            &self.material_emissive,
            &self.uv2_linear,
            &self.uv2_offset,
            &self.detail_combine,
            &self.house_color,
            &self.house_skin,
            &self.house_paint,
            &self.env_map,
            &self.color_source,
            &self.surface_flags,
            &self.height_shear,
            &self.light_add,
        ] {
            put(vector);
        }
        (floats, written)
    }

    #[cfg(test)]
    fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(usize::try_from(UNIFORM_SIZE).unwrap_or(416));
        self.write_into(&mut out);
        out
    }
}

static OFFSCREEN_DEVICE: std::sync::Mutex<Option<Opened>> = std::sync::Mutex::new(None);

#[derive(Clone)]
struct Opened {
    device: wgpu::Device,
    queue: wgpu::Queue,
    info: DeviceInfo,
    cache_key: Option<String>,
}

fn offscreen_device() -> Result<Opened> {
    let mut shared = OFFSCREEN_DEVICE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(open) = shared.as_ref() {
        return Ok(open.clone());
    }
    let (_, adapter) = open_adapter(|_| Ok(None))?;
    let (device, queue, info, cache_key) = request_device(&adapter)?;
    let open = Opened {
        device,
        queue,
        info,
        cache_key,
    };
    *shared = Some(open.clone());
    Ok(open)
}

fn open_adapter(
    surface_on: impl Fn(&wgpu::Instance) -> Result<Option<wgpu::Surface<'static>>>,
) -> Result<(Option<wgpu::Surface<'static>>, wgpu::Adapter)> {
    let all = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    if all.backends.contains(wgpu::Backends::VULKAN) && all.backends != wgpu::Backends::VULKAN {
        let mut vulkan = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
        vulkan.backends = wgpu::Backends::VULKAN;
        let instance = wgpu::Instance::new(vulkan);
        if let Ok(surface) = surface_on(&instance) {
            if let Ok(adapter) = request_adapter(&instance, surface.as_ref()) {
                if adapter.get_info().device_type == wgpu::DeviceType::DiscreteGpu {
                    return Ok((surface, adapter));
                }
            }
        }
    }
    let instance = wgpu::Instance::new(all);
    let surface = surface_on(&instance)?;
    let adapter = request_adapter(&instance, surface.as_ref())?;
    Ok((surface, adapter))
}

fn request_adapter(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'static>>,
) -> Result<wgpu::Adapter> {
    let options = wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: surface,
        ..Default::default()
    };
    block_on(instance.request_adapter(&options), || {
        instance.poll_all(false);
    })?
    .map_err(|_| Error::NoAdapter)
}

fn request_device(
    adapter: &wgpu::Adapter,
) -> Result<(wgpu::Device, wgpu::Queue, DeviceInfo, Option<String>)> {
    let block_compression = adapter
        .features()
        .contains(wgpu::Features::TEXTURE_COMPRESSION_BC);
    let required_features = adapter.features()
        & (wgpu::Features::TEXTURE_COMPRESSION_BC
            | wgpu::Features::DEPTH32FLOAT_STENCIL8
            | wgpu::Features::PIPELINE_CACHE);

    let (device, queue) = block_on(
        adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("mint device"),
            required_features,
            required_limits: wgpu::Limits::default(),
            ..Default::default()
        }),
        || {},
    )?
    .map_err(|e| Error::backend("opening the graphics device", e))?;

    let adapter_info = adapter.get_info();
    let info = DeviceInfo {
        adapter: adapter_info.name.clone(),
        backend: format!("{:?}", adapter_info.backend),
        driver: format!("{} {}", adapter_info.driver, adapter_info.driver_info)
            .trim()
            .to_owned(),
        block_compression,
    };
    let cache_key = wgpu::util::pipeline_cache_key(&adapter_info);
    Ok((device, queue, info, cache_key))
}

pub(crate) fn pop_scope(
    scope: wgpu::ErrorScopeGuard,
    device: &wgpu::Device,
) -> Result<Option<wgpu::Error>> {
    block_on(scope.pop(), || {
        let _ = device.poll(wgpu::PollType::Poll);
    })
}

fn block_on<F: Future>(future: F, pump: impl Fn()) -> Result<F::Output> {
    let mut future = pin!(future);
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    for _ in 0..MAX_POLLS {
        if let Poll::Ready(value) = future.as_mut().poll(&mut context) {
            return Ok(value);
        }
        pump();
        std::thread::yield_now();
    }
    Err(Error::DeviceTimeout)
}

fn wgpu_blend_state(blend: Blend) -> Option<wgpu::BlendState> {
    match blend {
        Blend::Opaque | Blend::MergeMask => None,
        Blend::AlphaBlend | Blend::AlphaBlendWritesDepth | Blend::MergeAlphaBlend => {
            Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::SrcAlpha,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::One,
                    dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
            })
        }
        Blend::AlphaAdditive => Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::SrcAlpha,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        }),
        Blend::Additive => Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        }),
        Blend::AddScaledByAlpha => Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::SrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        }),
        Blend::AddScaledByColor => Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::Src,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        }),
        Blend::AddInverseScaledByColor => Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrc,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        }),
        Blend::Multiply => Some(wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::Src,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::Zero,
                dst_factor: wgpu::BlendFactor::One,
                operation: wgpu::BlendOperation::Add,
            },
        }),
    }
}

fn merge_stencil_state(blend: Blend) -> wgpu::StencilState {
    let (compare, pass_op, write_mask) = match blend {
        Blend::MergeMask => (
            wgpu::CompareFunction::Always,
            wgpu::StencilOperation::Replace,
            0xFF,
        ),
        Blend::MergeAlphaBlend => (
            wgpu::CompareFunction::NotEqual,
            wgpu::StencilOperation::Keep,
            0,
        ),
        _ => return wgpu::StencilState::default(),
    };
    let face = wgpu::StencilFaceState {
        compare,
        fail_op: wgpu::StencilOperation::Keep,
        depth_fail_op: wgpu::StencilOperation::Keep,
        pass_op,
    };
    wgpu::StencilState {
        front: face,
        back: face,
        read_mask: 0xFF,
        write_mask,
    }
}

fn build_pipeline_masked(
    device: &wgpu::Device,
    cache: Option<&wgpu::PipelineCache>,
    layout: &wgpu::PipelineLayout,
    shader: &wgpu::ShaderModule,
    color_format: wgpu::TextureFormat,
    key: ColorKey,
    write_mask: wgpu::ColorWrites,
) -> wgpu::RenderPipeline {
    let ColorKey {
        cull_mode,
        skinned,
        direction,
        depth_test,
        blend,
    } = key;
    let buffers = vertex_buffer_layouts(skinned);
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("mint forward"),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(if skinned { "vs_skinned" } else { "vs_main" }),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: buffers
                .get(..usize::from(skinned).saturating_add(1))
                .unwrap_or(&[]),
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: depth_format(device),
            depth_write_enabled: Some(blend.writes_depth()),
            depth_compare: Some(match (direction, depth_test) {
                (DepthDirection::Standard, DepthTest::Nearer) => wgpu::CompareFunction::Less,
                (DepthDirection::Standard, DepthTest::NearerOrEqual) => {
                    wgpu::CompareFunction::LessEqual
                }
                (DepthDirection::Reversed, DepthTest::Nearer) => wgpu::CompareFunction::Greater,
                (DepthDirection::Reversed, DepthTest::NearerOrEqual) => {
                    wgpu::CompareFunction::GreaterEqual
                }
            }),
            stencil: merge_stencil_state(blend),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: wgpu_blend_state(blend),
                write_mask: if blend == Blend::MergeMask {
                    wgpu::ColorWrites::empty()
                } else {
                    write_mask
                },
            })],
        }),
        multiview_mask: None,
        cache,
    })
}

#[derive(Clone, Copy)]
struct ShadowLayouts<'a> {
    plain: &'a wgpu::PipelineLayout,
    alpha: &'a wgpu::PipelineLayout,
}

fn build_shadow_pipeline(
    device: &wgpu::Device,
    cache: Option<&wgpu::PipelineCache>,
    layouts: ShadowLayouts<'_>,
    shader: &wgpu::ShaderModule,
    key: ShadowKey,
) -> wgpu::RenderPipeline {
    let entry = match (key.alpha, key.skinned) {
        (false, false) => "vs_main",
        (true, false) => "vs_alpha",
        (false, true) => "vs_skinned",
        (true, true) => "vs_alpha_skinned",
    };
    let buffers = vertex_buffer_layouts(key.skinned);
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("mint shadow depth"),
        layout: Some(if key.alpha {
            layouts.alpha
        } else {
            layouts.plain
        }),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(entry),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: buffers
                .get(..usize::from(key.skinned).saturating_add(1))
                .unwrap_or(&[]),
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: key.culled.then_some(wgpu::Face::Back),
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: SHADOW_MAP_FORMAT,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::Less),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState {
                constant: 2,
                slope_scale: 2.0,
                clamp: 0.0,
            },
        }),
        multisample: wgpu::MultisampleState::default(),
        fragment: key.alpha.then(|| wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_alpha"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[],
        }),
        multiview_mask: None,
        cache,
    })
}

const VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 6] = [
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x3,
        offset: 0,
        shader_location: 0,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x3,
        offset: 12,
        shader_location: 1,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 24,
        shader_location: 2,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 32,
        shader_location: 3,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 48,
        shader_location: 4,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 56,
        shader_location: 5,
    },
];

const GEOMETRY_VERTEX_ATTRIBUTES: [wgpu::VertexAttribute; 4] = [
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 0,
        shader_location: 0,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 8,
        shader_location: 1,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 24,
        shader_location: 2,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x4,
        offset: 40,
        shader_location: 3,
    },
];

fn create_slot_buffer(
    device: &wgpu::Device,
    label: &'static str,
    stride: u64,
    slots: u32,
) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: stride.saturating_mul(u64::from(slots.max(1))),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_uniform_buffer(device: &wgpu::Device, capacity: u32) -> wgpu::Buffer {
    let size = u64::from(capacity)
        .checked_mul(UNIFORM_STRIDE)
        .unwrap_or(UNIFORM_STRIDE);
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mint uniforms"),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn bone_layout_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::VERTEX,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: wgpu::BufferSize::new(crate::skin::BONE_SIZE as u64),
        },
        count: None,
    }
}

fn create_bone_buffer(device: &wgpu::Device, capacity: u32) -> wgpu::Buffer {
    let size = u64::from(capacity.max(1))
        .checked_mul(crate::skin::BONE_SIZE as u64)
        .unwrap_or(crate::skin::BONE_SIZE as u64);
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mint bone palette"),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn vertex_buffer_layouts(skinned: bool) -> [Option<wgpu::VertexBufferLayout<'static>>; 2] {
    [
        Some(wgpu::VertexBufferLayout {
            array_stride: Vertex::SIZE as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &VERTEX_ATTRIBUTES,
        }),
        skinned.then_some(wgpu::VertexBufferLayout {
            array_stride: crate::SkinVertex::SIZE as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &SKIN_ATTRIBUTES,
        }),
    ]
}

const SKIN_ATTRIBUTES: [wgpu::VertexAttribute; 4] = [
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Uint16x2,
        offset: 0,
        shader_location: 6,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x2,
        offset: 4,
        shader_location: 7,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x3,
        offset: 12,
        shader_location: 8,
    },
    wgpu::VertexAttribute {
        format: wgpu::VertexFormat::Float32x3,
        offset: 24,
        shader_location: 9,
    },
];

#[allow(clippy::type_complexity)]
fn shadow_block(
    shadow: Option<ShadowSample>,
    map: impl Fn(&ShadowSample) -> (u32, usize),
) -> (
    [[[f32; 4]; 4]; MAX_SHADOW_CASCADES],
    [[f32; 4]; MAX_SHADOW_CASCADES],
    [f32; 4],
    [f32; 4],
) {
    let mut matrices = [Mat4::IDENTITY.to_cols(); MAX_SHADOW_CASCADES];
    let mut params = [[0.0f32; 4]; MAX_SHADOW_CASCADES];
    let Some(sample) = shadow else {
        return (matrices, params, [0.0, 1.0, 0.0, 0.0], [0.0; 4]);
    };
    let (size, layers) = map(&sample);
    let mut count = 0usize;
    for ((cascade, matrix), param) in sample
        .active_cascades()
        .take(layers)
        .zip(matrices.iter_mut())
        .zip(params.iter_mut())
    {
        *matrix = cascade.view_proj.to_cols();
        *param = [
            cascade.bias,
            cascade.texel.max(0.0),
            cascade.depth_range.max(0.0),
            0.0,
        ];
        count = count.saturating_add(1);
    }
    if count == 0 {
        return (matrices, params, [0.0, 1.0, 0.0, 0.0], [0.0; 4]);
    }
    let texel = f64_ratio(1, size.max(1));
    let blend = sample.blend.clamp(0.0, 1.0);
    let (spread, filter) = match sample.edge {
        ShadowEdge::Taps(texels) => (texel * texels.max(0.0), [blend, 0.0, 0.0, 0.0]),
        ShadowEdge::Tent => (0.0, [blend, 1.0, 0.0, 0.0]),
        ShadowEdge::Contact(contact) => (
            texel * contact.min_texels.max(0.0),
            [
                blend,
                2.0,
                contact.growth.max(0.0),
                contact.max_texels.max(contact.min_texels).max(1.0),
            ],
        ),
    };
    let count = f32::from(u8::try_from(count).unwrap_or(u8::MAX));
    (
        matrices,
        params,
        [1.0, sample.lightness, count, spread],
        filter,
    )
}

fn contact_shadow_block(contact: Option<ContactShadows>) -> [f32; 4] {
    contact.map_or([0.0; 4], |c| {
        [
            c.length.max(0.0),
            f32::from(u8::try_from(c.steps.min(MAX_CONTACT_SHADOW_STEPS)).unwrap_or(0)),
            c.thickness.max(0.0),
            c.lightness.clamp(0.0, 1.0),
        ]
    })
}

#[allow(clippy::type_complexity)]
fn light_set_block(
    scene: &Scene<'_>,
) -> (
    [[f32; 4]; SET_LIGHTS],
    [[f32; 4]; SET_LIGHTS],
    [[f32; 4]; LIGHT_SET_COUNT],
) {
    let mut directions = [[0.0f32; 4]; SET_LIGHTS];
    let mut colors = [[0.0f32; 4]; SET_LIGHTS];
    let mut ambient = [[0.0f32; 4]; LIGHT_SET_COUNT];
    for (index, set) in scene.light_sets.iter().enumerate() {
        let (lights, ambient_color, cookie) = match set {
            Some(set) => (set.lights, set.ambient_color, set.cookie),
            None => (scene.lights, scene.ambient_color, false),
        };
        for (slot, light) in lights.iter().enumerate() {
            let at = index
                .saturating_mul(MAX_DIRECTIONAL_LIGHTS)
                .saturating_add(slot);
            if let Some(out) = directions.get_mut(at) {
                *out = [light.direction.x, light.direction.y, light.direction.z, 0.0];
            }
            if let Some(out) = colors.get_mut(at) {
                *out = [light.color[0], light.color[1], light.color[2], 0.0];
            }
        }
        if let Some(out) = ambient.get_mut(index) {
            *out = [
                ambient_color[0],
                ambient_color[1],
                ambient_color[2],
                if cookie { 1.0 } else { 0.0 },
            ];
        }
    }
    (directions, colors, ambient)
}

fn create_uniform_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    buffer: &wgpu::Buffer,
    (shadow_map, shadow_sampler): (&wgpu::TextureView, &wgpu::Sampler),
    bones: &wgpu::Buffer,
    frame: &wgpu::Buffer,
    [(cookie, cookie_sampler), (mask, mask_sampler)]: [(&wgpu::TextureView, &wgpu::Sampler); 2],
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("mint uniforms"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer,
                    offset: 0,
                    size: None,
                }),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(shadow_map),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(shadow_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: bones.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: frame,
                    offset: 0,
                    size: wgpu::BufferSize::new(FRAME_UNIFORM_SIZE),
                }),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(cookie),
            },
            wgpu::BindGroupEntry {
                binding: 6,
                resource: wgpu::BindingResource::Sampler(cookie_sampler),
            },
            wgpu::BindGroupEntry {
                binding: 7,
                resource: wgpu::BindingResource::TextureView(mask),
            },
            wgpu::BindGroupEntry {
                binding: 8,
                resource: wgpu::BindingResource::Sampler(mask_sampler),
            },
        ],
    })
}

fn create_shadow_uniform_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    instances: &wgpu::Buffer,
    view_proj: &wgpu::Buffer,
    bones: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("mint shadow uniforms"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: instances,
                    offset: 0,
                    size: None,
                }),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: view_proj,
                    offset: 0,
                    size: wgpu::BufferSize::new(SHADOW_UNIFORM_SIZE),
                }),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: bones.as_entire_binding(),
            },
        ],
    })
}

fn create_ui_instance_buffer(device: &wgpu::Device, capacity: u32) -> wgpu::Buffer {
    let size = u64::from(capacity)
        .checked_mul(UI_INSTANCE_SIZE)
        .unwrap_or(UI_INSTANCE_SIZE);
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mint ui instances"),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_ui_geometry_buffer(device: &wgpu::Device, capacity: u32) -> wgpu::Buffer {
    let size = u64::from(capacity)
        .checked_mul(GEOMETRY_VERTEX_SIZE)
        .unwrap_or(GEOMETRY_VERTEX_SIZE);
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mint ui geometry vertices"),
        size,
        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_ui_geometry_textured_transform_buffer(
    device: &wgpu::Device,
    capacity: u32,
) -> wgpu::Buffer {
    let size = u64::from(capacity)
        .checked_mul(UI_GEOMETRY_TEXTURED_TRANSFORM_SIZE)
        .unwrap_or(UI_GEOMETRY_TEXTURED_TRANSFORM_SIZE);
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mint ui geometry textured transforms"),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn create_ui_geometry_textured_transform_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    transforms: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("mint ui geometry textured transforms"),
        layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                buffer: transforms,
                offset: 0,
                size: None,
            }),
        }],
    })
}

fn transform_ui_geometry_point(
    affine: [[f32; 2]; 2],
    translation: [f32; 2],
    local: [f32; 2],
) -> [f32; 2] {
    let [[m00, m01], [m10, m11]] = affine;
    [
        m00 * local[0] + m01 * local[1] + translation[0],
        m10 * local[0] + m11 * local[1] + translation[1],
    ]
}

fn write_geometry_vertex(
    position: [f32; 2],
    color: [f32; 4],
    clip: [f32; 4],
    color_add: [f32; 4],
    out: &mut Vec<u8>,
) {
    for value in position {
        out.extend_from_slice(&value.to_le_bytes());
    }
    for value in color {
        out.extend_from_slice(&value.to_le_bytes());
    }
    for value in clip {
        out.extend_from_slice(&value.to_le_bytes());
    }
    for value in color_add {
        out.extend_from_slice(&value.to_le_bytes());
    }
}

fn write_ui_geometry_textured_transform(
    geometry: &UiGeometry<'_>,
    width: u32,
    height: u32,
    out: &mut Vec<u8>,
) {
    let [[pm00, pm01], [pm10, pm11]] = geometry.affine;
    let [ptx, pty] = geometry.translation;
    let [[tm00, tm01], [tm10, tm11]] = geometry.texture_transform;
    let [ttx, tty] = geometry.texture_translation;
    let inv_w = f64::from(width).recip();
    let inv_h = f64::from(height).recip();
    #[allow(clippy::cast_possible_truncation)]
    let (inv_w, inv_h) = (inv_w as f32, inv_h as f32);
    for vector in [
        [pm00, pm01, pm10, pm11],
        [ptx, pty, 0.0, 0.0],
        [tm00 * inv_w, tm01 * inv_w, tm10 * inv_h, tm11 * inv_h],
        [ttx * inv_w, tty * inv_h, 0.0, 0.0],
    ] {
        for value in vector {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
}

fn create_ui_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    instances: &wgpu::Buffer,
    viewport: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("mint ui uniforms"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: instances,
                    offset: 0,
                    size: None,
                }),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: viewport,
                    offset: 0,
                    size: None,
                }),
            },
        ],
    })
}

fn write_ui_instance(quad: &UiQuad, out: &mut Vec<u8>) {
    let [[m00, m01], [m10, m11]] = quad.affine;
    for vector in [
        [m00, m01, m10, m11],
        [
            quad.translation[0],
            quad.translation[1],
            if quad.premultiplied { 1.0 } else { 0.0 },
            f32::from(
                u8::from(quad.greyscale)
                    | (u8::from(quad.opaque) << 1)
                    | (u8::from(quad.disc) << 2)
                    | (u8::from(quad.additive) << 3)
                    | (u8::from(quad.uv_rotated_cw) << 4),
            ),
        ],
        quad.uv_rect,
        quad.color_multiply,
        quad.color_add,
        quad.clip,
        [quad.wedge, quad.hole, 0.0, 0.0],
    ] {
        for value in vector {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
}

fn ui_viewport_bytes(width: u32, height: u32) -> [u8; 16] {
    let width = f64::from(width);
    let height = f64::from(height);
    #[allow(clippy::cast_possible_truncation)]
    let values: [f32; 4] = [width as f32, height as f32, 0.0, 0.0];
    let mut bytes = [0u8; 16];
    for (chunk, value) in bytes.chunks_exact_mut(4).zip(values) {
        chunk.copy_from_slice(&value.to_le_bytes());
    }
    bytes
}

struct UiGeometryPassOutput {
    runs: Vec<UiGeometryRun>,
    push_mask_ranges: Vec<(u32, u32)>,
    pop_mask_ranges: Vec<(u32, u32)>,
}

#[derive(Clone, Copy)]
struct UiGeometryRun {
    triangle_range: Option<(u32, u32)>,
    line_range: Option<(u32, u32)>,
    textured: Option<TexturedTriangleRun>,
    retained: Option<RetainedRun>,
}

#[derive(Clone, Copy)]
struct TexturedTriangleRun {
    vertex_range: (u32, u32),
    transform_index: u32,
    material: MaterialSlot,
}

fn ui_geometry_vertex_range(first: usize, end: usize) -> Result<(u32, u32)> {
    let first_u32 = u32::try_from(first).map_err(|_| Error::TooManyInstances { count: first })?;
    let end_u32 = u32::try_from(end).map_err(|_| Error::TooManyInstances { count: end })?;
    Ok((first_u32, end_u32.saturating_sub(first_u32)))
}

#[derive(Clone, Copy)]
struct UiRun {
    material: MaterialSlot,
    first: u32,
    count: u32,
}

fn ui_runs(materials: &[MaterialSlot]) -> Result<Vec<UiRun>> {
    let mut runs: Vec<UiRun> = Vec::new();
    for (index, material) in materials.iter().enumerate() {
        let merges = runs.last().is_some_and(|run| run.material == *material);
        if merges {
            if let Some(run) = runs.last_mut() {
                run.count = run.count.saturating_add(1);
            }
        } else {
            let first = u32::try_from(index).map_err(|_| Error::TooManyInstances {
                count: materials.len(),
            })?;
            runs.push(UiRun {
                material: *material,
                first,
                count: 1,
            });
        }
    }
    Ok(runs)
}

#[derive(Clone, Copy)]
enum OrderedScreenDraw {
    QuadRun { run: UiRun, depth: u32 },
    Geometry { run: UiGeometryRun, depth: u32 },
    PushMask { range: (u32, u32), reference: u32 },
    PopMask { range: (u32, u32), reference: u32 },
}

fn build_ordered_screen_draws(
    items: &[ScreenDraw<'_>],
    quad_materials: &[MaterialSlot],
    geometry_runs: &[UiGeometryRun],
    push_mask_ranges: &[(u32, u32)],
    pop_mask_ranges: &[(u32, u32)],
    quad_base: u32,
) -> Vec<OrderedScreenDraw> {
    let mut draws = Vec::new();
    let mut quad_index = 0usize;
    let mut geometry_index = 0usize;
    let mut push_index = 0usize;
    let mut pop_index = 0usize;
    let mut depth = 0u32;
    let mut current_run: Option<(UiRun, u32)> = None;
    let flush = |draws: &mut Vec<OrderedScreenDraw>, current_run: &mut Option<(UiRun, u32)>| {
        if let Some((run, run_depth)) = current_run.take() {
            draws.push(OrderedScreenDraw::QuadRun {
                run,
                depth: run_depth,
            });
        }
    };
    for item in items {
        match item {
            ScreenDraw::Quad(_) => {
                let Some(material) = quad_materials.get(quad_index).copied() else {
                    continue;
                };
                let offset = u32::try_from(quad_index).unwrap_or(u32::MAX);
                let buffer_index = quad_base.saturating_add(offset);
                quad_index = quad_index.saturating_add(1);
                let merges = current_run
                    .is_some_and(|(run, run_depth)| run.material == material && run_depth == depth);
                if merges {
                    if let Some((run, _)) = current_run.as_mut() {
                        run.count = run.count.saturating_add(1);
                    }
                } else {
                    flush(&mut draws, &mut current_run);
                    current_run = Some((
                        UiRun {
                            material,
                            first: buffer_index,
                            count: 1,
                        },
                        depth,
                    ));
                }
            }
            ScreenDraw::Geometry(_) => {
                flush(&mut draws, &mut current_run);
                if let Some(run) = geometry_runs.get(geometry_index).copied() {
                    draws.push(OrderedScreenDraw::Geometry { run, depth });
                }
                geometry_index = geometry_index.saturating_add(1);
            }
            ScreenDraw::PushMask(_) => {
                flush(&mut draws, &mut current_run);
                if let Some(range) = push_mask_ranges.get(push_index).copied() {
                    draws.push(OrderedScreenDraw::PushMask {
                        range,
                        reference: depth,
                    });
                }
                push_index = push_index.saturating_add(1);
                depth = depth.saturating_add(1);
            }
            ScreenDraw::PopMask(_) => {
                flush(&mut draws, &mut current_run);
                if let Some(range) = pop_mask_ranges.get(pop_index).copied() {
                    draws.push(OrderedScreenDraw::PopMask {
                        range,
                        reference: depth,
                    });
                }
                depth = depth.saturating_sub(1);
                pop_index = pop_index.saturating_add(1);
            }
        }
    }
    flush(&mut draws, &mut current_run);
    draws
}

fn create_shadow_instance_buffer(device: &wgpu::Device, capacity: u32) -> wgpu::Buffer {
    let size = u64::from(capacity)
        .checked_mul(SHADOW_UNIFORM_SIZE)
        .unwrap_or(SHADOW_UNIFORM_SIZE);
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("mint shadow instances"),
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

const SHADOW_MAP_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

fn create_shadow_texture(
    device: &wgpu::Device,
    size: u32,
    layers: u32,
) -> (wgpu::TextureView, Vec<wgpu::TextureView>) {
    let layers = layers.max(1);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mint shadow map"),
        size: wgpu::Extent3d {
            width: size.max(1),
            height: size.max(1),
            depth_or_array_layers: layers,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: SHADOW_MAP_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let array = texture.create_view(&wgpu::TextureViewDescriptor {
        label: Some("mint shadow map (cascades)"),
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let per_layer = (0..layers)
        .map(|layer| {
            texture.create_view(&wgpu::TextureViewDescriptor {
                label: Some("mint shadow map (one cascade)"),
                dimension: Some(wgpu::TextureViewDimension::D2),
                base_array_layer: layer,
                array_layer_count: Some(1),
                ..Default::default()
            })
        })
        .collect();
    (array, per_layer)
}

fn create_depth_views(
    device: &wgpu::Device,
    width: u32,
    height: u32,
) -> (wgpu::TextureView, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mint depth"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: depth_format(device),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let sampled = texture.create_view(&wgpu::TextureViewDescriptor {
        aspect: wgpu::TextureAspect::DepthOnly,
        ..Default::default()
    });
    (
        texture.create_view(&wgpu::TextureViewDescriptor::default()),
        sampled,
    )
}

fn create_screen_stencil_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mint screen stencil"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: STENCIL_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    texture.create_view(&wgpu::TextureViewDescriptor::default())
}

const fn frame_input_index(depth: bool, color: bool) -> usize {
    (depth as usize) | ((color as usize) << 1)
}

const MAX_CONTACT_SHADOW_STEPS: u32 = 64;

fn create_contact_shadow_pipeline(
    device: &wgpu::Device,
    cache: Option<&wgpu::PipelineCache>,
    uniform_layout: &wgpu::BindGroupLayout,
    depth_layout: Option<&wgpu::BindGroupLayout>,
    color_format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("mint contact shadows"),
        source: wgpu::ShaderSource::Wgsl(
            format!(
                "{}{}{}",
                include_str!("instance_block.wgsl"),
                crate::SCENE_DEPTH_PRELUDE,
                include_str!("contact_shadow.wgsl")
            )
            .into(),
        ),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("mint contact shadow layout"),
        bind_group_layouts: &[Some(uniform_layout), None, depth_layout],
        immediate_size: 0,
    });
    let multiply = wgpu::BlendState {
        color: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Zero,
            dst_factor: wgpu::BlendFactor::Src,
            operation: wgpu::BlendOperation::Add,
        },
        alpha: wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Zero,
            dst_factor: wgpu::BlendFactor::One,
            operation: wgpu::BlendOperation::Add,
        },
    };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("mint contact shadows"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &module,
            entry_point: Some("vs_contact"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &module,
            entry_point: Some("fs_contact"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: Some(multiply),
                write_mask: wgpu::ColorWrites::COLOR,
            })],
        }),
        multiview_mask: None,
        cache,
    })
}

fn create_frame_input_layout(device: &wgpu::Device, index: usize) -> wgpu::BindGroupLayout {
    let mut entries: Vec<wgpu::BindGroupLayoutEntry> = Vec::with_capacity(3);
    if index & 1 != 0 {
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Depth,
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
    }
    if index & 2 != 0 {
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 1,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 2,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        });
    }
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("mint frame inputs"),
        entries: &entries,
    })
}

fn create_frame_input_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    index: usize,
    depth: &wgpu::TextureView,
    color: Option<&SceneColor>,
) -> Option<wgpu::BindGroup> {
    let mut entries: Vec<wgpu::BindGroupEntry<'_>> = Vec::with_capacity(3);
    if index & 1 != 0 {
        entries.push(wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(depth),
        });
    }
    if index & 2 != 0 {
        let color = color?;
        entries.push(wgpu::BindGroupEntry {
            binding: 1,
            resource: wgpu::BindingResource::TextureView(&color.view),
        });
        entries.push(wgpu::BindGroupEntry {
            binding: 2,
            resource: wgpu::BindingResource::Sampler(&color.sampler),
        });
    }
    Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("mint frame inputs"),
        layout,
        entries: &entries,
    }))
}

struct ShadowMapEntry {
    view: wgpu::TextureView,
    layers: Vec<wgpu::TextureView>,
    size: u32,
}

struct RenderTarget {
    #[allow(dead_code)]
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    depth: wgpu::TextureView,
    width: u32,
    height: u32,
    texture_index: usize,
}

struct SceneColor {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
}

fn create_scene_color(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> SceneColor {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mint scene colour"),
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
    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("mint scene colour"),
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        ..Default::default()
    });
    SceneColor {
        texture,
        view,
        sampler,
    }
}

fn create_render_target_color(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mint render target"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: format.remove_srgb_suffix(),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn create_color_target(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> (wgpu::Texture, wgpu::TextureView) {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mint offscreen colour"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    (texture, view)
}

fn custom_group_entries(uniform_size: u32, texture_slots: u32) -> Vec<wgpu::BindGroupLayoutEntry> {
    let mut entries: Vec<wgpu::BindGroupLayoutEntry> = Vec::new();
    if uniform_size > 0 {
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: wgpu::BufferSize::new(u64::from(uniform_size)),
            },
            count: None,
        });
    }
    for slot in 0..texture_slots {
        let texture = slot.saturating_mul(2).saturating_add(1);
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: texture,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
        entries.push(wgpu::BindGroupLayoutEntry {
            binding: texture.saturating_add(1),
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        });
    }
    entries
}

fn create_buffer_init(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    usage: wgpu::BufferUsages,
    data: &[u8],
) -> wgpu::Buffer {
    let size = align_up(data.len(), 4).unwrap_or(data.len());
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size: u64::try_from(size).unwrap_or(u64::MAX),
        usage: usage | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    queue.write_buffer(&buffer, 0, data);
    buffer
}

fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    data: &TextureData,
    block_compression: bool,
) -> Result<GpuTexture> {
    let usable = data.usable_levels();
    if usable == 0 {
        return Err(Error::EmptyTexture);
    }
    if data.format.is_block_compressed() && !block_compression {
        return Err(Error::BlockCompressionUnsupported);
    }
    let (width, height) = (data.width(), data.height());
    let gpu = new_texture(
        device,
        (width, height),
        data.format,
        u32::try_from(usable).unwrap_or(1),
        (data.address_u, data.address_v),
    )?;
    for (index, level) in data.levels.iter().take(usable).enumerate() {
        let bytes_per_row = data
            .format
            .bytes_per_row(level.width)
            .and_then(|n| u32::try_from(n).ok())
            .ok_or(Error::EmptyTexture)?;
        let rows = level
            .height
            .checked_add(data.format.block_edge().saturating_sub(1))
            .and_then(|n| n.checked_div(data.format.block_edge()))
            .ok_or(Error::EmptyTexture)?;
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &gpu.texture,
                mip_level: u32::try_from(index).unwrap_or(0),
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &level.data,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(rows),
            },
            wgpu::Extent3d {
                width: level.width,
                height: level.height,
                depth_or_array_layers: 1,
            },
        );
    }
    Ok(gpu)
}

fn new_texture(
    device: &wgpu::Device,
    (width, height): (u32, u32),
    pixel_format: PixelFormat,
    levels: u32,
    (address_u, address_v): (AddressMode, AddressMode),
) -> Result<GpuTexture> {
    let limit = device.limits().max_texture_dimension_2d;
    if width > limit || height > limit {
        return Err(Error::TextureTooLarge {
            width,
            height,
            limit,
        });
    }

    let format = match pixel_format {
        PixelFormat::Rgba8Srgb => wgpu::TextureFormat::Rgba8UnormSrgb,
        PixelFormat::Bc1Srgb => wgpu::TextureFormat::Bc1RgbaUnormSrgb,
        PixelFormat::Bc3Srgb => wgpu::TextureFormat::Bc3RgbaUnormSrgb,
        PixelFormat::Rgba8Unorm => wgpu::TextureFormat::Rgba8Unorm,
        PixelFormat::Bc1Unorm => wgpu::TextureFormat::Bc1RgbaUnorm,
        PixelFormat::Bc3Unorm => wgpu::TextureFormat::Bc3RgbaUnorm,
        PixelFormat::Rg8Snorm => wgpu::TextureFormat::Rg8Snorm,
        PixelFormat::R8Unorm => wgpu::TextureFormat::R8Unorm,
    };

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("mint texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: levels,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    Ok(GpuTexture {
        texture,
        format: Some(pixel_format),
        view,
        address_u,
        address_v,
        width,
        height,
    })
}

struct SamplerSet {
    repeat_repeat: wgpu::Sampler,
    repeat_clamp: wgpu::Sampler,
    clamp_repeat: wgpu::Sampler,
    clamp_clamp: wgpu::Sampler,
}

impl SamplerSet {
    fn build(device: &wgpu::Device, anisotropy: u16) -> Self {
        let make = |u: wgpu::AddressMode, v: wgpu::AddressMode| {
            device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("mint texture sampler"),
                address_mode_u: u,
                address_mode_v: v,
                address_mode_w: wgpu::AddressMode::Repeat,
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                mipmap_filter: wgpu::MipmapFilterMode::Linear,
                anisotropy_clamp: anisotropy.clamp(1, MAX_ANISOTROPY),
                ..Default::default()
            })
        };
        use wgpu::AddressMode::{ClampToEdge, Repeat};
        Self {
            repeat_repeat: make(Repeat, Repeat),
            repeat_clamp: make(Repeat, ClampToEdge),
            clamp_repeat: make(ClampToEdge, Repeat),
            clamp_clamp: make(ClampToEdge, ClampToEdge),
        }
    }

    fn get(&self, u: AddressMode, v: AddressMode) -> &wgpu::Sampler {
        match (u, v) {
            (AddressMode::Repeat, AddressMode::Repeat) => &self.repeat_repeat,
            (AddressMode::Repeat, AddressMode::Clamp) => &self.repeat_clamp,
            (AddressMode::Clamp, AddressMode::Repeat) => &self.clamp_repeat,
            (AddressMode::Clamp, AddressMode::Clamp) => &self.clamp_clamp,
        }
    }
}

fn sorted_draw_order(
    instances: &[Instance],
    eye: Vec3,
    keep: impl Fn(&Instance) -> bool,
) -> Vec<usize> {
    let mut opaque = Vec::new();
    let mut overlays: Vec<(usize, u8)> = Vec::new();
    let mut leveled: Vec<(usize, u8)> = Vec::new();
    let mut blended: Vec<(usize, f32)> = Vec::new();
    let mut depth_blended: Vec<(usize, f32)> = Vec::new();
    let mut merge_masks = Vec::new();
    let mut merged = Vec::new();
    for (index, instance) in instances.iter().enumerate() {
        if !keep(instance) {
            continue;
        }
        if instance.blend == Blend::Opaque {
            opaque.push(index);
        } else if instance.blend == Blend::MergeMask {
            merge_masks.push(index);
        } else if instance.blend == Blend::MergeAlphaBlend {
            merged.push(index);
        } else if instance.surface_overlay && !instance.blend.writes_depth() {
            overlays.push((index, instance.sort_level.unwrap_or(0)));
        } else if instance.blend == Blend::AlphaBlendWritesDepth {
            depth_blended.push((index, instance_distance_sq(instance, eye)));
        } else if let Some(level) = instance.sort_level {
            leveled.push((index, level));
        } else {
            blended.push((index, instance_distance_sq(instance, eye)));
        }
    }
    leveled.sort_by(|a, b| b.1.cmp(&a.1));
    overlays.sort_by(|a, b| b.1.cmp(&a.1));

    blended.sort_by(|a, b| b.1.total_cmp(&a.1));
    depth_blended.sort_by(|a, b| b.1.total_cmp(&a.1));

    opaque
        .into_iter()
        .chain(overlays.into_iter().map(|(index, _)| index))
        .chain(depth_blended.into_iter().map(|(index, _)| index))
        .chain(merge_masks)
        .chain(merged)
        .chain(leveled.into_iter().map(|(index, _)| index))
        .chain(blended.into_iter().map(|(index, _)| index))
        .collect()
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct BatchKey {
    mesh: usize,
    cull: bool,
    surface: Option<SurfaceKey>,
    depth_material: Option<MaterialSlot>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct SurfaceKey {
    material: MaterialSlot,
    blend: Blend,
    depth_test: DepthTest,
    custom: Option<usize>,
    shader: Option<usize>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum BatchScope {
    Surface,
    MeshOnly,
    MeshAndMaterial,
}

#[derive(Clone, Copy)]
struct Batch {
    first: usize,
    count: u32,
    lead: usize,
    material: MaterialSlot,
}

fn is_opaque_lead(instances: &[Instance], batch: &Batch) -> bool {
    instances.get(batch.lead).is_some_and(|instance| {
        instance.blend.writes_depth() || (instance.surface_overlay && !instance.blend.merges())
    })
}

fn batch_draws(
    instances: &[Instance],
    order: &[usize],
    materials: &[MaterialSlot],
    cull_override: Option<bool>,
    scope: BatchScope,
) -> Vec<Batch> {
    let mut batches: Vec<Batch> = Vec::new();
    let mut previous: Option<BatchKey> = None;
    for (slot, index) in order.iter().enumerate() {
        let key =
            match scope {
                BatchScope::Surface => {
                    instances
                        .get(*index)
                        .zip(materials.get(*index))
                        .map(|(instance, material)| BatchKey {
                            mesh: instance.mesh.0,
                            cull: cull_override.unwrap_or(instance.cull),
                            depth_material: None,
                            surface: Some(SurfaceKey {
                                material: *material,
                                blend: instance.blend,
                                depth_test: instance.depth_test,
                                custom: instance.custom_material.map(|handle| handle.0),
                                shader: instance.custom_shader.map(|handle| handle.0),
                            }),
                        })
                }
                BatchScope::MeshOnly => instances.get(*index).map(|instance| BatchKey {
                    mesh: instance.mesh.0,
                    cull: cull_override.unwrap_or(instance.cull),
                    surface: None,
                    depth_material: None,
                }),
                BatchScope::MeshAndMaterial => instances
                    .get(*index)
                    .zip(materials.get(*index))
                    .map(|(instance, material)| BatchKey {
                        mesh: instance.mesh.0,
                        cull: cull_override.unwrap_or(instance.cull),
                        surface: None,
                        depth_material: Some(*material),
                    }),
            };
        let merges = key.is_some() && key == previous;
        match batches.last_mut().filter(|_| merges) {
            Some(batch) => batch.count = batch.count.saturating_add(1),
            None => batches.push(Batch {
                first: slot,
                count: 1,
                lead: *index,
                material: materials.get(*index).copied().unwrap_or(0),
            }),
        }
        previous = key;
    }
    batches
}

fn instance_distance_sq(instance: &Instance, eye: Vec3) -> f32 {
    let at = match instance.bounds {
        Some(bounds) if !bounds.is_empty() => instance.transform.transform_point(bounds.center()),
        _ => {
            let t = &instance.transform;
            Vec3::new(t.get(0, 3), t.get(1, 3), t.get(2, 3))
        }
    };
    let dx = at.x - eye.x;
    let dy = at.y - eye.y;
    let dz = at.z - eye.z;
    dx * dx + dy * dy + dz * dz
}

fn align_up(value: usize, alignment: usize) -> Option<usize> {
    if alignment == 0 {
        return None;
    }
    let remainder = value.checked_rem(alignment)?;
    if remainder == 0 {
        Some(value)
    } else {
        value.checked_add(alignment.checked_sub(remainder)?)
    }
}

fn clamp_size(width: u32, height: u32) -> (u32, u32) {
    (width.max(1), height.max(1))
}

fn small_index_as_f32(index: u32) -> f32 {
    match index {
        0 => 0.0,
        1 => 1.0,
        2 => 2.0,
        3 => 3.0,
        4 => 4.0,
        5 => 5.0,
        6 => 6.0,
        7 => 7.0,
        8 => 8.0,
        9 => 9.0,
        10 => 10.0,
        11 => 11.0,
        12 => 12.0,
        _ => 13.0,
    }
}

fn f64_ratio(width: u32, height: u32) -> f32 {
    let w = f64::from(width);
    let h = f64::from(height);
    if h == 0.0 {
        return 1.0;
    }
    let ratio = w / h;
    #[allow(clippy::cast_possible_truncation)]
    let narrowed = ratio as f32;
    narrowed
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests;

mod frame;
mod ui_retained;
