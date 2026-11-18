use std::sync::Arc;

use mint_sim::module::UpdateContext;
use mint_sim::{Entity, FieldError, Frame, ModuleFieldSource};

use crate::effects::{Effect, EffectLog, Name};
use crate::special_ability_update::{
    ability_data_from_fields, AbilityTarget, AbilityUpdate, Trigger,
};
use crate::world_reads::Positions;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetKindArg {
    TargetPos,
    ObjectPos,
}

#[derive(Debug, Clone, PartialEq)]
struct TriggerEntry {
    tag: Name,
    kind: TargetKindArg,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActivateTrigger {
    triggers: Vec<TriggerEntry>,
}

impl Trigger for ActivateTrigger {
    fn fire(&self, entity: Entity, target: AbilityTarget, ctx: &mut UpdateContext<'_>) {
        if self.triggers.is_empty() {
            return;
        }
        let positions = ctx.reads.get::<Positions>();
        let own_position = || {
            positions
                .and_then(|p| p.get(entity))
                .map(|(x, y, _, _)| (x, y))
        };
        let Some(log) = ctx.effects.get::<EffectLog>() else {
            return;
        };
        for entry in &self.triggers {
            let (object_target, at) = match entry.kind {
                TargetKindArg::ObjectPos => (None, own_position()),
                TargetKindArg::TargetPos => match target {
                    AbilityTarget::Object(target) => (Some(target), None),
                    AbilityTarget::Position(x, y) => (None, Some((x, y))),
                    AbilityTarget::None => (None, own_position()),
                },
            };
            log.push(
                entity,
                Effect::TriggerModule {
                    tag: Name::clone(&entry.tag),
                    target: object_target,
                    at,
                },
            );
        }
    }
}

pub type ActivateModuleSpecialPower = AbilityUpdate<ActivateTrigger>;

pub fn activate_module_special_power_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<ActivateModuleSpecialPower, FieldError> {
    let triggers = fields
        .field_args_each("TriggerSpecialPower")
        .into_iter()
        .map(|tokens| match tokens.as_slice() {
            [tag] => Ok(TriggerEntry {
                tag: Name::from(*tag),
                kind: TargetKindArg::ObjectPos,
            }),
            [tag, kind] if kind.eq_ignore_ascii_case("TARGETPOS") => Ok(TriggerEntry {
                tag: Name::from(*tag),
                kind: TargetKindArg::TargetPos,
            }),
            [tag, kind] if kind.eq_ignore_ascii_case("OBJECTPOS") => Ok(TriggerEntry {
                tag: Name::from(*tag),
                kind: TargetKindArg::ObjectPos,
            }),
            other => Err(FieldError::new(format!(
                "TriggerSpecialPower takes a module tag and optionally TARGETPOS or OBJECTPOS,                  not {other:?}"
            ))),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let trigger = ActivateTrigger { triggers };
    Ok(AbilityUpdate {
        data: Arc::new(ability_data_from_fields(fields, trigger)?),
        active: None,
        effect_until: None,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use mint_sim::module::{DestroyRequests, ModuleRegistry, Reads};
    use mint_sim::{Activation, Effects, Pcg32, World};

    #[test]
    fn missing_power_template_is_refused() {
        assert!(
            activate_module_special_power_from_fields(&TestSource(vec![]), Frame::ZERO).is_err()
        );
    }

    struct TestSource(Vec<(&'static str, &'static str)>);
    impl ModuleFieldSource for TestSource {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0.iter().find(|(k, _)| *k == keyword).map(|(_, v)| *v)
        }
        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.field(keyword)
                .map(|v| v.split_whitespace().collect())
                .unwrap_or_default()
        }
        fn field_args_each(&self, keyword: &str) -> Vec<Vec<&str>> {
            self.0
                .iter()
                .filter(|(k, _)| *k == keyword)
                .map(|(_, v)| v.split_whitespace().collect())
                .collect()
        }
    }

    #[test]
    fn three_trigger_lines_read_in_order() {
        let module = activate_module_special_power_from_fields(
            &TestSource(vec![
                ("SpecialPowerTemplate", "SpecialAbilityNecroFellStrength"),
                ("StartAbilityRange", "80"),
                (
                    "TriggerSpecialPower",
                    "ModuleTag_NecroFellStrengthBlastStarter TARGETPOS",
                ),
                (
                    "TriggerSpecialPower",
                    "ModuleTag_NecroFellStrengthBlastTrigger TARGETPOS",
                ),
                (
                    "TriggerSpecialPower",
                    "ModuleTag_NecroFellStrengthBuffStarter OBJECTPOS",
                ),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        let triggers = &module.data.trigger.triggers;
        assert_eq!(triggers.len(), 3);
        assert_eq!(&*triggers[0].tag, "ModuleTag_NecroFellStrengthBlastStarter");
        assert_eq!(triggers[0].kind, TargetKindArg::TargetPos);
        assert_eq!(triggers[2].kind, TargetKindArg::ObjectPos);
    }

    #[test]
    fn an_unknown_target_kind_is_refused() {
        assert!(activate_module_special_power_from_fields(
            &TestSource(vec![
                ("SpecialPowerTemplate", "P"),
                ("TriggerSpecialPower", "ModuleTag_Foo SIDEWAYS"),
            ]),
            Frame::ZERO
        )
        .is_err());
    }

    #[test]
    fn firing_reports_one_trigger_module_effect_per_entry() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<ActivateModuleSpecialPower>(&mut world, "ActivateModuleSpecialPower");
        let holder = world.spawn().unwrap();
        let module = activate_module_special_power_from_fields(
            &TestSource(vec![
                ("SpecialPowerTemplate", "ModAuthorMover"),
                ("TriggerSpecialPower", "ModuleTag_ModAuthorBlast OBJECTPOS"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        registry
            .attach(&mut world, holder, Frame::ZERO, module)
            .unwrap();

        let mut log = EffectLog::new();
        let activation = Activation {
            power: "ModAuthorMover",
            target: None,
            at: None,
        };
        assert!(registry.activate(
            &mut world,
            holder,
            &activation,
            Frame::ZERO,
            Effects::into_sink(&mut log)
        ));

        let positions = Positions::from_pairs([(holder, (10.0, 20.0, 5.0, 0.0))]);
        let mut rng = Pcg32::new(1);
        let mut destroy = DestroyRequests::new();
        let mut seen = None;
        for frame in 0..20u64 {
            let mut log = EffectLog::new();
            registry.update_with_reads(
                &mut world,
                Frame::new(frame),
                &mut rng,
                &mut destroy,
                Effects::into_sink(&mut log),
                Reads::from_snapshot(&positions),
            );
            for (_, effect) in log.drain() {
                if let Effect::TriggerModule { tag, target, at } = effect {
                    seen = Some((tag, target, at));
                }
            }
        }
        let (tag, target, at) = seen.expect("the trigger must report one TriggerModule effect");
        assert_eq!(&*tag, "ModuleTag_ModAuthorBlast");
        assert_eq!(target, None);
        assert_eq!(at, Some((10.0, 20.0)));
    }
}
