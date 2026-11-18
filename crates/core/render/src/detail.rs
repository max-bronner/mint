#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DetailCombine {
    #[default]
    Disable,
    Detail,
    Scale,
    InvScale,
    Add,
    Sub,
    Subr,
    Blend,
    DetailBlend,
    AddSigned,
    AddSigned2x,
    Scale2x,
    ModAlphaAddColor,
    HouseColor,
}

impl DetailCombine {
    #[must_use]
    pub fn combine_rgb(
        self,
        local: [f32; 3],
        other: [f32; 3],
        local_a: f32,
        other_a: f32,
    ) -> [f32; 3] {
        let mix = |t: f32, a: [f32; 3], b: [f32; 3]| -> [f32; 3] {
            [
                a[0] * t + b[0] * (1.0 - t),
                a[1] * t + b[1] * (1.0 - t),
                a[2] * t + b[2] * (1.0 - t),
            ]
        };
        match self {
            Self::Disable | Self::HouseColor => local,
            Self::Detail => other,
            Self::Scale => [
                local[0] * other[0],
                local[1] * other[1],
                local[2] * other[2],
            ],
            Self::InvScale => [
                local[0] + (1.0 - local[0]) * other[0],
                local[1] + (1.0 - local[1]) * other[1],
                local[2] + (1.0 - local[2]) * other[2],
            ],
            Self::Add => [
                local[0] + other[0],
                local[1] + other[1],
                local[2] + other[2],
            ],
            Self::Sub => [
                local[0] - other[0],
                local[1] - other[1],
                local[2] - other[2],
            ],
            Self::Subr => [
                other[0] - local[0],
                other[1] - local[1],
                other[2] - local[2],
            ],
            Self::Blend => mix(local_a, local, other),
            Self::DetailBlend => mix(other_a, local, other),
            Self::AddSigned => [
                (local[0] + other[0] - 0.5).clamp(0.0, 1.0),
                (local[1] + other[1] - 0.5).clamp(0.0, 1.0),
                (local[2] + other[2] - 0.5).clamp(0.0, 1.0),
            ],
            Self::AddSigned2x => [
                ((local[0] + other[0] - 0.5) * 2.0).clamp(0.0, 1.0),
                ((local[1] + other[1] - 0.5) * 2.0).clamp(0.0, 1.0),
                ((local[2] + other[2] - 0.5) * 2.0).clamp(0.0, 1.0),
            ],
            Self::Scale2x => [
                (2.0 * local[0] * other[0]).clamp(0.0, 1.0),
                (2.0 * local[1] * other[1]).clamp(0.0, 1.0),
                (2.0 * local[2] * other[2]).clamp(0.0, 1.0),
            ],
            Self::ModAlphaAddColor => [
                (local[0] + local_a * other[0]).clamp(0.0, 1.0),
                (local[1] + local_a * other[1]).clamp(0.0, 1.0),
                (local[2] + local_a * other[2]).clamp(0.0, 1.0),
            ],
        }
    }

    #[must_use]
    pub fn index(self) -> u32 {
        match self {
            Self::Disable => 0,
            Self::Detail => 1,
            Self::Scale => 2,
            Self::InvScale => 3,
            Self::Add => 4,
            Self::Sub => 5,
            Self::Subr => 6,
            Self::Blend => 7,
            Self::DetailBlend => 8,
            Self::AddSigned => 9,
            Self::AddSigned2x => 10,
            Self::Scale2x => 11,
            Self::ModAlphaAddColor => 12,
            Self::HouseColor => 13,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DetailAlphaCombine {
    #[default]
    Disable,
    Detail,
    Scale,
    InvScale,
}

impl DetailAlphaCombine {
    #[must_use]
    pub fn combine(self, local: f32, other: f32) -> f32 {
        match self {
            Self::Disable => local,
            Self::Detail => other,
            Self::Scale => local * other,
            Self::InvScale => local + (1.0 - local) * other,
        }
    }

    #[must_use]
    pub fn index(self) -> u32 {
        match self {
            Self::Disable => 0,
            Self::Detail => 1,
            Self::Scale => 2,
            Self::InvScale => 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disable_ignores_the_second_stage_entirely() {
        assert_eq!(
            DetailCombine::Disable.combine_rgb([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 1.0, 1.0),
            [1.0, 0.0, 0.0]
        );
    }

    #[test]
    fn scale_is_ordinary_multiplication() {
        assert_eq!(
            DetailCombine::Scale.combine_rgb([0.5, 0.5, 0.5], [0.5, 1.0, 0.0], 1.0, 1.0),
            [0.25, 0.5, 0.0]
        );
    }

    #[test]
    fn add_sums_every_channel() {
        let result = DetailCombine::Add.combine_rgb([0.2, 0.3, 0.4], [0.1, 0.1, 0.1], 1.0, 0.0);
        let expected = [0.3, 0.4, 0.5];
        for (got, want) in result.iter().zip(expected) {
            assert!((got - want).abs() < 1e-6, "got {result:?}");
        }
    }

    #[test]
    fn detail_blend_weights_by_the_second_stages_own_alpha() {
        let local = [1.0, 0.0, 0.0];
        let other_opaque = [0.0, 1.0, 0.0];
        assert_eq!(
            DetailCombine::DetailBlend.combine_rgb(local, other_opaque, 1.0, 1.0),
            local
        );
        assert_eq!(
            DetailCombine::DetailBlend.combine_rgb(local, other_opaque, 1.0, 0.0),
            other_opaque
        );
    }

    #[test]
    fn every_color_variant_has_a_distinct_index() {
        let all = [
            DetailCombine::Disable,
            DetailCombine::Detail,
            DetailCombine::Scale,
            DetailCombine::InvScale,
            DetailCombine::Add,
            DetailCombine::Sub,
            DetailCombine::Subr,
            DetailCombine::Blend,
            DetailCombine::DetailBlend,
            DetailCombine::AddSigned,
            DetailCombine::AddSigned2x,
            DetailCombine::Scale2x,
            DetailCombine::ModAlphaAddColor,
            DetailCombine::HouseColor,
        ];
        let mut indices: Vec<u32> = all.iter().map(|d| d.index()).collect();
        indices.sort_unstable();
        indices.dedup();
        assert_eq!(indices.len(), all.len());
    }

    #[test]
    fn scale2x_is_double_a_plain_scale() {
        let local = [0.3, 0.4, 1.0];
        let other = [0.5, 0.5, 0.5];
        assert_eq!(
            DetailCombine::Scale2x.combine_rgb(local, other, 1.0, 1.0),
            [0.3, 0.4, 1.0],
            "0.3*0.5*2 = 0.3, 0.4*0.5*2 = 0.4, clamped at 1.0*0.5*2 = 1.0 -> 1.0"
        );
        assert_eq!(
            DetailCombine::Scale.combine_rgb(local, other, 1.0, 1.0),
            [0.15, 0.2, 0.5],
            "the un-doubled formula this byte was wrongly mapped to before this fix"
        );
    }

    #[test]
    fn add_signed_centres_on_zero() {
        let mid = [0.5, 0.5, 0.5];
        let result = DetailCombine::AddSigned.combine_rgb(mid, mid, 1.0, 1.0);
        for c in result {
            assert!((c - 0.5).abs() < 1e-6, "got {result:?}");
        }
    }

    #[test]
    fn mod_alpha_add_color_gates_the_addend_by_locals_own_alpha() {
        let local = [0.2, 0.2, 0.2];
        let other = [1.0, 1.0, 1.0];
        assert_eq!(
            DetailCombine::ModAlphaAddColor.combine_rgb(local, other, 0.0, 1.0),
            local,
            "local_a = 0 must add nothing of other"
        );
        assert_eq!(
            DetailCombine::ModAlphaAddColor.combine_rgb(local, other, 1.0, 1.0),
            [1.0, 1.0, 1.0],
            "local_a = 1 adds other in full, clamped"
        );
    }

    #[test]
    fn a_coverage_mask_is_not_erased_by_an_opaque_second_stage() {
        let masked_out = 0.02;
        let opaque_other = 1.0;
        assert_eq!(
            DetailAlphaCombine::Disable.combine(masked_out, opaque_other),
            masked_out
        );
    }

    #[test]
    fn every_alpha_variant_has_a_distinct_index() {
        let all = [
            DetailAlphaCombine::Disable,
            DetailAlphaCombine::Detail,
            DetailAlphaCombine::Scale,
            DetailAlphaCombine::InvScale,
        ];
        let mut indices: Vec<u32> = all.iter().map(|d| d.index()).collect();
        indices.sort_unstable();
        indices.dedup();
        assert_eq!(indices.len(), all.len());
    }
}
