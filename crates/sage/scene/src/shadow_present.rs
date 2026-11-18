use std::cell::Cell;

use mint_render::{
    fit_directional_light_to_view, Aabb, Camera, ContactHardening, ContactShadows, FittedShadow,
    Instance, Renderer, Scene, ShadowCascade, ShadowEdge, ShadowFit, ShadowMapHandle, ShadowSample,
    MAX_SHADOW_CASCADES,
};
use mint_sage_terrain::WaterTuning;

use crate::lighting::TerrainRig;

const SHADOW_MAP_SIZES: [u32; 6] = [256, 512, 1024, 2048, 4096, 8192];
const SHADOW_MAP_SMALLEST: u32 = 256;
const SHADOW_REACH_PER_ZOOM: f32 = 10.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShadowFilter {
    #[default]
    Tent,
    Retail,
    Contact,
    Wide,
}

impl ShadowFilter {
    pub const ALL: [Self; 4] = [Self::Tent, Self::Retail, Self::Contact, Self::Wide];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Tent => "tent (hard 3x3-texel tent, 4 taps)",
            Self::Contact => "contact (sharp at the caster, softer with distance)",
            Self::Retail => "retail (one-texel edge, 1 tap)",
            Self::Wide => "wide (three-texel edge, 4 taps)",
        }
    }

    fn edge(self, tuning: &WaterTuning) -> ShadowEdge {
        match self {
            Self::Tent => ShadowEdge::Tent,
            Self::Retail => ShadowEdge::Taps(0.0),
            Self::Wide => ShadowEdge::Taps(1.0),
            Self::Contact => ShadowEdge::Contact(ContactHardening {
                growth: tuning.shadow_contact_growth.max(0.0),
                min_texels: tuning.shadow_contact_min_texels.max(0.0),
                max_texels: tuning
                    .shadow_contact_max_texels
                    .max(tuning.shadow_contact_min_texels)
                    .max(1.0),
            }),
        }
    }
}

pub struct ShadowPass {
    map: ShadowMapHandle,
    size: u32,
    layers: usize,
    pub filter: ShadowFilter,
    pub cascaded: bool,
    pub contact_shadows: bool,
    last_texels: Cell<[f32; MAX_SHADOW_CASCADES]>,
    reported: Cell<bool>,
}

#[derive(Debug, Clone, Copy)]
struct ShadowVolume {
    view_proj: mint_render::Mat4,
    range: f32,
    texel: f32,
}

impl ShadowPass {
    #[must_use]
    pub fn new(renderer: &mut Renderer, tuning: &WaterTuning) -> Option<Self> {
        let size = SHADOW_MAP_SIZES
            .into_iter()
            .filter(|candidate| f64::from(*candidate) <= f64::from(tuning.shadow_map_size))
            .next_back()
            .unwrap_or(SHADOW_MAP_SMALLEST);
        let wanted = whole_count(
            tuning.shadow_cascades,
            1,
            u32::try_from(MAX_SHADOW_CASCADES).unwrap_or(1),
        );
        match renderer.create_shadow_cascades(size, wanted) {
            Ok(map) => {
                let size = renderer.shadow_map_size(map).unwrap_or(size);
                let layers = renderer.shadow_map_cascades(map).unwrap_or(1);
                println!("shadows: {size}x{size} shadow map, {layers} cascade(s)");
                Some(Self {
                    map,
                    size,
                    layers,
                    filter: ShadowFilter::default(),
                    cascaded: true,
                    contact_shadows: true,
                    last_texels: Cell::new([0.0; MAX_SHADOW_CASCADES]),
                    reported: Cell::new(false),
                })
            }
            Err(error) => {
                println!("  SHADOW MAP FAILED: {error}");
                None
            }
        }
    }

    fn active_layers(&self) -> usize {
        if self.cascaded {
            self.layers.max(1)
        } else {
            1
        }
    }

    fn reaches(
        &self,
        camera: &Camera,
        aspect: f32,
        whole: &FittedShadow,
        tuning: &WaterTuning,
    ) -> [f32; MAX_SHADOW_CASCADES] {
        let count = self.active_layers();
        let mut reaches = [f32::INFINITY; MAX_SHADOW_CASCADES];
        if count < 2 {
            return reaches;
        }
        let start = whole.view_start.max(ground_start(camera, aspect)).max(1.0);
        let extent = whole.view_extent.max(start);
        let log = tuning.shadow_cascade_split.clamp(0.0, 1.0);
        let parts = count as f32;
        for (index, reach) in reaches.iter_mut().take(count - 1).enumerate() {
            let fraction = (index + 1) as f32 / parts;
            let even = start + (extent - start) * fraction;
            let geometric = start * mint_math::f32::powf(extent / start, fraction);
            *reach = even + (geometric - even) * log;
        }
        reaches
    }

    fn volumes(
        &self,
        renderer: &Renderer,
        camera: Camera,
        rig: &TerrainRig,
        tuning: &WaterTuning,
        world: Aabb,
    ) -> Option<[Option<ShadowVolume>; MAX_SHADOW_CASCADES]> {
        let sun = rig.lights.first()?;
        if sun.direction.length_squared() <= f32::EPSILON {
            return None;
        }
        let (width, height) = renderer.size();
        let aspect = width as f32 / height.max(1) as f32;
        let zoom = (camera.target - camera.eye).length();
        let fit = |distance: f32| {
            fit_directional_light_to_view(
                sun.direction,
                &camera,
                aspect,
                ShadowFit {
                    distance,
                    margin: 0.0,
                    depth: tuning.shadow_depth.max(0.0),
                    texels: self.size,
                    bounds: (!world.is_empty()).then_some(world),
                },
            )
        };
        let whole = fit(zoom * SHADOW_REACH_PER_ZOOM);
        let reaches = self.reaches(&camera, aspect, &whole, tuning);
        let count = self.active_layers();
        let mut volumes = [None; MAX_SHADOW_CASCADES];
        let mut texels = [0.0; MAX_SHADOW_CASCADES];
        for (index, ((volume, texel), reach)) in volumes
            .iter_mut()
            .zip(texels.iter_mut())
            .zip(reaches)
            .enumerate()
            .take(count)
        {
            let fitted = if index + 1 == count {
                whole
            } else {
                fit(reach)
            };
            *texel = fitted.texel;
            *volume = Some(ShadowVolume {
                texel: fitted.texel,
                view_proj: fitted.view_proj,
                range: fitted.range,
            });
        }
        self.last_texels.set(texels);
        if !self.reported.replace(true) {
            println!("shadows: first fit {}", self.texel_report());
        }
        Some(volumes)
    }

    #[must_use]
    pub fn texel_report(&self) -> String {
        let texels = self.last_texels.get();
        let parts: Vec<String> = texels
            .iter()
            .take(self.active_layers())
            .map(|texel| format!("{texel:.2}"))
            .collect();
        format!("{} world units a texel", parts.join(" / "))
    }

    #[must_use]
    pub fn light_view_proj(
        &self,
        renderer: &Renderer,
        camera: Camera,
        rig: &TerrainRig,
        tuning: &WaterTuning,
        world: Aabb,
    ) -> Option<mint_render::Mat4> {
        self.volumes(renderer, camera, rig, tuning, world)?
            .iter()
            .flatten()
            .last()
            .map(|volume| volume.view_proj)
    }

    #[must_use]
    pub fn contact_shadows(&self, tuning: &WaterTuning) -> Option<ContactShadows> {
        let steps = whole_count(tuning.contact_shadow_steps, 0, 64);
        (self.contact_shadows && steps > 0).then(|| ContactShadows {
            length: tuning.contact_shadow_length.max(0.0),
            steps,
            thickness: tuning.contact_shadow_thickness.max(0.0),
            lightness: tuning.shadow_lightness.clamp(0.0, 1.0),
        })
    }

    pub fn render(
        &self,
        renderer: &mut Renderer,
        camera: Camera,
        rig: &TerrainRig,
        instances: &[Instance],
        tuning: &WaterTuning,
        world: Aabb,
    ) -> mint_render::Result<Option<ShadowSample>> {
        let Some(volumes) = self.volumes(renderer, camera, rig, tuning, world) else {
            return Ok(None);
        };
        renderer.set_shadow_alpha_test(true);
        let casters = Scene::new(camera).with_instances(instances);
        let whole = volumes
            .iter()
            .flatten()
            .last()
            .map_or(1.0, |volume| volume.texel.max(1e-6));
        let mut cascades = [None; MAX_SHADOW_CASCADES];
        for (index, (slot, volume)) in cascades.iter_mut().zip(volumes).enumerate() {
            let Some(volume) = volume else {
                break;
            };
            renderer.render_shadow_cascade(self.map, index, volume.view_proj, &casters)?;
            let bias = tuning.shadow_bias.max(0.0) * (volume.texel / whole).min(1.0);
            *slot = Some(ShadowCascade {
                view_proj: volume.view_proj,
                bias: bias / volume.range.max(1e-3),
                texel: volume.texel,
                depth_range: volume.range,
            });
        }
        Ok(Some(ShadowSample {
            map: self.map,
            cascades,
            lightness: tuning.shadow_lightness.clamp(0.0, 1.0),
            edge: self.filter.edge(tuning),
            blend: tuning.shadow_cascade_blend.clamp(0.0, 1.0),
        }))
    }
}

fn whole_count(value: f32, low: u32, high: u32) -> u32 {
    let mut count = low;
    while count < high && value >= count as f32 + 0.5 {
        count += 1;
    }
    count
}

fn ground_start(camera: &Camera, aspect: f32) -> f32 {
    let forward = camera.forward();
    let right = forward.cross(camera.up).normalize();
    let up = right.cross(forward);
    let half_height = mint_math::f32::tan(camera.fov_y * 0.5);
    let half_width = half_height * aspect;
    let drop = camera.eye.z - camera.target.z;
    let mut nearest = f32::INFINITY;
    for side in [-1.0f32, 1.0] {
        let ray = forward - up * half_height + right * (half_width * side);
        if ray.z < -1e-6 && drop > 0.0 {
            nearest = nearest.min(drop / -ray.z);
        }
    }
    if nearest.is_finite() {
        nearest
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_render::Vec3;

    #[test]
    fn the_ground_starts_at_the_bottom_of_the_frame() {
        let camera = Camera {
            eye: Vec3::new(0.0, -100.0, 100.0),
            target: Vec3::new(0.0, 0.0, 0.0),
            up: Vec3::new(0.0, 0.0, 1.0),
            fov_y: std::f32::consts::FRAC_PI_4,
            near: 1.0,
            far: 5000.0,
        };
        let focus = (camera.target - camera.eye).length();
        let start = ground_start(&camera, 16.0 / 9.0);
        assert!(
            start > camera.near && start < focus,
            "{start} should lie between the near plane and the focus at {focus}"
        );
        let level = Camera {
            eye: Vec3::new(0.0, -100.0, 0.0),
            fov_y: 0.1,
            ..camera
        };
        assert_eq!(ground_start(&level, 16.0 / 9.0), 0.0);
    }
}
