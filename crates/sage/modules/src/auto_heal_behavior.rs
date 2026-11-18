use std::sync::Arc;

use mint_sim::module::{Capability, CapabilityMask, ModuleKind, Sleep, UpdateContext, UpgradeMask};
use mint_sim::{
    Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher, UpgradeContext,
};

use crate::effects::{names, Effect, EffectLog, Name, Names};
use crate::fire_weapon_update::{flag, milliseconds};
use crate::upgrade_mux::{sooner, UpgradeMuxData};

#[derive(Debug, PartialEq)]
struct HealData {
    starts_active: bool,
    amount: f32,
    delay: Option<LogicFrames>,
    start_delay: LogicFrames,
    radius: f32,
    kinds: Names,
    forbidden: Names,
    skip_self: bool,
    single_burst: bool,
    fx: Option<Name>,
    button_triggered: bool,
    affects_contained: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AutoHealBehavior {
    data: Arc<HealData>,
    mux: UpgradeMuxData,
    active: bool,
    phased: bool,
}

impl AutoHealBehavior {
    #[must_use]
    pub fn amount(&self) -> f32 {
        self.data.amount
    }

    #[must_use]
    pub fn delay(&self) -> Option<LogicFrames> {
        self.data.delay
    }

    #[must_use]
    pub fn start_delay(&self) -> LogicFrames {
        self.data.start_delay
    }

    #[must_use]
    pub fn radius(&self) -> f32 {
        self.data.radius
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }
}

impl Component for AutoHealBehavior {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bool(self.active);
        hasher.write_bool(self.phased);
        self.mux.hash_into(hasher);
    }
}

impl ModuleKind for AutoHealBehavior {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Update)
        .union(CapabilityMask::single(Capability::Upgrade));

    fn on_attach(&mut self, _now: Frame) {
        self.active = self.data.starts_active;
        self.phased = false;
    }

    fn attempt_upgrade(&mut self, active: &UpgradeMask, ctx: &mut UpgradeContext<'_>) {
        if self.mux.fire(active, ctx) {
            self.active = true;
        }
    }

    fn would_upgrade(&self, active: &UpgradeMask) -> bool {
        self.mux.would_fire(active)
    }

    fn update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        let anim = self.mux.update(ctx);
        sooner(anim, self.own_update(ctx))
    }
}

impl AutoHealBehavior {
    fn own_update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        let data = &self.data;
        if data.button_triggered || data.affects_contained || data.amount <= 0.0 {
            return Sleep::Forever;
        }
        let pulse = data.delay.map_or(Sleep::Forever, Sleep::Frames);
        if !self.active {
            return data.delay.map_or(
                Sleep::Frames(LogicFrames::from_engine_milliseconds(1000)),
                Sleep::Frames,
            );
        }
        if !self.phased {
            self.phased = true;
            let span = data.delay.map_or(0, LogicFrames::get);
            if span > 1 {
                let phase = ctx.rng.below(span).saturating_add(1);
                return Sleep::Frames(LogicFrames::from_frames(phase));
            }
        }
        let entity = ctx.entity;
        if let Some(log) = ctx.effects.get::<EffectLog>() {
            log.push(
                entity,
                Effect::AutoHeal {
                    amount: data.amount,
                    radius: data.radius,
                    kinds: Names::clone(&data.kinds),
                    forbidden: Names::clone(&data.forbidden),
                    skip_self: data.skip_self,
                    quiet: data.start_delay,
                    fx: data.fx.clone(),
                    sole_for: data.delay.unwrap_or(LogicFrames::ONE),
                    as_percent: false,
                    nonstackable: false,
                    skip_recently_damaged: false,
                    fx_always: false,
                },
            );
        }
        if data.single_burst && data.radius > 0.0 {
            return Sleep::Forever;
        }
        pulse
    }
}

pub fn auto_heal_behavior_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<AutoHealBehavior, FieldError> {
    let list = |keyword: &str| -> Names {
        names(
            fields
                .field_args(keyword)
                .into_iter()
                .map(str::to_owned)
                .collect(),
        )
    };
    let delay = milliseconds(fields, "HealingDelay");
    let starts_active = flag(fields, "StartsActive").unwrap_or(false);
    Ok(AutoHealBehavior {
        data: Arc::new(HealData {
            starts_active,
            amount: fields
                .field_numeric("HealingAmount")
                .filter(|amount| amount.is_finite())
                .unwrap_or(0.0),
            delay: fields.field("HealingDelay").map(|_| {
                if delay.is_zero() {
                    LogicFrames::ONE
                } else {
                    delay
                }
            }),
            start_delay: milliseconds(fields, "StartHealingDelay"),
            radius: fields
                .field_numeric("Radius")
                .filter(|radius| radius.is_finite() && *radius > 0.0)
                .unwrap_or(0.0),
            kinds: list("KindOf"),
            forbidden: list("ForbiddenKindOf"),
            skip_self: flag(fields, "SkipSelfForHealing")
                .or_else(|| flag(fields, "HealOnlyOthers"))
                .unwrap_or(false),
            single_burst: flag(fields, "SingleBurst").unwrap_or(false),
            fx: fields
                .field("UnitHealPulseFX")
                .filter(|name| !name.eq_ignore_ascii_case("NONE"))
                .map(Name::from),
            button_triggered: flag(fields, "ButtonTriggered").unwrap_or(false),
            affects_contained: flag(fields, "AffectsContained").unwrap_or(false),
        }),
        mux: UpgradeMuxData::from_fields(fields)?,
        active: starts_active,
        phased: false,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::float_cmp)]
mod tests {
    use super::*;
    use mint_sim::module::{DestroyRequests, ModuleRegistry, UpgradeBit};
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
            if keyword.eq_ignore_ascii_case("TriggeredBy") && self.0.contains_key("TriggeredBy") {
                mask.set(UpgradeBit::new(2));
            }
            mask
        }
    }

    fn fields(pairs: &[(&'static str, &'static str)]) -> TestFields {
        TestFields(
            pairs
                .iter()
                .map(|(k, v)| (*k, v.split_whitespace().collect()))
                .collect(),
        )
    }

    fn heals(module: AutoHealBehavior, frames: u64, upgrade_at: Option<u64>) -> Vec<(u64, Effect)> {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<AutoHealBehavior>(&mut world, "AutoHealBehavior");
        let entity = world.spawn().unwrap();
        registry
            .attach(&mut world, entity, Frame::ZERO, module)
            .unwrap();
        let mut rng = mint_sim::Pcg32::new(7);
        let mut out = Vec::new();
        for frame in 0..frames {
            if upgrade_at == Some(frame) {
                let mut mask = UpgradeMask::EMPTY;
                mask.set(UpgradeBit::new(2));
                registry.broadcast_upgrade(&mut world, entity, &mask, Frame::new(frame));
            }
            let mut log = EffectLog::new();
            registry.update_into(
                &mut world,
                Frame::new(frame),
                &mut rng,
                &mut DestroyRequests::new(),
                Effects::into_sink(&mut log),
            );
            out.extend(log.drain().into_iter().map(|(_, e)| (frame, e)));
        }
        out
    }

    #[test]
    fn a_hero_heals_itself_every_healing_delay_after_a_random_phase() {
        let module = auto_heal_behavior_from_fields(
            &fields(&[
                ("StartsActive", "Yes"),
                ("HealingAmount", "30"),
                ("HealingDelay", "1000"),
                ("StartHealingDelay", "15000"),
                ("HealOnlyIfNotInCombat", "Yes"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        let got = heals(module, 160, None);
        let frames: Vec<u64> = got.iter().map(|(f, _)| *f).collect();
        assert!(frames.len() >= 4, "{frames:?}");
        assert!((1..=30).contains(&frames[0]), "phased 1..=30: {frames:?}");
        for pair in frames.windows(2) {
            assert_eq!(pair[1] - pair[0], 30, "one heal a second: {frames:?}");
        }
        let Effect::AutoHeal {
            amount,
            radius,
            quiet,
            skip_self,
            ..
        } = &got[0].1
        else {
            panic!("{:?}", got[0].1);
        };
        assert_eq!(
            (*amount, *radius, quiet.get(), *skip_self),
            (30.0, 0.0, 450, false)
        );
    }

    #[test]
    fn an_upgrade_switches_healing_on() {
        let module = auto_heal_behavior_from_fields(
            &fields(&[
                ("StartsActive", "No"),
                ("TriggeredBy", "Upgrade_ElvenGift"),
                ("HealingAmount", "5"),
                ("HealingDelay", "1000"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert!(heals(module.clone(), 200, None).is_empty());
        let got = heals(module, 200, Some(40));
        assert!(!got.is_empty());
        assert!(got[0].0 > 40, "{:?}", got[0].0);
    }

    #[test]
    fn a_single_burst_circle_heals_once() {
        let module = auto_heal_behavior_from_fields(
            &fields(&[
                ("StartsActive", "Yes"),
                ("HealingAmount", "100"),
                ("HealingDelay", "500"),
                ("Radius", "200"),
                ("KindOf", "INFANTRY CAVALRY"),
                ("SingleBurst", "Yes"),
                ("UnitHealPulseFX", "FX_Heal"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        let got = heals(module, 200, None);
        assert_eq!(got.len(), 1, "{got:?}");
        let Effect::AutoHeal {
            radius, kinds, fx, ..
        } = &got[0].1
        else {
            panic!();
        };
        assert_eq!(*radius, 200.0);
        assert_eq!(kinds.len(), 2);
        assert_eq!(fx.as_deref(), Some("FX_Heal"));
    }

    #[test]
    fn a_button_or_container_heal_never_runs() {
        for extra in [("ButtonTriggered", "Yes"), ("AffectsContained", "Yes")] {
            let module = auto_heal_behavior_from_fields(
                &fields(&[
                    ("StartsActive", "Yes"),
                    ("HealingAmount", "10"),
                    ("HealingDelay", "100"),
                    extra,
                ]),
                Frame::ZERO,
            )
            .unwrap();
            assert!(heals(module, 100, None).is_empty(), "{extra:?}");
        }
    }

    #[test]
    fn instance_state_reaches_the_hash() {
        let base = auto_heal_behavior_from_fields(
            &fields(&[("HealingAmount", "10"), ("HealingDelay", "100")]),
            Frame::ZERO,
        )
        .unwrap();
        let hash = |module: &AutoHealBehavior| {
            let mut h = StateHasher::new();
            module.hash_into(&mut h);
            h.finish()
        };
        let mut on = base.clone();
        on.active = true;
        let mut phased = base.clone();
        phased.phased = true;
        assert_ne!(hash(&base), hash(&on));
        assert_ne!(hash(&base), hash(&phased));
        assert_ne!(hash(&on), hash(&phased));
    }
}
