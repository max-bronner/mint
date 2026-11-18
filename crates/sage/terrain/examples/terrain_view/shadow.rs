use super::lighting::*;
use super::*;

#[allow(clippy::too_many_arguments)]
pub(crate) const SHADOW_MAP_SIZES: [u32; 6] = [256, 512, 1024, 2048, 4096, 8192];

pub(crate) const SHADOW_MAP_SMALLEST: u32 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ShadowFitMode {
    Target,
    View,
    #[default]
    World,
}

impl ShadowFitMode {
    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "target" => Some(Self::Target),
            "view" => Some(Self::View),
            "world" => Some(Self::World),
            _ => None,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Target => "target",
            Self::View => "view",
            Self::World => "world",
        }
    }

    pub(crate) fn next(self) -> Self {
        match self {
            Self::Target => Self::View,
            Self::View => Self::World,
            Self::World => Self::Target,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ShadowAlpha {
    Off,
    #[default]
    On,
}

impl ShadowAlpha {
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

    pub(crate) fn on(self) -> bool {
        self == Self::On
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ShadowFrame {
    pub(crate) fit: ShadowFitMode,
    pub(crate) alpha: ShadowAlpha,
    pub(crate) world: Aabb,
}

pub(crate) struct ShadowPass {
    pub(crate) map: ShadowMapHandle,
    pub(crate) size: u32,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ShadowVolume {
    pub(crate) view_proj: mint_render::Mat4,
    pub(crate) radius: f32,
    pub(crate) texel: f32,
    pub(crate) range: f32,
}

pub(crate) fn view_shadow_reach(camera: Camera, tuning: &WaterTuning) -> f32 {
    let to_target = Vec3::new(
        camera.target.x - camera.eye.x,
        camera.target.y - camera.eye.y,
        camera.target.z - camera.eye.z,
    )
    .length();
    (to_target * tuning.shadow_reach.max(0.01)).max(16.0)
}

impl ShadowPass {
    pub(crate) fn new(renderer: &mut Renderer, tuning: &WaterTuning) -> Option<Self> {
        let size = SHADOW_MAP_SIZES
            .into_iter()
            .filter(|candidate| f64::from(*candidate) <= f64::from(tuning.shadow_map_size))
            .next_back()
            .unwrap_or(SHADOW_MAP_SMALLEST);
        match renderer.create_shadow_map(size) {
            Ok(map) => {
                let size = renderer.shadow_map_size(map).unwrap_or(size);
                println!("shadows = on (mode.shadows), {size}x{size} shadow map");
                Some(Self { map, size })
            }
            Err(error) => {
                println!("  SHADOW MAP FAILED: {error}");
                None
            }
        }
    }

    pub(crate) fn texel(&self, radius: f32) -> f32 {
        2.0 * radius / f32::from(u16::try_from(self.size).unwrap_or(u16::MAX)).max(1.0)
    }

    pub(crate) fn volume(
        &self,
        renderer: &Renderer,
        camera: Camera,
        rig: &TerrainRig,
        tuning: &WaterTuning,
        frame: ShadowFrame,
    ) -> Option<ShadowVolume> {
        let (fit, world) = (frame.fit, frame.world);
        let sun = rig.lights.first()?;
        let direction = sun.direction;
        if direction.length_squared() <= f32::EPSILON {
            return None;
        }
        match fit {
            ShadowFitMode::Target => {
                let radius = tuning.shadow_radius.max(16.0);
                let depth = radius * 3.0;
                let view_proj =
                    directional_light_view_proj(direction, camera.target, radius, depth);
                Some(ShadowVolume {
                    view_proj,
                    radius,
                    texel: self.texel(radius),
                    range: depth + radius,
                })
            }
            ShadowFitMode::View | ShadowFitMode::World => {
                let (width, height) = renderer.size();
                let aspect = width as f32 / height.max(1) as f32;
                let world_fit = fit == ShadowFitMode::World;
                let fitted = mint_render::fit_directional_light_to_view(
                    direction,
                    &camera,
                    aspect,
                    mint_render::ShadowFit {
                        distance: if world_fit {
                            f32::INFINITY
                        } else {
                            view_shadow_reach(camera, tuning)
                        },
                        margin: 0.0,
                        depth: tuning.shadow_depth.max(0.0),
                        texels: self.size,
                        bounds: (world_fit && !world.is_empty()).then_some(world),
                    },
                );
                Some(ShadowVolume {
                    view_proj: fitted.view_proj,
                    radius: fitted.radius,
                    texel: fitted.texel,
                    range: fitted.range,
                })
            }
        }
    }

    pub(crate) fn report(
        &self,
        renderer: &Renderer,
        camera: Camera,
        rig: &TerrainRig,
        tuning: &WaterTuning,
        frame: ShadowFrame,
    ) -> String {
        let (fit, world) = (frame.fit, frame.world);
        let Some(ShadowVolume { radius, texel, .. }) =
            self.volume(renderer, camera, rig, tuning, frame)
        else {
            return format!(
                "shadow fit = {} (6, --shadow-fit): this rig has no key light, so no map is drawn",
                fit.label()
            );
        };
        let size = self.size;
        match fit {
            ShadowFitMode::Target => format!(
                "shadow fit = target (6, --shadow-fit), {radius:.0} world unit radius around \
                 the camera target (engine.shadow_radius), {texel:.2} world units a texel \
                 over {size}x{size}"
            ),
            ShadowFitMode::View => format!(
                "shadow fit = view (6, --shadow-fit), {radius:.0} world unit radius over the \
                 first {:.0} of the view ({:.2}x the camera's distance to its target, \
                 engine.shadow_reach), {texel:.2} world units a texel over {size}x{size}, \
                 {:.0} of up-light caster headroom (engine.shadow_depth)",
                view_shadow_reach(camera, tuning),
                tuning.shadow_reach.max(0.01),
                tuning.shadow_depth.max(0.0),
            ),
            ShadowFitMode::World => format!(
                "shadow fit = world (6, --shadow-fit), {radius:.0} world unit radius over what                  the camera can see of a map {:.0} x {:.0} x {:.0} (engine.shadow_reach is not                  consulted), {texel:.2} world units a texel over {size}x{size}, {:.0} of                  up-light caster headroom (engine.shadow_depth)",
                world.max().x - world.min().x,
                world.max().y - world.min().y,
                world.max().z - world.min().z,
                tuning.shadow_depth.max(0.0),
            ),
        }
    }

    pub(crate) fn render(
        &self,
        renderer: &mut Renderer,
        camera: Camera,
        rig: &TerrainRig,
        instances: &[Instance],
        tuning: &WaterTuning,
        frame: ShadowFrame,
    ) -> mint_render::Result<Option<ShadowSample>> {
        let Some(volume) = self.volume(renderer, camera, rig, tuning, frame) else {
            return Ok(None);
        };
        renderer.set_shadow_alpha_test(frame.alpha.on());
        renderer.render_shadow_map(
            self.map,
            volume.view_proj,
            &Scene::new(camera).with_instances(instances),
        )?;
        let bias = tuning.shadow_bias.max(0.0) / volume.range.max(1e-3);
        Ok(Some(ShadowSample {
            lightness: tuning.shadow_lightness.clamp(0.0, 1.0),
            ..ShadowSample::single(self.map, volume.view_proj, bias)
        }))
    }
}
