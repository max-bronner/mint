use super::reflection::*;
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum WaterDraw {
    Off,
    Flat,
    Low,
    #[default]
    Default,
}

impl WaterDraw {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Flat => "flat",
            Self::Low => "low",
            Self::Default => "default",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "flat" => Some(Self::Flat),
            "low" => Some(Self::Low),
            "default" => Some(Self::Default),
            _ => None,
        }
    }

    pub(crate) fn on(self) -> bool {
        self != Self::Off
    }

    pub(crate) fn technique(self) -> Option<WaterTechnique> {
        match self {
            Self::Off | Self::Flat => None,
            Self::Low => Some(WaterTechnique::Low),
            Self::Default => Some(WaterTechnique::Default),
        }
    }

    pub(crate) fn animates(self) -> bool {
        self.technique().is_some()
    }
}

pub(crate) struct WaterScene {
    pub(crate) handles: Vec<MeshHandle>,
    pub(crate) sky: Vec<Option<TextureHandle>>,
    pub(crate) bump: Vec<Option<TextureHandle>>,
    pub(crate) wave: Vec<Option<TextureHandle>>,
    pub(crate) sky_source: Vec<EnvSource>,
    pub(crate) params: Vec<WaterShaderParams>,
    pub(crate) shader: Option<CustomShaderHandle>,
    pub(crate) materials: Vec<Option<[CustomMaterialHandle; 2]>>,
    pub(crate) failures: Vec<String>,
    pub(crate) shading: Vec<BodyShading>,
    pub(crate) reflects: Vec<bool>,
    pub(crate) depth: Vec<WaterDepth>,
    pub(crate) planes: Vec<(f32, mint_render::RenderTargetHandle)>,
    pub(crate) plane_of: Vec<Option<usize>>,
}

impl WaterScene {
    pub(crate) fn material(&self, slot: usize, mode: WaterDraw) -> Option<CustomMaterialHandle> {
        let technique = mode.technique()?;
        let pair = self.materials.get(slot).copied().flatten()?;
        let index = match technique {
            WaterTechnique::Low => 0,
            WaterTechnique::Default => 1,
        };
        pair.get(index).copied()
    }

    pub(crate) fn reflection(&self, slot: usize, mode: WaterReflection) -> WaterReflection {
        if self.reflects.get(slot).copied().unwrap_or(false) {
            mode
        } else {
            WaterReflection::Off
        }
    }

    pub(crate) fn source(&self, slot: usize, mode: WaterReflectionSource) -> WaterReflectionSource {
        match self.plane_index(slot) {
            Some(_) => mode,
            None => WaterReflectionSource::Environment,
        }
    }

    pub(crate) fn plane_index(&self, slot: usize) -> Option<usize> {
        self.plane_of.get(slot).copied().flatten()
    }

    pub(crate) fn depth(
        &self,
        slot: usize,
        mode: WaterDepthMode,
        tuning: &WaterTuning,
    ) -> WaterDepth {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnvSource {
    ShaderMaterial,
    MapRecord,
    None,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct WaterMaterial {
    pub(crate) params: Option<WaterShaderParams>,
    pub(crate) env_texture: Option<String>,
    pub(crate) bump_texture: Option<String>,
    pub(crate) wave_texture: Option<String>,
    pub(crate) diffuse: Option<[f32; 4]>,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn upload_water(
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
    depth_mode: WaterDepthMode,
    distortion: WaterDistortion,
    tuning: &WaterTuning,
    waves: &WaveSurface,
) -> mint_render::Result<WaterScene> {
    let shader = match renderer.register_custom_shader(&CustomShader {
        label: "bfme water (DefaultTechnique / DefaultTechnique_L)",
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
        let body_source = scene.source(slot, reflection_source);

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
            let mut pair = Vec::with_capacity(2);
            for technique in [WaterTechnique::Low, WaterTechnique::Default] {
                let block = WaterUniforms::new(
                    &params,
                    technique,
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
                    Ok(handle) => pair.push(handle),
                    Err(error) => {
                        scene
                            .failures
                            .push(format!("{}: water material: {error}", body.name));
                        return None;
                    }
                }
            }
            match (pair.first(), pair.get(1)) {
                (Some(low), Some(default)) => Some([*low, *default]),
                _ => None,
            }
        });
        scene.materials.push(built);
    }
    for line in &scene.failures {
        println!("  WATER TEXTURE FAILED {line}");
    }
    if !surface.bodies.is_empty() {
        let count = |want: EnvSource| {
            scene
                .sky_source
                .iter()
                .filter(|source| **source == want)
                .count()
        };
        println!(
            "  environment map: {} of {} from the fxShader's WaterEnvTexture, \
             {} from the record's skyTexture, {} none (research 6d)",
            count(EnvSource::ShaderMaterial),
            surface.bodies.len(),
            count(EnvSource::MapRecord),
            count(EnvSource::None),
        );
        if scene.shader.is_none() {
            println!("  the water shader did not register: only --water flat is available");
        }
        let with_material = scene.materials.iter().filter(|m| m.is_some()).count();
        let with_foam = scene.wave.iter().filter(|w| w.is_some()).count();
        println!(
            "  shader: {with_material} of {} bodies carry a registered water material,              {with_foam} a WaveTexture (water-shader.md sections 3, 4, 6c)",
            surface.bodies.len(),
        );
        for (body_slot, (body, params)) in
            surface.bodies.iter().zip(scene.params.iter()).enumerate()
        {
            let bump = (params.wind_speed + params.bump_speed) * WATER_UV_BASE;
            println!(
                "    #{:<4} {:<20} WindAngle {:>5.2} +/- WaveAngle/2 {:>5.3} rad,                  bump {:>6.3} + {:>5.3} = {:>6.2} world units/s, BumpUVScale {:?} x {:.1},                  EnvUVScale {:?}, ReflectionStrength {:.2}, ShadowLightness {}{}",
                body.unique_id,
                body.name,
                params.wind_angle,
                params.wave_angle * 0.5,
                params.wind_speed,
                params.bump_speed,
                bump,
                params.bump_uv_scale,
                params.bump_scale,
                params.env_uv_scale,
                params.reflection_strength,
                params.shadow_lightness.map_or_else(
                    || format!("{:.2} (engine.shadow_lightness)", tuning.shadow_lightness),
                    |value| format!("{value:.2}"),
                ),
                if scene.reflects.get(body_slot).copied().unwrap_or(false) {
                    ""
                } else {
                    " (no WaterEnvTexture - reflection off on this body)"
                },
            );
            println!(
                "          planar target {} (z {}), body z {}",
                match scene.plane_index(body_slot) {
                    Some(index) => format!("#{index}"),
                    None => "none".to_owned(),
                },
                match scene
                    .plane_index(body_slot)
                    .and_then(|index| scene.planes.get(index))
                {
                    Some((height, _)) => format!("{height}"),
                    None => "-".to_owned(),
                },
                body.height,
            );
        }
        let reflecting = scene.reflects.iter().filter(|on| **on).count();
        let muted = scene
            .params
            .iter()
            .filter(|params| params.reflection_strength == 0.0)
            .count();
        println!(
            "  reflection = {} (max(Schlick F0 {WATER_FRESNEL_F0}, engine.water_reflection_floor)              x ReflectionStrength);              {reflecting} of {} bodies carry a WaterEnvTexture to reflect,              {muted} author ReflectionStrength 0 (mode.reflection)",
            reflection.label(),
            surface.bodies.len(),
        );
        let from_material = scene
            .shading
            .iter()
            .filter(|shading| shading.from_material)
            .count();
        let with_ramp = scene
            .shading
            .iter()
            .filter(|shading| shading.ramp.is_some())
            .count();
        println!(
            "  colour: {from_material} of {} from the fxShader's MaterialColorDiffuse,              the rest from WATER_TINT; {with_ramp} depthColors ramp(s) loaded",
            surface.bodies.len(),
        );
        println!(
            "  depth over the water vertices: {:.1} to {:.1} world units, {} of {} inside the              {:.1}-unit fade band (vertical scale {vertical_scale})",
            depth_range.min,
            depth_range.max,
            depth_range.faded,
            depth_range.total,
            transparency.transparent_depth(),
        );
        println!("  water tint = {} (mode.water_tint)", tint.label());
        let fitted = scene
            .depth
            .iter()
            .filter(|terms| terms.extinction.iter().any(|sigma| *sigma > 0.0))
            .count();
        println!(
            "  absorption = {} (mode.water_depth); {fitted} of {} bodies carry a fitted             ramp, the rest fall back to the soft rung (design section 15)",
            depth_mode.label(),
            surface.bodies.len(),
        );
        let mut reported: BTreeMap<String, ()> = BTreeMap::new();
        for (shading, terms) in scene.shading.iter().zip(scene.depth.iter()) {
            let key = if shading.ramp_name.is_empty() {
                format!("MaterialColorDiffuse {:?}", shading.color)
            } else {
                shading.ramp_name.clone()
            };
            if reported.insert(key.clone(), ()).is_some() {
                continue;
            }
            let Some((fit, source)) = body_absorption(shading, transparency) else {
                continue;
            };
            let worst = fit.rms.iter().fold(0.0f32, |a, b| a.max(*b));
            println!(
                "    {:<28} sigma {:.4} {:.4} {:.4} /unit over {:.1} units, deep tint {:.3} {:.3}                  {:.3}, {}",
                key,
                terms.extinction[0],
                terms.extinction[1],
                terms.extinction[2],
                terms.fade_depth,
                fit.body[0],
                fit.body[1],
                fit.body[2],
                source.label(),
            );
            if source == RampSource::Fitted {
                println!(
                    "      exponential fit RMS {:.4} {:.4} {:.4} - {}",
                    fit.rms[0],
                    fit.rms[1],
                    fit.rms[2],
                    if worst <= RAMP_FIT_TOLERANCE {
                        "within 2/255, so the exponential form is used (design W14)"
                    } else {
                        "OVER 2/255: the design would select the LUT path, which is not built"
                    },
                );
            }
        }
        for (body, shading) in surface.bodies.iter().zip(scene.shading.iter()) {
            let [r, g, b, a] = shading.color;
            println!(
                "    #{:<4} {:<20} colour {r:.3} {g:.3} {b:.3} a {a:.3}  {}  ramp {}",
                body.unique_id,
                body.name,
                if shading.from_material {
                    "MaterialColorDiffuse"
                } else {
                    "WATER_TINT (no fxShader)"
                },
                match (&shading.ramp, shading.ramp_name.is_empty()) {
                    (Some(ramp), _) => format!("{} ({} texel(s))", shading.ramp_name, ramp.len()),
                    (None, true) => "(none named)".to_string(),
                    (None, false) => format!("{} (did not load)", shading.ramp_name),
                }
            );
        }
    }
    Ok(scene)
}

pub(crate) fn water_material(vfs: &Vfs, resolver: &Resolver, name: &str) -> WaterMaterial {
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
            mint_sage_w3d::ShaderMaterialValue::Vec4(value)
                if value.iter().all(|v| v.is_finite()) =>
            {
                let pair = [value[0], value[1]];
                match property.name.as_str() {
                    n if n.eq_ignore_ascii_case("BumpUVScale") => {
                        params.bump_uv_scale = pair;
                        seen = true;
                    }
                    n if n.eq_ignore_ascii_case("WaveUVScale") => {
                        params.wave_uv_scale = pair;
                        seen = true;
                    }
                    n if n.eq_ignore_ascii_case("EnvUVScale") => {
                        params.env_uv_scale = pair;
                        seen = true;
                    }
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

pub(crate) fn water_instances(
    scene: &WaterScene,
    surface: &WaterSurface,
    mode: WaterDraw,
    tint_mode: WaterTint,
) -> Vec<Instance> {
    if !mode.on() {
        return Vec::new();
    }
    scene
        .handles
        .iter()
        .enumerate()
        .map(|(slot, handle)| {
            let body = surface.bodies.get(slot);
            let additive = body.is_some_and(|body| body.additive_blend);
            let custom_material = scene.material(slot, mode);
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum RiverDraw {
    Off,
    Flat,
    Texture,
    #[default]
    Shader,
}

impl RiverDraw {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Flat => "flat",
            Self::Texture => "texture",
            Self::Shader => "shader",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "flat" => Some(Self::Flat),
            "texture" => Some(Self::Texture),
            "shader" => Some(Self::Shader),
            _ => None,
        }
    }

    pub(crate) fn on(self) -> bool {
        self != Self::Off
    }

    pub(crate) fn shaded(self) -> bool {
        self == Self::Shader
    }
}

pub(crate) struct RiverScene {
    pub(crate) handles: Vec<MeshHandle>,
    pub(crate) texture: Vec<Option<TextureHandle>>,
    pub(crate) opacity: Vec<Option<TextureHandle>>,
    pub(crate) noise: Vec<Option<TextureHandle>>,
    pub(crate) sparkle: Vec<Option<TextureHandle>>,
    pub(crate) ripple: Option<TextureHandle>,
    pub(crate) shader: Option<CustomShaderHandle>,
    pub(crate) materials: Vec<Option<CustomMaterialHandle>>,
    pub(crate) ripples: Vec<bool>,
    pub(crate) mirrors: Vec<Option<mint_render::RenderTargetHandle>>,
    pub(crate) refracts: Vec<bool>,
    pub(crate) failures: Vec<String>,
}

pub(crate) const DEFAULT_RIVER_RIPPLE: &str = "WaterRippleBump.tga";

pub(crate) fn is_lava(ribbon: &RiverRibbon) -> bool {
    ribbon.river_texture.to_ascii_lowercase().contains("lava")
}

pub(crate) fn over_standing_water(ribbon: &RiverRibbon, surface: &WaterSurface) -> bool {
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn upload_rivers(
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

pub(crate) fn river_instances(
    scene: &RiverScene,
    network: &RiverNetwork,
    mode: RiverDraw,
    seconds: f32,
    scroll: f32,
) -> Vec<Instance> {
    if !mode.on() {
        return Vec::new();
    }
    scene
        .handles
        .iter()
        .enumerate()
        .map(|(slot, handle)| {
            let ribbon = network.ribbons.get(slot);
            let additive = ribbon.is_some_and(|ribbon| ribbon.additive_blend);
            let speed = ribbon.map_or(0.0, |ribbon| ribbon.uv_scroll_speed);
            let material = if mode.shaded() {
                scene.materials.get(slot).copied().flatten()
            } else {
                None
            };
            let mut tint = ribbon.map_or([1.0, 1.0, 1.0, 1.0], |ribbon| ribbon.tint);
            if additive && material.is_none() {
                let alpha = tint[3];
                for channel in tint.iter_mut().take(3) {
                    *channel *= alpha;
                }
            }
            Instance {
                cull: false,
                uv_offset: river_scroll_offset(speed * scroll, seconds),
                texture: match mode {
                    RiverDraw::Texture => scene.texture.get(slot).copied().flatten(),
                    _ => None,
                },
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

pub(crate) fn push_river_uniforms(
    renderer: &mut Renderer,
    scene: &RiverScene,
    network: &RiverNetwork,
    look: RiverLook,
    refraction: WaterRefraction,
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
            scene.ripples.get(slot).copied().unwrap_or(false),
            if scene.refracts.get(slot).copied().unwrap_or(false) {
                refraction
            } else {
                WaterRefraction::Off
            },
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

pub(crate) fn report_river_shading(
    scene: &RiverScene,
    network: &RiverNetwork,
    mode: RiverDraw,
    look: RiverLook,
) {
    if network.ribbons.is_empty() {
        return;
    }
    if scene.shader.is_none() {
        println!(
            "  RIVER SHADER UNAVAILABLE - `{}` degrades to `texture`",
            mode.label()
        );
        return;
    }
    let count = |slots: &[Option<TextureHandle>]| slots.iter().filter(|h| h.is_some()).count();
    let total = network.ribbons.len();
    println!(
        "  river textures: {} of {total} riverTexture, {} alphaEdgeTexture,          {} noiseTexture, {} sparkleTexture (water-shader.md 7)",
        count(&scene.texture),
        count(&scene.opacity),
        count(&scene.noise),
        count(&scene.sparkle),
    );
    let rippling = scene.ripples.iter().filter(|on| **on).count();
    let refracting = scene.refracts.iter().filter(|on| **on).count();
    let mirroring = scene.mirrors.iter().filter(|on| on.is_some()).count();
    println!(
        "  {mirroring} of {total} ribbon(s) share a standing body's planar reflection          target (W12; the rest are lava, or run over a body already drawing it)"
    );
    println!(
        "  ripple normal map = {} (ours - no river record names one);          {rippling} of {total} ribbon(s) ripple, {} lava,          {refracting} may refract (the rest are additive or over standing water)",
        if scene.ripple.is_some() {
            "loaded"
        } else {
            "NONE - the ours-terms are off"
        },
        network.ribbons.iter().filter(|r| is_lava(r)).count(),
    );
    let speeds: Vec<f32> = network
        .ribbons
        .iter()
        .map(|ribbon| ribbon.uv_scroll_speed * RIVER_SCROLL_UNIT)
        .collect();
    let fastest = speeds.iter().copied().fold(0.0f32, f32::max);
    println!(
        "  river stage = {} (mode.river_stage), flow = {} (mode.river_flow);          scroll {:.2} world units/s at most ({RIVER_SCROLL_UNIT} x uvScrollSpeed,          water-shader.md 7b), ripple repeat {RIVER_RIPPLE_REPEAT} world units,          slope x{RIVER_RIPPLE_SLOPE}, glint x{RIVER_SPECULAR_GAIN},          sparkle x{RIVER_SPARKLE_GAIN}, edge band {RIVER_EDGE_BAND} world units",
        look.stage.label(),
        look.flow.label(),
        fastest,
    );
}

pub(crate) struct RiverInput<'a> {
    pub(crate) network: &'a RiverNetwork,
    pub(crate) mode: RiverDraw,
    pub(crate) look: RiverLook,
    pub(crate) seconds: f32,
}

pub(crate) struct WaveScene {
    pub(crate) handles: Vec<MeshHandle>,
    pub(crate) texture: Vec<Option<TextureHandle>>,
    pub(crate) shader: Option<CustomShaderHandle>,
    pub(crate) materials: Vec<Option<CustomMaterialHandle>>,
    pub(crate) failures: Vec<String>,
}

pub(crate) fn upload_waves(
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

pub(crate) fn wave_instances(
    scene: &WaveScene,
    field: &WaveField,
    look: WaveLook,
) -> Vec<Instance> {
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

pub(crate) fn push_wave_uniforms(
    renderer: &mut Renderer,
    scene: &WaveScene,
    field: &WaveField,
    look: WaveLook,
    tuning: &WaterTuning,
    seconds: f32,
) {
    for (slot, handle) in scene.materials.iter().enumerate() {
        let (Some(handle), Some(area)) = (handle, field.areas.get(slot)) else {
            continue;
        };
        let block = WaveUniforms::new(&area.params, look, tuning, seconds).to_bytes();
        if let Err(error) = renderer.update_custom_material(*handle, &block) {
            eprintln!("  could not update a wave material: {error}");
        }
    }
}
