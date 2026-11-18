use std::collections::{HashMap, HashSet};

use mint_render::{Renderer, Vec3};
use mint_sage_modules::DamageFxLocation;
use mint_sage_scene::Conditions;
use mint_sim::Entity;

use crate::game::MatchState;
use crate::render_state::UnitView;

#[derive(Debug, Default)]
pub(crate) struct TdfxPresenter {
    last_rung: HashMap<Entity, u8>,
    running: HashMap<Entity, Vec<mint_sage_particle::SystemId>>,
}

impl TdfxPresenter {
    fn prune(&mut self, present: &HashSet<Entity>) -> Vec<mint_sage_particle::SystemId> {
        self.last_rung.retain(|entity, _| present.contains(entity));
        let mut stopped = Vec::new();
        self.running.retain(|entity, systems| {
            if present.contains(entity) {
                true
            } else {
                stopped.append(systems);
                false
            }
        });
        stopped
    }
}

fn rung_of(dying: bool, conditions: Conditions) -> u8 {
    if dying {
        3
    } else if conditions.contains(Conditions::REALLYDAMAGED) {
        2
    } else if conditions.contains(Conditions::DAMAGED) {
        1
    } else {
        0
    }
}

fn slot_offset(location: &DamageFxLocation) -> Vec3 {
    match location {
        DamageFxLocation::Coord(x, y, z) => Vec3::new(*x, *y, *z),
        DamageFxLocation::Bone { .. } => Vec3::ZERO,
    }
}

fn slot_point(caller: &mint_sage_scene::UnitDrawable, location: &DamageFxLocation) -> Vec3 {
    caller.placement().transform_point(slot_offset(location))
}

pub(crate) fn apply_transition_damage_fx(
    renderer: &mut Renderer,
    state: &mut MatchState,
    units: &[UnitView],
) {
    let present: HashSet<Entity> = units.iter().map(|unit| unit.entity).collect();
    for system in state.tdfx.prune(&present) {
        state.scene.particle_scene.stop(system);
    }
    let rules = state.simulation.rules();
    let world = state.simulation.state();
    let mut edges = Vec::new();
    for unit in units {
        let new_rung = rung_of(unit.dying, unit.conditions);
        let old_rung = state
            .tdfx
            .last_rung
            .insert(unit.entity, new_rung)
            .unwrap_or(new_rung);
        if old_rung == new_rung {
            continue;
        }
        let Some(tdfx) = rules.tdfx_view(world, unit.entity) else {
            continue;
        };
        let outcome =
            mint_sage_modules::transition_damage_fx_transitions(old_rung, new_rung, None, tdfx);
        edges.push((unit.entity, outcome));
    }
    if edges.is_empty() {
        return;
    }
    let mut fired = 0u64;
    let gate = crate::game::FxShroud::of(state);
    for (entity, outcome) in edges {
        if let Some(running) = state.tdfx.running.remove(&entity) {
            for system in running {
                state.scene.particle_scene.stop(system);
            }
        }
        let Some(caller) = state.units.drawable_by_id(entity.index()) else {
            continue;
        };
        let mut running = Vec::new();
        if let Some(fx) = outcome.fx.as_ref().filter(|fx| {
            !gate.skips(
                &state.simulation,
                &state.scene.particle_scene,
                &fx.name,
                None,
                slot_point(caller, &fx.location),
            )
        }) {
            let at = slot_point(caller, &fx.location);
            let ground = mint_sage_scene::MapGround::new(
                &state.scene.height_map,
                state.scene.terrain.vertical_scale,
            );
            let is_on_water = |x: f32, y: f32| {
                mint_sage_scene::water_present::is_over_water(
                    &state.scene.wet,
                    &state.scene.height_map,
                    x,
                    y,
                )
            };
            let weather = if state.scene.snowy {
                mint_sage_ini::NuggetWeather::Snowy
            } else {
                mint_sage_ini::NuggetWeather::Normal
            };
            if state
                .scene
                .particle_scene
                .fire_list(
                    renderer,
                    &state.vfs,
                    &state.scene.resolver,
                    &fx.name,
                    at,
                    None,
                    &ground,
                    Some(caller),
                    None,
                    &is_on_water,
                    weather,
                )
                .is_some()
            {
                fired = fired.saturating_add(1);
            }
        }
        if let Some(particles) = &outcome.particles {
            if let Some(system) = state.scene.particle_scene.attach_named_to_unit(
                renderer,
                &state.vfs,
                &state.scene.resolver,
                &particles.name,
                caller,
                slot_offset(&particles.location),
            ) {
                running.push(system);
                fired = fired.saturating_add(1);
            }
        }
        if !running.is_empty() {
            state.tdfx.running.insert(entity, running);
        }
    }
    if fired > 0 && crate::diag::wants("tdfx") {
        println!("tdfx: {fired} fx/particle slot(s) fired this frame");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dying_unit_reads_the_rubble_rung_regardless_of_its_conditions() {
        assert_eq!(rung_of(true, Conditions::NONE), 3);
        assert_eq!(rung_of(true, Conditions::REALLYDAMAGED), 3);
        assert_eq!(rung_of(false, Conditions::REALLYDAMAGED), 2);
        assert_eq!(rung_of(false, Conditions::DAMAGED), 1);
        assert_eq!(rung_of(false, Conditions::NONE), 0);
    }

    #[test]
    fn prune_forgets_entities_gone_from_present() {
        let mut presenter = TdfxPresenter::default();
        let alive = Entity::from_parts(1, 1);
        let gone = Entity::from_parts(2, 1);
        presenter.last_rung.insert(alive, 2);
        presenter.last_rung.insert(gone, 2);
        presenter.running.insert(gone, Vec::new());

        let present: HashSet<Entity> = [alive].into_iter().collect();
        let stopped = presenter.prune(&present);

        assert!(stopped.is_empty());
        assert!(presenter.last_rung.contains_key(&alive));
        assert!(!presenter.last_rung.contains_key(&gone));
        assert!(!presenter.running.contains_key(&gone));
    }
}
