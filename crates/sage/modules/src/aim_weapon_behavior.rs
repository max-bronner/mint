use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, StateHasher};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pitch {
    High,
    Low,
    Straight,
}

impl Pitch {
    #[must_use]
    pub const fn condition(self) -> &'static str {
        match self {
            Self::High => "AIM_HIGH",
            Self::Low => "AIM_LOW",
            Self::Straight => "AIM_STRAIGHT",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Distance {
    Near,
    Far,
}

impl Distance {
    #[must_use]
    pub const fn condition(self) -> &'static str {
        match self {
            Self::Near => "AIM_NEAR",
            Self::Far => "AIM_FAR",
        }
    }

    #[must_use]
    pub const fn other(self) -> &'static str {
        match self {
            Self::Near => "AIM_FAR",
            Self::Far => "AIM_NEAR",
        }
    }
}

pub const ALL: [&str; 5] = ["AIM_HIGH", "AIM_LOW", "AIM_STRAIGHT", "AIM_NEAR", "AIM_FAR"];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AimWeaponBehavior {
    pub high_threshold: Option<f32>,
    pub low_threshold: Option<f32>,
    pub near_distance: Option<f32>,
    pub far_distance: Option<f32>,
}

impl AimWeaponBehavior {
    #[must_use]
    pub fn pitch(&self, slope: f32) -> Pitch {
        if self.high_threshold.is_some_and(|high| slope > high) {
            Pitch::High
        } else if self.low_threshold.is_some_and(|low| slope < low) {
            Pitch::Low
        } else {
            Pitch::Straight
        }
    }

    #[must_use]
    pub fn distance(&self, distance: f32) -> Option<Distance> {
        if self.near_distance.is_some_and(|near| distance < near) {
            Some(Distance::Near)
        } else if self.far_distance.is_some_and(|far| distance > far) {
            Some(Distance::Far)
        } else {
            None
        }
    }
}

impl Component for AimWeaponBehavior {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for AimWeaponBehavior {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn aim_weapon_behavior_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<AimWeaponBehavior, FieldError> {
    let number = |keyword: &str| -> Result<Option<f32>, FieldError> {
        match fields.field(keyword) {
            None => Ok(None),
            Some(raw) => fields
                .field_numeric(keyword)
                .filter(|value| value.is_finite())
                .map(Some)
                .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not a number"))),
        }
    };
    Ok(AimWeaponBehavior {
        high_threshold: number("AimHighThreshold")?,
        low_threshold: number("AimLowThreshold")?,
        near_distance: number("AimNearDistance")?,
        far_distance: number("AimFarDistance")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn archer() -> AimWeaponBehavior {
        AimWeaponBehavior {
            high_threshold: Some(0.15),
            low_threshold: Some(-0.15),
            near_distance: Some(40.0),
            far_distance: None,
        }
    }

    #[test]
    fn the_pitch_and_distance_axes_are_independent_a_near_high_shot_sets_both() {
        let archer = archer();
        assert_eq!(archer.pitch(2.0), Pitch::High);
        assert_eq!(archer.distance(39.0), Some(Distance::Near));
    }

    #[test]
    fn the_pitch_thresholds_are_a_strict_signed_slope_about_the_flat_shot() {
        let archer = archer();
        assert_eq!(archer.pitch(0.16), Pitch::High);
        assert_eq!(archer.pitch(0.15), Pitch::Straight);
        assert_eq!(archer.pitch(-0.16), Pitch::Low);
    }

    #[test]
    fn a_pitch_threshold_the_data_does_not_write_selects_nothing() {
        let steep = AimWeaponBehavior {
            high_threshold: Some(0.15),
            low_threshold: None,
            near_distance: None,
            far_distance: None,
        };
        assert_eq!(steep.pitch(-9.0), Pitch::Straight);
    }

    #[test]
    fn between_the_distance_thresholds_nothing_is_selected() {
        let archer = archer();
        assert_eq!(archer.distance(40.0), None);
    }
}
