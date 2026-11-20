use mint_sage_modules::LargeGroupBonusUpdate;
use mint_sim::{Component, Entity, LogicFrames, Result, StateHasher, Tick, World};

use super::{Ids, RotwkRules};
use crate::diag::diag;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GroupBonus;

impl Component for GroupBonus {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

const EXTRA_RETAIL_FRAMES: u32 = 1;

impl RotwkRules {
    pub(super) fn group_bonus_list_of(
        &self,
        world: &World,
        ids: Ids,
        entity: Entity,
    ) -> Option<mint_sage_combat::ModifierId> {
        let commanded = Self::commanded_unit(world, ids, entity);
        if !world.contains(ids.group_bonus, commanded) {
            return None;
        }
        let wiring = self.modules.as_ref()?;
        let name = world
            .get(wiring.group_bonus_state, commanded)?
            .0
            .first()?
            .state
            .attribute_modifier()?;
        self.combat.modifiers.id(name)
    }

    pub(super) fn step_group_bonuses(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let now = tick.frame();
        let mut casters: Vec<Entity> = tick
            .state()
            .store(wiring.group_bonus_state)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        casters.sort_unstable_by_key(|entity| entity.index());
        casters.dedup();
        for entity in casters {
            if tick.state().contains(ids.doomed, entity)
                || tick
                    .state()
                    .get(ids.health, entity)
                    .is_some_and(|health| health.0.is_dead())
            {
                continue;
            }
            let Some(module) = tick
                .state()
                .get(wiring.group_bonus_state, entity)
                .and_then(|column| column.0.first())
                .map(|slot| slot.state.clone())
            else {
                continue;
            };
            let mut module = module;
            if !module.started() {
                module.stagger(entity, now);
                self.store_group_bonus(tick, wiring, entity, &module);
            }
            if !module.due(now) {
                continue;
            }
            let extra =
                if self.scans_every_update(tick.state(), ids, entity) {
                    LogicFrames::ZERO
                } else {
                    LogicFrames::from_frames(EXTRA_RETAIL_FRAMES.saturating_mul(
                        mint_sim::SIMULATION_HZ / mint_sage_modules::RETAIL_LOGIC_HZ,
                    ))
                };
            module.scanned(now, extra);
            self.store_group_bonus(tick, wiring, entity, &module);
            let held = tick.state().contains(ids.group_bonus, entity);
            let (members, hordes) = self.crowd_of(tick.state(), ids, &module, entity);
            let rubbed = hordes
                .iter()
                .any(|(horde, near)| *near && tick.state().contains(ids.group_bonus, *horde));
            let enough = members.saturating_add(1) >= module.count() || rubbed;
            if enough == held {
                continue;
            }
            if enough {
                tick.state_mut()
                    .insert(ids.group_bonus, entity, GroupBonus)?;
            } else {
                tick.state_mut().remove(ids.group_bonus, entity);
            }
            let mut touched = vec![entity];
            if let Some(horde) = tick.state().get(ids.horde, entity) {
                touched.extend(horde.members.iter().copied());
            }
            for member in touched {
                if tick.state().is_live(member) {
                    self.recompute_bonuses(tick, ids, member)?;
                }
            }
            diag!(
                "group bonus: entity {} counts {} within {} and {} {}",
                entity.index(),
                members,
                module.radius(),
                if enough { "gains" } else { "loses" },
                module.attribute_modifier().unwrap_or("no list"),
            );
        }
        Ok(())
    }

    fn scans_every_update(&self, world: &World, ids: Ids, entity: Entity) -> bool {
        world
            .get(ids.kind, entity)
            .and_then(|kind| self.kinds.get(*kind))
            .is_some_and(|entry| {
                entry
                    .record
                    .kind_of
                    .iter()
                    .any(|kind| kind == "INFANTRY" || kind == "CAVALRY")
            })
    }

    fn crowd_of(
        &self,
        world: &World,
        ids: Ids,
        module: &LargeGroupBonusUpdate,
        entity: Entity,
    ) -> (u32, Vec<(Entity, bool)>) {
        let Some(position) = world.get(ids.body, entity).map(|body| body.0.position) else {
            return (0, Vec::new());
        };
        let owner = world.get(ids.owner, entity).map(|owner| owner.0);
        let (radius, rub_off) = (module.radius(), module.rub_off_radius());
        let mut total = 0u32;
        let mut found = Vec::new();
        let Some(store) = world.store(ids.horde) else {
            return (0, found);
        };
        for (horde, record) in store.iter() {
            if !world.is_live(horde)
                || world.contains(ids.doomed, horde)
                || world.get(ids.owner, horde).map(|other| other.0) != owner
            {
                continue;
            }
            let Some(there) = world.get(ids.body, horde).map(|body| body.0.position) else {
                continue;
            };
            let (dx, dy) = (there.x - position.x, there.y - position.y);
            let distance_squared = dx * dx + dy * dy;
            if distance_squared > radius * radius {
                continue;
            }
            let matching = record
                .members
                .iter()
                .filter(|member| {
                    world.is_live(**member)
                        && world
                            .get(ids.kind, **member)
                            .and_then(|kind| self.kinds.get(*kind))
                            .is_some_and(|entry| {
                                module
                                    .member_filter()
                                    .passes(&entry.record.kind_of, &entry.record.name)
                            })
                })
                .count();
            if matching == 0 {
                continue;
            }
            total = total.saturating_add(u32::try_from(matching).unwrap_or(u32::MAX));
            found.push((horde, distance_squared <= rub_off * rub_off));
        }
        (total, found)
    }

    fn store_group_bonus(
        &self,
        tick: &mut Tick<'_>,
        wiring: &super::upgrades::ModuleWiring,
        entity: Entity,
        module: &LargeGroupBonusUpdate,
    ) {
        if let Some(column) = tick.state_mut().get_mut(wiring.group_bonus_state, entity) {
            if let Some(slot) = column.0.first_mut() {
                slot.state = module.clone();
            }
        }
    }
}
