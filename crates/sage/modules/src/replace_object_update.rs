use std::sync::Arc;

use mint_sim::module::UpdateContext;
use mint_sim::{Entity, FieldError, Frame, ModuleFieldSource};

use crate::effects::{Effect, EffectLog, Name, Names, PowerPlace, ReplaceAreaEntry};
use crate::object_filter::ObjectFilter;
use crate::special_ability_update::{
    ability_data_from_fields, AbilityTarget, AbilityUpdate, Trigger,
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ReplaceTrigger {
    entries: Arc<[ReplaceAreaEntry]>,
    radius: f32,
    fx: Option<Name>,
    scatter: bool,
}

impl Trigger for ReplaceTrigger {
    fn fire(&self, entity: Entity, target: AbilityTarget, ctx: &mut UpdateContext<'_>) {
        if self.entries.is_empty() {
            return;
        }
        let at = match target {
            AbilityTarget::Object(target) => PowerPlace::Object(target),
            AbilityTarget::Position(x, y) => PowerPlace::At(x, y),
            AbilityTarget::None => return,
        };
        let Some(log) = ctx.effects.get::<EffectLog>() else {
            return;
        };
        log.push(
            entity,
            Effect::ReplaceInArea {
                entries: Arc::clone(&self.entries),
                radius: self.radius,
                fx: self.fx.clone(),
                scatter: self.scatter,
                at,
            },
        );
    }
}

pub(crate) type ReplaceObjectUpdate = AbilityUpdate<ReplaceTrigger>;

pub(crate) fn replace_object_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<ReplaceObjectUpdate, FieldError> {
    let mut entries = Vec::new();
    fields.nested("ReplaceObject", &mut |block| {
        let names: Vec<Name> = block
            .field_args("ReplacementObjectName")
            .into_iter()
            .map(Name::from)
            .collect();
        if names.is_empty() {
            return;
        }
        let filter_tokens: Vec<String> = block
            .field_args("TargetObjectFilter")
            .into_iter()
            .map(String::from)
            .collect();
        entries.push(ReplaceAreaEntry {
            filter: Arc::new(ObjectFilter::parse(&filter_tokens)),
            replacements: Names::from(names),
        });
    });
    let trigger = ReplaceTrigger {
        entries: Arc::from(entries),
        radius: fields.field_numeric("ReplaceRadius").unwrap_or(0.0),
        fx: fields.field("ReplaceFX").map(Name::from),
        scatter: crate::fire_weapon_update::flag(fields, "Scatter").unwrap_or(false),
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

    struct Fields {
        own: Vec<(&'static str, &'static str)>,
        blocks: Vec<Vec<(&'static str, &'static str)>>,
    }
    impl ModuleFieldSource for Fields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.own
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(keyword))
                .map(|(_, v)| *v)
        }
        fn field_args(&self, keyword: &str) -> Vec<&str> {
            self.field(keyword)
                .map(|v| v.split_whitespace().collect())
                .unwrap_or_default()
        }
        fn nested(&self, keyword: &str, visit: &mut dyn FnMut(&dyn ModuleFieldSource)) {
            if !keyword.eq_ignore_ascii_case("ReplaceObject") {
                return;
            }
            for block in &self.blocks {
                visit(&Fields {
                    own: block.clone(),
                    blocks: Vec::new(),
                });
            }
        }
    }

    #[test]
    fn missing_power_template_is_refused() {
        let fields = Fields {
            own: vec![],
            blocks: vec![],
        };
        assert!(replace_object_update_from_fields(&fields, Frame::ZERO).is_err());
    }

    #[test]
    fn two_replace_object_blocks_read_as_two_entries_in_order() {
        let fields = Fields {
            own: vec![("SpecialPowerTemplate", "ModAuthorPoultry")],
            blocks: vec![
                vec![
                    ("TargetObjectFilter", "NONE +INFANTRY"),
                    ("ReplacementObjectName", "ModAuthorHen"),
                ],
                vec![
                    ("TargetObjectFilter", "NONE +CAVALRY"),
                    ("ReplacementObjectName", "ModAuthorGoat ModAuthorPig"),
                ],
            ],
        };
        let module = replace_object_update_from_fields(&fields, Frame::ZERO).unwrap();
        let entries = &module.data.trigger.entries;
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].replacements[0].as_ref(), "ModAuthorHen");
        assert_eq!(
            entries[1].replacements.len(),
            2,
            "the second block's two replacement names must both be read"
        );
    }
}
