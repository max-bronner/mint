use crate::water_tuning::WaterTuning;
use crate::wave_shader::WaveLook;
use crate::waves::WaveField;

pub const WAVE_SURFACE_ROWS: u32 = 4;

pub const WAVE_SURFACE_HEADER_ROWS: u32 = 2;

pub const MAX_SURFACE_WAVES: u32 = 48;

pub const WAVE_SURFACE_BYTES: u32 =
    (WAVE_SURFACE_HEADER_ROWS + MAX_SURFACE_WAVES * WAVE_SURFACE_ROWS) * 16;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceWave {
    pub start: [f32; 2],
    pub along: [f32; 2],
    pub shore: [f32; 2],
    pub length: f32,
    pub half_width: f32,
    pub amplitude: f32,
    pub velocity: f32,
    pub distance_from_shore: f32,
    pub phase: f32,
    pub cycle: f32,
    pub life: f32,
    pub steepness: f32,
}

impl SurfaceWave {
    #[must_use]
    pub fn rows(&self) -> [[f32; 4]; 4] {
        [
            [self.start[0], self.start[1], self.along[0], self.along[1]],
            [self.shore[0], self.shore[1], self.length, self.half_width],
            [
                self.amplitude,
                self.velocity,
                self.distance_from_shore,
                self.phase,
            ],
            [self.cycle, self.life, self.steepness, 0.0],
        ]
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct WaveSurface {
    pub waves: Vec<SurfaceWave>,
    pub dropped: usize,
    pub areas: usize,
}

impl WaveSurface {
    #[must_use]
    pub fn to_bytes(&self, tuning: &WaterTuning, seconds: f32) -> Vec<u8> {
        let mut out = Vec::with_capacity(WAVE_SURFACE_BYTES as usize);
        let count = self.waves.len().min(MAX_SURFACE_WAVES as usize);
        #[allow(clippy::cast_precision_loss)]
        let header = [
            [
                count as f32,
                finite(seconds, 0.0),
                finite(tuning.wave_surface_foam, 1.0),
                finite(tuning.wave_beach_band, 6.0),
            ],
            [
                finite(tuning.wave_fade_in, 0.15),
                finite(tuning.wave_fade_out, 0.45),
                finite(
                    tuning.wave_foam_amplitude,
                    crate::water_shader::WATER_WAVE_FOAM_AMPLITUDE,
                ),
                finite(
                    tuning.wave_foam_response,
                    crate::water_shader::WATER_WAVE_FOAM_RESPONSE,
                ),
            ],
        ];
        for row in header {
            for value in row {
                out.extend_from_slice(&value.to_le_bytes());
            }
        }
        for slot in 0..MAX_SURFACE_WAVES as usize {
            let rows = self
                .waves
                .get(slot)
                .map_or([[0.0f32; 4]; 4], SurfaceWave::rows);
            for row in rows {
                for value in row {
                    out.extend_from_slice(&value.to_le_bytes());
                }
            }
        }
        out
    }
}

fn finite(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

#[must_use]
pub fn surface_waves(field: &WaveField, look: WaveLook, tuning: &WaterTuning) -> WaveSurface {
    let mut out = WaveSurface::default();
    if !look.draw.on() || !look.pca.displaces() {
        return out;
    }
    let unit = finite(look.scale.world_units(), 1.0) * finite(tuning.wave_unit, 1.0);
    let height_scale = unit * finite(tuning.wave_final_height, 1.0);
    let reach = finite(tuning.wave_surface_reach, 2.0).clamp(0.0, 64.0);
    let steepness = finite(tuning.wave_surface_steepness, 0.6).clamp(0.0, 4.0);
    let amplitude_fraction = finite(tuning.wave_surface_amplitude, 0.25).clamp(0.0, 16.0);
    let time = finite(tuning.wave_time, 1.0).max(1e-3);
    for area in &field.areas {
        if !area.params.pca {
            continue;
        }
        let mut used = false;
        for line in &area.lines {
            if out.waves.len() >= MAX_SURFACE_WAVES as usize {
                out.dropped = out.dropped.saturating_add(1);
                continue;
            }
            let half_width = (area.params.final_height * height_scale * reach * 0.5).max(1e-2);
            let amplitude = area.params.final_height * height_scale * amplitude_fraction;
            out.waves.push(SurfaceWave {
                start: line.start,
                along: line.along,
                shore: line.shore,
                length: line.length,
                half_width,
                amplitude,
                velocity: area.params.initial_velocity * unit * finite(tuning.wave_velocity, 4.0)
                    / time,
                distance_from_shore: area.params.distance_from_shore
                    * unit
                    * finite(tuning.wave_distance_from_shore, 1.0),
                phase: area.phase,
                cycle: (area.params.time_offset_second_wave * time).max(1e-3),
                life: (area.params.time_to_fade * time).max(1e-3),
                steepness,
            });
            used = true;
        }
        if used {
            out.areas = out.areas.saturating_add(1);
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::wave_shader::{WaveDraw, WaveLook, WavePca};
    use crate::waves::{WaveArea, WaveField, WaveLine, WaveParams, WaveScale};

    fn look() -> WaveLook {
        WaveLook {
            draw: WaveDraw::On,
            pca: WavePca::Displace,
            scale: WaveScale::Record,
        }
    }

    fn field(segments: usize, pca: bool) -> WaveField {
        let lines: Vec<WaveLine> = (0..segments)
            .map(|i| WaveLine {
                #[allow(clippy::cast_precision_loss)]
                start: [i as f32 * 10.0, 0.0],
                along: [1.0, 0.0],
                shore: [0.0, 1.0],
                length: 10.0,
            })
            .collect();
        let params = WaveParams {
            final_width: 28.0,
            final_height: 18.0,
            initial_width_fraction: 0.5,
            initial_height_fraction: 1.0,
            initial_velocity: 5.0,
            time_to_fade: 2.0,
            time_to_compress: 1.0,
            time_offset_second_wave: 3.0,
            distance_from_shore: 40.0,
            pca,
        };
        WaveField {
            areas: vec![WaveArea {
                unique_id: 1,
                name: "New Wave Area".to_string(),
                texture: "wave256.tga".to_string(),
                params,
                height: 50.0,
                body: None,
                mesh: mint_render::MeshData::default(),
                tiles: 0,
                length: 100.0,
                centre: [0.0, 0.0],
                lines,
                phase: 0.25,
            }],
            stats: crate::waves::WaveStats::default(),
        }
    }

    #[test]
    fn the_uniform_tail_is_fixed_size_and_its_count_never_exceeds_its_slots() {
        let tuning = WaterTuning::default();
        for segments in [0usize, 1, MAX_SURFACE_WAVES as usize + 7] {
            let surface = surface_waves(&field(segments, true), look(), &tuning);
            let bytes = surface.to_bytes(&tuning, 1.5);
            assert_eq!(bytes.len(), WAVE_SURFACE_BYTES as usize, "{segments}");
            let count = f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            #[allow(clippy::cast_precision_loss)]
            let want = segments.min(MAX_SURFACE_WAVES as usize) as f32;
            assert_eq!(count, want, "{segments}");
            let head = (WAVE_SURFACE_HEADER_ROWS * 16) as usize;
            let used = head + surface.waves.len() * (WAVE_SURFACE_ROWS * 16) as usize;
            assert!(bytes[used..].iter().all(|b| *b == 0), "{segments}");
        }
    }

    #[test]
    fn swells_past_the_cap_are_counted_rather_than_dropped_in_silence() {
        let over = 7usize;
        let surface = surface_waves(
            &field(MAX_SURFACE_WAVES as usize + over, true),
            look(),
            &WaterTuning::default(),
        );
        assert_eq!(surface.waves.len(), MAX_SURFACE_WAVES as usize);
        assert_eq!(surface.dropped, over);
        assert_eq!(surface.areas, 1);
    }

    #[test]
    fn the_before_halves_of_the_comparison_leave_the_water_surface_alone() {
        let tuning = WaterTuning::default();
        let full = field(3, true);
        for (draw, pca) in [
            (WaveDraw::Off, WavePca::Displace),
            (WaveDraw::On, WavePca::Ignore),
            (WaveDraw::On, WavePca::Crest),
        ] {
            let surface = surface_waves(
                &full,
                WaveLook {
                    draw,
                    pca,
                    scale: WaveScale::Record,
                },
                &tuning,
            );
            assert_eq!(surface, WaveSurface::default(), "{draw:?} {pca:?}");
        }
        assert_eq!(
            surface_waves(&field(3, false), look(), &tuning),
            WaveSurface::default()
        );
        assert_eq!(surface_waves(&full, look(), &tuning).waves.len(), 3);
    }
}
