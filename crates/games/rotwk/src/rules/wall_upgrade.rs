use mint_movement::Vec2;
use mint_sage_economy::EntryKind;
use mint_sage_modules::effects::{Effect, Names};
use mint_sage_modules::ReplaceSelfUpgrade;
use mint_sage_passability::{Footprint, FootprintRecord};
use mint_sim::module::ModuleKind;
use mint_sim::{Entity, Result, Tick, World};

use super::{Ids, RotwkRules};
use crate::construction::footprint_of;
use crate::diag::diag;
use crate::unit::Kind;

struct Placed {
    records: Vec<FootprintRecord>,
    centre: Vec2,
    angle: f32,
}

impl Placed {
    fn bounding_radius(&self) -> f32 {
        self.records
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

    fn shapes(&self) -> impl Iterator<Item = (Vec2, Vec2, Vec2, Footprint)> + '_ {
        let (sin, cos) = (
            mint_math::f32::sin(self.angle),
            mint_math::f32::cos(self.angle),
        );
        let x_axis = Vec2::new(cos, sin);
        let y_axis = Vec2::new(-sin, cos);
        self.records.iter().map(move |record| {
            let centre = self
                .centre
                .plus(x_axis.scale(record.offset.x))
                .plus(y_axis.scale(record.offset.y));
            (centre, x_axis, y_axis, record.footprint)
        })
    }

    fn overlaps(&self, other: &Self) -> bool {
        self.shapes()
            .any(|a| other.shapes().any(|b| shapes_overlap(a, b)))
    }
}

fn shapes_overlap(a: (Vec2, Vec2, Vec2, Footprint), b: (Vec2, Vec2, Vec2, Footprint)) -> bool {
    let (ca, xa, ya, fa) = a;
    let (cb, xb, yb, fb) = b;
    match (fa, fb) {
        (Footprint::Disc { radius: ra }, Footprint::Disc { radius: rb }) => {
            cb.minus(ca).length() < ra + rb
        }
        (Footprint::Box { major, minor }, Footprint::Disc { radius })
        | (Footprint::Disc { radius }, Footprint::Box { major, minor }) => {
            let (centre, x_axis, y_axis, disc) = if matches!(fa, Footprint::Box { .. }) {
                (ca, xa, ya, cb)
            } else {
                (cb, xb, yb, ca)
            };
            let d = disc.minus(centre);
            let local_x = d.dot(x_axis).clamp(-major, major);
            let local_y = d.dot(y_axis).clamp(-minor, minor);
            let nearest = centre
                .plus(x_axis.scale(local_x))
                .plus(y_axis.scale(local_y));
            disc.minus(nearest).length() < radius
        }
        (
            Footprint::Box {
                major: ma,
                minor: na,
            },
            Footprint::Box {
                major: mb,
                minor: nb,
            },
        ) => {
            let d = cb.minus(ca);
            [xa, ya, xb, yb].into_iter().all(|axis| {
                let reach_a = ma * xa.dot(axis).abs() + na * ya.dot(axis).abs();
                let reach_b = mb * xb.dot(axis).abs() + nb * yb.dot(axis).abs();
                d.dot(axis).abs() < reach_a + reach_b
            })
        }
    }
}

fn is_wall_upgrade_or_hub(record: &mint_sage_modules::ObjectRecord) -> bool {
    record.has_kind("WALL_UPGRADE") || record.has_kind("WALL_HUB")
}

impl RotwkRules {
    fn conversion_poses(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        template: &str,
        attendants: &Names,
    ) -> Vec<(Kind, Placed)> {
        let Some(body) = world.get(ids.body, entity).map(|body| body.0) else {
            return Vec::new();
        };
        let angle = body.facing.radians();
        let placed = |kind: Kind, centre: Vec2| {
            self.kinds.get(kind).map(|entry| {
                (
                    kind,
                    Placed {
                        records: footprint_of(&entry.record),
                        centre,
                        angle,
                    },
                )
            })
        };
        let mut out: Vec<(Kind, Placed)> = self
            .kinds
            .kind(template)
            .and_then(|kind| placed(kind, body.position))
            .into_iter()
            .collect();
        if attendants.is_empty() {
            return out;
        }
        let step = world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .map_or(0.0, |entry| {
                super::upgrades::axis_extent(&footprint_of(&entry.record))
            });
        let (sin, cos) = (mint_math::f32::sin(angle), mint_math::f32::cos(angle));
        for (index, name) in attendants.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let along = step * (index + 1) as f32;
            let at = Vec2::new(body.position.x + along * cos, body.position.y + along * sin);
            if let Some(entry) = self.kinds.kind(name).and_then(|kind| placed(kind, at)) {
                out.push(entry);
            }
        }
        out
    }

    fn placed_now(&self, world: &World, ids: Ids, entity: Entity) -> Option<(Kind, Placed)> {
        let body = world.get(ids.body, entity)?.0;
        let kind = *world.get(ids.kind, entity)?;
        let entry = self.kinds.get(kind)?;
        Some((
            kind,
            Placed {
                records: footprint_of(&entry.record),
                centre: body.position,
                angle: body.facing.radians(),
            },
        ))
    }

    fn conversion_fits(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        module: &ReplaceSelfUpgrade,
        pending: bool,
    ) -> bool {
        if world.contains(ids.construction, entity) {
            return false;
        }
        let Some(template) = module.replace_with() else {
            return true;
        };
        let mine = self.conversion_poses(world, ids, entity, template, module.and_then_add());
        let Some(bodies) = world.store(ids.body) else {
            return true;
        };
        for (kind, placed) in &mine {
            let Some(record) = self.kinds.get(*kind).map(|entry| &entry.record) else {
                continue;
            };
            let hub = record.has_kind("WALL_HUB");
            let blocks = |other: &mint_sage_modules::ObjectRecord| {
                if hub {
                    other.has_kind("WALL_HUB")
                } else {
                    is_wall_upgrade_or_hub(other)
                }
            };
            for &candidate in bodies.entities() {
                if candidate == entity {
                    continue;
                }
                let standing = self.placed_now(world, ids, candidate).filter(|(kind, _)| {
                    self.kinds
                        .get(*kind)
                        .is_some_and(|entry| blocks(&entry.record))
                });
                if standing.is_some_and(|(_, other)| placed.overlaps(&other)) {
                    return false;
                }
                if pending
                    && self.pending_conversions(world, ids, candidate).iter().any(
                        |(kind, other)| {
                            self.kinds
                                .get(*kind)
                                .is_some_and(|entry| blocks(&entry.record))
                                && placed.overlaps(other)
                        },
                    )
                {
                    return false;
                }
            }
        }
        true
    }

    fn pending_conversions(&self, world: &World, ids: Ids, entity: Entity) -> Vec<(Kind, Placed)> {
        let Some(wiring) = self.modules.as_ref() else {
            return Vec::new();
        };
        let (Some(producer), Some(column)) = (
            world.get(ids.producer, entity),
            world.get(wiring.replace_state, entity),
        ) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for entry in producer.queue.entries() {
            if !matches!(entry.kind, EntryKind::Upgrade) {
                continue;
            }
            let Some(upgrade) = usize::try_from(entry.item.index())
                .ok()
                .and_then(|index| self.upgrade_buildables.get(index))
                .filter(|upgrade| !upgrade.is_player)
            else {
                continue;
            };
            let mut mask = self.effective_upgrades_in(world, ids, entity);
            mask.set(upgrade.bit);
            for slot in &column.0 {
                if !slot.state.would_upgrade(&mask) {
                    continue;
                }
                if let Some(template) = slot.state.replace_with() {
                    out.extend(self.conversion_poses(
                        world,
                        ids,
                        entity,
                        template,
                        slot.state.and_then_add(),
                    ));
                }
            }
        }
        out
    }

    pub(super) fn affected_with_fit(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        mask: &mint_sim::UpgradeMask,
        pending: bool,
    ) -> bool {
        let Some(wiring) = self.modules.as_ref() else {
            return false;
        };
        let Some(replace) = wiring.registry.type_id_of::<ReplaceSelfUpgrade>() else {
            return wiring.registry.any_would_upgrade(world, entity, mask);
        };
        let discount = |module: mint_sim::module::ModuleTypeId, slot: u32| {
            module == replace
                && world
                    .get(wiring.replace_state, entity)
                    .and_then(|column| column.0.get(usize::try_from(slot).ok()?))
                    .is_some_and(|held| {
                        !self.conversion_fits(world, ids, entity, &held.state, pending)
                    })
        };
        wiring
            .registry
            .any_would_upgrade_except(world, entity, mask, &discount)
    }

    pub(super) fn segments_taken_over(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
        template: &str,
        attendants: &Names,
    ) -> Vec<Entity> {
        let mine = self.conversion_poses(world, ids, entity, template, attendants);
        let Some(bodies) = world.store(ids.body) else {
            return Vec::new();
        };
        let mut out: Vec<Entity> = Vec::new();
        for (_, placed) in &mine {
            let range = 0.5 * placed.bounding_radius();
            for &candidate in bodies.entities() {
                if candidate == entity || out.contains(&candidate) {
                    continue;
                }
                let Some((kind, other)) = self.placed_now(world, ids, candidate) else {
                    continue;
                };
                if !self
                    .kinds
                    .get(kind)
                    .is_some_and(|entry| entry.record.has_kind("WALL_SEGMENT"))
                {
                    continue;
                }
                let reach = other.centre.minus(placed.centre).length() - other.bounding_radius();
                if reach <= range && placed.overlaps(&other) {
                    out.push(candidate);
                }
            }
        }
        out
    }

    pub(super) fn take_over_segments(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        template: &str,
        attendants: &Names,
    ) -> Result<()> {
        let taken = self.segments_taken_over(tick.state(), ids, entity, template, attendants);
        if taken.is_empty() {
            return Ok(());
        }
        diag!(
            "replace: {template} at entity {} takes over {} segment(s)",
            entity.index(),
            taken.len()
        );
        self.apply_effects(
            tick,
            ids,
            taken
                .into_iter()
                .map(|segment| (segment, Effect::Die))
                .collect(),
        )
    }

    pub(super) fn cancel_unaffected_object_upgrades(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
    ) -> Result<()> {
        let Some(producers) = tick.state().store(ids.producer) else {
            return Ok(());
        };
        let mut cancels: Vec<(Entity, mint_sage_economy::ProductionId)> = Vec::new();
        for &structure in producers.entities() {
            let Some(head) = tick
                .state()
                .get(ids.producer, structure)
                .and_then(|producer| producer.queue.entries().first())
            else {
                continue;
            };
            if !matches!(head.kind, EntryKind::Upgrade) {
                continue;
            }
            let Some(upgrade) = usize::try_from(head.item.index())
                .ok()
                .and_then(|index| self.upgrade_buildables.get(index))
                .filter(|upgrade| !upgrade.is_player)
            else {
                continue;
            };
            let mut mask = self.effective_upgrades_in(tick.state(), ids, structure);
            mask.set(upgrade.bit);
            if !self.affected_with_fit(tick.state(), ids, structure, &mask, false) {
                cancels.push((structure, head.id));
            }
        }
        for (structure, entry) in cancels {
            let Some(player) = tick.state().get(ids.owner, structure).map(|owner| owner.0) else {
                continue;
            };
            let economy = Self::economy_ids(ids);
            let Some(player_entity) = crate::economy::player_entity(tick.state(), economy, player)
            else {
                continue;
            };
            let mut purse = crate::economy::purse_of(tick.state(), economy, player);
            let cancelled = tick
                .state_mut()
                .get_mut(ids.producer, structure)
                .and_then(|producer| producer.queue.cancel(entry, &mut purse));
            if cancelled.is_none() {
                continue;
            }
            tick.state_mut()
                .insert(ids.purse, player_entity, crate::economy::Purse(purse))?;
            diag!(
                "upgrade: entity {}'s queued upgrade no longer applies - cancelled, refunded in full",
                structure.index()
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wall_piece(centre: Vec2, minor: f32) -> Placed {
        Placed {
            records: vec![FootprintRecord {
                offset: Vec2::new(0.0, 0.0),
                footprint: Footprint::Box { major: 5.0, minor },
            }],
            centre,
            angle: 0.0,
        }
    }

    #[test]
    fn neighbouring_pieces_overlap_and_the_next_pair_out_does_not() {
        let middle = wall_piece(Vec2::new(0.0, 0.0), 20.0);
        assert!(middle.overlaps(&wall_piece(Vec2::new(0.0, 38.0), 20.0)));
        assert!(!middle.overlaps(&wall_piece(Vec2::new(0.0, 76.0), 20.0)));
        let disc = Placed {
            records: vec![FootprintRecord {
                offset: Vec2::new(0.0, 0.0),
                footprint: Footprint::Disc { radius: 20.0 },
            }],
            centre: Vec2::new(24.0, 0.0),
            angle: 0.0,
        };
        assert!(middle.overlaps(&disc));
        assert!(!middle.overlaps(&Placed {
            centre: Vec2::new(26.0, 0.0),
            ..disc
        }));
    }

    #[test]
    fn the_bounding_radius_takes_each_axis_out_to_the_corner() {
        let gate = Placed {
            records: vec![FootprintRecord {
                offset: Vec2::new(0.0, 50.0),
                footprint: Footprint::Box {
                    major: 10.0,
                    minor: 10.0,
                },
            }],
            centre: Vec2::new(0.0, 0.0),
            angle: 0.0,
        };
        assert!((gate.bounding_radius() - 3700f32.sqrt()).abs() < 1e-3);
    }
}
