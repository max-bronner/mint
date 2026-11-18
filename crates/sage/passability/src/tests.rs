use super::*;
use mint_sage_map::{Flammability, StandingWaterArea, Water, WaterAreas};

pub(crate) fn height_field(width: u32, height: u32, f: impl Fn(u32, u32) -> u16) -> HeightMapData {
    let mut samples = Vec::new();
    for y in 0..height {
        for x in 0..width {
            samples.push(f(x, y));
        }
    }
    HeightMapData {
        width,
        height,
        border_size: 0,
        boundaries: Vec::new(),
        samples,
    }
}

const UNIT_SCALE: f32 = 1.0;

fn single(footprint: Footprint) -> Vec<FootprintRecord> {
    vec![FootprintRecord {
        offset: Vec2::new(0.0, 0.0),
        footprint,
    }]
}

fn lake(points: Vec<[f32; 2]>, water_height: u32) -> Water {
    Water {
        standing: Some(WaterAreas {
            version: 2,
            areas: vec![StandingWaterArea {
                unique_id: 1,
                name: String::new(),
                layer_name: String::new(),
                uv_scroll_speed: 0.0,
                additive_blend: false,
                bump_texture: String::new(),
                sky_texture: String::new(),
                points,
                water_height,
                fx_shader: String::new(),
                depth_colors: String::new(),
            }],
        }),
        rivers: None,
        waves: None,
    }
}

#[test]
fn the_pinched_cost_is_one_orthogonal_step() {
    assert_eq!(u32::from(PINCHED_COST), mint_movement::COST_ORTHOGONAL);
}

#[test]
fn the_ford_cost_is_one_orthogonal_step() {
    assert_eq!(u32::from(FORD_COST), mint_movement::COST_ORTHOGONAL);
}

#[test]
fn flat_ground_blocks_nothing_and_costs_nothing_extra() {
    let hm = height_field(16, 16, |_, _| 500);
    let pass = Passability::build(&hm, UNIT_SCALE).unwrap();
    for y in 0..15u16 {
        for x in 0..15u16 {
            assert!(pass.grid.is_passable(Cell::new(x, y), GROUND));
            assert_eq!(pass.grid.extra_cost(Cell::new(x, y)), 0);
        }
    }
    assert_eq!(pass.blocked_cells(), 0);
}

#[test]
fn a_step_below_the_limit_is_walkable_and_one_above_it_is_a_cliff() {
    for (step, blocked) in [(9u16, false), (11u16, true)] {
        let hm = height_field(16, 16, |x, _| if x >= 8 { step } else { 0 });
        let pass = Passability::build(&hm, UNIT_SCALE).unwrap();
        assert_eq!(
            !pass.grid.is_passable(Cell::new(7, 4), GROUND),
            blocked,
            "a step of {step} world units"
        );
    }
}

#[test]
fn a_cliff_cell_blocks_its_neighbours_too_and_charges_the_ring_beyond() {
    let hm = height_field(16, 16, |x, y| if x == 8 && y == 8 { 400 } else { 0 });
    let pass = Passability::build(&hm, UNIT_SCALE).unwrap();
    for (x, y) in [(7u16, 7u16), (8, 7), (7, 8), (8, 8)] {
        assert!(
            !pass.grid.is_passable(Cell::new(x, y), GROUND),
            "cell ({x}, {y}) straddles the step and should be a cliff"
        );
    }
    assert!(!pass.grid.is_passable(Cell::new(6, 6), GROUND));
    assert!(!pass.grid.is_passable(Cell::new(9, 9), GROUND));
    assert!(pass.grid.is_passable(Cell::new(5, 5), GROUND));
    assert_eq!(
        pass.grid.extra_cost(Cell::new(5, 5)),
        u32::from(PINCHED_COST)
    );
    assert!(pass.grid.flags(Cell::new(5, 5)).intersects(PINCHED));
    assert!(pass.grid.is_passable(Cell::new(1, 1), GROUND));
    assert_eq!(pass.grid.extra_cost(Cell::new(1, 1)), 0);
}

#[test]
fn the_vertical_scale_decides_whether_a_raw_step_is_a_cliff() {
    let hm = height_field(12, 12, |x, _| if x >= 6 { 200 } else { 0 });
    let shipped_scale = 5.0 / 128.0;
    let gentle = Passability::build(&hm, shipped_scale).unwrap();
    assert!(gentle.grid.is_passable(Cell::new(5, 4), GROUND));
    let steep = Passability::build(&hm, UNIT_SCALE).unwrap();
    assert!(!steep.grid.is_passable(Cell::new(5, 4), GROUND));
}

#[test]
fn the_border_ring_offsets_the_grid_the_way_the_terrain_is_offset() {
    let mut hm = height_field(16, 16, |_, _| 0);
    hm.border_size = 3;
    let pass = Passability::build(&hm, UNIT_SCALE).unwrap();
    assert_eq!(pass.cell_at(Vec2::new(5.0, 5.0)), Some(Cell::new(3, 3)));
    assert_eq!(pass.cell_at(Vec2::new(-5.0, -5.0)), Some(Cell::new(2, 2)));
    let centre = pass.geometry.centre_of(Cell::new(3, 3));
    assert!((centre.x - 5.0).abs() < 1e-4 && (centre.y - 5.0).abs() < 1e-4);
}

#[test]
fn a_height_field_claiming_a_huge_grid_is_refused_rather_than_allocated() {
    let hostile = HeightMapData {
        width: 65_535,
        height: 65_535,
        border_size: 0,
        boundaries: Vec::new(),
        samples: Vec::new(),
    };
    let error = Passability::build(&hostile, UNIT_SCALE).unwrap_err();
    assert!(error.contains("limit"), "{error}");
}

#[test]
fn a_truncated_height_field_builds_a_grid_instead_of_panicking() {
    for kept in [0usize, 1, 17, 100, 255] {
        let mut truncated = height_field(16, 16, |x, _| u16::try_from(x * 400).unwrap_or(0));
        truncated.samples.truncate(kept);
        let grid = Passability::build(&truncated, UNIT_SCALE)
            .unwrap_or_else(|e| panic!("{kept} sample(s): {e}"));
        assert_eq!(grid.grid.extent().width, 15);
        for y in 0..15u16 {
            for x in 0..15u16 {
                let _ = grid.grid.is_passable(Cell::new(x, y), GROUND);
                let _ = grid.grid.extra_cost(Cell::new(x, y));
            }
        }
    }
}

#[test]
fn a_truncated_height_field_survives_every_source_at_once() {
    let obstacles = [Obstacle {
        id: 0,
        centre: Vec2::new(60.0, 60.0),
        angle: 0.7,
        records: single(Footprint::Box {
            major: 25.0,
            minor: 15.0,
        }),
    }];
    for kept in [0usize, 1, 17, 100, 255] {
        let mut truncated = height_field(16, 16, |x, _| u16::try_from(x * 400).unwrap_or(0));
        truncated.samples.truncate(kept);
        truncated.border_size = 2;
        let water = lake(
            vec![[0.0, 0.0], [140.0, 0.0], [140.0, 140.0], [0.0, 140.0]],
            60,
        );
        let grid = Passability::builder(&truncated, UNIT_SCALE)
            .water(&water, WaterDepths::SHIPPED)
            .obstacles(&obstacles)
            .playable_border(true)
            .build()
            .unwrap_or_else(|e| panic!("{kept} sample(s): {e}"));
        for y in 0..15u16 {
            for x in 0..15u16 {
                let _ = grid.grid.is_passable(Cell::new(x, y), GROUND);
                let _ = grid.obstacle_at(Cell::new(x, y));
            }
        }
        let _ = grid.report();
    }
}

#[test]
fn a_height_field_of_extremes_blocks_everything_and_still_terminates() {
    let noisy = height_field(20, 20, |x, y| if (x + y) % 2 == 0 { 0 } else { u16::MAX });
    let grid = Passability::build(&noisy, UNIT_SCALE).unwrap();
    for y in 0..19u16 {
        for x in 0..19u16 {
            assert!(!grid.grid.is_passable(Cell::new(x, y), GROUND));
        }
    }
    assert!(grid.report().contains("0 pinched"), "{}", grid.report());
}

#[test]
fn a_non_finite_vertical_scale_is_treated_as_flat_rather_than_as_nan() {
    let steep = height_field(12, 12, |x, _| u16::try_from(x * 1000).unwrap_or(0));
    for scale in [f32::NAN, f32::INFINITY, -f32::INFINITY] {
        let grid = Passability::build(&steep, scale).unwrap();
        assert!(
            grid.grid.is_passable(Cell::new(5, 5), GROUND),
            "scale {scale} should read as flat, not as a cliff"
        );
    }
    let mirrored = Passability::build(&steep, -1.0).unwrap();
    assert!(!mirrored.grid.is_passable(Cell::new(5, 5), GROUND));
}

#[test]
fn a_height_field_too_small_to_hold_a_cell_is_refused() {
    let hm = height_field(1, 1, |_, _| 0);
    assert!(Passability::build(&hm, UNIT_SCALE).is_err());
}

#[test]
fn deep_water_blocks_a_ground_unit_and_the_cliff_only_grid_does_not() {
    let hm = height_field(20, 20, |_, _| 0);
    let water = lake(
        vec![[45.0, 45.0], [145.0, 45.0], [145.0, 145.0], [45.0, 145.0]],
        50,
    );
    let dry = Passability::build(&hm, UNIT_SCALE).unwrap();
    assert!(dry.grid.is_passable(Cell::new(10, 10), GROUND));
    let wet = Passability::builder(&hm, UNIT_SCALE)
        .water(&water, WaterDepths::SHIPPED)
        .build()
        .unwrap();
    assert!(!wet.grid.is_passable(Cell::new(10, 10), GROUND));
    assert!(wet.grid.flags(Cell::new(10, 10)).intersects(DEEP_WATER));
    assert!(wet.grid.is_passable(Cell::new(1, 1), GROUND));
}

#[test]
fn a_ford_stays_passable_and_costs_one_orthogonal_step() {
    let hm = height_field(20, 20, |_, _| 0);
    let water = lake(
        vec![[45.0, 45.0], [145.0, 45.0], [145.0, 145.0], [45.0, 145.0]],
        4,
    );
    let grid = Passability::builder(&hm, UNIT_SCALE)
        .water(&water, WaterDepths::SHIPPED)
        .build()
        .unwrap();
    let cell = Cell::new(10, 10);
    assert!(grid.grid.is_passable(cell, GROUND));
    assert!(grid.grid.flags(cell).intersects(FORD));
    assert_eq!(grid.grid.extra_cost(cell), u32::from(FORD_COST));
}

#[test]
fn one_wet_corner_is_enough_to_make_a_cell_water() {
    let hm = height_field(20, 20, |_, _| 0);
    let water = lake(
        vec![[96.0, 96.0], [104.0, 96.0], [104.0, 104.0], [96.0, 104.0]],
        50,
    );
    let grid = Passability::builder(&hm, UNIT_SCALE)
        .water(&water, WaterDepths::SHIPPED)
        .build()
        .unwrap();
    for (x, y) in [(9u16, 9u16), (10, 9), (9, 10), (10, 10)] {
        assert!(
            !grid.grid.is_passable(Cell::new(x, y), GROUND),
            "cell ({x}, {y}) has a wet corner"
        );
    }
    assert!(grid.grid.is_passable(Cell::new(8, 8), GROUND));
    assert!(grid.grid.is_passable(Cell::new(11, 11), GROUND));
}

#[test]
fn water_does_not_dilate_the_way_a_cliff_does() {
    let hm = height_field(20, 20, |_, _| 0);
    let water = lake(
        vec![[45.0, 45.0], [145.0, 45.0], [145.0, 145.0], [45.0, 145.0]],
        50,
    );
    let grid = Passability::builder(&hm, UNIT_SCALE)
        .water(&water, WaterDepths::SHIPPED)
        .build()
        .unwrap();
    assert!(!grid.grid.is_passable(Cell::new(4, 9), GROUND));
    assert!(grid.grid.is_passable(Cell::new(3, 9), GROUND));
    assert_eq!(grid.grid.extra_cost(Cell::new(3, 9)), 0);
}

#[test]
fn a_box_footprint_blocks_the_cells_under_it_and_nothing_else() {
    let hm = height_field(20, 20, |_, _| 0);
    let obstacles = [Obstacle {
        id: 7,
        centre: Vec2::new(105.0, 105.0),
        angle: 0.0,
        records: single(Footprint::Box {
            major: 20.0,
            minor: 10.0,
        }),
    }];
    let grid = Passability::builder(&hm, UNIT_SCALE)
        .obstacles(&obstacles)
        .build()
        .unwrap();
    for x in 8..=12u16 {
        assert!(
            !grid.grid.is_passable(Cell::new(x, 10), GROUND),
            "cell ({x}, 10) is under the box"
        );
        assert_eq!(grid.obstacle_at(Cell::new(x, 10)), Some(7));
    }
    assert!(grid.grid.is_passable(Cell::new(7, 10), GROUND));
    assert!(grid.grid.is_passable(Cell::new(13, 10), GROUND));
    assert!(grid.grid.is_passable(Cell::new(10, 8), GROUND));
    assert!(grid.grid.is_passable(Cell::new(10, 12), GROUND));
    assert_eq!(grid.obstacle_at(Cell::new(7, 10)), None);
}

#[test]
fn a_scales_walls_mover_is_excused_only_through_a_scaleable_footprint() {
    let hm = height_field(20, 20, |_, _| 0);
    let obstacles = [
        Obstacle {
            id: 0,
            centre: Vec2::new(105.0, 105.0),
            angle: 0.0,
            records: single(Footprint::Box {
                major: 20.0,
                minor: 10.0,
            }),
        },
        Obstacle {
            id: 1,
            centre: Vec2::new(165.0, 105.0),
            angle: 0.0,
            records: single(Footprint::Box {
                major: 5.0,
                minor: 10.0,
            }),
        },
    ];
    let grid = Passability::builder(&hm, UNIT_SCALE)
        .obstacles(&obstacles)
        .scaleable_obstacles(&[0])
        .build()
        .unwrap();
    let scaler = mover_class(&["GROUND".to_string(), "OBSTACLE".to_string()], true).unwrap();
    assert!(grid.grid.is_passable(Cell::new(10, 10), scaler));
    assert!(!grid.grid.is_passable(Cell::new(16, 10), scaler));
}

#[test]
fn a_disc_footprint_reaches_zero_point_four_of_a_cell_past_its_radius() {
    let hm = height_field(20, 20, |_, _| 0);
    let obstacles = [Obstacle {
        id: 0,
        centre: Vec2::new(105.0, 105.0),
        angle: 0.0,
        records: single(Footprint::Disc { radius: 7.0 }),
    }];
    let grid = Passability::builder(&hm, UNIT_SCALE)
        .obstacles(&obstacles)
        .build()
        .unwrap();
    assert!(!grid.grid.is_passable(Cell::new(9, 10), GROUND));
    assert_eq!(grid.obstacle_at(Cell::new(9, 10)), Some(0));
}

#[test]
fn a_box_footprint_marks_a_cell_a_sample_lands_in_though_its_centre_is_outside() {
    let hm = height_field(20, 20, |_, _| 0);
    let obstacles = [Obstacle {
        id: 0,
        centre: Vec2::new(100.0, 100.0),
        angle: 0.0,
        records: single(Footprint::Box {
            major: 4.0,
            minor: 4.0,
        }),
    }];
    let grid = Passability::builder(&hm, UNIT_SCALE)
        .obstacles(&obstacles)
        .build()
        .unwrap();
    assert!(!grid.grid.is_passable(Cell::new(10, 10), GROUND));
    assert_eq!(grid.obstacle_at(Cell::new(10, 10)), Some(0));
    assert!(grid.grid.is_passable(Cell::new(12, 10), GROUND));
}

#[test]
fn a_rotated_non_grid_aligned_box_matches_the_reference_cell_set() {
    let hm = height_field(24, 24, |_, _| 0);
    let angle = 37.0 * core::f32::consts::PI / 180.0;
    let obstacles = [Obstacle {
        id: 0,
        centre: Vec2::new(103.3, 97.7),
        angle,
        records: single(Footprint::Box {
            major: 22.0,
            minor: 9.0,
        }),
    }];
    let grid = Passability::builder(&hm, UNIT_SCALE)
        .obstacles(&obstacles)
        .build()
        .unwrap();
    let expected: std::collections::HashSet<(u16, u16)> = [
        (8, 8),
        (8, 9),
        (9, 8),
        (9, 9),
        (9, 10),
        (10, 8),
        (10, 9),
        (10, 10),
        (10, 11),
        (11, 9),
        (11, 10),
        (11, 11),
        (12, 10),
    ]
    .into_iter()
    .collect();
    for y in 6..15u16 {
        for x in 6..15u16 {
            let blocked = !grid.grid.is_passable(Cell::new(x, y), GROUND);
            assert_eq!(
                blocked,
                expected.contains(&(x, y)),
                "cell ({x}, {y}): blocked {blocked}, expected {}",
                expected.contains(&(x, y))
            );
        }
    }
}

#[test]
fn a_rotated_box_covers_the_cells_its_rotation_puts_it_over() {
    let hm = height_field(24, 24, |_, _| 0);
    let angle = core::f32::consts::PI / 6.0;
    let obstacles = [Obstacle {
        id: 1,
        centre: Vec2::new(105.0, 105.0),
        angle,
        records: single(Footprint::Box {
            major: 40.0,
            minor: 10.0,
        }),
    }];
    let grid = Passability::builder(&hm, UNIT_SCALE)
        .obstacles(&obstacles)
        .build()
        .unwrap();
    assert!(!grid.grid.is_passable(Cell::new(10, 10), GROUND));
    assert!(
        !grid.grid.is_passable(Cell::new(13, 12), GROUND),
        "up the box's long axis"
    );
    assert!(
        grid.grid.is_passable(Cell::new(13, 8), GROUND),
        "across the box's short axis"
    );
    assert!(!grid.grid.is_passable(Cell::new(7, 8), GROUND));
    assert!(grid.grid.is_passable(Cell::new(7, 12), GROUND));
}

#[test]
fn overlapping_footprints_leave_the_later_obstacles_id() {
    let hm = height_field(20, 20, |_, _| 0);
    let obstacles = [
        Obstacle {
            id: 3,
            centre: Vec2::new(105.0, 105.0),
            angle: 0.0,
            records: single(Footprint::Disc { radius: 25.0 }),
        },
        Obstacle {
            id: 9,
            centre: Vec2::new(105.0, 105.0),
            angle: 0.0,
            records: single(Footprint::Disc { radius: 12.0 }),
        },
    ];
    let grid = Passability::builder(&hm, UNIT_SCALE)
        .obstacles(&obstacles)
        .build()
        .unwrap();
    assert_eq!(grid.obstacle_at(Cell::new(10, 10)), Some(9));
    assert_eq!(grid.obstacle_at(Cell::new(12, 10)), Some(3));
}

#[test]
fn a_footprint_off_the_map_is_counted_rather_than_clamped_onto_the_edge() {
    let hm = height_field(12, 12, |_, _| 0);
    let obstacles = [Obstacle {
        id: 0,
        centre: Vec2::new(-5000.0, -5000.0),
        angle: 0.0,
        records: single(Footprint::Disc { radius: 30.0 }),
    }];
    let grid = Passability::builder(&hm, UNIT_SCALE)
        .obstacles(&obstacles)
        .build()
        .unwrap();
    for y in 0..11u16 {
        for x in 0..11u16 {
            assert!(grid.grid.is_passable(Cell::new(x, y), GROUND));
        }
    }
    assert!(
        grid.report().contains("1 off the grid"),
        "{}",
        grid.report()
    );
}

#[test]
fn an_enormous_or_non_finite_footprint_does_not_run_away() {
    let hm = height_field(12, 12, |_, _| 0);
    for radius in [1e30f32, f32::INFINITY, f32::NAN, -5.0] {
        let obstacles = [Obstacle {
            id: 0,
            centre: Vec2::new(55.0, 55.0),
            angle: 0.0,
            records: single(Footprint::Disc { radius }),
        }];
        let grid = Passability::builder(&hm, UNIT_SCALE)
            .obstacles(&obstacles)
            .build()
            .unwrap_or_else(|e| panic!("radius {radius}: {e}"));
        for y in 0..11u16 {
            for x in 0..11u16 {
                let _ = grid.grid.is_passable(Cell::new(x, y), GROUND);
            }
        }
    }
}

#[test]
fn a_hostile_box_half_extent_does_not_run_away() {
    let hm = height_field(12, 12, |_, _| 0);
    for major in [1e18f32, 1e30f32, f32::INFINITY, f32::NAN, -5.0] {
        let obstacles = [Obstacle {
            id: 0,
            centre: Vec2::new(55.0, 55.0),
            angle: 0.3,
            records: single(Footprint::Box { major, minor: 9.0 }),
        }];
        let grid = Passability::builder(&hm, UNIT_SCALE)
            .obstacles(&obstacles)
            .build()
            .unwrap_or_else(|e| panic!("major {major}: {e}"));
        for y in 0..11u16 {
            for x in 0..11u16 {
                let _ = grid.grid.is_passable(Cell::new(x, y), GROUND);
            }
        }
    }
}

#[test]
fn the_border_ring_is_blocked_only_when_it_is_asked_for() {
    let mut hm = height_field(20, 20, |_, _| 0);
    hm.border_size = 3;
    let open = Passability::build(&hm, UNIT_SCALE).unwrap();
    assert!(open.grid.is_passable(Cell::new(0, 0), GROUND));
    let bounded = Passability::builder(&hm, UNIT_SCALE)
        .playable_border(true)
        .build()
        .unwrap();
    assert!(!bounded.grid.is_passable(Cell::new(0, 0), GROUND));
    assert!(!bounded.grid.is_passable(Cell::new(2, 9), GROUND));
    assert!(bounded.grid.is_passable(Cell::new(3, 9), GROUND));
    assert!(bounded.grid.is_passable(Cell::new(15, 9), GROUND));
    assert!(!bounded.grid.is_passable(Cell::new(16, 9), GROUND));
}

#[test]
fn a_border_that_covers_the_whole_field_marks_nothing() {
    let mut hm = height_field(8, 8, |_, _| 0);
    hm.border_size = 40;
    let grid = Passability::builder(&hm, UNIT_SCALE)
        .playable_border(true)
        .build()
        .unwrap();
    for y in 0..7u16 {
        for x in 0..7u16 {
            assert!(grid.grid.is_passable(Cell::new(x, y), GROUND));
        }
    }
}

#[test]
fn the_active_boundary_rectangle_decides_the_border_not_the_margin_formula() {
    let mut hm = height_field(30, 30, |_, _| 0);
    hm.border_size = 5;
    hm.boundaries = vec![mint_sage_map::Boundary { x: 16, y: 20 }];
    let grid = Passability::builder(&hm, UNIT_SCALE)
        .playable_border(true)
        .build()
        .unwrap();
    let at = |x: f32, y: f32| grid.cell_at(Vec2::new(x, y)).expect("inside the grid");
    assert!(
        !grid.grid.is_passable(at(165.0, 100.0), GROUND),
        "x=165 must be boundary"
    );
    assert!(
        grid.grid.is_passable(at(155.0, 100.0), GROUND),
        "x=155 must be playable"
    );
    assert!(
        !grid.grid.is_passable(at(-5.0, 100.0), GROUND),
        "x=-5 must be boundary"
    );
}

#[test]
fn a_unit_inside_a_footprint_escapes_to_the_nearest_cell_outside_it() {
    let hm = height_field(20, 20, |_, _| 0);
    let obstacles = [Obstacle {
        id: 0,
        centre: Vec2::new(105.0, 105.0),
        angle: 0.0,
        records: single(Footprint::Disc { radius: 22.0 }),
    }];
    let grid = Passability::builder(&hm, UNIT_SCALE)
        .obstacles(&obstacles)
        .build()
        .unwrap();
    let inside = Cell::new(10, 10);
    assert!(!grid.grid.is_passable(inside, GROUND));
    let escape = grid.nearest_passable(inside, GROUND).expect("a way out");
    assert!(grid.grid.is_passable(escape, GROUND));
    let ring = i32::from(escape.x)
        .saturating_sub(10)
        .abs()
        .max(i32::from(escape.y).saturating_sub(10).abs());
    assert_eq!(ring, 2, "found {escape:?}");
    assert_eq!(escape, Cell::new(8, 8));
    assert_eq!(
        grid.nearest_passable(Cell::new(1, 1), GROUND),
        Some(Cell::new(1, 1))
    );
}

#[test]
fn a_unit_walled_in_beyond_the_escape_cap_is_left_where_it_is() {
    let hm = height_field(40, 40, |_, _| 0);
    let obstacles = [Obstacle {
        id: 0,
        centre: Vec2::new(205.0, 205.0),
        angle: 0.0,
        records: single(Footprint::Disc { radius: 100.0 }),
    }];
    let grid = Passability::builder(&hm, UNIT_SCALE)
        .obstacles(&obstacles)
        .build()
        .unwrap();
    assert_eq!(grid.nearest_passable(Cell::new(20, 20), GROUND), None);
}

#[test]
fn nearest_passable_around_resolves_a_footprint_wider_than_the_escape_cap() {
    let hm = height_field(60, 60, |_, _| 0);
    let obstacle = Obstacle {
        id: 0,
        centre: Vec2::new(305.0, 305.0),
        angle: 0.0,
        records: single(Footprint::Disc { radius: 100.0 }),
    };
    let grid = Passability::builder(&hm, UNIT_SCALE)
        .obstacles(std::slice::from_ref(&obstacle))
        .build()
        .unwrap();
    let centre_cell = grid.cell_at(obstacle.centre).unwrap();
    assert_eq!(grid.nearest_passable(centre_cell, GROUND), None);
    let rect = grid
        .covering_rect(obstacle.centre, obstacle.reach())
        .unwrap();
    let edge = grid
        .nearest_passable_around(rect, GROUND)
        .expect("a cell just outside the footprint");
    assert!(grid.grid.is_passable(edge, GROUND));
    assert!(edge.x >= rect.x0 && edge.x <= rect.x1 && edge.y >= rect.y0 && edge.y <= rect.y1);
}

#[test]
fn nearest_passable_around_is_the_same_cell_every_time() {
    let hm = height_field(60, 60, |_, _| 0);
    let obstacles = [Obstacle {
        id: 0,
        centre: Vec2::new(305.0, 305.0),
        angle: 0.0,
        records: single(Footprint::Disc { radius: 100.0 }),
    }];
    let grid = Passability::builder(&hm, UNIT_SCALE)
        .obstacles(&obstacles)
        .build()
        .unwrap();
    let rect = grid
        .covering_rect(obstacles[0].centre, obstacles[0].reach())
        .unwrap();
    let first = grid.nearest_passable_around(rect, GROUND);
    assert!(first.is_some());
    for _ in 0..8 {
        assert_eq!(grid.nearest_passable_around(rect, GROUND), first);
    }
}

#[test]
fn a_dynamic_view_adds_obstacle_where_a_count_is_live() {
    let hm = height_field(10, 10, |_, _| 0);
    let grid = Passability::build(&hm, UNIT_SCALE).unwrap();
    let extent = grid.grid.extent();
    let mut counts = vec![0u8; usize::try_from(extent.cell_count()).unwrap()];
    let cell = Cell::new(3, 3);
    let index = usize::try_from(extent.index(cell).unwrap()).unwrap();
    counts[index] = 1;
    let view = DynamicView {
        grid: &grid.grid,
        counts: &counts,
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
    assert!(mint_movement::Passability::flags(&view, cell).intersects(OBSTACLE));
    assert!(!mint_movement::Passability::flags(&view, Cell::new(4, 4)).intersects(OBSTACLE));
    assert!(!grid.grid.flags(cell).intersects(OBSTACLE));
}

#[test]
fn a_wall_top_releases_a_live_obstacle_once_but_not_a_later_one() {
    let hm = height_field(10, 10, |_, _| 0);
    let grid = Passability::build(&hm, UNIT_SCALE).unwrap();
    let extent = grid.grid.extent();
    let cell_count = usize::try_from(extent.cell_count()).unwrap();
    let cell = Cell::new(3, 3);
    let index = usize::try_from(extent.index(cell).unwrap()).unwrap();
    let mut counts = vec![0u8; cell_count];
    counts[index] = 1;
    let mut tags = vec![GROUND_TAG; cell_count];
    tags[index] = FIRST_WALL_TAG;
    let mut released = vec![0u8; cell_count];
    released[index] = 1;
    let released_view = DynamicView {
        grid: &grid.grid,
        counts: &counts,
        rubble: &[],
        wall_rim: &[],
        tags: &tags,
        connect: &[],
        released: &released,
        closed: &[],
        obstacle_pinched: &[],
        ramp_side: &[],
        scaleable: &[],
        barred: &[],
    };
    assert!(
        !mint_movement::Passability::flags(&released_view, cell).intersects(OBSTACLE),
        "released under the wall top at stamp time"
    );
    let not_released_view = DynamicView {
        grid: &grid.grid,
        counts: &counts,
        rubble: &[],
        wall_rim: &[],
        tags: &tags,
        connect: &[],
        released: &[],
        closed: &[],
        obstacle_pinched: &[],
        ramp_side: &[],
        scaleable: &[],
        barred: &[],
    };
    assert!(
        mint_movement::Passability::flags(&not_released_view, cell).intersects(OBSTACLE),
        "a live obstacle not recorded as released still blocks"
    );
}

#[test]
fn a_wall_top_cell_carries_gate_restricted_but_ground_does_not() {
    let hm = height_field(10, 10, |_, _| 0);
    let grid = Passability::build(&hm, UNIT_SCALE).unwrap();
    let extent = grid.grid.extent();
    let cell_count = usize::try_from(extent.cell_count()).unwrap();
    let wall_cell = Cell::new(3, 3);
    let ground_cell = Cell::new(4, 4);
    let index = usize::try_from(extent.index(wall_cell).unwrap()).unwrap();
    let mut tags = vec![GROUND_TAG; cell_count];
    tags[index] = FIRST_WALL_TAG;
    let view = DynamicView {
        grid: &grid.grid,
        counts: &[],
        rubble: &[],
        wall_rim: &[],
        tags: &tags,
        connect: &[],
        released: &[],
        closed: &[],
        obstacle_pinched: &[],
        ramp_side: &[],
        scaleable: &[],
        barred: &[],
    };
    assert!(mint_movement::Passability::flags(&view, wall_cell).intersects(GATE_RESTRICTED));
    assert!(!mint_movement::Passability::flags(&view, ground_cell).intersects(GATE_RESTRICTED));
}

#[test]
fn a_ramp_side_cell_refuses_a_ground_mover_but_admits_an_impassable_one() {
    let hm = height_field(10, 10, |_, _| 0);
    let grid = Passability::build(&hm, UNIT_SCALE).unwrap();
    let extent = grid.grid.extent();
    let cell_count = usize::try_from(extent.cell_count()).unwrap();
    let cell = Cell::new(3, 3);
    let index = usize::try_from(extent.index(cell).unwrap()).unwrap();
    let mut tags = vec![GROUND_TAG; cell_count];
    tags[index] = RAMP_TAG;
    let mut side = vec![0u8; cell_count];
    side[index] = 1;
    let view = DynamicView {
        grid: &grid.grid,
        counts: &[],
        rubble: &[],
        wall_rim: &[],
        tags: &tags,
        connect: &[],
        released: &[],
        closed: &[],
        obstacle_pinched: &[],
        ramp_side: &side,
        scaleable: &[],
        barred: &[],
    };
    let ground_mover = mover_class(&["GROUND".to_owned()], false).unwrap();
    let impassable_mover =
        mover_class(&["GROUND".to_owned(), "IMPASSABLE".to_owned()], false).unwrap();
    assert!(!mint_movement::Passability::is_passable(
        &view,
        cell,
        ground_mover
    ));
    assert!(mint_movement::Passability::is_passable(
        &view,
        cell,
        impassable_mover
    ));
}

#[test]
fn nearest_passable_over_a_dynamic_view_routes_around_a_live_stamp() {
    let hm = height_field(20, 20, |_, _| 0);
    let grid = Passability::build(&hm, UNIT_SCALE).unwrap();
    let extent = grid.grid.extent();
    let mut counts = vec![0u8; usize::try_from(extent.cell_count()).unwrap()];
    for y in 9..=11u16 {
        for x in 9..=11u16 {
            let index = usize::try_from(extent.index(Cell::new(x, y)).unwrap()).unwrap();
            counts[index] = 1;
        }
    }
    let view = DynamicView {
        grid: &grid.grid,
        counts: &counts,
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
    assert_eq!(
        nearest_passable_over(&grid.grid, Cell::new(10, 10), GROUND),
        Some(Cell::new(10, 10))
    );
    let escape = nearest_passable_over(&view, Cell::new(10, 10), GROUND).unwrap();
    assert!(mint_movement::Passability::is_passable(
        &view, escape, GROUND
    ));
    assert!(escape.x < 9 || escape.x > 11 || escape.y < 9 || escape.y > 11);
}

#[test]
fn a_tag_mismatched_neighbour_is_refused_without_a_ramps_connect() {
    use mint_movement::{AStar, GoalId, NodeBudget, PathRequest, Search as _, SearchOutcome};

    let extent = GridExtent::new(3, 2).unwrap();
    let grid = CostGrid::new(extent);
    let mut tags = vec![GROUND_TAG; usize::try_from(extent.cell_count()).unwrap()];
    let mut connect = vec![0u8; tags.len()];
    let set = |tags: &mut [u8], connect: &mut [u8], x: u16, y: u16, tag: u8, conn: u8| {
        let index = usize::try_from(extent.index(Cell::new(x, y)).unwrap()).unwrap();
        tags[index] = tag;
        connect[index] = conn;
    };
    const WALL_TAG: u8 = 17;
    set(&mut tags, &mut connect, 1, 0, WALL_TAG, 0);
    set(&mut tags, &mut connect, 2, 0, WALL_TAG, 0);
    set(&mut tags, &mut connect, 0, 1, GROUND_TAG, RAMP_TAG);
    set(&mut tags, &mut connect, 1, 1, RAMP_TAG, 0);
    set(&mut tags, &mut connect, 2, 1, WALL_TAG, RAMP_TAG);
    let view = DynamicView {
        grid: &grid,
        counts: &[],
        rubble: &[],
        wall_rim: &[],
        tags: &tags,
        connect: &connect,
        released: &[],
        closed: &[],
        obstacle_pinched: &[],
        ramp_side: &[],
        scaleable: &[],
        barred: &[],
    };
    let request = PathRequest {
        start: Cell::new(0, 0),
        goal: Cell::new(2, 0),
        mover: GROUND,
        goal_id: GoalId::default(),
        origin: None,
        corner_radius: 0.0,
        departure: None,
    };
    let mut search = AStar::octile();
    let mut budget = NodeBudget::unlimited();
    let result = search
        .find(&view, &request, &mut budget)
        .expect("in-bounds request");
    let SearchOutcome::Found(path) = result.outcome else {
        panic!("expected a path through the ramp, got {:?}", result.outcome);
    };
    assert!(
        path.cells.contains(&Cell::new(1, 1)),
        "the path must detour through the ramp's own cell, not climb the rim directly: {:?}",
        path.cells
    );
    assert_eq!(
        path.cells.len(),
        5,
        "ground -> ramp end -> ramp -> wall end -> wall top"
    );
}

#[test]
fn a_structure_stamped_over_a_ramp_after_it_exists_still_blocks() {
    let extent = GridExtent::new(3, 1).unwrap();
    let grid = CostGrid::new(extent);
    let mut tags = vec![GROUND_TAG; usize::try_from(extent.cell_count()).unwrap()];
    let mut connect = vec![0u8; tags.len()];
    const WALL_TAG: u8 = 17;
    let set = |tags: &mut [u8], connect: &mut [u8], x: u16, tag: u8, conn: u8| {
        let index = usize::try_from(extent.index(Cell::new(x, 0)).unwrap()).unwrap();
        tags[index] = tag;
        connect[index] = conn;
    };
    set(&mut tags, &mut connect, 0, GROUND_TAG, RAMP_TAG);
    set(&mut tags, &mut connect, 1, RAMP_TAG, 0);
    set(&mut tags, &mut connect, 2, WALL_TAG, RAMP_TAG);
    let counts = vec![1u8; tags.len()];
    let view = DynamicView {
        grid: &grid,
        counts: &counts,
        rubble: &[],
        wall_rim: &[],
        tags: &tags,
        connect: &connect,
        released: &[],
        closed: &[],
        obstacle_pinched: &[],
        ramp_side: &[],
        scaleable: &[],
        barred: &[],
    };
    for x in 0..3 {
        assert!(
            mint_movement::Passability::flags(&view, Cell::new(x, 0)).intersects(OBSTACLE),
            "cell {x} must read OBSTACLE: a later structure's live stamp, not the ramp's own release"
        );
    }
}

#[test]
fn the_escape_cell_is_the_same_one_every_time() {
    let hm = height_field(20, 20, |_, _| 0);
    let obstacles = [Obstacle {
        id: 0,
        centre: Vec2::new(105.0, 105.0),
        angle: 0.0,
        records: single(Footprint::Disc { radius: 22.0 }),
    }];
    let grid = Passability::builder(&hm, UNIT_SCALE)
        .obstacles(&obstacles)
        .build()
        .unwrap();
    let first = grid.nearest_passable(Cell::new(10, 10), GROUND);
    for _ in 0..8 {
        assert_eq!(grid.nearest_passable(Cell::new(10, 10), GROUND), first);
    }
}

#[test]
fn adding_a_source_only_ever_blocks_more_cells() {
    let mut hm = height_field(30, 30, |x, y| if x == 20 && y == 20 { 400 } else { 0 });
    hm.border_size = 2;
    let water = lake(
        vec![[25.0, 25.0], [95.0, 25.0], [95.0, 95.0], [25.0, 95.0]],
        50,
    );
    let obstacles = [Obstacle {
        id: 0,
        centre: Vec2::new(155.0, 155.0),
        angle: 0.3,
        records: single(Footprint::Box {
            major: 30.0,
            minor: 20.0,
        }),
    }];
    let cliffs = Passability::build(&hm, UNIT_SCALE).unwrap();
    let wet = Passability::builder(&hm, UNIT_SCALE)
        .water(&water, WaterDepths::SHIPPED)
        .build()
        .unwrap();
    let built = Passability::builder(&hm, UNIT_SCALE)
        .water(&water, WaterDepths::SHIPPED)
        .obstacles(&obstacles)
        .build()
        .unwrap();
    let all = Passability::builder(&hm, UNIT_SCALE)
        .water(&water, WaterDepths::SHIPPED)
        .obstacles(&obstacles)
        .playable_border(true)
        .build()
        .unwrap();
    let ladder = [&cliffs, &wet, &built, &all];
    for pair in ladder.windows(2) {
        let (Some(lower), Some(higher)) = (pair.first(), pair.get(1)) else {
            continue;
        };
        assert!(
            higher.blocked_cells() > lower.blocked_cells(),
            "{} then {}",
            lower.report(),
            higher.report()
        );
        for y in 0..29u16 {
            for x in 0..29u16 {
                let cell = Cell::new(x, y);
                if !lower.grid.is_passable(cell, GROUND) {
                    assert!(
                        !higher.grid.is_passable(cell, GROUND),
                        "cell ({x}, {y}) unblocked when a source was added"
                    );
                }
            }
        }
    }
    let line = all.report();
    for fragment in ["cliff", "water", "deep", "obstacle", "boundary", "ford"] {
        assert!(line.contains(fragment), "{line} is missing {fragment}");
    }
}

#[test]
fn a_cell_that_is_both_pinched_and_a_ford_pays_for_both() {
    let hm = height_field(20, 20, |x, y| if x == 10 && y == 10 { 400 } else { 0 });
    let water = lake(
        vec![
            [-50.0, -50.0],
            [250.0, -50.0],
            [250.0, 250.0],
            [-50.0, 250.0],
        ],
        3,
    );
    let grid = Passability::builder(&hm, UNIT_SCALE)
        .water(&water, WaterDepths::SHIPPED)
        .build()
        .unwrap();
    let pinched = Cell::new(7, 7);
    assert!(grid.grid.flags(pinched).intersects(PINCHED));
    assert!(grid.grid.flags(pinched).intersects(FORD));
    assert_eq!(
        grid.grid.extra_cost(pinched),
        u32::from(PINCHED_COST).saturating_add(u32::from(FORD_COST))
    );
}

fn blend_tile_2x2(plane_bit: Option<(usize, bool)>, flammability_byte: u8) -> BlendTileData {
    let plane_len = 2usize;
    let mut bit_planes = vec![vec![0u8; plane_len]; 6];
    if let Some((index, true)) = plane_bit {
        if let Some(plane) = bit_planes.get_mut(index) {
            plane[0] = 0x01;
        }
    }
    BlendTileData {
        version: 18,
        width: 2,
        height: 2,
        cell_count: 4,
        tile_index: vec![0; 4],
        blend_index: vec![0; 4],
        extra_blend_index: vec![0; 4],
        cliff_info_index: vec![0; 4],
        unknown_cell_byte: Some(vec![flammability_byte, 0, 0, 0]),
        bit_planes,
        num_bitmap_tiles: 0,
        texture_classes: Vec::new(),
        num_edge_tiles: 0,
        edge_texture_classes: Vec::new(),
        blend_records: Vec::new(),
        cliff_records: Vec::new(),
    }
}

#[test]
fn extra_passable_overrides_the_persisted_impassable_bit() {
    let hm = height_field(2, 2, |_, _| 500);
    let mut bt = blend_tile_2x2(Some((0, true)), 0);
    bt.bit_planes[4][0] = 0x01;
    let grid = Passability::builder(&hm, UNIT_SCALE)
        .cliff_source(&bt)
        .build()
        .unwrap();
    assert!(grid.grid.is_passable(Cell::new(0, 0), GROUND));
    assert_eq!(grid.blocked_cells(), 0);
}

#[test]
fn the_persisted_impassable_bit_blocks_flat_ground_at_version_18() {
    let hm = height_field(2, 2, |_, _| 500);
    let bt = blend_tile_2x2(Some((0, true)), 0);
    let grid = Passability::builder(&hm, UNIT_SCALE)
        .cliff_source(&bt)
        .build()
        .unwrap();
    assert!(!grid.grid.is_passable(Cell::new(0, 0), GROUND));
    assert_eq!(grid.blocked_cells(), 1);
}

#[test]
fn impassable_to_players_blocks_the_player_mask_but_not_ground() {
    let hm = height_field(2, 2, |_, _| 500);
    let bt = blend_tile_2x2(Some((1, true)), 0);
    let grid = Passability::builder(&hm, UNIT_SCALE)
        .cliff_source(&bt)
        .build()
        .unwrap();
    assert!(grid.grid.is_passable(Cell::new(0, 0), GROUND));
    assert!(!grid.grid.is_passable(Cell::new(0, 0), GROUND_PLAYER));
}

#[test]
fn passage_width_narrow_sets_the_flag_but_never_blocks_ground() {
    let hm = height_field(2, 2, |_, _| 500);
    let bt = blend_tile_2x2(Some((2, true)), 0);
    let grid = Passability::builder(&hm, UNIT_SCALE)
        .cliff_source(&bt)
        .build()
        .unwrap();
    assert!(grid.grid.is_passable(Cell::new(0, 0), GROUND));
    assert!(!grid.grid.is_passable(
        Cell::new(0, 0),
        mint_movement::MoverClass::new(NARROW_PASSAGE)
    ));
}

#[test]
fn an_all_zero_blend_tile_blocks_nothing_beyond_the_height_rule() {
    let hm = height_field(2, 2, |_, _| 500);
    let bt = blend_tile_2x2(None, 0);
    let grid = Passability::builder(&hm, UNIT_SCALE)
        .cliff_source(&bt)
        .build()
        .unwrap();
    assert!(grid.grid.is_passable(Cell::new(0, 0), GROUND));
    assert!(grid.grid.is_passable(Cell::new(0, 0), GROUND_PLAYER));
    assert_eq!(bt.flammability(0, 0), Some(Flammability::FireResistant));
}

#[test]
fn a_locomotors_surfaces_decide_which_cells_it_is_stopped_by() {
    let cases: [(&str, [bool; 3]); 5] = [
        ("GROUND RUBBLE", [true, false, false]),
        ("GROUND RUBBLE CLIFF", [true, true, false]),
        ("AIR", [true, true, true]),
        ("WATER DEEP_WATER", [false, false, true]),
        ("WATER DEEP_WATER GROUND", [true, false, true]),
    ];
    for (names, expected) in cases {
        let list: Vec<String> = names.split(' ').map(str::to_owned).collect();
        let mover = mover_class(&list, false).unwrap();
        let got = [CellFlags::NONE, CLIFF, DEEP_WATER].map(|flags| {
            !flags.intersects(mover.blocked_by)
                && (mover.requires == CellFlags::NONE || flags.intersects(mover.requires))
        });
        assert_eq!(got, expected, "{names}");
    }
    assert!(mover_class(&["NOT_A_SURFACE".to_owned()], false).is_none());
}

#[test]
fn scales_walls_keeps_obstacle_blocked_even_when_the_surface_names_it() {
    let scaler = mover_class(&["GROUND".to_owned(), "OBSTACLE".to_owned()], true).unwrap();
    let non_scaler = mover_class(&["GROUND".to_owned(), "OBSTACLE".to_owned()], false).unwrap();
    assert!(scaler.blocked_by.intersects(OBSTACLE));
    assert!(!non_scaler.blocked_by.intersects(OBSTACLE));
}

fn obstacle_column(grid: &mut CostGrid, extent: GridExtent, x: u16) {
    for y in 0..extent.height {
        grid.add_flags(Cell::new(x, y), OBSTACLE).unwrap();
    }
}

#[test]
fn a_one_cell_gap_between_two_walls_closes() {
    let extent = GridExtent::new(12, 12).unwrap();
    let mut grid = CostGrid::new(extent);
    obstacle_column(&mut grid, extent, 3);
    obstacle_column(&mut grid, extent, 5);
    let mut counts = Counts::default();
    let bounds = CellRect {
        x0: 0,
        x1: 11,
        y0: 0,
        y1: 11,
    };
    apply_obstacle_border_passes(&mut grid, extent, bounds, &mut counts).unwrap();
    for y in 3..9u16 {
        assert!(
            grid.flags(Cell::new(4, y)).intersects(OBSTACLE),
            "cell (4, {y}) is a 1-cell slot and should close"
        );
    }
    assert!(counts.obstacle_closed > 0);
}

#[test]
fn a_cell_with_one_clear_orthogonal_neighbour_closes_on_that_alone() {
    let extent = GridExtent::new(12, 12).unwrap();
    let mut grid = CostGrid::new(extent);
    let cell = Cell::new(5, 5);
    for (dx, dy) in [(-1i32, 0i32), (0, -1), (0, 1)] {
        grid.add_flags(offset_cell(cell, dx, dy, extent).unwrap(), OBSTACLE)
            .unwrap();
    }
    let mut counts = Counts::default();
    let bounds = CellRect {
        x0: 0,
        x1: 11,
        y0: 0,
        y1: 11,
    };
    apply_obstacle_border_passes(&mut grid, extent, bounds, &mut counts).unwrap();
    assert!(
        grid.flags(cell).intersects(OBSTACLE),
        "one clear orthogonal neighbour out of four is still below 2"
    );
}

#[test]
fn a_four_cell_gap_pinches_its_two_edge_cells_only() {
    let extent = GridExtent::new(10, 10).unwrap();
    let mut grid = CostGrid::new(extent);
    obstacle_column(&mut grid, extent, 2);
    obstacle_column(&mut grid, extent, 7);
    let mut counts = Counts::default();
    let bounds = CellRect {
        x0: 0,
        x1: 9,
        y0: 0,
        y1: 9,
    };
    apply_obstacle_border_passes(&mut grid, extent, bounds, &mut counts).unwrap();
    let y = 5;
    assert!(
        !grid.flags(Cell::new(3, y)).intersects(OBSTACLE),
        "no cell in the gap closes"
    );
    assert!(
        grid.flags(Cell::new(3, y)).intersects(PINCHED),
        "edge cell 3 touches a wall"
    );
    assert!(
        !grid.flags(Cell::new(4, y)).intersects(PINCHED),
        "middle cell 4 does not"
    );
    assert!(
        !grid.flags(Cell::new(5, y)).intersects(PINCHED),
        "middle cell 5 does not"
    );
    assert!(
        grid.flags(Cell::new(6, y)).intersects(PINCHED),
        "edge cell 6 touches a wall"
    );
}

#[test]
fn a_cell_touching_an_obstacle_only_diagonally_is_not_pinched() {
    let extent = GridExtent::new(10, 10).unwrap();
    let mut grid = CostGrid::new(extent);
    grid.add_flags(Cell::new(5, 5), OBSTACLE).unwrap();
    let mut counts = Counts::default();
    let bounds = CellRect {
        x0: 0,
        x1: 9,
        y0: 0,
        y1: 9,
    };
    apply_obstacle_border_passes(&mut grid, extent, bounds, &mut counts).unwrap();
    assert!(
        grid.flags(Cell::new(4, 5)).intersects(PINCHED),
        "orthogonal contact pinches"
    );
    assert!(
        !grid.flags(Cell::new(4, 4)).intersects(PINCHED),
        "diagonal contact alone does not"
    );
}
