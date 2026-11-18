#[must_use]
pub fn class_mip_levels(width: u32, height: u32, tiles: i32) -> usize {
    let Ok(tiles) = u32::try_from(tiles) else {
        return 1;
    };
    let Some(across) = tiles.checked_mul(2) else {
        return 1;
    };
    if across == 0 {
        return 1;
    }
    let (Some(rect_w), Some(rect_h)) = (edge(width, across), edge(height, across)) else {
        return 1;
    };
    let levels = rect_w.trailing_zeros().min(rect_h.trailing_zeros());
    usize::try_from(levels).unwrap_or(0).saturating_add(1)
}

fn edge(pixels: u32, across: u32) -> Option<u32> {
    if pixels == 0 || pixels.checked_rem(across) != Some(0) {
        return None;
    }
    let edge = pixels.checked_div(across)?;
    (edge > 0).then_some(edge)
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn a_chain_stops_where_one_texel_becomes_one_cells_rectangle() {
        for (pixels, tiles) in [(64, 1), (128, 2), (256, 4), (384, 6), (512, 8), (1024, 16)] {
            assert_eq!(
                class_mip_levels(pixels, pixels, tiles),
                6,
                "{pixels}px, {tiles} tiles"
            );
        }
        assert_eq!(class_mip_levels(256, 256, 4), 6);
    }

    #[test]
    fn a_grid_that_does_not_divide_the_image_gets_one_level() {
        assert_eq!(class_mip_levels(254, 256, 3), 1);
        assert_eq!(class_mip_levels(256, 256, 6), 1);
        assert_eq!(class_mip_levels(96, 96, 2), 4);
    }

    #[test]
    fn a_degenerate_class_falls_back_to_a_single_level() {
        for (w, h, tiles) in [
            (256, 256, 0),
            (256, 256, -4),
            (0, 256, 4),
            (256, 0, 4),
            (256, 256, i32::MAX),
            (1, 1, 1),
        ] {
            assert_eq!(class_mip_levels(w, h, tiles), 1, "{w}x{h}, {tiles} tiles");
        }
    }
}
