use std::sync::Arc;

use mint_sim::module::{Capability, CapabilityMask, UpgradeMask};
use mint_sim::{Component, FieldError, Frame, ModuleFieldSource, ModuleKind, StateHasher};

use crate::effects::{Effect, Name};
use crate::upgrade_mux::{attempt_upgrade_via_mux, UpgradeEffect, UpgradeMuxData};

const MAX_RADIUS: f32 = 100_000.0;

#[derive(Debug, PartialEq)]
struct Data {
    upgrade: Option<Name>,
    wall_upgrade_radius: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CastleUpgrade {
    data: Arc<Data>,
    mux: UpgradeMuxData,
}

impl CastleUpgrade {
    #[must_use]
    pub fn upgrade(&self) -> Option<&str> {
        self.data.upgrade.as_deref()
    }

    #[must_use]
    pub fn wall_upgrade_radius(&self) -> Option<f32> {
        self.data.wall_upgrade_radius
    }
}

impl UpgradeEffect for CastleUpgrade {
    fn mux_mut(&mut self) -> &mut UpgradeMuxData {
        &mut self.mux
    }

    fn on_fire(&mut self, _active: &UpgradeMask, out: &mut Vec<Effect>) {
        if let Some(upgrade) = &self.data.upgrade {
            out.push(Effect::CastleUpgrade {
                upgrade: Name::clone(upgrade),
                wall_radius: self.data.wall_upgrade_radius,
            });
        }
    }
}

impl Component for CastleUpgrade {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.mux.hash_into(hasher);
    }
}

impl ModuleKind for CastleUpgrade {
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

fn radius(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<Option<f32>, FieldError> {
    let Some(raw) = fields.field(keyword) else {
        return Ok(None);
    };
    fields
        .field_numeric(keyword)
        .filter(|value| (0.0..=MAX_RADIUS).contains(value))
        .map(Some)
        .ok_or_else(|| {
            FieldError::new(format!(
                "{keyword} '{raw}' is not a number from 0 to {MAX_RADIUS}"
            ))
        })
}

pub fn castle_upgrade_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<CastleUpgrade, FieldError> {
    let data = Data {
        upgrade: fields.field("Upgrade").map(Name::from),
        wall_upgrade_radius: radius(fields, "WallUpgradeRadius")?,
    };
    Ok(CastleUpgrade {
        data: Arc::new(data),
        mux: UpgradeMuxData::from_fields(fields)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::EffectLog;
    use mint_sim::module::{ModuleRegistry, UpgradeBit};
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

    fn fields(pairs: &[(&'static str, &[&'static str])]) -> TestFields {
        TestFields(pairs.iter().map(|(k, v)| (*k, v.to_vec())).collect())
    }

    #[test]
    fn a_trigger_reports_the_castle_hand_out_with_its_wall_radius() {
        let module = castle_upgrade_from_fields(
            &fields(&[
                ("TriggeredBy", &["3"]),
                ("Upgrade", &["Upgrade_ModdedStonework"]),
                ("WallUpgradeRadius", &["900"]),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<CastleUpgrade>(&mut world, "CastleUpgrade");
        let entity = world.spawn().unwrap();
        registry
            .attach(&mut world, entity, Frame::ZERO, module)
            .unwrap();
        let mut trigger = UpgradeMask::EMPTY;
        trigger.set(UpgradeBit::new(3));
        let mut log = EffectLog::new();
        registry.broadcast_upgrade_into(
            &mut world,
            entity,
            &trigger,
            Frame::ZERO,
            Effects::into_sink(&mut log),
        );
        let effects: Vec<Effect> = log.drain().into_iter().map(|(_, e)| e).collect();
        assert_eq!(effects.len(), 1);
        match &effects[0] {
            Effect::CastleUpgrade {
                upgrade,
                wall_radius,
            } => {
                assert_eq!(&**upgrade, "Upgrade_ModdedStonework");
                assert_eq!(*wall_radius, Some(900.0));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn a_wall_radius_that_is_not_a_number_is_refused() {
        assert!(castle_upgrade_from_fields(
            &fields(&[
                ("TriggeredBy", &["1"]),
                ("Upgrade", &["Upgrade_X"]),
                ("WallUpgradeRadius", &["wide"]),
            ]),
            Frame::ZERO,
        )
        .is_err());
    }
}
