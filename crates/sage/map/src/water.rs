use crate::container::RawChunk;
use crate::reader::Reader;
use crate::{Error, Result};

pub const STANDING_WATER_CHUNK: &str = "StandingWaterAreas";
pub const RIVER_CHUNK: &str = "RiverAreas";
pub const STANDING_WAVE_CHUNK: &str = "StandingWaveAreas";

const STANDING_WATER_VERSION: u16 = 2;

const RIVER_MIN_VERSION: u16 = 1;

const RIVER_MAX_VERSION: u16 = 2;

const WAVE_MIN_VERSION: u16 = 1;

const WAVE_MAX_VERSION: u16 = 2;

const WAVE_PCA_FROM_VERSION: u16 = 2;

const POINT_BYTES: usize = 8;

const LINE_BYTES: usize = 16;

const STANDING_WATER_MIN_BYTES: usize = 29;

const RIVER_MIN_BYTES: usize = 39;

const WAVE_MIN_BYTES: usize = 55;

#[derive(Debug, Clone, PartialEq)]
pub struct StandingWaterArea {
    pub unique_id: u32,
    pub name: String,
    pub layer_name: String,
    pub uv_scroll_speed: f32,
    pub additive_blend: bool,
    pub bump_texture: String,
    pub sky_texture: String,
    pub points: Vec<[f32; 2]>,
    pub water_height: u32,
    pub fx_shader: String,
    pub depth_colors: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RiverLine {
    pub a: [f32; 2],
    pub b: [f32; 2],
}

#[derive(Debug, Clone, PartialEq)]
pub struct RiverArea {
    pub unique_id: u32,
    pub name: String,
    pub layer_name: String,
    pub uv_scroll_speed: f32,
    pub additive_blend: bool,
    pub river_texture: String,
    pub noise_texture: String,
    pub alpha_edge_texture: String,
    pub sparkle_texture: String,
    pub color: [u8; 3],
    pub reserved: u8,
    pub alpha: f32,
    pub water_height: u32,
    pub minimum_water_lod: String,
    pub lines: Vec<RiverLine>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StandingWaveArea {
    pub unique_id: u32,
    pub name: String,
    pub layer_name: String,
    pub uv_scroll_speed: f32,
    pub additive_blend: bool,
    pub points: Vec<[f32; 2]>,
    pub reserved: u32,
    pub final_width: u32,
    pub final_height: u32,
    pub initial_width_fraction: u32,
    pub initial_height_fraction: u32,
    pub initial_velocity: u32,
    pub time_to_fade: u32,
    pub time_to_compress: u32,
    pub time_offset_second_wave: u32,
    pub distance_from_shore: u32,
    pub wave_texture: String,
    pub enable_pca_wave: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WaterAreas<T> {
    pub version: u16,
    pub areas: Vec<T>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Water {
    pub standing: Option<WaterAreas<StandingWaterArea>>,
    pub rivers: Option<WaterAreas<RiverArea>>,
    pub waves: Option<WaterAreas<StandingWaveArea>>,
}

impl Water {
    #[must_use]
    pub fn standing_areas(&self) -> &[StandingWaterArea] {
        match &self.standing {
            Some(chunk) => &chunk.areas,
            None => &[],
        }
    }

    #[must_use]
    pub fn river_areas(&self) -> &[RiverArea] {
        match &self.rivers {
            Some(chunk) => &chunk.areas,
            None => &[],
        }
    }

    #[must_use]
    pub fn wave_areas(&self) -> &[StandingWaveArea] {
        match &self.waves {
            Some(chunk) => &chunk.areas,
            None => &[],
        }
    }
}

fn check_count(r: &Reader<'_>, chunk: &'static str, count: u32, item_size: usize) -> Result<usize> {
    let wanted = usize::try_from(count).unwrap_or(usize::MAX);
    let needed = wanted.saturating_mul(item_size);
    if needed > r.remaining() {
        return Err(Error::CountExceedsPayload {
            chunk,
            offset: r.position(),
            count,
            item_size,
            available: r.remaining(),
        });
    }
    Ok(wanted)
}

fn flag(r: &mut Reader<'_>, chunk: &'static str) -> Result<bool> {
    match r.u8()? {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(Error::Malformed {
            name: chunk,
            reason: "an additiveBlend byte was neither 0 nor 1",
        }),
    }
}

fn read_points(r: &mut Reader<'_>, chunk: &'static str) -> Result<Vec<[f32; 2]>> {
    let declared = r.u32()?;
    let count = check_count(r, chunk, declared, POINT_BYTES)?;
    let mut points = Vec::with_capacity(count);
    for _ in 0..count {
        points.push([r.f32()?, r.f32()?]);
    }
    Ok(points)
}

fn require_exact(r: &Reader<'_>, name: &'static str) -> Result<()> {
    if r.remaining() != 0 {
        return Err(Error::TrailingBytes {
            name,
            offset: r.position(),
            remaining: r.remaining(),
        });
    }
    Ok(())
}

fn read_standing_water_area(r: &mut Reader<'_>) -> Result<StandingWaterArea> {
    Ok(StandingWaterArea {
        unique_id: r.u32()?,
        name: r.ascii_string()?,
        layer_name: r.ascii_string()?,
        uv_scroll_speed: r.f32()?,
        additive_blend: flag(r, STANDING_WATER_CHUNK)?,
        bump_texture: r.ascii_string()?,
        sky_texture: r.ascii_string()?,
        points: read_points(r, STANDING_WATER_CHUNK)?,
        water_height: r.u32()?,
        fx_shader: r.ascii_string()?,
        depth_colors: r.ascii_string()?,
    })
}

fn read_river_area(r: &mut Reader<'_>) -> Result<RiverArea> {
    let unique_id = r.u32()?;
    let name = r.ascii_string()?;
    let layer_name = r.ascii_string()?;
    let uv_scroll_speed = r.f32()?;
    let additive_blend = flag(r, RIVER_CHUNK)?;
    let river_texture = r.ascii_string()?;
    let noise_texture = r.ascii_string()?;
    let alpha_edge_texture = r.ascii_string()?;
    let sparkle_texture = r.ascii_string()?;
    let color = [r.u8()?, r.u8()?, r.u8()?];
    let reserved = r.u8()?;
    let alpha = r.f32()?;
    let water_height = r.u32()?;
    let minimum_water_lod = r.ascii_string()?;
    let declared = r.u32()?;
    let count = check_count(r, RIVER_CHUNK, declared, LINE_BYTES)?;
    let mut lines = Vec::with_capacity(count);
    for _ in 0..count {
        lines.push(RiverLine {
            a: [r.f32()?, r.f32()?],
            b: [r.f32()?, r.f32()?],
        });
    }
    Ok(RiverArea {
        unique_id,
        name,
        layer_name,
        uv_scroll_speed,
        additive_blend,
        river_texture,
        noise_texture,
        alpha_edge_texture,
        sparkle_texture,
        color,
        reserved,
        alpha,
        water_height,
        minimum_water_lod,
        lines,
    })
}

fn read_wave_area(r: &mut Reader<'_>, version: u16) -> Result<StandingWaveArea> {
    Ok(StandingWaveArea {
        unique_id: r.u32()?,
        name: r.ascii_string()?,
        layer_name: r.ascii_string()?,
        uv_scroll_speed: r.f32()?,
        additive_blend: flag(r, STANDING_WAVE_CHUNK)?,
        points: read_points(r, STANDING_WAVE_CHUNK)?,
        reserved: r.u32()?,
        final_width: r.u32()?,
        final_height: r.u32()?,
        initial_width_fraction: r.u32()?,
        initial_height_fraction: r.u32()?,
        initial_velocity: r.u32()?,
        time_to_fade: r.u32()?,
        time_to_compress: r.u32()?,
        time_offset_second_wave: r.u32()?,
        distance_from_shore: r.u32()?,
        wave_texture: r.ascii_string()?,
        enable_pca_wave: if version >= WAVE_PCA_FROM_VERSION {
            Some(r.u32()?)
        } else {
            None
        },
    })
}

fn read_areas<T, F>(
    body: &[u8],
    chunk: &RawChunk,
    name: &'static str,
    min_bytes: usize,
    mut read_one: F,
) -> Result<WaterAreas<T>>
where
    F: FnMut(&mut Reader<'_>) -> Result<T>,
{
    let payload = chunk.payload(body)?;
    let mut r = Reader::new_at(payload, chunk.body);
    let declared = r.u32()?;
    let count = check_count(&r, name, declared, min_bytes)?;
    let mut areas = Vec::with_capacity(count);
    for _ in 0..count {
        areas.push(read_one(&mut r)?);
    }
    require_exact(&r, name)?;
    Ok(WaterAreas {
        version: chunk.version,
        areas,
    })
}

pub(crate) fn read_standing_water(
    body: &[u8],
    chunk: &RawChunk,
) -> Result<WaterAreas<StandingWaterArea>> {
    if chunk.version != STANDING_WATER_VERSION {
        return Err(Error::UnknownWaterVersion {
            chunk: STANDING_WATER_CHUNK,
            version: chunk.version,
        });
    }
    read_areas(
        body,
        chunk,
        STANDING_WATER_CHUNK,
        STANDING_WATER_MIN_BYTES,
        read_standing_water_area,
    )
}

pub(crate) fn read_rivers(body: &[u8], chunk: &RawChunk) -> Result<WaterAreas<RiverArea>> {
    if chunk.version < RIVER_MIN_VERSION || chunk.version > RIVER_MAX_VERSION {
        return Err(Error::UnknownWaterVersion {
            chunk: RIVER_CHUNK,
            version: chunk.version,
        });
    }
    read_areas(body, chunk, RIVER_CHUNK, RIVER_MIN_BYTES, read_river_area)
}

pub(crate) fn read_waves(body: &[u8], chunk: &RawChunk) -> Result<WaterAreas<StandingWaveArea>> {
    if chunk.version < WAVE_MIN_VERSION || chunk.version > WAVE_MAX_VERSION {
        return Err(Error::UnknownWaterVersion {
            chunk: STANDING_WAVE_CHUNK,
            version: chunk.version,
        });
    }
    let version = chunk.version;
    read_areas(body, chunk, STANDING_WAVE_CHUNK, WAVE_MIN_BYTES, |r| {
        read_wave_area(r, version)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::parse_top_level;
    use crate::testutil::MapBuilder;

    fn worked_example_payload() -> Vec<u8> {
        let mut p = 1u32.to_le_bytes().to_vec();
        p.extend_from_slice(&1u32.to_le_bytes());
        p.extend_from_slice(&MapBuilder::ascii("Default Water"));
        p.extend_from_slice(&MapBuilder::ascii(""));
        p.extend_from_slice(&0.06f32.to_le_bytes());
        p.push(0);
        p.extend_from_slice(&MapBuilder::ascii("WaterRippleBump.tga"));
        p.extend_from_slice(&MapBuilder::ascii("SkyEnv.tga"));
        p.extend_from_slice(&4u32.to_le_bytes());
        for (x, y) in [
            (-300.0f32, -300.0f32),
            (2300.0, -300.0),
            (2300.0, 2300.0),
            (-300.0, 2300.0),
        ] {
            p.extend_from_slice(&x.to_le_bytes());
            p.extend_from_slice(&y.to_le_bytes());
        }
        p.extend_from_slice(&7u32.to_le_bytes());
        p.extend_from_slice(&MapBuilder::ascii(""));
        p.extend_from_slice(&MapBuilder::ascii(""));
        p
    }

    fn parse_standing(version: u16, payload: &[u8]) -> Result<WaterAreas<StandingWaterArea>> {
        let mut b = MapBuilder::new();
        b.chunk(STANDING_WATER_CHUNK, version, payload);
        let body = b.build();
        let (_names, chunks) = parse_top_level(&body).expect("valid container");
        let chunk = chunks
            .iter()
            .find(|c| c.name == STANDING_WATER_CHUNK)
            .expect("present");
        read_standing_water(&body, chunk)
    }

    #[test]
    fn the_worked_example_decodes_field_for_field() {
        let payload = worked_example_payload();
        assert_eq!(payload.len(), 4 + 107 - 4, "section 2a's 107-byte chunk");
        let chunk = parse_standing(2, &payload).expect("valid chunk");
        let [area] = <[StandingWaterArea; 1]>::try_from(chunk.areas).expect("one area");
        assert_eq!(area.unique_id, 1);
        assert_eq!(area.name, "Default Water");
        assert_eq!(area.layer_name, "");
        assert_eq!(area.uv_scroll_speed, 0.06);
        assert!(!area.additive_blend);
        assert_eq!(area.bump_texture, "WaterRippleBump.tga");
        assert_eq!(area.sky_texture, "SkyEnv.tga");
        assert_eq!(area.points.len(), 4);
        assert_eq!(area.points[0], [-300.0, -300.0]);
        assert_eq!(area.points[2], [2300.0, 2300.0]);
        assert_eq!(area.water_height, 7);
        assert_eq!(area.fx_shader, "");
        assert_eq!(area.depth_colors, "");
    }

    #[test]
    fn a_count_of_zero_is_the_common_case_not_an_error() {
        let chunk = parse_standing(2, &0u32.to_le_bytes()).expect("valid empty chunk");
        assert!(chunk.areas.is_empty());
        assert_eq!(chunk.version, 2);
    }

    #[test]
    fn an_impossible_count_is_refused_without_allocating() {
        let err = parse_standing(2, &u32::MAX.to_le_bytes()).unwrap_err();
        assert!(
            matches!(
                err,
                Error::CountExceedsPayload {
                    chunk: STANDING_WATER_CHUNK,
                    ..
                }
            ),
            "got {err}"
        );

        let mut payload = worked_example_payload();
        let point_count_at: usize = 4 + 4 + 15 + 2 + 4 + 1 + 21 + 12;
        payload
            .get_mut(point_count_at..point_count_at.saturating_add(4))
            .expect("in range")
            .copy_from_slice(&u32::MAX.to_le_bytes());
        let err = parse_standing(2, &payload).unwrap_err();
        assert!(
            matches!(err, Error::CountExceedsPayload { .. }),
            "got {err}"
        );
    }

    #[test]
    fn truncation_and_trailing_bytes_are_errors_not_panics() {
        let full = worked_example_payload();
        for cut in 0..full.len() {
            let err = parse_standing(2, full.get(..cut).unwrap_or_default()).unwrap_err();
            assert!(
                matches!(
                    err,
                    Error::UnexpectedEof { .. } | Error::CountExceedsPayload { .. }
                ),
                "cut {cut} gave {err}"
            );
        }
        let mut over = full.clone();
        over.push(0);
        let err = parse_standing(2, &over).unwrap_err();
        assert!(matches!(err, Error::TrailingBytes { .. }), "got {err}");
    }

    #[test]
    fn bit_corruption_sweep_never_panics() {
        let full = worked_example_payload();
        for i in 0..full.len() {
            for bit in 0..8u8 {
                let mut corrupt = full.clone();
                if let Some(byte) = corrupt.get_mut(i) {
                    *byte ^= 1 << bit;
                }
                let _ = parse_standing(2, &corrupt);
            }
        }
    }

    #[test]
    fn an_unknown_standing_water_version_is_refused() {
        let err = parse_standing(1, &worked_example_payload()).unwrap_err();
        assert!(matches!(
            err,
            Error::UnknownWaterVersion {
                chunk: STANDING_WATER_CHUNK,
                version: 1
            }
        ));
    }

    #[test]
    fn the_wave_versions_differ_by_exactly_one_trailing_word() {
        fn wave_payload(with_pca: bool) -> Vec<u8> {
            let mut p = 1u32.to_le_bytes().to_vec();
            p.extend_from_slice(&9u32.to_le_bytes());
            p.extend_from_slice(&MapBuilder::ascii("New Wave Area"));
            p.extend_from_slice(&MapBuilder::ascii(""));
            p.extend_from_slice(&0.06f32.to_le_bytes());
            p.push(0);
            p.extend_from_slice(&2u32.to_le_bytes());
            for value in [1.0f32, 2.0, 3.0, 4.0] {
                p.extend_from_slice(&value.to_le_bytes());
            }
            for value in [0u32, 28, 18, 50, 100, 5, 2000, 1000, 3000, 40] {
                p.extend_from_slice(&value.to_le_bytes());
            }
            p.extend_from_slice(&MapBuilder::ascii("wave256.tga"));
            if with_pca {
                p.extend_from_slice(&1u32.to_le_bytes());
            }
            p
        }

        let parse = |version: u16, payload: &[u8]| {
            let mut b = MapBuilder::new();
            b.chunk(STANDING_WAVE_CHUNK, version, payload);
            let body = b.build();
            let (_names, chunks) = parse_top_level(&body).expect("valid container");
            let chunk = chunks
                .iter()
                .find(|c| c.name == STANDING_WAVE_CHUNK)
                .expect("present");
            read_waves(&body, chunk)
        };

        let v1 = parse(1, &wave_payload(false)).expect("valid version 1");
        assert_eq!(v1.areas.first().map(|a| a.enable_pca_wave), Some(None));
        let v2 = parse(2, &wave_payload(true)).expect("valid version 2");
        assert_eq!(v2.areas.first().map(|a| a.enable_pca_wave), Some(Some(1)));
        assert_eq!(v2.areas.first().map(|a| a.points.len()), Some(2));

        assert!(matches!(
            parse(2, &wave_payload(false)).unwrap_err(),
            Error::UnexpectedEof { .. }
        ));
        assert!(matches!(
            parse(1, &wave_payload(true)).unwrap_err(),
            Error::TrailingBytes { .. }
        ));
    }
}
