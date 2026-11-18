use mint_sage_modules::ProductionUpdate;
use mint_sim::{Entity, Result, Tick};

use super::{Ids, RotwkRules};
use crate::diag::diag;

const EXIT_DOOR: usize = 0;

impl RotwkRules {
    pub(super) fn step_production_doors(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        let Some(wiring) = self.modules.as_ref() else {
            return Ok(());
        };
        let producers: Vec<Entity> = tick
            .state()
            .store(wiring.production_doors)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        let now = tick.frame();
        let mut effects = Vec::new();
        for producer in producers {
            let data = self.door_data(tick, producer);
            let Some(mut doors) = tick.state().get(wiring.production_doors, producer).cloned()
            else {
                continue;
            };
            let changes = data.map_or_else(Vec::new, |data| doors.step(&data, now));
            if data.is_none() || doors.is_closed() {
                tick.state_mut().remove(wiring.production_doors, producer);
            } else {
                tick.state_mut()
                    .insert(wiring.production_doors, producer, doors)?;
            }
            for change in changes {
                diag!(
                    "door: entity {} {change:?} at frame {}",
                    producer.index(),
                    now.get()
                );
                effects.push((producer, change));
            }
        }
        self.apply_effects(tick, ids, effects)
    }

    pub(super) fn through_production_door(
        &self,
        tick: &mut Tick<'_>,
        ids: Ids,
        producer: Entity,
    ) -> Result<bool> {
        let (Some(wiring), Some(_)) = (self.modules.as_ref(), self.door_data(tick, producer))
        else {
            return Ok(true);
        };
        let now = tick.frame();
        let mut doors = tick
            .state()
            .get(wiring.production_doors, producer)
            .cloned()
            .unwrap_or_default();
        let (may_leave, change) = doors.exit(EXIT_DOOR, now);
        tick.state_mut()
            .insert(wiring.production_doors, producer, doors)?;
        if let Some(change) = change {
            diag!(
                "door: entity {} {change:?} at frame {}",
                producer.index(),
                now.get()
            );
            self.apply_effects(tick, ids, vec![(producer, change)])?;
        }
        Ok(may_leave)
    }

    fn door_data(&self, tick: &Tick<'_>, producer: Entity) -> Option<ProductionUpdate> {
        let wiring = self.modules.as_ref()?;
        tick.state()
            .get(wiring.production_state, producer)?
            .0
            .first()
            .map(|slot| slot.state)
            .filter(ProductionUpdate::has_doors)
    }
}
