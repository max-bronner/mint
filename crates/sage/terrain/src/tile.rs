use mint_sage_map::TextureClass;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellTile {
    pub class: usize,
    pub uv_xy: [f32; 2],
    pub uv_x1y1: [f32; 2],
}

#[derive(Debug, Clone, Copy)]
struct Range {
    first: i64,
    end: i64,
    class: usize,
    width: i64,
    rows: i64,
}

#[derive(Debug, Clone)]
pub struct TileLayout {
    ranges: Vec<Range>,
}

impl TileLayout {
    #[must_use]
    pub fn new(classes: &[TextureClass]) -> Self {
        let mut ranges = Vec::with_capacity(classes.len());
        for (class, entry) in classes.iter().enumerate() {
            let (first, count, width) = (
                i64::from(entry.first_tile),
                i64::from(entry.num_tiles),
                i64::from(entry.width),
            );
            if count <= 0 || width <= 0 {
                continue;
            }
            let Some(rows) = count
                .saturating_add(width.saturating_sub(1))
                .checked_div(width)
            else {
                continue;
            };
            ranges.push(Range {
                first,
                end: first.saturating_add(count),
                class,
                width,
                rows,
            });
        }
        ranges.sort_by_key(|range| range.first);
        Self { ranges }
    }

    #[must_use]
    pub fn cell(&self, value: u16) -> Option<CellTile> {
        let tile = i64::from(value >> 2);
        let sub = value & 3;
        let index = self
            .ranges
            .binary_search_by(|range| {
                if range.end <= tile {
                    std::cmp::Ordering::Less
                } else if range.first > tile {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            })
            .ok()?;
        let range = self.ranges.get(index)?;

        let local = tile.checked_sub(range.first)?;
        let column = local.checked_rem(range.width)?;
        let row = local.checked_div(range.width)?;
        if row >= range.rows {
            return None;
        }

        let (width, rows) = (range.width as f32, range.rows as f32);
        let sub_u = f32::from(sub & 1) * 0.5;
        let sub_v = f32::from(sub >> 1) * 0.5;
        let u0 = (column as f32 + sub_u) / width;
        let v0 = 1.0 - (row as f32 + sub_v) / rows;
        Some(CellTile {
            class: range.class,
            uv_xy: [u0, v0],
            uv_x1y1: [u0 + 0.5 / width, v0 - 0.5 / rows],
        })
    }
}

#[cfg(test)]
#[allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn class(first_tile: i32, num_tiles: i32, width: i32) -> TextureClass {
        TextureClass {
            first_tile,
            num_tiles,
            width,
            name: format!("class{first_tile}"),
        }
    }

    #[test]
    fn a_cells_rectangle_is_its_tile_then_its_quadrant() {
        let layout = TileLayout::new(&[class(0, 16, 4)]);
        let cell = layout.cell((6 << 2) | 3).expect("tile 6 is in range");
        assert_eq!(cell.class, 0);
        assert_eq!(cell.uv_xy, [0.625, 0.625]);
        assert_eq!(cell.uv_x1y1, [0.75, 0.5]);

        let cell = layout.cell(6 << 2).expect("tile 6 is in range");
        assert_eq!(cell.uv_xy, [0.5, 0.75]);
        assert_eq!(cell.uv_x1y1, [0.625, 0.625]);
    }

    #[test]
    fn tile_rows_are_counted_from_the_bottom_of_the_image() {
        let layout = TileLayout::new(&[class(0, 16, 4)]);
        let first = layout.cell(0).expect("tile 0 is in range");
        assert_eq!(first.uv_xy, [0.0, 1.0]);
        let last = layout.cell((15 << 2) | 3).expect("tile 15 is in range");
        assert_eq!(last.uv_x1y1, [1.0, 0.0]);
    }

    #[test]
    fn the_four_sub_tiles_of_a_tile_cover_it_exactly_once() {
        let layout = TileLayout::new(&[class(0, 16, 4)]);
        let quads: Vec<CellTile> = (0..4)
            .map(|sub| layout.cell((5 << 2) | sub).unwrap())
            .collect();
        assert_eq!(quads[0].uv_xy[0], quads[2].uv_xy[0]);
        assert_eq!(quads[1].uv_xy[0], quads[3].uv_xy[0]);
        assert_eq!(quads[0].uv_x1y1[0], quads[1].uv_xy[0]);
        assert_eq!(quads[0].uv_xy[1], quads[1].uv_xy[1]);
        assert_eq!(quads[2].uv_xy[1], quads[3].uv_xy[1]);
        assert_eq!(quads[0].uv_x1y1[1], quads[2].uv_xy[1]);
        assert_eq!(quads[0].uv_xy, [0.25, 0.75]);
        assert_eq!(quads[3].uv_x1y1, [0.5, 0.5]);
    }

    #[test]
    fn a_tile_index_past_every_class_has_no_rectangle() {
        let layout = TileLayout::new(&[class(0, 16, 4), class(16, 16, 4)]);
        assert!(layout.cell(31 << 2).is_some());
        assert!(layout.cell(32 << 2).is_none());
        assert!(layout.cell(u16::MAX).is_none());
    }

    #[test]
    fn a_class_declaring_no_tiles_or_no_width_is_skipped_rather_than_dividing_by_zero() {
        let layout = TileLayout::new(&[class(0, 0, 4), class(0, 16, 0), class(0, 16, -4)]);
        for value in [0u16, 1, 4, 63, u16::MAX] {
            assert!(layout.cell(value).is_none(), "value {value}");
        }
    }

    #[test]
    fn a_class_whose_tile_count_is_not_a_square_still_divides_v_by_its_own_row_count() {
        let layout = TileLayout::new(&[class(0, 6, 4)]);
        let cell = layout.cell(4 << 2).expect("tile 4 is in range");
        assert_eq!(cell.uv_xy, [0.0, 0.5]);
        assert_eq!(cell.uv_x1y1, [0.125, 0.25]);
    }
}
