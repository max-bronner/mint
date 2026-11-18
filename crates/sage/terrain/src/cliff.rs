use mint_sage_map::CliffRecord;

use crate::tile::TileLayout;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellCliff {
    pub class: usize,
    pub uv: [[f32; 2]; 4],
}

const SLOT_TO_CORNER: [usize; 4] = [0, 1, 3, 2];

#[must_use]
pub fn cell_cliff(
    layout: &TileLayout,
    records: &[CliffRecord],
    index: u32,
    layer_class: Option<usize>,
) -> Option<CellCliff> {
    let slot = usize::try_from(index).ok()?.checked_sub(1)?;
    let record = records.get(slot)?;
    let value = u16::try_from(record.tile_index).ok()?;
    let class = layout.cell(value)?.class;
    if layer_class != Some(class) {
        return None;
    }

    let mut uv = [[0.0f32; 2]; 4];
    for (slot, corner) in SLOT_TO_CORNER.iter().enumerate() {
        if let (Some((u, v)), Some(target)) = (record.tex_coords.get(slot), uv.get_mut(*corner)) {
            *target = [*u, *v];
        }
    }
    Some(CellCliff { class, uv })
}

#[must_use]
pub fn overlay_cliff_uv(
    layout: &TileLayout,
    records: &[CliffRecord],
    cliff_index: u32,
    (uv_xy, uv_x1y1): ([f32; 2], [f32; 2]),
    overlay_class: usize,
    heights: [u16; 4],
    relief_per_sample: f32,
) -> Option<[[f32; 2]; 4]> {
    if let Some(cliff) = cell_cliff(layout, records, cliff_index, Some(overlay_class)) {
        return Some(cliff.uv);
    }
    stretch_uv(uv_xy, uv_x1y1, heights, relief_per_sample)
}

#[must_use]
pub fn map_cliff_class(layout: &TileLayout, records: &[CliffRecord]) -> Option<usize> {
    let record = records.first()?;
    let value = u16::try_from(record.tile_index).ok()?;
    Some(layout.cell(value)?.class)
}

const STRETCH_LIMIT: f32 = 1.5;

const TILE_LIMIT: f32 = 4.0;

const TALL_STRETCH_LIMIT: f32 = 2.0;

const DIAMOND_STRETCH_LIMIT: f32 = 2.4;

#[must_use]
pub fn stretch_uv(
    uv_xy: [f32; 2],
    uv_x1y1: [f32; 2],
    heights: [u16; 4],
    relief_per_sample: f32,
) -> Option<[[f32; 2]; 4]> {
    let (n_u, x_u) = (uv_xy[0], uv_x1y1[0]);
    let (n_v, x_v) = (uv_x1y1[1], uv_xy[1]);
    let mut u = [n_u, x_u, x_u, n_u];
    let mut v = [x_v, x_v, n_v, n_v];

    let h = [
        i32::from(*heights.first()?),
        i32::from(*heights.get(1)?),
        i32::from(*heights.get(2)?),
        i32::from(*heights.get(3)?),
    ];
    let min_h = h.iter().copied().min()?;
    let max_h = h.iter().copied().max()?;
    let delta_h = max_h.saturating_sub(min_h);
    let relief = delta_h as f32 * relief_per_sample;
    if relief.is_nan() || relief < STRETCH_LIMIT {
        return None;
    }

    let below_limit = min_h.saturating_add(
        delta_h
            .saturating_mul(2)
            .saturating_add(1)
            .checked_div(3)
            .unwrap_or(0),
    );
    let above_limit = min_h.saturating_add(delta_h.saturating_add(1).checked_div(3).unwrap_or(0));
    let below = h.iter().filter(|&&value| value < below_limit).count();
    let above = h.iter().filter(|&&value| value > above_limit).count();

    if above != 1 && below != 1 && (above != 2 || below != 2) && relief < DIAMOND_STRETCH_LIMIT {
        return None;
    }

    let divisor = (TILE_LIMIT / relief).clamp(1.0, TILE_LIMIT);
    let step = 1.0 / divisor;

    if below == 1 || above > below {
        push_odd_corner(&mut v, &h, min_h, n_v, x_v, step);
    } else if above == 1 || below > above {
        push_odd_corner(&mut v, &h, max_h, n_v, x_v, step);
    } else {
        if relief < TALL_STRETCH_LIMIT {
            return None;
        }
        let edge = |a: usize, b: usize| -> f32 {
            let rise = h
                .get(a)
                .copied()
                .unwrap_or(0)
                .saturating_sub(h.get(b).copied().unwrap_or(0));
            let d = rise as f32 * relief_per_sample;
            let length = (1.0 + d * d).sqrt();
            if length < STRETCH_LIMIT {
                1.0
            } else {
                length.min(TILE_LIMIT)
            }
        };
        let dx = edge(3, 2) * (x_u - n_u);
        let dy = edge(3, 0) * (x_v - n_v);
        u = [n_u, n_u + dx, n_u + dx, n_u];
        v = [n_v + dy, n_v + dy, n_v, n_v];
        let dx1 = edge(1, 0) * (x_u - n_u);
        let dy1 = edge(2, 1) * (x_v - n_v);
        let (u0, v3) = (n_u, n_v);
        if let Some(slot) = u.get_mut(1) {
            *slot = u0 + dx1;
        }
        if let Some(slot) = v.get_mut(1) {
            *slot = v3 + dy1;
        }
    }

    let lift = v.iter().fold(0.0f32, |worst, &value| worst.max(-value));
    for value in &mut v {
        *value += lift;
    }
    let drop_u = u
        .iter()
        .fold(0.0f32, |worst, &value| worst.max(value - 1.0));
    let drop_v = v
        .iter()
        .fold(0.0f32, |worst, &value| worst.max(value - 1.0));
    for value in &mut u {
        *value -= drop_u;
    }
    for value in &mut v {
        *value -= drop_v;
    }

    Some([
        [*u.first()?, *v.first()?],
        [*u.get(1)?, *v.get(1)?],
        [*u.get(3)?, *v.get(3)?],
        [*u.get(2)?, *v.get(2)?],
    ])
}

fn push_odd_corner(v: &mut [f32; 4], h: &[i32; 4], target: i32, n_v: f32, x_v: f32, step: f32) {
    for (slot, &height) in h.iter().enumerate() {
        if height != target {
            continue;
        }
        let moved = if slot < 2 { n_v + step } else { x_v - step };
        if let Some(value) = v.get_mut(slot) {
            *value = moved;
        }
        return;
    }
}

#[cfg(test)]
#[allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]
mod tests {
    use super::*;
    use mint_sage_map::TextureClass;

    #[test]
    fn a_record_of_another_class_than_the_cells_own_is_not_applied() {
        let layout = layout();
        let records = [record(20 << 2)];
        assert!(
            cell_cliff(&layout, &records, 1, Some(1)).is_some(),
            "same class: the record applies"
        );
        assert!(
            cell_cliff(&layout, &records, 1, Some(0)).is_none(),
            "different class: the cell keeps its base tile"
        );
        assert!(
            cell_cliff(&layout, &records, 1, None).is_none(),
            "a cell whose own tile names no class has nothing to match"
        );
    }

    fn layout() -> TileLayout {
        TileLayout::new(&[
            TextureClass {
                first_tile: 0,
                num_tiles: 16,
                width: 4,
                name: "grass".to_owned(),
            },
            TextureClass {
                first_tile: 16,
                num_tiles: 16,
                width: 4,
                name: "cliff".to_owned(),
            },
        ])
    }

    fn record(tile: i32) -> CliffRecord {
        CliffRecord {
            tile_index: tile,
            tex_coords: [(0.1, 0.2), (0.3, 0.4), (0.5, 0.6), (0.7, 0.8)],
            flip: 0,
            unknown: 0,
        }
    }

    #[test]
    fn the_four_stored_pairs_run_counter_clockwise_from_the_cells_own_corner() {
        let cliff =
            cell_cliff(&layout(), &[record(20 << 2)], 1, Some(1)).expect("tile 20 is in class 1");
        assert_eq!(cliff.class, 1);
        assert_eq!(
            cliff.uv,
            [[0.1, 0.2], [0.3, 0.4], [0.7, 0.8], [0.5, 0.6]],
            "corner order is (x,y) (x+1,y) (x,y+1) (x+1,y+1)"
        );
    }

    #[test]
    fn a_cliff_index_is_one_based_and_bounds_checked() {
        let records = [record(20 << 2), record(24 << 2)];
        let layout = layout();
        assert!(
            cell_cliff(&layout, &records, 0, Some(1)).is_none(),
            "0 is not a cliff"
        );
        assert_eq!(
            cell_cliff(&layout, &records, 1, Some(1)).map(|c| c.uv[0]),
            Some([0.1, 0.2])
        );
        assert!(cell_cliff(&layout, &records, 2, Some(1)).is_some());
        assert!(
            cell_cliff(&layout, &records, 3, Some(1)).is_none(),
            "past the end"
        );
        assert!(cell_cliff(&layout, &records, u32::MAX, Some(1)).is_none());
    }

    const RELIEF: f32 = 0.039_062_5 / 10.0;

    fn raw(cell_widths: f32) -> u16 {
        (cell_widths / RELIEF) as u16
    }

    #[test]
    fn a_cell_below_the_stretch_limit_is_left_alone() {
        let flat = [0u16, 0, 0, 0];
        assert!(stretch_uv([0.0, 0.125], [0.125, 0.0], flat, RELIEF).is_none());
        let nearly = [0, 0, raw(1.4), 0];
        assert!(stretch_uv([0.0, 0.125], [0.125, 0.0], nearly, RELIEF).is_none());
    }

    #[test]
    fn one_low_corner_pulls_texture_down_the_face() {
        let top = raw(3.0);
        let heights = [top, top, 0, top];
        let uv = stretch_uv([0.0, 0.125], [0.125, 0.0], heights, RELIEF)
            .expect("3 cell widths of relief is well past the limit");
        let span = |axis: usize| {
            let values: Vec<f32> = uv.iter().map(|c| c[axis]).collect();
            values.iter().copied().fold(f32::MIN, f32::max)
                - values.iter().copied().fold(f32::MAX, f32::min)
        };
        assert!(
            span(1) > 0.125,
            "v span {} should exceed the plain rectangle's 0.125",
            span(1)
        );
        assert!(span(0) <= 0.125 + 1e-6, "u is untouched in this branch");
    }

    #[test]
    fn the_stretched_quad_stays_inside_the_class_texture() {
        for relief in [1.5f32, 2.0, 3.0, 8.0, 40.0] {
            for shape in [
                [raw(relief), raw(relief), 0, raw(relief)],
                [0, 0, raw(relief), raw(relief)],
                [0, raw(relief), raw(relief), 0],
                [raw(relief), 0, 0, 0],
            ] {
                let Some(uv) = stretch_uv([0.5, 0.625], [0.625, 0.5], shape, RELIEF) else {
                    continue;
                };
                for corner in uv {
                    assert!(
                        (-1e-5..=1.0 + 1e-5).contains(&corner[0])
                            && (-1e-5..=1.0 + 1e-5).contains(&corner[1]),
                        "corner {corner:?} left the texture at relief {relief}, shape {shape:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_non_finite_scale_produces_no_stretch() {
        let heights = [1000u16, 1000, 0, 1000];
        assert!(stretch_uv([0.0, 0.125], [0.125, 0.0], heights, f32::NAN).is_none());
        assert!(stretch_uv([0.0, 0.125], [0.125, 0.0], heights, 0.0).is_none());
        if let Some(uv) = stretch_uv([0.0, 0.125], [0.125, 0.0], heights, f32::INFINITY) {
            for corner in uv {
                assert!(corner[0].is_finite() && corner[1].is_finite(), "{corner:?}");
            }
        }
    }

    #[test]
    fn a_straight_face_widens_both_axes() {
        let top = raw(3.0);
        let heights = [top, top, 0, 0];
        let uv = stretch_uv([0.0, 0.125], [0.125, 0.0], heights, RELIEF)
            .expect("3 cell widths with two high and two low");
        let span = |axis: usize| {
            let values: Vec<f32> = uv.iter().map(|c| c[axis]).collect();
            values.iter().copied().fold(f32::MIN, f32::max)
                - values.iter().copied().fold(f32::MAX, f32::min)
        };
        assert!(span(1) > 0.125, "v span {}", span(1));
    }

    #[test]
    fn a_record_naming_no_class_produces_no_cliff() {
        assert!(cell_cliff(&layout(), &[record(9999 << 2)], 1, Some(1)).is_none());
        assert!(cell_cliff(&layout(), &[record(-1)], 1, Some(1)).is_none());
        assert!(cell_cliff(&layout(), &[record(i32::MAX)], 1, Some(1)).is_none());
    }
}
