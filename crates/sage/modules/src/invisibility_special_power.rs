use std::sync::Arc;

use mint_sim::module::{Capability, CapabilityMask, ModuleKind};
use mint_sim::{
    ActivateContext, Activation, Component, FieldError, Frame, ModuleFieldSource, StateHasher,
};

use crate::effects::{Effect, EffectLog, Name, Names, PowerPlace};
use crate::invisibility_update::InvisibilityNugget;

fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

#[derive(Debug, PartialEq)]
struct InvisibilitySpecialPowerData {
    power: Name,
    radius: f32,
    filter: Names,
    duration: mint_sim::LogicFrames,
    nugget: Arc<InvisibilityNugget>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InvisibilitySpecialPower(Arc<InvisibilitySpecialPowerData>);

impl Component for InvisibilitySpecialPower {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for InvisibilitySpecialPower {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::SpecialPower);

    fn is_for_power(&self, power: &str) -> bool {
        same(&self.0.power, power)
    }

    fn activate(&mut self, activation: &Activation<'_>, ctx: &mut ActivateContext<'_>) {
        let Some(log) = ctx.effects.get::<EffectLog>() else {
            return;
        };
        log.push(
            ctx.entity,
            Effect::GrantInvisibility {
                at: PowerPlace::of(activation),
                filter: Names::clone(&self.0.filter),
                radius: self.0.radius,
                duration: self.0.duration,
                nugget: Arc::clone(&self.0.nugget),
            },
        );
    }
}

pub fn invisibility_special_power_from_fields(
    fields: &dyn ModuleFieldSource,
    now: Frame,
) -> Result<InvisibilitySpecialPower, FieldError> {
    let power = fields
        .field("SpecialPowerTemplate")
        .map(Name::from)
        .ok_or_else(|| FieldError::new("SpecialPowerTemplate is missing"))?;
    let radius = fields
        .field_numeric("BroadcastRadius")
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(0.0);
    let filter: Names = fields
        .field_args("ObjectFilter")
        .into_iter()
        .map(Name::from)
        .collect();
    let duration_ms = fields
        .field_numeric("Duration")
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(0.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let duration =
        crate::special_ability_update::retail_duration_ticks(duration_ms.min(86_400_000.0) as u32);
    let update = crate::invisibility_update::invisibility_update_from_fields(fields, now)?;
    Ok(InvisibilitySpecialPower(Arc::new(
        InvisibilitySpecialPowerData {
            power,
            radius,
            filter,
            duration,
            nugget: Arc::new(update.nugget().clone()),
        },
    )))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use mint_sim::module::ModuleRegistry;
    use mint_sim::{Effects, World};
    use std::collections::BTreeMap;

    struct Fields {
        own: BTreeMap<&'static str, Vec<&'static str>>,
        nested: Vec<(&'static str, BTreeMap<&'static str, Vec<&'static str>>)>,
    }
    impl ModuleFieldSource for Fields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.own
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .and_then(|(_, v)| v.first())
                .copied()
        }
        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.own
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .map(|(_, v)| v.clone())
                .unwrap_or_default()
        }
        fn nested(&self, keyword: &str, visit: &mut dyn FnMut(&dyn ModuleFieldSource)) {
            for (name, block) in &self.nested {
                if name.eq_ignore_ascii_case(keyword) {
                    let child = Fields {
                        own: block.clone(),
                        nested: Vec::new(),
                    };
                    visit(&child);
                }
            }
        }
    }
    fn pairs(list: &[(&'static str, &'static str)]) -> BTreeMap<&'static str, Vec<&'static str>> {
        list.iter()
            .map(|(k, v)| (*k, v.split_whitespace().collect()))
            .collect()
    }

    #[test]
    fn missing_power_template_is_refused() {
        let fields = Fields {
            own: pairs(&[]),
            nested: Vec::new(),
        };
        assert!(invisibility_special_power_from_fields(&fields, Frame::ZERO).is_err());
    }

    #[test]
    fn activation_reports_the_broadcast_radius_filter_duration_and_nugget() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<InvisibilitySpecialPower>(&mut world, "InvisibilitySpecialPower");
        let holder = world.spawn().unwrap();
        let module = invisibility_special_power_from_fields(
            &Fields {
                own: pairs(&[
                    ("SpecialPowerTemplate", "SpecialAbilityTestCloak"),
                    ("BroadcastRadius", "120"),
                    ("ObjectFilter", "ANY +HORDE +HERO +DOZER ALLIES"),
                    ("Duration", "9000"),
                ]),
                nested: vec![(
                    "InvisibilityNugget",
                    pairs(&[
                        ("ForbiddenConditions", "FIRING_ANY"),
                        ("DetectionRange", "80"),
                        ("InvisibilityType", "CAMOUFLAGE"),
                    ]),
                )],
            },
            Frame::ZERO,
        )
        .unwrap();
        registry
            .attach(&mut world, holder, Frame::ZERO, module)
            .unwrap();
        let activation = Activation {
            power: "SpecialAbilityTestCloak",
            target: None,
            at: None,
        };
        let mut log = EffectLog::new();
        assert!(registry.activate(
            &mut world,
            holder,
            &activation,
            Frame::ZERO,
            Effects::into_sink(&mut log)
        ));
        let mut found = false;
        for (_, effect) in log.drain() {
            if let Effect::GrantInvisibility {
                radius,
                duration,
                nugget,
                ..
            } = effect
            {
                assert_eq!(radius, 120.0);
                assert_eq!(
                    duration,
                    mint_sim::LogicFrames::from_engine_milliseconds(9000)
                );
                assert_eq!(nugget.kind, crate::InvisibilityType::Camouflage);
                found = true;
            }
        }
        assert!(found, "activation must push GrantInvisibility");
    }
}
