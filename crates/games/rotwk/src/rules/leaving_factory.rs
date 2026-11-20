use mint_sage_modules::effects::{names, Effect};
use mint_sim::{Component, Entity, Frame, LogicFrames, Result, StateHasher, Tick};

use super::{Ids, RotwkRules};

const STATUS: &str = "IS_LEAVING_FACTORY";

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FactoryExit(pub(crate) Frame);

impl Component for FactoryExit {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.0.hash_into(hasher);
    }
}

impl RotwkRules {
    pub(super) fn begin_factory_exit(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Result<()> {
        self.apply_effects(
            tick,
            ids,
            vec![(
                entity,
                Effect::StatusBits {
                    set: names(vec![STATUS.to_owned()]),
                    clear: names(Vec::new()),
                },
            )],
        )?;
        if tick.state().contains(ids.horde, entity) {
            let now = tick.frame();
            tick.state_mut()
                .insert(ids.factory_exit, entity, FactoryExit(now))?;
        }
        Ok(())
    }

    pub(super) fn end_factory_exit(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Result<()> {
        tick.state_mut().remove(ids.factory_exit, entity);
        if !self.carries_object_status(tick.state(), entity, STATUS) {
            return Ok(());
        }
        self.apply_effects(
            tick,
            ids,
            vec![(
                entity,
                Effect::StatusBits {
                    set: names(Vec::new()),
                    clear: names(vec![STATUS.to_owned()]),
                },
            )],
        )
    }

    pub(super) fn order_ends_factory_exit(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Result<()> {
        let world = tick.state();
        if world.contains(ids.horde, entity) {
            let garrisoned = world.get(ids.contained, entity).is_some_and(|held| {
                self.contain_view(world, held.container)
                    .is_some_and(|view| view.garrison_family)
            });
            if garrisoned {
                return Ok(());
            }
        }
        self.end_factory_exit(tick, ids, entity)
    }

    pub(super) fn settle_factory_exits(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let now = tick.frame();
        let second = LogicFrames::from_engine_milliseconds(1000);
        let world = tick.state();
        let settled: Vec<Entity> = world
            .store(ids.factory_exit)
            .map(|store| {
                store
                    .iter()
                    .filter(|(entity, made)| {
                        !world.contains(ids.path, *entity)
                            && made.0.checked_add(second).is_some_and(|due| now > due)
                            && !self.member_leaving_factory(world, ids, *entity)
                    })
                    .map(|(entity, _)| entity)
                    .collect()
            })
            .unwrap_or_default();
        for entity in settled {
            self.end_factory_exit(tick, ids, entity)?;
        }
        Ok(())
    }

    fn member_leaving_factory(&self, world: &mint_sim::World, ids: Ids, horde: Entity) -> bool {
        let streaming = world
            .get(ids.production_stream, horde)
            .is_some_and(|stream| stream.remaining > 0);
        let Some(contain) = world.get(ids.horde, horde) else {
            return true;
        };
        streaming
            || contain.members.is_empty()
            || contain
                .members
                .iter()
                .any(|member| self.carries_object_status(world, *member, STATUS))
    }
}
