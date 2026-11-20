use mint_sim::StateHasher;

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HoldFire;

impl Component for HoldFire {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl RotwkRules {
    pub(super) fn holds_fire(world: &World, ids: Ids, entity: Entity) -> bool {
        world.contains(ids.hold_fire, entity)
    }

    pub(super) fn toggle_hold_fire(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        player: PlayerId,
        lead: Option<Entity>,
    ) -> Result<()> {
        let mut units: Vec<Entity> = self
            .selection(tick, ids, player)
            .into_iter()
            .filter(|entity| {
                tick.state()
                    .get(ids.owner, *entity)
                    .is_some_and(|owner| owner.0 == player)
            })
            .map(|entity| Self::commanded_unit(tick.state(), ids, entity))
            .collect();
        units.sort_unstable_by_key(|entity| entity.index());
        units.dedup();
        let lead = lead
            .map(|lead| Self::commanded_unit(tick.state(), ids, lead))
            .or_else(|| units.first().copied());
        let Some(lead) = lead else {
            return Ok(());
        };
        let on = !Self::holds_fire(tick.state(), ids, lead);
        for unit in units {
            let members = tick
                .state()
                .get(ids.horde, unit)
                .map(|horde| horde.members.clone())
                .unwrap_or_default();
            for entity in std::iter::once(unit).chain(members) {
                if on {
                    tick.state_mut().insert(ids.hold_fire, entity, HoldFire)?;
                } else {
                    tick.state_mut().remove(ids.hold_fire, entity);
                }
            }
        }
        Ok(())
    }
}
