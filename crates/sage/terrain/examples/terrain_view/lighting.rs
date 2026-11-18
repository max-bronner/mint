use super::*;

#[derive(Debug, Clone, Copy)]
pub(crate) struct TerrainRig {
    pub(crate) lights: [DirectionalLight; mint_render::MAX_DIRECTIONAL_LIGHTS],
    pub(crate) ambient: [f32; 3],
    pub(crate) ceiling: Option<f32>,
}

pub(crate) fn placeholder_rig() -> TerrainRig {
    TerrainRig {
        lights: [
            DirectionalLight {
                direction: Vec3::new(-0.4, -0.6, -0.7),
                color: [0.75, 0.72, 0.68],
            },
            DirectionalLight::default(),
            DirectionalLight::default(),
        ],
        ambient: [0.28, 0.28, 0.32],
        ceiling: Some(2.0),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum TerrainOverbright {
    All,
    #[default]
    Accents,
}

impl TerrainOverbright {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Accents => "accents",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "all" => Some(Self::All),
            "accents" => Some(Self::Accents),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum TerrainCeiling {
    Clamped,
    #[default]
    None,
}

impl TerrainCeiling {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Clamped => "clamped",
            Self::None => "none",
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        match text.trim().to_ascii_lowercase().as_str() {
            "clamped" => Some(Self::Clamped),
            "none" => Some(Self::None),
            _ => None,
        }
    }
}

pub(crate) const ALT_TERRAIN_SLOTS: [usize; 3] = [6, 7, 8];

pub(crate) fn map_rigs(
    lighting: &GlobalLighting,
    alt: bool,
    overbright: TerrainOverbright,
    ceiling: TerrainCeiling,
) -> [TerrainRig; 4] {
    TimeOfDay::ALL.map(|time| {
        let config = lighting.config(time);
        let set = if alt {
            ALT_TERRAIN_SLOTS.map(|slot| {
                config
                    .stored
                    .get(slot)
                    .copied()
                    .unwrap_or(config.terrain[0])
            })
        } else {
            config.terrain
        };
        let gain = lighting.overbright;
        let sun_gain = match overbright {
            TerrainOverbright::All => gain,
            TerrainOverbright::Accents => 1.0,
        };
        let mut ambient = [0.0f32; 3];
        for light in set {
            for (total, part) in ambient.iter_mut().zip(light.ambient) {
                *total += part * gain;
            }
        }
        let mut lights = [DirectionalLight::default(); mint_render::MAX_DIRECTIONAL_LIGHTS];
        for (index, (out, light)) in lights.iter_mut().zip(set).enumerate() {
            let scale = if index == 0 { sun_gain } else { gain };
            *out = DirectionalLight {
                direction: Vec3::new(light.direction[0], light.direction[1], light.direction[2]),
                color: light.color.map(|c| c * scale),
            };
        }
        TerrainRig {
            lights,
            ambient,
            ceiling: match ceiling {
                TerrainCeiling::Clamped => Some(gain),
                TerrainCeiling::None => None,
            },
        }
    })
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RoadAction {
    Toggle,
    NextLift,
    NextHeight,
    NextTessellation,
    NextChainOrder,
}

pub(crate) fn map_cliff_note(terrain: &Terrain, blend_tile: &BlendTileData) -> String {
    match terrain.map_cliff_class {
        Some(class) => format!(
            "class {class} {}",
            blend_tile
                .texture_classes
                .get(class)
                .map_or("(unnamed)", |entry| entry.name.as_str())
        ),
        None => {
            "none - this map's cliff record list is empty, or record 0 names no class".to_owned()
        }
    }
}

pub(crate) fn current_rig(
    rigs: &[[TerrainRig; 4]; 2],
    time: TimeOfDay,
    map_light: bool,
    alt_set: bool,
) -> TerrainRig {
    if !map_light {
        return placeholder_rig();
    }
    let [shipped, alternative] = rigs;
    rig_for(if alt_set { alternative } else { shipped }, time)
}

pub(crate) fn rig_for(rigs: &[TerrainRig; 4], time: TimeOfDay) -> TerrainRig {
    let [morning, afternoon, evening, night] = *rigs;
    match time {
        TimeOfDay::Morning => morning,
        TimeOfDay::Afternoon => afternoon,
        TimeOfDay::Evening => evening,
        TimeOfDay::Night => night,
    }
}

pub(crate) fn next_time(time: TimeOfDay) -> TimeOfDay {
    match time {
        TimeOfDay::Morning => TimeOfDay::Afternoon,
        TimeOfDay::Afternoon => TimeOfDay::Evening,
        TimeOfDay::Evening => TimeOfDay::Night,
        TimeOfDay::Night => TimeOfDay::Morning,
    }
}

pub(crate) fn peak_lit(rig: &TerrainRig) -> [f32; 3] {
    let mut peak = rig.ambient;
    let toward: Vec<Vec3> = rig
        .lights
        .iter()
        .map(|light| {
            let d = light.direction;
            if d.length_squared() > 0.0 {
                -d.normalize()
            } else {
                Vec3::ZERO
            }
        })
        .collect();
    for channel in 0..3 {
        let mut best = rig.ambient.get(channel).copied().unwrap_or(0.0);
        for mask in 1u8..8 {
            let mut direction = Vec3::ZERO;
            for (index, unit) in toward.iter().enumerate() {
                if mask & (1 << index) != 0 {
                    let colour = rig
                        .lights
                        .get(index)
                        .and_then(|light| light.color.get(channel).copied())
                        .unwrap_or(0.0);
                    direction = direction + *unit * colour;
                }
            }
            if direction.length_squared() <= 0.0 {
                continue;
            }
            let n = direction.normalize();
            let mut total = rig.ambient.get(channel).copied().unwrap_or(0.0);
            for (index, unit) in toward.iter().enumerate() {
                let colour = rig
                    .lights
                    .get(index)
                    .and_then(|light| light.color.get(channel).copied())
                    .unwrap_or(0.0);
                total += colour * n.dot(*unit).max(0.0);
            }
            best = best.max(total);
        }
        if let Some(slot) = peak.get_mut(channel) {
            *slot = best;
        }
    }
    peak
}

#[allow(clippy::fn_params_excessive_bools)]
pub(crate) fn report_lighting(
    lighting: &GlobalLighting,
    rigs: &[[TerrainRig; 4]; 2],
    time: TimeOfDay,
    map_light: bool,
    from_chunk: bool,
    alt_set: bool,
    overbright: TerrainOverbright,
    ceiling: TerrainCeiling,
) {
    println!();
    println!(
        "GlobalLighting version {}, overbright {:.1}x, no-cloud factor {:?}",
        lighting.version, lighting.overbright, lighting.no_cloud_factor
    );
    println!(
        "  unnamed block: flag {}, colours {:?}, packed {:#010x}",
        lighting.unnamed.bloom_enabled, lighting.unnamed.colors, lighting.unnamed.packed_color
    );
    let chosen = {
        let [shipped, alternative] = rigs;
        if alt_set {
            alternative
        } else {
            shipped
        }
    };
    for candidate in TimeOfDay::ALL {
        let rig = rig_for(chosen, candidate);
        let sun = rig.lights.first().copied().unwrap_or_default();
        let peak = peak_lit(&rig);
        println!(
            "  {:<9} ambient {:.3},{:.3},{:.3}  sun {:.3},{:.3},{:.3} travelling {:.2},{:.2},{:.2}  peak lit {:.3},{:.3},{:.3}{}{}",
            candidate.name(),
            rig.ambient[0],
            rig.ambient[1],
            rig.ambient[2],
            sun.color[0],
            sun.color[1],
            sun.color[2],
            sun.direction.x,
            sun.direction.y,
            sun.direction.z,
            peak[0],
            peak[1],
            peak[2],
            match rig.ceiling {
                Some(limit) if peak.iter().any(|value| *value > limit) =>
                    format!(" CLIPS at {limit:.2}"),
                _ => String::new(),
            },
            if candidate == time { "  <-" } else { "" },
        );
    }
    if map_light {
        let source = if from_chunk {
            "the map's own"
        } else {
            "--time"
        };
        println!("time of day = {} ({source})", time.name());
        println!(
            "terrain light set = {}",
            if alt_set {
                "stored slots 6,7,8 (--alt-light-set)"
            } else {
                "stored slots 0,4,5 (EA's own order)"
            }
        );
        let rig = rig_for(chosen, time);
        for (index, light) in rig.lights.iter().enumerate() {
            println!(
                "  light {index} {:<8} colour {:.3},{:.3},{:.3} travelling {:.2},{:.2},{:.2}",
                if index == 0 { "(sun)" } else { "(accent)" },
                light.color[0],
                light.color[1],
                light.color[2],
                light.direction.x,
                light.direction.y,
                light.direction.z,
            );
        }
    } else {
        println!("lighting = the placeholder rig (--placeholder-light)");
    }
    println!(
        "terrain overbright = {} (mode.terrain_overbright, --terrain-overbright),          lit ceiling = {} (mode.terrain_ceiling, --terrain-ceiling)",
        overbright.label(),
        ceiling.label(),
    );
}
