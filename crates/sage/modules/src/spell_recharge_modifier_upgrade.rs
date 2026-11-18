use std::sync::Arc;

use mint_sage_ini::value::{parse_bool, parse_percent};
use mint_sim::module::{
    Capability, CapabilityMask, CaptureContext, CreateContext, Sleep, UpdateContext, UpgradeMask,
};
use mint_sim::{
    Component, FieldError, Frame, ModuleFieldSource, ModuleKind, PlayerId, StateHasher,
    UpgradeContext,
};

use crate::effects::{Effect, EffectLog};
use crate::upgrade_mux::{attempt_upgrade_via_mux, UpgradeEffect, UpgradeMuxData};

#[derive(Debug, PartialEq)]
struct Data {
    percentages: Arc<[f32]>,
    starts_active: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpellRechargeModifierUpgrade {
    data: Arc<Data>,
    mux: UpgradeMuxData,
    active: bool,
}

impl SpellRechargeModifierUpgrade {
    #[must_use]
    pub fn percentages(&self) -> &[f32] {
        &self.data.percentages
    }

    fn count(&self, player: Option<PlayerId>, delta: i32) -> Effect {
        Effect::SpellRechargeCount {
            player,
            delta,
            percentages: Arc::clone(&self.data.percentages),
        }
    }
}

impl Component for SpellRechargeModifierUpgrade {
    fn hash_into(&self, hasher: &mut StateHasher) {
        self.mux.hash_into(hasher);
        hasher.write_bool(self.active);
    }
}

impl UpgradeEffect for SpellRechargeModifierUpgrade {
    fn mux_mut(&mut self) -> &mut UpgradeMuxData {
        &mut self.mux
    }

    fn on_fire(&mut self, _active: &UpgradeMask, out: &mut Vec<Effect>) {
        if !self.active {
            self.active = true;
            out.push(self.count(None, 1));
        }
    }
}

impl ModuleKind for SpellRechargeModifierUpgrade {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Upgrade)
        .union(CapabilityMask::single(Capability::Create))
        .union(CapabilityMask::single(Capability::Update));

    fn update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        self.mux.update(ctx)
    }

    fn on_create(&mut self, ctx: &mut CreateContext<'_>) {
        if !self.data.starts_active || self.active {
            return;
        }
        self.active = true;
        if let Some(log) = ctx.effects.get::<EffectLog>() {
            log.push(ctx.entity, self.count(None, 1));
        }
    }

    fn attempt_upgrade(&mut self, active: &UpgradeMask, ctx: &mut UpgradeContext<'_>) {
        attempt_upgrade_via_mux(self, active, ctx);
    }

    fn would_upgrade(&self, active: &UpgradeMask) -> bool {
        !self.active && self.mux.would_fire(active)
    }

    fn on_capture(&mut self, old: PlayerId, new: PlayerId, ctx: &mut CaptureContext<'_>) {
        if let Some(log) = ctx.effects.get::<EffectLog>() {
            log.push(ctx.entity, self.count(Some(old), -1));
            log.push(ctx.entity, self.count(Some(new), 1));
        }
    }
}

pub fn spell_recharge_modifier_upgrade_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<SpellRechargeModifierUpgrade, FieldError> {
    let mut percentages = Vec::new();
    for args in fields.field_args_each("Percentage") {
        let [token] = args.as_slice() else {
            return Err(FieldError::new(format!(
                "Percentage '{}' is not one percentage",
                args.join(" ")
            )));
        };
        let value = parse_percent(token)
            .ok_or_else(|| FieldError::new(format!("Percentage '{token}' is not a percentage")))?;
        percentages.push(value);
    }
    let starts_active = match fields.field("StartsActive") {
        None => false,
        Some(raw) => parse_bool(raw)
            .ok_or_else(|| FieldError::new(format!("StartsActive '{raw}' is not Yes/No")))?,
    };
    Ok(SpellRechargeModifierUpgrade {
        data: Arc::new(Data {
            percentages: percentages.into(),
            starts_active,
        }),
        mux: UpgradeMuxData::from_fields(fields)?,
        active: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct Fields(BTreeMap<&'static str, Vec<&'static str>>);

    impl ModuleFieldSource for Fields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0
                .get(keyword)
                .and_then(|values| values.first().copied())
        }
        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.field(keyword).into_iter().collect()
        }
        fn field_args_each(&self, keyword: &str) -> Vec<Vec<&str>> {
            self.0
                .get(keyword)
                .map(|values| values.iter().map(|value| vec![*value]).collect())
                .unwrap_or_default()
        }
    }

    fn fields(pairs: &[(&'static str, &[&'static str])]) -> Fields {
        Fields(pairs.iter().map(|(k, v)| (*k, v.to_vec())).collect())
    }

    fn created(module: &mut SpellRechargeModifierUpgrade) -> Vec<Effect> {
        let mut world = mint_sim::World::new();
        let entity = world.spawn().unwrap();
        let mut log = EffectLog::new();
        let mut ctx = CreateContext {
            entity,
            now: Frame::ZERO,
            effects: mint_sim::Effects::into_sink(&mut log),
        };
        module.on_create(&mut ctx);
        log.drain().into_iter().map(|(_, effect)| effect).collect()
    }

    #[test]
    fn reads_every_percentage_in_order() {
        let module = spell_recharge_modifier_upgrade_from_fields(
            &fields(&[("Percentage", &["-10%", "-30%", "-45%"])]),
            Frame::ZERO,
        )
        .unwrap();
        assert_eq!(module.percentages(), &[-0.1, -0.3, -0.45]);
    }

    #[test]
    fn a_malformed_percentage_is_refused() {
        assert!(spell_recharge_modifier_upgrade_from_fields(
            &fields(&[("Percentage", &["quick"])]),
            Frame::ZERO
        )
        .is_err());
    }

    #[test]
    fn starts_active_counts_once_on_creation() {
        let mut module = spell_recharge_modifier_upgrade_from_fields(
            &fields(&[("StartsActive", &["Yes"]), ("Percentage", &["-10%"])]),
            Frame::ZERO,
        )
        .unwrap();
        let first = created(&mut module);
        assert!(matches!(
            first.as_slice(),
            [Effect::SpellRechargeCount {
                player: None,
                delta: 1,
                ..
            }]
        ));
        assert!(created(&mut module).is_empty());
    }

    #[test]
    fn without_starts_active_creation_counts_nothing() {
        let mut module = spell_recharge_modifier_upgrade_from_fields(
            &fields(&[("Percentage", &["-10%"])]),
            Frame::ZERO,
        )
        .unwrap();
        assert!(created(&mut module).is_empty());
    }
}
