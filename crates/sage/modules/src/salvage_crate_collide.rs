use std::sync::Arc;

use mint_sage_ini::value::{parse_bool, parse_percent};
use mint_sim::module::{Capability, CapabilityMask};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, ModuleKind, StateHasher};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SalvageCrateData {
    pub required_kind_of: Vec<String>,
    pub forbidden_kind_of: Vec<String>,
    pub forbid_owner_player: bool,
    pub building_pickup: bool,
    pub human_only: bool,
    pub pickup_science: Option<String>,
    pub execute_fx: Option<String>,
    pub level_up_chance: f32,
    pub upgrade: Option<String>,
    pub resource: (i32, i32),
    pub allow_ai_pickup: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SalvageCrateCollide {
    data: Arc<SalvageCrateData>,
}

impl SalvageCrateCollide {
    #[must_use]
    pub fn data(&self) -> &SalvageCrateData {
        &self.data
    }
}

impl Component for SalvageCrateCollide {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for SalvageCrateCollide {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Collide);
}

fn flag(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<Option<bool>, FieldError> {
    fields
        .field(keyword)
        .map(|raw| {
            parse_bool(raw)
                .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not Yes/No")))
        })
        .transpose()
}

fn percent(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<Option<f32>, FieldError> {
    fields
        .field(keyword)
        .map(|raw| {
            parse_percent(raw)
                .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not a percentage")))
        })
        .transpose()
}

fn whole(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<Option<i32>, FieldError> {
    let Some(raw) = fields.field(keyword) else {
        return Ok(None);
    };
    match fields.field_numeric(keyword) {
        Some(value) if value.fract() == 0.0 && value.abs() <= 1_000_000_000.0 => {
            #[allow(clippy::cast_possible_truncation)]
            let value = value as i32;
            Ok(Some(value))
        }
        _ => Err(FieldError::new(format!(
            "{keyword} '{raw}' is not a whole number"
        ))),
    }
}

fn real(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<(), FieldError> {
    match fields.field(keyword) {
        Some(raw) if fields.field_numeric(keyword).is_none() => Err(FieldError::new(format!(
            "{keyword} '{raw}' is not a number"
        ))),
        _ => Ok(()),
    }
}

fn name(fields: &dyn ModuleFieldSource, keyword: &str) -> Option<String> {
    fields
        .field(keyword)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn names(fields: &dyn ModuleFieldSource, keyword: &str) -> Vec<String> {
    fields
        .field_args(keyword)
        .into_iter()
        .map(str::to_owned)
        .collect()
}

pub fn salvage_crate_collide_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<SalvageCrateCollide, FieldError> {
    for keyword in [
        "PorterChance",
        "BannerChance",
        "LevelUpRadius",
        "ResourceChance",
    ] {
        percent(fields, keyword)?;
    }
    real(fields, "ExecuteAnimationTime")?;
    real(fields, "ExecuteAnimationZRise")?;
    flag(fields, "ExecuteAnimationFades")?;
    let min = whole(fields, "MinResource")?.unwrap_or(0);
    let max = whole(fields, "MaxResource")?.unwrap_or(0);
    Ok(SalvageCrateCollide {
        data: Arc::new(SalvageCrateData {
            required_kind_of: names(fields, "RequiredKindOf"),
            forbidden_kind_of: names(fields, "ForbiddenKindOf"),
            forbid_owner_player: flag(fields, "ForbidOwnerPlayer")?.unwrap_or(false),
            building_pickup: flag(fields, "BuildingPickup")?.unwrap_or(false),
            human_only: flag(fields, "HumanOnly")?.unwrap_or(false),
            pickup_science: name(fields, "PickupScience"),
            execute_fx: name(fields, "ExecuteFX"),
            level_up_chance: percent(fields, "LevelUpChance")?.unwrap_or(0.0),
            upgrade: name(fields, "Upgrade"),
            resource: (min, max),
            allow_ai_pickup: flag(fields, "AllowAIPickup")?.unwrap_or(false),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct Fields(BTreeMap<&'static str, &'static str>);

    impl ModuleFieldSource for Fields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0.get(keyword).copied()
        }
        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.field(keyword)
                .map(|value| value.split_whitespace().collect())
                .unwrap_or_default()
        }
    }

    fn read(pairs: &[(&'static str, &'static str)]) -> Result<SalvageCrateCollide, FieldError> {
        salvage_crate_collide_from_fields(&Fields(pairs.iter().copied().collect()), Frame::ZERO)
    }

    #[test]
    fn reads_the_reward_and_the_pickup_rules() {
        let crate_ = read(&[
            ("ForbiddenKindOf", "PROJECTILE GHOST"),
            ("LevelUpChance", "40%"),
            ("ResourceChance", "60%"),
            ("MinResource", "7"),
            ("MaxResource", "9"),
            ("AllowAIPickup", "Yes"),
            ("ExecuteFX", "FX_Sparkle"),
        ])
        .unwrap();
        let data = crate_.data();
        assert_eq!(data.forbidden_kind_of, ["PROJECTILE", "GHOST"]);
        assert!((data.level_up_chance - 0.4).abs() < 1e-6);
        assert_eq!(data.resource, (7, 9));
        assert!(data.allow_ai_pickup);
        assert_eq!(data.execute_fx.as_deref(), Some("FX_Sparkle"));
        assert!(data.upgrade.is_none());
    }

    #[test]
    fn a_malformed_field_is_refused() {
        assert!(read(&[("MinResource", "lots")]).is_err());
        assert!(read(&[("BannerChance", "often")]).is_err());
        assert!(read(&[("AllowAIPickup", "maybe")]).is_err());
    }
}
