use mint_sim::module::{Capability, CapabilityMask, ModuleKind, Sleep, UpdateContext};
use mint_sim::{Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher};

use crate::effects::{Effect, EffectLog};
use crate::special_ability_update::{retail_duration_ticks, TICKS_PER_RETAIL_FRAME};
use crate::world_reads::ModelConditions;

const OATH_FULLFILLED: &str = "OATH_FULLFILLED";

#[derive(Debug, Clone, PartialEq)]
pub struct OathbreakersFadeAwayBehavior {
    steps: u32,
    opacity: f32,
    fading: bool,
}

impl OathbreakersFadeAwayBehavior {
    #[must_use]
    pub fn opacity(&self) -> f32 {
        self.opacity
    }
}

impl Component for OathbreakersFadeAwayBehavior {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_f32(self.opacity);
        hasher.write_bool(self.fading);
    }
}

impl ModuleKind for OathbreakersFadeAwayBehavior {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Update);

    fn update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        let frame = LogicFrames::from_frames(TICKS_PER_RETAIL_FRAME);
        let holds = ctx
            .reads
            .get::<ModelConditions>()
            .is_some_and(|held| held.holds(ctx.entity, OATH_FULLFILLED));
        if !self.fading && !holds {
            return Sleep::Frames(frame);
        }
        let first = !self.fading;
        self.fading = true;
        #[allow(clippy::cast_precision_loss)]
        let step = 1.0 / self.steps.max(1) as f32;
        self.opacity = (self.opacity - step).max(0.0);
        if let Some(log) = ctx.effects.get::<EffectLog>() {
            log.push(
                ctx.entity,
                Effect::FadeAway {
                    opacity: self.opacity,
                    first,
                },
            );
        }
        Sleep::Frames(frame)
    }
}

pub fn oathbreakers_fade_away_behavior_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<OathbreakersFadeAwayBehavior, FieldError> {
    let ms = match fields.field("FadeOutTime") {
        None => 0.0,
        Some(raw) => fields
            .field_numeric("FadeOutTime")
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or_else(|| FieldError::new(format!("FadeOutTime '{raw}' is not a number")))?,
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let ticks = retail_duration_ticks(ms.min(86_400_000.0) as u32).get();
    Ok(OathbreakersFadeAwayBehavior {
        steps: ticks
            .checked_div(TICKS_PER_RETAIL_FRAME)
            .unwrap_or(1)
            .max(1),
        opacity: 1.0,
        fading: false,
    })
}
