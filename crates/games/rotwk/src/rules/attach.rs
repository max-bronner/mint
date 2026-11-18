use mint_movement::Vec2;
use mint_sage_modules::effects::{names, Effect};
use mint_sim::{Component, Entity, Result, StateHasher, Tick, World};

use super::{Ids, RotwkRules};
use crate::combat::Foe;
use crate::diag::diag;
use mint_sim::PlayerId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AttachLink {
    pub(crate) parent: Entity,
}

impl Component for AttachLink {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.parent.hash_into(hasher);
    }
}

impl RotwkRules {
    pub(super) fn advance_attach(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        living: &[(Foe, PlayerId)],
    ) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let mut carried: Vec<Entity> = tick
            .state()
            .store(wiring.attach_state)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        carried.sort_unstable_by_key(|entity| entity.index());
        for entity in carried {
            if tick.state().contains(ids.doomed, entity) {
                continue;
            }
            let Some(module) = tick
                .state()
                .get(wiring.attach_state, entity)
                .and_then(|column| column.0.first())
                .map(|slot| slot.state.clone())
            else {
                continue;
            };
            match tick.state().get(ids.attach_link, entity).copied() {
                Some(link) => self.attach_follow_or_drop(tick, ids, entity, link, &module)?,
                None => self.attach_scan(tick, ids, entity, &module, living)?,
            }
        }
        Ok(())
    }

    fn attach_scan(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        module: &mint_sage_modules::AttachUpdate,
        living: &[(Foe, PlayerId)],
    ) -> Result<()> {
        let Some(pos) = tick
            .state()
            .get(ids.body, entity)
            .map(|body| body.0.position)
        else {
            return Ok(());
        };
        let own_owner = tick.state().get(ids.owner, entity).map(|owner| owner.0);
        let reach = module.scan_range();
        let world = tick.state();
        let mut best: Option<(f32, Entity)> = None;
        for (foe, owner) in living {
            if foe.entity == entity
                || world.contains(ids.doomed, foe.entity)
                || world
                    .get(ids.health, foe.entity)
                    .is_some_and(|health| health.0.is_dead())
            {
                continue;
            }
            let distance = distance_squared(pos, foe.position);
            if distance > reach * reach || best.is_some_and(|(nearest, _)| distance >= nearest) {
                continue;
            }
            let Some(record) = world
                .get(ids.kind, foe.entity)
                .and_then(|kind| self.kinds.get(*kind))
                .map(|entry| &entry.record)
            else {
                continue;
            };
            if !module.object_filter().passes(&record.kind_of, &record.name) {
                continue;
            }
            let (same, allied, hostile) = own_owner.map_or((false, false, false), |player| {
                super::spell_effects::standing(self, player, Some(*owner))
            });
            if !module
                .object_filter()
                .passes_relation(same, allied, hostile)
            {
                continue;
            }
            best = Some((distance, foe.entity));
        }
        let Some((_, candidate)) = best else {
            return Ok(());
        };
        let parent = tick
            .state()
            .get(ids.member, candidate)
            .map_or(candidate, |member| member.horde);
        if self
            .record_of(tick, ids, parent)
            .is_some_and(|record| record.has_kind("CREEP"))
            && tick
                .state()
                .get(ids.owner, parent)
                .is_some_and(|owner| owner.0.get() >= self.players)
        {
            return Ok(());
        }
        tick.state_mut()
            .insert(ids.attach_link, entity, AttachLink { parent })?;
        let mut status_effects = vec![(
            entity,
            Effect::StatusBits {
                set: names(vec![String::from("ATTACHED")]),
                clear: names(Vec::new()),
            },
        )];
        if let Some(status) = module.parent_status() {
            status_effects.push((
                parent,
                Effect::StatusBits {
                    set: names(vec![status.to_owned()]),
                    clear: names(Vec::new()),
                },
            ));
        }
        self.apply_effects(tick, ids, status_effects)?;
        let parent_owner = tick.state().get(ids.owner, parent).map(|owner| owner.0);
        Self::note_combat_fact(tick, ids, |facts, now| {
            facts.push_attach(now, entity, parent_owner);
        });
        diag!(
            "attach: {} -> {} (owner {:?})",
            self.template_name(tick.state(), ids, entity),
            self.template_name(tick.state(), ids, parent),
            parent_owner
        );
        Ok(())
    }

    fn attach_follow_or_drop(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
        link: AttachLink,
        module: &mint_sage_modules::AttachUpdate,
    ) -> Result<()> {
        let parent = link.parent;
        let world = tick.state();
        let parent_gone = !world.is_live(parent)
            || world.contains(ids.doomed, parent)
            || world
                .get(ids.health, parent)
                .is_some_and(|health| health.0.is_dead());
        if parent_gone {
            tick.state_mut().remove(ids.attach_link, entity);
            if let Some(status) = module.parent_status() {
                self.apply_effects(
                    tick,
                    ids,
                    vec![(
                        parent,
                        Effect::StatusBits {
                            set: names(Vec::new()),
                            clear: names(vec![status.to_owned()]),
                        },
                    )],
                )?;
            }
            Self::note_combat_fact(tick, ids, |facts, now| {
                facts.push_attach_loss(now, entity);
            });
            if let Some(killed) =
                self.kill(tick, ids, entity, None, crate::combat::DeathNaming::Normal)?
            {
                self.resolve_deaths(tick, ids, &[killed])?;
            }
            diag!("attach: parent died, ring re-created");
            return Ok(());
        }
        let Some(own) = tick
            .state()
            .get(ids.body, parent)
            .map(|body| body.0.position)
        else {
            return Ok(());
        };
        let (target, feet) = Self::follow_point(tick.state(), ids, parent, own);
        let top = if module.anchor_to_top_of_geometry() {
            let height = if tick.state().contains(ids.horde, parent) {
                self.record_of(tick, ids, parent)
                    .and_then(|record| record.geometry.height)
            } else {
                tick.state()
                    .get(ids.shape, parent)
                    .map(|shape| shape.height)
            };
            feet + height.unwrap_or(0.0)
        } else {
            feet
        };
        if tick.frame().get() % 60 == 0 {
            diag!(
                "attach: {} follows {} at ({:.1}, {:.1}) height {top:.1} (parent feet {feet:.1})",
                self.template_name(tick.state(), ids, entity),
                self.template_name(tick.state(), ids, parent),
                target.x,
                target.y
            );
        }
        if let Some(mut body) = tick.state().get(ids.body, entity).copied() {
            body.0.position = target;
            tick.state_mut().insert(ids.body, entity, body)?;
        }
        tick.state_mut()
            .insert(ids.elevation, entity, crate::unit::Elevation::new(top))?;
        tick.state_mut().remove(ids.ground_offset, entity);
        Ok(())
    }

    pub(super) fn follow_point(world: &World, ids: Ids, parent: Entity, own: Vec2) -> (Vec2, f32) {
        let members: Vec<(Vec2, f32)> = world
            .get(ids.horde, parent)
            .map(|horde| {
                horde
                    .members
                    .iter()
                    .filter(|&&member| world.is_live(member))
                    .filter_map(|&member| {
                        let at = world.get(ids.body, member)?.0.position;
                        Some((at, Self::feet(world, ids, member)))
                    })
                    .collect()
            })
            .unwrap_or_default();
        if members.is_empty() {
            return (own, Self::feet(world, ids, parent));
        }
        let count = f32::from(u16::try_from(members.len()).unwrap_or(u16::MAX));
        let (x, y, z) = members
            .iter()
            .fold((0.0, 0.0, 0.0), |(x, y, z), (at, feet)| {
                (x + at.x, y + at.y, z + feet)
            });
        (Vec2::new(x / count, y / count), z / count)
    }

    fn template_name(&self, world: &World, ids: Ids, entity: Entity) -> String {
        world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .map(|entry| entry.record.name.clone())
            .unwrap_or_else(|| format!("entity {}", entity.index()))
    }
}

fn distance_squared(a: mint_movement::Vec2, b: mint_movement::Vec2) -> f32 {
    let (dx, dy) = (a.x - b.x, a.y - b.y);
    dx * dx + dy * dy
}
