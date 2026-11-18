use std::sync::Arc;

use mint_sim::module::{Capability, CapabilityMask, UpgradeMask};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, ModuleKind, StateHasher};

use crate::effects::{names, Effect, Names};
use crate::upgrade_mux::{attempt_upgrade_via_mux, UpgradeEffect, UpgradeMuxData};

#[derive(Debug, Clone, PartialEq)]
pub struct StatusBitsUpgrade {
    mux: UpgradeMuxData,
    data: Arc<(Names, Names)>,
}

impl StatusBitsUpgrade {
    #[must_use]
    pub fn status_to_set(&self) -> Vec<&str> {
        self.data.0.iter().map(AsRef::as_ref).collect()
    }

    #[must_use]
    pub fn status_to_clear(&self) -> Vec<&str> {
        self.data.1.iter().map(AsRef::as_ref).collect()
    }
}

impl UpgradeEffect for StatusBitsUpgrade {
    fn mux_mut(&mut self) -> &mut UpgradeMuxData {
        &mut self.mux
    }

    fn on_fire(&mut self, _active: &UpgradeMask, out: &mut Vec<Effect>) {
        let (set, clear) = &*self.data;
        if set.is_empty() && clear.is_empty() {
            return;
        }
        out.push(Effect::StatusBits {
            set: Names::clone(set),
            clear: Names::clone(clear),
        });
    }
}

impl Component for StatusBitsUpgrade {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.mux.hash_into(hasher);
    }
}

impl ModuleKind for StatusBitsUpgrade {
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

pub fn status_bits_upgrade_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<StatusBitsUpgrade, FieldError> {
    let list = |keyword: &str| -> Names {
        names(
            fields
                .field_args(keyword)
                .into_iter()
                .map(str::to_owned)
                .collect(),
        )
    };
    Ok(StatusBitsUpgrade {
        mux: UpgradeMuxData::from_fields(fields)?,
        data: Arc::new((list("StatusToSet"), list("StatusToClear"))),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct TestFields(BTreeMap<&'static str, Vec<&'static str>>);
    impl ModuleFieldSource for TestFields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .and_then(|(_, v)| v.first())
                .copied()
        }
        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.0
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        }
    }

    #[test]
    fn reads_a_space_separated_status_list() {
        let fields = TestFields(BTreeMap::from([(
            "StatusToSet",
            vec!["IGNORE_AI_COMMAND", "SUMMONING_REPLACEMENT"],
        )]));
        let module = status_bits_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(
            module.status_to_set(),
            ["IGNORE_AI_COMMAND", "SUMMONING_REPLACEMENT"]
        );
        assert!(module.status_to_clear().is_empty());
    }
}
