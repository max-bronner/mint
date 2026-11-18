use mint_sim::{Entity, Result, Tick};

use super::upgrades::ModuleWiring;
use super::{Ids, RotwkRules};
use crate::construction::shown_footprint;
use crate::templates::pick_shape;

impl RotwkRules {
    pub(super) fn reshape_geometry(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        wiring: &ModuleWiring,
        entity: Entity,
    ) -> Result<()> {
        let Some(entry) = tick
            .state()
            .get(ids.kind, entity)
            .copied()
            .and_then(|kind| self.kinds.get(kind))
        else {
            return Ok(());
        };
        let geometry = &entry.record.geometry;
        let overrides = tick
            .state()
            .get(wiring.state, entity)
            .map(|state| state.geometry.clone())
            .unwrap_or_default();
        if let Some(shape) = tick.state().get(ids.shape, entity).copied() {
            let reshaped = pick_shape(
                shape.kind_of,
                geometry.shown_radius(&overrides),
                geometry.shown_height(&overrides),
                entry.record.geometry_is_small.unwrap_or(false),
            );
            if reshaped != shape {
                tick.state_mut().insert(ids.shape, entity, reshaped)?;
            }
        }
        let stamped = tick.state().store(ids.obstacles).is_some_and(|store| {
            store
                .entities()
                .first()
                .and_then(|clock| tick.state().get(ids.obstacles, *clock))
                .is_some_and(|layer| layer.holds(entity))
        });
        let Some(body) = tick.state().get(ids.body, entity).map(|body| body.0) else {
            return Ok(());
        };
        if stamped {
            self.stamp_footprint(
                tick,
                ids,
                entity,
                body.position,
                body.facing.radians(),
                shown_footprint(&entry.record, &overrides),
            )?;
        }
        Ok(())
    }
}
