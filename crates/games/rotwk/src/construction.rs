use mint_movement::Vec2;
use mint_sage_modules::{ObjectRecord, PieceShape};
use mint_sage_passability::{Footprint, FootprintRecord};
use mint_sim::{Component, Entity, LogicFrames, StateHasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Construction {
    pub(crate) builder: Option<Entity>,
    pub(crate) elapsed: u32,
    pub(crate) rebuild: Option<u32>,
    pub(crate) worker: Option<Entity>,
    pub(crate) delay: u32,
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
            Self::Ford | Self::NoFootprint | Self::NotOffered | Self::Disallowed => 1,
        }
    }
}

pub(crate) fn footprint_of(record: &ObjectRecord) -> Vec<FootprintRecord> {
    shown_footprint(record, &[])
}

pub(crate) fn shown_footprint(
    record: &ObjectRecord,
    overrides: &[(mint_sage_modules::effects::Name, bool)],
) -> Vec<FootprintRecord> {
    record
        .geometry
        .shown(overrides)
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

const BOOKEND_DECK_HEIGHT: f32 = 10.0;

pub(crate) fn bookend_footprint(record: &ObjectRecord) -> Option<FootprintRecord> {
    let piece = record.geometry.pieces.iter().find(|piece| {
        piece
            .name
            .as_deref()
            .is_some_and(|name| name.eq_ignore_ascii_case("Bookend"))
    })?;
    if piece
        .height
        .is_some_and(|height| height > BOOKEND_DECK_HEIGHT)
    {
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
    };
    const TIME: mint_sage_economy::TimeModifiers = mint_sage_economy::TimeModifiers {
        handicap: 1.0,
        production_time_change_percent: 0.0,
    };

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
        height: Option<f32>,
        shape: PieceShape,
    ) -> mint_sage_modules::GeometryPiece {
        mint_sage_modules::GeometryPiece {
            offset: (3.0, 4.0),
            offset_z: 0.0,
            shape,
            name: Some(name.to_owned()),
            active: false,
            height,
            for_health_box: true,
        }
    }

    #[test]
    fn bookend_footprint_reads_the_named_piece_at_or_under_the_deck_height() {
        let record = ObjectRecord {
            geometry: mint_sage_modules::Geometry {
                pieces: vec![piece_named(
                    "Bookend",
                    Some(BOOKEND_DECK_HEIGHT),
                    PieceShape::Disc { radius: 20.0 },
                )],
                ..mint_sage_modules::Geometry::default()
            },
            ..ObjectRecord::default()
        };
        let footprint = bookend_footprint(&record).expect("a Bookend piece at the limit qualifies");
        assert_eq!(footprint.offset, Vec2::new(3.0, 4.0));
        assert_eq!(footprint.footprint, Footprint::Disc { radius: 20.0 });
    }

    #[test]
    fn a_tall_bookend_or_a_missing_one_is_refused() {
        let tall = ObjectRecord {
            geometry: mint_sage_modules::Geometry {
                pieces: vec![piece_named(
                    "Bookend",
                    Some(BOOKEND_DECK_HEIGHT + 1.0),
                    PieceShape::Disc { radius: 20.0 },
                )],
                ..mint_sage_modules::Geometry::default()
            },
            ..ObjectRecord::default()
        };
        assert!(bookend_footprint(&tall).is_none(), "too tall, still a deck");
        let none = ObjectRecord {
            geometry: mint_sage_modules::Geometry {
                pieces: vec![piece_named(
                    "Other",
                    Some(0.0),
                    PieceShape::Disc { radius: 20.0 },
                )],
                ..mint_sage_modules::Geometry::default()
            },
            ..ObjectRecord::default()
        };
        assert!(bookend_footprint(&none).is_none(), "no piece named Bookend");
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
