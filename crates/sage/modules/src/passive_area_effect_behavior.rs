use std::sync::Arc;

use mint_sim::module::{Capability, CapabilityMask, ModuleKind, Sleep, UpdateContext, UpgradeMask};
use mint_sim::{
    Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher, UpgradeContext,
};

use crate::effects::{names, Effect, EffectLog, Name, Names};
use crate::fire_weapon_update::{flag, milliseconds};

#[derive(Debug, PartialEq)]
struct AreaData {
    effect_radius: f32,
    ping_delay: Option<LogicFrames>,
    ping_delay_ms: f32,
    heal_percent_per_second: Option<f32>,
    allow: Names,
    forbid: Names,
    upgrade_required: UpgradeMask,
    nonstackable: bool,
    heal_fx: Option<Name>,
    modifier_names: Vec<Name>,
    anti_categories: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PassiveAreaEffectBehavior {
    data: Arc<AreaData>,
    active: bool,
}

impl PassiveAreaEffectBehavior {
    #[must_use]
    pub fn effect_radius(&self) -> f32 {
        self.data.effect_radius
    }

    #[must_use]
    pub fn ping_delay_ms(&self) -> f32 {
        self.data.ping_delay_ms
    }

    #[must_use]
    pub fn heal_percent_per_second(&self) -> Option<f32> {
        self.data.heal_percent_per_second
    }

    #[must_use]
    pub fn allow(&self) -> &[Name] {
        &self.data.allow
    }

    #[must_use]
    pub fn forbid(&self) -> &[Name] {
        &self.data.forbid
    }

    #[must_use]
    pub fn nonstackable(&self) -> bool {
        self.data.nonstackable
    }

    #[must_use]
    pub fn modifier_names(&self) -> &[Name] {
        &self.data.modifier_names
    }

    #[must_use]
    pub fn anti_categories(&self) -> &[String] {
        &self.data.anti_categories
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }
}

impl Component for PassiveAreaEffectBehavior {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bool(self.active);
    }
}

impl ModuleKind for PassiveAreaEffectBehavior {
    const UPDATES_UNDER: u32 = crate::disabled_masks::NEVER;
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Update)
        .union(CapabilityMask::single(Capability::Upgrade));

    fn on_attach(&mut self, _now: Frame) {
        self.active = self.data.upgrade_required.is_empty();
    }

    fn attempt_upgrade(&mut self, active: &UpgradeMask, _ctx: &mut UpgradeContext<'_>) {
        if !self.active
            && !self.data.upgrade_required.is_empty()
            && active.intersects(&self.data.upgrade_required)
        {
            self.active = true;
        }
    }

    fn would_upgrade(&self, active: &UpgradeMask) -> bool {
        !self.active
            && !self.data.upgrade_required.is_empty()
            && active.intersects(&self.data.upgrade_required)
    }

    fn update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        let data = &self.data;
        let Some(ping) = data.ping_delay else {
            return Sleep::Forever;
        };
        if !self.active || data.effect_radius <= 0.0 {
            return Sleep::Forever;
        }
        let Some(percent) = data.heal_percent_per_second else {
            return Sleep::Forever;
        };
        let per_pulse = percent * (data.ping_delay_ms / 1000.0);
        if per_pulse > 0.0 {
            let entity = ctx.entity;
            if let Some(log) = ctx.effects.get::<EffectLog>() {
                log.push(
                    entity,
                    Effect::AutoHeal {
                        amount: per_pulse,
                        radius: data.effect_radius,
                        kinds: Names::clone(&data.allow),
                        forbidden: Names::clone(&data.forbid),
                        skip_self: true,
                        quiet: LogicFrames::ZERO,
                        fx: data.heal_fx.clone(),
                        sole_for: ping,
                        as_percent: true,
                        nonstackable: data.nonstackable,
                        skip_recently_damaged: true,
                        fx_always: true,
                        rules: crate::effects::HealerRules::default(),
                    },
                );
            }
        }
        Sleep::Frames(ping)
    }
}

fn plus_minus(tokens: Vec<String>) -> (Vec<String>, Vec<String>) {
    let mut allow = Vec::new();
    let mut forbid = Vec::new();
    for token in tokens {
        if let Some(name) = token.strip_prefix('+') {
            allow.push(name.to_owned());
        } else if let Some(name) = token.strip_prefix('-') {
            forbid.push(name.to_owned());
        }
    }
    (allow, forbid)
}

pub fn passive_area_effect_behavior_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<PassiveAreaEffectBehavior, FieldError> {
    let list = |keyword: &str| -> Vec<String> {
        fields
            .field_args(keyword)
            .into_iter()
            .map(str::to_owned)
            .collect()
    };
    let (allow, mut forbid) = plus_minus(list("AllowFilter"));
    forbid.push("IGNORED_IN_GUI".to_owned());
    forbid.push("ARMY_OF_DEAD".to_owned());
    let ping_delay_ms = fields
        .field_numeric("PingDelay")
        .filter(|ms| ms.is_finite() && *ms > 0.0)
        .unwrap_or(0.0);
    let ping_delay = (ping_delay_ms > 0.0).then(|| milliseconds(fields, "PingDelay"));
    let upgrade_required = fields.field_upgrade_mask("UpgradeRequired");
    let data = Arc::new(AreaData {
        effect_radius: fields
            .field_numeric("EffectRadius")
            .filter(|radius| radius.is_finite() && *radius > 0.0)
            .unwrap_or(0.0),
        ping_delay,
        ping_delay_ms,
        heal_percent_per_second: fields
            .field_percent("HealPercentPerSecond")
            .filter(|percent| percent.is_finite() && *percent > 0.0),
        allow: names(allow),
        forbid: names(forbid),
        upgrade_required,
        nonstackable: flag(fields, "NonStackable").unwrap_or(false),
        heal_fx: fields
            .field("HealFX")
            .filter(|name| !name.eq_ignore_ascii_case("NONE"))
            .map(Name::from),
        modifier_names: list("ModifierName").into_iter().map(Name::from).collect(),
        anti_categories: list("AntiCategories"),
    });
    let active = data.upgrade_required.is_empty();
    Ok(PassiveAreaEffectBehavior { data, active })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::float_cmp)]
mod tests {
    use super::*;
    use mint_sim::module::{DestroyRequests, ModuleRegistry, UpgradeBit};
    use mint_sim::{Effects, World};
    use std::collections::BTreeMap;

    struct TestFields(BTreeMap<&'static str, Vec<&'static str>>, UpgradeMask);
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
            if keyword.eq_ignore_ascii_case("UpgradeRequired")
                && self.0.contains_key("UpgradeRequired")
            {
                self.1.clone()
            } else {
                UpgradeMask::EMPTY
            }
        }
    }

    fn fields(pairs: &[(&'static str, &'static str)]) -> TestFields {
        let mut mask = UpgradeMask::EMPTY;
        mask.set(UpgradeBit::new(4));
        TestFields(
            pairs
                .iter()
                .map(|(k, v)| (*k, v.split_whitespace().collect()))
                .collect(),
            mask,
        )
    }

    #[test]
    fn no_fields_at_all_succeeds() {
        let module = passive_area_effect_behavior_from_fields(&fields(&[]), Frame::ZERO).unwrap();
        assert!(module.is_active(), "no UpgradeRequired: active from attach");
        assert_eq!(module.heal_percent_per_second(), None);
    }

    #[test]
    fn a_heal_circle_declaration_reads_its_fields() {
        let module = passive_area_effect_behavior_from_fields(
            &fields(&[
                ("UpgradeRequired", "Upgrade_TestKeepHealingWard"),
                ("EffectRadius", "150"),
                ("PingDelay", "2500"),
                ("HealPercentPerSecond", "4%"),
                (
                    "AllowFilter",
                    "ANY +INFANTRY +HERO -MACHINE +MONSTER -IMMOBILE +DOZER",
                ),
                ("NonStackable", "Yes"),
                ("HealFX", "FX_TestHealPulse"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert!(!module.is_active(), "gated on the upgrade");
        assert_eq!(module.effect_radius(), 150.0);
        assert_eq!(module.ping_delay_ms(), 2500.0);
        assert_eq!(module.heal_percent_per_second(), Some(0.04));
        fn strs(names: &[Name]) -> Vec<&str> {
            names.iter().map(|n| &**n).collect()
        }
        assert_eq!(
            strs(module.allow()),
            ["INFANTRY", "HERO", "MONSTER", "DOZER"]
        );
        assert_eq!(
            strs(module.forbid()),
            ["MACHINE", "IMMOBILE", "IGNORED_IN_GUI", "ARMY_OF_DEAD"]
        );
        assert!(module.nonstackable());
    }

    #[test]
    fn a_standing_buff_declaration_has_no_heal_branch() {
        let module = passive_area_effect_behavior_from_fields(
            &fields(&[
                ("EffectRadius", "300"),
                ("PingDelay", "2000"),
                ("ModifierName", "TestLeadershipBuff"),
                ("AntiCategories", "TESTBUFF"),
                ("AllowFilter", "TEST_BUFF_RECIPIENT_OBJECT_FILTER"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert!(module.is_active(), "no UpgradeRequired");
        assert_eq!(module.heal_percent_per_second(), None);
        assert_eq!(
            module
                .modifier_names()
                .iter()
                .map(|n| &**n)
                .collect::<Vec<_>>(),
            ["TestLeadershipBuff"]
        );
        assert_eq!(module.anti_categories(), ["TESTBUFF"]);
    }

    fn pulses(
        module: PassiveAreaEffectBehavior,
        frames: u64,
        upgrade_at: Option<u64>,
    ) -> Vec<(u64, Effect)> {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<PassiveAreaEffectBehavior>(&mut world, "PassiveAreaEffectBehavior");
        let entity = world.spawn().unwrap();
        registry
            .attach(&mut world, entity, Frame::ZERO, module)
            .unwrap();
        let mut rng = mint_sim::Pcg32::new(1);
        let mut out = Vec::new();
        for frame in 0..frames {
            if upgrade_at == Some(frame) {
                let mut mask = UpgradeMask::EMPTY;
                mask.set(UpgradeBit::new(4));
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
    fn a_well_pulses_from_the_start_and_sole_for_is_always_ping_delay() {
        let module = passive_area_effect_behavior_from_fields(
            &fields(&[
                ("EffectRadius", "200"),
                ("PingDelay", "2000"),
                ("HealPercentPerSecond", "2%"),
                ("AllowFilter", "ANY +INFANTRY"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        let got = pulses(module, 125, None);
        let frames: Vec<u64> = got.iter().map(|(f, _)| *f).collect();
        assert_eq!(frames, vec![0, 60, 120], "2000 ms is 60 frames");
        let Effect::AutoHeal {
            amount,
            radius,
            as_percent,
            sole_for,
            nonstackable,
            skip_self,
            skip_recently_damaged,
            fx_always,
            ..
        } = &got[0].1
        else {
            panic!("{:?}", got[0].1);
        };
        assert_eq!((*amount, *radius, *as_percent), (0.04, 200.0, true));
        assert_eq!(sole_for.get(), 60, "PingDelay always, NonStackable or not");
        assert!(!nonstackable);
        assert!(skip_self, "the gather always excludes the object itself");
        assert!(skip_recently_damaged);
        assert!(fx_always);
    }

    #[test]
    fn a_fortress_waits_for_its_upgrade() {
        let module = passive_area_effect_behavior_from_fields(
            &fields(&[
                ("UpgradeRequired", "Upgrade_TestKeepHealingWard"),
                ("EffectRadius", "200"),
                ("PingDelay", "2000"),
                ("HealPercentPerSecond", "3%"),
                ("AllowFilter", "ANY +INFANTRY"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert!(pulses(module.clone(), 200, None).is_empty());
        let got = pulses(module, 200, Some(70));
        let frames: Vec<u64> = got.iter().map(|(f, _)| *f).collect();
        assert_eq!(
            frames,
            vec![70, 130, 190],
            "on from the frame the upgrade arrives"
        );
    }

    #[test]
    fn instance_state_reaches_the_hash() {
        let base = passive_area_effect_behavior_from_fields(&fields(&[]), Frame::ZERO).unwrap();
        let hash = |module: &PassiveAreaEffectBehavior| {
            let mut h = StateHasher::new();
            module.hash_into(&mut h);
            h.finish()
        };
        let mut inactive = base.clone();
        inactive.active = false;
        assert_ne!(hash(&base), hash(&inactive));
    }
}
