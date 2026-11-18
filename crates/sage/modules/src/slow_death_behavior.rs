use mint_sage_ini::value::parse_int;
use mint_sim::module::{Capability, CapabilityMask, DieContext, ModuleKind, ModuleSlot};
use mint_sim::Pcg32;
use mint_sim::{Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher};

use crate::die_mux::DieMuxData;

const MAX_MILLISECONDS: f32 = 86_400_000.0;

#[derive(Debug, Clone, PartialEq)]
pub struct SlowDeathBehavior {
    weight: u32,
    mux: DieMuxData,
    selected: bool,
    sink_rate: f32,
    sink_delay: LogicFrames,
    sink_delay_variance: LogicFrames,
}

impl SlowDeathBehavior {
    #[must_use]
    pub fn new(weight: u32) -> Self {
        Self {
            weight: if weight == 0 { 1 } else { weight },
            mux: DieMuxData::default(),
            selected: false,
            sink_rate: 0.0,
            sink_delay: LogicFrames::ZERO,
            sink_delay_variance: LogicFrames::ZERO,
        }
    }

    #[must_use]
    pub const fn weight(&self) -> u32 {
        self.weight
    }

    #[must_use]
    pub const fn was_selected(&self) -> bool {
        self.selected
    }

    #[must_use]
    pub const fn sink_rate(&self) -> f32 {
        self.sink_rate
    }

    #[must_use]
    pub const fn sink_delay(&self) -> LogicFrames {
        self.sink_delay
    }

    #[must_use]
    pub const fn sink_delay_variance(&self) -> LogicFrames {
        self.sink_delay_variance
    }
}

impl Component for SlowDeathBehavior {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_u32(self.weight);
        hasher.write_u8(u8::from(self.selected));
        hasher.write_f32(self.sink_rate);
        self.sink_delay.hash_into(hasher);
        self.sink_delay_variance.hash_into(hasher);
    }
}

pub fn sink_rate_from_fields(fields: &dyn ModuleFieldSource) -> Result<f32, FieldError> {
    let Some(raw) = fields.field("SinkRate") else {
        return Ok(0.0);
    };
    fields
        .field_numeric("SinkRate")
        .filter(|v| v.is_finite())
        .ok_or_else(|| FieldError::new(format!("SinkRate '{raw}' is not a number")))
}

pub fn sink_delay_from_fields(
    fields: &dyn ModuleFieldSource,
    keyword: &str,
) -> Result<LogicFrames, FieldError> {
    let Some(raw) = fields.field(keyword) else {
        return Ok(LogicFrames::ZERO);
    };
    let ms = fields
        .field_numeric(keyword)
        .filter(|ms| ms.is_finite())
        .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not milliseconds")))?;
    if ms <= 0.0 {
        return Ok(LogicFrames::ZERO);
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = ms.min(MAX_MILLISECONDS) as u32;
    Ok(crate::special_ability_update::retail_duration_ticks(whole))
}

impl ModuleKind for SlowDeathBehavior {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Die);

    fn on_die(instances: &mut [ModuleSlot<Self>], ctx: &mut DieContext<'_>) {
        let admitted: Vec<bool> = instances
            .iter()
            .map(|slot| {
                slot.state
                    .mux
                    .gate(ctx.entity, "SlowDeathBehavior", &ctx.death)
            })
            .collect();
        let total: u32 = instances
            .iter()
            .zip(admitted.iter())
            .filter(|(_, admits)| **admits)
            .fold(0u32, |sum, (slot, _)| sum.saturating_add(slot.state.weight));
        if total == 0 {
            return;
        }

        let roll = draw_roll(ctx.rng, total);
        let mut remaining = roll;
        let mut winner_chosen = false;
        for (index, slot) in instances.iter_mut().enumerate() {
            slot.state.selected = false;
            if winner_chosen || !admitted.get(index).copied().unwrap_or(false) {
                continue;
            }
            remaining = remaining.saturating_sub(slot.state.weight);
            if remaining == 0 {
                slot.state.selected = true;
                winner_chosen = true;
            }
        }
    }
}

fn draw_roll(rng: &mut Pcg32, total: u32) -> u32 {
    rng.below(total).saturating_add(1)
}

pub fn slow_death_behavior_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<SlowDeathBehavior, FieldError> {
    let modifier = match fields.field("ProbabilityModifier") {
        Some(raw) => parse_int(raw).ok_or_else(|| {
            FieldError::new(format!("ProbabilityModifier '{raw}' is not an integer"))
        })?,
        None => 10,
    };
    let weight = u32::try_from(modifier.max(1)).unwrap_or(1);
    let mut module = SlowDeathBehavior::new(weight);
    module.mux = DieMuxData::from_fields(fields)?;
    module.sink_rate = sink_rate_from_fields(fields)?;
    module.sink_delay = sink_delay_from_fields(fields, "SinkDelay")?;
    module.sink_delay_variance = sink_delay_from_fields(fields, "SinkDelayVariance")?;
    Ok(module)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mint_sim::module::{DestroyRequests, ModuleRegistry};
    use mint_sim::{Entity, Frame, World};

    fn spawn(world: &mut World) -> Entity {
        world.spawn().unwrap()
    }

    #[test]
    fn a_zero_weight_is_floored_at_one() {
        assert_eq!(SlowDeathBehavior::new(0).weight(), 1);
        assert_eq!(SlowDeathBehavior::new(7).weight(), 7);
    }

    struct TestFields(std::collections::BTreeMap<&'static str, &'static str>);
    impl ModuleFieldSource for TestFields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .map(|(_, v)| *v)
        }
    }

    #[test]
    fn from_fields_reads_the_declared_probability_modifier() {
        let fields = TestFields(std::collections::BTreeMap::from([(
            "ProbabilityModifier",
            "25",
        )]));
        let module = slow_death_behavior_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.weight(), 25);
    }

    #[test]
    fn from_fields_defaults_to_ten_when_the_field_is_absent() {
        let fields = TestFields(std::collections::BTreeMap::new());
        let module = slow_death_behavior_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(
            module.weight(),
            10,
            "EA's own default, SlowDeathBehavior.cpp:67"
        );
    }

    #[test]
    fn from_fields_refuses_an_unparseable_probability_modifier() {
        let fields = TestFields(std::collections::BTreeMap::from([(
            "ProbabilityModifier",
            "not a number",
        )]));
        assert!(slow_death_behavior_from_fields(&fields, Frame::ZERO).is_err());
    }

    #[test]
    fn from_fields_reads_the_declared_sink_fields() {
        let fields = TestFields(std::collections::BTreeMap::from([
            ("SinkRate", "0.40"),
            ("SinkDelay", "3000"),
            ("SinkDelayVariance", "500"),
        ]));
        let module = slow_death_behavior_from_fields(&fields, Frame::ZERO).unwrap();
        assert!((module.sink_rate() - 0.40).abs() < 1e-6);
        assert_eq!(
            module.sink_delay(),
            LogicFrames::from_engine_milliseconds(3000)
        );
        assert_eq!(
            module.sink_delay_variance(),
            crate::special_ability_update::retail_duration_ticks(500)
        );
    }

    #[test]
    fn from_fields_defaults_the_sink_fields_to_zero_when_absent() {
        let fields = TestFields(std::collections::BTreeMap::new());
        let module = slow_death_behavior_from_fields(&fields, Frame::ZERO).unwrap();
        assert_eq!(module.sink_rate(), 0.0);
        assert_eq!(module.sink_delay(), LogicFrames::ZERO);
    }

    #[test]
    fn from_fields_refuses_an_unparseable_sink_rate() {
        let fields = TestFields(std::collections::BTreeMap::from([("SinkRate", "fast")]));
        assert!(slow_death_behavior_from_fields(&fields, Frame::ZERO).is_err());
    }

    #[test]
    fn exactly_one_instance_wins_and_it_is_recorded_in_state() {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<SlowDeathBehavior>(&mut world, "SlowDeathBehavior");
        let entity = spawn(&mut world);
        for weight in [3u32, 2, 5] {
            registry
                .attach(
                    &mut world,
                    entity,
                    Frame::ZERO,
                    SlowDeathBehavior::new(weight),
                )
                .unwrap();
        }

        let mut rng = Pcg32::new(42);
        let mut destroy = DestroyRequests::new();
        registry.broadcast_die(&mut world, entity, Frame::ZERO, &mut rng, &mut destroy);

        let id = world.register::<mint_sim::module::ModuleColumn<SlowDeathBehavior>>();
        let column = world.store(id).unwrap();
        let instances = &column.get(entity).unwrap().0;
        assert_eq!(instances.len(), 3);
        let winners: Vec<_> = instances
            .iter()
            .filter(|slot| slot.state.was_selected())
            .collect();
        assert_eq!(winners.len(), 1, "exactly one instance must win");
    }

    #[test]
    fn declaration_order_can_change_which_instance_wins_for_the_same_seed() {
        fn run(weights: [u32; 3], seed: u64) -> u32 {
            let mut world = World::new();
            let mut registry = ModuleRegistry::new(&mut world);
            registry.register::<SlowDeathBehavior>(&mut world, "SlowDeathBehavior");
            let entity = spawn(&mut world);
            for weight in weights {
                registry
                    .attach(
                        &mut world,
                        entity,
                        Frame::ZERO,
                        SlowDeathBehavior::new(weight),
                    )
                    .unwrap();
            }
            let mut rng = Pcg32::new(seed);
            let mut destroy = DestroyRequests::new();
            registry.broadcast_die(&mut world, entity, Frame::ZERO, &mut rng, &mut destroy);

            let id = world.register::<mint_sim::module::ModuleColumn<SlowDeathBehavior>>();
            let column = world.store(id).unwrap();
            let instances = &column.get(entity).unwrap().0;
            instances
                .iter()
                .find(|slot| slot.state.was_selected())
                .map(|slot| slot.state.weight())
                .expect("exactly one instance wins")
        }

        let forward = run([3, 2, 5], 1);
        let reversed = run([5, 2, 3], 1);
        assert_ne!(
            forward, reversed,
            "reordering the same weights for the same seed must change which weight wins"
        );
    }

    #[test]
    fn the_same_seed_and_order_always_pick_the_same_winner() {
        fn run() -> [bool; 3] {
            let mut world = World::new();
            let mut registry = ModuleRegistry::new(&mut world);
            registry.register::<SlowDeathBehavior>(&mut world, "SlowDeathBehavior");
            let entity = spawn(&mut world);
            for weight in [4u32, 4, 4] {
                registry
                    .attach(
                        &mut world,
                        entity,
                        Frame::ZERO,
                        SlowDeathBehavior::new(weight),
                    )
                    .unwrap();
            }
            let mut rng = Pcg32::new(999);
            let mut destroy = DestroyRequests::new();
            registry.broadcast_die(&mut world, entity, Frame::ZERO, &mut rng, &mut destroy);
            let id = world.register::<mint_sim::module::ModuleColumn<SlowDeathBehavior>>();
            let column = world.store(id).unwrap();
            let instances = &column.get(entity).unwrap().0;
            [
                instances[0].state.was_selected(),
                instances[1].state.was_selected(),
                instances[2].state.was_selected(),
            ]
        }
        assert_eq!(run(), run());
    }
}
