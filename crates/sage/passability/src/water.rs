use mint_sage_map::{HeightMapData, RiverArea, StandingWaterArea, Water, MAP_XY_FACTOR};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaterDepths {
    pub wade: f32,
    pub deep: f32,
}

impl WaterDepths {
    pub const SHIPPED: Self = Self {
        wade: 5.0,
        deep: 6.0,
    };

    #[must_use]
    pub fn read(block: &mint_sage_ini::Block) -> (Self, usize) {
        let mut depths = Self::SHIPPED;
        let mut read = 0usize;
        for field in block.fields() {
            let value = field.value().and_then(mint_sage_ini::value::parse_float);
            let Some(value) = value else { continue };
            if !value.is_finite() || value < 0.0 {
                continue;
            }
            if field.keyword.eq_ignore_ascii_case("WadeWaterDepth") {
                depths.wade = value;
                read = read.saturating_add(1);
            } else if field.keyword.eq_ignore_ascii_case("DeepWaterDepth") {
                depths.deep = value;
                read = read.saturating_add(1);
            }
        }
        if depths.deep < depths.wade {
            depths.deep = depths.wade;
        }
        (depths, read)
    }
}

impl Default for WaterDepths {
    fn default() -> Self {
        Self::SHIPPED
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Wetness {
    #[default]
    Dry,
    Ford,
    Water,
    Deep,
}

#[derive(Debug)]
pub struct WetGrid {
    width: u32,
    height: u32,
    wetness: Vec<Wetness>,
    surfaces: Vec<(f32, bool)>,
    pub areas_used: u32,
    pub areas_degenerate: u32,
}

impl WetGrid {
    #[must_use]
    pub fn dry(width: u32, height: u32) -> Self {
        let count = usize::try_from(width)
            .ok()
            .and_then(|w| usize::try_from(height).ok().and_then(|h| w.checked_mul(h)))
            .unwrap_or(0);
        Self {
            width,
            height,
            wetness: vec![Wetness::Dry; count],
            surfaces: vec![(f32::NEG_INFINITY, false); count],
            areas_used: 0,
            areas_degenerate: 0,
        }
    }

    #[must_use]
    pub fn at(&self, x: u32, y: u32) -> Wetness {
        if x >= self.width || y >= self.height {
            return Wetness::Dry;
        }
        let index = usize::try_from(y)
            .ok()
            .and_then(|y| {
                usize::try_from(self.width)
                    .ok()
                    .and_then(|w| y.checked_mul(w))
            })
            .and_then(|row| usize::try_from(x).ok().and_then(|x| row.checked_add(x)));
        index
            .and_then(|index| self.wetness.get(index))
            .copied()
            .unwrap_or(Wetness::Dry)
    }

    #[must_use]
    pub fn in_river(&self, x: u32, y: u32) -> bool {
        self.wetness_index(x, y)
            .and_then(|index| self.surfaces.get(index))
            .is_some_and(|(_, river)| *river)
            && self.at(x, y) != Wetness::Dry
    }

    fn wetness_index(&self, x: u32, y: u32) -> Option<usize> {
        if x >= self.width || y >= self.height {
            return None;
        }
        usize::try_from(y)
            .ok()
            .and_then(|y| {
                usize::try_from(self.width)
                    .ok()
                    .and_then(|w| y.checked_mul(w))
            })
            .and_then(|row| usize::try_from(x).ok().and_then(|x| row.checked_add(x)))
    }

    #[must_use]
    pub fn build(
        height_map: &HeightMapData,
        vertical_scale: f32,
        water: &Water,
        depths: WaterDepths,
    ) -> Self {
        let mut grid = Self::dry(height_map.width, height_map.height);
        let scale = if vertical_scale.is_finite() {
            vertical_scale.abs()
        } else {
            0.0
        };
        for area in water.standing_areas() {
            if standing_polygon(area).len() < 3 {
                grid.areas_degenerate = grid.areas_degenerate.saturating_add(1);
                continue;
            }
            grid.areas_used = grid.areas_used.saturating_add(1);
            grid.stamp_polygon(
                height_map,
                scale,
                standing_polygon(area),
                (surface_of(area.water_height), false),
                depths,
            );
        }
        for area in water.river_areas() {
            if area.lines.len() < 2 {
                grid.areas_degenerate = grid.areas_degenerate.saturating_add(1);
                continue;
            }
            grid.areas_used = grid.areas_used.saturating_add(1);
            grid.stamp_river(height_map, scale, area, depths);
        }
        grid
    }

    fn stamp_polygon(
        &mut self,
        height_map: &HeightMapData,
        scale: f32,
        polygon: &[[f32; 2]],
        surface: (f32, bool),
        depths: WaterDepths,
    ) {
        let Some(bounds) = sample_bounds(height_map, polygon) else {
            return;
        };
        for y in bounds.y0..=bounds.y1 {
            for x in bounds.x0..=bounds.x1 {
                let point = sample_world(height_map, x, y);
                if !inside(polygon, point) {
                    continue;
                }
                self.classify(height_map, scale, x, y, surface, depths);
            }
        }
    }

    fn stamp_river(
        &mut self,
        height_map: &HeightMapData,
        scale: f32,
        area: &RiverArea,
        depths: WaterDepths,
    ) {
        let surface = (surface_of(area.water_height), true);
        for pair in area.lines.windows(2) {
            let (Some(near), Some(far)) = (pair.first(), pair.get(1)) else {
                continue;
            };
            let quad = [near.a, near.b, far.b, far.a];
            self.stamp_polygon(height_map, scale, &quad, surface, depths);
        }
    }

    fn classify(
        &mut self,
        height_map: &HeightMapData,
        scale: f32,
        x: u32,
        y: u32,
        surface: (f32, bool),
        depths: WaterDepths,
    ) {
        let Some(sample) = height_map.sample(x, y) else {
            return;
        };
        if let Some(highest) = self
            .wetness_index(x, y)
            .and_then(|index| self.surfaces.get_mut(index))
        {
            if surface.0 > highest.0 {
                *highest = surface;
            }
        }
        let (surface, _) = surface;
        let ground = f32::from(sample) * scale;
        let depth = surface - ground;
        let wetness = if depth <= 0.0 {
            Wetness::Dry
        } else if depth <= depths.wade {
            Wetness::Ford
        } else if depth < depths.deep {
            Wetness::Water
        } else {
            Wetness::Deep
        };
        self.raise(x, y, wetness);
    }

    fn raise(&mut self, x: u32, y: u32, wetness: Wetness) {
        if x >= self.width || y >= self.height {
            return;
        }
        let index = usize::try_from(y)
            .ok()
            .and_then(|y| {
                usize::try_from(self.width)
                    .ok()
                    .and_then(|w| y.checked_mul(w))
            })
            .and_then(|row| usize::try_from(x).ok().and_then(|x| row.checked_add(x)));
        let Some(slot) = index.and_then(|index| self.wetness.get_mut(index)) else {
            return;
        };
        if rank(wetness) > rank(*slot) {
            *slot = wetness;
        }
    }
}

fn standing_polygon(area: &StandingWaterArea) -> &[[f32; 2]] {
    &area.points
}

fn surface_of(water_height: u32) -> f32 {
    const EXACT: u32 = 1 << 24;
    #[allow(clippy::cast_precision_loss)]
    {
        water_height.min(EXACT) as f32
    }
}

#[must_use]
pub fn deeper(a: Wetness, b: Wetness) -> bool {
    rank(a) > rank(b)
}

fn rank(wetness: Wetness) -> u8 {
    match wetness {
        Wetness::Dry => 0,
        Wetness::Ford => 1,
        Wetness::Water => 2,
        Wetness::Deep => 3,
    }
}

struct SampleBounds {
    x0: u32,
    x1: u32,
    y0: u32,
    y1: u32,
}

fn sample_bounds(height_map: &HeightMapData, polygon: &[[f32; 2]]) -> Option<SampleBounds> {
    let first = polygon.first()?;
    let (mut min_x, mut max_x) = (first[0], first[0]);
    let (mut min_y, mut max_y) = (first[1], first[1]);
    for point in polygon {
        let (Some(&x), Some(&y)) = (point.first(), point.get(1)) else {
            continue;
        };
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        min_x = min_x.min(x);
        max_x = max_x.max(x);
        min_y = min_y.min(y);
        max_y = max_y.max(y);
    }
    let border = border_of(height_map);
    let to_index = |world: f32, extent: u32, round_up: bool| -> Option<u32> {
        let raw = world / MAP_XY_FACTOR + border;
        if !raw.is_finite() {
            return None;
        }
        let raw = if round_up { raw.ceil() } else { raw.floor() };
        if raw < 0.0 {
            return Some(0);
        }
        let limit = extent.checked_sub(1)?;
        if raw > f32::from(u16::MAX) {
            return Some(limit);
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some((raw as u32).min(limit))
    };
    let x0 = to_index(min_x, height_map.width, false)?;
    let x1 = to_index(max_x, height_map.width, true)?;
    let y0 = to_index(min_y, height_map.height, false)?;
    let y1 = to_index(max_y, height_map.height, true)?;
    if x1 < x0 || y1 < y0 {
        return None;
    }
    Some(SampleBounds { x0, x1, y0, y1 })
}

fn border_of(height_map: &HeightMapData) -> f32 {
    f32::from(u16::try_from(height_map.border_size).unwrap_or(u16::MAX))
}

fn sample_world(height_map: &HeightMapData, x: u32, y: u32) -> [f32; 2] {
    let border = border_of(height_map);
    let to_world =
        |index: u32| (f32::from(u16::try_from(index).unwrap_or(u16::MAX)) - border) * MAP_XY_FACTOR;
    [to_world(x), to_world(y)]
}

fn inside(polygon: &[[f32; 2]], point: [f32; 2]) -> bool {
    let (Some(&px), Some(&py)) = (point.first(), point.get(1)) else {
        return false;
    };
    let mut crossings = 0u32;
    let count = polygon.len();
    for index in 0..count {
        let Some(a) = polygon.get(index) else {
            continue;
        };
        let next = index.saturating_add(1);
        let next = if next >= count { 0 } else { next };
        let Some(b) = polygon.get(next) else {
            continue;
        };
        let (Some(&ax), Some(&ay)) = (a.first(), a.get(1)) else {
            continue;
        };
        let (Some(&bx), Some(&by)) = (b.first(), b.get(1)) else {
            continue;
        };
        if (ay > py) == (by > py) {
            continue;
        }
        let dy = by - ay;
        let lhs = (px - ax) * dy;
        let rhs = (py - ay) * (bx - ax);
        let crosses = if dy > 0.0 { lhs < rhs } else { lhs > rhs };
        if crosses {
            crossings = crossings.saturating_add(1);
        }
    }
    crossings % 2 == 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sage_map::{RiverLine, WaterAreas};

    fn height_field(width: u32, height: u32, value: u16) -> HeightMapData {
        HeightMapData {
            width,
            height,
            border_size: 0,
            boundaries: Vec::new(),
            samples: vec![value; width.saturating_mul(height) as usize],
        }
    }

    fn standing(points: Vec<[f32; 2]>, water_height: u32) -> StandingWaterArea {
        StandingWaterArea {
            unique_id: 1,
            name: String::new(),
            layer_name: String::new(),
            uv_scroll_speed: 0.0,
            additive_blend: false,
            bump_texture: String::new(),
            sky_texture: String::new(),
            points,
            water_height,
            fx_shader: String::new(),
            depth_colors: String::new(),
        }
    }

    fn lake(points: Vec<[f32; 2]>, water_height: u32) -> Water {
        Water {
            standing: Some(WaterAreas {
                version: 2,
                areas: vec![standing(points, water_height)],
            }),
            rivers: None,
            waves: None,
        }
    }

    fn square() -> Vec<[f32; 2]> {
        vec![[5.0, 5.0], [95.0, 5.0], [95.0, 95.0], [5.0, 95.0]]
    }

    #[test]
    fn a_point_inside_a_convex_polygon_is_inside_and_one_outside_is_not() {
        let poly = square();
        assert!(inside(&poly, [50.0, 50.0]));
        assert!(!inside(&poly, [-1.0, 50.0]));
        assert!(!inside(&poly, [200.0, 50.0]));
        assert!(!inside(&poly, [50.0, -1.0]));
    }

    #[test]
    fn a_concave_polygon_excludes_its_notch() {
        let el = vec![
            [0.0, 0.0],
            [100.0, 0.0],
            [100.0, 40.0],
            [40.0, 40.0],
            [40.0, 100.0],
            [0.0, 100.0],
        ];
        assert!(inside(&el, [20.0, 20.0]));
        assert!(inside(&el, [80.0, 20.0]));
        assert!(inside(&el, [20.0, 80.0]));
        assert!(!inside(&el, [80.0, 80.0]), "the notch is outside the L");
    }

    #[test]
    fn the_two_shipped_depths_split_a_lake_three_ways() {
        for (surface, expected) in [
            (0u32, Wetness::Dry),
            (4, Wetness::Ford),
            (5, Wetness::Ford),
            (6, Wetness::Deep),
            (20, Wetness::Deep),
        ] {
            let hm = height_field(12, 12, 0);
            let grid = WetGrid::build(&hm, 1.0, &lake(square(), surface), WaterDepths::SHIPPED);
            assert_eq!(grid.at(5, 5), expected, "a surface at {surface}");
        }
        let mut hm = height_field(12, 12, 0);
        hm.samples = vec![1; 144];
        let grid = WetGrid::build(&hm, 0.5, &lake(square(), 6), WaterDepths::SHIPPED);
        assert_eq!(grid.at(5, 5), Wetness::Water, "6 - 0.5 = 5.5 world units");
    }

    #[test]
    fn the_water_height_is_world_units_and_the_ground_is_not() {
        let mut hm = height_field(12, 12, 512);
        hm.border_size = 0;
        let scale = 5.0 / 128.0;
        let dry = WetGrid::build(&hm, scale, &lake(square(), 10), WaterDepths::SHIPPED);
        assert_eq!(dry.at(5, 5), Wetness::Dry);
        let wet = WetGrid::build(&hm, scale, &lake(square(), 30), WaterDepths::SHIPPED);
        assert_eq!(wet.at(5, 5), Wetness::Deep);
    }

    #[test]
    fn overlapping_bodies_resolve_to_the_deeper_one_in_either_order() {
        let hm = height_field(12, 12, 0);
        let shallow = standing(square(), 3);
        let deep = standing(square(), 40);
        let one = Water {
            standing: Some(WaterAreas {
                version: 2,
                areas: vec![shallow.clone(), deep.clone()],
            }),
            rivers: None,
            waves: None,
        };
        let other = Water {
            standing: Some(WaterAreas {
                version: 2,
                areas: vec![deep, shallow],
            }),
            rivers: None,
            waves: None,
        };
        let a = WetGrid::build(&hm, 1.0, &one, WaterDepths::SHIPPED);
        let b = WetGrid::build(&hm, 1.0, &other, WaterDepths::SHIPPED);
        assert_eq!(a.at(5, 5), Wetness::Deep);
        assert_eq!(b.at(5, 5), Wetness::Deep);
    }

    #[test]
    fn a_polygon_reaching_far_off_the_map_is_clamped_rather_than_panicking() {
        let hm = height_field(8, 8, 0);
        let huge = vec![
            [-100_000.0, -100_000.0],
            [100_000.0, -100_000.0],
            [100_000.0, 100_000.0],
            [-100_000.0, 100_000.0],
        ];
        let grid = WetGrid::build(&hm, 1.0, &lake(huge, 50), WaterDepths::SHIPPED);
        assert_eq!(grid.at(0, 0), Wetness::Deep);
        assert_eq!(grid.at(7, 7), Wetness::Deep);
        assert_eq!(grid.at(8, 8), Wetness::Dry, "off the grid reads dry");
    }

    #[test]
    fn a_polygon_with_non_finite_vertices_stamps_nothing() {
        let hm = height_field(8, 8, 0);
        for bad in [f32::NAN, f32::INFINITY, -f32::INFINITY] {
            let poly = vec![[0.0, 0.0], [bad, 0.0], [50.0, 50.0], [0.0, 50.0]];
            let grid = WetGrid::build(&hm, 1.0, &lake(poly, 50), WaterDepths::SHIPPED);
            for y in 0..8 {
                for x in 0..8 {
                    assert_eq!(grid.at(x, y), Wetness::Dry, "{bad} at ({x}, {y})");
                }
            }
        }
    }

    #[test]
    fn a_river_ribbon_wets_the_ground_between_its_banks() {
        let hm = height_field(16, 16, 0);
        let river = Water {
            standing: None,
            rivers: Some(WaterAreas {
                version: 2,
                areas: vec![RiverArea {
                    unique_id: 1,
                    name: String::new(),
                    layer_name: String::new(),
                    uv_scroll_speed: 0.0,
                    additive_blend: false,
                    river_texture: String::new(),
                    noise_texture: String::new(),
                    alpha_edge_texture: String::new(),
                    sparkle_texture: String::new(),
                    color: [0, 0, 0],
                    reserved: 0,
                    alpha: 1.0,
                    water_height: 50,
                    minimum_water_lod: String::new(),
                    lines: vec![
                        RiverLine {
                            a: [20.0, 5.0],
                            b: [20.0, 95.0],
                        },
                        RiverLine {
                            a: [60.0, 5.0],
                            b: [60.0, 95.0],
                        },
                    ],
                }],
            }),
            waves: None,
        };
        let grid = WetGrid::build(&hm, 1.0, &river, WaterDepths::SHIPPED);
        assert_eq!(grid.at(4, 5), Wetness::Deep, "inside the ribbon");
        assert_eq!(grid.at(10, 5), Wetness::Dry, "past its far bank");
    }

    #[test]
    fn only_ground_under_a_river_as_its_highest_water_is_in_river() {
        let hm = height_field(16, 16, 0);
        let ribbon = |lake_height: u32| {
            let mut water = lake(
                vec![[30.0, 50.0], [140.0, 50.0], [140.0, 95.0], [30.0, 95.0]],
                lake_height,
            );
            water.rivers = Some(WaterAreas {
                version: 2,
                areas: vec![RiverArea {
                    unique_id: 2,
                    name: String::new(),
                    layer_name: String::new(),
                    uv_scroll_speed: 0.0,
                    additive_blend: false,
                    river_texture: String::new(),
                    noise_texture: String::new(),
                    alpha_edge_texture: String::new(),
                    sparkle_texture: String::new(),
                    color: [0, 0, 0],
                    reserved: 0,
                    alpha: 1.0,
                    water_height: 50,
                    minimum_water_lod: String::new(),
                    lines: vec![
                        RiverLine {
                            a: [20.0, 5.0],
                            b: [20.0, 95.0],
                        },
                        RiverLine {
                            a: [60.0, 5.0],
                            b: [60.0, 95.0],
                        },
                    ],
                }],
            });
            WetGrid::build(&hm, 1.0, &water, WaterDepths::SHIPPED)
        };
        let low_lake = ribbon(30);
        assert!(low_lake.in_river(4, 2), "river alone");
        assert!(low_lake.in_river(4, 7), "river above the lake");
        assert!(!low_lake.in_river(10, 7), "lake alone");
        assert!(!low_lake.in_river(12, 2), "dry ground");
        assert!(!ribbon(80).in_river(4, 7), "a higher lake over the river");
    }

    #[test]
    fn a_river_with_one_cross_section_is_counted_as_degenerate_rather_than_stamped() {
        let hm = height_field(8, 8, 0);
        let river = Water {
            standing: None,
            rivers: Some(WaterAreas {
                version: 2,
                areas: vec![RiverArea {
                    unique_id: 1,
                    name: String::new(),
                    layer_name: String::new(),
                    uv_scroll_speed: 0.0,
                    additive_blend: false,
                    river_texture: String::new(),
                    noise_texture: String::new(),
                    alpha_edge_texture: String::new(),
                    sparkle_texture: String::new(),
                    color: [0, 0, 0],
                    reserved: 0,
                    alpha: 1.0,
                    water_height: 50,
                    minimum_water_lod: String::new(),
                    lines: vec![RiverLine {
                        a: [0.0, 0.0],
                        b: [10.0, 10.0],
                    }],
                }],
            }),
            waves: None,
        };
        let grid = WetGrid::build(&hm, 1.0, &river, WaterDepths::SHIPPED);
        assert_eq!(grid.areas_degenerate, 1);
        assert_eq!(grid.areas_used, 0);
    }

    #[test]
    fn depths_written_the_wrong_way_round_are_widened_rather_than_believed() {
        let hm = height_field(8, 8, 0);
        let depths = WaterDepths {
            wade: 10.0,
            deep: 2.0,
        };
        let grid = WetGrid::build(&hm, 1.0, &lake(square(), 5), depths);
        assert_eq!(grid.at(3, 3), Wetness::Ford);
    }
}
