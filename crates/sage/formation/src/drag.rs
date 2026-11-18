use mint_movement::{Facing, Vec2};

use crate::assistant::FormationTemplate;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragUnit {
    pub offset: Vec2,
    pub class: Option<usize>,
    pub width: f32,
    pub depth: f32,
    pub template: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragSlot {
    pub unit: usize,
    pub local: Vec2,
}

#[allow(
    clippy::arithmetic_side_effects,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation
)]
fn centre_out(n: usize) -> Vec<usize> {
    if n == 0 {
        return Vec::new();
    }
    let middle = (n - 1) / 2;
    let mut order = vec![middle];
    let mut k: usize = 1;
    let mut sign = 1i64;
    while order.len() < n && k <= n {
        let idx = middle as i64 + sign * k as i64;
        if idx >= 0 {
            let idx = idx as usize;
            if idx < n {
                order.push(idx);
            }
        }
        if sign == 1 {
            sign = -1;
        } else {
            sign = 1;
            k += 1;
        }
    }
    order
}

fn fill_row(
    row: &[Vec<usize>],
    units: &[DragUnit],
    order: &[usize],
    placed: &mut [bool],
) -> Option<Vec<Option<usize>>> {
    let mut cells: Vec<Option<usize>> = vec![None; row.len()];
    let mut any = false;
    for cell_index in centre_out(row.len()) {
        let Some(classes) = row.get(cell_index) else {
            continue;
        };
        for &class in classes {
            if let Some(&unit) = order.iter().find(|&&u| {
                !placed.get(u).copied().unwrap_or(true)
                    && units.get(u).is_some_and(|unit| unit.class == Some(class))
            }) {
                if let Some(cell) = cells.get_mut(cell_index) {
                    *cell = Some(unit);
                }
                if let Some(flag) = placed.get_mut(unit) {
                    *flag = true;
                }
                any = true;
                break;
            }
        }
    }
    any.then_some(cells)
}

fn more_for_row(
    template: &FormationTemplate,
    units: &[DragUnit],
    order: &[usize],
    placed: &[bool],
    row_index: usize,
) -> bool {
    order.iter().any(|&u| {
        !placed.get(u).copied().unwrap_or(true)
            && units.get(u).is_some_and(|unit| {
                unit.class.is_some_and(|class| {
                    template.last_row_of_class.get(class).copied().flatten() == Some(row_index)
                })
            })
    })
}

#[must_use]
pub fn build(
    template: &FormationTemplate,
    units: &[DragUnit],
    row_padding: f32,
    column_padding: f32,
) -> Vec<DragSlot> {
    let offset_squared = |i: usize| units.get(i).map_or(f32::NAN, |u| u.offset.length_squared());
    let mut order: Vec<usize> = (0..units.len()).collect();
    order.sort_by(|&a, &b| offset_squared(a).total_cmp(&offset_squared(b)));
    let mut placed = vec![false; units.len()];
    let mut instances: Vec<Vec<Option<usize>>> = Vec::new();
    let mut instance_row: Vec<usize> = Vec::new();
    for (row_index, row) in template.rows.iter().enumerate() {
        loop {
            let Some(cells) = fill_row(row, units, &order, &mut placed) else {
                break;
            };
            instances.push(cells);
            instance_row.push(row_index);
            if !more_for_row(template, units, &order, &placed, row_index) {
                break;
            }
        }
    }

    let mut slots = Vec::new();
    let mut y = 0.0f32;
    for cells in &instances {
        let mut acc = 0.0f32;
        let mut max_depth = 0.0f32;
        let mut xs: Vec<Option<f32>> = vec![None; cells.len()];
        for (index, cell) in cells.iter().enumerate() {
            let Some(unit) = cell else { continue };
            let Some(footprint) = units.get(*unit).copied() else {
                continue;
            };
            let centre = (footprint.width / 2.0).trunc() + acc;
            if let Some(slot) = xs.get_mut(index) {
                *slot = Some(centre);
            }
            acc = (footprint.width + column_padding + acc).trunc();
            max_depth = max_depth.max(footprint.depth);
        }
        let half = ((acc - column_padding) / 2.0).trunc();
        for index in centre_out(cells.len()) {
            let (Some(Some(unit)), Some(Some(cx))) = (cells.get(index), xs.get(index)) else {
                continue;
            };
            slots.push(DragSlot {
                unit: *unit,
                local: Vec2::new(*cx - half, y),
            });
        }
        y = (max_depth + row_padding + y).trunc();
    }
    slots
}

#[must_use]
pub fn rotate(theta: Facing, local: Vec2) -> Vec2 {
    let (sin_t, cos_t) = mint_math::f32::sin_cos(theta.radians());
    Vec2::new(
        -local.x * sin_t - local.y * cos_t,
        local.x * cos_t - local.y * sin_t,
    )
}

#[must_use]
pub fn preview_positions(anchor: Vec2, theta: Facing, locals: &[Vec2]) -> Vec<Vec2> {
    locals
        .iter()
        .map(|&local| anchor.plus(rotate(theta, local)))
        .collect()
}

pub fn optimize(slots: &mut [DragSlot], units: &[DragUnit], theta: Facing) {
    let n = slots.len();
    #[allow(clippy::cast_possible_wrap)]
    let mut budget: i64 = (n as i64).saturating_mul(n as i64);
    let template_of = |slots: &[DragSlot], s: usize| {
        slots
            .get(s)
            .and_then(|slot| units.get(slot.unit))
            .map_or(u64::MAX, |unit| unit.template)
    };
    let offset_of = |slots: &[DragSlot], s: usize| {
        slots
            .get(s)
            .and_then(|slot| units.get(slot.unit))
            .map_or(Vec2::ZERO, |unit| unit.offset)
    };
    let local_of =
        |slots: &[DragSlot], s: usize| slots.get(s).map_or(Vec2::ZERO, |slot| slot.local);

    let mut order: Vec<usize> = (0..slots.len()).collect();
    order.sort_by_key(|&i| template_of(slots, i));
    let mut start = 0usize;
    while start < order.len() {
        let Some(&first) = order.get(start) else {
            break;
        };
        let key = template_of(slots, first);
        let mut end = start.saturating_add(1);
        while order
            .get(end)
            .is_some_and(|&i| template_of(slots, i) == key)
        {
            end = end.saturating_add(1);
        }
        let mut i = start;
        while i < end {
            let mut j = i.saturating_add(1);
            while j < end {
                let (Some(&si), Some(&sj)) = (order.get(i), order.get(j)) else {
                    break;
                };
                let oi = offset_of(slots, si);
                let oj = offset_of(slots, sj);
                let pi = rotate(theta, local_of(slots, si));
                let pj = rotate(theta, local_of(slots, sj));
                let current = oj.minus(pj).length_squared() + oi.minus(pi).length_squared();
                let swapped = oj.minus(pi).length_squared() + oi.minus(pj).length_squared();
                if swapped < current {
                    if budget <= 0 {
                        return;
                    }
                    budget = budget.saturating_sub(1);
                    let unit_i = slots.get(si).map(|slot| slot.unit);
                    let unit_j = slots.get(sj).map(|slot| slot.unit);
                    if let (Some(unit_i), Some(unit_j)) = (unit_i, unit_j) {
                        if let Some(slot) = slots.get_mut(si) {
                            slot.unit = unit_j;
                        }
                        if let Some(slot) = slots.get_mut(sj) {
                            slot.unit = unit_i;
                        }
                    }
                    j = i.saturating_add(1);
                    continue;
                }
                j = j.saturating_add(1);
            }
            i = i.saturating_add(1);
        }
        start = end;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::assistant::FormationAssistant;

    fn template(text: &str) -> FormationTemplate {
        let assistant = read(text);
        assistant.templates.into_iter().next().unwrap()
    }

    fn read(text: &str) -> FormationAssistant {
        use mint_sage_ini::{BlockTable, MemorySources, Reader};
        let full = format!(
            "FormationAssistant\n    UnitDefinition Inf\n        ObjectFilter = NONE +INFANTRY\n    End\n    UnitDefinition Cav\n        ObjectFilter = NONE +CAVALRY\n    End\n    UnitDefinition Archer\n        ObjectFilter = NONE +ARCHER\n    End\n{text}\nEnd\n"
        );
        let mut sources = MemorySources::new();
        sources.insert("data/ini/formationassistant.ini", full.into_bytes());
        let document = Reader::new(&sources)
            .read(
                "data/ini/formationassistant.ini",
                &BlockTable::corpus_path(),
            )
            .unwrap();
        FormationAssistant::find(document.roots()).unwrap().unwrap()
    }

    fn unit(offset: Vec2, class: usize, width: f32, depth: f32) -> DragUnit {
        DragUnit {
            offset,
            class: Some(class),
            width,
            depth,
            template: 0,
        }
    }

    #[test]
    fn cells_fill_centre_out_with_the_nearest_object() {
        let t =
            template("FormationTemplate Row3\n    Rows\n        Row = Inf Inf Inf\n    End\nEnd");
        let units = [
            unit(Vec2::new(500.0, 0.0), 0, 10.0, 10.0),
            unit(Vec2::new(1.0, 0.0), 0, 10.0, 10.0),
            unit(Vec2::new(500.0, 0.0), 0, 10.0, 10.0),
        ];
        let slots = build(&t, &units, 0.0, 0.0);
        let middle = slots.iter().find(|s| s.local.x == 0.0).unwrap();
        assert_eq!(middle.unit, 1, "the near object took the centre cell");
    }

    #[test]
    fn a_full_row_repeats_while_its_class_has_unplaced_objects() {
        let t = template("FormationTemplate OneCell\n    Rows\n        Row = Archer\n    End\nEnd");
        let units: Vec<DragUnit> = (0..3u8)
            .map(|i| unit(Vec2::new(f32::from(i), 0.0), 2, 10.0, 10.0))
            .collect();
        let slots = build(&t, &units, 10.0, 0.0);
        assert_eq!(slots.len(), 3, "{slots:?}");
        let ys: std::collections::BTreeSet<_> = slots.iter().map(|s| s.local.y.to_bits()).collect();
        assert_eq!(ys.len(), 3, "each repeat sits behind the last: {slots:?}");
    }

    #[test]
    fn local_coordinates_pack_and_centre_with_padding() {
        let t = template(
            "FormationTemplate TwoRows\n    Rows\n        Row = Inf Inf\n        Row = Archer Archer\n    End\nEnd",
        );
        let units = [
            unit(Vec2::new(0.0, 0.0), 0, 10.0, 20.0),
            unit(Vec2::new(1.0, 0.0), 0, 10.0, 20.0),
            unit(Vec2::new(2.0, 0.0), 2, 10.0, 20.0),
        ];
        let slots = build(&t, &units, 5.0, 4.0);
        let front: Vec<f32> = slots
            .iter()
            .filter(|s| s.local.y == 0.0)
            .map(|s| s.local.x)
            .collect();
        assert_eq!(front.len(), 2);
        assert!((front[0] + front[1]).abs() < 1e-3, "centred: {front:?}");
        assert!((front[0] - front[1]).abs() - 14.0 < 1e-3, "{front:?}");
        let back = slots.iter().find(|s| s.local.y != 0.0).unwrap();
        assert!(
            (back.local.y - 25.0).abs() < 1e-3,
            "20 depth + 5 padding: {back:?}"
        );
    }

    #[test]
    fn an_odd_footprint_width_truncates_its_own_half_before_packing() {
        let t = template("FormationTemplate Pair\n    Rows\n        Row = Inf Inf\n    End\nEnd");
        let units = [
            unit(Vec2::ZERO, 0, 11.0, 11.0),
            unit(Vec2::new(1.0, 0.0), 0, 11.0, 11.0),
        ];
        let slots = build(&t, &units, 0.0, 0.0);
        let mut xs: Vec<f32> = slots.iter().map(|s| s.local.x).collect();
        xs.sort_by(f32::total_cmp);
        assert_eq!(xs, vec![-6.0, 5.0], "{xs:?}");
    }

    #[test]
    fn rotation_sends_left_to_plus_y_and_back_to_minus_x_at_zero_facing() {
        let theta = Facing::from_radians(0.0);
        let left = rotate(theta, Vec2::new(10.0, 0.0));
        assert!(
            (left.x - 0.0).abs() < 1e-3 && (left.y - 10.0).abs() < 1e-3,
            "{left:?}"
        );
        let back = rotate(theta, Vec2::new(0.0, 10.0));
        assert!(
            (back.x + 10.0).abs() < 1e-3 && back.y.abs() < 1e-3,
            "{back:?}"
        );
    }

    #[test]
    fn optimize_swaps_only_within_the_same_template_and_only_if_shorter() {
        let theta = Facing::from_radians(0.0);
        let mut slots = vec![
            DragSlot {
                unit: 0,
                local: Vec2::new(10.0, 0.0),
            },
            DragSlot {
                unit: 1,
                local: Vec2::new(-10.0, 0.0),
            },
        ];
        let units = [
            DragUnit {
                offset: Vec2::new(0.0, -10.0),
                class: Some(0),
                width: 10.0,
                depth: 10.0,
                template: 1,
            },
            DragUnit {
                offset: Vec2::new(0.0, 10.0),
                class: Some(0),
                width: 10.0,
                depth: 10.0,
                template: 1,
            },
        ];
        optimize(&mut slots, &units, theta);
        assert_eq!(slots[0].unit, 1, "swapped to the shorter pairing");
        assert_eq!(slots[1].unit, 0);

        let mut slots = vec![
            DragSlot {
                unit: 0,
                local: Vec2::new(10.0, 0.0),
            },
            DragSlot {
                unit: 1,
                local: Vec2::new(-10.0, 0.0),
            },
        ];
        let mixed = [
            DragUnit {
                template: 1,
                ..units[0]
            },
            DragUnit {
                template: 2,
                ..units[1]
            },
        ];
        optimize(&mut slots, &mixed, theta);
        assert_eq!(slots[0].unit, 0, "different templates never swap");
        assert_eq!(slots[1].unit, 1);
    }

    #[test]
    fn preview_positions_rotate_by_theta_plus_a_quarter_turn() {
        let anchor = Vec2::new(100.0, 50.0);
        let positions = preview_positions(anchor, Facing::EAST, &[Vec2::new(0.0, 10.0)]);
        assert_eq!(positions, vec![Vec2::new(90.0, 50.0)]);
    }
}
