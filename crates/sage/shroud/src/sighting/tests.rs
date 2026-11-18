use super::*;
use crate::grid::{CellVisibility, GridExtent, VisibilityGrid};
use mint_sim::PlayerId;

const P0: PlayerId = PlayerId::new(0);

fn grid() -> VisibilityGrid {
    VisibilityGrid::new(
        GridExtent {
            low_x: 0.0,
            low_y: 0.0,
            cell_size: 40.0,
            cells_x: 64,
            cells_y: 64,
        },
        2,
    )
    .unwrap()
}

fn sighting(x: f32, y: f32) -> Sighting {
    Sighting {
        world_x: x,
        world_y: y,
        radius: 200.0,
        shape: VisionShape::ROUND,
        mask: PlayerMask::of(P0),
    }
}

#[test]
fn a_looker_lays_a_disc_down_once_and_not_again_within_one_cell() {
    let mut grid = grid();
    let mut queue = UndoQueue::new();
    let mut looker = Looker::new();

    assert!(looker.update(
        &mut grid,
        &mut queue,
        Frame::new(0),
        DEFAULT_UNLOOK_PERSIST,
        sighting(1_000.0, 1_000.0)
    ));
    let after_first = grid.watched_cells();
    assert!(after_first > 0);

    assert!(!looker.update(
        &mut grid,
        &mut queue,
        Frame::new(1),
        DEFAULT_UNLOOK_PERSIST,
        sighting(1_030.0, 1_010.0)
    ));
    assert_eq!(grid.watched_cells(), after_first);
    assert!(queue.is_empty());
}

#[test]
fn crossing_a_cell_boundary_relays_the_disc_and_defers_the_old_one() {
    let mut grid = grid();
    let mut queue = UndoQueue::new();
    let mut looker = Looker::new();
    looker.update(
        &mut grid,
        &mut queue,
        Frame::new(0),
        DEFAULT_UNLOOK_PERSIST,
        sighting(1_000.0, 1_000.0),
    );
    let one_disc = grid.watched_cells();

    assert!(looker.update(
        &mut grid,
        &mut queue,
        Frame::new(1),
        DEFAULT_UNLOOK_PERSIST,
        sighting(1_045.0, 1_000.0)
    ));
    assert_eq!(queue.len(), 1);
    assert!(
        grid.watched_cells() > one_disc,
        "both discs should be down while the old one waits"
    );

    assert_eq!(queue.drain_due(&mut grid, Frame::new(2)), 0);
    assert_eq!(queue.drain_due(&mut grid, Frame::new(3)), 1);
    assert_eq!(grid.watched_cells(), one_disc);
}

#[test]
fn the_overlap_never_blinks_to_fog_while_a_unit_crosses_a_boundary() {
    let mut grid = grid();
    let mut queue = UndoQueue::new();
    let mut looker = Looker::new();
    let watched = (25, 25);

    let mut x = 1_000.0f32;
    for frame in 0..12u64 {
        let now = Frame::new(frame);
        looker.update(
            &mut grid,
            &mut queue,
            now,
            DEFAULT_UNLOOK_PERSIST,
            sighting(x, 1_000.0),
        );
        queue.drain_due(&mut grid, now);
        assert_eq!(
            grid.cell_state(P0, watched.0, watched.1),
            CellVisibility::Visible,
            "cell {watched:?} lost vision on frame {frame}"
        );
        x += 15.0;
    }
}

#[test]
fn a_looker_that_stops_gives_every_cell_back() {
    let mut grid = grid();
    let mut queue = UndoQueue::new();
    let mut looker = Looker::new();
    looker.update(
        &mut grid,
        &mut queue,
        Frame::new(0),
        DEFAULT_UNLOOK_PERSIST,
        sighting(1_000.0, 1_000.0),
    );
    looker.stop_looking(&mut queue, &mut grid, Frame::new(1), DEFAULT_UNLOOK_PERSIST);
    assert!(!looker.is_looking());
    queue.drain_all(&mut grid);
    assert_eq!(grid.watched_cells(), 0);
    assert_eq!(grid.unbalanced_cells(), 0);
    assert_eq!(grid.cell_state(P0, 25, 25), CellVisibility::Explored);
}

#[test]
fn a_long_random_walk_conserves_every_counter() {
    let mut grid = grid();
    let mut queue = UndoQueue::new();
    let mut looker = Looker::new();
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    let mut x = 1_200.0f32;
    let mut y = 1_200.0f32;
    let mut expected_reference = grid.clone();

    for frame in 0..200u64 {
        seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
        let step = |bits: u64| -> f32 { f32::from((bits % 61) as i8 - 30) };
        x = (x + step(seed >> 16)).clamp(100.0, 2_400.0);
        y = (y + step(seed >> 32)).clamp(100.0, 2_400.0);
        let now = Frame::new(frame);
        if looker.update(
            &mut grid,
            &mut queue,
            now,
            DEFAULT_UNLOOK_PERSIST,
            sighting(x, y),
        ) {
            expected_reference.reveal(x, y, 200.0, PlayerMask::of(P0));
        }
        queue.drain_due(&mut grid, now);
    }
    looker.stop_looking(
        &mut queue,
        &mut grid,
        Frame::new(200),
        DEFAULT_UNLOOK_PERSIST,
    );
    queue.drain_all(&mut grid);

    assert_eq!(grid.watched_cells(), 0, "a cell was left permanently clear");
    assert_eq!(grid.unbalanced_cells(), 0);

    let mut explored = 0usize;
    let mut union = 0usize;
    for cy in 0..64 {
        for cx in 0..64 {
            if grid.cell_state(P0, cx, cy) == CellVisibility::Explored {
                explored = explored.saturating_add(1);
            }
            if expected_reference.cell_state(P0, cx, cy) == CellVisibility::Visible {
                union = union.saturating_add(1);
            }
        }
    }
    assert_eq!(
        explored, union,
        "the explored set is not the union of the discs walked"
    );
    assert!(
        explored > 349,
        "the walk should have covered more than one disc"
    );
}

#[test]
fn a_changed_radius_relays_the_disc_even_inside_one_cell() {
    let mut grid = grid();
    let mut queue = UndoQueue::new();
    let mut looker = Looker::new();
    looker.update(
        &mut grid,
        &mut queue,
        Frame::new(0),
        DEFAULT_UNLOOK_PERSIST,
        sighting(1_000.0, 1_000.0),
    );
    let wider = Sighting {
        radius: 400.0,
        ..sighting(1_000.0, 1_000.0)
    };
    assert!(looker.update(
        &mut grid,
        &mut queue,
        Frame::new(1),
        DEFAULT_UNLOOK_PERSIST,
        wider
    ));
    queue.drain_all(&mut grid);
    assert_eq!(
        grid.watched_cells(),
        349,
        "the wider disc should be the one left"
    );
}

#[test]
fn a_full_queue_applies_the_oldest_undo_rather_than_dropping_it() {
    let mut grid = grid();
    let mut queue = UndoQueue::new();
    for frame in 0..1_000u64 {
        queue.queue_undo(
            &mut grid,
            sighting(1_000.0, 1_000.0),
            Frame::new(frame),
            DEFAULT_UNLOOK_PERSIST,
        );
        grid.reveal(1_000.0, 1_000.0, 200.0, PlayerMask::of(P0));
    }
    assert_eq!(queue.len(), 1_000);
    queue.drain_all(&mut grid);
    assert_eq!(grid.watched_cells(), 0);
    assert_eq!(grid.unbalanced_cells(), 0);
}

#[test]
fn the_queues_hash_changes_with_its_contents_and_its_order() {
    let mut grid = grid();
    let mut a = UndoQueue::new();
    let mut b = UndoQueue::new();
    a.queue_undo(
        &mut grid,
        sighting(100.0, 100.0),
        Frame::new(0),
        DEFAULT_UNLOOK_PERSIST,
    );
    a.queue_undo(
        &mut grid,
        sighting(200.0, 200.0),
        Frame::new(0),
        DEFAULT_UNLOOK_PERSIST,
    );
    b.queue_undo(
        &mut grid,
        sighting(200.0, 200.0),
        Frame::new(0),
        DEFAULT_UNLOOK_PERSIST,
    );
    b.queue_undo(
        &mut grid,
        sighting(100.0, 100.0),
        Frame::new(0),
        DEFAULT_UNLOOK_PERSIST,
    );

    let mut hasher_a = StateHasher::new();
    let mut hasher_b = StateHasher::new();
    a.hash_into(&mut hasher_a);
    b.hash_into(&mut hasher_b);
    assert_ne!(hasher_a, hasher_b, "the queue's order must reach the hash");
}

#[test]
fn a_watcher_that_disappears_has_its_disc_retired_without_any_despawn_hook() {
    let mut grid = grid();
    let mut queue = UndoQueue::new();
    let mut set = LookerSet::new();

    let two = [
        (1u64, sighting(800.0, 800.0)),
        (2u64, sighting(1_600.0, 1_600.0)),
    ];
    let report = set.reconcile(
        &mut grid,
        &mut queue,
        Frame::new(0),
        DEFAULT_UNLOOK_PERSIST,
        &two,
    );
    assert_eq!(report.added, 2);
    assert_eq!(set.len(), 2);
    let both = grid.watched_cells();
    assert!(both > 0);

    let one = [(2u64, sighting(1_600.0, 1_600.0))];
    let report = set.reconcile(
        &mut grid,
        &mut queue,
        Frame::new(1),
        DEFAULT_UNLOOK_PERSIST,
        &one,
    );
    assert_eq!(report.retired, 1);
    assert_eq!(report.unchanged, 1);
    queue.drain_all(&mut grid);
    assert!(grid.watched_cells() < both);
    assert_eq!(grid.unbalanced_cells(), 0);

    set.reconcile(
        &mut grid,
        &mut queue,
        Frame::new(2),
        DEFAULT_UNLOOK_PERSIST,
        &[],
    );
    queue.drain_all(&mut grid);
    assert_eq!(grid.watched_cells(), 0);
    assert!(set.is_empty());
}

#[test]
fn a_stationary_crowd_costs_nothing_after_the_first_tick() {
    let mut grid = grid();
    let mut queue = UndoQueue::new();
    let mut set = LookerSet::new();
    let crowd: Vec<(u64, Sighting)> = (0..64u64)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let offset = i as f32;
            (i, sighting(400.0 + offset * 5.0, 400.0))
        })
        .collect();

    let first = set.reconcile(
        &mut grid,
        &mut queue,
        Frame::new(0),
        DEFAULT_UNLOOK_PERSIST,
        &crowd,
    );
    assert_eq!(first.added, 64);
    for frame in 1..10u64 {
        let again = set.reconcile(
            &mut grid,
            &mut queue,
            Frame::new(frame),
            DEFAULT_UNLOOK_PERSIST,
            &crowd,
        );
        assert_eq!(
            again.unchanged, 64,
            "frame {frame} did work it did not need to"
        );
        assert_eq!(again.relaid, 0);
        assert!(queue.is_empty());
    }
}

#[test]
fn reconcile_keeps_the_same_pair_of_allocations_once_the_population_is_stable() {
    let mut grid = grid();
    let mut queue = UndoQueue::new();
    let mut set = LookerSet::new();
    let crowd: Vec<(u64, Sighting)> = (0..300u64)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let offset = i as f32;
            (i, sighting(2_000.0 + offset * 5.0, 2_000.0))
        })
        .collect();

    set.reconcile(
        &mut grid,
        &mut queue,
        Frame::new(0),
        DEFAULT_UNLOOK_PERSIST,
        &crowd,
    );
    let buffer_a = set.watchers.as_ptr();
    set.reconcile(
        &mut grid,
        &mut queue,
        Frame::new(1),
        DEFAULT_UNLOOK_PERSIST,
        &crowd,
    );
    let buffer_b = set.watchers.as_ptr();
    assert_ne!(
        buffer_a, buffer_b,
        "the warm-up calls did not allocate two distinct buffers"
    );

    set.reconcile(
        &mut grid,
        &mut queue,
        Frame::new(2),
        DEFAULT_UNLOOK_PERSIST,
        &crowd,
    );
    assert_eq!(
        set.watchers.as_ptr(),
        buffer_a,
        "reconcile allocated a third buffer instead of swapping back to the first - \
             the per-tick allocation the spike round removed is back"
    );
    set.reconcile(
        &mut grid,
        &mut queue,
        Frame::new(3),
        DEFAULT_UNLOOK_PERSIST,
        &crowd,
    );
    assert_eq!(
        set.watchers.as_ptr(),
        buffer_b,
        "reconcile allocated a fourth buffer instead of swapping back to the second - \
             the per-tick allocation the spike round removed is back"
    );
}

#[test]
fn an_unsorted_caller_still_leaves_the_grid_balanced_and_the_set_sorted() {
    let mut grid = grid();
    let mut queue = UndoQueue::new();
    let mut set = LookerSet::new();
    let jumbled = [
        (9u64, sighting(400.0, 400.0)),
        (2u64, sighting(800.0, 800.0)),
        (7u64, sighting(1_200.0, 1_200.0)),
    ];
    set.reconcile(
        &mut grid,
        &mut queue,
        Frame::new(0),
        DEFAULT_UNLOOK_PERSIST,
        &jumbled,
    );
    assert_eq!(set.len(), 3);
    set.reconcile(
        &mut grid,
        &mut queue,
        Frame::new(1),
        DEFAULT_UNLOOK_PERSIST,
        &jumbled,
    );
    assert_eq!(set.len(), 3, "a duplicate id entered the set");
    set.reconcile(
        &mut grid,
        &mut queue,
        Frame::new(2),
        DEFAULT_UNLOOK_PERSIST,
        &[],
    );
    queue.drain_all(&mut grid);
    assert_eq!(grid.watched_cells(), 0);
    assert_eq!(grid.unbalanced_cells(), 0);
}

#[test]
fn clearing_the_set_gives_every_cell_back() {
    let mut grid = grid();
    let mut queue = UndoQueue::new();
    let mut set = LookerSet::new();
    let watchers: Vec<(u64, Sighting)> = (0..8u64)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let offset = i as f32;
            (i, sighting(300.0 + offset * 120.0, 700.0))
        })
        .collect();
    set.reconcile(
        &mut grid,
        &mut queue,
        Frame::new(0),
        DEFAULT_UNLOOK_PERSIST,
        &watchers,
    );
    set.clear(&mut grid, &mut queue, Frame::new(1), DEFAULT_UNLOOK_PERSIST);
    queue.drain_all(&mut grid);
    assert!(set.is_empty());
    assert_eq!(grid.watched_cells(), 0);
    assert_eq!(grid.unbalanced_cells(), 0);
}

#[test]
fn the_sets_hash_moves_with_who_is_watching_and_from_where() {
    let mut grid = grid();
    let mut queue = UndoQueue::new();
    let mut a = LookerSet::new();
    let mut b = LookerSet::new();
    a.reconcile(
        &mut grid,
        &mut queue,
        Frame::new(0),
        DEFAULT_UNLOOK_PERSIST,
        &[(1u64, sighting(400.0, 400.0))],
    );
    b.reconcile(
        &mut grid,
        &mut queue,
        Frame::new(0),
        DEFAULT_UNLOOK_PERSIST,
        &[(1u64, sighting(1_200.0, 400.0))],
    );
    let mut hasher_a = StateHasher::new();
    let mut hasher_b = StateHasher::new();
    a.hash_into(&mut hasher_a);
    b.hash_into(&mut hasher_b);
    assert_ne!(hasher_a, hasher_b);
}
