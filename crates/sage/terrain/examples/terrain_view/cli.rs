use super::lighting::*;
use super::object_compare::*;
use super::reflection::*;
use super::shadow::*;
use super::water_scene::*;
use super::*;

pub(crate) struct Options {
    pub(crate) installs: Vec<PathBuf>,
    pub(crate) needle: String,
    pub(crate) screenshot: Option<PathBuf>,
    pub(crate) scale: f32,
    pub(crate) pitch: f32,
    pub(crate) yaw: f32,
    pub(crate) zoom: f32,
    pub(crate) blend: bool,
    pub(crate) cliffs: bool,
    pub(crate) frames: Option<u32>,
    pub(crate) sweep: Option<u32>,
    pub(crate) frustum_cull: bool,
    pub(crate) focus: Option<(u32, u32)>,
    pub(crate) inspect: Vec<(u32, u32)>,
    pub(crate) pick: Vec<(f32, f32)>,
    pub(crate) mips: bool,
    pub(crate) aniso: u16,
    pub(crate) alias: Option<u32>,
    pub(crate) time: Option<TimeOfDay>,
    pub(crate) map_light: bool,
    pub(crate) alt_set: bool,
    pub(crate) objects: bool,
    pub(crate) proxies: bool,
    pub(crate) roads: bool,
    pub(crate) road_options: RoadOptions,
    pub(crate) water: WaterDraw,
    pub(crate) reflection: WaterReflection,
    pub(crate) reflection_source: WaterReflectionSource,
    pub(crate) reflection_composite: WaterReflectionComposite,
    pub(crate) highlights: WaterHighlights,
    pub(crate) alpha_source: WaterAlphaSource,
    pub(crate) water_tint: WaterTint,
    pub(crate) water_depth: WaterDepthMode,
    pub(crate) distortion: WaterDistortion,
    pub(crate) water_coarse: bool,
    pub(crate) water_moving: bool,
    pub(crate) rivers: RiverDraw,
    pub(crate) river_look: RiverLook,
    pub(crate) wave_look: WaveLook,
    pub(crate) water_time: f32,
    pub(crate) object_animation: ObjectAnimation,
    pub(crate) object_material: ObjectMaterial,
    pub(crate) sway: SwayMode,
    pub(crate) particles: bool,
    pub(crate) particle_keys: bool,
    pub(crate) particle_bones: BoneRung,

    pub(crate) particle_sort: SortMode,

    pub(crate) particle_billboard: BillboardMode,

    pub(crate) particle_lod: DetailLevel,

    pub(crate) particle_flipbook: FlipbookMode,

    pub(crate) particle_shape: ShapeMode,

    pub(crate) particle_compound: bool,

    pub(crate) particle_budget: Option<Option<usize>>,

    pub(crate) particle_motion: bool,

    pub(crate) particle_wind: WindMode,

    pub(crate) fx_name: Option<String>,

    pub(crate) particle_name: Option<String>,
    pub(crate) particle_time: f32,
    pub(crate) standard_z: bool,
    pub(crate) tuning_path: Option<PathBuf>,
    pub(crate) terrain_overbright: TerrainOverbright,
    pub(crate) terrain_ceiling: TerrainCeiling,
    pub(crate) sky: SkyDraw,
    pub(crate) terrain_cloud: TerrainCloud,
    pub(crate) terrain_macro: TerrainMacro,
    pub(crate) terrain_relief: TerrainRelief,
    pub(crate) terrain_sun: TerrainSun,
    pub(crate) shadows: TerrainShadows,
    pub(crate) shadow_fit: ShadowFitMode,
    pub(crate) shadow_alpha: ShadowAlpha,
    pub(crate) reflection_cull: ReflectionCull,
}

pub(crate) fn tuned<T: Default>(
    tuning: &WaterTuning,
    key: &str,
    parse: fn(&str) -> Option<T>,
) -> T {
    tuning.mode(key).and_then(parse).unwrap_or_default()
}

pub(crate) fn river_scroll_factor(tuning: &WaterTuning) -> f32 {
    let unit = tuning.river_uv_scroll;
    if unit.is_finite() {
        unit / RIVER_SCROLL_UNIT
    } else {
        1.0
    }
}

pub(crate) fn finite_positive(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        1.0
    }
}

pub(crate) fn tuning_path_arg() -> Option<PathBuf> {
    let mut args = std::env::args_os();
    while let Some(arg) = args.next() {
        if arg.to_string_lossy() == "--tuning" {
            return args.next().map(PathBuf::from);
        }
    }
    None
}

pub(crate) fn parse_args(tuning: &WaterTuning) -> Result<Options, String> {
    pub(crate) const USAGE: &str = "usage: terrain_view <install dir> <map name or path> \
        [--install <dir>] [--screenshot <file>] [--scale <value>] [--pitch <radians>] \
        [--yaw <radians>] [--zoom <factor>] [--no-blend] [--no-cliff] [--frames <n>] \
        [--particles off|on] [--particle-keys off|on] [--particle <name>] \
        [--particle-time <seconds>] [--particle-bones off|default|all] \
        [--particle-motion off|on] [--particle-sort none|level|depth] \
        [--particle-billboard data|screen|ground] \
        [--particle-lod off|verylow|low|medium|high|ultrahigh] \
        [--particle-flipbook off|frame|life|blend] [--particle-compound on|off] \
        [--particle-shape data|sprite] \
        [--particle-budget <n>] [--particle-wind retail|off|doubled] [--fx <name>] \
        [--no-mips] [--aniso <n>] [--alias <n>] [--focus <x>,<y>] [--inspect <x>,<y>] \
        [--pick <px>,<py>] [--time <name>] [--placeholder-light] [--alt-light-set]         [--no-objects] [--proxies] [--no-roads] [--standard-z] [--road-lift <value>]         [--road-height cell|max|surface] [--water off|flat|low|default]         [--reflection off|fresnel|flat|mirror]         [--reflection-source environment|planar]         [--water-highlights off|specular|shore|crest|all]         [--water-alpha ini|map]         [--rivers off|flat|texture|shader]         [--river-stage surface|opacity|sparkle|all|normal]         [--river-flow channel|diagonal] [--water-time <seconds>]         [--water-tint flat|depth|ramp|ramp-tinted]         [--water-depth vertex|soft|absorb|show] [--water-coarse]         [--refraction off|on|unclamped] [--water-normal unit|slope]         [--waves off|on] [--wave-pca ignore|crest|displace]         [--object-animation off|hold|play] [--object-uv off|step[:ticks]|scroll] [--sway off|gentle|generals|strong] \
        [--wave-scale record|cells] [--tuning <file>]         [--terrain-overbright all|accents] [--terrain-ceiling clamped|none]         [--terrain-cloud off|texture|factor] [--terrain-macro off|on] [--terrain-relief off|on] [--terrain-sun authored|overbright]         [--sky off|gradient|full]         [--shadows off|on]         [--shadow-fit target|view|world]         [--shadow-alpha off|on]         [--reflection-cull off|on]         [--frustum-cull off|on] [--sweep <n>]";
    let mut args = std::env::args_os().skip(1);
    let (Some(install), Some(needle)) = (args.next(), args.next()) else {
        return Err(USAGE.to_owned());
    };
    let mut options = Options {
        installs: vec![PathBuf::from(install)],
        needle: needle.to_string_lossy().into_owned(),
        screenshot: None,
        scale: DEFAULT_VERTICAL_SCALE,
        pitch: 0.5,
        yaw: 0.6,
        zoom: 1.0,
        blend: true,
        cliffs: true,
        frames: None,
        sweep: None,
        frustum_cull: true,
        focus: None,
        inspect: Vec::new(),
        pick: Vec::new(),
        mips: true,
        aniso: mint_render::MAX_ANISOTROPY,
        alias: None,
        time: None,
        map_light: true,
        alt_set: false,
        objects: true,
        proxies: false,
        roads: true,
        road_options: RoadOptions::default(),
        water: tuned(tuning, "mode.water", WaterDraw::parse),
        reflection: tuned(tuning, "mode.reflection", WaterReflection::parse),
        reflection_source: tuned(
            tuning,
            "mode.reflection_source",
            WaterReflectionSource::parse,
        ),
        reflection_composite: tuned(
            tuning,
            "mode.reflection_composite",
            WaterReflectionComposite::parse,
        ),
        highlights: tuned(tuning, "mode.water_highlights", WaterHighlights::parse),
        alpha_source: tuned(tuning, "mode.water_alpha", WaterAlphaSource::parse),
        water_tint: tuned(tuning, "mode.water_tint", WaterTint::parse),
        water_depth: tuned(tuning, "mode.water_depth", WaterDepthMode::parse),
        distortion: WaterDistortion {
            refraction: tuned(tuning, "mode.refraction", WaterRefraction::parse),
            normal: tuned(tuning, "mode.water_normal", WaterNormal::parse),
        },
        water_coarse: false,
        water_moving: tuning.mode("mode.water_motion") != Some("freeze"),
        rivers: tuned(tuning, "mode.rivers", RiverDraw::parse),
        river_look: RiverLook {
            stage: tuned(tuning, "mode.river_stage", RiverStage::parse),
            flow: tuned(tuning, "mode.river_flow", RiverFlow::parse),
        },
        wave_look: WaveLook {
            draw: tuned(tuning, "mode.waves", WaveDraw::parse),
            pca: tuned(tuning, "mode.wave_pca", WavePca::parse),
            scale: tuned(tuning, "mode.wave_scale", WaveScale::parse),
        },
        water_time: 0.0,
        object_animation: ObjectAnimation::Play,
        object_material: ObjectMaterial::default(),
        sway: SwayMode::default(),
        particles: true,
        particle_keys: true,
        particle_bones: BoneRung::default(),
        particle_sort: SortMode::default(),
        particle_billboard: BillboardMode::default(),
        particle_lod: DetailLevel::default(),
        particle_flipbook: FlipbookMode::default(),
        particle_shape: ShapeMode::default(),
        particle_compound: true,
        particle_budget: None,
        particle_motion: true,
        particle_wind: WindMode::default(),
        fx_name: None,
        particle_name: None,
        particle_time: 1.0,
        standard_z: false,
        tuning_path: None,
        terrain_overbright: tuned(tuning, "mode.terrain_overbright", TerrainOverbright::parse),
        terrain_ceiling: tuned(tuning, "mode.terrain_ceiling", TerrainCeiling::parse),
        sky: tuned(tuning, "mode.sky", SkyDraw::parse),
        terrain_cloud: tuned(tuning, "mode.terrain_cloud", TerrainCloud::parse),
        terrain_macro: tuned(tuning, "mode.terrain_macro", TerrainMacro::parse),
        shadows: tuned(tuning, "mode.shadows", TerrainShadows::parse),
        shadow_fit: ShadowFitMode::default(),
        shadow_alpha: ShadowAlpha::default(),
        reflection_cull: ReflectionCull::default(),
        terrain_relief: TerrainRelief::default(),
        terrain_sun: TerrainSun::default(),
    };
    while let Some(arg) = args.next() {
        let arg = arg.to_string_lossy().into_owned();
        let mut value = || {
            args.next()
                .map(|v| v.to_string_lossy().into_owned())
                .ok_or_else(|| format!("{arg} needs a value"))
        };
        match arg.as_str() {
            "--install" => options.installs.push(PathBuf::from(value()?)),
            "--screenshot" => options.screenshot = Some(PathBuf::from(value()?)),
            "--scale" => {
                let text = value()?;
                options.scale = text
                    .parse()
                    .map_err(|_| format!("--scale is not a number: {text}"))?;
            }
            "--pitch" => {
                let text = value()?;
                options.pitch = text
                    .parse()
                    .map_err(|_| format!("--pitch is not a number: {text}"))?;
            }
            "--zoom" => {
                let text = value()?;
                let zoom: f32 = text
                    .parse()
                    .map_err(|_| format!("--zoom is not a number: {text}"))?;
                if !(zoom.is_finite() && zoom > 0.0) {
                    return Err(format!("--zoom must be a positive number: {text}"));
                }
                options.zoom = zoom;
            }
            "--no-blend" => options.blend = false,
            "--standard-z" => options.standard_z = true,
            "--yaw" => {
                let text = value()?;
                options.yaw = text
                    .parse()
                    .map_err(|_| format!("--yaw is not a number: {text}"))?;
            }
            "--no-cliff" => options.cliffs = false,
            "--focus" => {
                let text = value()?;
                let (x, y) = text
                    .split_once(',')
                    .ok_or_else(|| format!("--focus wants <x>,<y>: {text}"))?;
                let parse = |part: &str| -> Result<u32, String> {
                    part.trim()
                        .parse()
                        .map_err(|_| format!("--focus is not a cell coordinate: {text}"))
                };
                options.focus = Some((parse(x)?, parse(y)?));
            }
            "--inspect" => {
                let text = value()?;
                let (x, y) = text
                    .split_once(',')
                    .ok_or_else(|| format!("--inspect wants <x>,<y>: {text}"))?;
                let parse = |part: &str| -> Result<u32, String> {
                    part.trim()
                        .parse()
                        .map_err(|_| format!("--inspect is not a cell coordinate: {text}"))
                };
                options.inspect.push((parse(x)?, parse(y)?));
            }
            "--pick" => {
                let text = value()?;
                let (x, y) = text
                    .split_once(',')
                    .ok_or_else(|| format!("--pick wants <px>,<py>: {text}"))?;
                let parse = |part: &str| -> Result<f32, String> {
                    part.trim()
                        .parse()
                        .map_err(|_| format!("--pick is not a pixel coordinate: {text}"))
                };
                options.pick.push((parse(x)?, parse(y)?));
            }
            "--no-objects" => options.objects = false,
            "--proxies" => options.proxies = true,
            "--no-roads" => options.roads = false,
            "--river-stage" => {
                let text = value()?;
                options.river_look.stage = RiverStage::parse(&text).ok_or_else(|| {
                    format!("--river-stage wants surface|opacity|sparkle|all|normal: {text}")
                })?;
            }
            "--waves" => {
                let text = value()?;
                options.wave_look.draw = WaveDraw::parse(&text)
                    .ok_or_else(|| format!("--waves wants off|on: {text}"))?;
            }
            "--tuning" => {
                options.tuning_path = Some(PathBuf::from(value()?));
            }
            "--wave-pca" => {
                let text = value()?;
                options.wave_look.pca = WavePca::parse(&text)
                    .ok_or_else(|| format!("--wave-pca wants ignore|crest|displace: {text}"))?;
            }
            "--wave-scale" => {
                let text = value()?;
                options.wave_look.scale = WaveScale::parse(&text)
                    .ok_or_else(|| format!("--wave-scale wants record|cells: {text}"))?;
            }
            "--terrain-overbright" => {
                let text = value()?;
                options.terrain_overbright = TerrainOverbright::parse(&text)
                    .ok_or_else(|| format!("--terrain-overbright wants all|accents: {text}"))?;
            }
            "--terrain-ceiling" => {
                let text = value()?;
                options.terrain_ceiling = TerrainCeiling::parse(&text)
                    .ok_or_else(|| format!("--terrain-ceiling wants clamped|none: {text}"))?;
            }
            "--sky" => {
                let text = value()?;
                options.sky = SkyDraw::parse(&text)
                    .ok_or_else(|| format!("--sky wants off|gradient|box|full: {text}"))?;
            }
            "--shadows" => {
                let text = value()?;
                options.shadows = TerrainShadows::parse(&text)
                    .ok_or_else(|| format!("--shadows wants off|on: {text}"))?;
            }
            "--shadow-fit" => {
                let text = value()?;
                options.shadow_fit = ShadowFitMode::parse(&text)
                    .ok_or_else(|| format!("--shadow-fit wants target|view|world: {text}"))?;
            }
            "--shadow-alpha" => {
                let text = value()?;
                options.shadow_alpha = ShadowAlpha::parse(&text)
                    .ok_or_else(|| format!("--shadow-alpha wants off|on: {text}"))?;
            }
            "--reflection-cull" => {
                let text = value()?;
                options.reflection_cull = ReflectionCull::parse(&text)
                    .ok_or_else(|| format!("--reflection-cull wants off|on: {text}"))?;
            }
            "--terrain-cloud" => {
                let text = value()?;
                options.terrain_cloud = TerrainCloud::parse(&text)
                    .ok_or_else(|| format!("--terrain-cloud wants off|texture|factor: {text}"))?;
            }
            "--terrain-sun" => {
                let text = value()?;
                options.terrain_sun = TerrainSun::parse(&text)
                    .ok_or_else(|| format!("--terrain-sun wants authored|overbright: {text}"))?;
            }
            "--terrain-relief" => {
                let text = value()?;
                options.terrain_relief = TerrainRelief::parse(&text)
                    .ok_or_else(|| format!("--terrain-relief wants off|on: {text}"))?;
            }
            "--terrain-macro" => {
                let text = value()?;
                options.terrain_macro = TerrainMacro::parse(&text)
                    .ok_or_else(|| format!("--terrain-macro wants off|on: {text}"))?;
            }
            "--river-flow" => {
                let text = value()?;
                options.river_look.flow = RiverFlow::parse(&text)
                    .ok_or_else(|| format!("--river-flow wants channel|diagonal: {text}"))?;
            }
            "--rivers" => {
                let text = value()?;
                options.rivers = match text.trim().to_ascii_lowercase().as_str() {
                    "off" => RiverDraw::Off,
                    "flat" => RiverDraw::Flat,
                    "texture" => RiverDraw::Texture,
                    "shader" => RiverDraw::Shader,
                    other => {
                        return Err(format!("--rivers wants off|flat|texture|shader: {other}"))
                    }
                };
            }
            "--water" => {
                let text = value()?;
                options.water = match text.trim().to_ascii_lowercase().as_str() {
                    "off" => WaterDraw::Off,
                    "flat" => WaterDraw::Flat,
                    "low" => WaterDraw::Low,
                    "default" => WaterDraw::Default,
                    other => return Err(format!("--water wants off|flat|low|default: {other}")),
                };
            }
            "--reflection" => {
                let text = value()?;
                options.reflection = WaterReflection::parse(text.trim())
                    .ok_or_else(|| format!("--reflection wants off|fresnel|flat|mirror: {text}"))?;
            }
            "--reflection-source" => {
                let text = value()?;
                options.reflection_source =
                    WaterReflectionSource::parse(text.trim()).ok_or_else(|| {
                        format!("--reflection-source wants environment|planar: {text}")
                    })?;
            }
            "--water-highlights" => {
                let text = value()?;
                options.highlights = WaterHighlights::parse(text.trim()).ok_or_else(|| {
                    format!("--water-highlights wants off|specular|shore|crest|all: {text}")
                })?;
            }
            "--water-alpha" => {
                let text = value()?;
                options.alpha_source = WaterAlphaSource::parse(text.trim())
                    .ok_or_else(|| format!("--water-alpha wants ini|map: {text}"))?;
            }
            "--refraction" => {
                let text = value()?;
                options.distortion.refraction = WaterRefraction::parse(text.trim())
                    .ok_or_else(|| format!("--refraction wants off|on|unclamped: {text}"))?;
            }
            "--water-normal" => {
                let text = value()?;
                options.distortion.normal = WaterNormal::parse(text.trim())
                    .ok_or_else(|| format!("--water-normal wants unit|slope: {text}"))?;
            }
            "--water-coarse" => options.water_coarse = true,
            "--water-depth" => {
                let text = value()?;
                options.water_depth = WaterDepthMode::parse(text.trim()).ok_or_else(|| {
                    format!("--water-depth wants vertex|soft|absorb|show: {text}")
                })?;
            }
            "--water-tint" => {
                let text = value()?;
                options.water_tint = WaterTint::parse(text.trim()).ok_or_else(|| {
                    format!("--water-tint wants flat|depth|ramp|ramp-tinted: {text}")
                })?;
            }
            "--object-uv" => {
                let text = value()?;
                options.object_material = ObjectMaterial::parse(&text)
                    .ok_or_else(|| format!("--object-uv wants off|step[:ticks]|scroll: {text}"))?;
            }
            "--sway" => {
                let text = value()?;
                options.sway = SwayMode::parse(&text)
                    .ok_or_else(|| format!("--sway wants off|gentle|generals|strong: {text}"))?;
            }
            "--object-animation" => {
                let text = value()?;
                options.object_animation = ObjectAnimation::parse(&text)
                    .ok_or_else(|| format!("--object-animation wants off|hold|play: {text}"))?;
            }
            "--water-time" => {
                let text = value()?;
                let seconds: f32 = text
                    .parse()
                    .map_err(|_| format!("--water-time is not a number: {text}"))?;
                if !seconds.is_finite() {
                    return Err(format!("--water-time must be finite: {text}"));
                }
                options.water_time = seconds;
            }
            "--road-lift" => {
                let text = value()?;
                let lift: f32 = text
                    .parse()
                    .map_err(|_| format!("--road-lift is not a number: {text}"))?;
                if !lift.is_finite() {
                    return Err(format!("--road-lift must be finite: {text}"));
                }
                options.road_options.lift = lift;
            }
            "--road-height" => {
                let text = value()?;
                options.road_options.height = match text.trim().to_ascii_lowercase().as_str() {
                    "surface" => RoadHeight::Surface,
                    "max" => RoadHeight::StationMax,
                    "cell" => RoadHeight::CellMax,
                    other => {
                        return Err(format!("--road-height wants surface, max or cell: {other}"))
                    }
                };
            }
            "--tessellation" => {
                let text = value()?;
                options.road_options.tessellation = match text.trim().to_ascii_lowercase().as_str()
                {
                    "ribbon" => RoadTessellation::Ribbon,
                    "1" => RoadTessellation::Grid(StepsPerCell::One),
                    "2" => RoadTessellation::Grid(StepsPerCell::Two),
                    "4" => RoadTessellation::Grid(StepsPerCell::Four),
                    other => {
                        return Err(format!("--tessellation wants ribbon, 1, 2 or 4: {other}"))
                    }
                };
            }
            "--chain-order" => {
                let text = value()?;
                options.road_options.chain_order = match text.trim().to_ascii_lowercase().as_str() {
                    "file" => RoadChainOrder::AsFile,
                    "chained" => RoadChainOrder::Chained,
                    "reversed" => RoadChainOrder::Reversed,
                    other => {
                        return Err(format!(
                            "--chain-order wants file, chained or reversed: {other}"
                        ))
                    }
                };
            }
            "--placeholder-light" => options.map_light = false,
            "--alt-light-set" => options.alt_set = true,
            "--time" => {
                let text = value()?;
                options.time = Some(time_of_day_by_name(&text).ok_or_else(|| {
                    format!("--time wants morning, afternoon, evening or night: {text}")
                })?);
            }
            "--no-mips" => options.mips = false,
            "--aniso" => {
                let text = value()?;
                options.aniso = text
                    .parse()
                    .map_err(|_| format!("--aniso is not a number: {text}"))?;
            }
            "--alias" => {
                let text = value()?;
                options.alias = Some(
                    text.parse()
                        .map_err(|_| format!("--alias is not a number: {text}"))?,
                );
            }
            "--frustum-cull" => {
                let text = value()?;
                options.frustum_cull = match text.as_str() {
                    "on" => true,
                    "off" => false,
                    other => return Err(format!("--frustum-cull wants off|on: {other}")),
                };
            }
            "--frames" => {
                let text = value()?;
                options.frames = Some(
                    text.parse()
                        .map_err(|_| format!("--frames is not a number: {text}"))?,
                );
            }
            "--sweep" => {
                let text = value()?;
                options.sweep = Some(
                    text.parse()
                        .map_err(|_| format!("--sweep is not a number: {text}"))?,
                );
            }
            "--particles" => {
                let text = value()?;
                options.particles = match text.as_str() {
                    "on" => true,
                    "off" => false,
                    other => return Err(format!("--particles wants off|on: {other}")),
                };
            }
            "--particle-keys" => {
                let text = value()?;
                options.particle_keys = match text.as_str() {
                    "on" => true,
                    "off" => false,
                    other => return Err(format!("--particle-keys wants off|on: {other}")),
                };
            }
            "--particle-bones" => {
                let text = value()?;
                options.particle_bones = BoneRung::parse(&text)
                    .ok_or_else(|| format!("--particle-bones wants off|default|all: {text}"))?;
            }
            "--particle-sort" => {
                let text = value()?;
                options.particle_sort = SortMode::parse(&text)
                    .ok_or_else(|| format!("--particle-sort wants none|level|depth: {text}"))?;
            }
            "--particle-billboard" => {
                let text = value()?;
                options.particle_billboard = BillboardMode::parse(&text).ok_or_else(|| {
                    format!("--particle-billboard wants data|screen|ground: {text}")
                })?;
            }
            "--particle-flipbook" => {
                let text = value()?;
                options.particle_flipbook = FlipbookMode::parse(&text).ok_or_else(|| {
                    format!("--particle-flipbook wants off|frame|life|blend: {text}")
                })?;
            }
            "--particle-shape" => {
                let text = value()?;
                options.particle_shape = ShapeMode::parse(&text)
                    .ok_or_else(|| format!("--particle-shape wants data|sprite: {text}"))?;
            }
            "--particle-compound" => {
                let text = value()?;
                options.particle_compound = match text.as_str() {
                    "on" => true,
                    "off" => false,
                    other => return Err(format!("--particle-compound wants on|off: {other}")),
                };
            }
            "--particle-lod" => {
                let text = value()?;
                options.particle_lod = DetailLevel::parse(&text).ok_or_else(|| {
                    format!("--particle-lod wants off|verylow|low|medium|high|ultrahigh: {text}")
                })?;
            }
            "--particle-budget" => {
                let text = value()?;
                let count: usize = text
                    .parse()
                    .map_err(|_| format!("--particle-budget is not a whole number: {text}"))?;
                options.particle_budget = Some((count > 0).then_some(count));
            }
            "--particle-motion" => {
                let text = value()?;
                options.particle_motion = match text.as_str() {
                    "off" => false,
                    "on" => true,
                    other => return Err(format!("--particle-motion wants off|on: {other}")),
                };
            }
            "--particle" => options.particle_name = Some(value()?),
            "--particle-wind" => {
                let text = value()?;
                options.particle_wind = WindMode::parse(&text)
                    .ok_or_else(|| format!("--particle-wind wants retail|off|doubled: {text}"))?;
            }
            "--fx" => options.fx_name = Some(value()?),
            "--particle-time" => {
                let text = value()?;
                options.particle_time = text
                    .parse()
                    .map_err(|_| format!("--particle-time is not a number: {text}"))?;
            }
            other => return Err(format!("unknown option {other}\n\n{USAGE}")),
        }
    }
    Ok(options)
}

pub(crate) fn time_of_day_by_name(text: &str) -> Option<TimeOfDay> {
    TimeOfDay::ALL
        .into_iter()
        .find(|time| time.name().eq_ignore_ascii_case(text.trim()))
}
