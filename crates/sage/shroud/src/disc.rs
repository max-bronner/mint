pub const MAX_CELL_RADIUS: i32 = 1_024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub y: i32,
    pub x_start: i32,
    pub x_end: i32,
}

impl Span {
    #[must_use]
    pub const fn width(self) -> u32 {
        match self.x_end.checked_sub(self.x_start) {
            Some(delta) if delta >= 0 => delta.unsigned_abs().saturating_add(1),
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscreteCircle {
    edges: Vec<Span>,
    y_centre: i32,
    y_doubled: i32,
}

impl DiscreteCircle {
    #[must_use]
    pub fn new(x_centre: i32, y_centre: i32, radius: i32) -> Self {
        let radius = radius.clamp(0, MAX_CELL_RADIUS);
        let mut edges = Vec::new();
        generate_edge_pairs(&mut edges, x_centre, y_centre, radius);
        remove_duplicate_rows(&mut edges);
        Self {
            edges,
            y_centre,
            y_doubled: y_centre.saturating_mul(2),
        }
    }

    pub fn spans(&self) -> impl Iterator<Item = Span> + '_ {
        self.edges.iter().flat_map(move |edge| {
            let mirror = (edge.y != self.y_centre).then(|| Span {
                y: self.y_doubled.saturating_sub(edge.y),
                ..*edge
            });
            core::iter::once(*edge).chain(mirror)
        })
    }

    #[must_use]
    pub fn stored_spans(&self) -> usize {
        self.edges.len()
    }

    #[must_use]
    pub fn covered_cells(&self) -> u64 {
        self.spans()
            .map(|span| u64::from(span.width()))
            .fold(0u64, u64::saturating_add)
    }
}

fn generate_edge_pairs(edges: &mut Vec<Span>, x_centre: i32, y_centre: i32, radius: i32) {
    let mut x: i32 = 0;
    let mut y: i32 = radius;
    let mut d: i32 = 1i32.saturating_sub(radius).saturating_mul(2);
    let guard = radius.saturating_mul(4).saturating_add(8);
    let mut steps: i32 = 0;

    while y >= 0 && steps < guard {
        steps = steps.saturating_add(1);
        edges.push(Span {
            y: y_centre.saturating_add(y),
            x_start: x_centre.saturating_sub(x),
            x_end: x_centre.saturating_add(x),
        });

        if d.saturating_add(y) > 0 {
            y = y.saturating_sub(1);
            d = d.saturating_sub(y.saturating_mul(2).saturating_sub(1));
        }

        if x > d {
            x = x.saturating_add(1);
            d = d.saturating_add(x.saturating_mul(2).saturating_add(1));
        }
    }
}

fn remove_duplicate_rows(edges: &mut Vec<Span>) {
    let mut kept: Vec<Span> = Vec::with_capacity(edges.len());
    for (index, edge) in edges.iter().enumerate() {
        let next_has_same_row = index
            .checked_add(1)
            .and_then(|next| edges.get(next))
            .is_some_and(|next| next.y == edge.y);
        if !next_has_same_row {
            kept.push(*edge);
        }
    }
    *edges = kept;
}

#[must_use]
pub fn circle_fill(x_centre: i32, y_centre: i32, radius: i32) -> Vec<(i32, i32)> {
    let radius = radius.clamp(0, MAX_CELL_RADIUS);
    let mut cells: Vec<(i32, i32)> = Vec::new();

    let mut y = radius.saturating_sub(1);
    let mut dec = 3i32.saturating_sub(radius.saturating_mul(2));
    let mut x = 0i32;
    while x < radius {
        push_row(
            &mut cells,
            x_centre.saturating_sub(x),
            x_centre.saturating_add(x),
            y_centre.saturating_add(y),
        );
        push_row(
            &mut cells,
            x_centre.saturating_sub(x),
            x_centre.saturating_add(x),
            y_centre.saturating_sub(y),
        );
        push_row(
            &mut cells,
            x_centre.saturating_sub(y),
            x_centre.saturating_add(y),
            y_centre.saturating_add(x),
        );
        push_row(
            &mut cells,
            x_centre.saturating_sub(y),
            x_centre.saturating_add(y),
            y_centre.saturating_sub(x),
        );

        if dec >= 0 {
            dec = dec.saturating_add(1i32.saturating_sub(y).saturating_mul(4));
            y = y.saturating_sub(1);
        }
        dec = dec.saturating_add(x.saturating_mul(4)).saturating_add(6);
        x = x.saturating_add(1);
    }

    cells.sort_unstable_by_key(|&(cx, cy)| (cy, cx));
    cells.dedup();
    cells
}

fn push_row(cells: &mut Vec<(i32, i32)>, x1: i32, x2: i32, y: i32) {
    let mut x = x1;
    while x <= x2 {
        cells.push((x, y));
        let Some(next) = x.checked_add(1) else { break };
        x = next;
    }
}

#[must_use]
pub fn small_fill(low: (i32, i32), high: (i32, i32)) -> Vec<(i32, i32)> {
    let mut cells = Vec::new();
    let mut y = low.1;
    while y <= high.1 {
        push_row(&mut cells, low.0, high.0, y);
        let Some(next) = y.checked_add(1) else { break };
        y = next;
    }
    cells.sort_unstable_by_key(|&(cx, cy)| (cy, cx));
    cells.dedup();
    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIPPED_TABLE: [(i32, u64, usize); 8] = [
        (1, 3, 2),
        (2, 21, 3),
        (4, 65, 5),
        (7, 181, 8),
        (10, 349, 11),
        (13, 573, 14),
        (15, 757, 16),
        (20, 1_325, 21),
    ];

    #[test]
    fn the_disc_covers_exactly_what_the_shipped_table_says() {
        for (radius, cells, spans) in SHIPPED_TABLE {
            let circle = DiscreteCircle::new(0, 0, radius);
            assert_eq!(
                circle.covered_cells(),
                cells,
                "cell radius {radius} covered {} cells, expected {cells}",
                circle.covered_cells()
            );
            assert_eq!(
                circle.stored_spans(),
                spans,
                "cell radius {radius} stored {} spans, expected {spans}",
                circle.stored_spans()
            );
        }
    }

    #[test]
    fn radius_one_is_three_cells_in_a_vertical_line_and_not_a_plus() {
        let mut cells: Vec<(i32, i32)> = DiscreteCircle::new(5, 7, 1)
            .spans()
            .flat_map(|span| (span.x_start..=span.x_end).map(move |x| (x, span.y)))
            .collect();
        cells.sort_unstable();
        assert_eq!(cells, vec![(5, 6), (5, 7), (5, 8)]);
    }

    #[test]
    fn the_mirror_never_doubles_the_centre_row() {
        for radius in 0..=40 {
            let circle = DiscreteCircle::new(0, 0, radius);
            let centre_rows = circle.spans().filter(|span| span.y == 0).count();
            assert_eq!(
                centre_rows, 1,
                "radius {radius} emitted the centre row twice"
            );
        }
    }

    #[test]
    fn every_cell_of_a_disc_is_named_exactly_once() {
        for radius in 0..=40 {
            let mut cells: Vec<(i32, i32)> = DiscreteCircle::new(0, 0, radius)
                .spans()
                .flat_map(|span| (span.x_start..=span.x_end).map(move |x| (x, span.y)))
                .collect();
            let total = cells.len();
            cells.sort_unstable();
            cells.dedup();
            assert_eq!(total, cells.len(), "radius {radius} named a cell twice");
        }
    }

    #[test]
    fn disc_iteration_count_stays_far_under_the_guard() {
        for radius in 0..=MAX_CELL_RADIUS {
            let mut edges = Vec::new();
            generate_edge_pairs(&mut edges, 0, 0, radius);
            let guard = radius.saturating_mul(4).saturating_add(8);
            assert!(
                i32::try_from(edges.len()).unwrap_or(i32::MAX) < guard,
                "radius {radius} took {} iterations against a guard of {guard}",
                edges.len()
            );
        }
    }

    #[test]
    fn a_hostile_radius_is_clamped_rather_than_allocating_from_it() {
        for radius in [i32::MIN, -1, MAX_CELL_RADIUS.saturating_add(1), i32::MAX] {
            let circle = DiscreteCircle::new(0, 0, radius);
            assert!(
                circle.stored_spans() <= 2_048,
                "radius {radius} was not clamped"
            );
        }
        assert_eq!(DiscreteCircle::new(0, 0, -5).covered_cells(), 1);
    }

    #[test]
    fn a_disc_centred_far_from_the_origin_does_not_overflow() {
        let circle = DiscreteCircle::new(i32::MAX, i32::MIN, 20);
        assert!(circle.covered_cells() > 0);
        let circle = DiscreteCircle::new(i32::MIN, i32::MAX, 20);
        assert!(circle.covered_cells() > 0);
    }

    #[test]
    fn the_object_fill_is_a_different_shape_from_the_reveal_disc() {
        assert_eq!(DiscreteCircle::new(0, 0, 2).covered_cells(), 21);
        assert_eq!(circle_fill(0, 0, 2).len(), 9);
    }

    #[test]
    fn the_object_fill_is_sorted_deduplicated_and_symmetric() {
        for radius in 1..=24 {
            let cells = circle_fill(0, 0, radius);
            let mut sorted = cells.clone();
            sorted.sort_unstable_by_key(|&(x, y)| (y, x));
            sorted.dedup();
            assert_eq!(cells, sorted, "radius {radius} was not in the fixed order");
            for &(x, y) in &cells {
                assert!(
                    cells.contains(&(-x, y)) && cells.contains(&(x, -y)),
                    "radius {radius} is not symmetric about ({x}, {y})"
                );
            }
        }
    }

    #[test]
    fn a_zero_radius_object_fill_is_empty_and_a_small_fill_is_one_cell() {
        assert!(circle_fill(0, 0, 0).is_empty());
        assert_eq!(small_fill((3, 4), (3, 4)), vec![(3, 4)]);
        assert_eq!(small_fill((3, 4), (4, 5)).len(), 4);
    }

    #[test]
    fn a_small_fill_with_inverted_corners_is_empty_rather_than_panicking() {
        assert!(small_fill((5, 5), (4, 4)).is_empty());
    }
}
