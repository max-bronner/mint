#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DepthTest {
    #[default]
    Nearer,
    NearerOrEqual,
    Always,
}

impl DepthTest {
    #[must_use]
    pub(crate) fn index(self) -> usize {
        match self {
            Self::Nearer => 0,
            Self::NearerOrEqual => 1,
            Self::Always => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DepthDirection {
    Standard,
    #[default]
    Reversed,
}

impl DepthDirection {
    #[must_use]
    pub fn clear_value(self) -> f32 {
        match self {
            Self::Standard => 1.0,
            Self::Reversed => 0.0,
        }
    }

    #[must_use]
    pub(crate) fn index(self) -> usize {
        match self {
            Self::Standard => 0,
            Self::Reversed => 1,
        }
    }

    #[must_use]
    pub fn linearisation(self, near: f32, far: f32) -> [f32; 3] {
        let scale = near * far;
        match self {
            Self::Standard => [scale, near - far, far],
            Self::Reversed => [scale, far - near, near],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearer_is_the_default_and_the_indices_are_distinct() {
        assert_eq!(DepthTest::default(), DepthTest::Nearer);
        assert_ne!(DepthTest::Nearer.index(), DepthTest::NearerOrEqual.index());
    }

    #[test]
    fn linearising_a_projected_depth_recovers_the_distance_at_either_end() {
        let (near, far) = (0.1f32, 1000.0f32);
        for direction in [DepthDirection::Standard, DepthDirection::Reversed] {
            let [a, b, c] = direction.linearisation(near, far);
            let at = |d: f32| a / (b * d + c);
            let cleared = direction.clear_value();
            assert!((at(cleared) - far).abs() < 0.5, "{direction:?} far");
            assert!(
                (at(1.0 - cleared) - near).abs() < 1e-4,
                "{direction:?} near"
            );
            let matrix = match direction {
                DepthDirection::Standard => crate::math::Mat4::perspective(1.0, 1.0, near, far),
                DepthDirection::Reversed => {
                    crate::math::Mat4::perspective_reversed_z(1.0, 1.0, near, far)
                }
            };
            let t = 37.5f32;
            let z = matrix.get(2, 2) * -t + matrix.get(2, 3);
            let w = matrix.get(3, 2) * -t + matrix.get(3, 3);
            let d = z / w;
            assert!(
                (at(d) - t).abs() < 0.01,
                "{direction:?} mid: {} vs {t}",
                at(d)
            );
        }
    }

    #[test]
    fn the_clear_value_is_the_far_plane_at_either_end() {
        assert_eq!(DepthDirection::default(), DepthDirection::Reversed);
        assert_eq!(DepthDirection::Standard.clear_value(), 1.0);
        assert_eq!(DepthDirection::Reversed.clear_value(), 0.0);
        assert_ne!(
            DepthDirection::Standard.index(),
            DepthDirection::Reversed.index()
        );
    }
}
