use mint_movement::{Facing, Vec2};
use mint_sage_formation::{AodSway, FormationBasis};
use mint_sim::{Component, StateHasher};

use crate::horde::HordeSlot;
use crate::templates::RETAIL_FRAME_SECONDS;

const TRAIL_POINTS: usize = 60;

const RANK_LINES: usize = 20;

const RANK_TURN_PER_FRAME: f32 = 0.174_532_92;

pub(crate) const MEMBER_STEP_FACTOR: f32 = 1.25;

const HEIGHT_BIAS: f32 = 0.95;

const NO_SEGMENT: Vec2 = Vec2::new(-1.0, 0.0);

const TRAIL_END: f32 = 99_999.9;

#[derive(Debug, Clone, Copy, PartialEq)]
struct TrailPoint {
    position: Vec2,
    facing: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct RankLine {
    x: f32,
    facing: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SlotSway {
    phase: f32,
    amplitude: f32,
    frequency: f32,
    phase_z: f32,
    amplitude_z: f32,
    frequency_z: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct HordeTrail {
    sway: AodSway,
    trail: Vec<TrailPoint>,
    lines: Vec<RankLine>,
    slots: Vec<SlotSway>,
    oath_distance: f32,
}

impl Component for HordeTrail {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.trail.len());
        for point in &self.trail {
            hasher.write_f32(point.position.x);
            hasher.write_f32(point.position.y);
            hasher.write_f32(point.facing);
        }
        hasher.write_usize(self.lines.len());
        for line in &self.lines {
            hasher.write_f32(line.x);
            hasher.write_f32(line.facing);
        }
        hasher.write_usize(self.slots.len());
        for slot in &self.slots {
            for value in [
                slot.phase,
                slot.amplitude,
                slot.frequency,
                slot.phase_z,
                slot.amplitude_z,
                slot.frequency_z,
            ] {
                hasher.write_f32(value);
            }
        }
        hasher.write_f32(self.oath_distance);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Glided;

impl Component for Glided {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TrailPlace {
    pub(crate) position: Vec2,
    pub(crate) facing: Facing,
    pub(crate) height: f32,
}

fn between(rng: &mut mint_math::rng::Pcg32, low: f32, high: f32) -> f32 {
    low + (high - low) * rng.next_f32()
}

impl HordeTrail {
    pub(crate) fn new(
        sway: AodSway,
        slots: &[HordeSlot],
        origin: Vec2,
        facing: Facing,
        rng: &mut mint_math::rng::Pcg32,
    ) -> Self {
        let tau = core::f32::consts::TAU;
        let draws = slots
            .iter()
            .map(|_| {
                let amplitude = between(
                    rng,
                    1.0 - sway.amplitude_randomness,
                    1.0 + sway.amplitude_randomness,
                );
                let phase = between(rng, 0.0, tau * sway.frequency_randomness);
                let frequency = between(
                    rng,
                    1.0 - sway.frequency_randomness,
                    1.0 + sway.frequency_randomness,
                );
                let amplitude_z = between(
                    rng,
                    1.0 - sway.amplitude_randomness_z,
                    1.0 + sway.amplitude_randomness_z,
                );
                let phase_z = between(rng, 0.0, tau * sway.frequency_randomness_z);
                let frequency_z = between(
                    rng,
                    1.0 - sway.frequency_randomness_z,
                    1.0 + sway.frequency_randomness_z,
                );
                SlotSway {
                    phase,
                    amplitude,
                    frequency,
                    phase_z,
                    amplitude_z,
                    frequency_z,
                }
            })
            .collect();
        let mut lines: Vec<RankLine> = Vec::new();
        for slot in slots.iter().filter(|slot| slot.unit_type.is_some()) {
            if lines.len() < RANK_LINES && lines.iter().all(|line| line.x != slot.offset.x) {
                lines.push(RankLine {
                    x: slot.offset.x,
                    facing: facing.radians(),
                });
            }
        }
        let front = lines.first().map_or(0.0, |line| line.x);
        let basis = FormationBasis::of(facing);
        let trail = lines
            .iter()
            .take(TRAIL_POINTS)
            .map(|line| TrailPoint {
                position: basis.place(origin, Vec2::new(line.x - front, 0.0)),
                facing: facing.radians(),
            })
            .collect();
        Self {
            sway,
            trail,
            lines,
            slots: draws,
            oath_distance: 0.0,
        }
    }

    pub(crate) fn advance_oath(&mut self, holding: bool, speed: f32) {
        if holding {
            self.oath_distance += speed * RETAIL_FRAME_SECONDS;
        }
    }

    pub(crate) fn oath_scatter(&self, direction: Option<Vec2>, slot: usize) -> (Vec2, f32) {
        let Some(direction) = direction else {
            return (Vec2::ZERO, 0.0);
        };
        let d = self.oath_distance;
        #[allow(clippy::cast_precision_loss)]
        let xy_rank = (slot % 5) as f32;
        #[allow(clippy::cast_precision_loss)]
        let z_rank = ((slot.saturating_add(2)) % 5) as f32;
        let xy_mag =
            self.sway.scatter_speed_factor * d + xy_rank * self.sway.scatter_randomness * d * 0.25;
        let z_mag =
            self.sway.scatter_speed_factor * d + z_rank * self.sway.scatter_randomness * d * 0.25;
        (direction.scale(xy_mag), self.sway.oath_z_factor * z_mag)
    }

    pub(crate) fn lay(&mut self, position: Vec2, facing: Facing, spacing: f32) {
        let moved = self
            .trail
            .first()
            .map_or(f32::INFINITY, |last| position.minus(last.position).length());
        if moved > spacing {
            self.trail.insert(
                0,
                TrailPoint {
                    position,
                    facing: facing.radians(),
                },
            );
            self.trail.truncate(TRAIL_POINTS);
        }
    }

    pub(crate) fn place_lines(
        &mut self,
        head: Vec2,
        head_facing: Facing,
        frames: f32,
    ) -> Vec<Vec2> {
        let head = TrailPoint {
            position: head,
            facing: head_facing.radians(),
        };
        let point = |at: usize| -> TrailPoint {
            if at == 0 {
                head
            } else {
                self.trail.get(at - 1).copied().unwrap_or(head)
            }
        };
        let count = self.trail.len() + 1;
        let front = self.lines.first().map_or(0.0, |line| line.x);
        let turn = RANK_TURN_PER_FRAME * frames;
        let mut placed = Vec::with_capacity(self.lines.len());
        let mut segment = 0usize;
        let mut walked = 0.0f32;
        let mut direction = NO_SEGMENT;
        let mut turned = Vec::with_capacity(self.lines.len());
        for line in &self.lines {
            let mut left = front - line.x - walked;
            loop {
                let from = point(segment);
                let last = segment + 1 >= count;
                let length = if last {
                    TRAIL_END
                } else {
                    let along = point(segment + 1).position.minus(from.position);
                    direction = along;
                    along.length()
                };
                if last || length > left {
                    let unit = direction.normalised().unwrap_or(NO_SEGMENT);
                    placed.push(from.position.plus(unit.scale(left)));
                    let next = if last {
                        from.facing
                    } else {
                        point(segment + 1).facing
                    };
                    let share = if length > 0.0 { left / length } else { 0.0 };
                    let span =
                        Facing::from_radians(from.facing).delta_to(Facing::from_radians(next));
                    let target = Facing::from_radians(from.facing + span * share);
                    turned.push(
                        Facing::from_radians(line.facing)
                            .turned_towards(target, turn)
                            .radians(),
                    );
                    break;
                }
                left -= length;
                walked += length;
                segment += 1;
            }
        }
        for (line, facing) in self.lines.iter_mut().zip(turned) {
            line.facing = facing;
        }
        placed
    }

    pub(crate) fn place(
        &self,
        lines: &[Vec2],
        slot: &HordeSlot,
        index: usize,
        frame: f32,
        pace: f32,
    ) -> Option<TrailPlace> {
        let at = self
            .lines
            .iter()
            .rposition(|line| line.x == slot.offset.x)
            .unwrap_or(0);
        let line = self.lines.get(at)?;
        let origin = *lines.get(at)?;
        let facing = Facing::from_radians(line.facing);
        let sway = self.sway;
        let (across, height) = self.slots.get(index).map_or((0.0, 0.0), |draw| {
            let still = sway.still_amplitude.clamp(0.0, 1.0);
            let blend = (1.0 - still) * pace + still;
            let wave =
                mint_math::f32::sin(frame * sway.frequency_scale * draw.frequency + draw.phase);
            let across = wave * draw.amplitude * sway.amplitude_scale * blend;
            let still_z = sway.still_amplitude_z.clamp(0.0, 1.0);
            let blend_z = (1.0 - still_z) * pace + still_z;
            let wave_z = mint_math::f32::sin(
                frame * sway.frequency_scale_z * draw.frequency_z + draw.phase_z,
            );
            let height =
                (wave_z * draw.amplitude_z + HEIGHT_BIAS) * blend_z * sway.amplitude_scale_z;
            (across, height)
        });
        let position =
            FormationBasis::of(facing).place(origin, Vec2::new(0.0, slot.offset.y + across));
        Some(TrailPlace {
            position,
            facing,
            height,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scatter_rank_cycles_with_the_slot_index() {
        let sway = AodSway {
            scatter_speed_factor: 0.0,
            scatter_randomness: 4.0,
            oath_z_factor: 1.0,
            ..AodSway::default()
        };
        let mut rng = mint_math::rng::Pcg32::new(7);
        let mut trail = HordeTrail::new(sway, &[], Vec2::ZERO, Facing::from_radians(0.0), &mut rng);
        trail.advance_oath(true, 30.0);
        let (xy, z) = trail.oath_scatter(Some(Vec2::new(1.0, 0.0)), 3);
        assert!(xy.x > 0.0, "slot 3 is rank 3 on the xy scatter");
        assert_eq!(z, 0.0, "slot 3 is rank (3 + 2) % 5 = 0 on the rise");
    }
}
