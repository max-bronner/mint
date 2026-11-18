use mint_sage_map::BlendRecord;

use crate::tile::{CellTile, TileLayout};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellBlend {
    pub tile: CellTile,
    pub ramp: [f32; 4],
}

#[must_use]
pub fn corner_mask(record: &BlendRecord) -> [bool; 4] {
    corner_ramp(record).map(|value| value >= 1.0)
}

#[must_use]
pub fn corner_ramp(record: &BlendRecord) -> [f32; 4] {
    let positive = !record.inverted;
    let sign = |towards_positive: bool| if towards_positive { 1.0f32 } else { -1.0f32 };
    let (ex, ey) = if record.horizontal {
        (sign(positive), 0.0)
    } else if record.vertical {
        (0.0, sign(positive))
    } else if record.right_diagonal {
        (1.0, sign(positive))
    } else if record.left_diagonal {
        (-1.0, sign(positive))
    } else {
        return [0.0; 4];
    };
    let long = record.long_diagonal != 0 && (record.right_diagonal || record.left_diagonal);
    let positive_axes = f32::from(u8::from(ex > 0.0)) + f32::from(u8::from(ey > 0.0));
    let k = 1.0 - positive_axes + f32::from(u8::from(long));
    [k, ex + k, ey + k, ex + ey + k]
}

#[must_use]
pub fn ramp_alpha(ramp: [f32; 4], s: f32, t: f32) -> f32 {
    let at = |slot: usize| ramp.get(slot).copied().unwrap_or(0.0);
    let value = at(0) * (1.0 - s) * (1.0 - t)
        + at(1) * s * (1.0 - t)
        + at(2) * (1.0 - s) * t
        + at(3) * s * t;
    value.clamp(0.0, 1.0)
}

#[must_use]
pub fn cell_blend(layout: &TileLayout, records: &[BlendRecord], index: u32) -> Option<CellBlend> {
    let slot = usize::try_from(index).ok()?.checked_sub(1)?;
    let record = records.get(slot)?;
    let value = u16::try_from(record.tile_index).ok()?;
    let tile = layout.cell(value)?;
    let ramp = corner_ramp(record);
    if !ramp.iter().any(|value| *value >= 1.0) {
        return None;
    }
    Some(CellBlend { tile, ramp })
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use mint_sage_map::TextureClass;

    fn record(flag: &str, inverted: bool, long_diagonal: u8) -> BlendRecord {
        BlendRecord {
            tile_index: 0,
            horizontal: flag == "h",
            vertical: flag == "v",
            right_diagonal: flag == "r",
            left_diagonal: flag == "l",
            inverted,
            long_diagonal,
            custom_blend_edge_class: -1,
        }
    }

    #[test]
    fn each_flag_covers_the_corners_on_its_own_neighbours_side() {
        assert_eq!(
            corner_mask(&record("h", false, 0)),
            [false, true, false, true],
            "horizontal names the +x neighbour"
        );
        assert_eq!(
            corner_mask(&record("h", true, 0)),
            [true, false, true, false],
            "horizontal + inverted names the -x neighbour"
        );
        assert_eq!(
            corner_mask(&record("v", false, 0)),
            [false, false, true, true],
            "vertical names the +y neighbour"
        );
        assert_eq!(
            corner_mask(&record("v", true, 0)),
            [true, true, false, false],
            "vertical + inverted names the -y neighbour"
        );
        assert_eq!(
            corner_mask(&record("r", false, 0)),
            [false, false, false, true]
        );
        assert_eq!(
            corner_mask(&record("r", true, 0)),
            [false, true, false, false]
        );
        assert_eq!(
            corner_mask(&record("l", false, 0)),
            [false, false, true, false]
        );
        assert_eq!(
            corner_mask(&record("l", true, 0)),
            [true, false, false, false]
        );
    }

    #[test]
    fn the_long_diagonal_byte_covers_everything_but_the_opposite_corner() {
        assert_eq!(
            corner_mask(&record("r", false, 1)),
            [false, true, true, true]
        );
        assert_eq!(
            corner_mask(&record("r", true, 1)),
            [true, true, false, true]
        );
        assert_eq!(
            corner_mask(&record("l", false, 1)),
            [true, false, true, true]
        );
        assert_eq!(
            corner_mask(&record("l", true, 1)),
            [true, true, true, false]
        );
        assert_eq!(
            corner_mask(&record("h", false, 1)),
            corner_mask(&record("h", false, 0))
        );
    }

    #[test]
    fn a_blend_index_is_one_based_and_bounds_checked() {
        let layout = TileLayout::new(&[TextureClass {
            first_tile: 0,
            num_tiles: 16,
            width: 4,
            name: "grass".to_owned(),
        }]);
        let records = vec![record("h", false, 0), record("v", false, 0)];

        assert!(
            cell_blend(&layout, &records, 0).is_none(),
            "0 means no blend"
        );
        assert_eq!(
            cell_blend(&layout, &records, 1).map(|b| b.ramp),
            Some([0.0, 1.0, 0.0, 1.0]),
            "index 1 selects the first record"
        );
        assert_eq!(
            cell_blend(&layout, &records, 2).map(|b| b.ramp),
            Some([0.0, 0.0, 1.0, 1.0])
        );
        assert!(cell_blend(&layout, &records, 3).is_none(), "past the end");
        assert!(cell_blend(&layout, &records, u32::MAX).is_none());
    }

    #[test]
    fn the_ramp_is_the_prepared_alpha_tiles_own_affine_function() {
        assert_eq!(corner_ramp(&record("h", false, 0)), [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(corner_ramp(&record("v", true, 0)), [1.0, 1.0, 0.0, 0.0]);
        assert_eq!(corner_ramp(&record("r", false, 0)), [-1.0, 0.0, 0.0, 1.0]);
        assert_eq!(corner_ramp(&record("l", true, 0)), [1.0, 0.0, 0.0, -1.0]);
        assert_eq!(corner_ramp(&record("r", false, 1)), [0.0, 1.0, 1.0, 2.0]);
        assert_eq!(corner_ramp(&record("l", true, 1)), [2.0, 1.0, 1.0, 0.0]);

        for flag in ["h", "v", "r", "l"] {
            for inverted in [false, true] {
                for long in [0u8, 1] {
                    let r = record(flag, inverted, long);
                    assert_eq!(
                        corner_ramp(&r).map(|value| value.clamp(0.0, 1.0)),
                        corner_mask(&r).map(|covered| if covered { 1.0 } else { 0.0 }),
                        "{flag} inverted={inverted} long={long}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_ramp_agrees_with_the_corner_mask_along_every_cell_edge() {
        for flag in ["h", "v", "r", "l"] {
            for inverted in [false, true] {
                for long in [0u8, 1] {
                    let r = record(flag, inverted, long);
                    let mask = corner_mask(&r).map(|c| if c { 1.0f32 } else { 0.0 });
                    let ramp = corner_ramp(&r);
                    for step in 0..=8 {
                        let f = step as f32 / 8.0;
                        let edges = [
                            ((f, 0.0), mask[0] * (1.0 - f) + mask[1] * f),
                            ((f, 1.0), mask[2] * (1.0 - f) + mask[3] * f),
                            ((0.0, f), mask[0] * (1.0 - f) + mask[2] * f),
                            ((1.0, f), mask[1] * (1.0 - f) + mask[3] * f),
                        ];
                        for ((s, t), expected) in edges {
                            let got = ramp_alpha(ramp, s, t);
                            assert!(
                                (got - expected).abs() < 1e-6,
                                "{flag} inverted={inverted} long={long} at ({s}, {t}):                                  ramp {got} vs corner interpolation {expected}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_ramp_is_the_same_field_whichever_diagonal_the_quad_folds_along() {
        let r = record("l", false, 0);
        let mask = corner_mask(&r).map(|c| if c { 1.0f32 } else { 0.0 });
        let ramp = corner_ramp(&r);
        let centre_ad = (mask[0] + mask[3]) / 2.0;
        let centre_bc = (mask[1] + mask[2]) / 2.0;
        assert!((centre_ad - 0.0).abs() < 1e-6);
        assert!((centre_bc - 0.5).abs() < 1e-6);
        assert!(
            (ramp_alpha(ramp, 0.5, 0.5) - 0.0).abs() < 1e-6,
            "the prepared tile is transparent at the centre of a left-diagonal cell"
        );
    }

    #[test]
    fn a_record_with_no_direction_flag_produces_no_overlay() {
        let layout = TileLayout::new(&[TextureClass {
            first_tile: 0,
            num_tiles: 16,
            width: 4,
            name: "grass".to_owned(),
        }]);
        let records = vec![record("none", false, 0)];
        assert!(cell_blend(&layout, &records, 1).is_none());
    }
}
