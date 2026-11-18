use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, StateHasher};

use crate::ai_power_types::ai_power_registry;
use crate::fire_weapon_update::flag;

#[derive(Debug, Clone, PartialEq)]
pub struct AISpecialPowerUpdate {
    command_button_name: String,
    special_power_ai_type: String,
    special_power_radius: Option<f32>,
    randomize_target_location: bool,
    special_power_range: Option<f32>,
    spell_makes_a_structure: bool,
}

impl AISpecialPowerUpdate {
    #[must_use]
    pub fn command_button_name(&self) -> &str {
        &self.command_button_name
    }

    #[must_use]
    pub fn special_power_ai_type(&self) -> &str {
        &self.special_power_ai_type
    }

    #[must_use]
    pub fn special_power_radius(&self) -> Option<f32> {
        self.special_power_radius
    }

    #[must_use]
    pub fn randomize_target_location(&self) -> bool {
        self.randomize_target_location
    }

    #[must_use]
    pub fn special_power_range(&self) -> Option<f32> {
        self.special_power_range
    }

    #[must_use]
    pub fn spell_makes_a_structure(&self) -> bool {
        self.spell_makes_a_structure
    }
}

impl Component for AISpecialPowerUpdate {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for AISpecialPowerUpdate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn ai_special_power_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<AISpecialPowerUpdate, FieldError> {
    let command_button_name = fields
        .field("CommandButtonName")
        .map(str::to_owned)
        .ok_or_else(|| FieldError::new("AISpecialPowerUpdate requires CommandButtonName"))?;
    let special_power_ai_type = fields
        .field("SpecialPowerAIType")
        .map(str::to_ascii_uppercase)
        .ok_or_else(|| FieldError::new("AISpecialPowerUpdate requires SpecialPowerAIType"))?;
    if !ai_power_registry().contains(&special_power_ai_type) {
        return Err(FieldError::new(format!(
            "SpecialPowerAIType '{special_power_ai_type}' is not one of the engine's known AI power types"
        )));
    }
    let special_power_radius = fields
        .field_numeric("SpecialPowerRadius")
        .filter(|value| value.is_finite() && *value >= 0.0);
    let randomize_target_location = flag(fields, "RandomizeTargetLocation").unwrap_or(false);
    let special_power_range = fields
        .field_numeric("SpecialPowerRange")
        .filter(|value| value.is_finite() && *value >= 0.0);
    let spell_makes_a_structure = flag(fields, "SpellMakesAStructure").unwrap_or(false);
    Ok(AISpecialPowerUpdate {
        command_button_name,
        special_power_ai_type,
        special_power_radius,
        randomize_target_location,
        special_power_range,
        spell_makes_a_structure,
    })
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
        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.field(keyword).into_iter().collect()
        }
    }

    #[test]
    fn the_shipped_stance_shape_reads_its_button_and_type() {
        let fields = TestFields(BTreeMap::from([
            ("CommandButtonName", "Command_SetStanceAggressive"),
            ("SpecialPowerAIType", "AI_SPECIAL_POWER_STANCEAGGRESSIVE"),
        ]));
        let module = ai_special_power_update_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.command_button_name(), "Command_SetStanceAggressive");
        assert_eq!(
            module.special_power_ai_type(),
            "AI_SPECIAL_POWER_STANCEAGGRESSIVE"
        );
    }

    #[test]
    fn missing_command_button_name_is_refused() {
        let fields = TestFields(BTreeMap::from([(
            "SpecialPowerAIType",
            "AI_SPECIAL_POWER_STANCEAGGRESSIVE",
        )]));
        assert!(ai_special_power_update_from_fields(&fields, Frame::ZERO).is_err());
    }

    #[test]
    fn an_unknown_special_power_ai_type_is_refused() {
        let fields = TestFields(BTreeMap::from([
            ("CommandButtonName", "Command_Whatever"),
            ("SpecialPowerAIType", "AI_SPECIAL_POWER_NOT_A_REAL_TYPE"),
        ]));
        assert!(ai_special_power_update_from_fields(&fields, Frame::ZERO).is_err());
    }

    #[test]
    fn an_inert_type_still_loads() {
        let fields = TestFields(BTreeMap::from([
            ("CommandButtonName", "Command_CaptureBuilding"),
            ("SpecialPowerAIType", "AI_SPECIAL_POWER_CAPTURE_BUILDING"),
        ]));
        assert!(ai_special_power_update_from_fields(&fields, Frame::ZERO).is_ok());
    }
}
