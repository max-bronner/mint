use std::sync::Arc;

use mint_sim::module::{Capability, CapabilityMask, CreateContext};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, ModuleKind, StateHasher};

use crate::effects::{Effect, EffectLog, Name};
use crate::object_filter::ObjectFilter;

const MAX_RADIUS: f32 = 100_000.0;

#[derive(Debug, PartialEq)]
struct Data {
    radius: f32,
    upgrade: Option<Name>,
    filter: Arc<ObjectFilter>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InheritUpgradeCreate {
    data: Arc<Data>,
}

impl InheritUpgradeCreate {
    #[must_use]
    pub fn radius(&self) -> f32 {
        self.data.radius
    }

    #[must_use]
    pub fn upgrade(&self) -> Option<&str> {
        self.data.upgrade.as_deref()
    }

    #[must_use]
    pub fn filter(&self) -> &ObjectFilter {
        &self.data.filter
    }
}

impl Component for InheritUpgradeCreate {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for InheritUpgradeCreate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Create);

    fn on_create(&mut self, ctx: &mut CreateContext<'_>) {
        let Some(upgrade) = &self.data.upgrade else {
            return;
        };
        if let Some(log) = ctx.effects.get::<EffectLog>() {
            log.push(
                ctx.entity,
                Effect::InheritUpgrade {
                    upgrade: Name::clone(upgrade),
                    radius: self.data.radius,
                    filter: Arc::clone(&self.data.filter),
                },
            );
        }
    }
}

fn radius(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<f32, FieldError> {
    let Some(raw) = fields.field(keyword) else {
        return Ok(0.0);
    };
    fields
        .field_numeric(keyword)
        .filter(|value| (0.0..=MAX_RADIUS).contains(value))
        .ok_or_else(|| {
            FieldError::new(format!(
                "{keyword} '{raw}' is not a number from 0 to {MAX_RADIUS}"
            ))
        })
}

pub fn inherit_upgrade_create_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<InheritUpgradeCreate, FieldError> {
    let tokens: Vec<String> = fields
        .field_args("ObjectFilter")
        .into_iter()
        .map(str::to_owned)
        .collect();
    Ok(InheritUpgradeCreate {
        data: Arc::new(Data {
            radius: radius(fields, "Radius")?,
            upgrade: fields.field("Upgrade").map(Name::from),
            filter: Arc::new(ObjectFilter::parse(&tokens)),
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sim::module::ModuleRegistry;
    use mint_sim::{Effects, World};
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

    fn fields(pairs: &[(&'static str, &[&'static str])]) -> TestFields {
        TestFields(pairs.iter().map(|(k, v)| (*k, v.to_vec())).collect())
    }

    #[test]
    fn creation_asks_for_the_upgrade_with_its_radius_and_filter() {
        let module = inherit_upgrade_create_from_fields(
            &fields(&[
                ("Radius", &["750"]),
                ("Upgrade", &["Upgrade_ModdedStonework"]),
                ("ObjectFilter", &["ANY", "+ModdedCitadel"]),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<InheritUpgradeCreate>(&mut world, "InheritUpgradeCreate");
        let entity = world.spawn().unwrap();
        registry
            .attach(&mut world, entity, Frame::ZERO, module)
            .unwrap();
        let mut log = EffectLog::new();
        registry.broadcast_create(
            &mut world,
            entity,
            Frame::ZERO,
            Effects::into_sink(&mut log),
        );
        let effects: Vec<Effect> = log.drain().into_iter().map(|(_, e)| e).collect();
        assert_eq!(effects.len(), 1);
        match &effects[0] {
            Effect::InheritUpgrade {
                upgrade,
                radius,
                filter,
            } => {
                assert_eq!(&**upgrade, "Upgrade_ModdedStonework");
                assert_eq!(*radius, 750.0);
                assert!(filter.passes(&[], "ModdedCitadel"));
                assert!(!filter.passes(&[], "SomethingElse"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_radius_that_is_not_a_number_is_refused() {
        assert!(inherit_upgrade_create_from_fields(
            &fields(&[("Radius", &["near"]), ("Upgrade", &["Upgrade_X"])]),
            Frame::ZERO,
        )
        .is_err());
    }
}
