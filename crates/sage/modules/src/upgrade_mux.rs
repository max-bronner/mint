use std::sync::Arc;

use mint_sage_ini::value::parse_bool;
use mint_sim::module::{Sleep, UpdateContext, UpgradeMask};
use mint_sim::{FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher, UpgradeContext};

use crate::effects::{Effect, EffectLog, Name};
use crate::special_ability_update::{custom_anim_and_duration, CustomAnim};

#[derive(Debug, Clone, PartialEq)]
struct Triggers {
    triggered_by: UpgradeMask,
    conflicts_with: UpgradeMask,
    requires_all_triggers: bool,
    removes_upgrades: UpgradeMask,
    custom_anim: Option<CustomAnim>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UpgradeMuxData {
    triggers: Arc<Triggers>,
    fired: bool,
    clears: Vec<(Frame, Name)>,
}

impl UpgradeMuxData {
    pub fn from_fields(fields: &dyn ModuleFieldSource) -> Result<Self, FieldError> {
        let requires_all_triggers = match fields.field("RequiresAllTriggers") {
            None => false,
            Some(raw) => parse_bool(raw).ok_or_else(|| {
                FieldError::new(format!("RequiresAllTriggers '{raw}' is not Yes/No"))
            })?,
        };
        Ok(Self {
            triggers: Arc::new(Triggers {
                triggered_by: fields.field_upgrade_mask("TriggeredBy"),
                conflicts_with: fields.field_upgrade_mask("ConflictsWith"),
                requires_all_triggers,
                removes_upgrades: fields.field_upgrade_mask("RemovesUpgrades"),
                custom_anim: custom_anim_and_duration(fields)?,
            }),
            fired: false,
            clears: Vec::new(),
        })
    }

    pub fn fire(&mut self, active: &UpgradeMask, ctx: &mut UpgradeContext<'_>) -> bool {
        if !self.try_fire(active) {
            return false;
        }
        let Some(anim) = self.triggers.custom_anim.clone() else {
            return true;
        };
        if let Some(lasts) = anim.lasts {
            self.clear_after(Name::clone(&anim.state), ctx.now, lasts);
        }
        if let Some(log) = ctx.effects.get::<EffectLog>() {
            log.push(
                ctx.entity,
                Effect::ModelConditions {
                    add: vec![anim.state].into(),
                    remove: Vec::new().into(),
                },
            );
        }
        true
    }

    pub(crate) fn clear_after(&mut self, flag: Name, now: Frame, lasts: LogicFrames) {
        let at = now.checked_add(lasts).unwrap_or(Frame::FOREVER);
        self.clears.retain(|(_, pending)| *pending != flag);
        self.clears.push((at, flag));
    }

    pub fn update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        let now = ctx.now;
        let due: Vec<Name> = self
            .clears
            .iter()
            .filter(|(at, _)| *at <= now)
            .map(|(_, flag)| Name::clone(flag))
            .collect();
        if !due.is_empty() {
            self.clears.retain(|(at, _)| *at > now);
            if let Some(log) = ctx.effects.get::<EffectLog>() {
                log.push(
                    ctx.entity,
                    Effect::ModelConditions {
                        add: Vec::new().into(),
                        remove: due.into(),
                    },
                );
            }
        }
        self.clears
            .iter()
            .filter_map(|(at, _)| at.since(now))
            .min()
            .map_or(Sleep::Forever, Sleep::Frames)
    }

    pub fn try_fire(&mut self, active: &UpgradeMask) -> bool {
        let satisfied = self.would_fire(active);
        if satisfied {
            self.fired = true;
        }
        satisfied
    }

    #[must_use]
    pub fn would_fire(&self, active: &UpgradeMask) -> bool {
        if self.fired {
            return false;
        }
        let triggers = &self.triggers;
        if triggers.triggered_by.is_empty() || active.is_empty() {
            return false;
        }
        if active.intersects(&triggers.conflicts_with) {
            return false;
        }
        if triggers.requires_all_triggers {
            active.contains_all(&triggers.triggered_by)
        } else {
            active.intersects(&triggers.triggered_by)
        }
    }

    #[must_use]
    pub const fn has_fired(&self) -> bool {
        self.fired
    }

    pub fn reset_if_triggered_by(&mut self, removed: &UpgradeMask) {
        if self.triggers.triggered_by.intersects(removed) {
            self.fired = false;
        }
    }

    #[must_use]
    pub fn triggered_by(&self) -> &UpgradeMask {
        &self.triggers.triggered_by
    }

    #[must_use]
    pub fn removes_upgrades(&self) -> &UpgradeMask {
        &self.triggers.removes_upgrades
    }

    pub fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bool(self.fired);
        hasher.write_usize(self.clears.len());
        for (at, flag) in &self.clears {
            at.hash_into(hasher);
            hasher.write_bytes(flag.as_bytes());
        }
    }
}

#[must_use]
pub(crate) fn sooner(a: Sleep, b: Sleep) -> Sleep {
    match (a, b) {
        (Sleep::NextFrame, _) | (_, Sleep::NextFrame) => Sleep::NextFrame,
        (Sleep::Forever, other) | (other, Sleep::Forever) => other,
        (Sleep::Frames(a), Sleep::Frames(b)) => Sleep::Frames(a.min(b)),
    }
}

pub trait UpgradeEffect {
    fn mux_mut(&mut self) -> &mut UpgradeMuxData;

    fn on_fire(&mut self, active: &UpgradeMask, out: &mut Vec<Effect>) {
        let _ = (active, out);
    }
}

pub fn attempt_upgrade_via_mux<T: UpgradeEffect>(
    module: &mut T,
    active: &UpgradeMask,
    ctx: &mut UpgradeContext<'_>,
) {
    if !module.mux_mut().fire(active, ctx) {
        return;
    }
    let mut out = Vec::new();
    module.on_fire(active, &mut out);
    let removes = module.mux_mut().removes_upgrades();
    if !removes.is_empty() {
        out.push(Effect::RemoveUpgrades(removes.clone(), false));
    }
    if let Some(log) = ctx.effects.get::<EffectLog>() {
        for effect in out {
            log.push(ctx.entity, effect);
        }
    }
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

    fn fields(pairs: &[(&'static str, &[&'static str])]) -> TestFields {
        TestFields(pairs.iter().map(|(k, v)| (*k, v.to_vec())).collect())
    }

    #[test]
    fn a_module_with_no_triggered_by_never_fires() {
        let mut mux = UpgradeMuxData::from_fields(&fields(&[])).unwrap();
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(0));
        assert!(!mux.try_fire(&active));
        assert!(!mux.has_fired());
    }

    #[test]
    fn any_trigger_satisfies_by_default() {
        let mut mux =
            UpgradeMuxData::from_fields(&fields(&[("TriggeredBy", &["1", "2"])])).unwrap();
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(2));
        assert!(mux.try_fire(&active));
        assert!(mux.has_fired());
    }

    #[test]
    fn requires_all_triggers_refuses_a_partial_match() {
        let mut mux = UpgradeMuxData::from_fields(&fields(&[
            ("TriggeredBy", &["1", "2"]),
            ("RequiresAllTriggers", &["Yes"]),
        ]))
        .unwrap();
        let mut partial = UpgradeMask::EMPTY;
        partial.set(UpgradeBit::new(1));
        assert!(!mux.try_fire(&partial));

        let mut full = partial.clone();
        full.set(UpgradeBit::new(2));
        assert!(mux.try_fire(&full));
    }

    #[test]
    fn a_conflicting_bit_refuses_even_when_triggered_by_is_satisfied() {
        let mut mux = UpgradeMuxData::from_fields(&fields(&[
            ("TriggeredBy", &["1"]),
            ("ConflictsWith", &["9"]),
        ]))
        .unwrap();
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(1));
        active.set(UpgradeBit::new(9));
        assert!(!mux.try_fire(&active));
    }

    #[test]
    fn would_fire_answers_without_latching() {
        let mut mux = UpgradeMuxData::from_fields(&fields(&[
            ("TriggeredBy", &["1"]),
            ("ConflictsWith", &["9"]),
        ]))
        .unwrap();
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(1));
        assert!(mux.would_fire(&active));
        assert!(mux.would_fire(&active), "asking twice is still yes");
        assert!(!mux.has_fired());
        let mut conflicting = active.clone();
        conflicting.set(UpgradeBit::new(9));
        assert!(!mux.would_fire(&conflicting));
        assert!(mux.try_fire(&active));
        assert!(!mux.would_fire(&active), "a fired mixin would not again");
    }

    #[test]
    fn it_fires_exactly_once_a_second_call_with_the_same_mask_returns_false() {
        let mut mux = UpgradeMuxData::from_fields(&fields(&[("TriggeredBy", &["1"])])).unwrap();
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(1));
        assert!(mux.try_fire(&active));
        assert!(!mux.try_fire(&active), "must not fire twice");
        assert!(mux.has_fired());
    }

    #[test]
    fn removing_a_triggering_bit_re_arms_the_latch_for_a_second_fire() {
        let mut mux = UpgradeMuxData::from_fields(&fields(&[("TriggeredBy", &["1"])])).unwrap();
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(1));

        assert!(mux.try_fire(&active), "fires the first time");
        assert!(!mux.try_fire(&active), "latched, refuses a second call");

        let mut removed = UpgradeMask::EMPTY;
        removed.set(UpgradeBit::new(1));
        mux.reset_if_triggered_by(&removed);

        assert!(mux.try_fire(&active), "re-armed, fires again");
    }

    #[test]
    fn removing_an_unrelated_bit_does_not_re_arm() {
        let mut mux = UpgradeMuxData::from_fields(&fields(&[("TriggeredBy", &["1"])])).unwrap();
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(1));
        assert!(mux.try_fire(&active));

        let mut unrelated = UpgradeMask::EMPTY;
        unrelated.set(UpgradeBit::new(9));
        mux.reset_if_triggered_by(&unrelated);

        assert!(!mux.try_fire(&active), "still latched");
    }

    #[test]
    fn an_unparseable_requires_all_triggers_is_refused() {
        let err = UpgradeMuxData::from_fields(&fields(&[("RequiresAllTriggers", &["Sideways"])]));
        assert!(err.is_err());
    }

    #[test]
    fn a_malformed_custom_anim_and_duration_is_refused() {
        for bad in [
            &["AnimState:USER_3"][..],
            &["AnimTime:0", "AnimState:USER_3"],
            &["AnimState:", "AnimTime:0"],
            &["AnimState:USER_3", "AnimTime:soon"],
            &["AnimState:USER_3", "AnimTime:-5"],
            &["AnimState:USER_3", "AnimTime:0", "Delay:0"],
            &[
                "AnimState:USER_3",
                "AnimTime:0",
                "TriggerTime:0",
                "AnimTime:0",
            ],
        ] {
            let parsed = UpgradeMuxData::from_fields(&fields(&[("CustomAnimAndDuration", bad)]));
            assert!(parsed.is_err(), "{bad:?} must refuse");
        }
    }

    #[derive(Debug, Clone, PartialEq)]
    struct Bare(UpgradeMuxData);
    impl UpgradeEffect for Bare {
        fn mux_mut(&mut self) -> &mut UpgradeMuxData {
            &mut self.0
        }
    }
    impl mint_sim::Component for Bare {
        fn hash_into(&self, hasher: &mut StateHasher) {
            self.0.hash_into(hasher);
        }
    }
    impl mint_sim::ModuleKind for Bare {
        const CAPABILITIES: mint_sim::module::CapabilityMask =
            mint_sim::module::CapabilityMask::single(mint_sim::module::Capability::Upgrade).union(
                mint_sim::module::CapabilityMask::single(mint_sim::module::Capability::Update),
            );
        fn attempt_upgrade(&mut self, active: &UpgradeMask, ctx: &mut UpgradeContext<'_>) {
            attempt_upgrade_via_mux(self, active, ctx);
        }
        fn update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
            self.0.update(ctx)
        }
    }

    fn conditions_by_frame(anim: &[&'static str]) -> Vec<(u64, String, String)> {
        use mint_sim::module::{DestroyRequests, ModuleRegistry};
        let mut declared = vec![("TriggeredBy", &["1"][..])];
        if !anim.is_empty() {
            declared.push(("CustomAnimAndDuration", anim));
        }
        let module = Bare(UpgradeMuxData::from_fields(&fields(&declared)).unwrap());
        let mut world = mint_sim::World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<Bare>(&mut world, "Bare");
        let entity = world.spawn().unwrap();
        registry
            .attach(&mut world, entity, Frame::ZERO, module)
            .unwrap();
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(1));
        let mut rng = mint_sim::Pcg32::new(7);
        let mut out = Vec::new();
        for frame in 0..200 {
            let mut log = EffectLog::new();
            if frame == 2 {
                registry.broadcast_upgrade_into(
                    &mut world,
                    entity,
                    &active,
                    Frame::new(frame),
                    mint_sim::Effects::into_sink(&mut log),
                );
            }
            registry.update_into(
                &mut world,
                Frame::new(frame),
                &mut rng,
                &mut DestroyRequests::new(),
                mint_sim::Effects::into_sink(&mut log),
            );
            for (_, effect) in log.drain() {
                if let Effect::ModelConditions { add, remove } = effect {
                    out.push((frame, add.join(" "), remove.join(" ")));
                }
            }
        }
        out
    }

    #[test]
    fn firing_raises_the_custom_anim_state_for_its_anim_time() {
        let raised = (2, "USER_7".to_owned(), String::new());
        assert_eq!(conditions_by_frame(&[]), []);
        assert_eq!(
            conditions_by_frame(&["AnimState:USER_7", "AnimTime:0", "TriggerTime:0"]),
            std::slice::from_ref(&raised)
        );
        assert_eq!(
            conditions_by_frame(&["AnimState:USER_7", "AnimTime:1000"]),
            [raised, (32, String::new(), "USER_7".to_owned())]
        );
    }

    #[derive(Debug, Default)]
    struct EffectToy {
        mux: Option<UpgradeMuxData>,
        fire_count: u32,
    }
    impl UpgradeEffect for EffectToy {
        fn mux_mut(&mut self) -> &mut UpgradeMuxData {
            self.mux.as_mut().expect("mux set by the test")
        }
        fn on_fire(&mut self, _active: &UpgradeMask, out: &mut Vec<Effect>) {
            self.fire_count = self.fire_count.saturating_add(1);
            out.push(Effect::CommandSet("Toy".into()));
        }
    }

    fn offer(toy: &mut EffectToy, active: &UpgradeMask, log: &mut EffectLog) {
        let mut world = mint_sim::World::new();
        let mut ctx = UpgradeContext {
            entity: world.spawn().unwrap(),
            now: mint_sim::Frame::ZERO,
            effects: mint_sim::Effects::into_sink(log),
        };
        attempt_upgrade_via_mux(toy, active, &mut ctx);
    }

    #[test]
    fn attempt_upgrade_via_mux_calls_on_fire_exactly_once() {
        let mux = UpgradeMuxData::from_fields(&fields(&[("TriggeredBy", &["1"])])).unwrap();
        let mut toy = EffectToy {
            mux: Some(mux),
            fire_count: 0,
        };
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(1));

        let mut log = EffectLog::new();
        offer(&mut toy, &active, &mut log);
        offer(&mut toy, &active, &mut log);
        offer(&mut toy, &active, &mut log);

        assert_eq!(toy.fire_count, 1, "on_fire must run exactly once");
        assert_eq!(log.entries().len(), 1, "and report exactly once");
    }

    #[test]
    fn attempt_upgrade_via_mux_never_calls_on_fire_when_the_condition_is_not_met() {
        let mux = UpgradeMuxData::from_fields(&fields(&[("TriggeredBy", &["1"])])).unwrap();
        let mut toy = EffectToy {
            mux: Some(mux),
            fire_count: 0,
        };
        let mut log = EffectLog::new();
        offer(&mut toy, &UpgradeMask::EMPTY, &mut log);
        assert_eq!(toy.fire_count, 0);
        assert!(log.is_empty());
    }

    #[test]
    fn removes_upgrades_is_reported_after_the_modules_own_effect() {
        let mux = UpgradeMuxData::from_fields(&fields(&[
            ("TriggeredBy", &["1"]),
            ("RemovesUpgrades", &["4"]),
        ]))
        .unwrap();
        let mut toy = EffectToy {
            mux: Some(mux),
            fire_count: 0,
        };
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(1));
        let mut log = EffectLog::new();
        offer(&mut toy, &active, &mut log);
        let entries = log.drain();
        assert_eq!(entries.len(), 2);
        assert!(matches!(entries[0].1, Effect::CommandSet(_)));
        match &entries[1].1 {
            Effect::RemoveUpgrades(mask, _) => assert!(mask.contains(UpgradeBit::new(4))),
            other => panic!("expected the removal second, got {other:?}"),
        }
    }

    #[test]
    fn clones_share_their_triggers_and_hash_only_the_latch() {
        let mux = UpgradeMuxData::from_fields(&fields(&[("TriggeredBy", &["1"])])).unwrap();
        let mut copy = mux.clone();
        assert!(Arc::ptr_eq(&mux.triggers, &copy.triggers));

        let other = UpgradeMuxData::from_fields(&fields(&[("TriggeredBy", &["2", "3"])])).unwrap();
        let hash = |m: &UpgradeMuxData| {
            let mut hasher = StateHasher::new();
            m.hash_into(&mut hasher);
            hasher.finish()
        };
        assert_eq!(hash(&mux), hash(&other));
        let mut active = UpgradeMask::EMPTY;
        active.set(UpgradeBit::new(1));
        assert!(copy.try_fire(&active));
        assert_ne!(hash(&mux), hash(&copy), "the latch must reach the hash");
    }
}
