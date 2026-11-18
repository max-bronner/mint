use mint_sage_passability::{Footprint, FootprintRecord};

use crate::unit::Kind;
use crate::wall_layer::Layer;
use crate::wall_meshes::{RampMesh, WallBounds};

use super::*;

fn small_footprint() -> Vec<FootprintRecord> {
    vec![FootprintRecord {
        offset: Vec2::ZERO,
        footprint: Footprint::Box {
            major: 4.0,
            minor: 4.0,
        },
    }]
}

#[test]
fn a_ramp_whose_far_end_needs_a_later_spawned_wall_still_resolves() {
    let map = flat_map(32, 32);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let geometry = passability.geometry;
    let extent = passability.grid.extent();

    let ramp_holder = Spawn {
        kind: Some(Kind(0)),
        ..spawn_at(0.0, 0.0, KindOf::STRUCTURE, PLAYER)
    };
    let wall = Spawn {
        kind: Some(Kind(1)),
        ..spawn_at(0.0, 0.0, KindOf::STRUCTURE, PLAYER)
    };
    let ramp_mesh = RampMesh {
        from_left: (50.0, 45.0, 0.0),
        from_right: (50.0, 55.0, 0.0),
        to_left: (95.0, 45.0, 30.0),
        to_right: (95.0, 55.0, 30.0),
    };
    let wall_bounds = WallBounds {
        perimeter: vec![(100.0, 40.0), (130.0, 40.0), (130.0, 60.0), (100.0, 60.0)],
        height: 30.0,
    };

    let rules = RotwkRules::new(passability, vec![ramp_holder, wall])
        .with_structure_footprints(vec![(0, small_footprint()), (1, small_footprint())])
        .with_wall_bounds(vec![None, Some(wall_bounds)])
        .with_ramp_meshes(vec![vec![ramp_mesh], Vec::new()]);
    let simulation = Simulation::new(11, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let clock = simulation
        .state()
        .store(ids.wall_layer)
        .and_then(|store| store.entities().first().copied())
        .expect("the clock entity carries a wall layer");
    let layer = simulation.state().get(ids.wall_layer, clock).unwrap();

    let on_ramp = Vec2::new(72.0, 50.0);
    assert_eq!(
        layer.layer_at(geometry, extent, on_ramp),
        Layer::RAMPS,
        "the ramp's far end needed entity 1's own wall tag, spawned after it; the setup loop \
         must stamp every wall before resolving any ramp"
    );
}
