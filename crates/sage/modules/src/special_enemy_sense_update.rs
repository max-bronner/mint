use std::sync::Arc;

use mint_sim::module::{Capability, CapabilityMask, ModuleKind, Sleep, UpdateContext};
use mint_sim::{Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher};

use crate::effects::{Effect, EffectLog};
use crate::object_filter::ObjectFilter;

#[derive(Debug, PartialEq)]
struct SenseData {
    filter: Arc<ObjectFilter>,
    range: f32,
    interval: LogicFrames,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpecialEnemySenseUpdate {
    data: Arc<SenseData>,
}

impl SpecialEnemySenseUpdate {
    #[must_use]
    pub fn range(&self) -> f32 {
        self.data.range
    }

    #[must_use]
    pub fn interval(&self) -> LogicFrames {
        self.data.interval
    }
}

impl Component for SpecialEnemySenseUpdate {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for SpecialEnemySenseUpdate {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Update);

    fn update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        if let Some(log) = ctx.effects.get::<EffectLog>() {
            log.push(
                ctx.entity,
                Effect::SenseEnemies {
                    range: self.data.range,
                    filter: Arc::clone(&self.data.filter),
                },
            );
        }
        Sleep::Frames(self.data.interval)
    }
}

pub fn special_enemy_sense_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<SpecialEnemySenseUpdate, FieldError> {
    let number = |keyword: &str| -> Result<f32, FieldError> {
        match fields.field(keyword) {
            None => Ok(0.0),
            Some(raw) => fields
                .field_numeric(keyword)
                .filter(|value| value.is_finite() && *value >= 0.0)
                .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not a number"))),
        }
    };
    let tokens: Vec<String> = fields
        .field_args("SpecialEnemyFilter")
        .into_iter()
        .map(str::to_owned)
        .collect();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let interval_ms = number("ScanInterval")?.min(86_400_000.0) as u32;
    Ok(SpecialEnemySenseUpdate {
        data: Arc::new(SenseData {
            filter: Arc::new(ObjectFilter::parse(&tokens)),
            range: number("ScanRange")?,
            interval: crate::special_ability_update::retail_duration_ticks(interval_ms)
                .max(LogicFrames::ONE),
        }),
    })
}
