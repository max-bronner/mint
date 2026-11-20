use mint_movement::Vec2;
use mint_sage_modules::{ObjectRecord, PieceShape};
use mint_sage_passability::{Footprint, FootprintRecord};
use mint_sim::{Component, Entity, Frame, LogicFrames, StateHasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Construction {
    pub(crate) builder: Option<Entity>,
    pub(crate) elapsed: u32,
    pub(crate) rebuild: Option<u32>,
    pub(crate) worker: Option<Entity>,
    pub(crate) delay: u32,
    pub(crate) created: Frame,
}

impl Component for Construction {
    fn hash_into(&self, hasher: &mut StateHasher) {
        match self.builder {
            Some(builder) => {
                hasher.write_bool(true);
                builder.hash_into(hasher);
            }
            None => hasher.write_bool(false),
        }
        hasher.write_u32(self.elapsed);
        hasher.write_u32(self.delay);
        self.created.hash_into(hasher);
        hasher.write_bool(self.rebuild.is_some());
        if let Some(frames) = self.rebuild {
            hasher.write_u32(frames);
        }
        match self.worker {
            Some(worker) => {
                hasher.write_bool(true);
                worker.hash_into(hasher);
            }
            None => hasher.write_bool(false),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BuildTask {
    pub(crate) kind: crate::unit::Kind,
    pub(crate) centre: Vec2,
    pub(crate) angle: f32,
    pub(crate) unchecked: bool,
}

impl Component for BuildTask {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u16(self.kind.0);
        hasher.write_f32(self.centre.x);
        hasher.write_f32(self.centre.y);
        hasher.write_f32(self.angle);
        if self.unchecked {
            hasher.write_u16(1);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Concealed;

impl Component for Concealed {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SiteWorker {
    pub(crate) site: Entity,
    pub(crate) stage: WorkerStage,
    pub(crate) deployed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkerStage {
    Approaching,
    Facing,
    Constructing,
    Released,
}

impl Component for SiteWorker {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.site.hash_into(hasher);
        hasher.write_u8(self.stage as u8);
        hasher.write_bool(self.deployed);
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AutoRepair {
    pub(crate) timer: f32,
    pub(crate) repair: Repairing,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Repairing {
    Nothing,
    Worker(Entity),
    Itself { per_frame: f32 },
}

impl Component for AutoRepair {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.timer);
        match self.repair {
            Repairing::Nothing => hasher.write_u8(0),
            Repairing::Worker(worker) => {
                hasher.write_u8(1);
                worker.hash_into(hasher);
            }
            Repairing::Itself { per_frame } => {
                hasher.write_u8(2);
                hasher.write_f32(per_frame);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reason {
    OffMap,
    Blocked,
    NotFlat,
    Planned,
    Ford,
    EnemyUnit,
    Unreachable,
    NoFootprint,
    NotOffered,
    NoMoney,
    Disallowed,
    WallHubTooFar,
}

impl Reason {
    pub(crate) const fn message(self) -> &'static str {
        match self {
            Self::OffMap => "off the map",
            Self::Blocked => "blocked by terrain or another structure",
            Self::NotFlat => "the ground is not flat enough",
            Self::Planned => "a planned building is there",
            Self::Ford => "in a ford",
            Self::EnemyUnit => "an enemy unit is there",
            Self::Unreachable => "no path from the builder",
            Self::NoFootprint => "this template has no usable footprint",
            Self::NotOffered => "not offered by this porter's command set",
            Self::NoMoney => "not enough resources",
            Self::Disallowed => "disallowed for this player",
            Self::WallHubTooFar => "too far from an owned command centre",
        }
    }

    pub(crate) const fn code(self) -> u8 {
        match self {
            Self::Unreachable => 2,
            Self::OffMap => 6,
            Self::NoMoney => 7,
            Self::Blocked | Self::Planned | Self::EnemyUnit => 8,
            Self::WallHubTooFar => 9,
            Self::NotFlat
            | Self::Ford
            | Self::NoFootprint
            | Self::NotOffered
            | Self::Disallowed => 1,
        }
    }
}

const FLATNESS_SAMPLE_STEP: f32 = 10.0;

pub(crate) fn height_variation(
    footprint: &[FootprintRecord],
    centre: Vec2,
    angle: f32,
    height_at: impl Fn(f32, f32) -> Option<f32>,
) -> f32 {
    let (sin, cos) = mint_math::f32::sin_cos(angle);
    let mut low = f32::INFINITY;
    let mut high = f32::NEG_INFINITY;
    for record in footprint {
        let (half_x, half_y, disc) = match record.footprint {
            Footprint::Box { major, minor } => (major, minor, false),
            Footprint::Disc { radius } => (radius, radius, true),
        };
        let mut local_y = -half_y;
        while local_y < half_y + FLATNESS_SAMPLE_STEP {
            let y = local_y.min(half_y);
            let mut local_x = -half_x;
            while local_x < half_x + FLATNESS_SAMPLE_STEP {
                let x = local_x.min(half_x);
                local_x += FLATNESS_SAMPLE_STEP;
                if disc && x * x + y * y > half_x * half_x {
                    continue;
                }
                let (px, py) = (x + record.offset.x, y + record.offset.y);
                let at_x = centre.x + px * cos - py * sin;
                let at_y = centre.y + px * sin + py * cos;
                if let Some(z) = height_at(at_x, at_y) {
                    low = low.min(z);
                    high = high.max(z);
                }
            }
            local_y += FLATNESS_SAMPLE_STEP;
        }
    }
    if high < low {
        0.0
    } else {
        high - low
    }
}

pub(crate) fn footprint_of(record: &ObjectRecord) -> Vec<FootprintRecord> {
    shown_footprint(record, &[])
}

pub(crate) fn bounding_radius(records: &[FootprintRecord]) -> f32 {
    records
        .iter()
        .map(|record| {
            let (ox, oy) = (record.offset.x.abs(), record.offset.y.abs());
            match record.footprint {
                Footprint::Box { major, minor } => {
                    let (x, y) = (ox + major, oy + minor);
                    (x * x + y * y).sqrt()
                }
                Footprint::Disc { radius } => (ox * ox + oy * oy).sqrt() + radius,
            }
        })
        .fold(0.0, f32::max)
}

pub(crate) fn object_bounding_radius(
    world: &mint_sim::World,
    kinds: &crate::templates::KindTable,
    ids: (
        mint_sim::ComponentId<crate::unit::Kind>,
        Option<mint_sim::ComponentId<mint_sage_modules::effects::UpgradeState>>,
    ),
    entity: Entity,
) -> f32 {
    let overrides = ids
        .1
        .and_then(|state| world.get(state, entity))
        .map_or(&[][..], |state| state.geometry.as_slice());
    world
        .get(ids.0, entity)
        .and_then(|kind| kinds.get(*kind))
        .map_or(0.0, |entry| {
            bounding_radius(&shown_footprint(&entry.record, overrides))
        })
        .max(0.01)
}

pub(crate) fn shown_footprint(
    record: &ObjectRecord,
    overrides: &[(mint_sage_modules::effects::Name, bool)],
) -> Vec<FootprintRecord> {
    record
        .geometry
        .shown(overrides)
        .filter(|piece| piece.offset_z <= mint_sage_passability::MAX_STAMPED_OFFSET_Z)
        .filter_map(|piece| {
            let footprint = match piece.shape {
                PieceShape::Box { major, minor } if major > 0.0 || minor > 0.0 => {
                    Footprint::Box { major, minor }
                }
                PieceShape::Disc { radius } if radius > 0.0 => Footprint::Disc { radius },
                _ => return None,
            };
            Some(FootprintRecord {
                offset: Vec2::new(piece.offset.0, piece.offset.1),
                footprint,
            })
        })
        .collect()
}

pub(crate) fn bookend_footprint(record: &ObjectRecord) -> Option<FootprintRecord> {
    let piece = record.geometry.pieces.iter().find(|piece| {
        piece
            .name
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case("Bookend"))
    })?;
    if piece.offset_z > mint_sage_passability::MAX_STAMPED_OFFSET_Z {
        return None;
    }
    let footprint = match piece.shape {
        PieceShape::Box { major, minor } if major > 0.0 || minor > 0.0 => {
            Footprint::Box { major, minor }
        }
        PieceShape::Disc { radius } if radius > 0.0 => Footprint::Disc { radius },
        _ => return None,
    };
    Some(FootprintRecord {
        offset: Vec2::new(piece.offset.0, piece.offset.1),
        footprint,
    })
}

pub(crate) fn sell_value(
    record: &ObjectRecord,
    sell_percentage: f32,
    modifiers: &mint_sage_economy::CostModifiers,
) -> i64 {
    if let Some(value) = record
        .refund_value
        .filter(|value| *value != 0.0)
        .and_then(crate::templates::whole)
    {
        return value.max(0);
    }
    cost_refund(record, sell_percentage, modifiers)
}

pub(crate) fn cost_refund(
    record: &ObjectRecord,
    percent: f32,
    modifiers: &mint_sage_economy::CostModifiers,
) -> i64 {
    price_of(
        record,
        modifiers,
        &mint_sage_economy::TimeModifiers::default(),
    )
    .map_or(0, |(cost, _)| scale_refund(cost, percent))
}

pub(crate) fn bounty_refund(
    record: &ObjectRecord,
    percent: f32,
    modifiers: &mint_sage_economy::CostModifiers,
) -> i64 {
    let Some(value) = record
        .bounty_value
        .and_then(crate::templates::whole)
        .filter(|value| *value >= 0)
    else {
        return 0;
    };
    scale_refund(mint_sage_economy::cost_to_build(value, modifiers), percent)
}

pub(crate) fn price_of(
    record: &ObjectRecord,
    modifiers: &mint_sage_economy::CostModifiers,
    time: &mint_sage_economy::TimeModifiers,
) -> Option<(i64, u32)> {
    let cost = record
        .build_cost
        .and_then(crate::templates::whole)
        .filter(|cost| *cost >= 0)?;
    let seconds = record
        .build_time
        .filter(|seconds| (0.0..=crate::templates::MAX_BUILD_SECONDS).contains(seconds))?;
    let frames: LogicFrames = mint_sage_economy::time_to_build(seconds, time);
    Some((
        mint_sage_economy::cost_to_build(cost, modifiers),
        frames.get(),
    ))
}

#[allow(clippy::cast_precision_loss)]
pub(crate) fn scale_refund(cost: i64, percent: f32) -> i64 {
    if !percent.is_finite() || percent <= 0.0 || cost <= 0 {
        return 0;
    }
    let scaled = f64::from(percent) * (cost as f64);
    if !scaled.is_finite() || scaled <= 0.0 {
        0
    } else if scaled >= i64::MAX as f64 {
        i64::MAX
    } else {
        #[allow(clippy::cast_possible_truncation)]
        let refund = scaled as i64;
        refund
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COST: mint_sage_economy::CostModifiers = mint_sage_economy::CostModifiers {
        production_cost_change_percent: 0.0,
        kind_of_modifier: 1.0,
        handicap: 1.0,
        brutal_reduction: 0.0,
    };
    const TIME: mint_sage_economy::TimeModifiers = mint_sage_economy::TimeModifiers {
        handicap: 1.0,
        production_time_change_percent: 0.0,
        builder_modifier: 1.0,
        speed_multiplier: 1.0,
        brutal_reduction: 0.0,
    };

    #[test]
    fn height_variation_spans_the_rotated_footprint_and_skips_off_map_samples() {
        let slope = |x: f32, _y: f32| Some(x * 0.5);
        let boxed = [FootprintRecord {
            offset: Vec2::ZERO,
            footprint: Footprint::Box {
                major: 40.0,
                minor: 10.0,
            },
        }];
        assert!(
            (height_variation(&boxed, Vec2::new(100.0, 100.0), 0.0, slope) - 40.0).abs() < 1e-3
        );
        let across = height_variation(
            &boxed,
            Vec2::new(100.0, 100.0),
            core::f32::consts::FRAC_PI_2,
            slope,
        );
        assert!(
            across < 11.0,
            "turned a quarter, only the minor radius lies along the slope: {across}"
        );
        let disc = [FootprintRecord {
            offset: Vec2::ZERO,
            footprint: Footprint::Disc { radius: 20.0 },
        }];
        assert!((height_variation(&disc, Vec2::new(100.0, 100.0), 0.0, slope) - 20.0).abs() < 1e-3);
        assert_eq!(height_variation(&boxed, Vec2::ZERO, 0.0, |_, _| None), 0.0);
    }

    fn record_with(pieces: Vec<(f32, f32, PieceShape)>) -> ObjectRecord {
        ObjectRecord {
            geometry: mint_sage_modules::Geometry {
                pieces: pieces
                    .into_iter()
                    .map(|(x, y, shape)| mint_sage_modules::GeometryPiece {
                        offset: (x, y),
                        offset_z: 0.0,
                        shape,
                        name: None,
                        active: true,
                        height: None,
                        for_health_box: true,
                    })
                    .collect(),
                ..mint_sage_modules::Geometry::default()
            },
            ..ObjectRecord::default()
        }
    }

    #[test]
    fn footprint_of_keeps_every_positive_piece_and_its_offset() {
        let record = record_with(vec![
            (
                45.0,
                15.0,
                PieceShape::Box {
                    major: 5.0,
                    minor: 25.0,
                },
            ),
            (0.0, 45.0, PieceShape::Disc { radius: 5.0 }),
            (0.0, 0.0, PieceShape::Disc { radius: 0.0 }),
        ]);
        let footprint = footprint_of(&record);
        assert_eq!(footprint.len(), 2, "the zero-radius piece is dropped");
        assert_eq!(footprint[0].offset, Vec2::new(45.0, 15.0));
        assert_eq!(
            footprint[0].footprint,
            Footprint::Box {
                major: 5.0,
                minor: 25.0
            }
        );
        assert_eq!(footprint[1].offset, Vec2::new(0.0, 45.0));
        assert_eq!(footprint[1].footprint, Footprint::Disc { radius: 5.0 });
    }

    #[test]
    fn footprint_of_a_bare_record_is_empty() {
        assert!(footprint_of(&ObjectRecord::default()).is_empty());
    }

    fn piece_named(
        name: &str,
        offset_z: f32,
        shape: PieceShape,
    ) -> mint_sage_modules::GeometryPiece {
        mint_sage_modules::GeometryPiece {
            offset: (3.0, 4.0),
            offset_z,
            shape,
            name: Some(name.to_owned()),
            active: false,
            height: Some(53.0),
            for_health_box: true,
        }
    }

    fn record_of(piece: mint_sage_modules::GeometryPiece) -> ObjectRecord {
        ObjectRecord {
            geometry: mint_sage_modules::Geometry {
                pieces: vec![piece],
                ..mint_sage_modules::Geometry::default()
            },
            ..ObjectRecord::default()
        }
    }

    #[test]
    fn a_bookend_is_re_added_by_its_offset_z_not_its_height() {
        let disc = PieceShape::Disc { radius: 20.0 };
        let footprint = bookend_footprint(&record_of(piece_named("Bookend", 0.0, disc)))
            .expect("a tall Bookend at ground level qualifies");
        assert_eq!(footprint.offset, Vec2::new(3.0, 4.0));
        assert_eq!(footprint.footprint, Footprint::Disc { radius: 20.0 });
        assert!(bookend_footprint(&record_of(piece_named("Bookend", 11.0, disc))).is_none());
        assert!(bookend_footprint(&record_of(piece_named("Other", 0.0, disc))).is_none());
    }

    #[test]
    fn a_raised_geometry_piece_is_not_stamped() {
        let mut low = piece_named("Low", 0.0, PieceShape::Disc { radius: 5.0 });
        low.active = true;
        let mut edge = piece_named("Edge", 10.0, PieceShape::Disc { radius: 6.0 });
        edge.active = true;
        let mut raised = piece_named("Raised", 11.0, PieceShape::Disc { radius: 7.0 });
        raised.active = true;
        let record = ObjectRecord {
            geometry: mint_sage_modules::Geometry {
                pieces: vec![low, edge, raised],
                ..mint_sage_modules::Geometry::default()
            },
            ..ObjectRecord::default()
        };
        let radii: Vec<Footprint> = footprint_of(&record)
            .into_iter()
            .map(|record| record.footprint)
            .collect();
        assert_eq!(
            radii,
            [
                Footprint::Disc { radius: 5.0 },
                Footprint::Disc { radius: 6.0 }
            ]
        );
    }

    #[test]
    fn price_of_reads_cost_and_converts_build_time_to_frames() {
        let record = ObjectRecord {
            build_cost: Some(1000.0),
            build_time: Some(10.0),
            ..ObjectRecord::default()
        };
        let (cost, frames) = price_of(&record, &COST, &TIME).unwrap();
        assert_eq!(cost, 1000);
        assert_eq!(frames, 300, "10 seconds at 30 Hz");
    }

    #[test]
    fn scale_refund_at_fifty_percent_halves_the_cost() {
        assert_eq!(scale_refund(1000, 0.5), 500);
        assert_eq!(
            scale_refund(1000, 1.0),
            1000,
            "100% is retail's own behaviour"
        );
        assert_eq!(scale_refund(1000, 0.0), 0);
        assert_eq!(
            scale_refund(1000, f32::NAN),
            0,
            "a hostile percent refunds nothing"
        );
        assert_eq!(
            scale_refund(-100, 0.5),
            0,
            "a nonsense cost refunds nothing"
        );
    }

    #[test]
    fn a_sale_refunds_the_refund_value_or_the_sell_percentage() {
        let barracks = ObjectRecord {
            build_cost: Some(300.0),
            build_time: Some(10.0),
            ..ObjectRecord::default()
        };
        assert_eq!(sell_value(&barracks, 0.5, &COST), 150);
        let statue = ObjectRecord {
            refund_value: Some(100.0),
            ..barracks.clone()
        };
        assert_eq!(sell_value(&statue, 0.5, &COST), 100);
        let zero = ObjectRecord {
            refund_value: Some(0.0),
            ..barracks
        };
        assert_eq!(sell_value(&zero, 0.5, &COST), 150, "0 means no override");
        assert_eq!(sell_value(&ObjectRecord::default(), 0.5, &COST), 0);
    }

    #[test]
    fn price_of_refuses_a_missing_or_hostile_field() {
        assert!(price_of(&ObjectRecord::default(), &COST, &TIME).is_none());
        let record = ObjectRecord {
            build_cost: Some(-5.0),
            build_time: Some(10.0),
            ..ObjectRecord::default()
        };
        assert!(
            price_of(&record, &COST, &TIME).is_none(),
            "a negative cost is refused"
        );
    }

    #[test]
    fn reason_code_follows_the_decoded_code_names() {
        assert_eq!(Reason::Unreachable.code(), 2);
        assert_eq!(Reason::OffMap.code(), 6);
        assert_eq!(Reason::NoMoney.code(), 7);
        for in_the_way in [Reason::Blocked, Reason::Planned, Reason::EnemyUnit] {
            assert_eq!(in_the_way.code(), 8);
        }
        assert_eq!(Reason::WallHubTooFar.code(), 9);
        for unmapped in [
            Reason::Ford,
            Reason::NoFootprint,
            Reason::NotOffered,
            Reason::Disallowed,
        ] {
            assert_eq!(unmapped.code(), 1);
        }
    }
}
