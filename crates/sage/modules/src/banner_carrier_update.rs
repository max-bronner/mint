use std::sync::Arc;

use mint_sim::module::{Capability, CapabilityMask, ModuleKind, Sleep, UpdateContext, UpgradeMask};
use mint_sim::{Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher};

use crate::effects::{Effect, EffectLog, Name};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BannerClocks {
    pub idle_spawn: LogicFrames,
    pub melee_free_unit: LogicFrames,
    pub died_respawn: LogicFrames,
    pub melee_free_banner: LogicFrames,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MorphCondition {
    pub unit_type: Name,
    pub conditions: Vec<Name>,
}

#[derive(Debug, PartialEq)]
struct BannerData {
    clocks: BannerClocks,
    unit_spawn_fx: Option<Name>,
    nearby: Option<f32>,
    all_nearby: bool,
    upgrade_required: UpgradeMask,
    morph_fx: Option<Name>,
    morphs: Vec<MorphCondition>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BannerCarrierUpdate {
    data: Arc<BannerData>,
    phased: bool,
}

impl BannerCarrierUpdate {
    #[must_use]
    pub fn clocks(&self) -> BannerClocks {
        self.data.clocks
    }

    #[must_use]
    pub fn morphs(&self) -> &[MorphCondition] {
        &self.data.morphs
    }

    #[must_use]
    pub fn morph_fx(&self) -> Option<&Name> {
        self.data.morph_fx.as_ref()
    }
}

impl Component for BannerCarrierUpdate {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bool(self.phased);
    }
}

impl ModuleKind for BannerCarrierUpdate {
    const UPDATES_UNDER: u32 = crate::disabled_masks::HELD;
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Update);

    fn on_attach(&mut self, _now: Frame) {
        self.phased = false;
    }

    fn update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        let rate = self.data.clocks.idle_spawn;
        if rate.is_zero() {
            return Sleep::Forever;
        }
        let entity = ctx.entity;
        if let Some(log) = ctx.effects.get::<EffectLog>() {
            log.push(
                entity,
                Effect::ReplenishHorde {
                    melee_free: self.data.clocks.melee_free_unit,
                    fx: self.data.unit_spawn_fx.clone(),
                    nearby: self.data.nearby,
                    all_nearby: self.data.all_nearby,
                    upgrade_required: self.data.upgrade_required.clone(),
                },
            );
        }
        if !self.phased {
            self.phased = true;
            let stagger = entity.index().checked_rem(rate.get()).unwrap_or(0);
            return Sleep::Frames(LogicFrames::from_frames(rate.get().saturating_add(stagger)));
        }
        Sleep::Frames(rate)
    }
}

pub fn banner_carrier_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<BannerCarrierUpdate, FieldError> {
    let nearby = if flag(fields, "ReplenishNearbyHorde")?.unwrap_or(false) {
        Some(number(fields, "ScanHordeDistance")?.unwrap_or(0.0))
    } else {
        None
    };
    Ok(BannerCarrierUpdate {
        data: Arc::new(BannerData {
            clocks: BannerClocks {
                idle_spawn: duration(fields, "IdleSpawnRate")?,
                melee_free_unit: duration(fields, "MeleeFreeUnitSpawnTime")?,
                died_respawn: duration(fields, "DiedRespawnTime")?,
                melee_free_banner: duration(fields, "MeleeFreeBannerReSpawnTime")?,
            },
            unit_spawn_fx: fields.field("UnitSpawnFX").map(Name::from),
            nearby,
            all_nearby: flag(fields, "ReplenishAllNearbyHordes")?.unwrap_or(false),
            upgrade_required: fields.field_upgrade_mask("UpgradeRequired"),
            morph_fx: fields.field("BannerMorphFX").map(Name::from),
            morphs: morph_conditions(fields)?,
        }),
        phased: false,
    })
}

fn morph_conditions(fields: &dyn ModuleFieldSource) -> Result<Vec<MorphCondition>, FieldError> {
    fields
        .field_args_each("MorphCondition")
        .into_iter()
        .map(|tokens| {
            let mut unit_type: Option<Name> = None;
            let mut conditions: Vec<Name> = Vec::new();
            let mut in_state = false;
            for token in tokens {
                match mint_sage_ini::value::split_tag(token) {
                    Some(("UnitType", name)) => {
                        in_state = false;
                        unit_type = Some(Name::from(name.trim_matches('"')));
                    }
                    Some(("ModelState", flag)) => {
                        in_state = true;
                        push_flag(&mut conditions, flag);
                    }
                    Some(_) => in_state = false,
                    None if in_state => push_flag(&mut conditions, token),
                    None => {}
                }
            }
            match (unit_type, conditions.is_empty()) {
                (Some(unit_type), false) => Ok(MorphCondition {
                    unit_type,
                    conditions,
                }),
                _ => Err(FieldError::new(
                    "MorphCondition needs a UnitType and a ModelState".to_owned(),
                )),
            }
        })
        .collect()
}

fn push_flag(conditions: &mut Vec<Name>, token: &str) {
    let flag = token.trim_matches('"');
    if !flag.is_empty() {
        conditions.push(Name::from(flag));
    }
}

fn duration(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<LogicFrames, FieldError> {
    let Some(ms) = number(fields, keyword)? else {
        return Ok(LogicFrames::ZERO);
    };
    if ms <= 0.0 {
        return Ok(LogicFrames::ZERO);
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Ok(crate::special_ability_update::retail_duration_ticks(
        ms.min(MAX_MILLISECONDS) as u32,
    ))
}

const MAX_MILLISECONDS: f32 = 3_600_000.0;

fn number(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<Option<f32>, FieldError> {
    let Some(raw) = fields.field(keyword) else {
        return Ok(None);
    };
    fields
        .field_numeric(keyword)
        .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not a number")))
        .map(Some)
}

fn flag(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<Option<bool>, FieldError> {
    let Some(raw) = fields.field(keyword) else {
        return Ok(None);
    };
    mint_sage_ini::value::parse_bool(raw)
        .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not Yes/No")))
        .map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct Fields(HashMap<&'static str, &'static str>);

    impl ModuleFieldSource for Fields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(keyword))
                .map(|(_, value)| *value)
        }
    }

    fn read(pairs: &[(&'static str, &'static str)]) -> Result<BannerCarrierUpdate, FieldError> {
        banner_carrier_update_from_fields(&Fields(pairs.iter().copied().collect()), Frame::ZERO)
    }

    #[test]
    fn the_first_pulse_is_immediate_and_the_stagger_shifts_the_next() {
        use mint_sim::module::{DestroyRequests, ModuleRegistry};
        use mint_sim::{Effects, World};
        let module = read(&[("IdleSpawnRate", "2000")]).expect("reads");
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<BannerCarrierUpdate>(&mut world, "BannerCarrierUpdate");
        let mut entity = world.spawn().unwrap();
        while entity.index() < 40 {
            entity = world.spawn().unwrap();
        }
        registry
            .attach(&mut world, entity, Frame::ZERO, module)
            .unwrap();
        let mut rng = mint_sim::Pcg32::new(7);
        let mut pulses = Vec::new();
        for frame in 0..200 {
            let mut log = EffectLog::new();
            registry.update_into(
                &mut world,
                Frame::new(frame),
                &mut rng,
                &mut DestroyRequests::new(),
                Effects::into_sink(&mut log),
            );
            if !log.drain().is_empty() {
                pulses.push(frame);
            }
        }
        assert_eq!(pulses, vec![0, 100, 160], "2000 ms is 60 frames; index 40");
    }

    #[test]
    fn a_duration_that_is_not_a_number_is_refused() {
        assert!(read(&[("IdleSpawnRate", "soon")]).is_err());
    }

    #[test]
    fn a_nearby_flag_that_is_not_yes_or_no_is_refused() {
        assert!(read(&[("ReplenishNearbyHorde", "sometimes")]).is_err());
    }

    #[test]
    fn the_four_clocks_read_as_milliseconds() {
        let read = read(&[
            ("IdleSpawnRate", "10000"),
            ("MeleeFreeUnitSpawnTime", "20000"),
            ("DiedRespawnTime", "10000"),
            ("MeleeFreeBannerReSpawnTime", "20000"),
        ])
        .expect("reads");
        assert_eq!(
            read.clocks(),
            BannerClocks {
                idle_spawn: LogicFrames::from_engine_milliseconds(10_000),
                melee_free_unit: LogicFrames::from_engine_milliseconds(20_000),
                died_respawn: LogicFrames::from_engine_milliseconds(10_000),
                melee_free_banner: LogicFrames::from_engine_milliseconds(20_000),
            }
        );
    }
}
