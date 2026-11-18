use super::*;
use crate::CLIFF;

struct Ground {
    extent: GridExtent,
    flags: Vec<CellFlags>,
}

impl Ground {
    fn new(flag_at: impl Fn(f32, f32) -> CellFlags) -> Self {
        let extent = GridExtent::new(100, 100).unwrap();
        let mut flags = Vec::new();
        for y in 0..100u16 {
            for x in 0..100u16 {
                flags.push(flag_at(f32::from(x) * 10.0, f32::from(y) * 10.0));
            }
        }
        Self { extent, flags }
    }
}

impl Passability for Ground {
    fn extent(&self) -> GridExtent {
        self.extent
    }
    fn flags(&self, cell: Cell) -> CellFlags {
        self.extent
            .index(cell)
            .and_then(|i| self.flags.get(i as usize).copied())
            .unwrap_or(CellFlags::NONE)
    }
    fn extra_cost(&self, _: Cell) -> u32 {
        0
    }
}

fn geometry() -> CellGeometry {
    CellGeometry::new(10.0, Vec2::ZERO).unwrap()
}

const FINISHED_7: Claimer = Claimer {
    key: 7,
    player: 1,
    finished: true,
};

const SIM: Claimer = Claimer {
    key: 9,
    player: 2,
    finished: false,
};

fn held_left_of(grid: &ClaimGrid, x: f32) -> ClaimLayer {
    let mut layer = ClaimLayer::empty(grid);
    let cells: Vec<u32> = (0..grid.extent.cell_count())
        .filter(|&i| {
            let cell = grid.extent.cell_at_index(i).unwrap();
            grid.geometry.centre_of(cell).x < x
        })
        .collect();
    layer.take(&cells, FINISHED_7);
    layer
}

const HALF: u64 = u64::MAX;

#[test]
fn the_share_is_the_usable_part_of_the_radius() {
    let open = |_: f32, _: f32| CellFlags::NONE;
    let grid = ClaimGrid::new(20.0, geometry(), Ground::new(open).extent).unwrap();
    let free = ClaimLayer::empty(&grid);
    let at = Vec2::new(500.0, 500.0);
    let footprint = |x: f32, y: f32| {
        if (480.0..520.0).contains(&x) && (480.0..520.0).contains(&y) {
            OBSTACLE
        } else {
            CellFlags::NONE
        }
    };
    let cliff = |x: f32, _: f32| if x >= 500.0 { CLIFF } else { CellFlags::NONE };
    let held = held_left_of(&grid, 500.0);
    let own: Vec<Cell> = (48..52u16)
        .flat_map(|y| (48..52u16).map(move |x| Cell::new(x, y)))
        .collect();
    let rows: Vec<(&str, Ground, &ClaimLayer, Vec2, &[Cell], u64)> = vec![
        ("open ground", Ground::new(open), &free, at, &[], 0),
        (
            "half over another claim",
            Ground::new(open),
            &held,
            at,
            &[],
            HALF,
        ),
        (
            "a cliff over half",
            Ground::new(cliff),
            &free,
            at,
            &[],
            HALF,
        ),
        (
            "the map edge through the centre",
            Ground::new(open),
            &free,
            Vec2::new(0.0, 500.0),
            &[],
            HALF,
        ),
        (
            "a 40-unit footprint",
            Ground::new(footprint),
            &free,
            at,
            &[],
            4,
        ),
        (
            "its own footprint",
            Ground::new(footprint),
            &free,
            at,
            &own,
            0,
        ),
    ];
    for (case, ground, layer, centre, own, lost) in rows {
        let usability = ClaimGround {
            grid: &grid,
            ground: &ground,
            ground_geometry: geometry(),
            layer,
            claimer: SIM,
            own,
            viewer: None,
        };
        let claim = assess(&usability, centre, 100.0).unwrap();
        let taken = claim.cells.len() as u64;
        if lost == HALF {
            assert_eq!(claim.share(), 0.5, "{case}: {taken} of {}", claim.wanted);
        } else {
            assert_eq!(
                claim.wanted - taken,
                lost,
                "{case}: {taken} of {}",
                claim.wanted
            );
        }
    }
}

fn held_where(grid: &ClaimGrid, held: impl Fn(Vec2) -> bool) -> ClaimLayer {
    let mut layer = ClaimLayer::empty(grid);
    let cells: Vec<u32> = (0..grid.extent.cell_count())
        .filter(|&i| {
            held(
                grid.geometry
                    .centre_of(grid.extent.cell_at_index(i).unwrap()),
            )
        })
        .collect();
    layer.take(&cells, FINISHED_7);
    layer
}

#[test]
fn the_fade_weighs_each_sample_by_the_unusable_cells_round_it() {
    let ground = Ground::new(|_, _| CellFlags::NONE);
    let grid = ClaimGrid::new(20.0, geometry(), ground.extent).unwrap();
    let fade_over = |layer: &ClaimLayer| {
        let usability = ClaimGround {
            grid: &grid,
            ground: &ground,
            ground_geometry: geometry(),
            layer,
            claimer: SIM,
            own: &[],
            viewer: Some(Viewer {
                unexplored: &|_| false,
                fogged: &|_| false,
                hidden: &|_| false,
            }),
        };
        fade(&usability, Vec2::new(500.0, 500.0), 100.0).unwrap()
    };
    let left = fade_over(&held_where(&grid, |c| c.x < 500.0));
    let rows = [
        ("inside a held cell", 490.0, 0.0),
        ("beside it, every left neighbour lost", 500.0, 2.0 / 12.0),
        ("one sample on, only the far neighbour", 510.0, 11.0 / 12.0),
        ("clear of it", 520.0, 1.0),
        (
            "between two samples",
            505.0,
            (2.0 / 12.0 + 11.0 / 12.0) / 2.0,
        ),
    ];
    for (case, x, weight) in rows {
        let got = left.at(Vec2::new(x, 500.0));
        assert!((got - weight).abs() < 1e-6, "{case}: {got} != {weight}");
    }
    let below = fade_over(&held_where(&grid, |c| c.x < 500.0 && c.y < 500.0));
    let above = fade_over(&held_where(&grid, |c| c.x < 500.0 && c.y > 500.0));
    assert_eq!(below.at(Vec2::new(500.0, 500.0)), 10.0 / 12.0);
    assert_eq!(above.at(Vec2::new(500.0, 490.0)), 9.0 / 12.0);
    let lone = fade_over(&held_where(&grid, |c| c.x == 510.0 && c.y == 510.0));
    assert_eq!(lone.at(Vec2::new(500.0, 500.0)), 0.0);
    assert!(fade_over(&ClaimLayer::empty(&grid))
        .weights
        .iter()
        .all(|w| *w == 1.0));
}

#[test]
fn a_viewer_cannot_claim_ground_they_have_never_explored() {
    let ground = Ground::new(|_, _| CellFlags::NONE);
    let grid = ClaimGrid::new(20.0, geometry(), ground.extent).unwrap();
    let free = ClaimLayer::empty(&grid);
    let unexplored = |at: Vec2| at.x < 500.0;
    fn view<'a>(
        grid: &'a ClaimGrid,
        ground: &'a Ground,
        layer: &'a ClaimLayer,
        unexplored: Option<&'a dyn Fn(Vec2) -> bool>,
    ) -> ClaimGround<'a> {
        ClaimGround {
            grid,
            ground,
            ground_geometry: geometry(),
            layer,
            claimer: SIM,
            own: &[],
            viewer: unexplored.map(|unexplored| Viewer {
                unexplored,
                fogged: &|_| false,
                hidden: &|_| false,
            }),
        }
    }
    let seen = view(&grid, &ground, &free, Some(&unexplored));
    let unlimited = view(&grid, &ground, &free, None);
    let at = Vec2::new(500.0, 500.0);
    assert_eq!(assess(&unlimited, at, 100.0).unwrap().share(), 1.0);
    assert_eq!(assess(&seen, at, 100.0).unwrap().share(), 0.5);
    let faded = fade(&seen, at, 100.0).unwrap();
    assert_eq!(faded.at(Vec2::new(490.0, 500.0)), 0.0);
    assert_eq!(faded.at(Vec2::new(520.0, 500.0)), 1.0);
}

fn one_cell(grid: &ClaimGrid) -> Vec<u32> {
    vec![grid.extent.index(Cell::new(10, 10)).unwrap()]
}

fn claimable(
    grid: &ClaimGrid,
    ground: &Ground,
    layer: &ClaimLayer,
    claimer: Claimer,
    viewer: Option<Viewer<'_>>,
) -> bool {
    let usability = ClaimGround {
        grid,
        ground,
        ground_geometry: geometry(),
        layer,
        claimer,
        own: &[],
        viewer,
    };
    usability.cell(Cell::new(10, 10)).unwrap().1
}

#[test]
fn an_unfinished_claim_holds_only_against_its_own_player_until_one_finishes() {
    let ground = Ground::new(|_, _| CellFlags::NONE);
    let grid = ClaimGrid::new(20.0, geometry(), ground.extent).unwrap();
    let cell = one_cell(&grid);
    let mine = |key, finished| Claimer {
        key,
        player: 1,
        finished,
    };
    let theirs = |key, finished| Claimer {
        key,
        player: 2,
        finished,
    };
    let mut layer = ClaimLayer::empty(&grid);
    assert_eq!(layer.take(&cell, mine(1, false)), 1, "a free cell");
    assert!(claimable(&grid, &ground, &layer, theirs(2, false), None));
    assert_eq!(
        layer.take(&cell, theirs(2, false)),
        1,
        "another player shares it"
    );
    assert!(claimable(&grid, &ground, &layer, mine(3, false), None));
    assert_eq!(
        layer.take(&cell, mine(3, false)),
        0,
        "not a second claim of the same player"
    );
    assert!(layer.holds(cell[0], 1) && layer.holds(cell[0], 2) && !layer.holds(cell[0], 3));

    let preview = Viewer {
        unexplored: &|_| false,
        fogged: &|_| false,
        hidden: &|_| false,
    };
    let viewer = |player| Claimer {
        key: 0,
        player,
        finished: false,
    };
    assert!(!claimable(&grid, &ground, &layer, viewer(1), Some(preview)));
    assert!(claimable(&grid, &ground, &layer, viewer(3), Some(preview)));

    assert_eq!(
        layer.take(&cell, theirs(2, true)),
        1,
        "the first to finish takes it"
    );
    assert!(layer.holds(cell[0], 2) && !layer.holds(cell[0], 1));
    assert!(!claimable(&grid, &ground, &layer, mine(1, false), None));
    assert!(!claimable(&grid, &ground, &layer, mine(1, true), None));
    assert!(
        claimable(&grid, &ground, &layer, theirs(2, true), None),
        "its own, again"
    );
    assert!(!claimable(&grid, &ground, &layer, viewer(1), Some(preview)));
    let fogged = Viewer {
        fogged: &|_| true,
        ..preview
    };
    let hidden = Viewer {
        hidden: &|key| key == 2,
        ..preview
    };
    assert!(claimable(&grid, &ground, &layer, viewer(1), Some(fogged)));
    assert!(claimable(&grid, &ground, &layer, viewer(1), Some(hidden)));
}

#[test]
fn leaving_a_shared_cell_frees_it_only_from_the_front_of_its_list() {
    let ground = Ground::new(|_, _| CellFlags::NONE);
    let grid = ClaimGrid::new(20.0, geometry(), ground.extent).unwrap();
    let cell = one_cell(&grid);
    let claimer = |key, player| Claimer {
        key,
        player,
        finished: false,
    };
    let mut layer = ClaimLayer::empty(&grid);
    for (key, player) in [(1, 1), (2, 2), (3, 3)] {
        layer.take(&cell, claimer(key, player));
    }
    layer.release(2);
    assert!(layer.holds(cell[0], 1) && layer.holds(cell[0], 3) && !layer.holds(cell[0], 2));
    layer.release(1);
    assert!(!layer.holds(cell[0], 3), "the front left: the cell is free");
    assert_eq!(layer.take(&cell, claimer(4, 3)), 1);
}
