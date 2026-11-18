use mint_sim::module::{Capability, CapabilityMask, UpgradeMask};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, ModuleKind, StateHasher};

use crate::effects::{Effect, Name};
use crate::upgrade_mux::{attempt_upgrade_via_mux, UpgradeEffect, UpgradeMuxData};

#[derive(Debug, Clone, PartialEq)]
pub struct DoCommandUpgrade {
    mux: UpgradeMuxData,
    press: Option<Name>,
    pub remove_press: Option<Name>,
}

impl DoCommandUpgrade {
    #[must_use]
    pub fn press(&self) -> Option<&str> {
        self.press.as_deref()
    }
}

impl UpgradeEffect for DoCommandUpgrade {
    fn mux_mut(&mut self) -> &mut UpgradeMuxData {
        &mut self.mux
    }

    fn on_fire(&mut self, _active: &UpgradeMask, out: &mut Vec<Effect>) {
        if let Some(button) = &self.press {
            out.push(Effect::PressCommand(Name::clone(button)));
        }
    }
}

impl Component for DoCommandUpgrade {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.mux.hash_into(hasher);
    }
}

impl ModuleKind for DoCommandUpgrade {
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

pub fn do_command_upgrade_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<DoCommandUpgrade, FieldError> {
    Ok(DoCommandUpgrade {
        mux: UpgradeMuxData::from_fields(fields)?,
        press: fields.field("GetUpgradeCommandButtonName").map(Name::from),
        remove_press: fields
            .field("RemoveUpgradeCommandButtonName")
            .map(Name::from),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use mint_sim::module::UpgradeBit;
    use std::collections::BTreeMap;

    struct Fields(BTreeMap<&'static str, &'static str>);
    impl ModuleFieldSource for Fields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0.get(keyword).copied()
        }
        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.0
                .get(keyword)
                .map(|value| value.split_whitespace().collect())
                .unwrap_or_default()
        }
        fn field_upgrade_mask(&self, keyword: &str) -> UpgradeMask {
            let mut mask = UpgradeMask::EMPTY;
            for token in self.field_args(keyword) {
                if let Ok(n) = token.parse::<u32>() {
                    mask.set(UpgradeBit::new(n));
                }
            }
            mask
        }
    }
    fn fields(pairs: &[(&'static str, &'static str)]) -> Fields {
        Fields(pairs.iter().copied().collect())
    }

    #[test]
    fn reads_the_press_and_remove_button_names() {
        let module = do_command_upgrade_from_fields(
            &fields(&[
                ("TriggeredBy", "1"),
                ("GetUpgradeCommandButtonName", "Command_Soul"),
                ("RemoveUpgradeCommandButtonName", "Command_Undo"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert_eq!(module.press(), Some("Command_Soul"));
        assert_eq!(module.remove_press.as_deref(), Some("Command_Undo"));
    }

    #[test]
    fn firing_reports_press_command_for_the_named_button() {
        let mut module = do_command_upgrade_from_fields(
            &fields(&[
                ("TriggeredBy", "1"),
                ("GetUpgradeCommandButtonName", "Command_Soul"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(1));
        let mut out = Vec::new();
        module.on_fire(&active, &mut out);
        assert_eq!(out, vec![Effect::PressCommand(Name::from("Command_Soul"))]);
    }
}
