use mint_sim::{PlayerId, StateHasher};

use crate::disc::{circle_fill, small_fill};
use crate::grid::{CellVisibility, VisibilityGrid};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ObjectVisibility {
    Shrouded,
    Fogged,
    PartialClear,
    Clear,
}

impl ObjectVisibility {
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Shrouded => 0,
            Self::Fogged => 1,
            Self::PartialClear => 2,
            Self::Clear => 3,
        }
    }

    pub fn hash_into(self, hasher: &mut StateHasher) {
        hasher.write_u8(self.code());
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ObjectTraits {
    pub immobile: bool,
    pub mine: bool,
    pub neutral_to_viewer: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shroudedness {
    pub visibility: ObjectVisibility,
    pub ever_seen: bool,
    pub remembered: bool,
}

#[must_use]
pub fn shroudedness(
    grid: &VisibilityGrid,
    player: PlayerId,
    cells: &[(i32, i32)],
    traits: ObjectTraits,
    ever_seen: bool,
) -> Shroudedness {
    if cells.is_empty() {
        return Shroudedness {
            visibility: ObjectVisibility::Shrouded,
            ever_seen: false,
            remembered: false,
        };
    }

    let mut unexplored = 0usize;
    let mut explored = 0usize;
    for &(x, y) in cells {
        match grid.cell_state(player, x, y) {
            CellVisibility::Unexplored => unexplored = unexplored.saturating_add(1),
            CellVisibility::Explored => explored = explored.saturating_add(1),
            CellVisibility::Visible => {}
        }
    }
    let total = cells.len();
    let fogged_or_worse = unexplored.saturating_add(explored);

    if unexplored == total {
        return Shroudedness {
            visibility: ObjectVisibility::Shrouded,
            ever_seen: false,
            remembered: false,
        };
    }

    if fogged_or_worse == total {
        let survives_fog = if traits.neutral_to_viewer {
            traits.immobile
        } else {
            traits.immobile && ever_seen && !traits.mine
        };
        return if survives_fog {
            Shroudedness {
                visibility: ObjectVisibility::Fogged,
                ever_seen,
                remembered: true,
            }
        } else {
            Shroudedness {
                visibility: ObjectVisibility::Shrouded,
                ever_seen,
                remembered: false,
            }
        };
    }

    let visibility = if unexplored == 0 && explored == 0 {
        ObjectVisibility::Clear
    } else {
        ObjectVisibility::PartialClear
    };
    Shroudedness {
        visibility,
        ever_seen: true,
        remembered: false,
    }
}

#[must_use]
pub fn cells_touched(
    grid: &VisibilityGrid,
    world_x: f32,
    world_y: f32,
    radius: f32,
    small: bool,
) -> Vec<(i32, i32)> {
    if !world_x.is_finite() || !world_y.is_finite() || !radius.is_finite() {
        return Vec::new();
    }
    let raw = if small {
        let low = grid.world_to_cell(world_x - radius, world_y - radius);
        let high = grid.world_to_cell(world_x + radius, world_y + radius);
        small_fill(low, high)
    } else {
        let (centre_x, centre_y) = grid.world_to_cell(world_x, world_y);
        let cell_radius = grid.world_to_cell_dist(radius).max(1);
        circle_fill(centre_x, centre_y, cell_radius)
    };
    let extent = grid.extent();
    raw.into_iter()
        .filter(|&(x, y)| {
            u32::try_from(x).is_ok_and(|c| c < extent.cells_x)
                && u32::try_from(y).is_ok_and(|r| r < extent.cells_y)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grid::{GridExtent, PlayerMask};

    const P0: PlayerId = PlayerId::new(0);
    const P1: PlayerId = PlayerId::new(1);

    fn grid() -> VisibilityGrid {
        VisibilityGrid::new(
            GridExtent {
                low_x: 0.0,
                low_y: 0.0,
                cell_size: 40.0,
                cells_x: 32,
                cells_y: 32,
            },
            2,
        )
        .unwrap()
    }

    const UNIT: ObjectTraits = ObjectTraits {
        immobile: false,
        mine: false,
        neutral_to_viewer: false,
    };
    const BUILDING: ObjectTraits = ObjectTraits {
        immobile: true,
        mine: false,
        neutral_to_viewer: false,
    };

    #[test]
    fn an_object_off_the_map_is_shrouded_and_never_seen() {
        let grid = grid();
        let result = shroudedness(&grid, P0, &[], BUILDING, true);
        assert_eq!(result.visibility, ObjectVisibility::Shrouded);
        assert!(!result.ever_seen, "everSeen is forced false off the map");
    }

    #[test]
    fn a_building_seen_once_is_remembered_in_fog_and_a_unit_is_not() {
        let mut grid = grid();
        let cells = cells_touched(&grid, 400.0, 400.0, 30.0, false);
        assert!(!cells.is_empty());

        assert_eq!(
            shroudedness(&grid, P0, &cells, BUILDING, false).visibility,
            ObjectVisibility::Shrouded
        );

        grid.reveal(400.0, 400.0, 200.0, PlayerMask::of(P0));
        let seen = shroudedness(&grid, P0, &cells, BUILDING, false);
        assert_eq!(seen.visibility, ObjectVisibility::Clear);
        assert!(seen.ever_seen);
        grid.unreveal(400.0, 400.0, 200.0, PlayerMask::of(P0));

        let building = shroudedness(&grid, P0, &cells, BUILDING, seen.ever_seen);
        assert_eq!(building.visibility, ObjectVisibility::Fogged);
        assert!(building.remembered, "a seen building is remembered");

        let unit = shroudedness(&grid, P0, &cells, UNIT, seen.ever_seen);
        assert_eq!(
            unit.visibility,
            ObjectVisibility::Shrouded,
            "a unit that walks out of sight vanishes"
        );
        assert!(!unit.remembered);
    }

    #[test]
    fn a_building_standing_on_explored_ground_it_was_never_seen_on_is_still_hidden() {
        let mut grid = grid();
        grid.reveal_all(P0);
        let cells = cells_touched(&grid, 400.0, 400.0, 30.0, false);
        let result = shroudedness(&grid, P0, &cells, BUILDING, false);
        assert_eq!(result.visibility, ObjectVisibility::Shrouded);
        assert!(!result.remembered);
        assert_eq!(
            shroudedness(&grid, P0, &cells, BUILDING, true).visibility,
            ObjectVisibility::Fogged
        );
    }

    #[test]
    fn a_mine_is_demoted_even_though_it_is_immobile_and_seen() {
        let mut grid = grid();
        let cells = cells_touched(&grid, 400.0, 400.0, 30.0, false);
        grid.reveal(400.0, 400.0, 200.0, PlayerMask::of(P0));
        grid.unreveal(400.0, 400.0, 200.0, PlayerMask::of(P0));
        let mine = ObjectTraits {
            mine: true,
            ..BUILDING
        };
        assert_eq!(
            shroudedness(&grid, P0, &cells, mine, true).visibility,
            ObjectVisibility::Shrouded
        );
    }

    #[test]
    fn a_neutral_mover_is_never_remembered_but_a_neutral_structure_is() {
        let mut grid = grid();
        let cells = cells_touched(&grid, 400.0, 400.0, 30.0, false);
        grid.reveal(400.0, 400.0, 200.0, PlayerMask::of(P0));
        grid.unreveal(400.0, 400.0, 200.0, PlayerMask::of(P0));

        let neutral_mover = ObjectTraits {
            neutral_to_viewer: true,
            ..UNIT
        };
        let neutral_structure = ObjectTraits {
            neutral_to_viewer: true,
            ..BUILDING
        };
        assert_eq!(
            shroudedness(&grid, P0, &cells, neutral_mover, true).visibility,
            ObjectVisibility::Shrouded
        );
        assert_eq!(
            shroudedness(&grid, P0, &cells, neutral_structure, false).visibility,
            ObjectVisibility::Fogged
        );
    }

    #[test]
    fn a_partly_covered_object_is_partial_clear_and_counts_as_seen() {
        let mut grid = grid();
        let cells = cells_touched(&grid, 400.0, 400.0, 200.0, false);
        assert!(cells.len() > 8);
        grid.reveal(400.0, 400.0, 40.0, PlayerMask::of(P0));
        let result = shroudedness(&grid, P0, &cells, BUILDING, false);
        assert_eq!(result.visibility, ObjectVisibility::PartialClear);
        assert!(result.ever_seen);
    }

    #[test]
    fn a_building_whose_ground_goes_fully_unexplored_again_is_forgotten() {
        let mut grid = grid();
        let cells = cells_touched(&grid, 400.0, 400.0, 30.0, false);
        grid.reveal(400.0, 400.0, 200.0, PlayerMask::of(P0));
        grid.unreveal(400.0, 400.0, 200.0, PlayerMask::of(P0));
        assert!(shroudedness(&grid, P0, &cells, BUILDING, true).ever_seen);

        for &(x, y) in &cells {
            grid.add_shrouder(P0, x, y);
        }
        let forgotten = shroudedness(&grid, P0, &cells, BUILDING, true);
        assert_eq!(forgotten.visibility, ObjectVisibility::Shrouded);
        assert!(!forgotten.ever_seen);
    }

    #[test]
    fn shroudedness_is_computed_per_player_and_one_players_answer_is_not_the_others() {
        let mut grid = grid();
        let cells = cells_touched(&grid, 400.0, 400.0, 30.0, false);
        grid.reveal(400.0, 400.0, 200.0, PlayerMask::of(P0));
        assert_eq!(
            shroudedness(&grid, P0, &cells, UNIT, false).visibility,
            ObjectVisibility::Clear
        );
        assert_eq!(
            shroudedness(&grid, P1, &cells, UNIT, false).visibility,
            ObjectVisibility::Shrouded
        );
    }

    #[test]
    fn cells_touched_drops_what_is_off_the_grid_rather_than_clamping_it() {
        let grid = grid();
        let corner = cells_touched(&grid, 0.0, 0.0, 200.0, false);
        let middle = cells_touched(&grid, 600.0, 600.0, 200.0, false);
        assert!(!corner.is_empty());
        assert!(
            corner.len() < middle.len(),
            "a corner object touched {} cells and a middle one {}",
            corner.len(),
            middle.len()
        );
        for &(x, y) in &corner {
            assert!((0..32).contains(&x) && (0..32).contains(&y));
        }
    }

    #[test]
    fn a_small_object_touches_between_one_and_four_cells() {
        let grid = grid();
        assert_eq!(cells_touched(&grid, 420.0, 420.0, 5.0, true).len(), 1);
        assert_eq!(cells_touched(&grid, 400.0, 400.0, 5.0, true).len(), 4);
    }

    #[test]
    fn an_object_entirely_off_the_grid_touches_nothing() {
        let grid = grid();
        assert!(cells_touched(&grid, -5_000.0, -5_000.0, 30.0, false).is_empty());
        assert!(cells_touched(&grid, f32::NAN, f32::NAN, 30.0, true).is_empty());
    }
}
