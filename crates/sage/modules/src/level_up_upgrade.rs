use mint_sim::module::{Capability, CapabilityMask, UpgradeMask};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, ModuleKind, StateHasher};

use crate::effects::Effect;
use crate::upgrade_mux::{attempt_upgrade_via_mux, UpgradeEffect, UpgradeMuxData};

#[derive(Debug, Clone, PartialEq)]
pub struct LevelUpUpgrade {
    mux: UpgradeMuxData,
    levels_to_gain: u32,
    level_cap: Option<u32>,
}

impl LevelUpUpgrade {
    #[must_use]
    pub const fn levels_to_gain(&self) -> u32 {
        self.levels_to_gain
    }

    #[must_use]
    pub const fn level_cap(&self) -> Option<u32> {
        self.level_cap
    }
}

impl UpgradeEffect for LevelUpUpgrade {
    fn mux_mut(&mut self) -> &mut UpgradeMuxData {
        &mut self.mux
    }

    fn on_fire(&mut self, _active: &UpgradeMask, out: &mut Vec<Effect>) {
        if self.levels_to_gain > 0 {
            out.push(Effect::LevelUp {
                levels: self.levels_to_gain,
                cap: self.level_cap,
            });
        }
    }
}

impl Component for LevelUpUpgrade {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.mux.hash_into(hasher);
        hasher.write_u32(self.levels_to_gain);
        match self.level_cap {
            Some(v) => {
                hasher.write_bool(true);
                hasher.write_u32(v);
            }
            None => hasher.write_bool(false),
        }
    }
}

impl ModuleKind for LevelUpUpgrade {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Upgrade)
        .union(CapabilityMask::single(Capability::Update));

    fn update(&mut self, ctx: &mut mint_sim::module::UpdateContext<'_>) -> mint_sim::module::Sleep {
        self.mux.update(ctx)
    }

    fn attempt_upgrade(&mut self, active: &UpgradeMask, ctx: &mut mint_sim::UpgradeContext<'_>) {
        attempt_upgrade_via_mux(self, active, ctx);
    }

    fn would_upgrade(&self, active: &UpgradeMask) -> bool {
        self.mux.would_fire(active)
    }

    fn on_upgrade_removed(&mut self, removed: &UpgradeMask) {
        self.mux.reset_if_triggered_by(removed);
    }
}

pub fn level_up_upgrade_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<LevelUpUpgrade, FieldError> {
    let levels_to_gain = optional_non_negative(fields, "LevelsToGain")?.unwrap_or(0);
    let level_cap = optional_non_negative(fields, "LevelCap")?;
    Ok(LevelUpUpgrade {
        mux: UpgradeMuxData::from_fields(fields)?,
        levels_to_gain,
        level_cap,
    })
}

fn optional_non_negative(
    fields: &dyn ModuleFieldSource,
    keyword: &str,
) -> Result<Option<u32>, FieldError> {
    let Some(raw) = fields.field(keyword) else {
        return Ok(None);
    };
    let value = fields
        .field_numeric(keyword)
        .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not a number/known macro")))?;
    if value < 0.0 {
        return Err(FieldError::new(format!("{keyword} '{raw}' is negative")));
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(Some(value as u32))
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
    fn reads_levels_to_gain_and_level_cap() {
        let fields = TestFields(BTreeMap::from([("LevelsToGain", "1"), ("LevelCap", "2")]));
        let module = level_up_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.levels_to_gain(), 1);
        assert_eq!(module.level_cap(), Some(2));
    }

    #[test]
    fn absent_fields_default_to_zero_and_none() {
        let fields = TestFields(BTreeMap::new());
        let module = level_up_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.levels_to_gain(), 0);
        assert_eq!(module.level_cap(), None);
    }

    #[test]
    fn a_negative_levels_to_gain_is_refused() {
        let fields = TestFields(BTreeMap::from([("LevelsToGain", "-1")]));
        assert!(level_up_upgrade_from_fields(&fields, Frame::ZERO).is_err());
    }
}
