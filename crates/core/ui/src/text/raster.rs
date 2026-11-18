use crate::text::outline::GlyphOutline;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct RasterizedGlyph {
    pub width: u32,
    pub height: u32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub coverage: Vec<u8>,
}

pub const MAX_RASTER_DIMENSION: u32 = 4096;

const SAMPLES: u32 = 4;

#[must_use]
pub fn rasterize(outline: &GlyphOutline) -> Option<RasterizedGlyph> {
    let Some((min_x, min_y, max_x, max_y)) = bounds(outline) else {
        return Some(RasterizedGlyph::default());
    };

    let left = min_x.floor();
    let top = max_y.ceil();
    let width_f = (max_x.ceil() - left).max(0.0);
    let height_f = (top - min_y.floor()).max(0.0);

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (width, height) = (width_f as u32, height_f as u32);
    if width == 0 || height == 0 {
        return Some(RasterizedGlyph::default());
    }
    if width > MAX_RASTER_DIMENSION || height > MAX_RASTER_DIMENSION {
        return None;
    }

    let raster_contours: Vec<Vec<[f32; 2]>> = outline
        .contours
        .iter()
        .map(|contour| contour.iter().map(|&[x, y]| [x - left, top - y]).collect())
        .collect();

    let mut coverage = vec![0u8; width as usize * height as usize];
    #[allow(clippy::cast_precision_loss)]
    let total_samples = (SAMPLES * SAMPLES) as f32;
    for py in 0..height {
        for px in 0..width {
            let mut inside_count: u32 = 0;
            for sy in 0..SAMPLES {
                for sx in 0..SAMPLES {
                    let sample = [pixel_to_sample(px, sx), pixel_to_sample(py, sy)];
                    if winding_nonzero(&raster_contours, sample) {
                        inside_count += 1;
                    }
                }
            }
            #[allow(clippy::cast_precision_loss)]
            let fraction = inside_count as f32 / total_samples;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let byte = (fraction * 255.0).round() as u8;
            let index = py as usize * width as usize + px as usize;
            if let Some(slot) = coverage.get_mut(index) {
                *slot = byte;
            }
        }
    }

    Some(RasterizedGlyph {
        width,
        height,
        bearing_x: left,
        bearing_y: top,
        coverage,
    })
}

#[allow(clippy::cast_precision_loss)]
fn pixel_to_sample(pixel: u32, sub: u32) -> f32 {
    pixel as f32 + (sub as f32 + 0.5) / SAMPLES as f32
}

fn bounds(outline: &GlyphOutline) -> Option<(f32, f32, f32, f32)> {
    let mut result: Option<(f32, f32, f32, f32)> = None;
    for contour in &outline.contours {
        for &[x, y] in contour {
            result = Some(match result {
                None => (x, y, x, y),
                Some((min_x, min_y, max_x, max_y)) => {
                    (min_x.min(x), min_y.min(y), max_x.max(x), max_y.max(y))
                }
            });
        }
    }
    result
}

fn winding_nonzero(contours: &[Vec<[f32; 2]>], point: [f32; 2]) -> bool {
    let mut winding: i32 = 0;
    for contour in contours {
        let len = contour.len();
        if len < 2 {
            continue;
        }
        for i in 0..len {
            let Some(&a) = contour.get(i) else {
                continue;
            };
            let Some(&b) = contour.get((i + 1) % len) else {
                continue;
            };
            let (ax, ay) = (a[0], a[1]);
            let (bx, by) = (b[0], b[1]);
            if (ay <= point[1]) != (by <= point[1]) {
                let t = (point[1] - ay) / (by - ay);
                let x_at = ax + t * (bx - ax);
                if x_at > point[0] {
                    winding += if by > ay { 1 } else { -1 };
                }
            }
        }
    }
    winding != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(min: f32, max: f32) -> GlyphOutline {
        GlyphOutline {
            contours: vec![vec![
                [min, min],
                [max, min],
                [max, max],
                [min, max],
                [min, min],
            ]],
        }
    }

    #[test]
    fn a_solid_square_rasterises_to_full_coverage_everywhere_inside() {
        let raster = rasterize(&square(0.0, 10.0)).unwrap();
        assert_eq!((raster.width, raster.height), (10, 10));
        assert_eq!(raster.coverage.len(), 100);
        for &byte in &raster.coverage {
            assert_eq!(
                byte, 255,
                "an axis-aligned square must have no partial pixels"
            );
        }
    }

    #[test]
    fn an_empty_outline_rasterises_to_a_zero_sized_bitmap() {
        let raster = rasterize(&GlyphOutline::default()).unwrap();
        assert_eq!((raster.width, raster.height), (0, 0));
        assert!(raster.coverage.is_empty());
    }

    #[test]
    fn a_triangle_is_full_inside_and_empty_well_outside() {
        let outline = GlyphOutline {
            contours: vec![vec![[0.0, 0.0], [10.0, 0.0], [0.0, 10.0], [0.0, 0.0]]],
        };
        let raster = rasterize(&outline).unwrap();
        let at = |x: u32, y: u32| raster.coverage[(y * raster.width + x) as usize];
        assert_eq!(at(1, 8), 255, "deep inside the right-angle corner");
        assert_eq!(at(8, 1), 0, "well outside the hypotenuse");
    }

    #[test]
    fn an_inner_contour_wound_the_opposite_way_cuts_a_hole() {
        let outer = vec![
            [0.0, 0.0],
            [20.0, 0.0],
            [20.0, 20.0],
            [0.0, 20.0],
            [0.0, 0.0],
        ];
        let inner = vec![
            [5.0, 5.0],
            [5.0, 15.0],
            [15.0, 15.0],
            [15.0, 5.0],
            [5.0, 5.0],
        ];
        let outline = GlyphOutline {
            contours: vec![outer, inner],
        };
        let raster = rasterize(&outline).unwrap();
        let at = |x: u32, y: u32| raster.coverage[(y * raster.width + x) as usize];
        assert_eq!(at(10, 10), 0, "the hole's centre must be empty");
        assert_eq!(at(1, 1), 255, "the ring itself must be full");
    }

    #[test]
    fn an_oversized_bounding_box_is_refused_not_allocated() {
        let huge = square(0.0, (MAX_RASTER_DIMENSION + 1) as f32);
        assert!(rasterize(&huge).is_none());
    }
}
