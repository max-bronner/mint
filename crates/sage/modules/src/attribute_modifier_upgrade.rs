use mint_sim::module::{Capability, CapabilityMask, UpgradeMask};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, ModuleKind, StateHasher};

use crate::effects::{Effect, Name};
use crate::upgrade_mux::{attempt_upgrade_via_mux, UpgradeEffect, UpgradeMuxData};

#[derive(Debug, Clone, PartialEq)]
pub struct AttributeModifierUpgrade {
    mux: UpgradeMuxData,
    attribute_modifier: Option<Name>,
}

impl AttributeModifierUpgrade {
    #[must_use]
    pub fn attribute_modifier(&self) -> Option<&str> {
        self.attribute_modifier.as_deref()
    }
}

impl UpgradeEffect for AttributeModifierUpgrade {
    fn mux_mut(&mut self) -> &mut UpgradeMuxData {
        &mut self.mux
    }

    fn on_fire(&mut self, _active: &UpgradeMask, out: &mut Vec<Effect>) {
        if let Some(modifier) = &self.attribute_modifier {
            out.push(Effect::AttributeModifier(Name::clone(modifier)));
        }
    }
}

impl Component for AttributeModifierUpgrade {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.mux.hash_into(hasher);
    }
}

impl ModuleKind for AttributeModifierUpgrade {
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

pub fn attribute_modifier_upgrade_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<AttributeModifierUpgrade, FieldError> {
    Ok(AttributeModifierUpgrade {
        mux: UpgradeMuxData::from_fields(fields)?,
        attribute_modifier: fields.field("AttributeModifier").map(Name::from),
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
    }

    #[test]
    fn reads_the_attribute_modifier_reference() {
        let fields = TestFields(BTreeMap::from([(
            "AttributeModifier",
            "EasyAISinglePlayer_Bonus",
        )]));
        let module = attribute_modifier_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(
            module.attribute_modifier(),
            Some("EasyAISinglePlayer_Bonus")
        );
    }

    #[test]
    fn absent_reference_reads_as_none() {
        let fields = TestFields(BTreeMap::new());
        let module = attribute_modifier_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.attribute_modifier(), None);
    }
}
