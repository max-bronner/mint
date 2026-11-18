use mint_sage_ini::value::parse_bool;
use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, StateHasher};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ArcFields {
    pub first_height: f32,
    pub second_height: f32,
    pub first_percent_indent: f32,
    pub second_percent_indent: f32,
}

impl ArcFields {
    fn hash_into(self, hasher: &mut StateHasher) {
        hasher.write_f32(self.first_height);
        hasher.write_f32(self.second_height);
        hasher.write_f32(self.first_percent_indent);
        hasher.write_f32(self.second_percent_indent);
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct BezierProjectileBehavior {
    pub arc: ArcFields,
    pub percent_heights: (Option<f32>, Option<f32>),
    pub tumble_randomly: bool,
    pub detonate_calls_kill: bool,
    pub orient_to_flight_path: bool,
    pub flight_path_adjust_dist_per_second: f32,
    pub curve_flatten_min_dist: f32,
    pub ignore_terrain_height: bool,
    pub die_on_impact: bool,
    pub crush_style: bool,
    pub ground_hit_fx: Option<String>,
    pub ground_hit_weapon: Option<String>,
    pub ground_bounce_fx: Option<String>,
    pub ground_bounce_weapon: Option<String>,
    pub bounce_count: u32,
    pub bounce_distance: f32,
    pub bounce_arc: ArcFields,
    pub garrison_hit_kill_required_kind_of: Vec<String>,
    pub garrison_hit_kill_forbidden_kind_of: Vec<String>,
    pub garrison_hit_kill_count: u32,
    pub garrison_hit_kill_fx: Option<String>,
    pub pre_landing_emotion: Option<String>,
    pub pre_landing_emotion_radius: Option<f32>,
    pub pre_landing_state_time: Option<f32>,
    pub post_landing_emotion: Option<String>,
    pub post_landing_emotion_radius: Option<f32>,
    pub post_landing_state_time: Option<f32>,
    pub invisible_frames: Option<f32>,
    pub fade_in_time: Option<f32>,
    pub final_stuck_time: Option<f32>,
    pub max_lifespan: Option<f32>,
}

impl Component for BezierProjectileBehavior {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.arc.hash_into(hasher);
        self.bounce_arc.hash_into(hasher);
        hasher.write_bool(self.tumble_randomly);
        hasher.write_bool(self.detonate_calls_kill);
        hasher.write_bool(self.orient_to_flight_path);
        hasher.write_bool(self.ignore_terrain_height);
        hasher.write_bool(self.die_on_impact);
        hasher.write_bool(self.crush_style);
        hasher.write_f32(self.flight_path_adjust_dist_per_second);
        hasher.write_f32(self.curve_flatten_min_dist);
        hasher.write_u32(self.bounce_count);
        hasher.write_f32(self.bounce_distance);
        hasher.write_u32(self.garrison_hit_kill_count);
        for name in [
            self.ground_hit_fx.as_deref(),
            self.ground_hit_weapon.as_deref(),
            self.ground_bounce_fx.as_deref(),
            self.ground_bounce_weapon.as_deref(),
            self.garrison_hit_kill_fx.as_deref(),
            self.pre_landing_emotion.as_deref(),
        ] {
            hasher.write_bool(name.is_some());
            hasher.write_bytes(name.unwrap_or("").as_bytes());
        }
    }
}

impl ModuleKind for BezierProjectileBehavior {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn bezier_projectile_behavior_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<BezierProjectileBehavior, FieldError> {
    Ok(BezierProjectileBehavior {
        arc: ArcFields {
            first_height: numeric(fields, "FirstHeight")?.unwrap_or(0.0),
            second_height: numeric(fields, "SecondHeight")?.unwrap_or(0.0),
            first_percent_indent: percent(fields, "FirstPercentIndent")?.unwrap_or(0.0),
            second_percent_indent: percent(fields, "SecondPercentIndent")?.unwrap_or(0.0),
        },
        percent_heights: (
            percent(fields, "FirstPercentHeight")?,
            percent(fields, "SecondPercentHeight")?,
        ),
        tumble_randomly: boolean(fields, "TumbleRandomly")?.unwrap_or(false),
        detonate_calls_kill: boolean(fields, "DetonateCallsKill")?.unwrap_or(false),
        orient_to_flight_path: boolean(fields, "OrientToFlightPath")?.unwrap_or(true),
        flight_path_adjust_dist_per_second: numeric(fields, "FlightPathAdjustDistPerSecond")?
            .unwrap_or(0.0),
        curve_flatten_min_dist: numeric(fields, "CurveFlattenMinDist")?.unwrap_or(0.0),
        ignore_terrain_height: boolean(fields, "IgnoreTerrainHeight")?.unwrap_or(false),
        die_on_impact: boolean(fields, "DieOnImpact")?.unwrap_or(false),
        crush_style: boolean(fields, "CrushStyle")?.unwrap_or(false),
        ground_hit_fx: text(fields, "GroundHitFX"),
        ground_hit_weapon: text(fields, "GroundHitWeapon"),
        ground_bounce_fx: text(fields, "GroundBounceFX"),
        ground_bounce_weapon: text(fields, "GroundBounceWeapon"),
        bounce_count: count(fields, "BounceCount")?,
        bounce_distance: numeric(fields, "BounceDistance")?.unwrap_or(0.0),
        bounce_arc: ArcFields {
            first_height: numeric(fields, "BounceFirstHeight")?.unwrap_or(0.0),
            second_height: numeric(fields, "BounceSecondHeight")?.unwrap_or(0.0),
            first_percent_indent: percent(fields, "BounceFirstPercentIndent")?.unwrap_or(0.0),
            second_percent_indent: percent(fields, "BounceSecondPercentIndent")?.unwrap_or(0.0),
        },
        garrison_hit_kill_required_kind_of: tokens(fields, "GarrisonHitKillRequiredKindOf"),
        garrison_hit_kill_forbidden_kind_of: tokens(fields, "GarrisonHitKillForbiddenKindOf"),
        garrison_hit_kill_count: count(fields, "GarrisonHitKillCount")?,
        garrison_hit_kill_fx: text(fields, "GarrisonHitKillFX"),
        pre_landing_emotion: text(fields, "PreLandingEmotion"),
        pre_landing_emotion_radius: numeric(fields, "PreLandingEmotionRadius")?,
        pre_landing_state_time: numeric(fields, "PreLandingStateTime")?,
        post_landing_emotion: text(fields, "PostLandingEmotion"),
        post_landing_emotion_radius: numeric(fields, "PostLandingEmotionRadius")?,
        post_landing_state_time: numeric(fields, "PostLandingStateTime")?,
        invisible_frames: numeric(fields, "InvisibleFrames")?,
        fade_in_time: numeric(fields, "FadeInTime")?,
        final_stuck_time: numeric(fields, "FinalStuckTime")?,
        max_lifespan: numeric(fields, "MaxLifespan")?,
    })
}

fn numeric(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<Option<f32>, FieldError> {
    match fields.field(keyword) {
        None => Ok(None),
        Some(_) => fields.field_numeric(keyword).map(Some).ok_or_else(|| {
            FieldError::new(format!("{keyword} is present but not a number/known macro"))
        }),
    }
}

fn percent(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<Option<f32>, FieldError> {
    match fields.field(keyword) {
        None => Ok(None),
        Some(raw) => fields.field_percent(keyword).map(Some).ok_or_else(|| {
            FieldError::new(format!("{keyword} '{raw}' is not a percentage or number"))
        }),
    }
}

fn boolean(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<Option<bool>, FieldError> {
    match fields.field(keyword) {
        None => Ok(None),
        Some(raw) => parse_bool(raw)
            .map(Some)
            .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not Yes/No"))),
    }
}

fn count(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<u32, FieldError> {
    let Some(value) = numeric(fields, keyword)? else {
        return Ok(0);
    };
    if value < 0.0 || value > f32::from(u16::MAX) {
        return Err(FieldError::new(format!(
            "{keyword} '{value}' is not a count"
        )));
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = value as u32;
    Ok(count)
}

fn text(fields: &dyn ModuleFieldSource, keyword: &str) -> Option<String> {
    fields
        .field(keyword)
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("None"))
        .map(ToOwned::to_owned)
}

fn tokens(fields: &dyn ModuleFieldSource, keyword: &str) -> Vec<String> {
    fields
        .field_args(keyword)
        .into_iter()
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct TestFields(BTreeMap<&'static str, &'static str>);

    impl ModuleFieldSource for TestFields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0
                .iter()
                .find(|(held, _)| held.eq_ignore_ascii_case(keyword))
                .map(|(_, value)| *value)
        }
    }

    fn read(
        pairs: &[(&'static str, &'static str)],
    ) -> Result<BezierProjectileBehavior, FieldError> {
        let fields = TestFields(pairs.iter().copied().collect());
        bezier_projectile_behavior_from_fields(&fields, Frame::ZERO)
    }

    #[test]
    fn each_field_shape_reads_as_the_retail_table_writes_it() {
        let read = read(&[
            ("FirstPercentIndent", "30%"),
            ("FirstHeight", "24"),
            ("TumbleRandomly", "Yes"),
            ("BounceCount", "1"),
            ("GroundHitFX", "FX_Anything"),
        ])
        .unwrap();
        assert!((read.arc.first_percent_indent - 0.3).abs() < 1e-6);
        assert!((read.arc.first_height - 24.0).abs() < 1e-6);
        assert!(read.tumble_randomly);
        assert_eq!(read.bounce_count, 1);
        assert_eq!(read.ground_hit_fx.as_deref(), Some("FX_Anything"));
        assert!(read.orient_to_flight_path);
    }

    #[test]
    fn a_wrong_typed_value_is_refused_rather_than_defaulted() {
        for pair in [
            ("FirstHeight", "soon"),
            ("FirstPercentIndent", "half"),
            ("DieOnImpact", "Maybe"),
            ("BounceCount", "-3"),
        ] {
            assert!(read(&[pair]).is_err(), "{pair:?} was accepted");
        }
        assert!(read(&[]).is_ok());
    }
}
