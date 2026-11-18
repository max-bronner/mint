use std::collections::BTreeMap;

use mint_render::{
    Blend, CustomMaterialHandle, CustomShader, CustomShaderHandle, DepthTest, Instance, MeshData,
    Renderer, TextureHandle,
};
use mint_sage_model::Resolver;
use mint_sage_terrain::{
    body_absorption, load_ramp, shade_body, BodyShading, DepthRange, RiverLook, RiverNetwork,
    RiverRibbon, RiverUniforms, WaterDepth, WaterDepthMode, WaterDistortion, WaterHighlights,
    WaterNormal, WaterReflection, WaterReflectionComposite, WaterReflectionSource, WaterRefraction,
    WaterShaderParams, WaterSurface, WaterSurfaceLook, WaterTechnique, WaterTint,
    WaterTransparency, WaterTuning, WaterUniforms, WaveField, WaveLook, WaveSurface, WaveUniforms,
    RIVER_SCENE_TEXTURE_SLOTS, RIVER_SCENE_UNIFORM_SIZE, RIVER_SHADER_WGSL, RIVER_TEXTURE_SLOTS,
    RIVER_UNIFORM_SIZE, WATER_SCENE_TEXTURE_SLOTS, WATER_SCENE_UNIFORM_SIZE, WATER_SHADER_WGSL,
    WATER_TEXTURE_SLOTS, WATER_TINT, WATER_UNIFORM_SIZE, WAVE_SCENE_TEXTURE_SLOTS,
    WAVE_SCENE_UNIFORM_SIZE, WAVE_SHADER_WGSL, WAVE_TEXTURE_SLOTS, WAVE_UNIFORM_SIZE,
};
use mint_vfs::Vfs;

pub(crate) const NO_SHROUD_EXTENT: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

const MAX_PLANAR_PLANES: usize = 4;

const PLANAR_HEIGHT_EPSILON: f32 = 0.01;

fn group_by_height(heights: &[f32]) -> (Vec<f32>, Vec<usize>) {
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

fn plane_of_body(group_of_body: &[usize], created: usize) -> Vec<Option<usize>> {
    group_of_body
        .iter()
        .map(|group| (*group < created).then_some(*group))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvSource {
    ShaderMaterial,
    MapRecord,
    None,
}

#[derive(Debug, Clone, Default)]
pub struct WaterMaterial {
    pub params: Option<WaterShaderParams>,
    pub env_texture: Option<String>,
    pub bump_texture: Option<String>,
    pub wave_texture: Option<String>,
    pub diffuse: Option<[f32; 4]>,
}

pub struct WaterScene {
    pub handles: Vec<mint_render::MeshHandle>,
    pub sky: Vec<Option<TextureHandle>>,
    pub bump: Vec<Option<TextureHandle>>,
    pub wave: Vec<Option<TextureHandle>>,
    pub sky_source: Vec<EnvSource>,
    pub params: Vec<WaterShaderParams>,
    pub shader: Option<CustomShaderHandle>,
    pub materials: Vec<Option<CustomMaterialHandle>>,
    pub failures: Vec<String>,
    pub shading: Vec<BodyShading>,
    pub reflects: Vec<bool>,
    pub depth: Vec<WaterDepth>,
    pub planes: Vec<(f32, mint_render::RenderTargetHandle)>,
    pub plane_of: Vec<Option<usize>>,
    pub deep_alpha: Vec<f32>,
}

impl WaterScene {
    #[must_use]
    pub fn from_material(&self, slot: usize) -> bool {
        self.shading.get(slot).is_some_and(|s| s.from_material)
    }

    fn plane_index(&self, slot: usize) -> Option<usize> {
        self.plane_of.get(slot).copied().flatten()
    }

    #[must_use]
    pub fn depth(&self, slot: usize, mode: WaterDepthMode, tuning: &WaterTuning) -> WaterDepth {
        let mut terms = self.depth.get(slot).copied().unwrap_or_default();
        if tuning.env_max_alpha_depth.is_finite() {
            terms.fade_depth *= tuning.env_max_alpha_depth;
        }
        if tuning.env_deep_water_alpha.is_finite() {
            terms.max_alpha = (terms.max_alpha * tuning.env_deep_water_alpha).clamp(0.0, 1.0);
        }
        terms.mode = if mode == WaterDepthMode::Absorb
            && terms.extinction.iter().all(|sigma| *sigma <= 0.0)
        {
            WaterDepthMode::Soft
        } else {
            mode
        };
        terms
    }
}

const UV_SCALES: [&str; 3] = ["BumpUVScale", "WaveUVScale", "EnvUVScale"];

fn uv_pair(value: &mint_sage_w3d::ShaderMaterialValue) -> Option<[f32; 2]> {
    let pair = match value {
        mint_sage_w3d::ShaderMaterialValue::Vec2(v) => *v,
        mint_sage_w3d::ShaderMaterialValue::Vec4([x, y, _, _]) => [*x, *y],
        _ => return None,
    };
    pair.iter().all(|v| v.is_finite()).then_some(pair)
}

fn water_material(vfs: &Vfs, resolver: &Resolver, name: &str) -> WaterMaterial {
    let Some(bytes) = resolver
        .resolve(vfs, name)
        .ok()
        .and_then(|path| vfs.read_path(&path).ok())
    else {
        return WaterMaterial::default();
    };
    let Ok(model) = mint_sage_w3d::Model::parse(&bytes) else {
        return WaterMaterial::default();
    };
    let mut out = WaterMaterial::default();
    let mut params = WaterShaderParams::default();
    let mut seen = false;
    for property in model
        .meshes
        .iter()
        .flat_map(|mesh| mesh.shader_materials.iter())
        .flat_map(|material| material.properties.iter())
    {
        match &property.value {
            mint_sage_w3d::ShaderMaterialValue::Float(value) if value.is_finite() => {
                let slot = match property.name.as_str() {
                    n if n.eq_ignore_ascii_case("WindAngle") => Some(&mut params.wind_angle),
                    n if n.eq_ignore_ascii_case("WaveAngle") => Some(&mut params.wave_angle),
                    n if n.eq_ignore_ascii_case("WindSpeed") => Some(&mut params.wind_speed),
                    n if n.eq_ignore_ascii_case("WaveSpeed") => Some(&mut params.wave_speed),
                    n if n.eq_ignore_ascii_case("BumpSpeed") => Some(&mut params.bump_speed),
                    n if n.eq_ignore_ascii_case("BumpScale") => Some(&mut params.bump_scale),
                    n if n.eq_ignore_ascii_case("DimmingFactor") => {
                        Some(&mut params.dimming_factor)
                    }
                    n if n.eq_ignore_ascii_case("ReflectionStrength") => {
                        Some(&mut params.reflection_strength)
                    }
                    n if n.eq_ignore_ascii_case("ReflectionPerturbation") => {
                        Some(&mut params.reflection_perturbation)
                    }
                    n if n.eq_ignore_ascii_case("SpecularExponent") => {
                        Some(&mut params.specular_exponent)
                    }
                    n if n.eq_ignore_ascii_case("ShadowLightness") => {
                        params.shadow_lightness = Some(*value);
                        seen = true;
                        None
                    }
                    _ => None,
                };
                if let Some(slot) = slot {
                    *slot = *value;
                    seen = true;
                }
            }
            value
                if UV_SCALES
                    .iter()
                    .any(|n| property.name.eq_ignore_ascii_case(n))
                    && uv_pair(value).is_some() =>
            {
                let pair = uv_pair(value).unwrap_or([1.0, 1.0]);
                let name = property.name.as_str();
                if name.eq_ignore_ascii_case("BumpUVScale") {
                    params.bump_uv_scale = pair;
                } else if name.eq_ignore_ascii_case("WaveUVScale") {
                    params.wave_uv_scale = pair;
                } else {
                    params.env_uv_scale = pair;
                }
                seen = true;
            }
            mint_sage_w3d::ShaderMaterialValue::Vec4(value)
                if value.iter().all(|v| v.is_finite()) =>
            {
                match property.name.as_str() {
                    n if n.eq_ignore_ascii_case("MaterialColorDiffuse")
                        && out.diffuse.is_none() =>
                    {
                        out.diffuse = Some([value[0], value[1], value[2], value[3]]);
                    }
                    n if n.eq_ignore_ascii_case("SpecularColor") => {
                        params.specular_color = [value[0], value[1], value[2]];
                        seen = true;
                    }
                    _ => {}
                }
            }
            mint_sage_w3d::ShaderMaterialValue::Texture(texture)
                if !texture.is_empty() && !texture.eq_ignore_ascii_case("None") =>
            {
                let slot = match property.name.as_str() {
                    n if n.eq_ignore_ascii_case("WaterEnvTexture") => Some(&mut out.env_texture),
                    n if n.eq_ignore_ascii_case("WaterBumpTexture") => Some(&mut out.bump_texture),
                    n if n.eq_ignore_ascii_case("WaveTexture") => Some(&mut out.wave_texture),
                    _ => None,
                };
                if let Some(slot) = slot {
                    if slot.is_none() {
                        *slot = Some(texture.clone());
                    }
                }
            }
            _ => {}
        }
    }
    if seen {
        out.params = Some(params);
    }
    out
}

#[allow(clippy::too_many_arguments)]
pub fn upload_water(
    renderer: &mut Renderer,
    vfs: &Vfs,
    resolver: &Resolver,
    surface: &WaterSurface,
    transparency: WaterTransparency,
    vertical_scale: f32,
    tint: WaterTint,
    reflection: WaterReflection,
    reflection_source: WaterReflectionSource,
    reflection_composite: WaterReflectionComposite,
    highlights: WaterHighlights,
    distortion: WaterDistortion,
    tuning: &WaterTuning,
    waves: &WaveSurface,
) -> mint_render::Result<WaterScene> {
    let shader = match renderer.register_custom_shader(&CustomShader {
        label: "bfme water (DefaultTechnique)",
        source: WATER_SHADER_WGSL,
        texture_slots: WATER_TEXTURE_SLOTS,
        uniform_size: WATER_UNIFORM_SIZE,
        scene_texture_slots: WATER_SCENE_TEXTURE_SLOTS,
        scene_uniform_size: WATER_SCENE_UNIFORM_SIZE,
        writes_alpha: false,
        scene_depth: true,
        scene_color: true,
        ..CustomShader::default()
    }) {
        Ok(handle) => Some(handle),
        Err(error) => {
            println!("  WATER SHADER FAILED TO REGISTER: {error}");
            None
        }
    };
    if let Err(error) = crate::bind_shroud_scene(renderer, shader, None, NO_SHROUD_EXTENT, 0.0) {
        println!("  water shroud binding: {error}");
    }
    let mut scene = WaterScene {
        handles: Vec::with_capacity(surface.bodies.len()),
        sky: Vec::with_capacity(surface.bodies.len()),
        bump: Vec::with_capacity(surface.bodies.len()),
        wave: Vec::with_capacity(surface.bodies.len()),
        sky_source: Vec::with_capacity(surface.bodies.len()),
        params: Vec::with_capacity(surface.bodies.len()),
        shader,
        materials: Vec::with_capacity(surface.bodies.len()),
        failures: Vec::new(),
        shading: Vec::with_capacity(surface.bodies.len()),
        reflects: Vec::with_capacity(surface.bodies.len()),
        depth: Vec::with_capacity(surface.bodies.len()),
        planes: Vec::new(),
        plane_of: Vec::with_capacity(surface.bodies.len()),
        deep_alpha: Vec::with_capacity(surface.bodies.len()),
    };
    let (frame_width, frame_height) = renderer.size();
    let (target_width, target_height) = (
        frame_width.div_ceil(2).max(1),
        frame_height.div_ceil(2).max(1),
    );
    let heights: Vec<f32> = surface.bodies.iter().map(|body| body.height).collect();
    let (distinct, group_of_body) = group_by_height(&heights);
    for height in distinct.iter().take(MAX_PLANAR_PLANES) {
        match renderer.create_render_target(target_width, target_height) {
            Ok(target) => scene.planes.push((*height, target)),
            Err(error) => {
                scene
                    .failures
                    .push(format!("planar reflection target: {error}"));
                break;
            }
        }
    }
    scene.plane_of = plane_of_body(&group_of_body, scene.planes.len());
    let mut ramps: BTreeMap<String, Option<Vec<[f32; 3]>>> = BTreeMap::new();
    let mut materials: BTreeMap<String, WaterMaterial> = BTreeMap::new();
    let compressed = renderer.supports_block_compression();
    let mut uploaded: BTreeMap<String, Option<TextureHandle>> = BTreeMap::new();
    let mut depth_range = DepthRange::default();
    for (slot, body) in surface.bodies.iter().enumerate() {
        let material = if body.fx_shader.is_empty() {
            WaterMaterial::default()
        } else {
            let key = body.fx_shader.to_ascii_lowercase();
            match materials.get(&key) {
                Some(material) => material.clone(),
                None => {
                    let material = water_material(vfs, resolver, &body.fx_shader);
                    materials.insert(key, material.clone());
                    material
                }
            }
        };
        let params = material.params.unwrap_or_default();
        scene.params.push(params);

        let mut shading = BodyShading {
            color: material.diffuse.unwrap_or(WATER_TINT),
            from_material: material.diffuse.is_some(),
            ramp: None,
            ramp_name: body.depth_colors.clone(),
        };
        if !body.depth_colors.is_empty() {
            let key = body.depth_colors.to_ascii_lowercase();
            let ramp = match ramps.get(&key) {
                Some(ramp) => ramp.clone(),
                None => {
                    let loaded = match load_ramp(vfs, &body.depth_colors) {
                        Ok(ramp) => Some(ramp),
                        Err(error) => {
                            scene.failures.push(format!("depthColors {error}"));
                            None
                        }
                    };
                    ramps.insert(key, loaded.clone());
                    loaded
                }
            };
            shading.ramp = ramp;
        }
        let (vertices, range) = shade_body(body, &shading, transparency, vertical_scale, tint);
        depth_range.merge(range);
        scene.shading.push(shading);
        scene.handles.push(renderer.upload_mesh(&MeshData {
            vertices,
            indices: body.mesh.indices.clone(),
        })?);
        let (env_name, source) = match &material.env_texture {
            Some(name) => (name.as_str(), EnvSource::ShaderMaterial),
            None => (body.sky_texture.as_str(), EnvSource::MapRecord),
        };
        let bump_name = material
            .bump_texture
            .as_deref()
            .unwrap_or(body.bump_texture.as_str());
        let wave_name = material.wave_texture.as_deref().unwrap_or("");
        for (name, into) in [
            (env_name, &mut scene.sky),
            (bump_name, &mut scene.bump),
            (wave_name, &mut scene.wave),
        ] {
            if name.is_empty() {
                into.push(None);
                continue;
            }
            let key = name.to_ascii_lowercase();
            let handle = match uploaded.get(&key) {
                Some(handle) => *handle,
                None => {
                    let handle =
                        match mint_sage_model::load_texture(vfs, resolver, name, compressed) {
                            Ok(loaded) => Some(renderer.upload_texture(&loaded.data)?),
                            Err(error) => {
                                scene.failures.push(format!("{name}: {error}"));
                                None
                            }
                        };
                    uploaded.insert(key, handle);
                    handle
                }
            };
            into.push(handle);
        }
        scene.sky_source.push(match scene.sky.last() {
            Some(Some(_)) => source,
            _ => EnvSource::None,
        });
        let reflects = matches!(scene.sky.last(), Some(Some(_)));
        scene.reflects.push(reflects);
        let body_reflection = if reflects {
            reflection
        } else {
            WaterReflection::Off
        };
        let body_source = if reflects {
            reflection_source
        } else {
            WaterReflectionSource::Environment
        };

        let shading = scene.shading.last().cloned().unwrap_or_default();
        let fit = body_absorption(&shading, transparency);
        let body_depth = WaterDepth {
            mode: WaterDepthMode::Vertex,
            extinction: fit.map_or([0.0; 3], |(fit, _)| fit.extinction),
            fade_depth: transparency.transparent_depth(),
            body: fit.map_or(
                [shading.color[0], shading.color[1], shading.color[2]],
                |(fit, _)| fit.body,
            ),
            max_alpha: transparency.max_alpha(shading.color[3]),
            additive: body.additive_blend,
            uniform_colour: false,
        };
        scene.depth.push(body_depth);
        scene.deep_alpha.push(transparency.max_alpha(1.0));

        let planar = scene
            .plane_index(slot)
            .and_then(|index| scene.planes.get(index))
            .and_then(|(_, target)| renderer.render_target_texture(*target));
        let slots = [
            scene.bump.last().copied().flatten(),
            scene.sky.last().copied().flatten(),
            scene.wave.last().copied().flatten(),
            planar,
        ];
        let built = shader.and_then(|shader| {
            let block = WaterUniforms::new(
                &params,
                WaterTechnique::Default,
                body_reflection,
                distortion,
                WaterSurfaceLook {
                    source: body_source,
                    composite: reflection_composite,
                    highlights,
                },
                body_depth,
                tuning,
                0.0,
            )
            .to_bytes(waves, tuning, 0.0);
            match renderer.create_custom_material(shader, &slots, &block) {
                Ok(handle) => Some(handle),
                Err(error) => {
                    scene
                        .failures
                        .push(format!("{}: water material: {error}", body.name));
                    None
                }
            }
        });
        scene.materials.push(built);
    }
    for line in &scene.failures {
        println!("  WATER TEXTURE FAILED {line}");
    }
    if !surface.bodies.is_empty() {
        let with_material = scene.materials.iter().filter(|m| m.is_some()).count();
        println!(
            "water: {with_material} of {} bod(ies) carry a registered water material",
            surface.bodies.len(),
        );
    }
    Ok(scene)
}

#[must_use]
pub fn water_instances(
    scene: &WaterScene,
    surface: &WaterSurface,
    tint_mode: WaterTint,
) -> Vec<Instance> {
    scene
        .handles
        .iter()
        .enumerate()
        .map(|(slot, handle)| {
            let body = surface.bodies.get(slot);
            let additive = body.is_some_and(|body| body.additive_blend);
            let custom_material = scene.materials.get(slot).copied().flatten();
            Instance {
                cull: false,
                custom_material,
                tint: if tint_mode.uses_depth() {
                    [1.0, 1.0, 1.0, 1.0]
                } else {
                    WATER_TINT
                },
                ambient_from_vertex_color: custom_material.is_none() && tint_mode.uses_depth(),
                diffuse_from_vertex_color: custom_material.is_none() && tint_mode.uses_depth(),
                blend: standing_blend(additive, scene.from_material(slot)),
                depth_test: DepthTest::Nearer,
                ..Instance::new(*handle)
            }
        })
        .collect()
}

#[must_use]
fn standing_blend(additive: bool, from_material: bool) -> Blend {
    if additive && !from_material {
        Blend::Additive
    } else {
        Blend::AlphaBlend
    }
}

const STANDING_DISTORTION: WaterDistortion = WaterDistortion {
    refraction: WaterRefraction::Off,
    normal: WaterNormal::Unit,
};
const STANDING_COMPOSITE: WaterReflectionComposite = WaterReflectionComposite::Surface;
const STANDING_HIGHLIGHTS: WaterHighlights = WaterHighlights::Off;
const STANDING_DEPTH_MODE: WaterDepthMode = WaterDepthMode::Soft;

fn standing_tuning(tuning: &WaterTuning) -> WaterTuning {
    WaterTuning {
        water_ripple_detail: 0.0,
        water_ripple_lod_bias: 0.0,
        water_reflection_blur: 0.0,
        water_ripple_light: 0.0,
        ..*tuning
    }
}

fn standing_depth(
    base: WaterDepth,
    colour: [f32; 4],
    from_material: bool,
    deep_alpha: f32,
) -> WaterDepth {
    if !from_material {
        return base;
    }
    WaterDepth {
        body: [colour[0], colour[1], colour[2]],
        max_alpha: if deep_alpha.is_finite() {
            deep_alpha.clamp(0.0, 1.0)
        } else {
            1.0
        },
        additive: false,
        uniform_colour: true,
        ..base
    }
}

pub fn push_water_uniforms(
    renderer: &mut Renderer,
    scene: &WaterScene,
    reflection_source: Option<WaterReflectionSource>,
    tuning: &WaterTuning,
    wave_surface: &WaveSurface,
    seconds: f32,
) {
    let tuning = &standing_tuning(tuning);
    for (slot, params) in scene.params.iter().enumerate() {
        let Some(handle) = scene.materials.get(slot).copied().flatten() else {
            continue;
        };
        let reflects = scene.reflects.get(slot).copied().unwrap_or(false);
        let source = match reflection_source {
            Some(source) if reflects => source,
            _ => WaterReflectionSource::Environment,
        };
        let reflection = if reflects && reflection_source.is_some() {
            WaterReflection::Flat
        } else {
            WaterReflection::Off
        };
        let colour = scene.shading.get(slot).map_or(WATER_TINT, |s| s.color);
        let depth = standing_depth(
            scene.depth(slot, STANDING_DEPTH_MODE, tuning),
            colour,
            scene.from_material(slot),
            scene.deep_alpha.get(slot).copied().unwrap_or(1.0),
        );
        let block = WaterUniforms::new(
            params,
            WaterTechnique::Default,
            reflection,
            STANDING_DISTORTION,
            WaterSurfaceLook {
                source,
                composite: STANDING_COMPOSITE,
                highlights: STANDING_HIGHLIGHTS,
            },
            depth,
            tuning,
            seconds,
        )
        .to_bytes(wave_surface, tuning, seconds);
        if let Err(error) = renderer.update_custom_material(handle, &block) {
            eprintln!("  could not update a water material: {error}");
        }
    }
}

const DEFAULT_RIVER_RIPPLE: &str = "WaterRippleBump.tga";

const RIVER_PLANE_TOLERANCE: f32 = 4.0;

#[must_use]
pub fn is_lava(ribbon: &RiverRibbon) -> bool {
    ribbon.river_texture.to_ascii_lowercase().contains("lava")
}

#[must_use]
pub fn over_standing_water(ribbon: &RiverRibbon, surface: &WaterSurface) -> bool {
    let (Some(x), Some(y)) = (ribbon.centre.first(), ribbon.centre.get(1)) else {
        return false;
    };
    surface.bodies.iter().any(|body| {
        if (body.height - ribbon.height).abs() > 12.0 {
            return false;
        }
        let mut min = [f32::MAX; 2];
        let mut max = [f32::MIN; 2];
        for vertex in &body.mesh.vertices {
            for axis in 0..2 {
                let (Some(value), Some(low), Some(high)) = (
                    vertex.position.get(axis).copied(),
                    min.get_mut(axis),
                    max.get_mut(axis),
                ) else {
                    continue;
                };
                *low = low.min(value);
                *high = high.max(value);
            }
        }
        min.first().is_some_and(|v| *x >= *v)
            && max.first().is_some_and(|v| *x <= *v)
            && min.get(1).is_some_and(|v| *y >= *v)
            && max.get(1).is_some_and(|v| *y <= *v)
    })
}

#[must_use]
pub fn is_over_water(
    wet: &mint_sage_passability::water::WetGrid,
    height_map: &mint_sage_map::HeightMapData,
    x: f32,
    y: f32,
) -> bool {
    let border = f32::from(u16::try_from(height_map.border_size).unwrap_or(u16::MAX));
    let sample_x = x / mint_sage_map::MAP_XY_FACTOR + border;
    let sample_y = y / mint_sage_map::MAP_XY_FACTOR + border;
    if !sample_x.is_finite() || !sample_y.is_finite() {
        return false;
    }
    #[allow(clippy::cast_possible_truncation)]
    let (rounded_x, rounded_y) = (sample_x.round() as i64, sample_y.round() as i64);
    let (Ok(sample_x), Ok(sample_y)) = (u32::try_from(rounded_x), u32::try_from(rounded_y)) else {
        return false;
    };
    wet.at(sample_x, sample_y) != mint_sage_passability::water::Wetness::Dry
}

pub struct RiverScene {
    pub handles: Vec<mint_render::MeshHandle>,
    pub texture: Vec<Option<TextureHandle>>,
    pub opacity: Vec<Option<TextureHandle>>,
    pub noise: Vec<Option<TextureHandle>>,
    pub sparkle: Vec<Option<TextureHandle>>,
    pub ripple: Option<TextureHandle>,
    pub shader: Option<CustomShaderHandle>,
    pub materials: Vec<Option<CustomMaterialHandle>>,
    pub ripples: Vec<bool>,
    pub mirrors: Vec<Option<mint_render::RenderTargetHandle>>,
    pub refracts: Vec<bool>,
    pub failures: Vec<String>,
}

#[allow(clippy::too_many_arguments)]
pub fn upload_rivers(
    renderer: &mut Renderer,
    vfs: &Vfs,
    resolver: &Resolver,
    network: &RiverNetwork,
    surface: &WaterSurface,
    planes: &[(f32, mint_render::RenderTargetHandle)],
    look: RiverLook,
    refraction: WaterRefraction,
    tuning: &WaterTuning,
) -> mint_render::Result<RiverScene> {
    let shader = match renderer.register_custom_shader(&CustomShader {
        label: "bfme river (riverwater.fxo Default_M, plus the design's ripples)",
        source: RIVER_SHADER_WGSL,
        texture_slots: RIVER_TEXTURE_SLOTS,
        uniform_size: RIVER_UNIFORM_SIZE,
        scene_texture_slots: RIVER_SCENE_TEXTURE_SLOTS,
        scene_uniform_size: RIVER_SCENE_UNIFORM_SIZE,
        writes_alpha: false,
        scene_depth: true,
        scene_color: true,
        ..CustomShader::default()
    }) {
        Ok(handle) => Some(handle),
        Err(error) => {
            println!("  RIVER SHADER FAILED TO REGISTER: {error}");
            None
        }
    };
    if let Err(error) = crate::bind_shroud_scene(renderer, shader, None, NO_SHROUD_EXTENT, 0.0) {
        println!("  river shroud binding: {error}");
    }
    let mut scene = RiverScene {
        handles: Vec::with_capacity(network.ribbons.len()),
        texture: Vec::with_capacity(network.ribbons.len()),
        opacity: Vec::with_capacity(network.ribbons.len()),
        noise: Vec::with_capacity(network.ribbons.len()),
        sparkle: Vec::with_capacity(network.ribbons.len()),
        ripple: None,
        shader,
        materials: Vec::with_capacity(network.ribbons.len()),
        ripples: Vec::with_capacity(network.ribbons.len()),
        refracts: Vec::with_capacity(network.ribbons.len()),
        mirrors: Vec::with_capacity(network.ribbons.len()),
        failures: Vec::new(),
    };
    let compressed = renderer.supports_block_compression();
    let mut uploaded: BTreeMap<String, Option<TextureHandle>> = BTreeMap::new();
    let mut failures: Vec<String> = Vec::new();
    let load = |renderer: &mut Renderer,
                uploaded: &mut BTreeMap<String, Option<TextureHandle>>,
                failures: &mut Vec<String>,
                name: &str|
     -> mint_render::Result<Option<TextureHandle>> {
        if name.is_empty() {
            return Ok(None);
        }
        let key = name.to_ascii_lowercase();
        if let Some(handle) = uploaded.get(&key) {
            return Ok(*handle);
        }
        let handle = match mint_sage_model::load_texture(vfs, resolver, name, compressed) {
            Ok(loaded) => Some(renderer.upload_texture(&loaded.data)?),
            Err(error) => {
                failures.push(format!("{name}: {error}"));
                None
            }
        };
        uploaded.insert(key, handle);
        Ok(handle)
    };

    let ripple_name = surface
        .bodies
        .iter()
        .map(|body| body.bump_texture.as_str())
        .find(|name| !name.is_empty())
        .unwrap_or(DEFAULT_RIVER_RIPPLE)
        .to_owned();
    scene.ripple = load(renderer, &mut uploaded, &mut failures, &ripple_name)?;

    for ribbon in &network.ribbons {
        scene.handles.push(renderer.upload_mesh(&ribbon.mesh)?);
        for (name, into) in [
            (&ribbon.river_texture, &mut scene.texture),
            (&ribbon.alpha_edge_texture, &mut scene.opacity),
            (&ribbon.noise_texture, &mut scene.noise),
            (&ribbon.sparkle_texture, &mut scene.sparkle),
        ] {
            let handle = load(renderer, &mut uploaded, &mut failures, name)?;
            into.push(handle);
        }
        scene
            .ripples
            .push(!is_lava(ribbon) && scene.ripple.is_some());
        scene.refracts.push(
            !ribbon.additive_blend && !is_lava(ribbon) && !over_standing_water(ribbon, surface),
        );
        let height = ribbon.height;
        scene
            .mirrors
            .push(if is_lava(ribbon) || over_standing_water(ribbon, surface) {
                None
            } else {
                planes
                    .iter()
                    .find(|(plane, _)| (plane - height).abs() <= RIVER_PLANE_TOLERANCE)
                    .map(|(_, target)| *target)
            });
    }
    scene.failures = failures;

    for slot in 0..scene.handles.len() {
        let Some(shader) = shader else {
            scene.materials.push(None);
            continue;
        };
        let ribbon = network.ribbons.get(slot);
        let slots = [
            scene.texture.get(slot).copied().flatten(),
            scene.opacity.get(slot).copied().flatten(),
            scene.noise.get(slot).copied().flatten(),
            scene.sparkle.get(slot).copied().flatten(),
            scene.ripple,
            scene
                .mirrors
                .get(slot)
                .copied()
                .flatten()
                .and_then(|target| renderer.render_target_texture(target)),
        ];
        let block = RiverUniforms::new(
            ribbon.map_or(0.0, |ribbon| ribbon.uv_scroll_speed),
            ribbon.map_or(mint_sage_terrain::FALLBACK_RIVER_WIDTH, |ribbon| {
                ribbon.width
            }),
            ribbon.map_or(1.0, |ribbon| ribbon.tint[3]),
            ribbon.is_some_and(|ribbon| ribbon.additive_blend),
            look,
            scene.ripples.get(slot).copied().unwrap_or(false),
            if scene.refracts.get(slot).copied().unwrap_or(false) {
                refraction
            } else {
                WaterRefraction::Off
            },
            scene.mirrors.get(slot).copied().flatten().is_some(),
            tuning,
            0.0,
        )
        .to_bytes();
        match renderer.create_custom_material(shader, &slots, &block) {
            Ok(handle) => scene.materials.push(Some(handle)),
            Err(error) => {
                scene.failures.push(format!("river material: {error}"));
                scene.materials.push(None);
            }
        }
    }
    for line in &scene.failures {
        println!("  RIVER TEXTURE FAILED {line}");
    }
    Ok(scene)
}

#[must_use]
pub fn river_instances(scene: &RiverScene, network: &RiverNetwork) -> Vec<Instance> {
    scene
        .handles
        .iter()
        .enumerate()
        .map(|(slot, handle)| {
            let ribbon = network.ribbons.get(slot);
            let additive = ribbon.is_some_and(|ribbon| ribbon.additive_blend);
            let material = scene.materials.get(slot).copied().flatten();
            let tint = ribbon.map_or([1.0, 1.0, 1.0, 1.0], |ribbon| ribbon.tint);
            Instance {
                cull: false,
                custom_material: material,
                tint,
                blend: if additive {
                    Blend::Additive
                } else {
                    Blend::AlphaBlend
                },
                depth_test: DepthTest::Nearer,
                ..Instance::new(*handle)
            }
        })
        .collect()
}

pub fn push_river_uniforms(
    renderer: &mut Renderer,
    scene: &RiverScene,
    network: &RiverNetwork,
    look: RiverLook,
    tuning: &WaterTuning,
    seconds: f32,
) {
    for (slot, handle) in scene.materials.iter().enumerate() {
        let Some(handle) = handle else {
            continue;
        };
        let ribbon = network.ribbons.get(slot);
        let block = RiverUniforms::new(
            ribbon.map_or(0.0, |ribbon| ribbon.uv_scroll_speed),
            ribbon.map_or(mint_sage_terrain::FALLBACK_RIVER_WIDTH, |ribbon| {
                ribbon.width
            }),
            ribbon.map_or(1.0, |ribbon| ribbon.tint[3]),
            ribbon.is_some_and(|ribbon| ribbon.additive_blend),
            look,
            false,
            WaterRefraction::Off,
            scene.mirrors.get(slot).copied().flatten().is_some(),
            tuning,
            seconds,
        )
        .to_bytes();
        if let Err(error) = renderer.update_custom_material(*handle, &block) {
            eprintln!("  could not update a river material: {error}");
        }
    }
}

pub struct WaveScene {
    pub handles: Vec<mint_render::MeshHandle>,
    pub texture: Vec<Option<TextureHandle>>,
    pub shader: Option<CustomShaderHandle>,
    pub materials: Vec<Option<CustomMaterialHandle>>,
    pub failures: Vec<String>,
}

pub fn upload_waves(
    renderer: &mut Renderer,
    vfs: &Vfs,
    resolver: &Resolver,
    field: &WaveField,
    look: WaveLook,
    tuning: &WaterTuning,
) -> mint_render::Result<WaveScene> {
    let mut scene = WaveScene {
        handles: Vec::with_capacity(field.areas.len()),
        texture: Vec::with_capacity(field.areas.len()),
        shader: None,
        materials: Vec::with_capacity(field.areas.len()),
        failures: Vec::new(),
    };
    if field.areas.is_empty() {
        return Ok(scene);
    }
    scene.shader = match renderer.register_custom_shader(&CustomShader {
        label: "bfme standing wave (StandingWaveAreas, ours entirely)",
        source: WAVE_SHADER_WGSL,
        texture_slots: WAVE_TEXTURE_SLOTS,
        uniform_size: WAVE_UNIFORM_SIZE,
        scene_texture_slots: WAVE_SCENE_TEXTURE_SLOTS,
        scene_uniform_size: WAVE_SCENE_UNIFORM_SIZE,
        writes_alpha: false,
        scene_depth: true,
        scene_color: false,
        ..CustomShader::default()
    }) {
        Ok(handle) => Some(handle),
        Err(error) => {
            println!("  WAVE SHADER FAILED TO REGISTER: {error}");
            None
        }
    };
    if let Err(error) =
        crate::bind_shroud_scene(renderer, scene.shader, None, NO_SHROUD_EXTENT, 0.0)
    {
        println!("  wave shroud binding: {error}");
    }
    let compressed = renderer.supports_block_compression();
    let mut uploaded: BTreeMap<String, Option<TextureHandle>> = BTreeMap::new();
    for area in &field.areas {
        scene.handles.push(renderer.upload_mesh(&area.mesh)?);
        let handle = if area.texture.is_empty() {
            None
        } else {
            let key = area.texture.to_ascii_lowercase();
            match uploaded.get(&key) {
                Some(handle) => *handle,
                None => {
                    let name = area.texture.as_str();
                    let handle =
                        match mint_sage_model::load_texture(vfs, resolver, name, compressed) {
                            Ok(loaded) => Some(renderer.upload_texture(&loaded.data)?),
                            Err(error) => {
                                scene.failures.push(format!("{name}: {error}"));
                                None
                            }
                        };
                    uploaded.insert(key, handle);
                    handle
                }
            }
        };
        scene.texture.push(handle);
        let built = scene.shader.and_then(|shader| {
            let block = WaveUniforms::new(&area.params, look, tuning, 0.0).to_bytes();
            match renderer.create_custom_material(shader, &[handle], &block) {
                Ok(built) => Some(built),
                Err(error) => {
                    scene
                        .failures
                        .push(format!("#{}: wave material: {error}", area.unique_id));
                    None
                }
            }
        });
        scene.materials.push(built);
    }
    for line in &scene.failures {
        println!("  WAVE TEXTURE FAILED {line}");
    }
    Ok(scene)
}

#[must_use]
pub fn wave_instances(scene: &WaveScene, field: &WaveField, look: WaveLook) -> Vec<Instance> {
    if !look.draw.on() {
        return Vec::new();
    }
    scene
        .handles
        .iter()
        .enumerate()
        .filter_map(|(slot, handle)| {
            let pca = field.areas.get(slot).is_some_and(|area| area.params.pca);
            if !look.pca.draws_sprite(pca) {
                return None;
            }
            let material = scene.materials.get(slot).copied().flatten()?;
            Some(Instance {
                cull: false,
                custom_material: Some(material),
                blend: Blend::AlphaBlend,
                depth_test: DepthTest::Nearer,
                ..Instance::new(*handle)
            })
        })
        .collect()
}

pub fn push_wave_uniforms(
    renderer: &mut Renderer,
    scene: &WaveScene,
    field: &WaveField,
    look: WaveLook,
    sprites: Option<bool>,
    swell_gain: f32,
    tuning: &WaterTuning,
    seconds: f32,
) {
    let unit = look.scale.world_units() * tuning.wave_unit * tuning.wave_final_height;
    for (slot, handle) in scene.materials.iter().enumerate() {
        let (Some(handle), Some(area)) = (handle, field.areas.get(slot)) else {
            continue;
        };
        let shown = sprites.unwrap_or_else(|| look.pca.draws_sprite(area.params.pca));
        let lift = if area.params.pca && look.pca.displaces() {
            area.params.final_height * unit * tuning.wave_surface_amplitude * swell_gain
        } else {
            0.0
        };
        let block = WaveUniforms::new(&area.params, look, tuning, seconds).lifted(lift);
        let block = if shown { block } else { block.hidden() }.to_bytes();
        if let Err(error) = renderer.update_custom_material(*handle, &block) {
            eprintln!("  could not update a wave material: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_uv_scale_reads_from_either_width_and_refuses_the_rest() {
        use mint_sage_w3d::ShaderMaterialValue as V;
        assert_eq!(uv_pair(&V::Vec2([15.0, 15.0])), Some([15.0, 15.0]));
        assert_eq!(uv_pair(&V::Vec4([1.2, 3.0, 0.0, 0.0])), Some([1.2, 3.0]));
        assert_eq!(uv_pair(&V::Vec2([f32::NAN, 1.0])), None);
        assert_eq!(uv_pair(&V::Float(2.0)), None);
    }

    #[test]
    fn heights_within_epsilon_share_one_group() {
        let (distinct, of_body) = group_by_height(&[30.0, 295.0, 30.0001, 382.0, 295.0]);
        assert_eq!(distinct, vec![30.0, 295.0, 382.0]);
        assert_eq!(of_body, vec![0, 1, 0, 2, 1]);
    }

    #[test]
    fn heights_past_the_epsilon_get_their_own_group() {
        let (distinct, of_body) = group_by_height(&[30.0, 30.5]);
        assert_eq!(distinct, vec![30.0, 30.5]);
        assert_eq!(of_body, vec![0, 1]);
    }

    #[test]
    fn a_group_past_the_created_count_maps_to_no_plane() {
        let plane_of = plane_of_body(&[0, 1, 2, 0], 2);
        assert_eq!(plane_of, vec![Some(0), Some(1), None, Some(0)]);
    }

    #[test]
    fn no_targets_created_leaves_every_body_on_the_environment_source() {
        let plane_of = plane_of_body(&[0, 0, 1], 0);
        assert_eq!(plane_of, vec![None, None, None]);
    }

    #[test]
    fn is_over_water_follows_the_maps_wet_grid() {
        use mint_sage_map::{StandingWaterArea, Water, WaterAreas};
        let height_map = mint_sage_map::HeightMapData {
            width: 20,
            height: 20,
            border_size: 0,
            boundaries: Vec::new(),
            samples: vec![0; 400],
        };
        let water = Water {
            standing: Some(WaterAreas {
                version: 2,
                areas: vec![StandingWaterArea {
                    unique_id: 1,
                    name: String::new(),
                    layer_name: String::new(),
                    uv_scroll_speed: 0.0,
                    additive_blend: false,
                    bump_texture: String::new(),
                    sky_texture: String::new(),
                    points: vec![[5.0, 5.0], [95.0, 5.0], [95.0, 95.0], [5.0, 95.0]],
                    water_height: 10,
                    fx_shader: String::new(),
                    depth_colors: String::new(),
                }],
            }),
            rivers: None,
            waves: None,
        };
        let wet = mint_sage_passability::water::WetGrid::build(
            &height_map,
            1.0,
            &water,
            mint_sage_passability::water::WaterDepths::default(),
        );
        assert!(is_over_water(&wet, &height_map, 50.0, 50.0));
        assert!(!is_over_water(&wet, &height_map, -50.0, 50.0));
        let dry_map = mint_sage_passability::water::WetGrid::dry(20, 20);
        assert!(!is_over_water(&dry_map, &height_map, 50.0, 50.0));
    }

    #[test]
    fn is_lava_reads_the_ribbons_own_texture_name_case_insensitively() {
        let mut ribbon = river_ribbon_for_test();
        ribbon.river_texture = "TLava_Mor01.tga".to_owned();
        assert!(is_lava(&ribbon));
        ribbon.river_texture = "TWWaterEmpty.tga".to_owned();
        assert!(!is_lava(&ribbon));
    }

    fn river_ribbon_for_test() -> RiverRibbon {
        RiverRibbon {
            unique_id: 0,
            name: String::new(),
            height: 0.0,
            additive_blend: false,
            river_texture: String::new(),
            noise_texture: String::new(),
            alpha_edge_texture: String::new(),
            sparkle_texture: String::new(),
            tint: [1.0, 1.0, 1.0, 1.0],
            uv_scroll_speed: 0.0,
            minimum_water_lod: String::new(),
            mesh: mint_render::MeshData::new(),
            sections: 0,
            length: 0.0,
            width: 1.0,
            repeats: 0.0,
            flow: [0.0, 0.0],
            centre: [0.0, 0.0],
        }
    }
}
