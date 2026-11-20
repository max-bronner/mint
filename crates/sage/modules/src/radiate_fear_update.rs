use std::sync::Arc;

use mint_sage_ini::value::parse_bool;
use mint_sage_ini::EmotionType;
use mint_sim::module::{Capability, CapabilityMask, ModuleKind, Sleep, UpdateContext, UpgradeMask};
use mint_sim::{
    Component, Entity, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher,
    UpgradeContext,
};

use crate::effects::{Effect, EffectLog, PowerPlace};
use crate::object_filter::ObjectFilter;
use crate::upgrade_mux::{sooner, UpgradeMuxData};

const DEFAULT_INTERVAL_MS: u32 = 1000;

#[derive(Debug, PartialEq)]
struct RadiatorData {
    initially_active: bool,
    radius: f32,
    interval: LogicFrames,
    kinds: Vec<EmotionType>,
    victim_filter: Arc<ObjectFilter>,
    which_special_power: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RadiateFearUpdate {
    data: Arc<RadiatorData>,
    mux: UpgradeMuxData,
    active: bool,
    next_pulse: Option<Frame>,
}

impl RadiateFearUpdate {
    #[must_use]
    pub fn initially_active(&self) -> bool {
        self.data.initially_active
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }

    #[must_use]
    pub fn radius(&self) -> f32 {
        self.data.radius
    }

    #[must_use]
    pub fn interval(&self) -> LogicFrames {
        self.data.interval
    }

    #[must_use]
    pub fn kinds(&self) -> &[EmotionType] {
        &self.data.kinds
    }

    #[must_use]
    pub fn victim_filter(&self) -> &ObjectFilter {
        &self.data.victim_filter
    }

    #[must_use]
    pub fn which_special_power(&self) -> Option<u32> {
        self.data.which_special_power
    }

    #[must_use]
    pub fn triggered_by(&self) -> &UpgradeMask {
        self.mux.triggered_by()
    }
}

impl Component for RadiateFearUpdate {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bool(self.active);
        hasher.write_bool(self.next_pulse.is_some());
        hasher.write_u64(self.next_pulse.map_or(0, Frame::get));
        self.mux.hash_into(hasher);
    }
}

impl ModuleKind for RadiateFearUpdate {
    const UPDATES_UNDER: u32 = crate::disabled_masks::NEVER;
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Update)
        .union(CapabilityMask::single(Capability::Upgrade));

    fn on_attach(&mut self, _now: Frame) {
        self.active = self.data.initially_active;
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

impl RadiateFearUpdate {
    fn own_update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        if !self.active || self.data.kinds.is_empty() || self.data.radius <= 0.0 {
            return Sleep::Forever;
        }
        let due = self.next_pulse.unwrap_or(ctx.now);
        let every = self.data.interval.saturating_add(stagger(ctx.entity));
        if ctx.now < due {
            return Sleep::Frames(every);
        }
        self.next_pulse = ctx.now.checked_add(every);
        if let Some(log) = ctx.effects.get::<EffectLog>() {
            for kind in &self.data.kinds {
                log.push(
                    ctx.entity,
                    Effect::EmotionPulse {
                        at: PowerPlace::Source,
                        radius: self.data.radius,
                        kind: *kind,
                        filter: Arc::clone(&self.data.victim_filter),
                        lasts: self.data.interval,
                    },
                );
            }
        }
        Sleep::Frames(every)
    }
}

fn stagger(entity: Entity) -> LogicFrames {
    let frames = entity.index().checked_rem(5).unwrap_or(0);
    LogicFrames::from_frames(
        frames.saturating_mul(crate::special_ability_update::TICKS_PER_RETAIL_FRAME),
    )
}

pub fn radiate_fear_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<RadiateFearUpdate, FieldError> {
    let flag = |keyword: &str| -> Result<bool, FieldError> {
        match fields.field(keyword) {
            None => Ok(false),
            Some(raw) => parse_bool(raw)
                .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not Yes/No"))),
        }
    };
    let number = |keyword: &str| -> Result<Option<f32>, FieldError> {
        match fields.field(keyword) {
            None => Ok(None),
            Some(raw) => fields
                .field_numeric(keyword)
                .filter(|value| value.is_finite() && *value >= 0.0)
                .map(Some)
                .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not a number"))),
        }
    };
    let interval_ms = number("EmotionPulseInterval")?.map_or(DEFAULT_INTERVAL_MS, |ms| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let whole = ms.min(86_400_000.0) as u32;
        whole
    });
    let kinds = [
        ("GenerateFear", EmotionType::Fear),
        ("GenerateTerror", EmotionType::Terror),
        (
            "GenerateUncontrollableFear",
            EmotionType::UncontrollableFear,
        ),
    ]
    .into_iter()
    .map(|(keyword, kind)| Ok(flag(keyword)?.then_some(kind)))
    .collect::<Result<Vec<_>, FieldError>>()?
    .into_iter()
    .flatten()
    .collect();
    let victim_tokens: Vec<String> = fields
        .field_args("VictimFilter")
        .into_iter()
        .map(str::to_owned)
        .collect();
    let initially_active = flag("InitiallyActive")?;
    Ok(RadiateFearUpdate {
        data: Arc::new(RadiatorData {
            initially_active,
            radius: number("EmotionPulseRadius")?.unwrap_or(0.0),
            interval: crate::special_ability_update::retail_duration_ticks(interval_ms)
                .max(LogicFrames::ONE),
            kinds,
            victim_filter: Arc::new(ObjectFilter::parse(&victim_tokens)),
            which_special_power: number("WhichSpecialPower")?.map(|value| {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let index = value.min(f32::from(u16::MAX)) as u32;
                index
            }),
        }),
        mux: UpgradeMuxData::from_fields(fields)?,
        active: initially_active,
        next_pulse: None,
    })
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
            if keyword.eq_ignore_ascii_case("TriggeredBy") && self.0.contains_key("TriggeredBy") {
                self.1.clone()
            } else {
                UpgradeMask::EMPTY
            }
        }
    }

    fn fields(pairs: &[(&'static str, &'static str)]) -> TestFields {
        let mut mask = UpgradeMask::EMPTY;
        mask.set(UpgradeBit::new(3));
        TestFields(
            pairs
                .iter()
                .map(|(k, v)| (*k, v.split_whitespace().collect()))
                .collect(),
            mask,
        )
    }

    fn pulses(
        module: RadiateFearUpdate,
        frames: u64,
        upgrade_at: Option<u64>,
    ) -> Vec<(u64, Effect)> {
        pulses_for(module, frames, upgrade_at, 0)
    }

    fn pulses_for(
        module: RadiateFearUpdate,
        frames: u64,
        upgrade_at: Option<u64>,
        before: u32,
    ) -> Vec<(u64, Effect)> {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<RadiateFearUpdate>(&mut world, "RadiateFearUpdate");
        for _ in 0..before {
            world.spawn().unwrap();
        }
        let entity = world.spawn().unwrap();
        registry
            .attach(&mut world, entity, Frame::ZERO, module)
            .unwrap();
        let mut rng = mint_sim::Pcg32::new(1);
        let mut out = Vec::new();
        for frame in 0..frames {
            if upgrade_at == Some(frame) {
                let mut mask = UpgradeMask::EMPTY;
                mask.set(UpgradeBit::new(3));
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
    fn a_triggered_radiator_pulses_its_own_type_on_its_own_interval() {
        let module = radiate_fear_update_from_fields(
            &fields(&[
                ("InitiallyActive", "No"),
                ("TriggeredBy", "Upgrade_BlackRiderDreadVisage"),
                ("EmotionPulseRadius", "100"),
                ("EmotionPulseInterval", "1000"),
                ("GenerateFear", "Yes"),
                ("GenerateTerror", "No"),
                ("VictimFilter", "ALL ENEMIES"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert!(pulses(module.clone(), 200, None).is_empty());
        let got = pulses(module, 200, Some(70));
        let frames: Vec<u64> = got.iter().map(|(f, _)| *f).collect();
        assert_eq!(frames, vec![70, 100, 130, 160, 190]);
        let Effect::EmotionPulse {
            radius,
            kind,
            filter,
            ..
        } = &got[0].1
        else {
            panic!("{:?}", got[0].1);
        };
        assert_eq!((*radius, *kind), (100.0, EmotionType::Fear));
        assert!(filter.passes(&["INFANTRY".to_owned()], "ZFighter"));
    }

    #[test]
    fn a_radiator_pulses_every_interval_plus_its_id_mod_five_frames() {
        let module = radiate_fear_update_from_fields(
            &fields(&[
                ("InitiallyActive", "Yes"),
                ("EmotionPulseRadius", "100"),
                ("EmotionPulseInterval", "1000"),
                ("GenerateFear", "Yes"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        let frames: Vec<u64> = pulses_for(module, 100, None, 3)
            .iter()
            .map(|(frame, _)| *frame)
            .collect();
        assert_eq!(frames, vec![0, 48, 96]);
    }

    #[test]
    fn a_radius_that_is_not_a_number_is_refused_rather_than_defaulted() {
        assert!(radiate_fear_update_from_fields(
            &fields(&[("EmotionPulseRadius", "wide")]),
            Frame::ZERO
        )
        .is_err());
    }

    #[test]
    fn a_radiator_with_no_generate_flag_pulses_nothing() {
        let module = radiate_fear_update_from_fields(
            &fields(&[
                ("InitiallyActive", "Yes"),
                ("EmotionPulseRadius", "100"),
                ("EmotionPulseInterval", "1000"),
                ("GenerateFear", "No"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert!(pulses(module, 130, None).is_empty());
    }
}
