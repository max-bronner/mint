use std::sync::Arc;

use mint_sim::module::{Capability, CapabilityMask, CreateContext};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, ModuleKind, StateHasher};

use crate::effects::{Effect, EffectLog, Name};

#[derive(Debug, PartialEq)]
struct Data {
    upgrade_to_grant: Option<Name>,
    give_on_build_complete: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GrantUpgradeCreate {
    data: Arc<Data>,
}

impl GrantUpgradeCreate {
    #[must_use]
    pub fn upgrade_to_grant(&self) -> Option<&str> {
        self.data.upgrade_to_grant.as_deref()
    }

    #[must_use]
    pub fn gives_on_build_complete(&self) -> bool {
        self.data.give_on_build_complete
    }
}

impl Component for GrantUpgradeCreate {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for GrantUpgradeCreate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Create);

    fn on_create(&mut self, ctx: &mut CreateContext<'_>) {
        let Some(name) = &self.data.upgrade_to_grant else {
            return;
        };
        if let Some(log) = ctx.effects.get::<EffectLog>() {
            log.push(ctx.entity, Effect::GrantUpgrade(Name::clone(name)));
        }
    }
}

pub fn grant_upgrade_create_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<GrantUpgradeCreate, FieldError> {
    let give_on_build_complete = match fields.field("GiveOnBuildComplete") {
        None => false,
        Some(raw) => mint_sage_ini::value::parse_bool(raw)
            .ok_or_else(|| FieldError::new(format!("GiveOnBuildComplete '{raw}' is not Yes/No")))?,
    };
    Ok(GrantUpgradeCreate {
        data: Arc::new(Data {
            upgrade_to_grant: fields.field("UpgradeToGrant").map(Name::from),
            give_on_build_complete,
        }),
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
    }

    fn fields(pairs: &[(&'static str, &'static str)]) -> TestFields {
        TestFields(pairs.iter().map(|(k, v)| (*k, vec![*v])).collect())
    }

    #[test]
    fn reads_upgrade_to_grant_and_give_on_build_complete() {
        let module = grant_upgrade_create_from_fields(
            &fields(&[
                ("UpgradeToGrant", "Upgrade_TrebuchetTurret"),
                ("GiveOnBuildComplete", "Yes"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert_eq!(module.upgrade_to_grant(), Some("Upgrade_TrebuchetTurret"));
        assert!(module.gives_on_build_complete());
    }

    #[test]
    fn give_on_build_complete_defaults_to_no() {
        let module = grant_upgrade_create_from_fields(
            &fields(&[("UpgradeToGrant", "Upgrade_A")]),
            Frame::ZERO,
        )
        .unwrap();
        assert!(!module.gives_on_build_complete());
    }

    #[test]
    fn an_unparseable_give_on_build_complete_is_refused() {
        let err = grant_upgrade_create_from_fields(
            &fields(&[("GiveOnBuildComplete", "Sideways")]),
            Frame::ZERO,
        );
        assert!(err.is_err());
    }

    #[test]
    fn on_create_reports_grant_upgrade_for_the_named_upgrade() {
        let mut module = grant_upgrade_create_from_fields(
            &fields(&[("UpgradeToGrant", "Upgrade_TrebuchetTurret")]),
            Frame::ZERO,
        )
        .unwrap();
        let mut world = mint_sim::World::new();
        let entity = world.spawn().unwrap();
        let mut log = EffectLog::new();
        let mut ctx = CreateContext {
            entity,
            now: Frame::ZERO,
            effects: mint_sim::Effects::into_sink(&mut log),
        };
        module.on_create(&mut ctx);
        let entries = log.drain();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].1,
            Effect::GrantUpgrade(Name::from("Upgrade_TrebuchetTurret"))
        );
    }

    #[test]
    fn on_create_with_no_upgrade_to_grant_reports_nothing() {
        let mut module = grant_upgrade_create_from_fields(&fields(&[]), Frame::ZERO).unwrap();
        let mut world = mint_sim::World::new();
        let entity = world.spawn().unwrap();
        let mut log = EffectLog::new();
        let mut ctx = CreateContext {
            entity,
            now: Frame::ZERO,
            effects: mint_sim::Effects::into_sink(&mut log),
        };
        module.on_create(&mut ctx);
        assert!(log.is_empty());
    }
}
