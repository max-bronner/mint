use core::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Section {
    Map,
    Engine,
    Mode,
    Text,
}

impl Section {
    #[must_use]
    pub fn title(self) -> &'static str {
        match self {
            Self::Map => "1. FACTORS ON WHAT THE MAP AUTHORED",
            Self::Engine => "2. ENGINE CONSTANTS",
            Self::Mode => "3. MODES",
            Self::Text => "4. NAMES FROM THE GAME DATA",
        }
    }

    #[must_use]
    pub fn blurb(self) -> &'static [&'static str] {
        match self {
            Self::Map => &[
                "",
                "Each is a factor applied to a number the map stores. If something",
                "moves at the wrong speed or is the wrong size, it is one of these.",
            ],
            Self::Engine => &[
                "",
                "Constants of mint's own. Nothing in the game files supplies them.",
            ],
            Self::Mode => &[
                "",
                "These take a name, not a number. The accepted names are listed",
                "above each one.",
            ],
            Self::Text => &[
                "",
                "These take a name out of the shipped INI. An empty value means",
                "nothing is selected, which is the default.",
            ],
        }
    }
}

pub struct TuningEntry {
    pub key: &'static str,
    pub section: Section,
    pub get: fn(&WaterTuning) -> f32,
    pub set: fn(&mut WaterTuning, f32),
    pub doc: &'static [&'static str],
}

pub struct ModeEntry {
    pub key: &'static str,
    pub values: &'static [&'static str],
    pub get: fn(&WaterTuning) -> u8,
    pub set: fn(&mut WaterTuning, u8),
    pub doc: &'static str,
    pub heading: Option<&'static [&'static str]>,
}

pub const MAX_TUNING_NAME: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TuningName {
    bytes: [u8; MAX_TUNING_NAME],
    len: u8,
}

impl Default for TuningName {
    fn default() -> Self {
        Self::EMPTY
    }
}

impl TuningName {
    pub const EMPTY: Self = Self {
        bytes: [0; MAX_TUNING_NAME],
        len: 0,
    };

    #[must_use]
    pub fn as_str(&self) -> &str {
        let len = usize::from(self.len).min(MAX_TUNING_NAME);
        self.bytes
            .get(..len)
            .and_then(|bytes| core::str::from_utf8(bytes).ok())
            .unwrap_or("")
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[must_use]
    pub fn new(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.len() > MAX_TUNING_NAME {
            return None;
        }
        if text.bytes().any(|byte| !byte.is_ascii_graphic()) {
            return None;
        }
        let mut name = Self::EMPTY;
        for (slot, byte) in text.bytes().enumerate() {
            if let Some(held) = name.bytes.get_mut(slot) {
                *held = byte;
            }
        }
        name.len = u8::try_from(text.len()).unwrap_or(0);
        Some(name)
    }
}

pub struct TextEntry {
    pub key: &'static str,
    pub get: fn(&WaterTuning) -> TuningName,
    pub set: fn(&mut WaterTuning, TuningName),
    pub doc: &'static [&'static str],
}

macro_rules! entry {
    ($key:literal, $section:ident, $field:ident, $doc:expr) => {
        TuningEntry {
            key: $key,
            section: Section::$section,
            get: |t| t.$field,
            set: |t, v| t.$field = v,
            doc: $doc,
        }
    };
}

macro_rules! mode {
    ($key:literal, $field:ident, $values:expr, $doc:literal) => {
        mode!($key, $field, $values, $doc, None)
    };
    ($key:literal, $field:ident, $values:expr, $doc:literal, $heading:expr) => {
        ModeEntry {
            key: $key,
            values: $values,
            get: |t| t.$field,
            set: |t, v| t.$field = v,
            doc: $doc,
            heading: $heading,
        }
    };
}

pub static ENTRIES: &[TuningEntry] = &[
    entry!(
        "map.river_uv_scroll",
        Map,
        river_uv_scroll,
        &["Speed of the river flow, scaling the map's own uvScrollSpeed."]
    ),
    entry!(
        "map.wave_unit",
        Map,
        wave_unit,
        &["World units per unit of every size in a wave record."]
    ),
    entry!(
        "map.wave_velocity",
        Map,
        wave_velocity,
        &["World units a second per unit of a wave's initialVelocity."]
    ),
    entry!(
        "map.wave_final_width",
        Map,
        wave_final_width,
        &["Factor on finalWidth - a wave's extent along the shore."]
    ),
    entry!(
        "map.wave_final_height",
        Map,
        wave_final_height,
        &["Factor on finalHeight - a wave's extent across the shore."]
    ),
    entry!(
        "map.wave_distance_from_shore",
        Map,
        wave_distance_from_shore,
        &["Factor on distanceFromShore - how far out a wave is born."]
    ),
    entry!(
        "map.wave_time",
        Map,
        wave_time,
        &["Factor on all three wave timings. Bigger = surf breaks less often."]
    ),
    entry!(
        "map.env_max_alpha_depth",
        Map,
        env_max_alpha_depth,
        &["Factor on the map's waterMaxAlphaDepth - clear to full tint."]
    ),
    entry!(
        "map.env_deep_water_alpha",
        Map,
        env_deep_water_alpha,
        &["Factor on the map's deepWaterAlpha - how opaque deep water gets."]
    ),
    entry!(
        "map.water_shore_fade",
        Map,
        water_shore_fade,
        &["Factor on water.ini's TransparentWaterDepth, the shore fade."]
    ),
    entry!(
        "map.water_surface_drift",
        Map,
        water_surface_drift,
        &["Factor on the standing water's surface drift. 0 holds it still."]
    ),
    entry!(
        "map.water_tessellation_edge",
        Map,
        water_tessellation_edge,
        &[
            "World-unit edge the standing-water mesh is subdivided toward.",
            "APPLIED AT MAP LOAD ONLY - restart to change it.",
        ]
    ),
    entry!(
        "map.water_vertex_budget",
        Map,
        water_vertex_budget,
        &[
            "Most vertices one standing-water body may reach.",
            "APPLIED AT MAP LOAD ONLY - restart to change it.",
        ]
    ),
    entry!(
        "engine.wave_surface_amplitude",
        Engine,
        wave_surface_amplitude,
        &["How tall a PCA swell lifts the water, as a fraction of finalHeight."]
    ),
    entry!(
        "engine.wave_surface_steepness",
        Engine,
        wave_surface_steepness,
        &["Gerstner steepness: 0 is a plain bump, near 1 sharpens the crest."]
    ),
    entry!(
        "engine.wave_surface_reach",
        Engine,
        wave_surface_reach,
        &["How wide the swell is, in multiples of finalHeight."]
    ),
    entry!(
        "engine.wave_surface_foam",
        Engine,
        wave_surface_foam,
        &["Gain on crest foam taken from where the swell's surface pinches."]
    ),
    entry!(
        "engine.wave_foam_amplitude",
        Engine,
        wave_foam_amplitude,
        &["Gain on crest foam taken from the swell's height."]
    ),
    entry!(
        "engine.wave_foam_response",
        Engine,
        wave_foam_response,
        &["How sharply that foam falls off the crest. Higher is tighter."]
    ),
    entry!(
        "engine.water_fresnel_f0",
        Engine,
        water_fresnel_f0,
        &["Reflectance looking straight down. Raise it for more mirror."]
    ),
    entry!(
        "engine.water_reflection_floor",
        Engine,
        water_reflection_floor,
        &["Smallest Fresnel weight the reflection may fall to. 0 disables it."]
    ),
    entry!(
        "engine.water_refraction_scale",
        Engine,
        water_refraction_scale,
        &["How far the water bends what is behind it."]
    ),
    entry!(
        "engine.water_refraction_falloff",
        Engine,
        water_refraction_falloff,
        &["World units beyond which the refraction fades out."]
    ),
    entry!(
        "engine.water_bump_slope",
        Engine,
        water_bump_slope,
        &["How steeply the whole ripple tilts the surface. The roughness knob."]
    ),
    entry!(
        "engine.water_slope_normalize",
        Engine,
        water_slope_normalize,
        &["How much of the ripple's height is discarded, keeping its tilt."]
    ),
    entry!(
        "engine.water_drift_speed_cap",
        Engine,
        water_drift_speed_cap,
        &["Fastest the surface pattern may travel, in world units a second."]
    ),
    entry!(
        "engine.water_unit_slope",
        Engine,
        water_unit_slope,
        &["Fixed slope the unit normal rung uses. Leave at 0.05."]
    ),
    entry!(
        "engine.water_planar_perturbation",
        Engine,
        water_planar_perturbation,
        &["How far the ripple wobbles the reflection, in screen fractions."]
    ),
    entry!(
        "engine.water_reflection_blur",
        Engine,
        water_reflection_blur,
        &["How much the reflection blurs with distance. 0 is a sharp mirror."]
    ),
    entry!(
        "engine.water_ripple_detail",
        Engine,
        water_ripple_detail,
        &["Gain on the two coarse ripple octaves mint adds."]
    ),
    entry!(
        "engine.water_ripple_repeat",
        Engine,
        water_ripple_repeat,
        &["World units one repeat of the coarser added ripple octave covers."]
    ),
    entry!(
        "engine.water_ripple_lod_bias",
        Engine,
        water_ripple_lod_bias,
        &["Mip bias on the material's bump layers. Negative is sharper."]
    ),
    entry!(
        "engine.water_ripple_light",
        Engine,
        water_ripple_light,
        &["How far the ripple's light and shade swing the water's brightness."]
    ),
    entry!(
        "engine.water_still_orbit",
        Engine,
        water_still_orbit,
        &["On mode.water_drift = still, how far each layer wanders, in repeats."]
    ),
    entry!(
        "engine.water_drift_rate_cap",
        Engine,
        water_drift_rate_cap,
        &["Fastest a layer's pattern may travel, in texture repeats a second."]
    ),
    entry!(
        "engine.water_specular_gain",
        Engine,
        water_specular_gain,
        &["Gain on the sun glint. Too big and it becomes a white sheet."]
    ),
    entry!(
        "engine.water_foam_band",
        Engine,
        water_foam_band,
        &["World units of water column over which shore foam fades out."]
    ),
    entry!(
        "engine.water_foam_gain",
        Engine,
        water_foam_gain,
        &["Brightness of shore and crest foam alike."]
    ),
    entry!(
        "engine.water_crest_threshold",
        Engine,
        water_crest_threshold,
        &["How steep the ripple must be before crest foam appears."]
    ),
    entry!(
        "engine.river_ripple_repeat",
        Engine,
        river_ripple_repeat,
        &["World units one repeat of the river's ripple normal map covers."]
    ),
    entry!(
        "engine.river_ripple_slope",
        Engine,
        river_ripple_slope,
        &["How steeply those ripples tilt the river. 0 is flat and glassy."]
    ),
    entry!(
        "engine.river_ripple_detail_rate",
        Engine,
        river_ripple_detail_rate,
        &["How much faster the second ripple layer scrolls."]
    ),
    entry!(
        "engine.river_specular_gain",
        Engine,
        river_specular_gain,
        &["Brightness of the river's sun glint."]
    ),
    entry!(
        "engine.river_specular_exponent",
        Engine,
        river_specular_exponent,
        &["Tightness of that glint. Higher is a smaller, harder highlight."]
    ),
    entry!(
        "engine.river_sparkle_gain",
        Engine,
        river_sparkle_gain,
        &["Gain on the river's sparkle layer."]
    ),
    entry!(
        "engine.river_bank_turbulence",
        Engine,
        river_bank_turbulence,
        &["Extra ripple amplitude where the water shears against the bank."]
    ),
    entry!(
        "engine.river_edge_band",
        Engine,
        river_edge_band,
        &["World units over which a river dissolves onto dry ground."]
    ),
    entry!(
        "engine.river_shallow_ripple",
        Engine,
        river_shallow_ripple,
        &["Extra ripple amplitude in shallow water."]
    ),
    entry!(
        "engine.river_reflection_strength",
        Engine,
        river_reflection_strength,
        &["How much sky a river takes from the lake plane it shares."]
    ),
    entry!(
        "engine.river_texture_scroll",
        Engine,
        river_texture_scroll,
        &["How fast the river's own texture travels downstream. 0 pins it still."]
    ),
    entry!(
        "engine.wave_alpha_gain",
        Engine,
        wave_alpha_gain,
        &["Gain on the wave sprite's alpha."]
    ),
    entry!(
        "engine.wave_beach_band",
        Engine,
        wave_beach_band,
        &["World units of water column over which a wave dissolves."]
    ),
    entry!(
        "engine.wave_crest_height",
        Engine,
        wave_crest_height,
        &["World units a wave's leading edge tilts up by on the crest rung."]
    ),
    entry!(
        "engine.wave_compressed_depth",
        Engine,
        wave_compressed_depth,
        &["What fraction of its across-shore extent a wave collapses to."]
    ),
    entry!(
        "engine.wave_fade_in",
        Engine,
        wave_fade_in,
        &["Fraction of a wave's life spent rising."]
    ),
    entry!(
        "engine.wave_fade_out",
        Engine,
        wave_fade_out,
        &["Fraction of a wave's life at which it starts to die."]
    ),
    entry!(
        "engine.terrain_cloud_strength",
        Engine,
        terrain_cloud_strength,
        &["How much of the map's cloud shadow darkens the ground."]
    ),
    entry!(
        "engine.terrain_cloud_scale",
        Engine,
        terrain_cloud_scale,
        &["World units one repeat of the cloud texture covers."]
    ),
    entry!(
        "engine.terrain_cloud_scroll",
        Engine,
        terrain_cloud_scroll,
        &["Factor on the cloud layer's scroll rate."]
    ),
    entry!(
        "engine.terrain_macro_strength",
        Engine,
        terrain_macro_strength,
        &["How much of the map's macro texture darkens the ground."]
    ),
    entry!(
        "engine.sky_horizon_r",
        Engine,
        sky_horizon_r,
        &["Sky colour at the horizon, red. Linear, not sRGB."]
    ),
    entry!(
        "engine.sky_horizon_g",
        Engine,
        sky_horizon_g,
        &["Sky colour at the horizon, green."]
    ),
    entry!(
        "engine.sky_horizon_b",
        Engine,
        sky_horizon_b,
        &["Sky colour at the horizon, blue."]
    ),
    entry!(
        "engine.sky_zenith_r",
        Engine,
        sky_zenith_r,
        &["Sky colour straight overhead, red. Linear, not sRGB."]
    ),
    entry!(
        "engine.sky_zenith_g",
        Engine,
        sky_zenith_g,
        &["Sky colour straight overhead, green."]
    ),
    entry!(
        "engine.sky_zenith_b",
        Engine,
        sky_zenith_b,
        &["Sky colour straight overhead, blue."]
    ),
    entry!(
        "engine.sky_gradient_power",
        Engine,
        sky_gradient_power,
        &["How tightly the sky gradient hugs the horizon. Under 1 widens it."]
    ),
    entry!(
        "engine.sky_cloud_strength",
        Engine,
        sky_cloud_strength,
        &["How strongly the scrolling cloud sheet covers the sky."]
    ),
    entry!(
        "engine.sky_cloud_scale",
        Engine,
        sky_cloud_scale,
        &["World units one repeat of the cloud texture covers."]
    ),
    entry!(
        "engine.sky_cloud_scroll",
        Engine,
        sky_cloud_scroll,
        &["Factor on the cloud sheet's 2 world units a second of travel."]
    ),
    entry!(
        "engine.sky_cloud_horizon",
        Engine,
        sky_cloud_horizon,
        &["How far above the horizon the cloud sheet fades in over."]
    ),
    entry!(
        "engine.shadow_lightness",
        Engine,
        shadow_lightness,
        &["What the sun is multiplied by in shadow. 1 is no shadow at all."]
    ),
    entry!(
        "engine.terrain_shadow_lightness",
        Engine,
        terrain_shadow_lightness,
        &[
            "What the ground's sun term is multiplied by in shadow. 0 is what",
            "terrain.fxo does: it puts no constant between the four-tap filter",
            "and the multiply, and declares no shadow-strength parameter at",
            "all. The ambient and the two accents are never shadowed, so 0 is",
            "not black. engine.shadow_lightness still weights the models.",
        ]
    ),
    entry!(
        "engine.shadow_radius",
        Engine,
        shadow_radius,
        &["World units around the camera's target the shadow map covers."]
    ),
    entry!(
        "engine.shadow_reach",
        Engine,
        shadow_reach,
        &[
            "How far down the view the shadow map reaches, as a factor on the",
            "camera's own distance to its target. Reach and sharpness are one",
            "number: the world units a texel is printed on every change.",
        ]
    ),
    entry!(
        "engine.shadow_depth",
        Engine,
        shadow_depth,
        &["World units up-light of the view a caster still casts from."]
    ),
    entry!(
        "engine.shadow_depth_bias",
        Engine,
        shadow_bias,
        &[
            "World units of depth bias against self-shadowing.",
            "Too much detaches a shadow from the foot of what casts it.",
        ]
    ),
    entry!(
        "engine.shadow_contact_growth",
        Engine,
        shadow_contact_growth,
        &[
            "Contact edge (--shadow-filter contact): world units of penumbra",
            "per world unit between a point and what shades it - the light's",
            "apparent size. The real sun's is 0.009. Larger softens the far",
            "ends of tall shadows.",
        ]
    ),
    entry!(
        "engine.shadow_contact_min_texels",
        Engine,
        shadow_contact_min_texels,
        &[
            "Contact edge: the narrowest penumbra, in shadow-map texels -",
            "what keeps a small moving caster from drawing a stepped outline.",
        ]
    ),
    entry!(
        "engine.shadow_contact_max_texels",
        Engine,
        shadow_contact_max_texels,
        &["Contact edge: the widest penumbra, in shadow-map texels."]
    ),
    entry!(
        "engine.shadow_cascade_split",
        Engine,
        shadow_cascade_split,
        &[
            "How the shadow cascades divide the visible ground: 0 into equal",
            "lengths, 1 into equal ratios (sharper near the camera). The last",
            "always reaches as far as the single map did. The console's",
            "first-fit line prints each cascade's world units a texel.",
        ]
    ),
    entry!(
        "engine.shadow_cascade_blend",
        Engine,
        shadow_cascade_blend,
        &[
            "How far before its edge a cascade fades into the next, as a",
            "fraction of its half-width. 0 switches at a hard line.",
        ]
    ),
    entry!(
        "engine.contact_shadow_length",
        Engine,
        contact_shadow_length,
        &[
            "Contact shadows (--contact-shadows): world units each screen-",
            "space ray reaches toward the sun. Longer catches more, and more",
            "of what is wrong.",
        ]
    ),
    entry!(
        "engine.contact_shadow_steps",
        Engine,
        contact_shadow_steps,
        &[
            "Contact shadows: depth samples along each ray, up to 64. 0 turns",
            "them off."
        ]
    ),
    entry!(
        "engine.contact_shadow_thickness",
        Engine,
        contact_shadow_thickness,
        &[
            "Contact shadows: world units a surface in the depth buffer is",
            "taken to be thick. A ray further behind it than this went round",
            "it.",
        ]
    ),
    entry!(
        "map.shadow_map_size",
        Map,
        shadow_map_size,
        &["Texels a side of the shadow map, a power of two. Load-time."]
    ),
    entry!(
        "map.shadow_cascades",
        Map,
        shadow_cascades,
        &[
            "Shadow cascades, 1 to 4, each a map of map.shadow_map_size of its",
            "own: nearer ones are sharper. Load-time.",
        ]
    ),
    entry!(
        "map.particle_max_count",
        Map,
        particle_max_count,
        &[
            "Live particles allowed at once. gamedata.ini says 2500.",
            "0 removes the ceiling: one shipped map wants 4382 without it.",
        ]
    ),
    entry!(
        "map.particle_skip_mask",
        Map,
        particle_skip_mask,
        &[
            "Create only every 2nd (1) or 4th (3) particle. 0 creates all.",
            "gamelod.ini's own five dynamic rungs are 0, 0, 1, 3, 3.",
        ]
    ),
];

pub static MODES: &[ModeEntry] = &[
    mode!(
        "mode.water",
        water_draw,
        &["off", "flat", "low", "default"],
        "Hides or shows the standing water - lakes and oceans.",
        Some(&[
            "-------------------------------------------------------------------",
            "Which layers are drawn. Set one to off to judge the others alone.",
            "-------------------------------------------------------------------",
        ])
    ),
    mode!(
        "mode.rivers",
        river_draw,
        &["off", "flat", "texture", "shader"],
        "Hides or shows the river ribbons."
    ),
    mode!(
        "mode.waves",
        wave_draw,
        &["off", "on"],
        "Hides or shows the map's placed waves. Independent of mode.water."
    ),
    mode!(
        "mode.water_drift",
        water_drift,
        &["travel", "still"],
        "Whether the surface pattern travels or churns in place.",
        Some(&[
            "-------------------------------------------------------------------",
            "How each layer is drawn.",
            "-------------------------------------------------------------------",
        ])
    ),
    mode!(
        "mode.water_tint",
        water_tint,
        &["flat", "depth", "ramp", "ramp-tinted"],
        "Where the water's colour comes from."
    ),
    mode!(
        "mode.water_depth",
        water_depth,
        &["vertex", "soft", "absorb", "show"],
        "How the water's depth is measured; show paints the measurement."
    ),
    mode!(
        "mode.water_alpha",
        water_alpha,
        &["ini", "map"],
        "Which file supplies the alpha limits."
    ),
    mode!(
        "mode.reflection",
        water_reflection,
        &["off", "fresnel", "flat", "mirror"],
        "How the reflection is weighted; mirror shows it alone, for diagnosis."
    ),
    mode!(
        "mode.reflection_source",
        reflection_source,
        &["environment", "planar"],
        "Where the reflection's colour comes from."
    ),
    mode!(
        "mode.reflection_composite",
        reflection_composite,
        &["surface", "body"],
        "Whether the reflection covers the ground under the water too."
    ),
    mode!(
        "mode.sky",
        sky_draw,
        &["off", "gradient", "full"],
        "An invented stand-in sky. The real one is a placed W3D dome."
    ),
    mode!(
        "mode.water_highlights",
        water_highlights,
        &["off", "specular", "shore", "crest", "all"],
        "Which of the surface's own light terms is drawn."
    ),
    mode!(
        "mode.refraction",
        water_refraction,
        &["off", "on", "unclamped"],
        "Whether the ground under the water bends."
    ),
    mode!(
        "mode.water_normal",
        water_normal,
        &["unit", "slope"],
        "Which surface normal the ripples use."
    ),
    mode!(
        "mode.water_motion",
        water_motion,
        &["run", "freeze"],
        "Whether the water clock runs."
    ),
    mode!(
        "mode.river_stage",
        river_stage,
        &["surface", "opacity", "sparkle", "all", "normal"],
        "Which stage of the river composite is drawn."
    ),
    mode!(
        "mode.river_flow",
        river_flow,
        &["channel", "diagonal"],
        "Which frame a river scrolls in."
    ),
    mode!(
        "mode.wave_pca",
        wave_pca,
        &["ignore", "crest", "displace"],
        "What enablePcaWave selects."
    ),
    mode!(
        "mode.wave_scale",
        wave_scale,
        &["record", "cells"],
        "Whether a wave record's sizes are world units or terrain cells."
    ),
    mode!(
        "mode.terrain_overbright",
        terrain_overbright,
        &["all", "accents"],
        "Which lights the map's 2x overbright factor multiplies.",
        Some(&[
            "-------------------------------------------------------------------",
            "The terrain's lighting, not the water.",
            "-------------------------------------------------------------------",
        ])
    ),
    mode!(
        "mode.terrain_ceiling",
        terrain_ceiling,
        &["clamped", "none"],
        "Whether the ground's lit term is clamped before the texture."
    ),
    mode!(
        "mode.terrain_cloud",
        terrain_cloud,
        &["off", "texture", "factor"],
        "The map's cloud shadow layer."
    ),
    mode!(
        "mode.terrain_macro",
        terrain_macro,
        &["off", "on"],
        "The map's macro texture layer."
    ),
    mode!(
        "mode.shadows",
        shadows,
        &["off", "on"],
        "Whether the sun casts a shadow map over the whole scene."
    ),
];

pub static TEXTS: &[TextEntry] = &[
    TextEntry {
        key: "particle.system",
        get: |t| t.particle_system,
        set: |t, v| t.particle_system = v,
        doc: &[
            "Which particle system the debug spawn plays, by its INI name.",
            "Changing it respawns at the point the camera is looking at.",
        ],
    },
    TextEntry {
        key: "particle.fxlist",
        get: |t| t.particle_fxlist,
        set: |t, v| t.particle_fxlist = v,
        doc: &[
            "Which FXList the 0 key fires, by its INI name. There is no",
            "game event yet, so a key is the only thing that fires one.",
        ],
    },
];

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(missing_docs)]
pub struct WaterTuning {
    pub river_uv_scroll: f32,
    pub wave_unit: f32,
    pub wave_velocity: f32,
    pub wave_final_width: f32,
    pub wave_final_height: f32,
    pub wave_distance_from_shore: f32,
    pub wave_time: f32,
    pub env_max_alpha_depth: f32,
    pub env_deep_water_alpha: f32,
    pub water_shore_fade: f32,
    pub water_surface_drift: f32,
    pub water_tessellation_edge: f32,
    pub water_vertex_budget: f32,
    pub wave_surface_amplitude: f32,
    pub wave_surface_steepness: f32,
    pub wave_surface_reach: f32,
    pub wave_surface_foam: f32,
    pub wave_foam_amplitude: f32,
    pub wave_foam_response: f32,
    pub water_fresnel_f0: f32,
    pub water_reflection_floor: f32,
    pub water_refraction_scale: f32,
    pub water_refraction_falloff: f32,
    pub water_bump_slope: f32,
    pub water_slope_normalize: f32,
    pub water_drift_speed_cap: f32,
    pub water_unit_slope: f32,
    pub water_planar_perturbation: f32,
    pub water_reflection_blur: f32,
    pub water_ripple_detail: f32,
    pub water_ripple_repeat: f32,
    pub water_ripple_lod_bias: f32,
    pub particle_max_count: f32,
    pub particle_skip_mask: f32,
    pub water_ripple_light: f32,
    pub water_still_orbit: f32,
    pub water_drift_rate_cap: f32,
    pub water_specular_gain: f32,
    pub water_foam_band: f32,
    pub water_foam_gain: f32,
    pub water_crest_threshold: f32,
    pub river_ripple_repeat: f32,
    pub river_ripple_slope: f32,
    pub river_ripple_detail_rate: f32,
    pub river_specular_gain: f32,
    pub river_specular_exponent: f32,
    pub river_sparkle_gain: f32,
    pub river_bank_turbulence: f32,
    pub river_edge_band: f32,
    pub river_shallow_ripple: f32,
    pub river_reflection_strength: f32,
    pub river_texture_scroll: f32,
    pub wave_alpha_gain: f32,
    pub wave_beach_band: f32,
    pub wave_crest_height: f32,
    pub wave_compressed_depth: f32,
    pub wave_fade_in: f32,
    pub wave_fade_out: f32,
    pub terrain_cloud_strength: f32,
    pub terrain_cloud_scale: f32,
    pub terrain_cloud_scroll: f32,
    pub terrain_macro_strength: f32,
    pub sky_horizon_r: f32,
    pub sky_horizon_g: f32,
    pub sky_horizon_b: f32,
    pub sky_zenith_r: f32,
    pub sky_zenith_g: f32,
    pub sky_zenith_b: f32,
    pub sky_gradient_power: f32,
    pub sky_cloud_strength: f32,
    pub sky_cloud_scale: f32,
    pub sky_cloud_scroll: f32,
    pub sky_cloud_horizon: f32,
    pub shadow_lightness: f32,
    pub terrain_shadow_lightness: f32,
    pub shadow_radius: f32,
    pub shadow_reach: f32,
    pub shadow_depth: f32,
    pub shadow_bias: f32,
    pub shadow_map_size: f32,
    pub shadow_contact_growth: f32,
    pub shadow_contact_min_texels: f32,
    pub shadow_contact_max_texels: f32,
    pub shadow_cascades: f32,
    pub shadow_cascade_split: f32,
    pub shadow_cascade_blend: f32,
    pub contact_shadow_length: f32,
    pub contact_shadow_steps: f32,
    pub contact_shadow_thickness: f32,
    pub water_draw: u8,
    pub water_tint: u8,
    pub water_depth: u8,
    pub water_alpha: u8,
    pub water_reflection: u8,
    pub reflection_source: u8,
    pub reflection_composite: u8,
    pub sky_draw: u8,
    pub shadows: u8,
    pub water_highlights: u8,
    pub water_refraction: u8,
    pub water_normal: u8,
    pub water_drift: u8,
    pub water_motion: u8,
    pub river_draw: u8,
    pub river_stage: u8,
    pub river_flow: u8,
    pub wave_draw: u8,
    pub wave_pca: u8,
    pub wave_scale: u8,
    pub terrain_overbright: u8,
    pub terrain_ceiling: u8,
    pub terrain_cloud: u8,
    pub terrain_macro: u8,
    pub particle_system: TuningName,
    pub particle_fxlist: TuningName,
}

impl Default for WaterTuning {
    fn default() -> Self {
        Self {
            river_uv_scroll: crate::river_shader::RIVER_SCROLL_UNIT,
            wave_unit: 1.0,
            wave_velocity: crate::waves::WAVE_VELOCITY_UNIT,
            wave_final_width: 1.0,
            wave_final_height: 1.0,
            wave_distance_from_shore: 1.0,
            wave_time: 1.0,
            env_max_alpha_depth: 1.0,
            env_deep_water_alpha: 1.0,
            water_shore_fade: 1.0,
            water_surface_drift: 1.0,
            water_tessellation_edge: crate::water_depth::WATER_DEPTH_EDGE,
            #[allow(clippy::cast_precision_loss)]
            water_vertex_budget: crate::water_depth::MAX_WATER_VERTICES as f32,
            wave_surface_amplitude: 0.25,
            wave_surface_steepness: 0.6,
            wave_surface_reach: 2.0,
            wave_surface_foam: 1.0,
            wave_foam_amplitude: crate::water_shader::WATER_WAVE_FOAM_AMPLITUDE,
            wave_foam_response: crate::water_shader::WATER_WAVE_FOAM_RESPONSE,
            water_fresnel_f0: crate::water_shader::WATER_FRESNEL_F0,
            water_reflection_floor: crate::water_shader::WATER_REFLECTION_FLOOR,
            water_refraction_scale: crate::water_shader::WATER_REFRACTION_SCALE,
            water_refraction_falloff: crate::water_shader::WATER_REFRACTION_FALLOFF,
            water_bump_slope: crate::water_shader::WATER_BUMP_SLOPE,
            water_slope_normalize: crate::water_shader::WATER_SLOPE_NORMALIZE,
            water_drift_speed_cap: crate::water_shader::WATER_DRIFT_SPEED_CAP,
            water_unit_slope: crate::water_shader::WATER_UNIT_SLOPE,
            water_planar_perturbation: crate::water_shader::WATER_PLANAR_PERTURBATION,
            water_reflection_blur: crate::water_shader::WATER_REFLECTION_BLUR,
            water_ripple_detail: crate::water_shader::WATER_RIPPLE_DETAIL,
            water_ripple_repeat: crate::water_shader::WATER_RIPPLE_REPEAT,
            water_ripple_lod_bias: crate::water_shader::WATER_RIPPLE_LOD_BIAS,
            particle_max_count: 2_500.0,
            particle_skip_mask: 0.0,
            water_ripple_light: crate::water_shader::WATER_RIPPLE_LIGHT,
            water_still_orbit: crate::water_shader::WATER_STILL_ORBIT,
            water_drift_rate_cap: crate::water_shader::WATER_DRIFT_RATE_CAP,
            water_specular_gain: crate::water_shader::WATER_SPECULAR_GAIN,
            water_foam_band: crate::water_shader::WATER_FOAM_BAND,
            water_foam_gain: crate::water_shader::WATER_FOAM_GAIN,
            water_crest_threshold: crate::water_shader::WATER_CREST_THRESHOLD,
            river_ripple_repeat: crate::river_shader::RIVER_RIPPLE_REPEAT,
            river_ripple_slope: crate::river_shader::RIVER_RIPPLE_SLOPE,
            river_ripple_detail_rate: crate::river_shader::RIVER_RIPPLE_DETAIL_RATE,
            river_specular_gain: crate::river_shader::RIVER_SPECULAR_GAIN,
            river_specular_exponent: crate::river_shader::RIVER_SPECULAR_EXPONENT,
            river_sparkle_gain: crate::river_shader::RIVER_SPARKLE_GAIN,
            river_bank_turbulence: crate::river_shader::RIVER_BANK_TURBULENCE,
            river_edge_band: crate::river_shader::RIVER_EDGE_BAND,
            river_shallow_ripple: crate::river_shader::RIVER_SHALLOW_RIPPLE,
            river_reflection_strength: crate::river_shader::RIVER_REFLECTION_STRENGTH,
            river_texture_scroll: crate::river_shader::RIVER_TEXTURE_SCROLL,
            wave_alpha_gain: crate::wave_shader::WAVE_ALPHA_GAIN,
            wave_beach_band: crate::wave_shader::WAVE_BEACH_BAND,
            wave_crest_height: crate::wave_shader::WAVE_CREST_HEIGHT,
            wave_compressed_depth: crate::wave_shader::WAVE_COMPRESSED_DEPTH,
            wave_fade_in: crate::wave_shader::WAVE_FADE_IN,
            wave_fade_out: crate::wave_shader::WAVE_FADE_OUT,
            terrain_cloud_strength: 1.0,
            terrain_cloud_scale: crate::terrain_shader::TERRAIN_CLOUD_REPEAT,
            terrain_cloud_scroll: 1.0,
            terrain_macro_strength: 1.0,
            sky_horizon_r: 0.30,
            sky_horizon_g: 0.45,
            sky_horizon_b: 0.68,
            sky_zenith_r: 0.09,
            sky_zenith_g: 0.24,
            sky_zenith_b: 0.62,
            sky_gradient_power: 0.55,
            sky_cloud_strength: 1.0,
            sky_cloud_scale: crate::sky::SKY_CLOUD_REPEAT,
            sky_cloud_scroll: 1.0,
            sky_cloud_horizon: 0.12,
            shadow_lightness: 0.55,
            terrain_shadow_lightness: 0.0,
            shadow_radius: 900.0,
            shadow_reach: 1.15,
            shadow_depth: 1000.0,
            shadow_bias: 5.0,
            shadow_map_size: 2048.0,
            shadow_contact_growth: 0.03,
            shadow_contact_min_texels: 1.5,
            shadow_contact_max_texels: 4.0,
            shadow_cascades: 3.0,
            shadow_cascade_split: 0.75,
            shadow_cascade_blend: 0.1,
            contact_shadow_length: 12.0,
            contact_shadow_steps: 16.0,
            contact_shadow_thickness: 4.0,
            water_draw: 3,
            water_tint: 1,
            water_depth: 2,
            water_alpha: 1,
            water_reflection: 1,
            reflection_source: 0,
            reflection_composite: 1,
            sky_draw: 0,
            shadows: 1,
            water_highlights: 4,
            water_refraction: 1,
            water_normal: 1,
            water_drift: 0,
            water_motion: 0,
            river_draw: 3,
            river_stage: 3,
            river_flow: 0,
            wave_draw: 1,
            wave_pca: 2,
            wave_scale: 0,
            terrain_overbright: 1,
            terrain_ceiling: 1,
            terrain_cloud: 1,
            terrain_macro: 1,
            particle_system: TuningName::EMPTY,
            particle_fxlist: TuningName::EMPTY,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedTuning {
    pub tuning: WaterTuning,
    pub warnings: Vec<String>,
}

impl WaterTuning {
    #[must_use]
    pub fn parse(text: &str) -> ParsedTuning {
        let mut tuning = Self::default();
        let mut warnings = Vec::new();
        for (index, raw) in text.lines().enumerate() {
            let number = index.saturating_add(1);
            let line = match raw.split_once('#') {
                Some((before, _)) => before,
                None => raw,
            }
            .trim();
            if line.is_empty() {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                warnings.push(format!("line {number}: no '=' in {line:?}"));
                continue;
            };
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim();
            if let Some(entry) = ENTRIES.iter().find(|e| e.key == key) {
                match value.parse::<f32>() {
                    Ok(parsed) if parsed.is_finite() => (entry.set)(&mut tuning, parsed),
                    Ok(_) => warnings.push(format!("line {number}: {key} is not a finite number")),
                    Err(error) => {
                        warnings.push(format!("line {number}: {key} = {value:?}: {error}"));
                    }
                }
                continue;
            }
            if let Some(entry) = TEXTS.iter().find(|e| e.key == key) {
                match TuningName::new(value) {
                    Some(name) => (entry.set)(&mut tuning, name),
                    None => warnings.push(format!(
                        "line {number}: {key} = {value:?}: wants at most {MAX_TUNING_NAME}                          printable ASCII characters"
                    )),
                }
                continue;
            }
            if let Some(entry) = MODES.iter().find(|e| e.key == key) {
                let wanted = value.to_ascii_lowercase();
                match entry.values.iter().position(|name| *name == wanted) {
                    #[allow(clippy::cast_possible_truncation)]
                    Some(slot) => (entry.set)(&mut tuning, slot as u8),
                    None => warnings.push(format!(
                        "line {number}: {key} = {value:?}: wants {}",
                        entry.values.join(" | ")
                    )),
                }
                continue;
            }
            warnings.push(format!("line {number}: unknown key {key:?}"));
        }
        ParsedTuning { tuning, warnings }
    }

    #[must_use]
    pub fn drift_mode(&self) -> crate::water_shader::WaterDrift {
        self.mode("mode.water_drift")
            .and_then(crate::water_shader::WaterDrift::parse)
            .unwrap_or_default()
    }

    #[must_use]
    pub fn mode(&self, key: &str) -> Option<&'static str> {
        let entry = MODES.iter().find(|e| e.key == key)?;
        let slot = usize::from((entry.get)(self));
        entry.values.get(slot).copied()
    }

    #[must_use]
    pub fn differences(&self, other: &Self) -> Vec<String> {
        let mut out = Vec::new();
        for entry in ENTRIES {
            let before = (entry.get)(self);
            let after = (entry.get)(other);
            if before.to_bits() != after.to_bits() {
                out.push(format!("{}: {before} -> {after}", entry.key));
            }
        }
        for entry in TEXTS {
            let before = (entry.get)(self);
            let after = (entry.get)(other);
            if before != after {
                out.push(format!(
                    "{}: {:?} -> {:?}",
                    entry.key,
                    before.as_str(),
                    after.as_str()
                ));
            }
        }
        for entry in MODES {
            let before = (entry.get)(self);
            let after = (entry.get)(other);
            if before != after {
                let name = |slot: u8| entry.values.get(usize::from(slot)).copied().unwrap_or("?");
                out.push(format!(
                    "{}: {} -> {}",
                    entry.key,
                    name(before),
                    name(after)
                ));
            }
        }
        out
    }

    #[must_use]
    pub fn default_text(&self) -> String {
        let mut out = String::new();
        out.push_str(
            "# mint tuning - the water systems, and the terrain lighting\n\
             #\n\
             # Re-read while the viewer runs: save it and the next frame changes.\n\
             # Every value below is the default, so an untouched file changes nothing.\n\
             #\n\
             # One `key = value` per line; `#` starts a comment. A line that does not\n\
             # parse is reported in the console and leaves its knob alone.\n\
             #\n\
             # Delete the file to get this one back.\n",
        );
        for section in [Section::Map, Section::Engine, Section::Mode, Section::Text] {
            let _ = write!(
                out,
                "\n\
                 # ===================================================================\n\
                 # {}\n\
                 # ===================================================================\n",
                section.title()
            );
            for line in section.blurb() {
                if line.is_empty() {
                    let _ = writeln!(out, "#");
                } else {
                    let _ = writeln!(out, "# {line}");
                }
            }
            for entry in ENTRIES.iter().filter(|e| e.section == section) {
                out.push('\n');
                for line in entry.doc {
                    let _ = writeln!(out, "# {line}");
                }
                let _ = writeln!(out, "{} = {}", entry.key, (entry.get)(self));
            }
            if section == Section::Text {
                for entry in TEXTS {
                    out.push('\n');
                    for line in entry.doc {
                        let _ = writeln!(out, "# {line}");
                    }
                    let _ = writeln!(out, "{} = {}", entry.key, (entry.get)(self).as_str());
                }
            }
            if section == Section::Mode {
                for entry in MODES {
                    if let Some(heading) = entry.heading {
                        out.push('\n');
                        for line in heading {
                            let _ = writeln!(out, "# {line}");
                        }
                    }
                    let selected = entry
                        .values
                        .get(usize::from((entry.get)(self)))
                        .copied()
                        .unwrap_or("?");
                    let _ = write!(
                        out,
                        "\n# {}\n# values: {}\n{} = {selected}\n",
                        entry.doc,
                        entry.values.join(" | "),
                        entry.key
                    );
                }
            }
        }
        out
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn the_generated_default_file_reads_back_as_the_defaults() {
        let defaults = WaterTuning::default();
        let parsed = WaterTuning::parse(&defaults.default_text());
        assert_eq!(parsed.warnings, Vec::<String>::new());
        assert_eq!(parsed.tuning, defaults);
        assert_eq!(defaults.differences(&parsed.tuning), Vec::<String>::new());
    }

    #[test]
    fn no_generated_line_is_wider_than_the_file_has_ever_been() {
        let text = WaterTuning::default().default_text();
        let widest = text.lines().map(str::len).max().unwrap_or(0);
        assert!(widest <= 73, "widest generated line is {widest} characters");
    }

    #[test]
    fn a_name_round_trips_and_an_impossible_one_is_refused_by_line() {
        let parsed = WaterTuning::parse(
            "particle.system = AngBuildingContructDust
",
        );
        assert_eq!(parsed.warnings, Vec::<String>::new());
        assert_eq!(
            parsed.tuning.particle_system.as_str(),
            "AngBuildingContructDust"
        );

        let long = "x".repeat(MAX_TUNING_NAME + 1);
        let parsed = WaterTuning::parse(&format!(
            "particle.system = {long}
"
        ));
        assert_eq!(parsed.warnings.len(), 1);
        assert!(parsed.warnings[0].starts_with("line 1: particle.system"));
        assert!(parsed.tuning.particle_system.is_empty(), "left alone");

        let parsed = WaterTuning::parse(
            "particle.system = a b
",
        );
        assert_eq!(parsed.warnings.len(), 1, "a space is not part of a name");

        let parsed = WaterTuning::parse(
            "particle.system =
",
        );
        assert_eq!(parsed.warnings, Vec::<String>::new());
        assert!(parsed.tuning.particle_system.is_empty());
    }

    #[test]
    fn no_two_entries_claim_the_same_key() {
        let mut keys: Vec<&str> = ENTRIES
            .iter()
            .map(|e| e.key)
            .chain(MODES.iter().map(|e| e.key))
            .chain(TEXTS.iter().map(|e| e.key))
            .collect();
        let total = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), total, "duplicate key in ENTRIES/MODES");
    }

    #[test]
    fn a_bad_line_keeps_its_value_names_its_line_number_and_does_not_stop_the_file() {
        let text = "\
# a comment, and the blank line below is ignored

engine.water_bump_slope
engine.water_bump_slope = wide
engine.water_bump_slope = NaN
engine.wobble = 3
mode.water_normal = sideways
engine.water_bump_slope = 0.25   # a good line, after five bad ones
";
        let parsed = WaterTuning::parse(text);
        let defaults = WaterTuning::default();

        assert_eq!(parsed.tuning.water_bump_slope, 0.25);
        assert_eq!(parsed.tuning.water_normal, defaults.water_normal);

        let lines: Vec<usize> = parsed
            .warnings
            .iter()
            .map(|w| {
                w.strip_prefix("line ")
                    .and_then(|rest| rest.split(':').next())
                    .and_then(|n| n.parse::<usize>().ok())
                    .unwrap_or(0)
            })
            .collect();
        assert_eq!(lines, vec![3, 4, 5, 6, 7], "{:?}", parsed.warnings);
        assert!(
            parsed.warnings[1].contains("wide"),
            "{}",
            parsed.warnings[1]
        );
        assert!(
            parsed.warnings[2].contains("not a finite number"),
            "{}",
            parsed.warnings[2]
        );
        assert!(
            parsed.warnings[3].contains("unknown key"),
            "{}",
            parsed.warnings[3]
        );
        assert!(
            parsed.warnings[4].contains("unit | slope"),
            "{}",
            parsed.warnings[4]
        );
    }

    #[test]
    fn a_reload_reports_every_knob_that_moved_and_no_others() {
        let before = WaterTuning::default();
        let after = WaterTuning::parse(
            "engine.water_bump_slope = 0.25\n\
             mode.water_normal = unit\n",
        )
        .tuning;
        let moved = before.differences(&after);
        assert_eq!(
            moved,
            vec![
                format!(
                    "engine.water_bump_slope: {} -> 0.25",
                    before.water_bump_slope
                ),
                "mode.water_normal: slope -> unit".to_string(),
            ]
        );
        assert_eq!(before.differences(&before), Vec::<String>::new());
    }
}
