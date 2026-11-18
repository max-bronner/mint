use mint_sage_ini::value::parse_bool;
use mint_sim::module::{CapabilityMask, ModuleKind};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, StateHasher};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DualWeaponBehavior {
    pub switch_distance: f32,
    pub while_mounted: bool,
    pub minimum_switch_time_ms: f32,
    pub horde_range_weapon: bool,
    pub real_victim_range: bool,
}

impl Component for DualWeaponBehavior {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for DualWeaponBehavior {
    const CAPABILITIES: CapabilityMask = CapabilityMask::NONE;
}

pub fn dual_weapon_behavior_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<DualWeaponBehavior, FieldError> {
    let flag = |keyword: &str| -> Result<bool, FieldError> {
        match fields.field(keyword) {
            None => Ok(false),
            Some(raw) => parse_bool(raw)
                .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not Yes/No"))),
        }
    };
    Ok(DualWeaponBehavior {
        switch_distance: number(fields, "SwitchWeaponOnCloseRangeDistance")?,
        while_mounted: flag("UseCloseRangeWhileMounted")?,
        minimum_switch_time_ms: number(fields, "MinimumSwitchTime")?,
        horde_range_weapon: flag("UseHordeRangeWeapon")?,
        real_victim_range: flag("UseRealVictimRange")?,
    })
}

fn number(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<f32, FieldError> {
    match fields.field(keyword) {
        None => Ok(0.0),
        Some(raw) => fields
            .field_numeric(keyword)
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not a distance"))),
    }
}
