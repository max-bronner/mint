use mint_sage_passability::{Footprint, FootprintRecord};

use crate::unit::Kind;
use crate::wall_layer::Layer;
use crate::wall_meshes::{RampMesh, WallBounds};

use super::*;

fn cliff_block_map(
    x: std::ops::RangeInclusive<u32>,
    y: std::ops::RangeInclusive<u32>,
) -> HeightMapData {
    let mut map = flat_map(32, 32);
    for sy in y {
        for sx in x.clone() {
            if (sx + sy) % 2 == 0 {
                map.samples[(sy * 32 + sx) as usize] = 400;
            }
        }
    }
    map
}

#[test]
fn a_goal_on_a_cliff_backed_wall_top_is_reached_up_its_ramp() {
    let map = cliff_block_map(11..=14, 4..=7);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();

    let walker = spawn_at(20.0, 50.0, KindOf::SELECTABLE, PLAYER);
    let ramp_holder = Spawn {
        kind: Some(Kind(0)),
        ..spawn_at(0.0, 0.0, KindOf::STRUCTURE, PLAYER)
    };
    let wall = Spawn {
        kind: Some(Kind(1)),
        ..spawn_at(0.0, 0.0, KindOf::STRUCTURE, PLAYER)
    };
    let under_ramp = vec![FootprintRecord {
        offset: Vec2::new(75.0, 50.0),
        footprint: Footprint::Box {
            major: 20.0,
            minor: 8.0,
        },
    }];
    let ramp_mesh = RampMesh {
        from_left: (48.0, 38.0, 0.0),
        from_right: (48.0, 62.0, 0.0),
        to_left: (103.0, 38.0, 30.0),
        to_right: (103.0, 62.0, 30.0),
    };
    let wall_bounds = WallBounds {
        perimeter: vec![(102.0, 32.0), (158.0, 32.0), (158.0, 78.0), (102.0, 78.0)],
        height: 30.0,
    };
    let rules = RotwkRules::new(passability, vec![walker, ramp_holder, wall])
        .with_structure_footprints(vec![(1, under_ramp), (2, Vec::new())])
        .with_wall_bounds(vec![None, Some(wall_bounds)])
        .with_ramp_meshes(vec![vec![ramp_mesh], Vec::new()]);
    let mut simulation = Simulation::new(13, rules).unwrap();
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
    let goal = Vec2::new(130.0, 55.0);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: goal.x,
                y: goal.y,
                queue: false,
            },
        )
        .unwrap();
    let mut layers: Vec<Layer> = Vec::new();
    for _ in 0..600 {
        simulation.tick().unwrap();
        let layer = simulation.state().get(ids.layer, unit).copied();
        if let Some(layer) = layer {
            if layers.last() != Some(&layer) {
                layers.push(layer);
            }
        }
    }
    let at = position(&simulation, ids, unit);
    assert!(
        at.minus(goal).length() < 10.0,
        "the unit stopped at {at:?}, short of {goal:?}; layers {layers:?}"
    );
    assert_eq!(
        layers,
        vec![Layer::GROUND, Layer::RAMPS, Layer(17)],
        "ground, then the ramp, then the wall top"
    );
}

#[test]
fn a_wall_stamped_mid_match_rebuilds_the_reachability_oracle() {
    let map = cliff_block_map(11..=14, 4..=7);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new());
    let geometry = rules.passability.geometry;
    let extent = rules.passability.grid.extent();
    let start = Cell::new(2, 2);
    let goal = Cell::new(13, 5);
    assert!(
        !rules.reachable(GROUND, start, goal),
        "the cliff block refuses the goal before any wall exists"
    );
    let mut world = World::new();
    let wall = world.spawn().unwrap();
    let mut layer =
        crate::wall_layer::WallLayer::empty(usize::try_from(extent.cell_count()).unwrap());
    layer.stamp(
        geometry,
        extent,
        wall,
        crate::wall_layer::Polygon {
            perimeter: vec![
                Vec2::new(102.0, 32.0),
                Vec2::new(158.0, 32.0),
                Vec2::new(158.0, 78.0),
                Vec2::new(102.0, 78.0),
            ],
            height: 30.0,
        },
        &[],
    );
    rules.rebuild_reachability(&layer, &[]);
    assert!(
        rules.reachable(GROUND, start, goal),
        "the wall top clears the cliff under its own goal cell, and the oracle does not gate \
         on the rim"
    );
    let ramp_holder = world.spawn().unwrap();
    let ramp = crate::wall_meshes::RampMesh {
        from_left: (48.0, 38.0, 0.0),
        from_right: (48.0, 62.0, 0.0),
        to_left: (103.0, 38.0, 30.0),
        to_right: (103.0, 62.0, 30.0),
    };
    let placed = ramp.placed(Vec2::new(0.0, 0.0), 0.0, 0.0);
    let _diags = layer.stamp_resolved_ramps(
        geometry,
        extent,
        ramp_holder,
        &rules.passability.heights,
        &[placed],
        &[],
    );
    rules.rebuild_reachability(&layer, &[]);
    assert!(
        rules.reachable(GROUND, start, goal),
        "the mid-match ramp stamp must not have broken reachability, and the oracle must still \
         commit on this later rebuild"
    );
}

#[test]
fn rebuild_reachability_excuses_a_scaleable_obstacle_for_its_own_mover() {
    let map = flat_map(64, 16);
    let obstacle = Obstacle {
        id: 0,
        centre: Vec2::new(100.0, 50.0),
        angle: 0.0,
        records: vec![FootprintRecord {
            offset: Vec2::ZERO,
            footprint: Footprint::Box {
                major: 40.0,
                minor: 100.0,
            },
        }],
    };
    let passability = Passability::builder(&map, 5.0 / 128.0)
        .obstacles(std::slice::from_ref(&obstacle))
        .build()
        .unwrap();
    let geometry = passability.geometry;
    let extent = passability.grid.extent();
    let cell_count = usize::try_from(extent.cell_count()).unwrap();
    let climber = mint_sage_passability::mover_class(
        &[String::from("GROUND"), String::from("OBSTACLE")],
        true,
    )
    .unwrap();
    let start = passability.cell_at(Vec2::new(20.0, 50.0)).unwrap();
    let goal = passability.cell_at(Vec2::new(200.0, 50.0)).unwrap();
    let rules = RotwkRules::new(passability, Vec::new());
    let bare_view = DynamicView {
        grid: &rules.passability.grid,
        counts: &[],
        rubble: &[],
        wall_rim: &[],
        tags: &[],
        connect: &[],
        released: &[],
        closed: &[],
        obstacle_pinched: &[],
        ramp_side: &[],
        scaleable: &[],
        barred: &[],
    };
    *rules.reachability_extra.borrow_mut() =
        vec![(climber.terrain(), Reachability::build(&bare_view, climber))];
    assert!(
        !rules.reachability_extra.borrow()[0]
            .1
            .same_component(start, goal),
        "the static obstacle must split the grid before any scaleable bit is set"
    );
    let layer = crate::wall_layer::WallLayer::empty(cell_count);
    let mut scaleable = vec![0u8; cell_count];
    for cell in mint_sage_passability::covered_cells(geometry, extent, &obstacle) {
        if let Some(index) = extent.index(cell).and_then(|i| usize::try_from(i).ok()) {
            scaleable[index] = 1;
        }
    }
    rules.rebuild_reachability(&layer, &scaleable);
    assert!(
        rules.reachability_extra.borrow()[0]
            .1
            .same_component(start, goal),
        "a ScalesWalls mover's own oracle must be excused across a dynamically scaleable \
         obstacle, the same cells the search itself is excused across"
    );
}
