const FRACTION_BITS: u32 = 8;
const HALF_CELL: i32 = 128;
const CELL_CAP: usize = 1 << 16;
const ROW_CAP: i32 = 4096;

#[derive(Debug, Clone, Copy, Default)]
struct Edge {
    x: i32,
    step: i32,
    rows: i32,
}

impl Edge {
    fn start(&mut self, from: i32, to: i32, rows: i32) {
        self.rows = rows;
        if rows > 0 {
            self.step = to.wrapping_sub(from).wrapping_shl(FRACTION_BITS) / rows;
            self.x = from.wrapping_shl(FRACTION_BITS).wrapping_add(self.step / 2);
        }
    }
}

pub(super) fn rectangle_cells(
    centre: (i32, i32),
    (width, height): (i32, i32),
    angle: f32,
) -> Vec<(i32, i32)> {
    if width == 1 && height == 1 {
        return vec![centre];
    }
    let (xs, ys) = corners(centre, (width, height), angle);
    fill(xs, ys)
}

#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn corners((cx, cy): (i32, i32), (width, height): (i32, i32), angle: f32) -> ([i32; 4], [i32; 4]) {
    if angle == 0.0 {
        let x0 = cx - width / 2;
        let y0 = cy - height / 2;
        let (x1, y1) = (x0 + width - 1, y0 + height - 1);
        return ([x0, x1, x1, x0], [y0, y0, y1, y1]);
    }
    let (sin, cos) = mint_math::f32::sin_cos(angle);
    let (half_w, half_h) = (width as f32 * 0.5, height as f32 * 0.5);
    let (fx, fy) = (cx as f32, cy as f32);
    let (hh_s, hh_c) = (half_h * sin, half_h * cos);
    let (hw_c, hw_s) = (half_w * cos, half_w * sin);
    let x_low = fx - hw_c;
    let y_low = fy - hh_c;
    (
        [
            (hh_s + x_low) as i32,
            ((hh_s + hw_c) + fx) as i32,
            ((hw_c + fx) - hh_s) as i32,
            (x_low - hh_s) as i32,
        ],
        [
            (y_low - hw_s) as i32,
            (hw_s + y_low) as i32,
            ((hw_s + hh_c) + fy) as i32,
            ((hh_c + fy) - hw_s) as i32,
        ],
    )
}

fn fill(xs: [i32; 4], ys: [i32; 4]) -> Vec<(i32, i32)> {
    let mut vertices: Vec<(i32, i32)> = Vec::with_capacity(4);
    for point in xs.into_iter().zip(ys) {
        if vertices.last() != Some(&point) {
            vertices.push(point);
        }
    }
    while vertices.len() > 2 && vertices.last() == vertices.first() {
        vertices.pop();
    }
    let count = vertices.len();
    if count < 3 {
        return Vec::new();
    }
    let at = |i: usize| vertices.get(i).copied().unwrap_or_default();
    let next = |i: usize| if i + 1 == count { 0 } else { i + 1 };
    let previous = |i: usize| if i == 0 { count - 1 } else { i - 1 };
    let mut top = 0usize;
    let mut bottom = 0i32;
    for i in 1..count {
        let (x, y) = at(i);
        let (top_x, top_y) = at(top);
        if y < top_y || (y == top_y && x < top_x) {
            top = i;
        }
        bottom = bottom.max(y);
    }
    let mut left = Edge::default();
    let mut right = Edge::default();
    let mut left_at = previous(top);
    left.start(at(top).0, at(left_at).0, at(left_at).1 - at(top).1);
    let mut right_at = top;
    let mut skipped = 0usize;
    while at(right_at).1 == at(next(right_at)).1 {
        right_at = next(right_at);
        skipped += 1;
        if skipped > count {
            return Vec::new();
        }
    }
    let after = next(right_at);
    right.start(at(right_at).0, at(after).0, at(after).1 - at(right_at).1);
    right_at = after;

    let mut cells = Vec::new();
    let mut y = at(top).1;
    let top_row = y;
    while y <= bottom && y - top_row <= ROW_CAP {
        let (first, last) = (
            left.x.wrapping_add(HALF_CELL) / 256,
            right.x.wrapping_add(HALF_CELL) / 256,
        );
        for x in first..=last {
            if cells.len() >= CELL_CAP {
                return cells;
            }
            cells.push((x, y));
        }
        y += 1;
        left.x = left.x.wrapping_add(left.step);
        left.rows -= 1;
        for _ in 0..=count {
            if left.rows != 0 {
                break;
            }
            let to = previous(left_at);
            let rows = at(to).1 - at(left_at).1;
            if rows < 0 {
                break;
            }
            left.start(at(left_at).0, at(to).0, rows);
            left_at = to;
        }
        right.x = right.x.wrapping_add(right.step);
        right.rows -= 1;
        for _ in 0..=count {
            if right.rows != 0 {
                break;
            }
            let to = next(right_at);
            let rows = at(to).1 - at(right_at).1;
            if rows < 0 {
                break;
            }
            right.start(at(right_at).0, at(to).0, rows);
            right_at = to;
        }
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fill_visits_the_cells_retail_visits() {
        let sum = |cells: &[(i32, i32)]| cells.iter().map(|&(x, y)| x * 7 + y).sum::<i32>();
        for (size, angle, count, checksum) in [
            ((16, 7), 0.0, 112, 447_608),
            ((16, 7), 0.3, 111, 443_655),
            ((16, 7), 1.0, 131, 523_388),
            ((16, 7), 2.0, 122, 487_451),
            ((16, 7), 3.5, 91, 362_668),
            ((20, 8), 5.0, 166, 663_311),
        ] {
            let cells = rectangle_cells((500, 500), size, angle);
            assert_eq!(
                (cells.len(), sum(&cells)),
                (count, checksum),
                "{size:?} turned {angle}"
            );
        }
    }

    #[test]
    fn a_one_cell_wide_rectangle_visits_nothing() {
        assert!(rectangle_cells((50, 50), (1, 5), 0.0).is_empty());
        assert_eq!(rectangle_cells((50, 50), (1, 1), 0.7), vec![(50, 50)]);
    }
}
