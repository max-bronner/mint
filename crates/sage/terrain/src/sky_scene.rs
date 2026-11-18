use mint_render::{
    Blend, CustomMaterialHandle, CustomShader, CustomShaderHandle, DepthTest, Instance, Mat4,
    MeshHandle, Renderer, TextureHandle, Vec3,
};

use crate::water_tuning::WaterTuning;
use crate::{
    sky_mesh, SkyDraw, SkyLook, SkyPlaneTerms, SkyUniforms, SKY_SHADER_WGSL, SKY_TEXTURE_SLOTS,
    SKY_UNIFORM_SIZE,
};

#[must_use]
pub fn parse_water_sky_texture(text: &str) -> Option<String> {
    parse_water_set(text, None).and_then(|set| set.texture)
}

#[derive(Debug, Clone, PartialEq)]
pub struct WaterSkySet {
    pub name: String,
    pub texture: Option<String>,
    pub plane: SkyPlaneTerms,
    pub diffuse: [f32; 4],
}

#[must_use]
pub fn parse_water_set(text: &str, name: Option<&str>) -> Option<WaterSkySet> {
    let mut current: Option<WaterSkySet> = None;
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let Some(set) = current.as_mut() else {
            let Some(block) = line
                .get(..9)
                .filter(|p| p.eq_ignore_ascii_case("WaterSet "))
                .and_then(|_| line.get(9..))
                .map(str::trim)
            else {
                continue;
            };
            if name.is_none_or(|wanted| wanted.eq_ignore_ascii_case(block)) {
                current = Some(WaterSkySet {
                    name: block.to_owned(),
                    texture: None,
                    plane: SkyPlaneTerms::default(),
                    diffuse: [1.0; 4],
                });
            }
            continue;
        };
        if line.eq_ignore_ascii_case("End") {
            return current;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        let number = || {
            value
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<f32>().ok())
        };
        if key.eq_ignore_ascii_case("SkyTexture") && !value.is_empty() {
            set.texture = Some(value.to_owned());
        } else if key.eq_ignore_ascii_case("Vertex00Color") {
            if let Some(red) = value
                .split_whitespace()
                .find_map(|part| part.strip_prefix("R:"))
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|v| v.is_finite())
            {
                set.plane.tint = (red / 255.0).clamp(0.0, 1.0);
            }
        } else if key.eq_ignore_ascii_case("DiffuseColor") {
            for part in value.split_whitespace() {
                let Some((channel, number)) = part.split_once(':') else {
                    continue;
                };
                let slot = match channel {
                    "R" | "r" => 0,
                    "G" | "g" => 1,
                    "B" | "b" => 2,
                    "A" | "a" => 3,
                    _ => continue,
                };
                if let (Some(v), Some(out)) = (
                    number.parse::<f32>().ok().filter(|v| v.is_finite()),
                    set.diffuse.get_mut(slot),
                ) {
                    *out = (v / 255.0).clamp(0.0, 1.0);
                }
            }
        } else if key.eq_ignore_ascii_case("UScrollPerMS") {
            if let Some(v) = number().filter(|v| v.is_finite()) {
                set.plane.travel = v * 1000.0;
            }
        } else if key.eq_ignore_ascii_case("SkyTexelsPerUnit") {
            if let Some(v) = number().filter(|v| v.is_finite() && *v > 0.0) {
                set.plane.repeat = 256.0 / v;
            }
        }
    }
    None
}

#[derive(Debug, thiserror::Error)]
pub enum SkySceneError {
    #[error("sky shader failed to register: {0}")]
    Shader(#[source] mint_render::Error),
    #[error("sky material failed: {0}")]
    Material(#[source] mint_render::Error),
    #[error("sky mesh failed: {0}")]
    Mesh(#[source] mint_render::Error),
}

pub struct SkyScene {
    mesh: MeshHandle,
    #[allow(dead_code)]
    shader: CustomShaderHandle,
    material: CustomMaterialHandle,
    pub look: SkyLook,
}

impl SkyScene {
    pub fn load(
        renderer: &mut Renderer,
        cloud: Option<TextureHandle>,
        draw: SkyDraw,
        seconds: f32,
        tuning: &WaterTuning,
    ) -> Result<Self, SkySceneError> {
        let shader = renderer
            .register_custom_shader(&CustomShader {
                label: "mint stand-in sky (gradient + water.ini cloud sheet)",
                source: SKY_SHADER_WGSL,
                texture_slots: SKY_TEXTURE_SLOTS,
                uniform_size: SKY_UNIFORM_SIZE,
                writes_alpha: false,
                scene_depth: false,
                scene_color: false,
                ..CustomShader::default()
            })
            .map_err(SkySceneError::Shader)?;
        let look = SkyLook {
            draw,
            cloud_loaded: cloud.is_some(),
            seconds,
            plane: SkyPlaneTerms::default(),
        };
        let block = SkyUniforms::new(&look, tuning).to_bytes();
        let slots = [cloud];
        let material = renderer
            .create_custom_material(shader, &slots, &block)
            .map_err(SkySceneError::Material)?;
        let mesh = renderer
            .upload_mesh(&sky_mesh())
            .map_err(SkySceneError::Mesh)?;
        Ok(Self {
            mesh,
            shader,
            material,
            look,
        })
    }

    #[must_use]
    pub fn instance(&self, eye: Vec3) -> Instance {
        let mut instance = Instance::new(self.mesh);
        instance.transform = Mat4::from_translation(eye);
        instance.custom_material = Some(self.material);
        instance.blend = Blend::Opaque;
        instance.depth_test = DepthTest::NearerOrEqual;
        instance.cull = false;
        instance.unlit = true;
        instance
    }

    pub fn set_plane_terms(&mut self, plane: SkyPlaneTerms) {
        self.look.plane = plane;
    }

    pub fn push(&mut self, renderer: &mut Renderer, tuning: &WaterTuning, seconds: f32) {
        self.look.seconds = seconds;
        let block = SkyUniforms::new(&self.look, tuning).to_bytes();
        let _ = renderer.update_custom_material(self.material, &block);
    }

    #[must_use]
    pub fn report(&self) -> String {
        format!(
            "sky = {} (mode.sky), clouds {} - a stand-in only; a map's real sky is a placed W3D dome",
            self.look.draw.label(),
            if self.look.cloud_loaded {
                "loaded"
            } else {
                "NOT LOADED - gradient only"
            }
        )
    }
}
