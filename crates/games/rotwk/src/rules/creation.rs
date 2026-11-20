use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Created(pub(crate) u64);

impl Component for Created {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u64(self.0);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CreationCount(pub(crate) u64);

impl Component for CreationCount {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u64(self.0);
    }
}

impl RotwkRules {
    pub(super) fn stamp_creation(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        entity: Entity,
    ) -> Result<()> {
        if !tick.state().is_live(entity) {
            return Ok(());
        }
        let Some(clock) = tick
            .state()
            .store(ids.clock)
            .and_then(|store| store.entities().first().copied())
        else {
            return Ok(());
        };
        let next = tick
            .state()
            .get(ids.creation_count, clock)
            .map_or(0, |count| count.0)
            .saturating_add(1);
        tick.state_mut()
            .insert(ids.creation_count, clock, CreationCount(next))?;
        tick.state_mut()
            .insert(ids.created, entity, Created(next))?;
        Ok(())
    }
}
