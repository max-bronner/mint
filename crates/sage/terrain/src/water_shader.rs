use mint_math::f32::sin_cos;

pub const WATER_SHADER_WGSL: &str = concat!(
    include_str!("shroud_scene.wgsl"),
    include_str!("water_shader.wgsl")
);

pub const WATER_SCENE_TEXTURE_SLOTS: u32 = crate::shroud_scene::SHROUD_SCENE_TEXTURE_SLOTS;

pub const WATER_SCENE_UNIFORM_SIZE: u32 = crate::shroud_scene::SHROUD_SCENE_UNIFORM_SIZE;

pub const WATER_TEXTURE_SLOTS: u32 = 4;

pub const WATER_UV_BASE: f32 = 600.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WaterTechnique {
    #[default]
    Default,
    Low,
}

impl WaterTechnique {
    fn index(self) -> f32 {
        match self {
            Self::Default => 0.0,
            Self::Low => 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaterShaderParams {
    pub wind_angle: f32,
    pub wave_angle: f32,
    pub wind_speed: f32,
    pub wave_speed: f32,
    pub bump_speed: f32,
    pub bump_scale: f32,
    pub bump_uv_scale: [f32; 2],
    pub wave_uv_scale: [f32; 2],
    pub env_uv_scale: [f32; 2],
    pub dimming_factor: f32,
    pub reflection_strength: f32,
    pub reflection_perturbation: f32,
    pub specular_color: [f32; 3],
    pub specular_exponent: f32,
    pub shadow_lightness: Option<f32>,
}

impl Default for WaterShaderParams {
    fn default() -> Self {
        Self {
            wind_angle: 0.0,
            wave_angle: 0.0,
            wind_speed: 0.0,
            wave_speed: 0.0,
            bump_speed: 0.0,
            bump_scale: 1.0,
            bump_uv_scale: [1.0, 1.0],
            wave_uv_scale: [1.0, 1.0],
            env_uv_scale: [1.0, 1.0],
            dimming_factor: 1.0,
            reflection_strength: 0.0,
            reflection_perturbation: 0.02,
            specular_color: [1.0, 1.0, 1.0],
            specular_exponent: 50.0,
            shadow_lightness: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WaterReflection {
    Off,
    #[default]
    Fresnel,
    Flat,
    Mirror,
}

impl WaterReflection {
    fn index(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Fresnel => 1.0,
            Self::Flat => 2.0,
            Self::Mirror => 3.0,
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Fresnel,
            Self::Fresnel => Self::Flat,
            Self::Flat => Self::Mirror,
            Self::Mirror => Self::Off,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Fresnel => "fresnel",
            Self::Flat => "flat",
            Self::Mirror => "mirror",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "fresnel" => Some(Self::Fresnel),
            "flat" => Some(Self::Flat),
            "mirror" => Some(Self::Mirror),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WaterRefraction {
    Off,
    #[default]
    On,
    Unclamped,
}

impl WaterRefraction {
    pub(crate) fn index(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::On => 1.0,
            Self::Unclamped => 2.0,
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::On,
            Self::On => Self::Unclamped,
            Self::Unclamped => Self::Off,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::On => "on",
            Self::Unclamped => "unclamped",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "off" => Some(Self::Off),
            "on" => Some(Self::On),
            "unclamped" => Some(Self::Unclamped),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WaterNormal {
    Unit,
    #[default]
    Slope,
}

impl WaterNormal {
    fn index(self) -> f32 {
        match self {
            Self::Unit => 0.0,
            Self::Slope => 1.0,
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Unit => Self::Slope,
            Self::Slope => Self::Unit,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Unit => "unit",
            Self::Slope => "slope",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "unit" => Some(Self::Unit),
            "slope" => Some(Self::Slope),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WaterDistortion {
    pub refraction: WaterRefraction,
    pub normal: WaterNormal,
}

pub const WATER_UNIT_SLOPE: f32 = 0.05;

pub const WATER_BUMP_SLOPE: f32 = 0.6;

pub const WATER_SLOPE_NORMALIZE: f32 = 0.0;

pub const WATER_DRIFT_SPEED_CAP: f32 = 40.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WaterDrift {
    #[default]
    Travel,
    Still,
}

impl WaterDrift {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Travel => "travel",
            Self::Still => "still",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "travel" => Some(Self::Travel),
            "still" => Some(Self::Still),
            _ => None,
        }
    }
}

pub const WATER_STILL_ORBIT: f32 = 0.25;

pub const WATER_DRIFT_RATE_CAP: f32 = 0.02;

pub const WATER_RIPPLE_DETAIL: f32 = 1.0;

pub const WATER_RIPPLE_REPEAT: f32 = 140.0;

pub const WATER_RIPPLE_LIGHT: f32 = 0.20;

pub const WATER_RIPPLE_LOD_BIAS: f32 = -1.0;

pub const WATER_REFLECTION_BLUR: f32 = 0.012;

pub const WATER_REFRACTION_SCALE: f32 = 8.0;

pub const WATER_REFRACTION_FALLOFF: f32 = 400.0;

pub const WATER_FRESNEL_F0: f32 = 0.02;

pub const WATER_REFLECTION_FLOOR: f32 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WaterReflectionSource {
    Environment,
    #[default]
    Planar,
}

impl WaterReflectionSource {
    fn index(self) -> f32 {
        match self {
            Self::Environment => 0.0,
            Self::Planar => 1.0,
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Environment => Self::Planar,
            Self::Planar => Self::Environment,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Environment => "environment",
            Self::Planar => "planar",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "environment" | "env" => Some(Self::Environment),
            "planar" => Some(Self::Planar),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WaterReflectionComposite {
    Surface,
    #[default]
    Body,
}

impl WaterReflectionComposite {
    fn index(self) -> f32 {
        match self {
            Self::Surface => 0.0,
            Self::Body => 1.0,
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Surface => Self::Body,
            Self::Body => Self::Surface,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Surface => "surface",
            Self::Body => "body",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "surface" => Some(Self::Surface),
            "body" => Some(Self::Body),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WaterHighlights {
    Off,
    Specular,
    Shore,
    Crest,
    #[default]
    All,
}

impl WaterHighlights {
    fn index(self) -> f32 {
        match self {
            Self::Off => 0.0,
            Self::Specular => 1.0,
            Self::Shore => 2.0,
            Self::Crest => 3.0,
            Self::All => 4.0,
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Specular,
            Self::Specular => Self::Shore,
            Self::Shore => Self::Crest,
            Self::Crest => Self::All,
            Self::All => Self::Off,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Specular => "specular",
            Self::Shore => "shore",
            Self::Crest => "crest",
            Self::All => "all",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "off" => Some(Self::Off),
            "specular" => Some(Self::Specular),
            "shore" => Some(Self::Shore),
            "crest" => Some(Self::Crest),
            "all" => Some(Self::All),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WaterSurfaceLook {
    pub source: WaterReflectionSource,
    pub composite: WaterReflectionComposite,
    pub highlights: WaterHighlights,
}

pub const WATER_PLANAR_PERTURBATION: f32 = 0.06;

pub const WATER_SPECULAR_GAIN: f32 = 8.0;

pub const WATER_FOAM_BAND: f32 = 4.0;

pub const WATER_CREST_THRESHOLD: f32 = 0.15;

pub const WATER_WAVE_FOAM_AMPLITUDE: f32 = 1.0;

pub const WATER_WAVE_FOAM_RESPONSE: f32 = 2.0;

pub const WATER_FOAM_GAIN: f32 = 3.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WaterDepthMode {
    Vertex,
    Soft,
    #[default]
    Absorb,
    Show,
}

impl WaterDepthMode {
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Vertex => Self::Soft,
            Self::Soft => Self::Absorb,
            Self::Absorb => Self::Show,
            Self::Show => Self::Vertex,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Vertex => "vertex",
            Self::Soft => "soft",
            Self::Absorb => "absorb",
            Self::Show => "show",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text.to_ascii_lowercase().as_str() {
            "vertex" => Some(Self::Vertex),
            "soft" => Some(Self::Soft),
            "absorb" => Some(Self::Absorb),
            "show" => Some(Self::Show),
            _ => None,
        }
    }

    #[must_use]
    pub fn index(self) -> f32 {
        match self {
            Self::Vertex => 0.0,
            Self::Soft => 1.0,
            Self::Absorb => 2.0,
            Self::Show => 3.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaterDepth {
    pub mode: WaterDepthMode,
    pub extinction: [f32; 3],
    pub fade_depth: f32,
    pub body: [f32; 3],
    pub max_alpha: f32,
    pub additive: bool,
    pub uniform_colour: bool,
}

impl Default for WaterDepth {
    fn default() -> Self {
        Self {
            mode: WaterDepthMode::Vertex,
            extinction: [0.0; 3],
            fade_depth: 0.0,
            body: [1.0; 3],
            max_alpha: 1.0,
            additive: false,
            uniform_colour: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaterMotion {
    pub rotation1: [f32; 2],
    pub rotation2: [f32; 2],
    pub bump1: [f32; 2],
    pub bump2: [f32; 2],
    pub wave1: [f32; 2],
    pub wave2: [f32; 2],
}

#[must_use]
fn wrap_angle(angle: f32) -> f32 {
    if !angle.is_finite() {
        return 0.0;
    }
    let turns = angle / std::f32::consts::TAU + 0.5;
    (turns - turns.floor()) * std::f32::consts::TAU - std::f32::consts::PI
}

#[must_use]
pub fn water_motion(params: &WaterShaderParams, seconds: f32) -> WaterMotion {
    water_motion_capped(params, seconds, f32::INFINITY)
}

#[must_use]
pub fn water_motion_capped(
    params: &WaterShaderParams,
    seconds: f32,
    drift_speed_cap: f32,
) -> WaterMotion {
    let half = if params.wave_angle.is_finite() {
        params.wave_angle * 0.5
    } else {
        0.0
    };
    let a1 = wrap_angle(params.wind_angle - half);
    let a2 = wrap_angle(params.wind_angle + half);
    let (sin1, cos1) = sin_cos(a1);
    let (sin2, cos2) = sin_cos(a2);

    let bump_travel = cap_travel(
        travel(params.wind_speed, params.bump_speed, seconds),
        seconds,
        drift_speed_cap,
    );
    let wave_travel = cap_travel(
        travel(params.wind_speed, params.wave_speed, seconds),
        seconds,
        drift_speed_cap,
    );
    WaterMotion {
        rotation1: [cos1, sin1],
        rotation2: [cos2, sin2],
        bump1: [bump_travel * cos1, bump_travel * sin1],
        bump2: [bump_travel * cos2, bump_travel * sin2],
        wave1: [wave_travel * cos1, wave_travel * sin1],
        wave2: [wave_travel * cos2, wave_travel * sin2],
    }
}

#[must_use]
pub fn water_motion_drift(
    params: &WaterShaderParams,
    seconds: f32,
    drift: WaterDrift,
    speed_cap: f32,
    orbit: f32,
    rate_cap: f32,
) -> WaterMotion {
    if drift == WaterDrift::Travel {
        return water_motion_capped(
            params,
            rate_capped_seconds(params, seconds, rate_cap),
            speed_cap,
        );
    }
    let half = if params.wave_angle.is_finite() {
        params.wave_angle * 0.5
    } else {
        0.0
    };
    let a1 = wrap_angle(params.wind_angle - half);
    let a2 = wrap_angle(params.wind_angle + half);
    let (sin1, cos1) = sin_cos(a1);
    let (sin2, cos2) = sin_cos(a2);
    let bump_scale = finite(params.bump_uv_scale[0] * params.bump_scale, 1.0).abs();
    let wave_scale = finite(params.wave_uv_scale[0], 1.0).abs();
    let bump = orbit_offsets(
        a1,
        a2,
        params.wind_speed + params.bump_speed,
        bump_scale,
        seconds,
        orbit,
        rate_cap,
    );
    let wave = orbit_offsets(
        a1,
        a2,
        params.wind_speed + params.wave_speed,
        wave_scale,
        seconds,
        orbit,
        rate_cap,
    );
    WaterMotion {
        rotation1: [cos1, sin1],
        rotation2: [cos2, sin2],
        bump1: bump.0,
        bump2: bump.1,
        wave1: wave.0,
        wave2: wave.1,
    }
}

fn rate_capped_seconds(params: &WaterShaderParams, seconds: f32, rate_cap: f32) -> f32 {
    if !rate_cap.is_finite() || rate_cap < 0.0 {
        return seconds;
    }
    let scale = finite(params.bump_uv_scale[0] * params.bump_scale, 1.0).abs();
    let rate = finite((params.wind_speed + params.bump_speed) * scale, 0.0).abs();
    if rate <= rate_cap {
        return seconds;
    }
    finite(seconds * (rate_cap / rate), seconds)
}

fn orbit_offsets(
    a1: f32,
    a2: f32,
    speed: f32,
    uv_scale: f32,
    seconds: f32,
    orbit: f32,
    rate_cap: f32,
) -> ([f32; 2], [f32; 2]) {
    let orbit = if orbit.is_finite() && orbit > 1e-4 {
        orbit
    } else {
        WATER_STILL_ORBIT
    };
    let rate = finite(speed * uv_scale, 0.0).abs();
    let cap = if rate_cap.is_finite() && rate_cap >= 0.0 {
        rate_cap
    } else {
        f32::INFINITY
    };
    let rate = rate.min(cap);
    let theta = finite(rate / orbit * seconds, 0.0);
    let radius = orbit / uv_scale.max(1e-4);
    let (s1, c1) = sin_cos(a1 + theta);
    let (s2, c2) = sin_cos(a2 - theta);
    ([radius * c1, radius * s1], [radius * c2, radius * s2])
}

fn ripple_offsets(
    params: &WaterShaderParams,
    seconds: f32,
    drift: WaterDrift,
    repeat: f32,
    orbit: f32,
    rate_cap: f32,
) -> [f32; 4] {
    let half = if params.wave_angle.is_finite() {
        params.wave_angle * 0.5
    } else {
        0.0
    };
    let a1 = wrap_angle(params.wind_angle - half);
    let a2 = wrap_angle(params.wind_angle + half);
    let coarse = if repeat.is_finite() && repeat > 1.0 {
        repeat
    } else {
        WATER_RIPPLE_REPEAT
    };
    let fine = coarse / 3.17;
    let scale = finite(params.bump_uv_scale[0] * params.bump_scale, 1.0).abs();
    let rate = finite((params.wind_speed + params.bump_speed) * scale, 0.0).abs();
    match drift {
        WaterDrift::Travel => {
            let rate = if rate_cap.is_finite() && rate_cap >= 0.0 {
                rate.min(rate_cap)
            } else {
                rate
            };
            let d1 = finite(rate * coarse * seconds, 0.0);
            let d2 = finite(rate * fine * seconds, 0.0);
            let (s1, c1) = sin_cos(a1);
            let (s2, c2) = sin_cos(a2);
            [d1 * c1, d1 * s1, d2 * c2, d2 * s2]
        }
        WaterDrift::Still => {
            let orbit = if orbit.is_finite() && orbit > 1e-4 {
                orbit
            } else {
                WATER_STILL_ORBIT
            };
            let cap = if rate_cap.is_finite() && rate_cap >= 0.0 {
                rate_cap
            } else {
                f32::INFINITY
            };
            let theta = finite(rate.min(cap) / orbit * seconds, 0.0);
            let (s1, c1) = sin_cos(a1 + theta);
            let (s2, c2) = sin_cos(a2 - theta);
            let r1 = orbit * coarse;
            let r2 = orbit * fine;
            [r1 * c1, r1 * s1, r2 * c2, r2 * s2]
        }
    }
}

fn cap_travel(distance: f32, seconds: f32, cap: f32) -> f32 {
    if !cap.is_finite() || cap < 0.0 || seconds == 0.0 || !seconds.is_finite() {
        return distance;
    }
    let limit = (cap / WATER_UV_BASE) * seconds.abs();
    distance.clamp(-limit, limit)
}

fn travel(shared: f32, layer: f32, seconds: f32) -> f32 {
    let distance = (shared + layer) * seconds;
    if distance.is_finite() {
        distance
    } else {
        0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaterUniforms {
    rotations: [f32; 4],
    bump_offsets: [f32; 4],
    wave_offsets: [f32; 4],
    uv_scales: [f32; 4],
    env: [f32; 4],
    terms: [f32; 4],
    reflection: [f32; 4],
    depth_terms: [f32; 4],
    absorption: [f32; 4],
    body: [f32; 4],
    refraction: [f32; 4],
    surface: [f32; 4],
    planar: [f32; 4],
    specular: [f32; 4],
    highlights: [f32; 4],
    ripple: [f32; 4],
    ripple_offsets: [f32; 4],
    ripple_light: [f32; 4],
    shadow: [f32; 4],
}

pub const WATER_UNIFORM_SIZE: u32 = 304 + crate::wave_surface::WAVE_SURFACE_BYTES;

fn finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

impl WaterUniforms {
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        params: &WaterShaderParams,
        technique: WaterTechnique,
        reflection: WaterReflection,
        distortion: WaterDistortion,
        look: WaterSurfaceLook,
        depth: WaterDepth,
        tuning: &crate::water_tuning::WaterTuning,
        seconds: f32,
    ) -> Self {
        let drift = finite(tuning.water_surface_drift, 1.0);
        let rung = crate::water_tuning::WaterTuning::drift_mode(tuning);
        let orbit = finite(tuning.water_still_orbit, WATER_STILL_ORBIT);
        let rate_cap = finite(tuning.water_drift_rate_cap, WATER_DRIFT_RATE_CAP);
        let motion = water_motion_drift(
            params,
            seconds * drift,
            rung,
            finite(tuning.water_drift_speed_cap, WATER_DRIFT_SPEED_CAP),
            orbit,
            rate_cap,
        );
        let ripple_repeat = finite(tuning.water_ripple_repeat, WATER_RIPPLE_REPEAT);
        let ripple_offsets = ripple_offsets(
            params,
            seconds * drift,
            rung,
            ripple_repeat,
            orbit,
            rate_cap,
        );
        let bump = [
            params.bump_uv_scale[0] * params.bump_scale,
            params.bump_uv_scale[1] * params.bump_scale,
        ];
        let foam = match technique {
            WaterTechnique::Default => 2.0,
            WaterTechnique::Low => 0.1,
        };
        let specular_gain = finite(tuning.water_specular_gain, WATER_SPECULAR_GAIN);
        Self {
            rotations: [
                motion.rotation1[0],
                motion.rotation1[1],
                motion.rotation2[0],
                motion.rotation2[1],
            ],
            bump_offsets: [
                motion.bump1[0],
                motion.bump1[1],
                motion.bump2[0],
                motion.bump2[1],
            ],
            wave_offsets: [
                motion.wave1[0],
                motion.wave1[1],
                motion.wave2[0],
                motion.wave2[1],
            ],
            uv_scales: [
                bump[0],
                bump[1],
                params.wave_uv_scale[0],
                params.wave_uv_scale[1],
            ],
            env: [params.env_uv_scale[0], params.env_uv_scale[1], 0.0, 0.0],
            terms: [
                params.dimming_factor,
                foam,
                finite(tuning.water_unit_slope, WATER_UNIT_SLOPE),
                technique.index(),
            ],
            reflection: [
                if params.reflection_strength.is_finite() {
                    params.reflection_strength
                } else {
                    0.0
                },
                finite(tuning.water_fresnel_f0, WATER_FRESNEL_F0),
                match technique {
                    WaterTechnique::Default => reflection.index(),
                    WaterTechnique::Low => WaterReflection::Off.index(),
                },
                finite(tuning.water_reflection_floor, WATER_REFLECTION_FLOOR).clamp(0.0, 1.0),
            ],
            depth_terms: [
                match technique {
                    WaterTechnique::Default => depth.mode.index(),
                    WaterTechnique::Low => WaterDepthMode::Vertex.index(),
                },
                finite(depth.fade_depth, 0.0),
                finite(depth.max_alpha, 1.0),
                if depth.additive { 1.0 } else { 0.0 },
            ],
            absorption: [
                finite(depth.extinction[0], 0.0),
                finite(depth.extinction[1], 0.0),
                finite(depth.extinction[2], 0.0),
                0.0,
            ],
            body: [
                finite(depth.body[0], 1.0),
                finite(depth.body[1], 1.0),
                finite(depth.body[2], 1.0),
                if depth.uniform_colour { 1.0 } else { 0.0 },
            ],
            refraction: [
                match technique {
                    WaterTechnique::Default if !depth.additive => distortion.refraction.index(),
                    _ => WaterRefraction::Off.index(),
                },
                finite(params.reflection_perturbation, 0.02)
                    * finite(tuning.water_refraction_scale, WATER_REFRACTION_SCALE),
                finite(tuning.water_refraction_falloff, WATER_REFRACTION_FALLOFF),
                0.0,
            ],
            surface: [
                match technique {
                    WaterTechnique::Default => distortion.normal.index(),
                    _ => WaterNormal::Unit.index(),
                },
                finite(tuning.water_bump_slope, WATER_BUMP_SLOPE),
                finite(tuning.water_slope_normalize, WATER_SLOPE_NORMALIZE).clamp(0.0, 1.0),
                0.0,
            ],
            planar: [
                match technique {
                    WaterTechnique::Default => look.source.index(),
                    _ => WaterReflectionSource::Environment.index(),
                },
                finite(tuning.water_planar_perturbation, WATER_PLANAR_PERTURBATION),
                match technique {
                    WaterTechnique::Default => look.composite.index(),
                    _ => WaterReflectionComposite::Surface.index(),
                },
                0.0,
            ],
            specular: [
                finite(params.specular_color[0], 1.0) * specular_gain,
                finite(params.specular_color[1], 1.0) * specular_gain,
                finite(params.specular_color[2], 1.0) * specular_gain,
                finite(params.specular_exponent, 50.0),
            ],
            highlights: [
                match technique {
                    WaterTechnique::Default => look.highlights.index(),
                    _ => WaterHighlights::Off.index(),
                },
                finite(tuning.water_foam_band, WATER_FOAM_BAND),
                finite(tuning.water_crest_threshold, WATER_CREST_THRESHOLD),
                finite(tuning.water_foam_gain, WATER_FOAM_GAIN),
            ],
            ripple: [
                finite(tuning.water_ripple_detail, WATER_RIPPLE_DETAIL),
                ripple_repeat,
                finite(tuning.water_ripple_lod_bias, WATER_RIPPLE_LOD_BIAS),
                finite(tuning.water_reflection_blur, WATER_REFLECTION_BLUR),
            ],
            ripple_offsets,
            ripple_light: [
                finite(tuning.water_ripple_light, WATER_RIPPLE_LIGHT).max(0.0),
                0.0,
                0.0,
                0.0,
            ],
            shadow: [
                params
                    .shadow_lightness
                    .filter(|value| value.is_finite())
                    .unwrap_or_else(|| finite(tuning.shadow_lightness, 1.0))
                    .clamp(0.0, 1.0),
                0.0,
                0.0,
                0.0,
            ],
        }
    }

    #[must_use]
    pub fn to_bytes(
        &self,
        waves: &crate::wave_surface::WaveSurface,
        tuning: &crate::water_tuning::WaterTuning,
        seconds: f32,
    ) -> Vec<u8> {
        let mut out = Vec::with_capacity(WATER_UNIFORM_SIZE as usize);
        for (_, row) in self.rows() {
            for value in row {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        out.extend_from_slice(&waves.to_bytes(tuning, seconds));
        out
    }

    fn rows(&self) -> [(&'static str, [f32; 4]); 19] {
        [
            ("rotations", self.rotations),
            ("bump_offsets", self.bump_offsets),
            ("wave_offsets", self.wave_offsets),
            ("uv_scales", self.uv_scales),
            ("env", self.env),
            ("terms", self.terms),
            ("reflection", self.reflection),
            ("depth_terms", self.depth_terms),
            ("absorption", self.absorption),
            ("body", self.body),
            ("refraction", self.refraction),
            ("surface", self.surface),
            ("planar", self.planar),
            ("specular", self.specular),
            ("highlights", self.highlights),
            ("ripple", self.ripple),
            ("ripple_offsets", self.ripple_offsets),
            ("ripple_light", self.ripple_light),
            ("shadow", self.shadow),
        ]
    }
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests;
