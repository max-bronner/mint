#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Candidate {
    pub index: u32,
    pub distance_squared: f32,
    pub priority: i32,
}

pub fn nearest_target(candidates: &mut [Candidate]) -> Option<Candidate> {
    sort_candidates(candidates);
    candidates.first().copied()
}

pub fn sort_candidates(candidates: &mut [Candidate]) {
    candidates.sort_unstable_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| a.distance_squared.total_cmp(&b.distance_squared))
            .then_with(|| a.index.cmp(&b.index))
    });
}

pub fn sort_victims(victims: &mut [u32]) {
    victims.sort_unstable();
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn candidate(index: u32, distance: f32) -> Candidate {
        Candidate {
            index,
            distance_squared: distance,
            priority: 0,
        }
    }

    #[test]
    fn two_targets_at_exactly_equal_range_break_the_tie_on_entity_index() {
        let mut forwards = [candidate(7, 400.0), candidate(3, 400.0)];
        let mut backwards = [candidate(3, 400.0), candidate(7, 400.0)];
        assert_eq!(nearest_target(&mut forwards).unwrap().index, 3);
        assert_eq!(nearest_target(&mut backwards).unwrap().index, 3);
    }

    #[test]
    fn the_nearest_target_wins_when_the_distances_differ() {
        let mut candidates = [
            candidate(1, 900.0),
            candidate(2, 100.0),
            candidate(3, 400.0),
        ];
        assert_eq!(nearest_target(&mut candidates).unwrap().index, 2);
        assert_eq!(
            candidates.iter().map(|c| c.index).collect::<Vec<_>>(),
            vec![2, 3, 1]
        );
    }

    #[test]
    fn a_higher_priority_target_wins_over_a_nearer_one() {
        let mut candidates = [
            Candidate {
                index: 1,
                distance_squared: 100.0,
                priority: 1,
            },
            Candidate {
                index: 2,
                distance_squared: 900.0,
                priority: 5,
            },
            Candidate {
                index: 0,
                distance_squared: 900.0,
                priority: 5,
            },
        ];
        assert_eq!(nearest_target(&mut candidates).unwrap().index, 0);
    }

    #[test]
    fn a_non_finite_distance_sorts_last_rather_than_corrupting_the_order() {
        let mut candidates = [
            candidate(1, f32::NAN),
            candidate(2, 400.0),
            candidate(3, f32::INFINITY),
            candidate(4, 100.0),
        ];
        assert_eq!(nearest_target(&mut candidates).unwrap().index, 4);
        assert_eq!(
            candidates.iter().map(|c| c.index).collect::<Vec<_>>(),
            vec![4, 2, 3, 1]
        );
    }

    #[test]
    fn no_candidates_means_no_target() {
        assert_eq!(nearest_target(&mut []), None);
    }

    #[test]
    fn area_damage_victims_are_ordered_by_index_whatever_order_they_arrived_in() {
        let mut forwards = [9u32, 2, 5];
        let mut backwards = [5u32, 9, 2];
        sort_victims(&mut forwards);
        sort_victims(&mut backwards);
        assert_eq!(forwards, [2, 5, 9]);
        assert_eq!(forwards, backwards);
    }
}
