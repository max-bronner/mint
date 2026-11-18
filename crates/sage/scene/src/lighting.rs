use mint_render::{
    Camera, DirectionalLight, DistanceFog, Instance, LightSet, Scene, ShadowSample, Vec3,
};
use mint_sage_ini::{CloudDarkeningSetting, HardwareFog};
use mint_sage_map::{GlobalLighting, TimeOfDay};
use mint_vfs::Vfs;

#[derive(Debug, Clone, Copy)]
pub struct TerrainRig {
    pub lights: [DirectionalLight; mint_render::MAX_DIRECTIONAL_LIGHTS],
    pub effect_lights: [DirectionalLight; mint_render::MAX_DIRECTIONAL_LIGHTS],
    pub ambient: [f32; 3],
    pub ceiling: Option<f32>,
    pub models: [Option<LightSet>; mint_render::LIGHT_SETS as usize],
    pub modulate2x: bool,
}

#[must_use]
pub fn terrain_rig(lighting: &GlobalLighting, time: TimeOfDay) -> TerrainRig {
    let config = lighting.config(time);
    let gain = lighting.overbright;
    let mut ambient = [0.0f32; 3];
    for light in config.terrain {
        for (total, part) in ambient.iter_mut().zip(light.ambient) {
            *total += part * gain;
        }
    }
    let mut lights = [DirectionalLight::default(); mint_render::MAX_DIRECTIONAL_LIGHTS];
    let mut effect_lights = lights;
    for (index, ((out, effect), light)) in lights
        .iter_mut()
        .zip(effect_lights.iter_mut())
        .zip(config.terrain)
        .enumerate()
    {
        let direction = Vec3::new(light.direction[0], light.direction[1], light.direction[2]);
        let scale = if index == 0 { 1.0 } else { gain };
        *out = DirectionalLight {
            direction,
            color: light.color.map(|c| c * scale),
        };
        *effect = DirectionalLight {
            direction,
            color: light.color.map(|c| c * gain),
        };
    }
    TerrainRig {
        lights,
        effect_lights,
        ambient,
        ceiling: None,
        models: model_light_sets(lighting, time),
        modulate2x: lighting.overbright > 1.0,
    }
}

#[must_use]
pub fn model_light_sets(
    lighting: &GlobalLighting,
    time: TimeOfDay,
) -> [Option<LightSet>; mint_render::LIGHT_SETS as usize] {
    let config = lighting.config(time);
    let set = |rig: &[mint_sage_map::MapLight; 3], gain: f32| {
        let mut ambient = [0.0f32; 3];
        let mut lights = [DirectionalLight::default(); mint_render::MAX_DIRECTIONAL_LIGHTS];
        for (out, light) in lights.iter_mut().zip(rig) {
            for (total, part) in ambient.iter_mut().zip(light.ambient) {
                *total += part * gain;
            }
            *out = DirectionalLight {
                direction: Vec3::new(light.direction[0], light.direction[1], light.direction[2]),
                color: light.color.map(|c| c * gain),
            };
        }
        Some(LightSet {
            lights,
            ambient_color: ambient,
            cookie: true,
        })
    };
    let gain = lighting.overbright;
    [
        set(&config.object, gain),
        set(&config.infantry, gain),
        set(&config.object, 1.0),
        set(&config.infantry, 1.0),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SunConvention {
    #[default]
    Stored,
    MirroredAzimuth,
}

impl SunConvention {
    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Stored => Self::MirroredAzimuth,
            Self::MirroredAzimuth => Self::Stored,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Stored => "stored",
            Self::MirroredAzimuth => "mirrored-azimuth",
        }
    }
}

impl TerrainRig {
    pub fn mirror_azimuth(&mut self) {
        let mirror = |light: &mut DirectionalLight| {
            light.direction = Vec3::new(-light.direction.x, -light.direction.y, light.direction.z);
        };
        self.lights.iter_mut().for_each(mirror);
        self.effect_lights.iter_mut().for_each(mirror);
        for set in self.models.iter_mut().flatten() {
            set.lights.iter_mut().for_each(mirror);
        }
    }
}

#[must_use]
pub fn distance_fog(fog: &HardwareFog) -> Option<DistanceFog> {
    let fog = fog.active()?;
    let unit = |c: u8| f32::from(c) / 255.0;
    Some(DistanceFog {
        color: [unit(fog.color.r), unit(fog.color.g), unit(fog.color.b)],
        start: fog.start,
        end: fog.end,
    })
}

#[must_use]
pub fn default_time_of_day(lighting: &GlobalLighting) -> TimeOfDay {
    lighting.time_of_day.unwrap_or(TimeOfDay::Afternoon)
}

#[must_use]
pub fn water_set_name(time: TimeOfDay) -> &'static str {
    match time {
        TimeOfDay::Morning => "MORNING",
        TimeOfDay::Afternoon => "AFTERNOON",
        TimeOfDay::Evening => "EVENING",
        TimeOfDay::Night => "NIGHT",
    }
}

#[must_use]
pub fn terrain_scene<'a>(
    camera: Camera,
    instances: &'a [Instance],
    rig: &TerrainRig,
    shadow: Option<ShadowSample>,
) -> Scene<'a> {
    let base = Scene::new(camera);
    Scene {
        lights: rig.lights,
        ambient_color: rig.ambient,
        lit_ceiling: rig.ceiling,
        modulate2x_scene: rig.modulate2x,
        shadow,
        light_sets: rig.models,
        ..base
    }
    .with_instances(instances)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ModelLights {
    #[default]
    Retail,
    TerrainRig,
}

impl ModelLights {
    pub const ALL: [Self; 2] = [Self::Retail, Self::TerrainRig];

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Retail => "retail (object/infantry sets x overbright, under the cloud)",
            Self::TerrainRig => "terrain (the ground's rig, no cloud)",
        }
    }

    pub fn apply(self, scene: &mut Scene<'_>, cloud: Option<mint_render::LightCookie>) {
        match self {
            Self::Retail => scene.light_cookie = cloud,
            Self::TerrainRig => {
                scene.light_sets = [None; mint_render::LIGHT_SETS as usize];
                scene.light_cookie = None;
            }
        }
    }
}

pub const ENVIRONMENT_INI: &str = "data/ini/environment.ini";

#[must_use]
pub fn load_cloud_darkening(vfs: &Vfs, map_path: &str) -> (CloudDarkeningSetting, String) {
    let folder = map_path
        .rsplit_once(['/', '\\'])
        .map_or("", |(folder, _)| folder);
    let map_ini = format!("{folder}/map.ini");
    let mut documents = Vec::new();
    let mut read = Vec::new();
    for path in [ENVIRONMENT_INI, map_ini.as_str()] {
        let Ok(bytes) = vfs.read(path) else {
            continue;
        };
        let text = String::from_utf8_lossy(&bytes);
        let table = mint_sage_ini::BlockTable::corpus_path();
        match mint_sage_ini::Document::parse(path, &text, &table) {
            Ok(document) => {
                let blocks = document
                    .roots_named(mint_sage_ini::CLOUD_EFFECT_KEYWORD)
                    .count();
                documents.push(document);
                read.push(format!("{path} ({blocks} CloudEffect block(s))"));
            }
            Err(_) => read.push(format!("{path} (did not parse)")),
        }
    }
    let setting = mint_sage_ini::cloud_darkening(&documents);
    let line = format!(
        "cloud darkening: factor {:?}, rain factor {:?} [from {}]",
        setting.factor,
        setting.factor_rain,
        if read.is_empty() {
            "no environment file".to_owned()
        } else {
            read.join(", ")
        }
    );
    (setting, line)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CloudDarkeningLook {
    Cloudy,
    Rainy,
    #[default]
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum CloudPhase {
    #[default]
    Idle,
    Darkening,
    Held,
    Lightening,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CloudDarkening {
    cached: CloudDarkeningLook,
    phase: CloudPhase,
    level: u8,
    carry: f32,
    last_seconds: Option<f32>,
}

impl CloudDarkening {
    #[must_use]
    pub fn new() -> Self {
        Self {
            cached: CloudDarkeningLook::default(),
            phase: CloudPhase::Idle,
            level: 255,
            carry: 0.0,
            last_seconds: None,
        }
    }

    #[must_use]
    pub fn active(&self) -> bool {
        self.phase != CloudPhase::Idle
    }

    #[must_use]
    pub fn level(&self) -> u8 {
        self.level
    }

    pub fn advance(&mut self, look: CloudDarkeningLook, seconds: f32) {
        let dt = self
            .last_seconds
            .map_or(0.0, |last| (seconds - last).max(0.0));
        self.last_seconds = Some(seconds);
        let steps = crate::client_clock::client_frames_elapsed(&mut self.carry, dt);
        for _ in 0..steps {
            self.step(look);
        }
    }

    fn step(&mut self, look: CloudDarkeningLook) {
        if look == CloudDarkeningLook::Cloudy && self.cached != CloudDarkeningLook::Cloudy {
            self.phase = CloudPhase::Darkening;
            self.cached = CloudDarkeningLook::Cloudy;
        } else if look != CloudDarkeningLook::Cloudy && self.cached == CloudDarkeningLook::Cloudy {
            self.phase = CloudPhase::Lightening;
            self.cached = look;
        }
        match self.phase {
            CloudPhase::Darkening => {
                self.level = self.level.saturating_sub(1);
                if self.level == 0 {
                    self.phase = CloudPhase::Held;
                }
            }
            CloudPhase::Lightening => {
                self.level = self.level.saturating_add(1);
                if self.level == 255 {
                    self.phase = CloudPhase::Idle;
                }
            }
            CloudPhase::Idle | CloudPhase::Held => {}
        }
    }

    #[must_use]
    pub fn multiplier(&self, setting: &CloudDarkeningSetting) -> [f32; 3] {
        let scale = match self.phase {
            CloudPhase::Idle => return [1.0, 1.0, 1.0],
            CloudPhase::Held => 1.0,
            CloudPhase::Darkening | CloudPhase::Lightening => 1.0 - f32::from(self.level) / 255.0,
        };
        let base = if self.cached == CloudDarkeningLook::Rainy {
            setting.factor_rain
        } else {
            setting.factor
        }
        .to_unit();
        [
            1.0 - base[0] * scale,
            1.0 - base[1] * scale,
            1.0 - base[2] * scale,
        ]
    }

    #[must_use]
    pub fn apply(&self, rig: &TerrainRig, setting: &CloudDarkeningSetting) -> TerrainRig {
        let mut darkened = *rig;
        if !self.active() {
            return darkened;
        }
        let scale = self.multiplier(setting);
        let darken = |color: &mut [f32; 3]| {
            for (channel, factor) in color.iter_mut().zip(scale) {
                *channel *= factor;
            }
        };
        for light in darkened
            .lights
            .iter_mut()
            .chain(&mut darkened.effect_lights)
        {
            darken(&mut light.color);
        }
        for set in darkened.models.iter_mut().flatten() {
            for light in &mut set.lights {
                darken(&mut light.color);
            }
            darken(&mut set.ambient_color);
        }
        darkened
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::float_cmp)]
mod cloud_darkening_tests {
    use super::*;
    use crate::client_clock::CLIENT_FRAME_SECONDS;
    use mint_sage_ini::Rgb8;

    fn primed(look: CloudDarkeningLook) -> (CloudDarkening, f32) {
        let mut cloud = CloudDarkening::new();
        cloud.advance(look, 0.0);
        (cloud, 0.0)
    }

    fn step(cloud: &mut CloudDarkening, look: CloudDarkeningLook, seconds: &mut f32) {
        *seconds += CLIENT_FRAME_SECONDS;
        cloud.advance(look, *seconds);
    }

    #[test]
    fn darkening_then_lightening_steps_255_client_frames_each_way() {
        let (mut cloud, mut seconds) = primed(CloudDarkeningLook::Other);
        step(&mut cloud, CloudDarkeningLook::Cloudy, &mut seconds);
        assert_eq!(cloud.level(), 254);
        assert!(cloud.active());
        for _ in 1..255 {
            step(&mut cloud, CloudDarkeningLook::Cloudy, &mut seconds);
        }
        assert_eq!(cloud.level(), 0, "255 frames in, fully dark");
        step(&mut cloud, CloudDarkeningLook::Cloudy, &mut seconds);
        assert_eq!(
            cloud.level(),
            0,
            "the 256th frame holds rather than wrapping"
        );
        for _ in 0..10 {
            step(&mut cloud, CloudDarkeningLook::Other, &mut seconds);
        }
        assert_eq!(cloud.level(), 10, "ten frames into clearing");
    }

    #[test]
    fn the_multiplier_scales_by_the_remaining_level_over_255() {
        let setting = CloudDarkeningSetting {
            factor: Rgb8 { r: 255, g: 0, b: 0 },
            factor_rain: Rgb8::default(),
        };
        let (mut cloud, mut seconds) = primed(CloudDarkeningLook::Other);
        step(&mut cloud, CloudDarkeningLook::Cloudy, &mut seconds);
        let multiplier = cloud.multiplier(&setting);
        let expected = 1.0 - (1.0 - 254.0 / 255.0);
        assert!((multiplier[0] - expected).abs() < 1.0e-5);
        assert_eq!(multiplier[1], 1.0);
        assert_eq!(multiplier[2], 1.0);
    }

    #[test]
    fn apply_darkens_model_ambient_but_leaves_terrain_ambient_alone() {
        let setting = CloudDarkeningSetting {
            factor: Rgb8 {
                r: 255,
                g: 255,
                b: 255,
            },
            factor_rain: Rgb8::default(),
        };
        let mut rig = TerrainRig {
            lights: [DirectionalLight {
                direction: Vec3::new(0.0, 0.0, -1.0),
                color: [1.0, 1.0, 1.0],
            }; mint_render::MAX_DIRECTIONAL_LIGHTS],
            effect_lights: [DirectionalLight::default(); mint_render::MAX_DIRECTIONAL_LIGHTS],
            ambient: [1.0, 1.0, 1.0],
            ceiling: None,
            modulate2x: false,
            models: [
                Some(LightSet {
                    lights: [DirectionalLight {
                        direction: Vec3::new(0.0, 0.0, -1.0),
                        color: [1.0, 1.0, 1.0],
                    }; mint_render::MAX_DIRECTIONAL_LIGHTS],
                    ambient_color: [1.0, 1.0, 1.0],
                    cookie: true,
                }),
                None,
                None,
                None,
            ],
        };
        let (mut cloud, mut seconds) = primed(CloudDarkeningLook::Other);
        for _ in 0..255 {
            step(&mut cloud, CloudDarkeningLook::Cloudy, &mut seconds);
        }
        rig = cloud.apply(&rig, &setting);
        assert_eq!(rig.ambient, [1.0, 1.0, 1.0], "terrain ambient untouched");
        assert_eq!(rig.lights[0].color, [0.0, 0.0, 0.0]);
        let model = rig.models[0].unwrap();
        assert_eq!(model.lights[0].color, [0.0, 0.0, 0.0]);
        assert_eq!(
            model.ambient_color,
            [0.0, 0.0, 0.0],
            "model ambient scaled too"
        );
    }
}
