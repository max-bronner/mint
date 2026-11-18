use mint_math::rng::Pcg32;
use mint_movement::{Facing, Vec2};

use mint_sage_ini::MeleeBehaviorDef;

use crate::contain::{HordeContainDef, LeaderRef};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FormationSlot {
    pub offset: Vec2,
    pub rank: u32,
    pub index_in_rank: u32,
    pub unit_type: u16,
    pub leader: Option<LeaderRef>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HordeSettings {
    pub melee_attack_leash_distance: f32,
    pub use_slow_horde_movement: bool,
    pub ranks_that_stop_advance: Vec<u32>,
    pub ranks_to_release: Vec<u32>,
    pub melee_behavior: Option<MeleeBehaviorDef>,
    pub minimum_horde_size: u32,
    pub horse: bool,
    pub porcupine: bool,
    pub main_formation: bool,
    pub alternate_formation: Option<String>,
    pub aod: Option<crate::contain::AodSway>,
    pub eva_event_last_member_death: Option<String>,
}

pub const DEFAULT_MELEE_ATTACK_LEASH_DISTANCE: f32 = 60.0;

impl Default for HordeSettings {
    fn default() -> Self {
        Self {
            melee_attack_leash_distance: DEFAULT_MELEE_ATTACK_LEASH_DISTANCE,
            use_slow_horde_movement: false,
            ranks_that_stop_advance: Vec::new(),
            ranks_to_release: Vec::new(),
            melee_behavior: None,
            minimum_horde_size: 0,
            horse: false,
            porcupine: false,
            main_formation: false,
            alternate_formation: None,
            aod: None,
            eva_event_last_member_death: None,
        }
    }
}

impl HordeSettings {
    #[must_use]
    pub fn from_contain(def: &HordeContainDef) -> Self {
        Self {
            melee_attack_leash_distance: def
                .melee_attack_leash_distance
                .filter(|leash| leash.is_finite() && *leash >= 0.0)
                .unwrap_or(DEFAULT_MELEE_ATTACK_LEASH_DISTANCE),
            use_slow_horde_movement: def.use_slow_horde_movement.unwrap_or(false),
            ranks_that_stop_advance: def.ranks_that_stop_advance.clone(),
            ranks_to_release: def.ranks_to_release.clone(),
            melee_behavior: def.melee_behavior.clone(),
            minimum_horde_size: def.minimum_horde_size.unwrap_or(0),
            horse: def.horse,
            porcupine: def.porcupine.unwrap_or(false),
            main_formation: def.main_formation.unwrap_or(false),
            alternate_formation: def.alternate_formation.clone(),
            aod: def.aod,
            eva_event_last_member_death: def.eva_event_last_member_death.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FormationTable {
    slots: Vec<FormationSlot>,
    unit_types: Vec<String>,
    jitter: (f32, f32),
    settings: HordeSettings,
    leader_offset: Option<Vec2>,
}

impl FormationTable {
    #[must_use]
    pub fn from_contain(def: &HordeContainDef) -> Self {
        let wanted = def.slot_count();
        let mut slots = Vec::with_capacity(wanted);
        let mut unit_types: Vec<String> = Vec::new();
        for rank in &def.ranks {
            let unit_type = rank.unit_type.trim();
            let type_index = match unit_types
                .iter()
                .position(|known| known.eq_ignore_ascii_case(unit_type))
            {
                Some(at) => at,
                None => {
                    unit_types.push(unit_type.to_owned());
                    unit_types.len().saturating_sub(1)
                }
            };
            let type_index = u16::try_from(type_index).unwrap_or(u16::MAX);
            for (index, position) in rank.positions.iter().enumerate() {
                if slots.len() >= wanted {
                    break;
                }
                slots.push(FormationSlot {
                    offset: Vec2::new(position.x, position.y),
                    rank: rank.rank,
                    index_in_rank: u32::try_from(index).unwrap_or(u32::MAX),
                    unit_type: type_index,
                    leader: position.leader,
                });
            }
        }
        Self {
            slots,
            unit_types,
            jitter: def.random_offset,
            settings: HordeSettings::from_contain(def),
            leader_offset: if def.leaders_allowed.is_empty() {
                None
            } else {
                def.leader_position.map(|(x, y)| Vec2::new(x, y))
            },
        }
    }

    #[must_use]
    pub fn from_offsets(offsets: &[Vec2]) -> Self {
        Self {
            slots: offsets
                .iter()
                .enumerate()
                .map(|(index, offset)| FormationSlot {
                    offset: *offset,
                    rank: 0,
                    index_in_rank: u32::try_from(index).unwrap_or(u32::MAX),
                    unit_type: 0,
                    leader: None,
                })
                .collect(),
            unit_types: Vec::new(),
            jitter: (0.0, 0.0),
            settings: HordeSettings::default(),
            leader_offset: None,
        }
    }

    #[must_use]
    pub fn with_settings(mut self, settings: HordeSettings) -> Self {
        self.settings = settings;
        self
    }

    #[must_use]
    pub fn settings(&self) -> &HordeSettings {
        &self.settings
    }

    #[must_use]
    pub fn unit_types(&self) -> &[String] {
        &self.unit_types
    }

    #[must_use]
    pub fn slots(&self) -> &[FormationSlot] {
        &self.slots
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    #[must_use]
    pub fn jitter(&self) -> (f32, f32) {
        self.jitter
    }

    #[must_use]
    pub fn leader_offset(&self) -> Option<Vec2> {
        self.leader_offset
    }

    #[must_use]
    pub fn centroid(&self) -> Vec2 {
        let count = self.slots.len();
        if count == 0 {
            return Vec2::ZERO;
        }
        let mut sum = Vec2::ZERO;
        for slot in &self.slots {
            sum = sum.plus(slot.offset);
        }
        #[allow(clippy::cast_precision_loss)]
        let divisor = count as f32;
        Vec2::new(sum.x / divisor, sum.y / divisor)
    }

    #[must_use]
    pub fn place(&self, rng: &mut Pcg32) -> Vec<Vec2> {
        let (nx, ny) = (whole(self.jitter.0), whole(self.jitter.1));
        self.slots
            .iter()
            .map(|slot| {
                let dx = draw(rng, nx);
                let dy = draw(rng, ny);
                Vec2::new(slot.offset.x + dx, slot.offset.y + dy)
            })
            .collect()
    }
}

const MAX_JITTER: u32 = 1 << 20;

fn whole(jitter: f32) -> u32 {
    if !(jitter.is_finite() && jitter >= 1.0) {
        return 0;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let n = jitter.min(MAX_JITTER as f32) as u32;
    n
}

fn draw(rng: &mut Pcg32, n: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)]
    let span = n.saturating_mul(2).saturating_add(1) as f32;
    #[allow(clippy::cast_precision_loss)]
    let value = (rng.next_f32() * span).floor() - n as f32;
    value
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FormationBasis {
    sin: f32,
    cos: f32,
}

impl FormationBasis {
    #[must_use]
    pub fn of(facing: Facing) -> Self {
        let forward = facing.to_vector();
        Self {
            sin: forward.y,
            cos: forward.x,
        }
    }

    #[must_use]
    pub fn place(&self, origin: Vec2, offset: Vec2) -> Vec2 {
        Vec2::new(
            origin.x + offset.x * self.cos - offset.y * self.sin,
            origin.y + offset.x * self.sin + offset.y * self.cos,
        )
    }

    #[must_use]
    pub fn project(&self, origin: Vec2, point: Vec2) -> Vec2 {
        let dx = point.x - origin.x;
        let dy = point.y - origin.y;
        Vec2::new(dx * self.cos + dy * self.sin, dy * self.cos - dx * self.sin)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::contain::{RankLine, SlotPosition};

    fn def(slots: u32, ranks: &[(u32, &[(f32, f32)])], jitter: (f32, f32)) -> HordeContainDef {
        HordeContainDef {
            slots: Some(slots),
            random_offset: jitter,
            ranks: ranks
                .iter()
                .map(|(rank, positions)| RankLine {
                    rank: *rank,
                    unit_type: "U".to_owned(),
                    positions: positions
                        .iter()
                        .map(|(x, y)| SlotPosition {
                            x: *x,
                            y: *y,
                            leader: None,
                        })
                        .collect(),
                })
                .collect(),
            ..HordeContainDef::default()
        }
    }

    #[test]
    fn project_recovers_the_offset_place_produced() {
        for facing in [
            Facing::from_radians(0.0),
            Facing::from_radians(core::f32::consts::FRAC_PI_2),
            Facing::from_radians(0.7),
            Facing::from_radians(-1.9),
        ] {
            let basis = FormationBasis::of(facing);
            let origin = Vec2::new(12.0, -4.0);
            let offset = Vec2::new(17.0, -5.5);
            let world = basis.place(origin, offset);
            let back = basis.project(origin, world);
            assert!((back.x - offset.x).abs() < 1e-3, "{back:?} vs {offset:?}");
            assert!((back.y - offset.y).abs() < 1e-3, "{back:?} vs {offset:?}");
        }
    }

    #[test]
    fn the_table_is_the_rank_lines_flattened_in_declaration_order() {
        let table = FormationTable::from_contain(&def(
            15,
            &[
                (
                    1,
                    &[
                        (50.0, 0.0),
                        (50.0, 20.0),
                        (50.0, -20.0),
                        (50.0, 40.0),
                        (50.0, -40.0),
                    ],
                ),
                (
                    2,
                    &[
                        (30.0, 0.0),
                        (30.0, 20.0),
                        (30.0, -20.0),
                        (30.0, 40.0),
                        (30.0, -40.0),
                    ],
                ),
                (
                    3,
                    &[
                        (10.0, 0.0),
                        (10.0, 20.0),
                        (10.0, -20.0),
                        (10.0, 40.0),
                        (10.0, -40.0),
                    ],
                ),
            ],
            (0.0, 0.0),
        ));
        assert_eq!(table.len(), 15);
        assert_eq!(table.slots()[0].offset, Vec2::new(50.0, 0.0));
        assert_eq!(table.slots()[0].rank, 1);
        assert_eq!(table.slots()[5].offset, Vec2::new(30.0, 0.0));
        assert_eq!(table.slots()[5].rank, 2);
        assert_eq!(table.slots()[14].offset, Vec2::new(10.0, -40.0));
        assert_eq!(table.slots()[14].index_in_rank, 4);
        assert_eq!(table.centroid(), Vec2::new(30.0, 0.0));
    }

    #[test]
    fn a_table_takes_the_first_slots_positions_and_no_more() {
        let table = FormationTable::from_contain(&def(
            2,
            &[(1, &[(1.0, 0.0), (2.0, 0.0), (3.0, 0.0)])],
            (0.0, 0.0),
        ));
        assert_eq!(table.len(), 2);
        assert_eq!(table.slots()[1].offset, Vec2::new(2.0, 0.0));
        assert!(!table.is_empty());

        let empty = FormationTable::from_contain(&HordeContainDef::default());
        assert!(empty.is_empty());
        assert_eq!(empty.centroid(), Vec2::ZERO);
    }

    #[test]
    fn a_slot_pose_rotates_the_offset_onto_the_hordes_facing() {
        let offset = Vec2::new(10.0, 4.0);
        let origin = Vec2::new(100.0, 200.0);

        let east = FormationBasis::of(Facing::EAST).place(origin, offset);
        assert!(
            (east.x - 110.0).abs() < 1e-4 && (east.y - 204.0).abs() < 1e-4,
            "{east:?}"
        );

        let north = FormationBasis::of(Facing::from_radians(core::f32::consts::FRAC_PI_2))
            .place(origin, offset);
        assert!(
            (north.x - 96.0).abs() < 1e-3 && (north.y - 210.0).abs() < 1e-3,
            "{north:?}"
        );

        let radius = offset.length();
        for step in 0..16 {
            #[allow(clippy::cast_precision_loss)]
            let angle = step as f32 * 0.4;
            let placed = FormationBasis::of(Facing::from_radians(angle)).place(origin, offset);
            assert!(
                (placed.minus(origin).length() - radius).abs() < 1e-3,
                "step {step} moved the slot off its circle"
            );
        }
    }

    #[test]
    fn the_jitter_is_seeded_bounded_and_drawn_the_same_number_of_times() {
        let table = FormationTable::from_contain(&def(
            3,
            &[(1, &[(0.0, 0.0), (10.0, 0.0), (20.0, 0.0)])],
            (5.0, 3.0),
        ));
        let mut rng = Pcg32::new(7);
        let placed = table.place(&mut rng);
        assert_eq!(placed.len(), 3);
        for (index, point) in placed.iter().enumerate() {
            let base = table.slots()[index].offset;
            assert!((point.x - base.x).abs() <= 5.0, "{point:?}");
            assert!((point.y - base.y).abs() <= 3.0, "{point:?}");
            assert_eq!((point.x - base.x).fract(), 0.0, "{point:?}");
            assert_eq!((point.y - base.y).fract(), 0.0, "{point:?}");
        }
        let mut same = Pcg32::new(7);
        assert_eq!(table.place(&mut same), placed);
        let mut other = Pcg32::new(8);
        assert_ne!(table.place(&mut other), placed);

        let quiet = FormationTable::from_contain(&def(
            3,
            &[(1, &[(0.0, 0.0), (10.0, 0.0), (20.0, 0.0)])],
            (0.0, 0.0),
        ));
        let mut a = Pcg32::new(3);
        let mut b = Pcg32::new(3);
        let placed_quiet = quiet.place(&mut a);
        let _ = table.place(&mut b);
        assert_eq!(
            a.next_u32(),
            b.next_u32(),
            "a zero jitter must consume the same draws a non-zero one does"
        );
        for (index, point) in placed_quiet.iter().enumerate() {
            assert_eq!(*point, quiet.slots()[index].offset);
        }
    }
}
