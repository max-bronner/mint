use crate::container::RawChunk;
use crate::reader::Reader;
use crate::{Error, Result};

const LIGHTS_PER_CONFIG: usize = 9;

const CONFIG_COUNT: usize = 4;

const UNNAMED_LEN: usize = 44;

const MIN_VERSION: u16 = 7;

const MAX_VERSION: u16 = 8;

const NO_CLOUD_FROM_VERSION: u16 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimeOfDay {
    Morning,
    Afternoon,
    Evening,
    Night,
}

impl TimeOfDay {
    pub const ALL: [Self; CONFIG_COUNT] =
        [Self::Morning, Self::Afternoon, Self::Evening, Self::Night];

    #[must_use]
    pub fn from_code(code: u32) -> Option<Self> {
        match code {
            1 => Some(Self::Morning),
            2 => Some(Self::Afternoon),
            3 => Some(Self::Evening),
            4 => Some(Self::Night),
            _ => None,
        }
    }

    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Self::Morning => 0,
            Self::Afternoon => 1,
            Self::Evening => 2,
            Self::Night => 3,
        }
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Morning => "morning",
            Self::Afternoon => "afternoon",
            Self::Evening => "evening",
            Self::Night => "night",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapLight {
    pub ambient: [f32; 3],
    pub color: [f32; 3],
    pub direction: [f32; 3],
}

impl MapLight {
    const ZERO: Self = Self {
        ambient: [0.0; 3],
        color: [0.0; 3],
        direction: [0.0; 3],
    };
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightingConfig {
    pub terrain: [MapLight; 3],
    pub object: [MapLight; 3],
    pub infantry: [MapLight; 3],
    pub stored: [MapLight; LIGHTS_PER_CONFIG],
}

impl LightingConfig {
    const ZERO: Self = Self {
        terrain: [MapLight::ZERO; 3],
        object: [MapLight::ZERO; 3],
        infantry: [MapLight::ZERO; 3],
        stored: [MapLight::ZERO; LIGHTS_PER_CONFIG],
    };
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnnamedBlock {
    pub raw: [u8; UNNAMED_LEN],
    pub bloom_enabled: u32,
    pub colors: [[f32; 3]; 3],
    pub packed_color: u32,
}

#[derive(Debug, Clone)]
pub struct GlobalLighting {
    pub version: u16,
    pub time_of_day: Option<TimeOfDay>,
    pub time_of_day_code: u32,
    pub configs: [LightingConfig; CONFIG_COUNT],
    pub overbright: f32,
    pub unnamed: UnnamedBlock,
    pub no_cloud_factor: Option<[f32; 3]>,
}

impl GlobalLighting {
    #[must_use]
    pub fn config(&self, time: TimeOfDay) -> LightingConfig {
        let [morning, afternoon, evening, night] = self.configs;
        match time {
            TimeOfDay::Morning => morning,
            TimeOfDay::Afternoon => afternoon,
            TimeOfDay::Evening => evening,
            TimeOfDay::Night => night,
        }
    }
}

fn read_light(r: &mut Reader<'_>) -> Result<MapLight> {
    let mut values = [0.0f32; 9];
    for value in &mut values {
        *value = r.f32()?;
    }
    let [ar, ag, ab, cr, cg, cb, dx, dy, dz] = values;
    Ok(MapLight {
        ambient: [ar, ag, ab],
        color: [cr, cg, cb],
        direction: [dx, dy, dz],
    })
}

const TERRAIN_SLOTS: [usize; 3] = [0, 4, 5];
const OBJECT_SLOTS: [usize; 3] = [1, 2, 3];
const INFANTRY_SLOTS: [usize; 3] = [6, 7, 8];

fn read_config(r: &mut Reader<'_>) -> Result<LightingConfig> {
    let mut lights = [MapLight::ZERO; LIGHTS_PER_CONFIG];
    for light in &mut lights {
        *light = read_light(r)?;
    }
    let pick = |slots: [usize; 3]| -> [MapLight; 3] {
        slots.map(|slot| lights.get(slot).copied().unwrap_or(MapLight::ZERO))
    };
    Ok(LightingConfig {
        terrain: pick(TERRAIN_SLOTS),
        object: pick(OBJECT_SLOTS),
        infantry: pick(INFANTRY_SLOTS),
        stored: lights,
    })
}

pub(crate) fn read_global_lighting(body: &[u8], chunk: &RawChunk) -> Result<GlobalLighting> {
    if chunk.version < MIN_VERSION || chunk.version > MAX_VERSION {
        return Err(Error::UnknownLightingVersion {
            version: chunk.version,
        });
    }
    let payload = chunk.payload(body)?;
    let mut r = Reader::new_at(payload, chunk.body);

    let time_of_day_code = r.u32()?;
    let mut configs = [LightingConfig::ZERO; CONFIG_COUNT];
    for config in &mut configs {
        *config = read_config(&mut r)?;
    }
    let overbright = f32::from_bits(r.u32()?);
    let raw =
        <[u8; UNNAMED_LEN]>::try_from(r.take(UNNAMED_LEN)?).map_err(|_| Error::Malformed {
            name: "GlobalLighting",
            reason: "the unnamed block was short",
        })?;
    let mut u = Reader::new(&raw);
    let flag = u.u32()?;
    let mut colors = [[0.0f32; 3]; 3];
    for color in &mut colors {
        for component in color.iter_mut() {
            *component = u.f32()?;
        }
    }
    let packed_color = u.u32()?;
    let unnamed = UnnamedBlock {
        raw,
        bloom_enabled: flag,
        colors,
        packed_color,
    };
    let no_cloud_factor = if chunk.version >= NO_CLOUD_FROM_VERSION {
        Some([r.f32()?, r.f32()?, r.f32()?])
    } else {
        None
    };

    if r.remaining() != 0 {
        return Err(Error::TrailingBytes {
            name: "GlobalLighting",
            offset: r.position(),
            remaining: r.remaining(),
        });
    }

    Ok(GlobalLighting {
        version: chunk.version,
        time_of_day: TimeOfDay::from_code(time_of_day_code),
        time_of_day_code,
        configs,
        overbright,
        unnamed,
        no_cloud_factor,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::parse_top_level;
    use crate::testutil::MapBuilder;

    use crate::testutil::global_lighting_payload as lighting_payload;

    fn parse(version: u16, payload: &[u8]) -> Result<GlobalLighting> {
        let mut b = MapBuilder::new();
        b.chunk("GlobalLighting", version, payload);
        let body = b.build();
        let (_names, chunks) = parse_top_level(&body).expect("valid container");
        let chunk = chunks
            .iter()
            .find(|c| c.name == "GlobalLighting")
            .expect("present");
        read_global_lighting(&body, chunk)
    }

    #[test]
    fn the_three_sets_take_the_slots_ea_writes_them_to() {
        let lighting = parse(8, &lighting_payload(8, 2)).expect("valid chunk");
        let evening = lighting.config(TimeOfDay::Evening);
        assert_eq!(evening.terrain[0].ambient, [200.0, 201.0, 202.0]);
        assert_eq!(evening.terrain[0].color, [203.0, 204.0, 205.0]);
        assert_eq!(evening.terrain[0].direction, [206.0, 207.0, 208.0]);
        assert_eq!(evening.terrain[1].ambient[0], 240.0);
        assert_eq!(evening.terrain[2].ambient[0], 250.0);
        assert_eq!(evening.object[0].ambient[0], 210.0);
        assert_eq!(evening.object[2].ambient[0], 230.0);
        assert_eq!(evening.infantry[0].ambient[0], 260.0);
        assert_eq!(evening.infantry[2].ambient[0], 280.0);
    }

    #[test]
    fn version_decides_whether_the_no_cloud_factor_is_present() {
        let v7 = parse(7, &lighting_payload(7, 1)).expect("valid version 7");
        assert_eq!(v7.no_cloud_factor, None);
        assert_eq!(v7.time_of_day, Some(TimeOfDay::Morning));

        let v8 = parse(8, &lighting_payload(8, 4)).expect("valid version 8");
        assert_eq!(v8.no_cloud_factor, Some([1.0, 2.0, 3.0]));
        assert_eq!(v8.time_of_day, Some(TimeOfDay::Night));
        assert_eq!(v8.overbright.to_bits(), 0x8080_4020);
        assert_eq!(v8.unnamed.raw, [0xAB; UNNAMED_LEN]);
        assert_eq!(v8.unnamed.bloom_enabled, 0xABAB_ABAB);
        assert_eq!(v8.unnamed.packed_color, 0xABAB_ABAB);
        assert_eq!(v8.unnamed.colors[2][2], f32::from_bits(0xABAB_ABAB));

        let err = parse(7, &lighting_payload(8, 1)).unwrap_err();
        assert!(matches!(err, Error::TrailingBytes { .. }));
    }

    #[test]
    fn truncation_is_an_error_not_a_panic() {
        let full = lighting_payload(8, 1);
        for cut in 0..full.len() {
            let err = parse(8, &full[..cut]).unwrap_err();
            assert!(
                matches!(err, Error::UnexpectedEof { .. }),
                "cut {cut} gave {err}"
            );
        }
    }

    #[test]
    fn an_out_of_range_time_of_day_keeps_the_configurations() {
        let lighting = parse(8, &lighting_payload(8, 9)).expect("still parses");
        assert_eq!(lighting.time_of_day, None);
        assert_eq!(lighting.time_of_day_code, 9);
        assert_eq!(lighting.configs.len(), CONFIG_COUNT);
    }

    #[test]
    fn an_unknown_version_is_refused() {
        let err = parse(9, &lighting_payload(8, 1)).unwrap_err();
        assert!(matches!(err, Error::UnknownLightingVersion { version: 9 }));
    }
}
