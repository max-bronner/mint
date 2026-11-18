#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Blend {
    #[default]
    Opaque,
    AlphaBlend,
    Additive,
    AlphaAdditive,
    AlphaBlendWritesDepth,
    AddScaledByAlpha,
    AddScaledByColor,
    AddInverseScaledByColor,
    Multiply,
    MergeMask,
    MergeAlphaBlend,
}

impl Blend {
    #[must_use]
    pub fn merges(self) -> bool {
        matches!(self, Self::MergeMask | Self::MergeAlphaBlend)
    }

    #[must_use]
    pub fn writes_depth(self) -> bool {
        matches!(self, Self::Opaque | Self::AlphaBlendWritesDepth)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_opaque_writes_depth() {
        assert!(Blend::Opaque.writes_depth());
        assert!(Blend::AlphaBlendWritesDepth.writes_depth());
        assert!(!Blend::AlphaBlend.writes_depth());
        assert!(!Blend::Additive.writes_depth());
        assert!(!Blend::AlphaAdditive.writes_depth());
        assert!(!Blend::AddScaledByAlpha.writes_depth());
        assert!(!Blend::AddScaledByColor.writes_depth());
        assert!(!Blend::AddInverseScaledByColor.writes_depth());
        assert!(!Blend::Multiply.writes_depth());
        assert!(!Blend::MergeMask.writes_depth());
        assert!(!Blend::MergeAlphaBlend.writes_depth());
    }

    #[test]
    fn opaque_is_the_default() {
        assert_eq!(Blend::default(), Blend::Opaque);
    }
}
