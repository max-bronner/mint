use std::sync::Arc;

use mint_sim::module::UpdateContext;
use mint_sim::{Entity, FieldError, Frame, ModuleFieldSource};

use crate::effects::{Effect, EffectLog, Name, PowerPlace};
use crate::special_ability_update::{
    ability_data_from_fields, AbilityTarget, AbilityUpdate, Trigger,
};

#[derive(Debug, Clone, PartialEq)]
pub struct CurseTrigger {
    share: f32,
    cursed_fx: Option<Name>,
    trigger_fx: Option<Name>,
}

impl Trigger for CurseTrigger {
    fn fire(&self, entity: Entity, target: AbilityTarget, ctx: &mut UpdateContext<'_>) {
        let Some(log) = ctx.effects.get::<EffectLog>() else {
            return;
        };
        if let Some(list) = &self.trigger_fx {
            log.push(
                entity,
                Effect::PlayFx {
                    list: Name::clone(list),
                    at: PowerPlace::Source,
                    on_object: false,
                    secondary: None,
                },
            );
        }
        log.push(
            entity,
            Effect::CursePowers {
                at: crate::dominate_enemy_special_power::place_of(target),
                share: self.share,
                fx: self.cursed_fx.clone(),
            },
        );
    }
}

pub type CurseSpecialPower = AbilityUpdate<CurseTrigger>;

pub fn curse_special_power_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<CurseSpecialPower, FieldError> {
    let share = match fields.field("CursePercentage") {
        None => 0.0,
        Some(_) => fields
            .field_percent("CursePercentage")
            .filter(|value| value.is_finite())
            .ok_or_else(|| {
                FieldError::new("CursePercentage is present but not a percentage/known macro")
            })?,
    };
    let trigger = CurseTrigger {
        share,
        cursed_fx: fields.field("CursedFX").map(Name::from),
        trigger_fx: fields.field("TriggerFX").map(Name::from),
    };
    Ok(AbilityUpdate {
        data: Arc::new(ability_data_from_fields(fields, trigger)?),
        active: None,
        effect_until: None,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct Fields(BTreeMap<&'static str, &'static str>);
    impl ModuleFieldSource for Fields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0.get(keyword).copied()
        }
        fn field_percent(&self, keyword: &str) -> Option<f32> {
            self.field(keyword)
                .map(|value| value.trim_end_matches('%'))
                .and_then(|value| value.parse::<f32>().ok())
                .map(|value| value / 100.0)
        }
        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.0
                .get(keyword)
                .map(|value| value.split_whitespace().collect())
                .unwrap_or_default()
        }
    }
    fn fields(pairs: &[(&'static str, &'static str)]) -> Fields {
        Fields(pairs.iter().copied().collect())
    }

    #[test]
    fn missing_power_template_is_refused() {
        assert!(curse_special_power_from_fields(&fields(&[]), Frame::ZERO).is_err());
    }

    #[test]
    fn a_curse_percentage_reads_as_a_share_of_one_reload() {
        for (written, share) in [("100.0%", 1.0), ("33.33%", 0.3333)] {
            let module = curse_special_power_from_fields(
                &fields(&[
                    ("SpecialPowerTemplate", "ModAuthorCurse"),
                    ("CursePercentage", written),
                ]),
                Frame::ZERO,
            )
            .unwrap();
            assert!(
                (module.data.trigger.share - share).abs() < 0.001,
                "{written}"
            );
        }
        assert!(curse_special_power_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "ModAuthorCurse"),
                ("CursePercentage", "soon"),
            ]),
            Frame::ZERO,
        )
        .is_err());
    }
}
