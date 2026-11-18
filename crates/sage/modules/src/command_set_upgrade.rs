use std::sync::Arc;

use mint_sim::module::{Capability, CapabilityMask, UpgradeMask};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, ModuleKind, StateHasher};

use crate::effects::{Effect, Name};
use crate::upgrade_mux::{attempt_upgrade_via_mux, UpgradeEffect, UpgradeMuxData};

#[derive(Debug, PartialEq)]
struct Data {
    command_set: Name,
    command_set_alt: Name,
    trigger_alt: UpgradeMask,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommandSetUpgrade {
    mux: UpgradeMuxData,
    data: Arc<Data>,
    took_alt: Option<bool>,
}

impl CommandSetUpgrade {
    #[must_use]
    pub fn command_set(&self) -> &str {
        &self.data.command_set
    }

    #[must_use]
    pub fn command_set_alt(&self) -> &str {
        &self.data.command_set_alt
    }

    #[must_use]
    pub fn resolved_command_set(&self) -> Option<&str> {
        self.took_alt.map(|alt| {
            if alt {
                self.command_set_alt()
            } else {
                self.command_set()
            }
        })
    }
}

impl UpgradeEffect for CommandSetUpgrade {
    fn mux_mut(&mut self) -> &mut UpgradeMuxData {
        &mut self.mux
    }

    fn on_fire(&mut self, active: &UpgradeMask, out: &mut Vec<Effect>) {
        let alt = active.intersects(&self.data.trigger_alt);
        self.took_alt = Some(alt);
        out.push(Effect::CommandSet(Name::clone(if alt {
            &self.data.command_set_alt
        } else {
            &self.data.command_set
        })));
    }
}

impl Component for CommandSetUpgrade {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.mux.hash_into(hasher);
        match self.took_alt {
            Some(alt) => {
                hasher.write_bool(true);
                hasher.write_bool(alt);
            }
            None => hasher.write_bool(false),
        }
    }
}

impl ModuleKind for CommandSetUpgrade {
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

pub fn command_set_upgrade_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<CommandSetUpgrade, FieldError> {
    Ok(CommandSetUpgrade {
        mux: UpgradeMuxData::from_fields(fields)?,
        data: Arc::new(Data {
            command_set: Name::from(fields.field("CommandSet").unwrap_or_default()),
            command_set_alt: Name::from(fields.field("CommandSetAlt").unwrap_or_default()),
            trigger_alt: fields.field_upgrade_mask("TriggerAlt"),
        }),
        took_alt: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sim::module::UpgradeBit;
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

    fn ctx() -> mint_sim::UpgradeContext<'static> {
        mint_sim::UpgradeContext {
            entity: mint_sim::World::new().spawn().unwrap(),
            now: Frame::ZERO,
            effects: mint_sim::Effects::none(),
        }
    }

    #[test]
    fn firing_reports_the_resolved_command_set() {
        let fields = TestFields(BTreeMap::from([
            ("TriggeredBy", vec!["1"]),
            ("TriggerAlt", vec!["2"]),
            ("CommandSet", vec!["Primary"]),
            ("CommandSetAlt", vec!["Alt"]),
        ]));
        let mut module = command_set_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(1));
        active.set(UpgradeBit::new(2));
        let mut log = crate::effects::EffectLog::new();
        let mut ctx = mint_sim::UpgradeContext {
            entity: mint_sim::World::new().spawn().unwrap(),
            now: Frame::ZERO,
            effects: mint_sim::Effects::into_sink(&mut log),
        };
        module.attempt_upgrade(&active, &mut ctx);
        assert_eq!(log.entries()[0].1, Effect::CommandSet(Name::from("Alt")));
    }

    #[test]
    fn reads_command_set_fields() {
        let fields = TestFields(BTreeMap::from([
            ("CommandSet", vec!["CommandSetGondorFortress"]),
            ("CommandSetAlt", vec!["CommandSetGondorFortressElf"]),
        ]));
        let module = command_set_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.command_set(), "CommandSetGondorFortress");
        assert_eq!(module.command_set_alt(), "CommandSetGondorFortressElf");
    }

    #[test]
    fn without_trigger_alt_firing_resolves_to_the_primary_set() {
        let fields = TestFields(BTreeMap::from([
            ("TriggeredBy", vec!["1"]),
            ("CommandSet", vec!["Primary"]),
            ("CommandSetAlt", vec!["Alt"]),
        ]));
        let mut module = command_set_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(1));
        module.attempt_upgrade(&active, &mut ctx());
        assert_eq!(module.resolved_command_set(), Some("Primary"));
    }

    #[test]
    fn when_trigger_alt_is_in_the_offered_mask_it_resolves_to_the_alt_set() {
        let fields = TestFields(BTreeMap::from([
            ("TriggeredBy", vec!["1"]),
            ("TriggerAlt", vec!["2"]),
            ("CommandSet", vec!["Primary"]),
            ("CommandSetAlt", vec!["Alt"]),
        ]));
        let mut module = command_set_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(1));
        active.set(UpgradeBit::new(2));
        module.attempt_upgrade(&active, &mut ctx());
        assert_eq!(module.resolved_command_set(), Some("Alt"));
    }

    #[test]
    fn resolution_only_happens_once_it_fires() {
        let fields = TestFields(BTreeMap::new());
        let module = command_set_upgrade_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.resolved_command_set(), None);
    }
}
