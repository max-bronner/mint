use super::*;
use crate::rules::{Ids, RotwkCommand};
use crate::unit::{Shape, Spawn};
use mint_movement::Facing;
use mint_sage_asset::KindOf;
use mint_sage_map::HeightMapData;
use mint_sage_passability::Passability;

const PLAYER: PlayerId = PlayerId::new(0);

fn walked() -> (Simulation<RotwkRules>, PoseTrack, Ids, Entity) {
    let map = HeightMapData {
        width: 48,
        height: 48,
        border_size: 0,
        boundaries: Vec::new(),
        samples: vec![0; 48 * 48],
    };
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let shape = |kind_of, radius| Shape {
        kind_of,
        radius,
        height: 18.0,
        geometry_is_small: false,
    };
    let spawns = vec![
        Spawn::inert(
            Vec2::new(55.0, 55.0),
            Facing::EAST,
            shape(KindOf::SELECTABLE, 5.0),
            PLAYER,
        ),
        Spawn::inert(
            Vec2::new(85.0, 55.0),
            Facing::EAST,
            shape(KindOf::SHRUBBERY, 6.0),
            PLAYER,
        ),
    ];
    let mut simulation = Simulation::new(3, RotwkRules::new(passability, spawns)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let unit = simulation
        .state()
        .store(ids.body)
        .and_then(|store| store.entities().first().copied())
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![unit],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 300.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    let mut previous = PoseTrack::new();
    for _ in 0..3 {
        render_state::step_remembering(&mut simulation, &mut previous, ids).unwrap();
    }
    (simulation, previous, ids, unit)
}

fn per_consumer(
    simulation: &Simulation<RotwkRules>,
    previous: &PoseTrack,
    ids: Ids,
    blend: Option<f32>,
) -> Vec<UnitView> {
    let world = simulation.state();
    let rules = simulation.rules();
    let state = match blend {
        Some(blend) => RenderState::interpolated(world, previous, blend),
        None => RenderState::new(world, 0.0),
    };
    let mut units = state.units(
        ids,
        PLAYER,
        PLAYER,
        |weapon, state, moved, now, conditions| {
            rules.weapon_cycle_seconds(weapon, state, moved, now, conditions)
        },
        |weapon| rules.weapon_firing_duration(weapon),
    );
    render_state::apply_disguise(&mut units, world, rules, PLAYER);
    units
}

#[test]
fn a_snapshot_is_a_pure_function_of_the_tick() {
    let (simulation, previous, ..) = walked();
    let first = FrameSnapshot::build(&simulation, &previous, PLAYER, PLAYER);
    let second = FrameSnapshot::build(&simulation, &previous, PLAYER, PLAYER);
    assert_eq!(first, second);
    assert!(first.is_for(simulation.frame(), PLAYER, PLAYER));
    assert!(!first.is_for(simulation.frame(), PlayerId::new(1), PLAYER));
}

#[test]
fn every_blend_of_the_snapshot_equals_the_per_consumer_build() {
    let (simulation, previous, ids, unit) = walked();
    let snapshot = FrameSnapshot::build(&simulation, &previous, PLAYER, PLAYER);
    for blend in [0.0, 0.25, 0.5, 0.999, 1.0, -1.0, 2.0, f32::NAN] {
        let ours = snapshot.units(Some(blend));
        assert_eq!(ours, per_consumer(&simulation, &previous, ids, Some(blend)));
    }
    assert_eq!(
        snapshot.units(None),
        per_consumer(&simulation, &previous, ids, None)
    );
    let at = |blend| {
        snapshot
            .units(blend)
            .into_iter()
            .find(|view| view.entity == unit)
            .unwrap()
            .position
    };
    assert_ne!(at(Some(0.5)), at(None));
}

#[test]
fn the_snapshot_carries_what_the_hud_reads() {
    let (simulation, previous, ids, unit) = walked();
    let world = simulation.state();
    let snapshot = FrameSnapshot::build(&simulation, &previous, PLAYER, PLAYER);
    let units = snapshot.units(Some(0.5));
    let selected: Vec<Entity> = units
        .iter()
        .filter(|u| u.selected)
        .map(|u| u.entity)
        .collect();
    assert_eq!(selected, vec![unit]);
    let view = units.iter().find(|u| u.entity == unit).unwrap();
    assert_eq!(view.kind, world.get(ids.kind, unit).map(|kind| kind.0));
    assert_eq!(view.told_kind, view.kind);
    assert_eq!(
        view.health,
        world
            .get(ids.health, unit)
            .map(|health| health.0.fraction())
    );
    assert_eq!(snapshot.selected_by_local(), 1);
    assert_eq!(
        snapshot.script_view(),
        &simulation.rules().script_view(world)
    );
    assert_eq!(
        snapshot.facts(unit).map(|facts| facts.geometry_kind),
        Some(RotwkRules::geometry_kind(world, ids, unit))
    );
}

#[test]
fn a_snapshot_can_cross_to_another_thread() {
    fn handed_over<T: Send + Sync + 'static>() {}
    handed_over::<FrameSnapshot>();
}
