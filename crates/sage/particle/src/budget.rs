use mint_sage_ini::{Document, ParticlePriority};

pub const DEFAULT_MAX_PARTICLES: usize = 2_500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DetailLevel {
    #[default]
    Off,
    VeryLow,
    Low,
    Medium,
    High,
    UltraHigh,
}

impl DetailLevel {
    #[must_use]
    pub fn floor(self) -> Option<ParticlePriority> {
        match self {
            Self::Off => None,
            Self::VeryLow => Some(ParticlePriority::VeryLowOrAbove),
            Self::Low => Some(ParticlePriority::LowOrAbove),
            Self::Medium => Some(ParticlePriority::MediumOrAbove),
            Self::High => Some(ParticlePriority::HighOrAbove),
            Self::UltraHigh => Some(ParticlePriority::UltraHighOnly),
        }
    }

    #[must_use]
    pub fn admits(self, priority: ParticlePriority) -> bool {
        match self.floor() {
            None => true,
            Some(floor) => priority >= floor,
        }
    }

    #[must_use]
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::VeryLow,
            Self::VeryLow => Self::Low,
            Self::Low => Self::Medium,
            Self::Medium => Self::High,
            Self::High => Self::UltraHigh,
            Self::UltraHigh => Self::Off,
        }
    }

    #[must_use]
    pub fn static_lod_name(self) -> Option<&'static str> {
        match self {
            Self::Off => None,
            Self::VeryLow => Some("VeryLow"),
            Self::Low => Some("Low"),
            Self::Medium => Some("Medium"),
            Self::High => Some("High"),
            Self::UltraHigh => Some("UltraHigh"),
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::VeryLow => "verylow",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::UltraHigh => "ultrahigh",
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "off" => Self::Off,
            "verylow" => Self::VeryLow,
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            "ultrahigh" => Self::UltraHigh,
            _ => return None,
        })
    }

    #[must_use]
    pub fn retail_max_particles(self) -> Option<usize> {
        match self {
            Self::Off => None,
            Self::VeryLow | Self::Low => Some(500),
            Self::Medium => Some(1_500),
            Self::High | Self::UltraHigh => Some(3_000),
        }
    }
}

#[must_use]
pub fn static_lod_max_particles(documents: &[Document], level: DetailLevel) -> Option<usize> {
    static_lod_field(documents, level, "MaxParticleCount")?
        .parse()
        .ok()
}

#[must_use]
pub fn static_lod_effects(documents: &[Document], level: DetailLevel) -> Option<DetailLevel> {
    DetailLevel::parse(&static_lod_field(documents, level, "EffectsLOD")?.to_ascii_lowercase())
}

fn static_lod_field<'a>(
    documents: &'a [Document],
    level: DetailLevel,
    field: &str,
) -> Option<&'a str> {
    let name = level.static_lod_name()?;
    documents
        .iter()
        .flat_map(Document::roots)
        .filter(|block| {
            block.keyword.eq_ignore_ascii_case("StaticGameLOD")
                && block.name().is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
        .filter_map(|block| block.field(field)?.value())
        .next_back()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    pub max_particles: Option<usize>,
    pub skip_mask: u32,
    pub detail: DetailLevel,
}

impl Default for Budget {
    fn default() -> Self {
        Self {
            max_particles: Some(DEFAULT_MAX_PARTICLES),
            skip_mask: 0,
            detail: DetailLevel::Off,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BudgetReport {
    pub counted: usize,
    pub exempt: usize,
    pub evicted: u64,
    pub refused: u64,
    pub skipped: u64,
    pub lod_refused: u64,
    pub depth_refused: u64,
    pub collisions: u64,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn the_detail_ladder_admits_what_its_names_say() {
        assert!(DetailLevel::VeryLow.admits(ParticlePriority::VeryLowOrAbove));
        assert!(
            !DetailLevel::VeryLow.admits(ParticlePriority::HighOrAbove),
            "a HIGH_OR_ABOVE system must not appear below High detail"
        );
        assert!(
            !DetailLevel::VeryLow.admits(ParticlePriority::LowOrAbove),
            "the VeryLow rung's floor is VERY_LOW_OR_ABOVE and not the rung above it"
        );
        assert!(DetailLevel::Low.admits(ParticlePriority::LowOrAbove));
        assert!(!DetailLevel::Low.admits(ParticlePriority::MediumOrAbove));
        assert!(DetailLevel::Medium.admits(ParticlePriority::MediumOrAbove));
        assert!(!DetailLevel::Medium.admits(ParticlePriority::HighOrAbove));
        assert!(DetailLevel::High.admits(ParticlePriority::HighOrAbove));
        for priority in [
            ParticlePriority::UltraHighOnly,
            ParticlePriority::HighOrAbove,
            ParticlePriority::MediumOrAbove,
            ParticlePriority::LowOrAbove,
            ParticlePriority::VeryLowOrAbove,
        ] {
            let mut admitted = false;
            for rung in [
                DetailLevel::VeryLow,
                DetailLevel::Low,
                DetailLevel::Medium,
                DetailLevel::High,
                DetailLevel::UltraHigh,
            ] {
                let now = rung.admits(priority);
                assert!(
                    !admitted || now,
                    "{} dropped {} that a lower rung admitted",
                    rung.label(),
                    priority.as_str()
                );
                admitted = now;
            }
            assert!(admitted, "{} never appears at all", priority.as_str());
        }
    }

    #[test]
    fn always_render_is_admitted_by_every_rung_including_the_top_one() {
        for rung in [
            DetailLevel::Off,
            DetailLevel::VeryLow,
            DetailLevel::Low,
            DetailLevel::Medium,
            DetailLevel::High,
            DetailLevel::UltraHigh,
        ] {
            assert!(
                rung.admits(ParticlePriority::AlwaysRender),
                "{} dropped an ALWAYS_RENDER system",
                rung.label()
            );
        }
    }

    #[test]
    fn an_unrated_system_survives_the_ladder_it_says_nothing_about() {
        assert!(DetailLevel::VeryLow.admits(ParticlePriority::Unrated));
        assert!(DetailLevel::High.admits(ParticlePriority::Unrated));
        assert!(
            ParticlePriority::Unrated > ParticlePriority::VeryLowOrAbove,
            "an unrated system must not be evicted before one that asked to be kept"
        );
    }

    #[test]
    fn every_rung_round_trips_through_its_label() {
        let mut rung = DetailLevel::Off;
        for _ in 0..6 {
            assert_eq!(DetailLevel::parse(rung.label()), Some(rung));
            rung = rung.next();
        }
        assert_eq!(rung, DetailLevel::Off, "the ladder wraps in six steps");
        assert_eq!(DetailLevel::parse("ultra"), None);
    }

    #[test]
    fn the_retail_ceilings_are_gamelods_own_five() {
        assert_eq!(DetailLevel::Off.retail_max_particles(), None);
        assert_eq!(DetailLevel::VeryLow.retail_max_particles(), Some(500));
        assert_eq!(DetailLevel::Low.retail_max_particles(), Some(500));
        assert_eq!(DetailLevel::Medium.retail_max_particles(), Some(1_500));
        assert_eq!(DetailLevel::High.retail_max_particles(), Some(3_000));
        assert_eq!(DetailLevel::UltraHigh.retail_max_particles(), Some(3_000));
    }

    #[test]
    fn the_ceiling_comes_from_the_active_static_lod_block() {
        use mint_sage_ini::BlockTable;
        let text = "StaticGameLOD = Medium
  EffectsLOD = Low ; quality of effects
  MaxParticleCount = 111
End
                    StaticGameLOD = UltraHigh
  MaxParticleCount = 4321
End
";
        let document = Document::parse("lod.ini", text, &BlockTable::corpus_path()).unwrap();
        let docs = [document];
        assert_eq!(
            static_lod_max_particles(&docs, DetailLevel::UltraHigh),
            Some(4321)
        );
        assert_eq!(
            static_lod_max_particles(&docs, DetailLevel::Medium),
            Some(111)
        );
        assert_eq!(static_lod_max_particles(&docs, DetailLevel::Low), None);
        assert_eq!(static_lod_max_particles(&docs, DetailLevel::Off), None);
        assert_eq!(
            static_lod_effects(&docs, DetailLevel::Medium),
            Some(DetailLevel::Low)
        );
        assert_eq!(static_lod_effects(&docs, DetailLevel::UltraHigh), None);
    }

    #[test]
    fn the_default_budget_is_gamedatas_ceiling_alone() {
        let budget = Budget::default();
        assert_eq!(budget.max_particles, Some(2_500));
        assert_eq!(budget.skip_mask, 0);
        assert_eq!(budget.detail, DetailLevel::Off);
    }
}
