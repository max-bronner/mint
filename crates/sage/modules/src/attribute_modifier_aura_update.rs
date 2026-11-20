use std::sync::Arc;

use mint_sage_ini::value::parse_bool;
use mint_sim::module::{Capability, CapabilityMask, ModuleKind, Sleep, UpdateContext, UpgradeMask};
use mint_sim::{
    Component, FieldError, Frame, LogicFrames, ModuleFieldSource, StateHasher, UpgradeContext,
};

use crate::effects::{names, Effect, EffectLog, Name, Names};
use crate::special_ability_update::TICKS_PER_RETAIL_FRAME;
use crate::upgrade_mux::{sooner, UpgradeMuxData};

#[derive(Debug, PartialEq)]
struct AuraData {
    starts_active: bool,
    bonus_name: Option<Name>,
    refresh: LogicFrames,
    refresh_delay_raw: Option<f32>,
    object_filter: Names,
    range: Option<f32>,
    anti_category: Vec<String>,
    anti_fx: Option<Name>,
    target_enemy: bool,
    affect_good: bool,
    affect_evil: bool,
    allow_while_attacking: bool,
    max_active_rank: Option<f32>,
    allow_self: bool,
    run_while_dead: bool,
    affect_contained_only: bool,
    required_conditions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AttributeModifierAuraUpdate {
    data: Arc<AuraData>,
    mux: UpgradeMuxData,
    active: bool,
}

impl AttributeModifierAuraUpdate {
    #[must_use]
    pub fn starts_active(&self) -> bool {
        self.data.starts_active
    }

    #[must_use]
    pub const fn is_active(&self) -> bool {
        self.active
    }

    #[must_use]
    pub fn bonus_name(&self) -> Option<&str> {
        self.data.bonus_name.as_deref()
    }

    #[must_use]
    pub fn refresh_delay_raw(&self) -> Option<f32> {
        self.data.refresh_delay_raw
    }

    #[must_use]
    pub fn refresh(&self) -> LogicFrames {
        self.data.refresh
    }

    #[must_use]
    pub fn object_filter(&self) -> &[Name] {
        &self.data.object_filter
    }

    #[must_use]
    pub fn range(&self) -> Option<f32> {
        self.data.range
    }

    #[must_use]
    pub fn triggered_by(&self) -> &UpgradeMask {
        self.mux.triggered_by()
    }

    #[must_use]
    pub fn anti_category(&self) -> &[String] {
        &self.data.anti_category
    }

    #[must_use]
    pub fn target_enemy(&self) -> bool {
        self.data.target_enemy
    }

    #[must_use]
    pub fn max_active_rank(&self) -> Option<f32> {
        self.data.max_active_rank
    }

    #[must_use]
    pub fn allow_self(&self) -> bool {
        self.data.allow_self
    }

    #[must_use]
    pub fn run_while_dead(&self) -> bool {
        self.data.run_while_dead
    }

    #[must_use]
    pub fn affect_contained_only(&self) -> bool {
        self.data.affect_contained_only
    }

    #[must_use]
    pub fn required_conditions(&self) -> &[String] {
        &self.data.required_conditions
    }
}

impl Component for AttributeModifierAuraUpdate {
    fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_bool(self.active);
        self.mux.hash_into(hasher);
    }
}

impl ModuleKind for AttributeModifierAuraUpdate {
    const UPDATES_UNDER: u32 = crate::disabled_masks::NEVER;
    const CAPABILITIES: CapabilityMask = CapabilityMask::single(Capability::Update)
        .union(CapabilityMask::single(Capability::Upgrade));

    fn on_attach(&mut self, _now: Frame) {
        self.active = self.data.starts_active;
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

impl AttributeModifierAuraUpdate {
    fn own_update(&mut self, ctx: &mut UpdateContext<'_>) -> Sleep {
        if !self.active {
            return Sleep::Forever;
        }
        let Some(list) = &self.data.bonus_name else {
            return Sleep::Forever;
        };
        let entity = ctx.entity;
        if let Some(log) = ctx.effects.get::<EffectLog>() {
            log.push(
                entity,
                Effect::Aura {
                    list: Name::clone(list),
                    radius: self.data.range,
                    affects: Names::clone(&self.data.object_filter),
                    target_enemy: self.data.target_enemy,
                    affect_good: self.data.affect_good,
                    affect_evil: self.data.affect_evil,
                    allow_self: self.data.allow_self,
                    while_dead: self.data.run_while_dead,
                    anti: names(self.data.anti_category.clone()),
                    anti_fx: self.data.anti_fx.clone(),
                    max_active_rank: self.data.max_active_rank,
                    contained_only: self.data.affect_contained_only,
                    required_mounted: self
                        .data
                        .required_conditions
                        .iter()
                        .any(|name| name.eq_ignore_ascii_case("MOUNTED")),
                    allow_while_attacking: self.data.allow_while_attacking,
                },
            );
        }
        Sleep::Frames(LogicFrames::from_frames(
            self.data
                .refresh
                .get()
                .saturating_add((entity.index() % 5).saturating_mul(TICKS_PER_RETAIL_FRAME)),
        ))
    }
}

pub fn attribute_modifier_aura_update_from_fields(
    fields: &dyn ModuleFieldSource,
    _now: Frame,
) -> Result<AttributeModifierAuraUpdate, FieldError> {
    let refresh_delay_raw = fields.field_numeric("RefreshDelay");
    let refresh = match refresh_delay_raw {
        Some(ms) if ms.is_finite() && ms >= 1.0 => {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let whole = ms.min(86_400_000.0) as u32;
            crate::special_ability_update::retail_duration_ticks(whole)
        }
        _ => LogicFrames::from_engine_milliseconds(1000),
    };
    let refresh = if refresh.is_zero() {
        LogicFrames::ONE
    } else {
        refresh
    };
    let list = |keyword: &str| -> Vec<String> {
        fields
            .field_args(keyword)
            .into_iter()
            .map(str::to_owned)
            .collect()
    };
    let starts_active = optional_bool(fields, "StartsActive")?.unwrap_or(false);
    Ok(AttributeModifierAuraUpdate {
        data: Arc::new(AuraData {
            starts_active,
            bonus_name: fields
                .field("BonusName")
                .filter(|name| !name.eq_ignore_ascii_case("NONE"))
                .map(Name::from),
            refresh,
            refresh_delay_raw,
            object_filter: names(list("ObjectFilter")),
            range: fields
                .field_numeric("Range")
                .filter(|range| range.is_finite() && *range >= 0.0),
            anti_category: list("AntiCategory"),
            anti_fx: fields
                .field("AntiFX")
                .filter(|name| !name.eq_ignore_ascii_case("NONE"))
                .map(Name::from),
            target_enemy: optional_bool(fields, "TargetEnemy")?.unwrap_or(false),
            affect_good: optional_bool(fields, "AffectGood")?.unwrap_or(false),
            affect_evil: optional_bool(fields, "AffectEvil")?.unwrap_or(false),
            allow_while_attacking: optional_bool(fields, "AllowPowerWhenAttacking")?
                .unwrap_or(true),
            max_active_rank: fields.field_numeric("MaxActiveRank"),
            allow_self: optional_bool(fields, "AllowSelf")?.unwrap_or(false),
            run_while_dead: optional_bool(fields, "RunWhileDead")?.unwrap_or(false),
            affect_contained_only: optional_bool(fields, "AffectContainedOnly")?.unwrap_or(false),
            required_conditions: list("RequiredConditions"),
        }),
        mux: UpgradeMuxData::from_fields(fields)?,
        active: starts_active,
    })
}

fn optional_bool(
    fields: &dyn ModuleFieldSource,
    keyword: &str,
) -> Result<Option<bool>, FieldError> {
    match fields.field(keyword) {
        None => Ok(None),
        Some(raw) => parse_bool(raw)
            .map(Some)
            .ok_or_else(|| FieldError::new(format!("{keyword} '{raw}' is not Yes/No"))),
    }
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

    #[test]
    fn no_fields_at_all_succeeds() {
        let module = attribute_modifier_aura_update_from_fields(&fields(&[]), Frame::ZERO).unwrap();
        assert!(!module.starts_active());
        assert_eq!(module.bonus_name(), None);
        assert!(module.triggered_by().is_empty());
    }

    #[test]
    fn a_shipped_shape_declaration_reads_its_fields() {
        let module = attribute_modifier_aura_update_from_fields(
            &fields(&[
                ("StartsActive", "No"),
                ("BonusName", "GenericHeroLeadership"),
                ("RefreshDelay", "2000"),
                ("Range", "200"),
                ("ObjectFilter", "ANY +INFANTRY"),
                ("AntiCategory", "LEADERSHIP BUFF"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert!(!module.starts_active());
        assert_eq!(module.bonus_name(), Some("GenericHeroLeadership"));
        assert_eq!(module.refresh_delay_raw(), Some(2000.0));
        assert_eq!(module.refresh().get(), 60, "2000 ms is 60 frames");
        assert_eq!(module.range(), Some(200.0));
        assert_eq!(module.anti_category(), ["LEADERSHIP", "BUFF"]);
    }

    #[test]
    fn an_unparseable_bool_is_refused() {
        assert!(attribute_modifier_aura_update_from_fields(
            &fields(&[("TargetEnemy", "Sideways")]),
            Frame::ZERO
        )
        .is_err());
    }

    fn pulses(
        module: AttributeModifierAuraUpdate,
        frames: u64,
        upgrade_at: Option<u64>,
    ) -> Vec<(u64, Effect)> {
        pulses_of_entity(module, frames, upgrade_at, 0)
    }

    fn pulses_of_entity(
        module: AttributeModifierAuraUpdate,
        frames: u64,
        upgrade_at: Option<u64>,
        before: usize,
    ) -> Vec<(u64, Effect)> {
        let mut world = World::new();
        let mut registry = ModuleRegistry::new(&mut world);
        registry.register::<AttributeModifierAuraUpdate>(&mut world, "AttributeModifierAuraUpdate");
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
    fn a_starting_aura_pulses_every_refresh_delay() {
        let module = attribute_modifier_aura_update_from_fields(
            &fields(&[
                ("StartsActive", "Yes"),
                ("BonusName", "GenericBuff"),
                ("RefreshDelay", "2000"),
                ("Range", "250"),
                ("ObjectFilter", "ANY +INFANTRY"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        let got = pulses(module, 125, None);
        let frames: Vec<u64> = got.iter().map(|(f, _)| *f).collect();
        assert_eq!(frames, vec![0, 60, 120]);
        let Effect::Aura {
            list,
            radius,
            target_enemy,
            ..
        } = &got[0].1
        else {
            panic!("{:?}", got[0].1);
        };
        assert_eq!(
            (&**list, *radius, *target_enemy),
            ("GenericBuff", Some(250.0), false)
        );
    }

    #[test]
    fn the_pulse_spread_by_object_id_is_in_retail_frames() {
        let module = attribute_modifier_aura_update_from_fields(
            &fields(&[
                ("StartsActive", "Yes"),
                ("BonusName", "GenericBuff"),
                ("RefreshDelay", "2000"),
                ("Range", "250"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        let got = pulses_of_entity(module, 200, None, 4);
        let frames: Vec<u64> = got.iter().map(|(f, _)| *f).collect();
        assert_eq!(frames, vec![0, 84, 168]);
    }

    #[test]
    fn an_upgrade_switches_an_aura_on_for_good() {
        let module = attribute_modifier_aura_update_from_fields(
            &fields(&[
                ("StartsActive", "No"),
                ("TriggeredBy", "Upgrade_AragornLeadership"),
                ("BonusName", "GenericHeroLeadership"),
                ("RefreshDelay", "2000"),
                ("Range", "200"),
                ("TargetEnemy", "Yes"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        assert!(pulses(module.clone(), 200, None).is_empty());
        let got = pulses(module, 200, Some(70));
        let frames: Vec<u64> = got.iter().map(|(f, _)| *f).collect();
        assert_eq!(
            frames,
            vec![70, 130, 190],
            "on from the frame the upgrade arrives, then every pulse"
        );
        assert!(matches!(
            got[0].1,
            Effect::Aura {
                target_enemy: true,
                ..
            }
        ));
    }

    #[test]
    fn an_aura_with_no_range_still_pulses_and_carries_its_gates() {
        let module = attribute_modifier_aura_update_from_fields(
            &fields(&[
                ("StartsActive", "Yes"),
                ("BonusName", "WallBonus"),
                ("RefreshDelay", "2000"),
                ("AffectContainedOnly", "Yes"),
                ("MaxActiveRank", "1"),
                ("AntiCategory", "BUFF"),
                ("RequiredConditions", "MOUNTED"),
                ("AllowPowerWhenAttacking", "No"),
            ]),
            Frame::ZERO,
        )
        .unwrap();
        let got = pulses(module, 1, None);
        let Effect::Aura {
            radius,
            anti,
            max_active_rank,
            contained_only,
            required_mounted,
            allow_while_attacking,
            ..
        } = &got[0].1
        else {
            panic!("{:?}", got[0].1);
        };
        assert_eq!(
            (
                *radius,
                anti.len(),
                *max_active_rank,
                *contained_only,
                *required_mounted,
                *allow_while_attacking
            ),
            (None, 1, Some(1.0), true, true, false)
        );
    }
}
