#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WaveRung {
    Off,
    Breakers,
    Swell,
    #[default]
    Both,
    BothTall,
}

impl WaveRung {
    pub const ALL: [Self; 5] = [
        Self::Off,
        Self::Breakers,
        Self::Swell,
        Self::Both,
        Self::BothTall,
    ];

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Breakers,
            Self::Breakers => Self::Swell,
            Self::Swell => Self::Both,
            Self::Both => Self::BothTall,
            Self::BothTall => Self::Off,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Breakers => "breakers",
            Self::Swell => "swell",
            Self::Both => "swell+breakers",
            Self::BothTall => "tall-swell+breakers",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim().to_ascii_lowercase();
        Self::ALL.into_iter().find(|rung| rung.label() == text)
    }

    #[must_use]
    pub fn ordinal(self) -> usize {
        Self::ALL
            .iter()
            .position(|rung| *rung == self)
            .map_or(1, |i| i.saturating_add(1))
    }

    #[must_use]
    pub fn swell(self) -> bool {
        matches!(self, Self::Swell | Self::Both | Self::BothTall)
    }

    #[must_use]
    pub fn swell_gain(self) -> f32 {
        if self == Self::BothTall {
            4.0
        } else {
            1.0
        }
    }

    #[must_use]
    pub fn sprite(self, pca: bool) -> bool {
        match self {
            Self::Off => false,
            Self::Swell => !pca,
            Self::Breakers | Self::Both | Self::BothTall => true,
        }
    }
}
