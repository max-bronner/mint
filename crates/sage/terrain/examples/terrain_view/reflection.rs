use super::lighting::*;
use super::water_scene::*;
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ReflectionCull {
    Off,
    #[default]
    On,
}

impl ReflectionCull {
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "on" => Some(Self::On),
            _ => None,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::On => "on",
        }
    }

    pub(crate) fn next(self) -> Self {
        match self {
            Self::Off => Self::On,
            Self::On => Self::Off,
        }
    }
}

pub(crate) const PLANAR_VISIBILITY_MARGIN: f32 = 512.0;

pub(crate) fn group_by_height(heights: &[f32]) -> (Vec<f32>, Vec<usize>) {
    let mut distinct: Vec<f32> = Vec::new();
    let mut of_body = Vec::with_capacity(heights.len());
    for height in heights {
        let group = match distinct
            .iter()
            .position(|known| (known - height).abs() <= PLANAR_HEIGHT_EPSILON)
        {
            Some(index) => index,
            None => {
                distinct.push(*height);
                distinct.len().saturating_sub(1)
            }
        };
        of_body.push(group);
    }
    (distinct, of_body)
}

pub(crate) fn plane_of_body(group_of_body: &[usize], created: usize) -> Vec<Option<usize>> {
    group_of_body
        .iter()
        .map(|group| (*group < created).then_some(*group))
        .collect()
}

pub(crate) fn water_sampler_bounds(
    planes: &[(f32, mint_render::RenderTargetHandle)],
    water: &WaterScene,
    surface: &WaterSurface,
) -> Vec<Aabb> {
    let mut bounds = vec![Aabb::EMPTY; planes.len()];
    for (slot, body) in surface.bodies.iter().enumerate() {
        let Some(index) = water.plane_index(slot) else {
            continue;
        };
        grow_sampler_bounds(&mut bounds, index, &body.mesh);
    }
    bounds
}

pub(crate) fn river_sampler_bounds(
    planes: &[(f32, mint_render::RenderTargetHandle)],
    rivers: &RiverScene,
    network: &RiverNetwork,
) -> Vec<Aabb> {
    let mut bounds = vec![Aabb::EMPTY; planes.len()];
    for (slot, ribbon) in network.ribbons.iter().enumerate() {
        let Some(target) = rivers.mirrors.get(slot).copied().flatten() else {
            continue;
        };
        let Some(index) = planes.iter().position(|(_, handle)| *handle == target) else {
            continue;
        };
        grow_sampler_bounds(&mut bounds, index, &ribbon.mesh);
    }
    bounds
}

pub(crate) fn grow_sampler_bounds(bounds: &mut [Aabb], index: usize, mesh: &MeshData) {
    let Some(slot) = bounds.get_mut(index) else {
        return;
    };
    let mesh_bounds = mesh.bounds();
    if mesh_bounds.is_empty() {
        return;
    }
    let (min, max) = (mesh_bounds.min(), mesh_bounds.max());
    let margin = PLANAR_VISIBILITY_MARGIN;
    slot.union(&Aabb::from_corners(
        Vec3::new(min.x - margin, min.y - margin, min.z - margin),
        Vec3::new(max.x + margin, max.y + margin, max.z + margin),
    ));
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_planar_reflections(
    renderer: &mut Renderer,
    planes: &[(f32, mint_render::RenderTargetHandle)],
    sampler_bounds: &[Aabb],
    cull: ReflectionCull,
    reflected: &mut [Instance],
    sky_slot: Option<usize>,
    camera: Camera,
    rig: &TerrainRig,
    exposure: f32,
    mask: LightMask,
    shadow: Option<ShadowSample>,
) -> mint_render::Result<PlanarPassCount> {
    let mut count = PlanarPassCount::default();
    if planes.is_empty() || reflected.is_empty() {
        return Ok(count);
    }
    let (width, height) = renderer.size();
    let (target_width, target_height) = (width.div_ceil(2).max(1), height.div_ceil(2).max(1));
    let aspect = width as f32 / height.max(1) as f32;
    let frustum = mint_render::Frustum::from_view_proj(
        camera
            .projection_with_depth(renderer.depth_direction(), aspect)
            .mul_mat4(camera.view()),
    );
    let restore = renderer.cull_override();
    renderer.set_cull_override(Some(false));
    let mut result = Ok(());
    for (index, (plane, target)) in planes.iter().enumerate() {
        if cull == ReflectionCull::On {
            let visible = sampler_bounds
                .get(index)
                .is_none_or(|bounds| frustum.intersects(bounds));
            if !visible {
                count.skipped = count.skipped.saturating_add(1);
                continue;
            }
        }
        count.drawn = count.drawn.saturating_add(1);
        if let Err(error) = renderer.resize_render_target(*target, target_width, target_height) {
            result = Err(error);
            continue;
        }
        let mut mirrored = camera;
        mirrored.eye.z = 2.0 * plane - camera.eye.z;
        mirrored.target.z = 2.0 * plane - camera.target.z;
        mirrored.up.z = -camera.up.z;
        if let Some(sky) = sky_slot.and_then(|slot| reflected.get_mut(slot)) {
            sky.transform = mint_render::Mat4::from_translation(mirrored.eye);
        }
        let mut scene = terrain_scene(mirrored, reflected, rig, exposure, mask, shadow);
        scene.clip_plane = Some([0.0, 0.0, 1.0, -(plane - PLANAR_CLIP_BIAS)]);
        if let Err(error) = renderer.render_to_target(*target, &scene) {
            result = Err(error);
        }
    }
    if let Some(sky) = sky_slot.and_then(|slot| reflected.get_mut(slot)) {
        sky.transform = mint_render::Mat4::from_translation(camera.eye);
    }
    renderer.set_cull_override(restore);
    result.map(|()| count)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct PlanarPassCount {
    pub(crate) drawn: usize,
    pub(crate) skipped: usize,
}

pub(crate) fn terrain_scene<'a>(
    camera: Camera,
    instances: &'a [Instance],
    rig: &TerrainRig,
    exposure: f32,
    mask: LightMask,
    shadow: Option<ShadowSample>,
) -> Scene<'a> {
    let base = Scene::new(camera);
    let mut lights = rig.lights;
    for (slot, light) in lights.iter_mut().enumerate() {
        let on = mask.lights.get(slot).copied().unwrap_or(true);
        light.color = if on {
            light.color.map(|c| c * exposure)
        } else {
            [0.0; 3]
        };
    }
    Scene {
        lights,
        ambient_color: if mask.ambient {
            rig.ambient.map(|c| c * exposure)
        } else {
            [0.0; 3]
        },
        lit_ceiling: rig.ceiling,
        shadow,
        ..base
    }
    .with_instances(instances)
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct LightMask {
    pub(crate) ambient: bool,
    pub(crate) lights: [bool; mint_render::MAX_DIRECTIONAL_LIGHTS],
}

impl Default for LightMask {
    fn default() -> Self {
        Self {
            ambient: true,
            lights: [true; mint_render::MAX_DIRECTIONAL_LIGHTS],
        }
    }
}
