pub trait Ground {
    fn height_at(&self, x: f32, y: f32) -> Option<f32>;
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FlatGround(pub f32);

impl Ground for FlatGround {
    fn height_at(&self, _x: f32, _y: f32) -> Option<f32> {
        Some(self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct NoGround;

impl Ground for NoGround {
    fn height_at(&self, _x: f32, _y: f32) -> Option<f32> {
        None
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;

    #[test]
    fn a_flat_ground_answers_everywhere_and_no_ground_answers_nowhere() {
        assert_eq!(FlatGround(12.5).height_at(-4000.0, 9000.0), Some(12.5));
        assert_eq!(NoGround.height_at(0.0, 0.0), None);
    }
}
