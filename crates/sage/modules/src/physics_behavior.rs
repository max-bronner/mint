use mint_sage_ini::value::parse_bool;
use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, StateHasher};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct PhysicsBehavior {
    gravity_mult: Option<f32>,
    shock_standing_time: Option<f32>,
    shock_stunned_time_low: Option<f32>,
    shock_stunned_time_high: Option<f32>,
    kill_when_resting_on_ground: bool,
    allow_bouncing: bool,
    arc: ArcFields,
    ground_hit_fx: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct ArcFields {
    first_height: Option<f32>,
    second_height: Option<f32>,
    first_percent_indent: Option<f32>,
    second_percent_indent: Option<f32>,
    ignore_terrain_height: bool,
    first_percent_height: Option<f32>,
    second_percent_height: Option<f32>,
    curve_flatten_min_dist: Option<f32>,
}

impl PhysicsBehavior {
    #[must_use]
    pub fn fling_arc(&self) -> mint_sage_combat::BezierArc {
        let arc = self.arc;
        mint_sage_combat::BezierArc {
            first_height: arc.first_height.unwrap_or(1.3),
            second_height: arc.second_height.unwrap_or(1.3),
            first_percent_indent: arc.first_percent_indent.unwrap_or(0.33),
            second_percent_indent: arc.second_percent_indent.unwrap_or(0.66),
            ignore_terrain_height: arc.ignore_terrain_height,
            first_percent_height: arc.first_percent_height.unwrap_or(0.33),
            second_percent_height: arc.second_percent_height.unwrap_or(0.66),
            curve_flatten_min_dist: arc.curve_flatten_min_dist.unwrap_or(0.0),
        }
    }

    #[must_use]
    pub fn ground_hit_fx(&self) -> Option<&str> {
        self.ground_hit_fx.as_deref()
    }

    #[must_use]
    pub const fn gravity_mult(&self) -> Option<f32> {
        self.gravity_mult
    }

    #[must_use]
    pub const fn shock_standing_time(&self) -> Option<f32> {
        self.shock_standing_time
    }

    #[must_use]
    pub const fn shock_stunned_time_low(&self) -> Option<f32> {
        self.shock_stunned_time_low
    }

    #[must_use]
    pub const fn shock_stunned_time_high(&self) -> Option<f32> {
        self.shock_stunned_time_high
    }

    #[must_use]
    pub const fn kill_when_resting_on_ground(&self) -> bool {
        self.kill_when_resting_on_ground
    }

    #[must_use]
    pub const fn allow_bouncing(&self) -> bool {
        self.allow_bouncing
    }
}

impl Component for PhysicsBehavior {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hash_optional_f32(hasher, self.gravity_mult);
        hash_optional_f32(hasher, self.shock_standing_time);
        hash_optional_f32(hasher, self.shock_stunned_time_low);
        hash_optional_f32(hasher, self.shock_stunned_time_high);
        hasher.write_bool(self.kill_when_resting_on_ground);
        hasher.write_bool(self.allow_bouncing);
        let arc = self.arc;
        for value in [
            arc.first_height,
            arc.second_height,
            arc.first_percent_indent,
            arc.second_percent_indent,
            arc.first_percent_height,
            arc.second_percent_height,
            arc.curve_flatten_min_dist,
        ] {
            hash_optional_f32(hasher, value);
        }
        hasher.write_bool(arc.ignore_terrain_height);
        hasher.write_bytes(self.ground_hit_fx.as_deref().unwrap_or("").as_bytes());
    }
}

fn hash_optional_f32(hasher: &mut StateHasher, value: Option<f32>) {
    hasher.write_bool(value.is_some());
    hasher.write_f32(value.unwrap_or(0.0));
}

impl ModuleKind for PhysicsBehavior {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn physics_behavior_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<PhysicsBehavior, FieldError> {
    let gravity_mult = optional_numeric(fields, "GravityMult")?;
    let shock_standing_time = optional_numeric(fields, "ShockStandingTime")?;
    let shock_stunned_time_low = optional_numeric(fields, "ShockStunnedTimeLow")?;
    let shock_stunned_time_high = optional_numeric(fields, "ShockStunnedTimeHigh")?;
    let kill_when_resting_on_ground = optional_bool(fields, "KillWhenRestingOnGround")?;
    let allow_bouncing = optional_bool(fields, "AllowBouncing")?;
    let arc = ArcFields {
        first_height: optional_numeric(fields, "FirstHeight")?,
        second_height: optional_numeric(fields, "SecondHeight")?,
        first_percent_indent: optional_percent(fields, "FirstPercentIndent")?,
        second_percent_indent: optional_percent(fields, "SecondPercentIndent")?,
        ignore_terrain_height: optional_bool(fields, "IgnoreTerrainHeight")?.unwrap_or(false),
        first_percent_height: optional_percent(fields, "FirstPercentHeight")?,
        second_percent_height: optional_percent(fields, "SecondPercentHeight")?,
        curve_flatten_min_dist: optional_numeric(fields, "CurveFlattenMinDist")?,
    };
    let ground_hit_fx = fields.field("GroundHitFX").map(str::to_owned);
    Ok(PhysicsBehavior {
        gravity_mult,
        shock_standing_time,
        shock_stunned_time_low,
        shock_stunned_time_high,
        kill_when_resting_on_ground: kill_when_resting_on_ground.unwrap_or(false),
        allow_bouncing: allow_bouncing.unwrap_or(false),
        arc,
        ground_hit_fx,
    })
}

fn optional_percent(
    fields: &dyn ModuleFieldSource,
    keyword: &str,
) -> Result<Option<f32>, FieldError> {
    match fields.field(keyword) {
        None => Ok(None),
        Some(raw) => fields.field_percent(keyword).map(Some).ok_or_else(|| {
            FieldError::new(format!("{keyword} '{raw}' is not a percentage or number"))
        }),
    }
}

fn optional_numeric(
    fields: &dyn ModuleFieldSource,
    keyword: &str,
) -> Result<Option<f32>, FieldError> {
    match fields.field(keyword) {
        None => Ok(None),
        Some(_) => match fields.field_numeric(keyword) {
            Some(value) => Ok(Some(value)),
            None => Err(FieldError::new(format!(
                "{keyword} is present but not a number/known macro"
            ))),
        },
    }
}

fn optional_bool(
    fields: &dyn ModuleFieldSource,
    keyword: &str,
) -> Result<Option<bool>, FieldError> {
    match fields.field(keyword) {
        None => Ok(None),
        Some(raw) => parse_bool(raw)
            .map(Some)
            .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not Yes/No"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct TestFields(BTreeMap<&'static str, &'static str>);
    impl ModuleFieldSource for TestFields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .map(|(_, v)| *v)
        }
    }

    #[test]
    fn no_fields_at_all_succeeds_with_every_optional_field_absent() {
        let fields = TestFields(BTreeMap::new());
        let physics = physics_behavior_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(physics.gravity_mult(), None);
        assert!(!physics.allow_bouncing());
        assert!(!physics.kill_when_resting_on_ground());
    }

    #[test]
    fn declared_fields_are_read() {
        let fields = TestFields(BTreeMap::from([
            ("GravityMult", "1.5"),
            ("ShockStandingTime", "500"),
            ("AllowBouncing", "Yes"),
            ("KillWhenRestingOnGround", "No"),
        ]));
        let physics = physics_behavior_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(physics.gravity_mult(), Some(1.5));
        assert_eq!(physics.shock_standing_time(), Some(500.0));
        assert!(physics.allow_bouncing());
        assert!(!physics.kill_when_resting_on_ground());
    }

    #[test]
    fn a_present_but_unparseable_field_is_refused_rather_than_ignored() {
        let fields = TestFields(BTreeMap::from([("GravityMult", "not a number")]));
        assert!(physics_behavior_from_fields(&fields, Frame::ZERO).is_err());

        let fields = TestFields(BTreeMap::from([("AllowBouncing", "Maybe")]));
        assert!(physics_behavior_from_fields(&fields, Frame::ZERO).is_err());
    }

    #[test]
    fn absence_and_an_explicit_zero_hash_differently() {
        let mut absent = StateHasher::new();
        hash_optional_f32(&mut absent, None);
        let mut zero = StateHasher::new();
        hash_optional_f32(&mut zero, Some(0.0));
        assert_ne!(absent, zero);
    }
}
