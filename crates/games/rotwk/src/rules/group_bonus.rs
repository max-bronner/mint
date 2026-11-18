use mint_sage_modules::LargeGroupBonusUpdate;
use mint_sim::{Component, Entity, Result, StateHasher, Tick, World};

use super::{Ids, RotwkRules};
use crate::combat::{grid_cell, grid_radius_cells, Foe, TargetGrid};
use crate::diag::diag;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GroupBonus;

impl Component for GroupBonus {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

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

    pub(super) fn step_group_bonuses(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        living: &[(Foe, mint_sim::PlayerId)],
        grid: &TargetGrid,
        global_max_radius: f32,
    ) -> Result<()> {
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
            module.scanned(now);
            self.store_group_bonus(tick, wiring, entity, &module);
            let held = tick.state().contains(ids.group_bonus, entity);
            let crowd = self.count_crowd(
                tick.state(),
                ids,
                &module,
                entity,
                held,
                living,
                grid,
                global_max_radius,
            );
            let enough = crowd >= module.count();
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
                crowd,
                module.reach(held),
                if enough { "gains" } else { "loses" },
                module.attribute_modifier().unwrap_or("no list"),
            );
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn count_crowd(
        &self,
        world: &World,
        ids: Ids,
        module: &LargeGroupBonusUpdate,
        entity: Entity,
        held: bool,
        living: &[(Foe, mint_sim::PlayerId)],
        grid: &TargetGrid,
        global_max_radius: f32,
    ) -> u32 {
        let reach = module.reach(held);
        let Some(position) = world.get(ids.body, entity).map(|body| body.0.position) else {
            return 0;
        };
        if reach.is_nan() || reach <= 0.0 {
            return 0;
        }
        let owner = world.get(ids.owner, entity).map(|owner| owner.0);
        let mut candidates: Vec<u32> = Vec::new();
        grid.query_into(
            grid_cell(position.x),
            grid_cell(position.y),
            grid_radius_cells(TargetGrid::search_radius(reach, 0.0, global_max_radius)),
            &mut candidates,
        );
        let mut count = 0u32;
        for index in candidates {
            let Some((foe, other_owner)) =
                usize::try_from(index).ok().and_then(|at| living.get(at))
            else {
                continue;
            };
            if foe.entity == entity {
                continue;
            }
            let (dx, dy) = (foe.position.x - position.x, foe.position.y - position.y);
            if dx * dx + dy * dy > reach * reach {
                continue;
            }
            if module.allies_only()
                && !owner.is_some_and(|mine| {
                    mine == *other_owner || self.object_allied(world, entity, foe.entity)
                })
            {
                continue;
            }
            let passes = world
                .get(ids.kind, foe.entity)
                .and_then(|kind| self.kinds.get(*kind))
                .is_some_and(|entry| {
                    module
                        .member_filter()
                        .passes(&entry.record.kind_of, &entry.record.name)
                });
            if passes {
                count = count.saturating_add(1);
            }
        }
        count
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
