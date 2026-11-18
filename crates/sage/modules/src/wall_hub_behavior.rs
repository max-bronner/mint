use std::sync::Arc;

use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, StateHasher};

const MAX_DISTANCE: f32 = 100_000.0;

pub const BUILDER_RADIUS_USES_HUB: f32 = 9.876;

const DEFAULT_STAGGERED_BUILD_FACTOR: f32 = 5.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModuleOption(u8);

impl ModuleOption {
    pub const FIRST: Self = Self(0);

    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        match token.trim().to_ascii_uppercase().as_str() {
            "OPTION_ONE" => Some(Self(0)),
            "OPTION_TWO" => Some(Self(1)),
            "OPTION_THREE" => Some(Self(2)),
            _ => None,
        }
    }

    #[must_use]
    pub const fn index(self) -> u8 {
        self.0
    }
}

#[derive(Debug, PartialEq)]
struct Data {
    option: ModuleOption,
    max_buildout_distance: f32,
    staggered_build_factor: f32,
    builder_radius: f32,
    segments: Vec<String>,
    default_segment: Option<String>,
    hub_cap: Option<String>,
    cliff_cap: Option<String>,
    shore_cap: Option<String>,
    border_cap: Option<String>,
    elevated_segment: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WallHubBehavior {
    data: Arc<Data>,
}

impl WallHubBehavior {
    #[must_use]
    pub fn option(&self) -> ModuleOption {
        self.data.option
    }

    #[must_use]
    pub fn max_buildout_distance(&self) -> f32 {
        self.data.max_buildout_distance
    }

    #[must_use]
    pub fn staggered_build_factor(&self) -> f32 {
        self.data.staggered_build_factor
    }

    #[must_use]
    pub fn builder_radius(&self) -> f32 {
        self.data.builder_radius
    }

    #[must_use]
    pub fn segments(&self) -> &[String] {
        &self.data.segments
    }

    #[must_use]
    pub fn default_segment(&self) -> Option<&str> {
        self.data.default_segment.as_deref()
    }

    #[must_use]
    pub fn hub_cap(&self) -> Option<&str> {
        self.data.hub_cap.as_deref()
    }

    #[must_use]
    pub fn cliff_cap(&self) -> Option<&str> {
        self.data.cliff_cap.as_deref()
    }

    #[must_use]
    pub fn shore_cap(&self) -> Option<&str> {
        self.data.shore_cap.as_deref()
    }

    #[must_use]
    pub fn border_cap(&self) -> Option<&str> {
        self.data.border_cap.as_deref()
    }

    #[must_use]
    pub fn elevated_segment(&self) -> Option<&str> {
        self.data.elevated_segment.as_deref()
    }

    #[must_use]
    pub fn segment_at(&self, index: usize) -> Option<&str> {
        let len = self.data.segments.len();
        if len == 0 {
            return self.default_segment();
        }
        self.data
            .segments
            .get(index.checked_rem(len).unwrap_or(0))
            .map(String::as_str)
            .or_else(|| self.default_segment())
    }
}

impl Component for WallHubBehavior {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for WallHubBehavior {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

fn distance(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<f32, FieldError> {
    distance_or(fields, keyword, 0.0)
}

fn distance_or(
    fields: &dyn ModuleFieldSource,
    keyword: &str,
    default: f32,
) -> Result<f32, FieldError> {
    let Some(raw) = fields.field(keyword) else {
        return Ok(default);
    };
    fields
        .field_numeric(keyword)
        .filter(|value| (0.0..=MAX_DISTANCE).contains(value))
        .ok_or_else(|| {
            FieldError::new(format!(
                "{keyword} '{raw}' is not a number from 0 to {MAX_DISTANCE}"
            ))
        })
}

fn option(fields: &dyn ModuleFieldSource) -> Result<ModuleOption, FieldError> {
    let Some(raw) = fields.field("Options") else {
        return Ok(ModuleOption::FIRST);
    };
    ModuleOption::parse(raw)
        .ok_or_else(|| FieldError::new(format!("Options '{raw}' is not OPTION_ONE/TWO/THREE")))
}

pub fn wall_hub_behavior_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<WallHubBehavior, FieldError> {
    let segments = fields
        .field_args_each("SegmentTemplateName")
        .into_iter()
        .filter_map(|args| args.first().map(|name| (*name).to_owned()))
        .collect();
    let data = Data {
        option: option(fields)?,
        max_buildout_distance: distance(fields, "MaxBuildoutDistance")?,
        staggered_build_factor: distance_or(
            fields,
            "StaggeredBuildFactor",
            DEFAULT_STAGGERED_BUILD_FACTOR,
        )?,
        builder_radius: distance_or(fields, "BuilderRadius", BUILDER_RADIUS_USES_HUB)?,
        segments,
        default_segment: fields
            .field("DefaultSegmentTemplateName")
            .map(str::to_owned),
        hub_cap: fields.field("HubCapTemplateName").map(str::to_owned),
        cliff_cap: fields.field("CliffCapTemplateName").map(str::to_owned),
        shore_cap: fields.field("ShoreCapTemplateName").map(str::to_owned),
        border_cap: fields.field("BorderCapTemplateName").map(str::to_owned),
        elevated_segment: fields
            .field("ElevatedSegmentTemplateName")
            .map(str::to_owned),
    };
    Ok(WallHubBehavior {
        data: Arc::new(data),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct TestFields(BTreeMap<&'static str, Vec<&'static str>>);
    impl ModuleFieldSource for TestFields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.field_args(keyword).first().copied()
        }
        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.0
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .map(|(_, v)| vec![v[0]])
                .unwrap_or_default()
        }
        fn field_args_each(&self, keyword: &str) -> Vec<Vec<&str>> {
            self.0
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .map(|(_, v)| v.iter().map(|s| vec![*s]).collect())
                .unwrap_or_default()
        }
    }

    fn fields(pairs: &[(&'static str, &[&'static str])]) -> TestFields {
        TestFields(pairs.iter().map(|(k, v)| (*k, v.to_vec())).collect())
    }

    #[test]
    fn the_segment_pattern_is_ordered_and_restarts() {
        let module = wall_hub_behavior_from_fields(
            &fields(&[
                ("Options", &["OPTION_TWO"]),
                ("MaxBuildoutDistance", &["900"]),
                ("StaggeredBuildFactor", &["20"]),
                ("SegmentTemplateName", &["ModPlank", "ModPost", "ModPlank"]),
                ("HubCapTemplateName", &["ModTower"]),
                ("DefaultSegmentTemplateName", &["ModPlank"]),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert_eq!(module.option(), ModuleOption::parse("OPTION_TWO").unwrap());
        assert_eq!(module.segments().len(), 3);
        assert_eq!(module.segment_at(0), Some("ModPlank"));
        assert_eq!(module.segment_at(1), Some("ModPost"));
        assert_eq!(module.segment_at(3), Some("ModPlank"));
        assert_eq!(module.segment_at(4), Some("ModPost"));
        assert_eq!(module.hub_cap(), Some("ModTower"));
        assert_eq!(module.max_buildout_distance(), 900.0);
    }

    #[test]
    fn a_hub_with_no_pattern_lays_its_default_segment() {
        let module = wall_hub_behavior_from_fields(
            &fields(&[("DefaultSegmentTemplateName", &["ModPlank"])]),
            Frame::ZERO,
        )
        .unwrap();
        assert_eq!(module.segment_at(0), Some("ModPlank"));
        assert_eq!(module.segment_at(7), Some("ModPlank"));
        assert_eq!(module.option(), ModuleOption::FIRST);
    }

    #[test]
    fn an_option_token_the_engine_does_not_know_is_refused() {
        assert!(wall_hub_behavior_from_fields(
            &fields(&[("Options", &["OPTION_FOUR"])]),
            Frame::ZERO
        )
        .is_err());
    }

    #[test]
    fn a_buildout_distance_that_is_not_a_number_is_refused() {
        assert!(wall_hub_behavior_from_fields(
            &fields(&[("MaxBuildoutDistance", &["far"])]),
            Frame::ZERO
        )
        .is_err());
    }
}
