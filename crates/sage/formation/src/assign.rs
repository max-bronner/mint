use mint_movement::Vec2;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Assignment {
    pub slot_of_unit: Vec<Option<u16>>,
}

impl Assignment {
    #[must_use]
    pub fn filled(&self) -> usize {
        self.slot_of_unit
            .iter()
            .filter(|slot| slot.is_some())
            .count()
    }
}

fn cost(a: Vec2, b: Vec2) -> i64 {
    let squared = b.minus(a).length_squared();
    if !squared.is_finite() {
        return i64::MAX;
    }
    let clamped = squared.min(9.0e18_f32);
    #[allow(clippy::cast_possible_truncation)]
    let rounded = clamped.round() as i64;
    rounded
}

#[must_use]
pub fn assign_slots(units: &[Vec2], slot_poses: &[Vec2]) -> Assignment {
    let mut slot_of_unit: Vec<Option<u16>> = vec![None; units.len()];
    if units.is_empty() || slot_poses.is_empty() {
        return Assignment { slot_of_unit };
    }

    let mut centre = Vec2::ZERO;
    for pose in slot_poses {
        centre = centre.plus(*pose);
    }
    #[allow(clippy::cast_precision_loss)]
    let divisor = slot_poses.len() as f32;
    let centre = Vec2::new(centre.x / divisor, centre.y / divisor);

    let mut order: Vec<(i64, u16)> = slot_poses
        .iter()
        .enumerate()
        .filter_map(|(index, pose)| Some((cost(*pose, centre), u16::try_from(index).ok()?)))
        .collect();
    order.sort_unstable();

    let mut taken: Vec<bool> = vec![false; units.len()];
    for (_, slot) in order {
        let Some(pose) = usize::from(slot)
            .checked_sub(0)
            .and_then(|i| slot_poses.get(i))
        else {
            continue;
        };
        let mut best: Option<(i64, usize)> = None;
        for (index, unit) in units.iter().enumerate() {
            if taken.get(index).copied().unwrap_or(true) {
                continue;
            }
            let here = cost(*unit, *pose);
            if best.is_none_or(|(best_cost, _)| here < best_cost) {
                best = Some((here, index));
            }
        }
        let Some((_, index)) = best else {
            break;
        };
        if let Some(entry) = taken.get_mut(index) {
            *entry = true;
        }
        if let Some(entry) = slot_of_unit.get_mut(index) {
            *entry = Some(slot);
        }
    }
    Assignment { slot_of_unit }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn each_unit_takes_the_slot_it_is_nearest() {
        let units = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(100.0, 0.0),
            Vec2::new(50.0, 0.0),
        ];
        let slots = vec![
            Vec2::new(98.0, 0.0),
            Vec2::new(2.0, 0.0),
            Vec2::new(52.0, 0.0),
        ];
        let assignment = assign_slots(&units, &slots);
        assert_eq!(
            assignment.slot_of_unit,
            vec![Some(1), Some(0), Some(2)],
            "a unit walked past a free slot to reach a further one"
        );
        assert_eq!(assignment.filled(), 3);
    }

    #[test]
    fn the_same_inputs_assign_the_same_way_every_time() {
        let units: Vec<Vec2> = (0..20)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let f = i as f32;
                Vec2::new(f * 7.0 % 53.0, f * 13.0 % 31.0)
            })
            .collect();
        let slots: Vec<Vec2> = (0..20)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let f = i as f32;
                Vec2::new(20.0 + (f % 4.0) * 20.0, (f / 4.0).floor() * 20.0 - 40.0)
            })
            .collect();
        let first = assign_slots(&units, &slots);
        for _ in 0..8 {
            assert_eq!(assign_slots(&units, &slots), first);
        }
        let mut used: Vec<u16> = first.slot_of_unit.iter().filter_map(|s| *s).collect();
        used.sort_unstable();
        used.dedup();
        assert_eq!(used.len(), 20, "a slot was handed out twice");
    }

    #[test]
    fn a_perfect_tie_is_broken_by_declaration_and_input_order() {
        let units = vec![Vec2::ZERO; 4];
        let slots = vec![
            Vec2::new(10.0, 0.0),
            Vec2::new(-10.0, 0.0),
            Vec2::new(0.0, 10.0),
            Vec2::new(0.0, -10.0),
        ];
        let assignment = assign_slots(&units, &slots);
        assert_eq!(
            assignment.slot_of_unit,
            vec![Some(0), Some(1), Some(2), Some(3)],
            "the tie-break is not (slot declaration order, unit input order)"
        );
    }

    #[test]
    fn surplus_units_get_no_slot_rather_than_a_shared_one() {
        let units = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(500.0, 500.0),
        ];
        let slots = vec![Vec2::new(0.0, 0.0), Vec2::new(2.0, 0.0)];
        let assignment = assign_slots(&units, &slots);
        assert_eq!(assignment.filled(), 2);
        assert_eq!(
            assignment.slot_of_unit[2], None,
            "the far unit is the surplus"
        );
        assert_ne!(assignment.slot_of_unit[0], assignment.slot_of_unit[1]);
    }

    #[test]
    fn an_understrength_horde_fills_from_the_centre_out() {
        let slots = vec![
            Vec2::new(-40.0, 0.0),
            Vec2::new(0.0, 0.0),
            Vec2::new(40.0, 0.0),
        ];
        let units = vec![Vec2::new(0.0, 1000.0)];
        let assignment = assign_slots(&units, &slots);
        assert_eq!(
            assignment.slot_of_unit,
            vec![Some(1)],
            "the one member should take the middle slot, not an edge"
        );
    }

    #[test]
    fn the_centre_slots_fill_first_even_when_the_formation_is_offset() {
        let slots: Vec<Vec2> = (0..5)
            .map(|index| {
                #[allow(clippy::cast_precision_loss)]
                let offset = index as f32;
                Vec2::new(100.0 + offset * 20.0, 0.0)
            })
            .collect();
        let units = vec![Vec2::new(140.0, 5000.0)];
        let assignment = assign_slots(&units, &slots);
        assert_eq!(
            assignment.slot_of_unit,
            vec![Some(2)],
            "the member took an end slot, so 'centre' is being read as the origin"
        );
    }

    #[test]
    fn a_non_finite_position_sorts_last_rather_than_first() {
        let units = vec![
            Vec2::new(f32::NAN, 0.0),
            Vec2::new(0.0, 0.0),
            Vec2::new(f32::INFINITY, f32::NEG_INFINITY),
        ];
        let slots = vec![Vec2::new(0.0, 0.0), Vec2::new(1000.0, 1000.0)];
        let assignment = assign_slots(&units, &slots);
        assert_eq!(
            assignment.slot_of_unit[1],
            Some(0),
            "the finite unit must take the slot it is standing on"
        );
        assert_eq!(assignment.filled(), 2);

        assert_eq!(assign_slots(&[], &slots).slot_of_unit, Vec::new());
        assert_eq!(
            assign_slots(&units, &[]).slot_of_unit,
            vec![None, None, None]
        );
        let far = vec![Vec2::new(f32::MAX, f32::MAX)];
        assert_eq!(assign_slots(&far, &slots).filled(), 1);
    }
}
