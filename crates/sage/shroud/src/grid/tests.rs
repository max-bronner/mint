use super::*;

fn grid(cells: u32, players: u32) -> VisibilityGrid {
    VisibilityGrid::new(
        GridExtent {
            low_x: 0.0,
            low_y: 0.0,
            cell_size: 40.0,
            cells_x: cells,
            cells_y: cells,
        },
        players,
    )
    .unwrap()
}

const P0: PlayerId = PlayerId::new(0);
const P1: PlayerId = PlayerId::new(1);

#[test]
fn a_fresh_grid_is_unexplored_for_every_player() {
    let grid = grid(8, 4);
    for player in 0..4u8 {
        for y in 0..8 {
            for x in 0..8 {
                assert_eq!(
                    grid.cell_state(PlayerId::new(player), x, y),
                    CellVisibility::Unexplored
                );
            }
        }
    }
}

#[test]
fn one_looker_arriving_and_leaving_leaves_the_cell_explored_not_unexplored() {
    let mut grid = grid(4, 2);
    assert_eq!(grid.cell_state(P0, 1, 1), CellVisibility::Unexplored);
    grid.add_looker(P0, 1, 1);
    assert_eq!(grid.cell_state(P0, 1, 1), CellVisibility::Visible);
    grid.remove_looker(P0, 1, 1);
    assert_eq!(grid.cell_state(P0, 1, 1), CellVisibility::Explored);
}

#[test]
fn n_lookers_leaving_one_at_a_time_never_flicker() {
    let mut grid = grid(4, 1);
    for _ in 0..5 {
        grid.add_looker(P0, 2, 2);
    }
    assert_eq!(grid.looker_count(P0, 2, 2), 5);
    for remaining in (1..5).rev() {
        grid.remove_looker(P0, 2, 2);
        assert_eq!(grid.cell_state(P0, 2, 2), CellVisibility::Visible);
        assert_eq!(grid.looker_count(P0, 2, 2), remaining);
    }
    grid.remove_looker(P0, 2, 2);
    assert_eq!(grid.cell_state(P0, 2, 2), CellVisibility::Explored);
}

#[test]
fn a_shrouder_takes_an_explored_cell_back_to_unexplored_and_holds_it_there() {
    let mut grid = grid(4, 1);
    grid.add_looker(P0, 0, 0);
    grid.remove_looker(P0, 0, 0);
    assert_eq!(grid.cell_state(P0, 0, 0), CellVisibility::Explored);

    grid.add_shrouder(P0, 0, 0);
    assert_eq!(grid.cell_state(P0, 0, 0), CellVisibility::Unexplored);

    grid.add_looker(P0, 0, 0);
    assert_eq!(grid.cell_state(P0, 0, 0), CellVisibility::Visible);
    grid.remove_looker(P0, 0, 0);
    assert_eq!(grid.cell_state(P0, 0, 0), CellVisibility::Unexplored);

    grid.remove_shrouder(P0, 0, 0);
    assert_eq!(
        grid.cell_state(P0, 0, 0),
        CellVisibility::Unexplored,
        "removing a shrouder never changes `current` - EA's own comment"
    );
}

#[test]
fn one_players_vision_is_invisible_to_another() {
    let mut grid = grid(4, 3);
    grid.add_looker(P0, 1, 2);
    assert_eq!(grid.cell_state(P0, 1, 2), CellVisibility::Visible);
    assert_eq!(grid.cell_state(P1, 1, 2), CellVisibility::Unexplored);
    assert_eq!(
        grid.cell_state(PlayerId::new(2), 1, 2),
        CellVisibility::Unexplored
    );
}

#[test]
fn a_reveal_and_its_undo_leave_every_counter_balanced() {
    let mut grid = grid(12, 2);
    let mask = PlayerMask::of(P0).with(P1);
    let path = [
        (10.0f32, 10.0f32),
        (200.0, 200.0),
        (479.0, 479.0),
        (-100.0, 250.0),
        (250.0, -100.0),
        (900.0, 900.0),
    ];
    for (x, y) in path {
        grid.reveal(x, y, 400.0, mask);
    }
    assert!(grid.watched_cells() > 0);
    for (x, y) in path {
        grid.unreveal(x, y, 400.0, mask);
    }
    assert_eq!(grid.watched_cells(), 0, "a cell was left permanently clear");
    assert_eq!(grid.unbalanced_cells(), 0);
}

#[test]
fn revealing_all_takes_every_cell_to_explored_and_no_further() {
    let mut grid = grid(6, 2);
    grid.reveal_all(P0);
    for y in 0..6 {
        for x in 0..6 {
            assert_eq!(grid.cell_state(P0, x, y), CellVisibility::Explored);
            assert_eq!(grid.cell_state(P1, x, y), CellVisibility::Unexplored);
        }
    }
    assert_eq!(grid.watched_cells(), 0);
    assert_eq!(grid.unbalanced_cells(), 0);
}

#[test]
fn revealing_all_permanently_leaves_a_looker_on_every_cell() {
    let mut grid = grid(5, 1);
    grid.reveal_all_permanently(P0);
    for y in 0..5 {
        for x in 0..5 {
            assert_eq!(grid.cell_state(P0, x, y), CellVisibility::Visible);
        }
    }
    assert_eq!(grid.watched_cells(), 25);
}

#[test]
fn a_reveal_covers_the_cells_the_disc_table_predicts() {
    let mut grid = VisibilityGrid::new(
        GridExtent {
            low_x: 0.0,
            low_y: 0.0,
            cell_size: 40.0,
            cells_x: 64,
            cells_y: 64,
        },
        1,
    )
    .unwrap();
    grid.reveal(32.0 * 40.0, 32.0 * 40.0, 400.0, PlayerMask::of(P0));
    assert_eq!(grid.watched_cells(), 349);
}

#[test]
fn a_reveal_at_the_map_edge_covers_only_what_is_on_the_grid() {
    let mut grid = VisibilityGrid::new(
        GridExtent {
            low_x: 0.0,
            low_y: 0.0,
            cell_size: 40.0,
            cells_x: 64,
            cells_y: 64,
        },
        1,
    )
    .unwrap();
    grid.reveal(0.0, 0.0, 400.0, PlayerMask::of(P0));
    let covered = grid.watched_cells();
    assert!(
        (60..140).contains(&covered),
        "a corner reveal covered {covered} cells, expected roughly a quarter of 349"
    );
}

#[test]
fn a_huge_radius_reveal_covers_the_whole_grid_and_nothing_past_it() {
    let mut grid = grid(20, 1);
    grid.reveal(10.0 * 40.0, 10.0 * 40.0, 1_000_000.0, PlayerMask::of(P0));
    assert_eq!(grid.watched_cells(), 20 * 20, "the whole grid, no more");
}

#[test]
fn world_to_cell_floors_and_world_to_cell_dist_ceils() {
    let grid = grid(16, 1);
    assert_eq!(grid.world_to_cell(0.0, 0.0), (0, 0));
    assert_eq!(grid.world_to_cell(39.9, 40.1), (0, 1));
    assert_eq!(grid.world_to_cell(-0.1, -40.0), (-1, -1));
    assert_eq!(grid.world_to_cell_dist(0.0), 0);
    assert_eq!(grid.world_to_cell_dist(1.0), 1);
    assert_eq!(grid.world_to_cell_dist(40.0), 1);
    assert_eq!(grid.world_to_cell_dist(40.1), 2);
    assert_eq!(grid.world_to_cell_dist(800.0), 20);
}

#[test]
fn contains_is_exactly_the_grids_own_cells() {
    let grid = grid(16, 1);
    assert!(grid.contains(0.0, 0.0));
    assert!(grid.contains(639.9, 639.9));
    assert!(!grid.contains(640.0, 10.0));
    assert!(!grid.contains(10.0, 640.0));
    assert!(!grid.contains(-0.1, 10.0));
}

#[test]
fn a_non_finite_or_absurd_coordinate_is_answered_rather_than_panicking() {
    let mut grid = grid(8, 1);
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1.0e30, -1.0e30] {
        let _ = grid.world_to_cell(bad, bad);
        let _ = grid.world_to_cell_dist(bad);
        grid.reveal(bad, bad, 200.0, PlayerMask::of(P0));
        grid.reveal(0.0, 0.0, bad, PlayerMask::of(P0));
    }
    assert_eq!(grid.unbalanced_cells(), 0);
}

#[test]
fn a_player_past_the_grids_slots_is_refused_rather_than_wrapping() {
    let mut grid = grid(4, 2);
    let outsider = PlayerId::new(9);
    grid.add_looker(outsider, 1, 1);
    assert_eq!(grid.cell_state(P0, 1, 1), CellVisibility::Unexplored);
    assert_eq!(grid.cell_state(P1, 1, 1), CellVisibility::Unexplored);
    assert_eq!(grid.cell_state(outsider, 1, 1), CellVisibility::Unexplored);
}

#[test]
fn an_oversized_grid_is_refused_rather_than_allocated() {
    let huge = GridExtent {
        low_x: 0.0,
        low_y: 0.0,
        cell_size: 1.0,
        cells_x: 65_536,
        cells_y: 65_536,
    };
    assert!(matches!(
        VisibilityGrid::new(huge, 16),
        Err(Error::GridTooLarge { .. })
    ));
    assert!(matches!(
        VisibilityGrid::new(
            GridExtent {
                cells_x: 4,
                cells_y: 4,
                ..huge
            },
            17
        ),
        Err(Error::TooManyPlayers { asked: 17 })
    ));
}

#[test]
fn covering_derives_cell_counts_from_the_extent_and_survives_a_broken_one() {
    let normal = GridExtent::covering((0.0, 0.0), (6_500.0, 6_000.0), 40.0);
    assert_eq!((normal.cells_x, normal.cells_y), (163, 150));
    for broken in [
        GridExtent::covering((0.0, 0.0), (-1.0, -1.0), 40.0),
        GridExtent::covering((f32::NAN, 0.0), (10.0, 10.0), 40.0),
        GridExtent::covering((0.0, 0.0), (10.0, 10.0), 0.0),
        GridExtent::covering((0.0, 0.0), (1.0e30, 1.0e30), 40.0),
    ] {
        assert!(broken.cells_x >= 1 && broken.cells_x <= 65_536);
        assert!(broken.cells_y >= 1 && broken.cells_y <= 65_536);
        assert!(broken.cell_size > 0.0);
    }
}

#[test]
fn the_hash_covers_every_player_slot_and_not_just_one() {
    let mut a = grid(4, 3);
    let mut b = grid(4, 3);
    a.add_looker(P0, 1, 1);
    b.add_looker(P0, 1, 1);
    let mut hasher_a = StateHasher::new();
    let mut hasher_b = StateHasher::new();
    a.hash_into(&mut hasher_a);
    b.hash_into(&mut hasher_b);
    assert_eq!(hasher_a, hasher_b);

    b.add_looker(P1, 3, 3);
    let mut hasher_b = StateHasher::new();
    b.hash_into(&mut hasher_b);
    assert_ne!(
        hasher_a, hasher_b,
        "a second player's vision did not reach the hash"
    );
}

#[test]
fn the_reveal_order_over_players_is_ascending_and_the_result_does_not_depend_on_it() {
    let mut ascending = grid(16, 2);
    ascending.reveal(300.0, 300.0, 200.0, PlayerMask::of(P0).with(P1));

    let mut manual = grid(16, 2);
    manual.reveal(300.0, 300.0, 200.0, PlayerMask::of(P1));
    manual.reveal(300.0, 300.0, 200.0, PlayerMask::of(P0));

    let mut hasher_a = StateHasher::new();
    let mut hasher_b = StateHasher::new();
    ascending.hash_into(&mut hasher_a);
    manual.hash_into(&mut hasher_b);
    assert_eq!(hasher_a, hasher_b);
}

#[test]
fn a_player_mask_refuses_a_slot_past_the_sixteen_the_original_has() {
    assert!(PlayerMask::of(PlayerId::new(15)).holds(PlayerId::new(15)));
    assert_eq!(PlayerMask::of(PlayerId::new(16)), PlayerMask::NONE);
    assert!(!PlayerMask::ALL.holds(PlayerId::new(16)));
    assert!(PlayerMask::ALL.holds(PlayerId::new(0)));
}

fn shaped(x: f32, y: f32, facing: (f32, f32), side: f32, rear: f32) -> Sighting {
    Sighting {
        world_x: x,
        world_y: y,
        radius: 400.0,
        shape: crate::sighting::VisionShape::new(facing.0, facing.1, side, rear),
        mask: PlayerMask::of(P0),
    }
}

#[test]
fn a_shaped_reveal_reaches_the_full_radius_ahead_and_its_fraction_behind() {
    let mut grid = grid(64, 1);
    grid.reveal_sighting(&shaped(32.0 * 40.0, 32.0 * 40.0, (1.0, 0.0), 0.35, 0.25));
    let seen = |x: i32| grid.cell_state(P0, x, 32) == CellVisibility::Visible;
    assert!(seen(42), "ahead at the full radius");
    assert!(seen(29), "behind within the rear reach");
    assert!(!seen(28), "behind past the rear reach");
}

#[test]
fn a_shaped_reveal_and_its_undo_leave_every_counter_balanced() {
    let mut grid = grid(12, 1);
    let facings = [(1.0f32, 0.0f32), (0.6, -0.8), (-0.3, 0.95), (0.0, -1.0)];
    let at = [60.0f32, 150.0, 240.0, 330.0];
    for (facing, x) in facings.into_iter().zip(at) {
        grid.reveal_sighting(&shaped(x, 300.0, facing, 0.5, 0.25));
    }
    assert!(grid.watched_cells() > 0);
    for (facing, x) in facings.into_iter().zip(at) {
        grid.unreveal_sighting(&shaped(x, 300.0, facing, 0.5, 0.25));
    }
    assert_eq!(grid.watched_cells(), 0, "a cell was left permanently clear");
    assert_eq!(grid.unbalanced_cells(), 0);
}

#[test]
fn a_polygon_reveal_stays_visible_with_no_watcher_to_undo_it() {
    let mut grid = grid(8, 2);
    let mask = PlayerMask::of(P0);
    let square = [(80.0, 80.0), (160.0, 80.0), (160.0, 160.0), (80.0, 160.0)];
    grid.reveal_polygon_permanently(&square, mask);
    for (x, y) in [(2, 2), (3, 2), (2, 3), (3, 3)] {
        assert_eq!(
            grid.cell_state(P0, x, y),
            CellVisibility::Visible,
            "cell ({x},{y}) is inside the square"
        );
        assert!(grid.looker_count(P0, x, y) > 0);
    }
    assert_eq!(grid.cell_state(P0, 0, 0), CellVisibility::Unexplored);
    assert_eq!(
        grid.cell_state(P1, 2, 2),
        CellVisibility::Unexplored,
        "the mask named only player 0"
    );
}

#[test]
fn the_cached_grid_hash_tracks_every_reveal() {
    let mut grid = VisibilityGrid::new(
        GridExtent::covering((0.0, 0.0), (4_000.0, 4_000.0), 40.0),
        4,
    )
    .unwrap();
    let hashes = |grid: &mut VisibilityGrid| {
        let mut full = StateHasher::new();
        grid.hash_into(&mut full);
        let mut cached = StateHasher::new();
        grid.hash_into_cached(&mut cached);
        (full.finish(), cached.finish())
    };
    let (mut previous, cached) = hashes(&mut grid);
    assert_eq!(previous, cached);
    let mask = PlayerMask::of(PlayerId::new(2));
    for step in 0..60u16 {
        let at = f32::from(step) * 61.0;
        if step % 3 == 2 {
            grid.unreveal(at - 122.0, at - 122.0, 150.0, mask);
        } else {
            grid.reveal(at, at, 150.0, mask);
        }
        let (full, cached) = hashes(&mut grid);
        assert_eq!(full, cached, "step {step}");
        assert_ne!(full, previous, "step {step}");
        previous = full;
    }
}
