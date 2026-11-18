use std::sync::Arc;

use mint_sim::module::{Capability, CapabilityMask};
use mint_sim::{
    ActivateContext, Activation, Component, FieldError, Frame, LogicFrames, ModuleFieldSource,
    ModuleKind, StateHasher,
};

use crate::effects::{names, CreateLocation, Effect, Name, Names, PowerPlace, WeatherKind};
use crate::special_power_effects::{push, push_trigger, trigger_fx};

fn power_of(fields: &dyn ModuleFieldSource) -> Result<Name, FieldError> {
    fields
        .field("SpecialPowerTemplate")
        .map(Name::from)
        .ok_or_else(|| FieldError::new("SpecialPowerTemplate is missing"))
}

fn name(fields: &dyn ModuleFieldSource, keyword: &str) -> Option<Name> {
    fields
        .field(keyword)
        .filter(|value| !value.eq_ignore_ascii_case("NONE"))
        .map(Name::from)
}

fn tokens(fields: &dyn ModuleFieldSource, keyword: &str) -> Names {
    names(
        fields
            .field_args(keyword)
            .into_iter()
            .map(str::to_owned)
            .collect(),
    )
}

fn flag(fields: &dyn ModuleFieldSource, keyword: &str) -> bool {
    fields
        .field(keyword)
        .and_then(mint_sage_ini::value::parse_bool)
        .unwrap_or(false)
}

fn number(fields: &dyn ModuleFieldSource, keyword: &str) -> f32 {
    fields
        .field(keyword)
        .and_then(|raw| match raw.strip_suffix('%') {
            Some(text) => mint_sage_ini::value::parse_float(text).map(|v| v / 100.0),
            None => fields.field_numeric(keyword),
        })
        .filter(|value| value.is_finite() && *value >= 0.0)
        .unwrap_or(0.0)
}

fn duration(fields: &dyn ModuleFieldSource, keyword: &str) -> LogicFrames {
    let ms = number(fields, keyword).min(86_400_000.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = ms as u32;
    LogicFrames::from_milliseconds(whole)
}

fn checked_number(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<f32, FieldError> {
    match fields.field(keyword) {
        None => Ok(0.0),
        Some(raw) => fields
            .field_numeric(keyword)
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not a number"))),
    }
}

fn checked_percent(fields: &dyn ModuleFieldSource, keyword: &str) -> Result<f32, FieldError> {
    match fields.field(keyword) {
        None => Ok(0.0),
        Some(raw) => fields
            .field_percent(keyword)
            .filter(|value| value.is_finite() && *value >= 0.0)
            .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not a percentage"))),
    }
}

fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn change_weather(fields: &dyn ModuleFieldSource) -> Result<Option<WeatherKind>, FieldError> {
    match fields.field("ChangeWeather") {
        None => Ok(None),
        Some(raw) => WeatherKind::parse(raw)
            .map(Some)
            .ok_or_else(|| FieldError::new(format!("ChangeWeather '{raw}' is not a weather"))),
    }
}

fn push_change_weather(
    ctx: &mut ActivateContext<'_>,
    kind: Option<WeatherKind>,
    lasts: LogicFrames,
) {
    if let Some(kind) = kind {
        push(ctx, Effect::ChangeWeather { kind, lasts });
    }
}

#[derive(Debug, PartialEq)]
struct Spell<T> {
    power: Name,
    fx: Option<Name>,
    own: T,
}

macro_rules! spell_module {
    ($(#[$doc:meta])* $type:ident, $data:ty) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq)]
        pub struct $type(Arc<Spell<$data>>);

        impl Component for $type {
            fn hash_into(&self, _hasher: &mut StateHasher) {}
        }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GroundKind {
    Taint,
    ElvenWood,
}

#[derive(Debug, PartialEq)]
struct Ground {
    kind: GroundKind,
    object: Option<Name>,
    fx: Option<Name>,
    ocl: Option<Name>,
    radius: f32,
}

spell_module!(TaintSpecialPower, Ground);

spell_module!(ElvenWoodSpecialPower, Ground);

fn activate_ground(
    spell: &Spell<Ground>,
    activation: &Activation<'_>,
    ctx: &mut ActivateContext<'_>,
) {
    let at = PowerPlace::of(activation);
    push_trigger(ctx, spell.fx.as_ref(), activation);
    let own = &spell.own;
    push(
        ctx,
        Effect::MorphTrees {
            at,
            radius: own.radius,
            tainted: own.kind == GroundKind::ElvenWood,
        },
    );
    if let Some(list) = &own.fx {
        push(
            ctx,
            Effect::PlayFx {
                list: Name::clone(list),
                at,
                on_object: false,
                secondary: None,
            },
        );
    }
    if let Some(list) = &own.ocl {
        push(
            ctx,
            Effect::CreateObjects {
                list: Name::clone(list),
                location: CreateLocation::AtLocation,
                at,
                from: crate::effects::OclFrom::Location,
            },
        );
    }
    if let Some(object) = &own.object {
        let name = Name::clone(object);
        push(
            ctx,
            match own.kind {
                GroundKind::Taint => Effect::CreateObject { name, at },
                GroundKind::ElvenWood => Effect::AddObject { name, at },
            },
        );
    }
}

macro_rules! ground_module {
    ($type:ident) => {
        impl $type {
            #[must_use]
            pub fn object(&self) -> Option<&str> {
                self.0.own.object.as_deref()
            }

            #[must_use]
            pub fn radius(&self) -> f32 {
                self.0.own.radius
            }
        }

        impl ModuleKind for $type {
            const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::SpecialPower);

            fn is_for_power(&self, power: &str) -> bool {
                same(&self.0.power, power)
            }

            fn activate(&mut self, activation: &Activation<'_>, ctx: &mut ActivateContext<'_>) {
                activate_ground(&self.0, activation, ctx);
            }
        }
    };
}

ground_module!(TaintSpecialPower);
ground_module!(ElvenWoodSpecialPower);

fn ground_from_fields(
    fields: &dyn ModuleFieldSource,
    kind: GroundKind,
    [object, radius, fx, ocl]: [&str; 4],
) -> Result<Arc<Spell<Ground>>, FieldError> {
    Ok(Arc::new(Spell {
        power: power_of(fields)?,
        fx: trigger_fx(fields),
        own: Ground {
            kind,
            object: name(fields, object),
            fx: name(fields, fx),
            ocl: name(fields, ocl),
            radius: checked_number(fields, radius)?,
        },
    }))
}

pub fn taint_special_power_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<TaintSpecialPower, FieldError> {
    ground_from_fields(
        fields,
        GroundKind::Taint,
        ["TaintObject", "TaintRadius", "TaintFX", "TaintOCL"],
    )
    .map(TaintSpecialPower)
}

pub fn elven_wood_special_power_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<ElvenWoodSpecialPower, FieldError> {
    ground_from_fields(
        fields,
        GroundKind::ElvenWood,
        [
            "ElvenGroveObject",
            "ElvenWoodRadius",
            "ElvenWoodFX",
            "ElvenWoodOCL",
        ],
    )
    .map(ElvenWoodSpecialPower)
}

#[derive(Debug, PartialEq)]
struct Darkness {
    list: Option<Name>,
    affects: Names,
    affect_good: bool,
    affect_evil: bool,
    lasts: LogicFrames,
    weather: Option<WeatherKind>,
}

spell_module!(DarknessSpecialPower, Darkness);

impl ModuleKind for DarknessSpecialPower {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::SpecialPower);

    fn is_for_power(&self, power: &str) -> bool {
        same(&self.0.power, power)
    }

    fn activate(&mut self, activation: &Activation<'_>, ctx: &mut ActivateContext<'_>) {
        push_trigger(ctx, self.0.fx.as_ref(), activation);
        let own = &self.0.own;
        if own.list.is_some() {
            push(
                ctx,
                Effect::SetWeatherEffect {
                    list: own.list.clone(),
                    anti: Names::default(),
                    affects: Names::clone(&own.affects),
                    affect_good: own.affect_good,
                    affect_evil: own.affect_evil,
                    lasts: own.lasts,
                },
            );
        }
        push_change_weather(ctx, own.weather, own.lasts);
    }
}

pub fn darkness_special_power_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<DarknessSpecialPower, FieldError> {
    Ok(DarknessSpecialPower(Arc::new(Spell {
        power: power_of(fields)?,
        fx: trigger_fx(fields),
        own: Darkness {
            list: name(fields, "AttributeModifier"),
            affects: tokens(fields, "AttributeModifierAffects"),
            affect_good: flag(fields, "AffectGood"),
            affect_evil: flag(fields, "AffectEvil"),
            lasts: duration(fields, "WeatherDuration"),
            weather: change_weather(fields)?,
        },
    })))
}

#[derive(Debug, PartialEq)]
struct FreezingRain {
    categories: Names,
    affects: Names,
    affect_good: bool,
    affect_evil: bool,
    lasts: LogicFrames,
    weather: Option<WeatherKind>,
}

spell_module!(FreezingRainSpecialPower, FreezingRain);

impl ModuleKind for FreezingRainSpecialPower {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::SpecialPower);

    fn is_for_power(&self, power: &str) -> bool {
        same(&self.0.power, power)
    }

    fn activate(&mut self, activation: &Activation<'_>, ctx: &mut ActivateContext<'_>) {
        push_trigger(ctx, self.0.fx.as_ref(), activation);
        let own = &self.0.own;
        if !own.categories.is_empty() {
            push(
                ctx,
                Effect::SetWeatherEffect {
                    list: None,
                    anti: Names::clone(&own.categories),
                    affects: Names::clone(&own.affects),
                    affect_good: own.affect_good,
                    affect_evil: own.affect_evil,
                    lasts: own.lasts,
                },
            );
        }
        push_change_weather(ctx, own.weather, own.lasts);
    }
}

pub fn freezing_rain_special_power_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<FreezingRainSpecialPower, FieldError> {
    Ok(FreezingRainSpecialPower(Arc::new(Spell {
        power: power_of(fields)?,
        fx: trigger_fx(fields),
        own: FreezingRain {
            categories: tokens(fields, "AntiCategory"),
            affects: tokens(fields, "AttributeModifierAffects"),
            affect_good: flag(fields, "AffectGood"),
            affect_evil: flag(fields, "AffectEvil"),
            lasts: duration(fields, "WeatherDuration"),
            weather: change_weather(fields)?,
        },
    })))
}

#[derive(Debug, PartialEq)]
struct CloudBreak {
    sunbeam: Option<Name>,
    spacing: f32,
    affects: Names,
    re_enable: bool,
    lasts: LogicFrames,
    weather: Option<WeatherKind>,
}

spell_module!(CloudBreakSpecialPower, CloudBreak);

impl ModuleKind for CloudBreakSpecialPower {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::SpecialPower);

    fn is_for_power(&self, power: &str) -> bool {
        same(&self.0.power, power)
    }

    fn activate(&mut self, activation: &Activation<'_>, ctx: &mut ActivateContext<'_>) {
        push_trigger(ctx, self.0.fx.as_ref(), activation);
        let own = &self.0.own;
        if own.re_enable {
            push(ctx, Effect::ClearWeatherEffect);
        }
        push(
            ctx,
            Effect::FearEveryoneAdmitted {
                affects: Names::clone(&own.affects),
            },
        );
        push_change_weather(ctx, own.weather, own.lasts);
        if let Some(sunbeam) = &own.sunbeam {
            push(
                ctx,
                Effect::SpacedObjects {
                    name: Name::clone(sunbeam),
                    affects: Names::clone(&own.affects),
                    spacing: own.spacing,
                },
            );
        }
    }
}

pub fn cloud_break_special_power_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<CloudBreakSpecialPower, FieldError> {
    Ok(CloudBreakSpecialPower(Arc::new(Spell {
        power: power_of(fields)?,
        fx: trigger_fx(fields),
        own: CloudBreak {
            sunbeam: name(fields, "SunbeamObject"),
            spacing: number(fields, "ObjectSpacing"),
            affects: tokens(fields, "AttributeModifierAffects"),
            re_enable: flag(fields, "ReEnableAntiCategory"),
            lasts: duration(fields, "WeatherDuration"),
            weather: change_weather(fields)?,
        },
    })))
}

#[derive(Debug, PartialEq)]
struct Devastate {
    fx: Option<Name>,
    weapon: Option<Name>,
    radius: f32,
    multiplier: f32,
    cap: f32,
}

spell_module!(DevastateSpecialPower, Devastate);

impl ModuleKind for DevastateSpecialPower {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::SpecialPower);

    fn is_for_power(&self, power: &str) -> bool {
        same(&self.0.power, power)
    }

    fn activate(&mut self, activation: &Activation<'_>, ctx: &mut ActivateContext<'_>) {
        let at = PowerPlace::of(activation);
        push_trigger(ctx, self.0.fx.as_ref(), activation);
        let own = &self.0.own;
        push(
            ctx,
            Effect::FellTreesForMoney {
                at,
                radius: own.radius,
                fx: own.fx.clone(),
                multiplier: own.multiplier,
                cap: own.cap,
            },
        );
        if let Some(weapon) = &own.weapon {
            push(
                ctx,
                Effect::FireWeapon {
                    weapon: Name::clone(weapon),
                    at,
                    offset: (0.0, 0.0),
                    repeat: None,
                    alive_only: false,
                    reload_slot: None,
                },
            );
        }
    }
}

pub fn devastate_special_power_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<DevastateSpecialPower, FieldError> {
    Ok(DevastateSpecialPower(Arc::new(Spell {
        power: power_of(fields)?,
        fx: trigger_fx(fields),
        own: Devastate {
            fx: name(fields, "FX"),
            weapon: name(fields, "FireWeapon"),
            radius: checked_number(fields, "Radius")?,
            multiplier: checked_percent(fields, "TreeValueMultiplier")?,
            cap: checked_number(fields, "TreeValueTotalCap")?,
        },
    })))
}

spell_module!(ScavengerSpecialPower, f32);

impl ModuleKind for ScavengerSpecialPower {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::SpecialPower);

    fn is_for_power(&self, power: &str) -> bool {
        same(&self.0.power, power)
    }

    fn activate(&mut self, activation: &Activation<'_>, ctx: &mut ActivateContext<'_>) {
        push_trigger(ctx, self.0.fx.as_ref(), activation);
        push(ctx, Effect::CashBounty(self.0.own));
    }
}

pub fn scavenger_special_power_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<ScavengerSpecialPower, FieldError> {
    Ok(ScavengerSpecialPower(Arc::new(Spell {
        power: power_of(fields)?,
        fx: trigger_fx(fields),
        own: number(fields, "BountyPercent").min(1000.0),
    })))
}

#[derive(Debug, PartialEq)]
struct Allegiance {
    radius: f32,
    affects: Names,
}

spell_module!(UntamedAllegianceSpecialPower, Allegiance);

impl ModuleKind for UntamedAllegianceSpecialPower {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::SpecialPower);

    fn is_for_power(&self, power: &str) -> bool {
        same(&self.0.power, power)
    }

    fn activate(&mut self, activation: &Activation<'_>, ctx: &mut ActivateContext<'_>) {
        push_trigger(ctx, self.0.fx.as_ref(), activation);
        push(
            ctx,
            Effect::Capture {
                at: PowerPlace::of(activation),
                radius: self.0.own.radius,
                affects: Names::clone(&self.0.own.affects),
            },
        );
    }
}

pub fn untamed_allegiance_special_power_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<UntamedAllegianceSpecialPower, FieldError> {
    Ok(UntamedAllegianceSpecialPower(Arc::new(Spell {
        power: power_of(fields)?,
        fx: trigger_fx(fields),
        own: Allegiance {
            radius: number(fields, "AttributeModifierRange"),
            affects: tokens(fields, "AttributeModifierAffects"),
        },
    })))
}

#[derive(Debug, PartialEq)]
struct Production {
    kinds: Names,
    multiplier: f32,
    lasts: LogicFrames,
}

spell_module!(ProductionSpeedBonus, Production);

impl ModuleKind for ProductionSpeedBonus {
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::SpecialPower);

    fn is_for_power(&self, power: &str) -> bool {
        same(&self.0.power, power)
    }

    fn activate(&mut self, activation: &Activation<'_>, ctx: &mut ActivateContext<'_>) {
        push_trigger(ctx, self.0.fx.as_ref(), activation);
        let own = &self.0.own;
        push(
            ctx,
            Effect::ProductionBonus {
                kinds: Names::clone(&own.kinds),
                multiplier: own.multiplier,
                lasts: own.lasts,
            },
        );
    }
}

pub fn production_speed_bonus_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<ProductionSpeedBonus, FieldError> {
    let shipped = number(fields, "SpeedMulitplier");
    let multiplier = if shipped > 0.0 {
        shipped
    } else {
        number(fields, "SpeedMultiplier")
    };
    let frames = number(fields, "NumberOfFrames").min(2_592_000.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = frames as u32;
    Ok(ProductionSpeedBonus(Arc::new(Spell {
        power: power_of(fields)?,
        fx: trigger_fx(fields),
        own: Production {
            kinds: tokens(fields, "Type"),
            multiplier: multiplier.min(1000.0),
            lasts: LogicFrames::from_frames(frames),
        },
    })))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::effects::EffectLog;
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
        power: "SpellBookTaint",
        target: None,
        at: Some((10.0, 20.0)),
    };

    #[test]
    fn taint_and_elven_wood_morph_then_create_their_ground() {
        let mut taint = taint_special_power_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "SpellBookTaint"),
                ("TaintObject", "TaintLand"),
                ("TaintRadius", "250"),
                ("TaintFX", "FX_TaintSpellFX"),
                ("TaintOCL", "OCL_TaintSeed"),
                ("TriggerFX", "FX_Cast"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert!(taint.is_for_power("spellbooktaint"));
        let at = PowerPlace::At(10.0, 20.0);
        assert_eq!(
            fire(&mut taint, &AT),
            vec![
                Effect::PlayFx {
                    list: Name::from("FX_Cast"),
                    at,
                    on_object: false,
                    secondary: None,
                },
                Effect::MorphTrees {
                    at,
                    radius: 250.0,
                    tainted: false,
                },
                Effect::PlayFx {
                    list: Name::from("FX_TaintSpellFX"),
                    at,
                    on_object: false,
                    secondary: None,
                },
                Effect::CreateObjects {
                    list: Name::from("OCL_TaintSeed"),
                    location: CreateLocation::AtLocation,
                    at,
                    from: crate::effects::OclFrom::Location,
                },
                Effect::CreateObject {
                    name: Name::from("TaintLand"),
                    at
                },
            ]
        );
        assert_eq!(taint.radius(), 250.0);
        let mut wood = elven_wood_special_power_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "SpellBookElvenWood"),
                ("ElvenGroveObject", "ElvenGrove"),
                ("ElvenWoodOCL", "OCL_ElvenWoodSeed"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert_eq!(wood.object(), Some("ElvenGrove"));
        assert_eq!(
            fire(&mut wood, &AT),
            vec![
                Effect::MorphTrees {
                    at,
                    radius: 0.0,
                    tainted: true,
                },
                Effect::CreateObjects {
                    list: Name::from("OCL_ElvenWoodSeed"),
                    location: CreateLocation::AtLocation,
                    at,
                    from: crate::effects::OclFrom::Location,
                },
                Effect::AddObject {
                    name: Name::from("ElvenGrove"),
                    at
                },
            ]
        );
        assert!(taint_special_power_from_fields(&fields(&[]), Frame::ZERO).is_err());
    }

    #[test]
    fn darkness_sets_the_weather_effect_slot_and_changes_the_weather() {
        let mut module = darkness_special_power_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "SpellBookDarkness"),
                ("AttributeModifier", "SpellBookDarkness"),
                ("AttributeModifierAffects", "ANY +INFANTRY -HERO ALLIES"),
                ("WeatherDuration", "60000"),
                ("ChangeWeather", "CLOUDY"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        let effects = fire(&mut module, &AT);
        let [Effect::SetWeatherEffect {
            list,
            anti,
            affects,
            lasts,
            ..
        }, Effect::ChangeWeather {
            kind: WeatherKind::Cloudy,
            lasts: weather_lasts,
        }] = effects.as_slice()
        else {
            panic!("{effects:?}");
        };
        assert_eq!(list.as_deref(), Some("SpellBookDarkness"));
        assert!(anti.is_empty());
        assert_eq!(affects.len(), 4);
        assert_eq!(lasts.get(), 1800, "60 s");
        assert_eq!(weather_lasts.get(), 1800);
    }

    #[test]
    fn freezing_rain_suppresses_its_anti_category_and_needs_one() {
        let build = |extra: &[(&'static str, &'static str)]| {
            let mut pairs = vec![
                ("SpecialPowerTemplate", "SpellBookFreezingRain"),
                ("AttributeModifierAffects", "ALL ENEMIES"),
                ("WeatherDuration", "30000"),
            ];
            pairs.extend_from_slice(extra);
            freezing_rain_special_power_from_fields(&fields(&pairs), Frame::ZERO).unwrap()
        };
        let effects = fire(&mut build(&[("AntiCategory", "LEADERSHIP")]), &AT);
        assert!(matches!(
            effects.as_slice(),
            [Effect::SetWeatherEffect { anti, lasts, .. }]
                if &*anti[0] == "LEADERSHIP" && lasts.get() == 900
        ));
        assert!(fire(&mut build(&[]), &AT).is_empty());
    }

    #[test]
    fn cloud_break_lifts_suppression_only_when_told_and_spaces_its_sunbeams() {
        let build = |re_enable: &'static str| {
            cloud_break_special_power_from_fields(
                &fields(&[
                    ("SpecialPowerTemplate", "SpellBookCloudBreak"),
                    ("SunbeamObject", "CloudBreakSunbeam"),
                    ("ObjectSpacing", "300"),
                    ("AttributeModifierAffects", "ANY +INFANTRY ENEMIES"),
                    ("ReEnableAntiCategory", re_enable),
                ]),
                Frame::ZERO,
            )
            .unwrap()
        };
        let on = fire(&mut build("Yes"), &AT);
        assert_eq!(on.first(), Some(&Effect::ClearWeatherEffect));
        assert!(matches!(
            on.get(1),
            Some(Effect::FearEveryoneAdmitted { .. })
        ));
        assert!(
            matches!(on.get(2), Some(Effect::SpacedObjects { spacing, .. }) if *spacing == 300.0)
        );
        let off = fire(&mut build("No"), &AT);
        assert_eq!(
            off.len(),
            2,
            "no ClearWeatherEffect without ReEnableAntiCategory"
        );
    }

    #[test]
    fn devastate_fells_trees_for_money_then_fires_its_weapon() {
        let mut module = devastate_special_power_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "SpellBookDevastation"),
                ("FX", "FX_Devastation"),
                ("FireWeapon", "DevastationEntWeapon"),
                ("Radius", "200"),
                ("TreeValueMultiplier", "50%"),
                ("TreeValueTotalCap", "1500"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        let at = PowerPlace::At(10.0, 20.0);
        assert_eq!(
            fire(&mut module, &AT),
            vec![
                Effect::FellTreesForMoney {
                    at,
                    radius: 200.0,
                    fx: Some(Name::from("FX_Devastation")),
                    multiplier: 0.5,
                    cap: 1500.0,
                },
                Effect::FireWeapon {
                    weapon: Name::from("DevastationEntWeapon"),
                    at,
                    offset: (0.0, 0.0),
                    repeat: None,
                    alive_only: false,
                    reload_slot: None
                },
            ]
        );
    }

    #[test]
    fn an_unreadable_tree_field_refuses_the_module() {
        for (keyword, raw) in [
            ("TreeValueMultiplier", "half"),
            ("TreeValueTotalCap", "lots"),
            ("Radius", "wide"),
        ] {
            let read = devastate_special_power_from_fields(
                &fields(&[
                    ("SpecialPowerTemplate", "SpellBookDevastation"),
                    (keyword, raw),
                ]),
                Frame::ZERO,
            );
            assert!(read.is_err(), "{keyword} = {raw}");
        }
    }

    #[test]
    fn scavenger_sets_the_bounty_as_written_or_as_a_percentage() {
        for raw in ["1.0", "100%"] {
            let mut module = scavenger_special_power_from_fields(
                &fields(&[
                    ("SpecialPowerTemplate", "SpellBookScavenger"),
                    ("BountyPercent", raw),
                ]),
                Frame::ZERO,
            )
            .unwrap();
            assert_eq!(
                fire(&mut module, &AT),
                vec![Effect::CashBounty(1.0)],
                "{raw}"
            );
        }
    }

    #[test]
    fn untamed_allegiance_captures_in_its_range() {
        let mut module = untamed_allegiance_special_power_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "SpellBookUntamedAllegiance"),
                ("AttributeModifierRange", "60"),
                ("AttributeModifierAffects", "ANY +CREEP"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert!(matches!(
            fire(&mut module, &AT).as_slice(),
            [Effect::Capture { radius, .. }] if *radius == 60.0
        ));
    }

    #[test]
    fn production_speed_bonus_reads_the_shipped_spelling() {
        let mut module = production_speed_bonus_from_fields(
            &fields(&[
                ("SpecialPowerTemplate", "SpellBookCalltheHorde"),
                ("NumberOfFrames", "180"),
                ("SpeedMulitplier", "20.0"),
                ("Type", "MordorFighterHorde MordorArcherHorde"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert!(matches!(
            fire(&mut module, &AT).as_slice(),
            [Effect::ProductionBonus { kinds, multiplier, lasts }]
                if kinds.len() == 2 && *multiplier == 20.0 && lasts.get() == 180
        ));
    }
}
