use mint_sage_ini::value::parse_bool;
use mint_sim::module::{Capability, CapabilityMask, DieContext, ModuleKind, ModuleSlot};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, StateHasher};

use crate::die_mux::DieMuxData;
use crate::effects::{Effect, EffectLog, Name, PowerPlace};

#[derive(Debug, Clone, PartialEq)]
pub struct FxListDie {
    death_fx: Option<String>,
    orient_to_object: bool,
    starts_active: bool,
    mux: DieMuxData,
}

impl FxListDie {
    #[must_use]
    pub fn death_fx(&self) -> Option<&str> {
        self.death_fx.as_deref()
    }

    #[must_use]
    pub const fn orient_to_object(&self) -> bool {
        self.orient_to_object
    }

    #[must_use]
    pub const fn starts_active(&self) -> bool {
        self.starts_active
    }

    #[must_use]
    pub fn plays_on(
        &self,
        entity: mint_sim::Entity,
        death: &mint_sim::module::Death<'_>,
    ) -> Option<&str> {
        if !self.starts_active || !self.mux.gate(entity, "FXListDie", death) {
            return None;
        }
        self.death_fx.as_deref()
    }
}

impl Component for FxListDie {
    fn hash_into(&self, hasher: &mut StateHasher) {
        match &self.death_fx {
            Some(name) => {
                hasher.write_bool(true);
                hasher.write_usize(name.len());
                hasher.write_bytes(name.as_bytes());
            }
            None => hasher.write_bool(false),
        }
        hasher.write_bool(self.orient_to_object);
        hasher.write_bool(self.starts_active);
    }
}

impl ModuleKind for FxListDie {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Die);

    fn on_die(instances: &mut [ModuleSlot<Self>], ctx: &mut DieContext<'_>) {
        let Some(log) = ctx.effects.get::<EffectLog>() else {
            return;
        };
        for slot in instances.iter() {
            if !slot.state.starts_active {
                continue;
            }
            if !slot.state.mux.gate(ctx.entity, "FXListDie", &ctx.death) {
                continue;
            }
            if let Some(name) = &slot.state.death_fx {
                log.push(
                    ctx.entity,
                    Effect::PlayFx {
                        list: Name::from(name.as_str()),
                        at: PowerPlace::Source,
                        on_object: slot.state.orient_to_object,
                        secondary: None,
                    },
                );
            }
        }
    }
}

pub fn fx_list_die_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<FxListDie, FieldError> {
    let death_fx = fields.field("DeathFX").map(str::to_owned);
    let orient_to_object = match fields.field("OrientToObject") {
        None => true,
        Some(raw) => parse_bool(raw)
            .ok_or_else(|| FieldError::new(format!("OrientToObject '{raw}' is not Yes/No")))?,
    };
    let starts_active = match fields.field("StartsActive") {
        None => true,
        Some(raw) => parse_bool(raw)
            .ok_or_else(|| FieldError::new(format!("StartsActive '{raw}' is not Yes/No")))?,
    };
    Ok(FxListDie {
        death_fx,
        orient_to_object,
        starts_active,
        mux: DieMuxData::from_fields(fields)?,
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
    fn defaults_are_eas_own_when_nothing_is_declared() {
        let fields = TestFields(BTreeMap::new());
        let module = fx_list_die_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.death_fx(), None);
        assert!(module.orient_to_object());
        assert!(module.starts_active());
    }

    #[test]
    fn declared_fields_override_the_defaults() {
        let fields = TestFields(BTreeMap::from([
            ("DeathFX", "FX_TreeFall"),
            ("OrientToObject", "No"),
            ("StartsActive", "No"),
        ]));
        let module = fx_list_die_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.death_fx(), Some("FX_TreeFall"));
        assert!(!module.orient_to_object());
        assert!(!module.starts_active());
    }

    #[test]
    fn an_unparseable_bool_is_refused() {
        let fields = TestFields(BTreeMap::from([("OrientToObject", "Sideways")]));
        assert!(fx_list_die_from_fields(&fields, Frame::ZERO).is_err());
    }

    #[test]
    fn on_die_requests_nothing_playing_the_fx_is_presentations_job() {
        let mut world = mint_sim::World::new();
        let mut registry = mint_sim::module::ModuleRegistry::new(&mut world);
        registry.register::<FxListDie>(&mut world, "FXListDie");
        let entity = world.spawn().unwrap();
        registry
            .attach(
                &mut world,
                entity,
                Frame::ZERO,
                FxListDie {
                    death_fx: None,
                    orient_to_object: true,
                    starts_active: true,
                    mux: DieMuxData::default(),
                },
            )
            .unwrap();

        let mut rng = mint_sim::Pcg32::new(1);
        let mut destroy = mint_sim::module::DestroyRequests::new();
        registry.broadcast_die(&mut world, entity, Frame::ZERO, &mut rng, &mut destroy);
        assert!(destroy.drain_sorted().is_empty());
    }

    #[test]
    fn a_declared_death_fx_is_reported_as_an_effect_not_played_directly() {
        let mut world = mint_sim::World::new();
        let mut registry = mint_sim::module::ModuleRegistry::new(&mut world);
        registry.register::<FxListDie>(&mut world, "FXListDie");
        let entity = world.spawn().unwrap();
        registry
            .attach(
                &mut world,
                entity,
                Frame::ZERO,
                FxListDie {
                    death_fx: Some(String::from("FX_TreeFall")),
                    orient_to_object: true,
                    starts_active: true,
                    mux: DieMuxData::default(),
                },
            )
            .unwrap();

        let mut rng = mint_sim::Pcg32::new(1);
        let mut destroy = mint_sim::module::DestroyRequests::new();
        let mut log = EffectLog::new();
        registry.broadcast_die_into(
            &mut world,
            entity,
            Frame::ZERO,
            &mut rng,
            &mut destroy,
            mint_sim::Effects::into_sink(&mut log),
            mint_sim::module::Death::NONE,
        );
        assert!(destroy.drain_sorted().is_empty(), "still requests nothing");
        let entries = log.drain();
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0],
            (
                entity,
                Effect::PlayFx {
                    list: Name::from("FX_TreeFall"),
                    at: PowerPlace::Source,
                    on_object: true,
                    secondary: None,
                }
            )
        );
    }

    #[test]
    fn starts_active_no_reports_nothing() {
        let mut world = mint_sim::World::new();
        let mut registry = mint_sim::module::ModuleRegistry::new(&mut world);
        registry.register::<FxListDie>(&mut world, "FXListDie");
        let entity = world.spawn().unwrap();
        registry
            .attach(
                &mut world,
                entity,
                Frame::ZERO,
                FxListDie {
                    death_fx: Some(String::from("FX_TreeFall")),
                    orient_to_object: true,
                    starts_active: false,
                    mux: DieMuxData::default(),
                },
            )
            .unwrap();

        let mut rng = mint_sim::Pcg32::new(1);
        let mut destroy = mint_sim::module::DestroyRequests::new();
        let mut log = EffectLog::new();
        registry.broadcast_die_into(
            &mut world,
            entity,
            Frame::ZERO,
            &mut rng,
            &mut destroy,
            mint_sim::Effects::into_sink(&mut log),
            mint_sim::module::Death::NONE,
        );
        assert!(log.drain().is_empty());
    }
}
