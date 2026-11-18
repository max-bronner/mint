use std::sync::Arc;

use mint_sim::module::{Capability, CapabilityMask};
use mint_sim::{
    ActivateContext, Activation, Component, FieldError, Frame, ModuleFieldSource, ModuleKind,
    StateHasher,
};

use crate::effects::{
    build_modifier_rule, names, CreateLocation, Effect, EffectLog, ModifierFieldValues,
    ModifierRule, Name, PowerPlace, WeatherKind,
};
use crate::object_filter::ObjectFilter;

fn power_of(fields: &dyn ModuleFieldSource) -> Result<Name, FieldError> {
    fields
        .field("SpecialPowerTemplate")
        .map(Name::from)
        .ok_or_else(|| FieldError::new("SpecialPowerTemplate is missing"))
}

fn flag(fields: &dyn ModuleFieldSource, keyword: &str) -> Option<bool> {
    fields
        .field(keyword)
        .and_then(mint_sage_ini::value::parse_bool)
}

fn weather_duration(fields: &dyn ModuleFieldSource) -> mint_sim::LogicFrames {
    let ms = fields
        .field_numeric("WeatherDuration")
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(0.0)
        .min(86_400_000.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = ms as u32;
    mint_sim::LogicFrames::from_milliseconds(whole)
}

fn change_weather(fields: &dyn ModuleFieldSource) -> Result<Option<WeatherKind>, FieldError> {
    match fields.field("ChangeWeather") {
        None => Ok(None),
        Some(raw) => WeatherKind::parse(raw)
            .map(Some)
            .ok_or_else(|| FieldError::new(format!("ChangeWeather '{raw}' is not a weather"))),
    }
}

pub(crate) fn push(ctx: &mut ActivateContext<'_>, effect: Effect) {
    let entity = ctx.entity;
    if let Some(log) = ctx.effects.get::<EffectLog>() {
        log.push(entity, effect);
    }
}

fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

pub(crate) fn trigger_fx(fields: &dyn ModuleFieldSource) -> Option<Name> {
    fields
        .field("TriggerFX")
        .filter(|name| !name.eq_ignore_ascii_case("NONE"))
        .map(Name::from)
}

pub(crate) fn push_trigger(
    ctx: &mut ActivateContext<'_>,
    fx: Option<&Name>,
    activation: &Activation<'_>,
) {
    if let Some(list) = fx {
        push(
            ctx,
            Effect::PlayFx {
                list: Name::clone(list),
                at: PowerPlace::of(activation),
                on_object: matches!(PowerPlace::of(activation), PowerPlace::Source),
                secondary: None,
            },
        );
    }
}

#[derive(Debug, PartialEq)]
struct OclData {
    power: Name,
    list: Name,
    location: CreateLocation,
    fx: Option<Name>,
    re_enable: bool,
    weather: Option<WeatherKind>,
    weather_lasts: mint_sim::LogicFrames,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OclSpecialPower(Arc<OclData>);

impl OclSpecialPower {
    #[must_use]
    pub fn list(&self) -> &str {
        &self.0.list
    }
}

impl Component for OclSpecialPower {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for OclSpecialPower {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::SpecialPower);

    fn is_for_power(&self, power: &str) -> bool {
        same(&self.0.power, power)
    }

    fn activate(&mut self, activation: &Activation<'_>, ctx: &mut ActivateContext<'_>) {
        push_trigger(ctx, self.0.fx.as_ref(), activation);
        if self.0.re_enable {
            push(ctx, Effect::ClearWeatherEffect);
        }
        if let Some(kind) = self.0.weather {
            push(
                ctx,
                Effect::ChangeWeather {
                    kind,
                    lasts: self.0.weather_lasts,
                },
            );
        }
        push(
            ctx,
            Effect::CreateObjects {
                list: Name::clone(&self.0.list),
                location: self.0.location.clone(),
                at: PowerPlace::of(activation),
                from: crate::effects::OclFrom::Location,
            },
        );
    }
}

fn refine_create_location(
    location: CreateLocation,
    fields: &dyn ModuleFieldSource,
) -> CreateLocation {
    let CreateLocation::Other(name) = &location else {
        return location;
    };
    match name.to_ascii_uppercase().as_str() {
        "USE_SECONDARY_OBJECT_LOCATION" => {
            let tokens: Vec<String> = fields
                .field_args("NearestSecondaryObjectFilter")
                .into_iter()
                .map(str::to_owned)
                .collect();
            CreateLocation::UseSecondaryObject(Arc::new(ObjectFilter::parse(&tokens)))
        }
        "CREATE_AT_EDGE_NEAR_TARGET_AND_MOVE_TO_LOCATION" => {
            CreateLocation::AtEdgeNearTargetAndMove
        }
        _ => location,
    }
}

pub fn ocl_special_power_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<OclSpecialPower, FieldError> {
    let power = power_of(fields)?;
    let list = fields
        .field("OCL")
        .map(Name::from)
        .ok_or_else(|| FieldError::new("OCL is missing"))?;
    let location = fields
        .field("CreateLocation")
        .map_or(CreateLocation::AtEdgeNearSource, CreateLocation::parse);
    let location = refine_create_location(location, fields);
    Ok(OclSpecialPower(Arc::new(OclData {
        power,
        list,
        location,
        fx: trigger_fx(fields),
        re_enable: flag(fields, "ReEnableAntiCategory").unwrap_or(false),
        weather: change_weather(fields)?,
        weather_lasts: weather_duration(fields),
    })))
}

#[derive(Debug, PartialEq)]
struct HealData {
    power: Name,
    radius: f32,
    amount: f32,
    as_percent: bool,
    affects: crate::effects::Names,
    fx: Option<Name>,
    heal_fx: Option<Name>,
    ocl: Option<Name>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerHealSpecialPower(Arc<HealData>);

impl Component for PlayerHealSpecialPower {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for PlayerHealSpecialPower {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::SpecialPower);

    fn is_for_power(&self, power: &str) -> bool {
        same(&self.0.power, power)
    }

    fn activate(&mut self, activation: &Activation<'_>, ctx: &mut ActivateContext<'_>) {
        push_trigger(ctx, self.0.fx.as_ref(), activation);
        if let Some(list) = &self.0.ocl {
            push(
                ctx,
                Effect::CreateObjects {
                    list: Name::clone(list),
                    location: CreateLocation::AtLocation,
                    at: PowerPlace::of(activation),
                    from: crate::effects::OclFrom::Location,
                },
            );
        }
        push(
            ctx,
            Effect::AreaHeal {
                at: PowerPlace::of(activation),
                radius: self.0.radius,
                amount: self.0.amount,
                as_percent: self.0.as_percent,
                affects: crate::effects::Names::clone(&self.0.affects),
                fx: self.0.heal_fx.clone(),
            },
        );
    }
}

pub fn player_heal_special_power_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<PlayerHealSpecialPower, FieldError> {
    let power = power_of(fields)?;
    let number = |keyword: &str| -> Result<f32, FieldError> {
        match fields.field(keyword) {
            None => Ok(0.0),
            Some(raw) => fields
                .field_numeric(keyword)
                .filter(|value| value.is_finite() && *value >= 0.0)
                .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not a number"))),
        }
    };
    Ok(PlayerHealSpecialPower(Arc::new(HealData {
        power,
        radius: number("HealRadius")?,
        amount: number("HealAmount")?,
        as_percent: flag(fields, "HealAsPercent").unwrap_or(true),
        affects: names(
            fields
                .field_args("HealAffects")
                .into_iter()
                .map(str::to_owned)
                .collect(),
        ),
        fx: trigger_fx(fields),
        heal_fx: fields
            .field("HealFX")
            .filter(|name| !name.eq_ignore_ascii_case("NONE"))
            .map(Name::from),
        ocl: fields
            .field("HealOCL")
            .filter(|name| !name.eq_ignore_ascii_case("NONE"))
            .map(Name::from),
    })))
}

#[derive(Debug, PartialEq)]
struct PowerData {
    power: Name,
    modifier: Option<ModifierRule>,
    fx: Option<Name>,
    defer_trigger: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpecialPowerModule(Arc<PowerData>);

impl Component for SpecialPowerModule {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for SpecialPowerModule {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::SpecialPower);

    fn is_for_power(&self, power: &str) -> bool {
        same(&self.0.power, power)
    }

    fn activate(&mut self, activation: &Activation<'_>, ctx: &mut ActivateContext<'_>) {
        if self.0.defer_trigger {
            return;
        }
        push_trigger(ctx, self.0.fx.as_ref(), activation);
        let Some(rule) = self.0.modifier.clone() else {
            return;
        };
        push(
            ctx,
            Effect::AreaModifier {
                at: PowerPlace::of(activation),
                rule,
            },
        );
    }
}

pub fn special_power_module_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<SpecialPowerModule, FieldError> {
    let power = power_of(fields)?;
    Ok(SpecialPowerModule(Arc::new(PowerData {
        power,
        modifier: modifier_rule_from_fields(fields),
        fx: trigger_fx(fields),
        defer_trigger: flag(fields, "UpdateModuleStartsAttack").unwrap_or(false),
    })))
}

fn modifier_rule_from_fields(fields: &dyn ModuleFieldSource) -> Option<ModifierRule> {
    if flag(fields, "AttributeModifierWeatherBased").unwrap_or(false) {
        return None;
    }
    build_modifier_rule(ModifierFieldValues {
        list: fields.field("AttributeModifier").map(Name::from),
        range: fields
            .field_numeric("AttributeModifierRange")
            .filter(|value| value.is_finite() && *value >= 0.0)
            .unwrap_or(0.0),
        affects: names(
            fields
                .field_args("AttributeModifierAffects")
                .into_iter()
                .map(str::to_owned)
                .collect(),
        ),
        affects_self: flag(fields, "AttributeModifierAffectsSelf").unwrap_or(false),
        all_sides: flag(fields, "TargetAllSides").unwrap_or(false),
        target_enemy: flag(fields, "TargetEnemy").unwrap_or(false),
        anti: names(
            fields
                .field_args("AntiCategory")
                .into_iter()
                .map(str::to_owned)
                .collect(),
        ),
        re_enable: flag(fields, "ReEnableAntiCategory").unwrap_or(false),
        anti_fx: fields
            .field("AntiFX")
            .filter(|name| !name.eq_ignore_ascii_case("NONE"))
            .map(Name::from),
        modifier_fx: fields
            .field("AttributeModifierFX")
            .filter(|name| !name.eq_ignore_ascii_case("NONE"))
            .map(Name::from),
        affect_good: flag(fields, "AffectGood").unwrap_or(false),
        affect_evil: flag(fields, "AffectEvil").unwrap_or(false),
        affect_allies: flag(fields, "AffectAllies").unwrap_or(true),
    })
}

#[derive(Debug, PartialEq)]
struct RefreshRecipients {
    range: f32,
    affects: crate::effects::Names,
}

#[derive(Debug, PartialEq)]
struct RefreshData {
    base: Arc<PowerData>,
    recipients: Option<RefreshRecipients>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpecialPowerTimerRefreshSpecialPower(Arc<RefreshData>);

impl Component for SpecialPowerTimerRefreshSpecialPower {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for SpecialPowerTimerRefreshSpecialPower {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::SpecialPower);

    fn is_for_power(&self, power: &str) -> bool {
        same(&self.0.base.power, power)
    }

    fn activate(&mut self, activation: &Activation<'_>, ctx: &mut ActivateContext<'_>) {
        if !self.0.base.defer_trigger {
            push_trigger(ctx, self.0.base.fx.as_ref(), activation);
            if let Some(rule) = self.0.base.modifier.clone() {
                push(
                    ctx,
                    Effect::AreaModifier {
                        at: PowerPlace::of(activation),
                        rule,
                    },
                );
            }
        }
        let (range, affects) = self
            .0
            .recipients
            .as_ref()
            .map_or((0.0, crate::effects::Names::from(Vec::new())), |r| {
                (r.range, crate::effects::Names::clone(&r.affects))
            });
        push(
            ctx,
            Effect::RefreshOtherPowers {
                except: Name::clone(&self.0.base.power),
                affect_allies: self.0.recipients.is_some(),
                range,
                affects,
            },
        );
    }
}

pub fn special_power_timer_refresh_special_power_from_fields(
    fields: &dyn ModuleFieldSource,
    now: Frame,
) -> Result<SpecialPowerTimerRefreshSpecialPower, FieldError> {
    let SpecialPowerModule(base) = special_power_module_from_fields(fields, now)?;
    let recipients = (flag(fields, "AffectAllies") == Some(true)).then(|| RefreshRecipients {
        range: fields
            .field_numeric("AttributeModifierRange")
            .filter(|value| value.is_finite() && *value >= 0.0)
            .unwrap_or(0.0),
        affects: names(
            fields
                .field_args("AttributeModifierAffects")
                .into_iter()
                .map(str::to_owned)
                .collect(),
        ),
    });
    Ok(SpecialPowerTimerRefreshSpecialPower(Arc::new(
        RefreshData { base, recipients },
    )))
}

#[derive(Debug, PartialEq)]
struct GrantData {
    power: Name,
    upgrade: Name,
    fx: Option<Name>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerUpgradeSpecialPower(Arc<GrantData>);

impl Component for PlayerUpgradeSpecialPower {
    fn hash_into(&self, _hasher: &mut StateHasher) {}
}

impl ModuleKind for PlayerUpgradeSpecialPower {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::SpecialPower);

    fn is_for_power(&self, power: &str) -> bool {
        same(&self.0.power, power)
    }

    fn activate(&mut self, activation: &Activation<'_>, ctx: &mut ActivateContext<'_>) {
        push_trigger(ctx, self.0.fx.as_ref(), activation);
        push(
            ctx,
            Effect::GrantPlayerUpgrade(Name::clone(&self.0.upgrade)),
        );
    }
}

pub fn player_upgrade_special_power_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<PlayerUpgradeSpecialPower, FieldError> {
    let power = power_of(fields)?;
    let upgrade = fields
        .field("UpgradeName")
        .map(Name::from)
        .ok_or_else(|| FieldError::new("UpgradeName is missing"))?;
    Ok(PlayerUpgradeSpecialPower(Arc::new(GrantData {
        power,
        upgrade,
        fx: trigger_fx(fields),
    })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::effects::PowerSides;
    use mint_sim::{Effects, World};
    use std::collections::BTreeMap;

    struct Fields(BTreeMap<&'static str, &'static str>);
    impl ModuleFieldSource for Fields {
        fn field(&self, keyword: &str) -> Option<&str> {
            self.0.get(keyword).copied()
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

    fn fire(module: &mut impl ModuleKind, activation: &Activation<'_>) -> Vec<Effect> {
        let mut log = EffectLog::new();
        let mut ctx = ActivateContext {
            entity: World::new().spawn().unwrap(),
            now: Frame::ZERO,
            effects: Effects::into_sink(&mut log),
        };
        module.activate(activation, &mut ctx);
        log.drain().into_iter().map(|(_, effect)| effect).collect()
    }

    const AT: Activation<'static> = Activation {
        power: "SpellBookSummon",
        target: None,
        at: Some((100.0, 200.0)),
    };

    #[test]
    fn an_ocl_power_creates_its_list_where_the_activation_points() {
        let mut module = ocl_special_power_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "SpellBookSummon"),
                ("OCL", "OCL_SpawnEgg"),
                ("CreateLocation", "CREATE_AT_LOCATION"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert!(module.is_for_power("spellbooksummon"));
        assert!(!module.is_for_power("SpellBookHeal"));
        assert_eq!(
            fire(&mut module, &AT),
            vec![Effect::CreateObjects {
                list: Name::from("OCL_SpawnEgg"),
                location: CreateLocation::AtLocation,
                at: PowerPlace::At(100.0, 200.0),
                from: crate::effects::OclFrom::Location,
            }]
        );
    }

    #[test]
    fn an_ocl_power_with_re_enable_and_change_weather_clears_the_slot_first() {
        let mut module = ocl_special_power_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "SpellBookSunflare"),
                ("OCL", "OCL_Sunflare"),
                ("ReEnableAntiCategory", "Yes"),
                ("ChangeWeather", "SUNNY"),
                ("WeatherDuration", "5000"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert_eq!(
            fire(&mut module, &AT),
            vec![
                Effect::ClearWeatherEffect,
                Effect::ChangeWeather {
                    kind: WeatherKind::Sunny,
                    lasts: mint_sim::LogicFrames::from_milliseconds(5000),
                },
                Effect::CreateObjects {
                    list: Name::from("OCL_Sunflare"),
                    location: CreateLocation::AtEdgeNearSource,
                    at: PowerPlace::At(100.0, 200.0),
                    from: crate::effects::OclFrom::Location,
                },
            ]
        );
        assert!(ocl_special_power_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "P"),
                ("OCL", "L"),
                ("ChangeWeather", "SOMETHING_ELSE"),
            ]),
            Frame::ZERO,
        )
        .is_err());
    }

    #[test]
    fn an_ocl_power_defaults_to_the_edge_near_the_source_and_needs_its_list() {
        let module = ocl_special_power_from_fields(
            &fields(&[("SpecialPowerTemplate", "P"), ("OCL", "L")]),
            Frame::ZERO,
        )
        .unwrap();
        assert_eq!(module.0.location, CreateLocation::AtEdgeNearSource);
        assert!(ocl_special_power_from_fields(
            &fields(&[("SpecialPowerTemplate", "P")]),
            Frame::ZERO
        )
        .is_err());
        assert!(ocl_special_power_from_fields(&fields(&[("OCL", "L")]), Frame::ZERO).is_err());
    }

    #[test]
    fn use_secondary_object_location_carries_its_own_filter() {
        let module = ocl_special_power_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "P"),
                ("OCL", "L"),
                ("CreateLocation", "USE_SECONDARY_OBJECT_LOCATION"),
                (
                    "NearestSecondaryObjectFilter",
                    "NONE SAME_PLAYER +CASTLE_KEEP",
                ),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        let CreateLocation::UseSecondaryObject(filter) = &module.0.location else {
            panic!("expected UseSecondaryObject, got {:?}", module.0.location);
        };
        assert!(filter.passes(&[String::from("CASTLE_KEEP")], "MenFortress"));
        assert!(!filter.passes(&[String::from("STRUCTURE")], "GondorFarm"));
    }

    #[test]
    fn create_at_edge_near_target_and_move_to_location_is_recognised_by_name() {
        let module = ocl_special_power_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "P"),
                ("OCL", "L"),
                (
                    "CreateLocation",
                    "CREATE_AT_EDGE_NEAR_TARGET_AND_MOVE_TO_LOCATION",
                ),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert_eq!(module.0.location, CreateLocation::AtEdgeNearTargetAndMove);
    }

    #[test]
    fn the_place_follows_what_the_activation_named() {
        let target = World::new().spawn().unwrap();
        let at_object = Activation {
            power: "P",
            target: Some(target),
            at: None,
        };
        assert_eq!(PowerPlace::of(&at_object), PowerPlace::Object(target));
        let targetless = Activation {
            target: None,
            ..at_object
        };
        assert_eq!(PowerPlace::of(&targetless), PowerPlace::Source);
    }

    #[test]
    fn a_targetless_trigger_fx_plays_on_the_caster() {
        let mut heal = player_heal_special_power_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "P"),
                ("HealAmount", "0.5"),
                ("TriggerFX", "FX_Cast"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        let play = |activation: &Activation<'_>, heal: &mut PlayerHealSpecialPower| {
            fire(heal, activation)
                .into_iter()
                .find_map(|effect| match effect {
                    Effect::PlayFx { at, on_object, .. } => Some((at, on_object)),
                    _ => None,
                })
        };
        let target = World::new().spawn().unwrap();
        let at_object = Activation {
            power: "P",
            target: Some(target),
            at: None,
        };
        let targetless = Activation {
            target: None,
            ..at_object
        };
        assert_eq!(
            play(&AT, &mut heal),
            Some((PowerPlace::At(100.0, 200.0), false))
        );
        assert_eq!(
            play(&at_object, &mut heal),
            Some((PowerPlace::Object(target), false))
        );
        assert_eq!(
            play(&targetless, &mut heal),
            Some((PowerPlace::Source, true))
        );
    }

    #[test]
    fn a_heal_reads_percent_by_default_and_hit_points_when_told() {
        let mut heal = player_heal_special_power_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "SpellBookSummon"),
                ("HealAmount", "0.6"),
                ("HealRadius", "150"),
                ("HealAffects", "INFANTRY CAVALRY"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        let Effect::AreaHeal {
            radius,
            amount,
            as_percent,
            affects,
            ..
        } = &fire(&mut heal, &AT)[0]
        else {
            panic!("not a heal");
        };
        assert_eq!((*radius, *amount, *as_percent), (150.0, 0.6, true));
        assert_eq!(affects.len(), 2);
        let rebuild = player_heal_special_power_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "P"),
                ("HealAmount", "4000"),
                ("HealAsPercent", "No"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert!(!rebuild.0.as_percent);
        assert!(player_heal_special_power_from_fields(
            &fields(&[("SpecialPowerTemplate", "P"), ("HealRadius", "NaN")]),
            Frame::ZERO
        )
        .is_err());
    }

    #[test]
    fn a_power_modifier_reaches_friends_unless_told_enemies_or_all() {
        let build = |extra: &[(&'static str, &'static str)]| {
            let mut pairs = vec![
                ("SpecialPowerTemplate", "SpellBookSummon"),
                ("AttributeModifier", "RallyingCall"),
                ("AttributeModifierRange", "100"),
                ("AttributeModifierAffects", "ANY +INFANTRY -HERO"),
            ];
            pairs.extend_from_slice(extra);
            special_power_module_from_fields(&fields(&pairs), Frame::ZERO).unwrap()
        };
        let sides_of = |mut module: SpecialPowerModule| match fire(&mut module, &AT).pop() {
            Some(Effect::AreaModifier { rule, .. }) => {
                assert_eq!(rule.radius, 100.0);
                rule.sides
            }
            other => panic!("{other:?}"),
        };
        assert_eq!(sides_of(build(&[])), PowerSides::Friends);
        assert_eq!(
            sides_of(build(&[("TargetEnemy", "Yes")])),
            PowerSides::Enemies
        );
        assert_eq!(
            sides_of(build(&[("TargetEnemy", "Yes"), ("TargetAllSides", "Yes")])),
            PowerSides::All
        );
    }

    #[test]
    fn anti_category_swaps_the_default_side_to_enemies() {
        let mut module = special_power_module_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "SpecialAbilityElendil"),
                ("AntiCategory", "LEADERSHIP"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        let Effect::AreaModifier { rule, .. } = &fire(&mut module, &AT)[0] else {
            panic!("expected AreaModifier");
        };
        assert_eq!(rule.sides, PowerSides::Enemies);
    }

    #[test]
    fn re_enable_anti_category_reaches_both_sides_but_not_neutral() {
        let mut module = special_power_module_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "P"),
                ("AntiCategory", "LEADERSHIP"),
                ("ReEnableAntiCategory", "Yes"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        let Effect::AreaModifier { rule, .. } = &fire(&mut module, &AT)[0] else {
            panic!("expected AreaModifier");
        };
        assert_eq!(rule.sides, PowerSides::FriendsAndEnemies);
    }

    #[test]
    fn a_deferred_power_pushes_nothing_at_activation() {
        let mut module = special_power_module_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "SpecialAbilityElendil"),
                ("UpdateModuleStartsAttack", "Yes"),
                ("AntiCategory", "LEADERSHIP"),
                ("TriggerFX", "FX_Elendil"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert!(fire(&mut module, &AT).is_empty());
    }

    #[test]
    fn a_plain_power_module_answers_its_power_and_does_nothing() {
        let mut module = special_power_module_from_fields(
            &fields(&[("SpecialPowerTemplate", "SpellBookSummon")]),
            Frame::ZERO,
        )
        .unwrap();
        assert!(module.is_for_power("SpellBookSummon"));
        assert!(fire(&mut module, &AT).is_empty());
    }

    #[test]
    fn a_player_upgrade_power_grants_its_upgrade() {
        let mut module = player_upgrade_special_power_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "SpellBookSummon"),
                ("UpgradeName", "Upgrade_ElvenGift"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert_eq!(
            fire(&mut module, &AT),
            vec![Effect::GrantPlayerUpgrade(Name::from("Upgrade_ElvenGift"))]
        );
        assert!(player_upgrade_special_power_from_fields(
            &fields(&[("SpecialPowerTemplate", "P")]),
            Frame::ZERO
        )
        .is_err());
    }

    #[test]
    fn a_timer_refresh_power_refreshes_every_other_power() {
        let mut module = special_power_timer_refresh_special_power_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "SpecialAbilityElrondRestoration"),
                ("AttributeModifierRange", "150"),
                ("AttributeModifierAffects", "ALL -ElvenElrond ALLIES"),
                ("AffectAllies", "Yes"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert!(module.is_for_power("SpecialAbilityElrondRestoration"));
        assert_eq!(
            fire(&mut module, &AT),
            vec![Effect::RefreshOtherPowers {
                except: Name::from("SpecialAbilityElrondRestoration"),
                affect_allies: true,
                range: 150.0,
                affects: names(vec![
                    "ALL".to_owned(),
                    "-ElvenElrond".to_owned(),
                    "ALLIES".to_owned(),
                ]),
            }]
        );
    }

    #[test]
    fn without_affect_allies_the_refresh_reports_no_recipients() {
        let mut module = special_power_timer_refresh_special_power_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "P"),
                ("AttributeModifierRange", "150"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert_eq!(
            fire(&mut module, &AT),
            vec![Effect::RefreshOtherPowers {
                except: Name::from("P"),
                affect_allies: false,
                range: 0.0,
                affects: names(Vec::new()),
            }]
        );
    }

    #[test]
    fn a_timer_refresh_power_still_reports_its_own_modifier_when_one_is_declared() {
        let mut module = special_power_timer_refresh_special_power_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "P"),
                ("AttributeModifier", "ModBoost"),
                ("AttributeModifierRange", "150"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert!(fire(&mut module, &AT).iter().any(|effect| matches!(
            effect,
            Effect::AreaModifier { rule, .. }
                if rule.list.as_deref() == Some("ModBoost")
        )));
    }
}
