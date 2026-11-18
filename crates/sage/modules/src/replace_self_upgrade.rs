use std::sync::Arc;

use mint_sim::module::{Capability, CapabilityMask, UpgradeMask};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, ModuleKind, StateHasher};

use crate::effects::{Effect, Name, Names, ReplaceCarry};
use crate::upgrade_mux::{attempt_upgrade_via_mux, UpgradeEffect, UpgradeMuxData};

#[derive(Debug, PartialEq)]
struct Data {
    replace_with: Option<Name>,
    and_then_add: Names,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReplaceSelfUpgrade {
    data: Arc<Data>,
    mux: UpgradeMuxData,
}

impl ReplaceSelfUpgrade {
    #[must_use]
    pub fn replace_with(&self) -> Option<&str> {
        self.data.replace_with.as_deref()
    }

    #[must_use]
    pub fn and_then_add(&self) -> &Names {
        &self.data.and_then_add
    }
}

impl Component for ReplaceSelfUpgrade {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.mux.hash_into(hasher);
    }
}

impl UpgradeEffect for ReplaceSelfUpgrade {
    fn mux_mut(&mut self) -> &mut UpgradeMuxData {
        &mut self.mux
    }

    fn on_fire(&mut self, _active: &UpgradeMask, out: &mut Vec<Effect>) {
        if let Some(template) = self.data.replace_with.clone() {
            out.push(Effect::ReplaceAndAdd {
                template,
                carry: ReplaceCarry::default(),
                attendants: Arc::clone(&self.data.and_then_add),
            });
        }
    }
}

impl ModuleKind for ReplaceSelfUpgrade {
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

pub fn replace_self_upgrade_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<ReplaceSelfUpgrade, FieldError> {
    let named = fields.field_args("ReplaceWith");
    if named.len() > 1 {
        return Err(FieldError::new(format!(
            "ReplaceWith names {} objects; it takes one",
            named.len()
        )));
    }
    let and_then_add: Vec<Name> = fields
        .field_args_each("AndThenAddA")
        .into_iter()
        .filter_map(|tokens| tokens.first().map(|token| Name::from(*token)))
        .collect();
    Ok(ReplaceSelfUpgrade {
        data: Arc::new(Data {
            replace_with: named.first().map(|name| Name::from(*name)),
            and_then_add: and_then_add.into(),
        }),
        mux: UpgradeMuxData::from_fields(fields)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct TestFields(BTreeMap<&'static str, Vec<Vec<&'static str>>>);
    impl ModuleFieldSource for TestFields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.field_args(keyword).first().copied()
        }
        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.field_args_each(keyword)
                .into_iter()
                .next()
                .unwrap_or_default()
        }
        fn field_args_each(&self, keyword: &str) -> Vec<Vec<&str>> {
            self.0
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        }
    }

    #[test]
    fn a_repeated_and_then_add_a_keeps_every_object_in_order() {
        let fields = TestFields(BTreeMap::from([
            ("ReplaceWith", vec![vec!["GondorCastleWallSegment"]]),
            (
                "AndThenAddA",
                vec![vec!["GondorCastleWallHub"], vec!["GondorCastleWallSegment"]],
            ),
        ]));
        let module = replace_self_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.replace_with(), Some("GondorCastleWallSegment"));
        let added: Vec<&str> = module.and_then_add().iter().map(AsRef::as_ref).collect();
        assert_eq!(added, ["GondorCastleWallHub", "GondorCastleWallSegment"]);
    }

    #[test]
    fn a_replace_with_naming_two_objects_is_refused() {
        let fields = TestFields(BTreeMap::from([(
            "ReplaceWith",
            vec![vec!["OneObject", "AndAnother"]],
        )]));
        assert!(replace_self_upgrade_from_fields(&fields, Frame::ZERO).is_err());
    }
}
