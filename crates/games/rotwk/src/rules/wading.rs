use mint_movement::HeightRule;
use mint_sage_modules::effects::Effect;
use mint_sim::{Entity, Result, Tick};

use super::{Ids, RotwkRules};

const WADING: &str = "WADING";
const SWIMMING: &str = "SWIMMING";

fn wet_bands(depth: f32, wade: f32) -> (bool, bool) {
    (depth > 0.25 * wade, depth > wade)
}

impl RotwkRules {
    pub(super) fn update_wading(&self, tick: &mut Tick<'_>, ids: Ids) -> Result<()> {
        if !self.passability.has_water() {
            return Ok(());
        }
        let wade = self.passability.water_depths.wade;
        let mut changes: Vec<(Entity, Effect)> = Vec::new();
        let Some(bodies) = tick.state().store(ids.body) else {
            return Ok(());
        };
        for (entity, body) in bodies.iter() {
            let Some(flight) = tick
                .state()
                .get(ids.kind, entity)
                .and_then(|kind| self.kinds.get(*kind))
                .and_then(|entry| entry.flight)
            else {
                continue;
            };
            let floating = matches!(flight.height, HeightRule::Unchanged);
            if !flight.wades {
                continue;
            }
            if !floating && tick.state().contains(ids.held, entity) {
                continue;
            }
            let on_ground = tick
                .state()
                .get(ids.layer, entity)
                .is_none_or(|layer| *layer == crate::wall_layer::Layer::GROUND);
            let (wading, swimming) = if on_ground {
                wet_bands(self.passability.water_depth(body.0.position), wade)
            } else {
                (false, false)
            };
            let mut add = Vec::new();
            let mut remove = Vec::new();
            for (name, want) in [(WADING, wading), (SWIMMING, swimming)] {
                if self.holds_model_condition(tick, entity, name) != want {
                    if want {
                        add.push(name.to_owned());
                    } else {
                        remove.push(name.to_owned());
                    }
                }
            }
            if add.is_empty() && remove.is_empty() {
                continue;
            }
            changes.push((
                entity,
                Effect::ModelConditions {
                    add: mint_sage_modules::effects::names(add),
                    remove: mint_sage_modules::effects::names(remove),
                },
            ));
        }
        self.apply_effects(tick, ids, changes)
    }
}

#[cfg(test)]
mod tests {
    use super::wet_bands;

    #[test]
    fn the_water_column_sets_the_two_bands_at_a_quarter_and_at_the_full_wade_depth() {
        assert_eq!(wet_bands(1.0, 5.0), (false, false));
        assert_eq!(wet_bands(3.0, 5.0), (true, false));
        assert_eq!(wet_bands(5.0, 5.0), (true, false));
        assert_eq!(wet_bands(6.0, 5.0), (true, true));
    }
}
