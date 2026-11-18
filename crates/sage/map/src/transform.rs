pub const MAP_XY_FACTOR: f32 = 10.0;

#[must_use]
pub fn cell_to_world(cell: i32, border_size: u32) -> f32 {
    let cell = cell as f32;
    let border = border_size as f32;
    (cell - border) * MAP_XY_FACTOR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_documented_formula() {
        assert_eq!(cell_to_world(30, 30), 0.0);
        assert_eq!(cell_to_world(0, 30), -300.0);
        assert_eq!(cell_to_world(40, 30), 100.0);
    }
}
