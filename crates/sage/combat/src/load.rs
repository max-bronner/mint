use mint_sage_ini::value::{
    field_expression, parse_bool, parse_float_or_macro, resolve_percent, tagged_value,
};
use mint_sage_ini::{Block, Document, Field, MacroTable, Node};
use mint_sim::{LogicFrames, UpgradeBit, UpgradeMask};

use crate::armor::{ArmorStore, ArmorTemplate};
use crate::attributes::{ModifierStore, MODIFIER_LIST_KEYWORD};
use crate::collide::CollideMask;
use crate::damage::{DamageTypeId, DamageTypes, DeathTypeId, DeathTypes};
use crate::nugget::{
    carries_upgrade_gate, DamageFxTypes, DamageNugget, DamageShape, DotNugget, EffectNugget,
    EffectNuggets, EmotionWeaponNugget, FireLogicNugget, GrabNugget, HordeAttackNugget,
    MetaImpactNugget, ModifierNugget, Nugget, OpenGateNugget, ParalyzeNugget, ProjectileNugget,
    SpawnAndFadeNugget, SpecialModelConditionNugget, StealMoneyNugget, WeaponOclNugget,
};
use crate::upgrade_filter::{UpgradeFilter, UpgradeFilters};
use crate::victim::{RadiusAffects, VictimRuleTable, VictimRules};
use crate::weapon::{
    AntiMask, FrameRange, FxTrigger, PreAttackType, RangeBonus, WeaponId, WeaponStore,
    WeaponTemplate,
};
use crate::weapon_bonus::WeaponBonusSet;
use crate::weapon_slots::WeaponSlot;

const DAMAGE_SPEED_PER_FRAME: f32 = 0.2;

pub const WEAPON_KEYWORD: &str = "Weapon";
pub const ARMOR_KEYWORD: &str = "Armor";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadReport {
    pub weapons: usize,
    pub armors: usize,
    pub armor_rows: usize,
    pub nuggets: usize,
    pub modelled_nuggets: usize,
    pub unresolved_fields: usize,
    pub weapons_without_effect: usize,
    pub unnamed_blocks: usize,
    pub unreadable_scalars: usize,
}

impl LoadReport {
    pub fn merge(&mut self, other: &Self) {
        self.weapons = self.weapons.saturating_add(other.weapons);
        self.armors = self.armors.saturating_add(other.armors);
        self.armor_rows = self.armor_rows.saturating_add(other.armor_rows);
        self.nuggets = self.nuggets.saturating_add(other.nuggets);
        self.modelled_nuggets = self.modelled_nuggets.saturating_add(other.modelled_nuggets);
        self.unresolved_fields = self
            .unresolved_fields
            .saturating_add(other.unresolved_fields);
        self.weapons_without_effect = self
            .weapons_without_effect
            .saturating_add(other.weapons_without_effect);
        self.unnamed_blocks = self.unnamed_blocks.saturating_add(other.unnamed_blocks);
        self.unreadable_scalars = self
            .unreadable_scalars
            .saturating_add(other.unreadable_scalars);
    }
}

#[derive(Debug, Clone, Default)]
pub struct CombatData {
    pub damage_types: DamageTypes,
    pub death_types: DeathTypes,
    pub weapons: WeaponStore,
    pub weapon_bonus: WeaponBonusSet,
    pub armors: ArmorStore,
    pub filters: UpgradeFilters,
    pub victims: VictimRuleTable,
    pub modifiers: ModifierStore,
    pub effects: EffectNuggets,
    pub damage_fx_types: DamageFxTypes,
    pub damage_fx_type_names: crate::damage::DamageFxTypeNames,
    pub damage_fx: mint_sage_ini::DamageFxIndex,
    pub report: LoadReport,
    pub authored_logic_hz: Option<core::num::NonZeroU32>,
    pub value_errors: mint_sage_ini::ValueErrors,
}

impl CombatData {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn load_document(&mut self, document: &Document, macros: &MacroTable) {
        for block in document.roots() {
            if block.keyword.eq_ignore_ascii_case(ARMOR_KEYWORD) {
                self.load_armor(block, macros);
            }
        }
        for block in document.roots() {
            if block.keyword.eq_ignore_ascii_case(WEAPON_KEYWORD) {
                self.load_weapon(block, macros);
            } else if block.keyword.eq_ignore_ascii_case(MODIFIER_LIST_KEYWORD) {
                self.modifiers.load(block, macros, &mut self.damage_types);
            }
        }
    }

    pub fn load_armor(&mut self, block: &Block, macros: &MacroTable) {
        let Some(name) = block.name() else {
            self.report.unnamed_blocks = self.report.unnamed_blocks.saturating_add(1);
            return;
        };
        let mut rows: Vec<(String, f32)> = Vec::new();
        for field in block.fields() {
            if !field.keyword.eq_ignore_ascii_case(ARMOR_KEYWORD) {
                continue;
            }
            let Some(damage_type) = field.args.first() else {
                continue;
            };
            let Some(rest) = field.args.get(1..) else {
                continue;
            };
            match resolve_percent(rest, macros) {
                Some(coefficient) => rows.push((damage_type.clone(), coefficient)),
                None => {
                    self.report.unresolved_fields = self.report.unresolved_fields.saturating_add(1);
                }
            }
        }
        self.report.armor_rows = self.report.armor_rows.saturating_add(rows.len());
        let template = ArmorTemplate::from_rows(&rows, &mut self.damage_types).with_scalars(
            self.percent(ARMOR_KEYWORD, block, "FlankedPenalty", macros),
            block
                .field("DamageScalar")
                .and_then(|field| resolve_percent(&field.args, macros)),
        );
        self.armors.insert(name, template);
        self.report.armors = self.report.armors.saturating_add(1);
    }

    pub fn load_weapon(&mut self, block: &Block, macros: &MacroTable) {
        let Some(name) = block.name() else {
            self.report.unnamed_blocks = self.report.unnamed_blocks.saturating_add(1);
            return;
        };
        let attack_range = self.number(block, "AttackRange", macros).unwrap_or(0.0);
        let minimum_attack_range = self
            .number(block, "MinimumAttackRange", macros)
            .unwrap_or(0.0);
        let aim_delta = self
            .number(block, "AcceptableAimDelta", macros)
            .map_or(0.0, |degrees| degrees * (core::f32::consts::PI / 180.0));
        let delay_between_shots = self.duration_range(block, "DelayBetweenShots", macros);
        let pre_attack_delay = self.milliseconds(block, "PreAttackDelay", macros);
        let firing_duration = self.milliseconds(block, "FiringDuration", macros);
        let idle_after_firing = self
            .number(block, "IdleAfterFiringDelay", macros)
            .filter(|ms| *ms >= 0.0)
            .map(|ms| milliseconds_to_frames(ms, self.authored_logic_hz));
        let hold_after_firing = self.milliseconds(block, "HoldAfterFiringDelay", macros);
        let clip_reload_time = self.duration_range(block, "ClipReloadTime", macros);
        let auto_reload_when_idle = self.milliseconds(block, "AutoReloadWhenIdle", macros);
        let clip_size = self
            .number(block, "ClipSize", macros)
            .map_or(0, non_negative_count);
        let weapon_speed = self.number(block, "WeaponSpeed", macros).unwrap_or(0.0);
        let min_weapon_speed = self.number(block, "MinWeaponSpeed", macros).unwrap_or(0.0);
        let max_weapon_speed = self.number(block, "MaxWeaponSpeed", macros).unwrap_or(0.0);

        let mut nuggets = Vec::new();
        let mut filters: Vec<(usize, UpgradeFilter)> = Vec::new();
        let mut victims: Vec<(usize, VictimRules)> = Vec::new();
        let mut effects: Vec<(usize, EffectNugget)> = Vec::new();
        let mut damage_fx: Vec<(usize, crate::damage::DamageFxTypeId)> = Vec::new();
        let weapon_fx_type = block
            .field("DamageFXType")
            .and_then(Field::value)
            .and_then(|token| self.damage_fx_type_names.intern(token));
        let weapon_death_type = block
            .field("DeathType")
            .and_then(Field::value)
            .and_then(|token| self.death_types.intern(token));
        let weapon_sub_type = block
            .field("DamageSubType")
            .and_then(Field::value)
            .and_then(|token| self.damage_types.intern(token));
        let cleared = block
            .children
            .iter()
            .rposition(|node| matches!(node, Node::Field(field) if field.keyword == "ClearNuggets"))
            .map_or(0, |at| at.saturating_add(1));
        let kept = block
            .children
            .iter()
            .skip(cleared)
            .filter_map(|node| match node {
                Node::Block(sub) => Some(sub),
                _ => None,
            });
        for sub in kept {
            self.report.nuggets = self.report.nuggets.saturating_add(1);
            let nugget = self.read_nugget(sub, macros, weapon_death_type, weapon_sub_type);
            if !matches!(nugget, Nugget::Unmodelled(_)) {
                self.report.modelled_nuggets = self.report.modelled_nuggets.saturating_add(1);
            }
            let gated = carries_upgrade_gate(&sub.keyword);
            if let Some(filter) = UpgradeFilter::read(sub).filter(|_| gated) {
                filters.push((nuggets.len(), filter));
            }
            if let Some(mut rules) = VictimRules::read(sub, &mut self.report.unreadable_scalars) {
                if !gated {
                    rules.special.clear();
                }
                if !rules.special.is_empty()
                    || !rules.scalars.is_empty()
                    || !rules.force_kill.is_empty()
                {
                    victims.push((nuggets.len(), rules));
                }
            }
            if let Some(effect) = self.read_effect_nugget(sub, macros) {
                self.report.modelled_nuggets = self.report.modelled_nuggets.saturating_add(1);
                effects.push((nuggets.len(), effect));
            }
            if let Some(fx_type) = sub
                .field("DamageFXType")
                .and_then(Field::value)
                .and_then(|token| self.damage_fx_type_names.intern(token))
            {
                damage_fx.push((nuggets.len(), fx_type));
            }
            nuggets.push(nugget);
        }
        let flag = |keyword: &str| {
            block
                .field(keyword)
                .and_then(Field::value)
                .and_then(parse_bool)
                .unwrap_or(false)
        };
        let anti_bit = |mask: AntiMask, keyword: &str, bit: AntiMask| -> AntiMask {
            match block
                .field(keyword)
                .and_then(Field::value)
                .and_then(parse_bool)
            {
                Some(true) => mask.union(bit),
                Some(false) => mask.without(bit),
                None => mask,
            }
        };
        let anti_mask = anti_bit(AntiMask::DEFAULT, "AntiGround", AntiMask::GROUND);
        let anti_mask = anti_bit(anti_mask, "AntiAirborneVehicle", AntiMask::AIRBORNE_VEHICLE);
        let anti_mask = anti_bit(
            anti_mask,
            "AntiAirborneInfantry",
            AntiMask::AIRBORNE_INFANTRY,
        );
        let anti_mask = anti_bit(anti_mask, "AntiAirborneMonster", AntiMask::AIRBORNE_MONSTER);
        let anti_mask = anti_bit(anti_mask, "AntiStructure", AntiMask::STRUCTURE);

        let template = WeaponTemplate {
            attack_range,
            minimum_attack_range,
            delay_between_shots,
            pre_attack_delay,
            firing_duration,
            idle_after_firing,
            hold_after_firing,
            pre_attack_type: block
                .field("PreAttackType")
                .and_then(Field::value)
                .map_or(PreAttackType::PerShot, PreAttackType::parse),
            clip_size,
            clip_reload_time,
            clip_reload_step: retail_frames(3, self.authored_logic_hz),
            auto_reload_when_idle,
            hold_during_reload: flag("HoldDuringReload"),
            auto_reloads_clip: block
                .field("AutoReloadsClip")
                .and_then(Field::value)
                .and_then(parse_bool)
                .unwrap_or(true),
            melee: block
                .field("MeleeWeapon")
                .and_then(Field::value)
                .and_then(parse_bool)
                .unwrap_or(false),
            aiming: flag("IsAimingWeapon"),
            weapon_speed,
            min_weapon_speed,
            max_weapon_speed,
            scale_weapon_speed: flag("ScaleWeaponSpeed"),
            leech_range: flag("LeechRangeWeapon"),
            can_swoop: flag("CanSwoop"),
            nuggets,
            radius_affects: block
                .field("RadiusDamageAffects")
                .map_or_else(RadiusAffects::default, |field| {
                    RadiusAffects::parse(&field.args)
                }),
            projectile_collides: self.collide_mask(block),
            damage_at_self: flag("DamageDealtAtSelfPosition"),
            fire_fx: fx_name(block, "FireFX"),
            fire_flank_fx: fx_name(block, "FireFlankFX"),
            pre_attack_fx: fx_name(block, "PreAttackFX"),
            anti_mask,
            aim_delta,
            hit_chance: self.fraction(block, "HitPercentage", macros, 1.0),
            scatter_radius: self
                .non_negative(block, "ScatterRadius", macros)
                .unwrap_or(0.0),
            scatter_vs_infantry: self
                .non_negative(block, "ScatterRadiusVsInfantry", macros)
                .unwrap_or(0.0),
            range_bonus: RangeBonus {
                min_height: self
                    .non_negative(block, "RangeBonusMinHeight", macros)
                    .unwrap_or(0.0),
                bonus: self
                    .non_negative(block, "RangeBonus", macros)
                    .unwrap_or(0.0),
                per_foot: self
                    .non_negative(block, "RangeBonusPerFoot", macros)
                    .unwrap_or(0.0),
                restricted_height: self
                    .non_negative(block, "RestrictedHeightRange", macros)
                    .unwrap_or(0.0),
            },
            pre_attack_random: self
                .non_negative(block, "PreAttackRandomAmount", macros)
                .map_or(LogicFrames::ZERO, |ms| {
                    milliseconds_to_frames(ms, self.authored_logic_hz)
                }),
            hit_stored_target: self.yes_no(block, "HitStoredTarget"),
            can_be_dodged: self.yes_no(block, "CanBeDodged"),
            hit_passenger_chance: self.fraction(block, "HitPassengerPercentage", macros, 0.0),
            use_innate_attributes: self.yes_no(block, "UseInnateAttributes"),
            passenger_proportional_attack: self.yes_no(block, "PassengerProportionalAttack"),
            max_attack_passengers: self.number(block, "MaxAttackPassengers", macros).map_or(
                0,
                |value| {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let count = value.clamp(0.0, 255.0) as u8;
                    count
                },
            ),
            no_victim_needed: self.yes_no(block, "NoVictimNeeded"),
            can_fire_while_moving: self.yes_no(block, "CanFireWhileMoving"),
            can_fire_while_charging: self.yes_no(block, "CanFireWhileCharging"),
            aim_direction: self
                .number(block, "AimDirection", macros)
                .map_or(0.0, |degrees| degrees * (core::f32::consts::PI / 180.0)),
            continue_attack_range: self
                .non_negative(block, "ContinueAttackRange", macros)
                .unwrap_or(0.0),
            projectile_filter_in_container: self
                .object_filter(block, "ProjectileFilterInContainer"),
            projectile_self: flag("ProjectileSelf"),
            fx_trigger: self.fx_trigger(block),
            instant_load_clip_on_activate: flag("InstantLoadClipOnActivate"),
            finish_attack_once_started: flag("FinishAttackOnceStarted"),
            chase_weapon: flag("ChaseWeapon"),
            lock_when_using: flag("LockWhenUsing"),
            bombard_type: flag("BombardType"),
            disable_scatter_for_targets_on_wall: flag("DisableScatterForTargetsOnWall"),
            bonus: self.weapon_bonus(block, macros),
        };
        if !template.has_effect() {
            self.report.weapons_without_effect =
                self.report.weapons_without_effect.saturating_add(1);
        }
        if let Some(id) = self.weapons.insert(name, template) {
            self.filters.clear_weapon(id.index());
            for (nugget, filter) in filters {
                self.filters.insert(id.index(), nugget, filter);
            }
            self.victims.clear_weapon(id.index());
            for (nugget, rules) in victims {
                self.victims.insert(id.index(), nugget, rules);
            }
            self.effects.clear_weapon(id.index());
            for (nugget, effect) in effects {
                self.effects.insert(id.index(), nugget, effect);
            }
            self.damage_fx_types.clear_weapon(id.index());
            if let Some(fx_type) = weapon_fx_type {
                self.damage_fx_types.insert_weapon(id.index(), fx_type);
            }
            for (nugget, fx_type) in damage_fx {
                self.damage_fx_types.insert(id.index(), nugget, fx_type);
            }
        }
        self.report.weapons = self.report.weapons.saturating_add(1);
    }

    pub fn resolve_upgrade_filters(&mut self, bit: impl Fn(&str) -> Option<UpgradeBit>) {
        self.filters.resolve(&bit);
    }

    #[must_use]
    pub fn nugget_passes(&self, weapon: WeaponId, nugget: usize, held: &UpgradeMask) -> bool {
        match self.filters.get(weapon.index(), nugget) {
            Some(filter) => filter.passes(held),
            None => !self
                .weapons
                .get(weapon)
                .and_then(|template| template.nuggets.get(nugget))
                .is_some_and(Nugget::is_gated),
        }
    }

    fn read_nugget(
        &mut self,
        block: &Block,
        macros: &MacroTable,
        weapon_death_type: Option<DeathTypeId>,
        weapon_sub_type: Option<DamageTypeId>,
    ) -> Nugget {
        let death_type = |loader: &mut Self, block: &Block| {
            block
                .field("DeathType")
                .and_then(Field::value)
                .and_then(|token| loader.death_types.intern(token))
                .or(weapon_death_type)
        };
        let sub_type = |loader: &mut Self, block: &Block| {
            block
                .field("DamageSubType")
                .and_then(Field::value)
                .and_then(|token| loader.damage_types.intern(token))
                .or(weapon_sub_type)
        };
        if block.keyword.eq_ignore_ascii_case("DamageNugget") {
            let Some(damage_type) = block
                .field("DamageType")
                .and_then(Field::value)
                .and_then(|token| self.damage_types.intern(token))
            else {
                return Nugget::Unmodelled(block.keyword.clone());
            };
            return Nugget::Damage(DamageNugget {
                damage: self.number(block, "Damage", macros).unwrap_or(0.0),
                damage_type,
                death_type: death_type(self, block),
                sub_type: sub_type(self, block),
                radius: self.number(block, "Radius", macros).unwrap_or(0.0),
                damage_scalar: 1.0,
                delay: self.milliseconds(block, "DelayTime", macros),
                arc: self.number(block, "DamageArc", macros),
                arc_inverted: self.boolean(block, "DamageArcInverted"),
                flanking_bonus: self
                    .percent(WEAPON_KEYWORD, block, "FlankingBonus", macros)
                    .unwrap_or(0.0),
                requires_upgrade: requires_upgrade(block),
                accepts_add: self.boolean_or(block, "AcceptDamageAdd", true),
                shape: DamageShape {
                    cylinder_aoe: self.boolean(block, "CylinderAOE"),
                    min_radius: self.number(block, "MinRadius", macros).unwrap_or(0.0),
                    taper_off: self.number(block, "DamageTaperOff", macros),
                    max_height: self.number(block, "DamageMaxHeight", macros),
                    speed: self.number(block, "DamageSpeed", macros).unwrap_or(0.0)
                        * DAMAGE_SPEED_PER_FRAME,
                    max_height_above_terrain: self.number(
                        block,
                        "DamageMaxHeightAboveTerrain",
                        macros,
                    ),
                    drain_life: self.boolean(block, "DrainLife"),
                    drain_multiplier: self
                        .number(block, "DrainLifeMultiplier", macros)
                        .unwrap_or(0.0),
                    flanked_scalar: self.number(block, "FlankedScalar", macros),
                },
            });
        }
        if block.keyword.eq_ignore_ascii_case("DOTNugget") {
            let Some(damage_type) = block
                .field("DamageType")
                .and_then(Field::value)
                .and_then(|token| self.damage_types.intern(token))
            else {
                return Nugget::Unmodelled(block.keyword.clone());
            };
            return Nugget::Dot(DotNugget {
                damage: self.number(block, "Damage", macros).unwrap_or(0.0),
                damage_type,
                radius: self.number(block, "Radius", macros).unwrap_or(0.0),
                interval: self.milliseconds(block, "DamageInterval", macros),
                duration: self.milliseconds(block, "DamageDuration", macros),
                death_type: death_type(self, block),
                sub_type: sub_type(self, block),
                requires_upgrade: requires_upgrade(block),
                accepts_add: self.boolean_or(block, "AcceptDamageAdd", true),
            });
        }
        if block
            .keyword
            .eq_ignore_ascii_case("AttributeModifierNugget")
        {
            let Some(list) = block.field("AttributeModifier").and_then(Field::value) else {
                return Nugget::Unmodelled(block.keyword.clone());
            };
            return Nugget::Modifier(ModifierNugget {
                list: list.to_owned(),
                radius: self.number(block, "Radius", macros).unwrap_or(0.0),
            });
        }
        if block.keyword.eq_ignore_ascii_case("ProjectileNugget") {
            let Some(warhead) = block.field("WarheadTemplateName").and_then(Field::value) else {
                return Nugget::Unmodelled(block.keyword.clone());
            };
            return Nugget::Projectile(ProjectileNugget {
                projectile_template: block
                    .field("ProjectileTemplateName")
                    .and_then(Field::value)
                    .map(str::to_owned),
                warhead: warhead.to_owned(),
                requires_upgrade: requires_upgrade(block),
                launch_slot: block
                    .field("WeaponLaunchBoneSlotOverride")
                    .and_then(Field::value)
                    .and_then(WeaponSlot::parse),
                always_attack_offset: self.boolean(block, "UseAlwaysAttackOffset").then(|| {
                    let args = block
                        .field("AlwaysAttackHereOffset")
                        .map(|field| field.args.clone())
                        .unwrap_or_default();
                    let axis = |tag: &str| {
                        tagged_value(&args, tag)
                            .and_then(|v| parse_float_or_macro(v, macros))
                            .unwrap_or(0.0)
                    };
                    [axis("X"), axis("Y"), axis("Z")]
                }),
            });
        }
        if block.keyword.eq_ignore_ascii_case("MetaImpactNugget") {
            let defaults = MetaImpactNugget::default();
            return Nugget::MetaImpact(MetaImpactNugget {
                amount: self.number(block, "ShockWaveAmount", macros).unwrap_or(0.0),
                radius: self.number(block, "ShockWaveRadius", macros).unwrap_or(0.0),
                taper_off: self
                    .number(block, "ShockWaveTaperOff", macros)
                    .unwrap_or(defaults.taper_off),
                z_mult: self
                    .number(block, "ShockWaveZMult", macros)
                    .unwrap_or(defaults.z_mult),
                arc: self.number(block, "ShockWaveArc", macros),
                speed: self.number(block, "ShockWaveSpeed", macros).unwrap_or(0.0),
                delay: self.milliseconds(block, "DelayTime", macros),
                hero_resist: self.number(block, "HeroResist", macros).unwrap_or(0.0),
                invert: self.boolean(block, "InvertShockWave"),
                flip_direction: self.boolean(block, "FlipDirection"),
                arc_inverted: self.boolean(block, "ShockWaveArcInverted"),
                only_when_just_died: self.boolean(block, "OnlyWhenJustDied"),
                affect_hordes: self.boolean(block, "AffectHordes"),
                cyclonic_factor: self.number(block, "CyclonicFactor", macros).unwrap_or(0.0),
                clear_radius: self.boolean(block, "ShockWaveClearRadius"),
                clear_mult: self
                    .number(block, "ShockWaveClearMult", macros)
                    .unwrap_or(0.0),
                clear_fling_height: self
                    .number(block, "ShockWaveClearFlingHeight", macros)
                    .unwrap_or(0.0),
                kill_filter: self.object_filter(block, "KillObjectFilter"),
                special_filter: self.object_filter(block, "SpecialObjectFilter"),
            });
        }
        Nugget::Unmodelled(block.keyword.clone())
    }

    fn read_effect_nugget(&mut self, block: &Block, macros: &MacroTable) -> Option<EffectNugget> {
        if block.keyword.eq_ignore_ascii_case("WeaponOCLNugget") {
            let list = block.field("WeaponOCLName").and_then(Field::value)?;
            return Some(EffectNugget::WeaponOcl(WeaponOclNugget {
                list: list.to_owned(),
            }));
        }
        if block.keyword.eq_ignore_ascii_case("FireLogicNugget") {
            let logic_type = block.field("LogicType").and_then(Field::value)?;
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let as_u16 = |value: f32| value.clamp(0.0, f32::from(u16::MAX)) as u16;
            return Some(EffectNugget::FireLogic(FireLogicNugget {
                logic_type: logic_type.to_owned(),
                radius: self.number(block, "Radius", macros).unwrap_or(0.0),
                damage: self.number(block, "Damage", macros).unwrap_or(0.0),
                damage_arc: self.number(block, "DamageArc", macros),
                min_max_burn_rate: self.number(block, "MinMaxBurnRate", macros).map(as_u16),
                min_decay: self.number(block, "MinDecay", macros).map(as_u16),
                max_resistance: self
                    .number(block, "MaxResistance", macros)
                    .map_or(10_000, as_u16),
            }));
        }
        if block.keyword.eq_ignore_ascii_case("HordeAttackNugget") {
            return Some(EffectNugget::HordeAttack(HordeAttackNugget {
                lock_weapon_slot: block
                    .field("LockWeaponSlot")
                    .and_then(Field::value)
                    .map(str::to_owned),
            }));
        }
        if block.keyword.eq_ignore_ascii_case("SlaveAttackNugget") {
            return Some(EffectNugget::SlaveAttack);
        }
        if block.keyword.eq_ignore_ascii_case("ParalyzeNugget") {
            return Some(EffectNugget::Paralyze(ParalyzeNugget {
                radius: self.number(block, "Radius", macros).unwrap_or(0.0),
                duration: self.milliseconds(block, "Duration", macros),
                special_object_filter: block
                    .field("SpecialObjectFilter")
                    .map(|field| field.args.clone())
                    .unwrap_or_default(),
                fx: block
                    .field("ParalyzeFX")
                    .and_then(Field::value)
                    .map(str::to_owned),
                damage_arc: self.number(block, "DamageArc", macros),
                freeze_animation: self.boolean(block, "FreezeAnimation"),
                affect_horde_members: self.boolean(block, "AffectHordeMembers"),
            }));
        }
        if block.keyword.eq_ignore_ascii_case("GrabNugget") {
            return Some(EffectNugget::Grab(GrabNugget {
                special_object_filter: block
                    .field("SpecialObjectFilter")
                    .map(|field| field.args.clone())
                    .unwrap_or_default(),
                contain_target_on_effect: block
                    .field("ContainTargetOnEffect")
                    .and_then(Field::value)
                    .and_then(parse_bool)
                    .unwrap_or(false),
                remove_target_from_other_contain: block
                    .field("RemoveTargetFromOtherContain")
                    .and_then(Field::value)
                    .and_then(parse_bool)
                    .unwrap_or(false),
                impact_target_on_effect: block
                    .field("ImpactTargetOnEffect")
                    .and_then(Field::value)
                    .and_then(parse_bool)
                    .unwrap_or(false),
                shock_wave_amount: self.number(block, "ShockWaveAmount", macros).unwrap_or(0.0),
                shock_wave_radius: self.number(block, "ShockWaveRadius", macros).unwrap_or(0.0),
                shock_wave_taper_off: self
                    .number(block, "ShockWaveTaperOff", macros)
                    .unwrap_or(0.0),
                shock_wave_z_mult: self.number(block, "ShockWaveZMult", macros).unwrap_or(0.0),
            }));
        }
        if block
            .keyword
            .eq_ignore_ascii_case("SpecialModelConditionNugget")
        {
            let conditions: Vec<String> = block
                .field("ModelConditionNames")
                .map(|field| field.args.clone())
                .unwrap_or_default();
            if conditions.is_empty() {
                return None;
            }
            return Some(EffectNugget::SpecialModelCondition(
                SpecialModelConditionNugget {
                    conditions,
                    duration: self.milliseconds(block, "ModelConditionDuration", macros),
                },
            ));
        }
        if block.keyword.eq_ignore_ascii_case("EmotionWeaponNugget") {
            let emotion = block.field("EmotionType").and_then(Field::value)?;
            let seconds = self.number(block, "Duration", macros).unwrap_or(0.0);
            let milliseconds = seconds.trunc().clamp(0.0, f32::from(u16::MAX)) * 1000.0;
            return Some(EffectNugget::EmotionWeapon(EmotionWeaponNugget {
                emotion: emotion.to_owned(),
                radius: self.number(block, "Radius", macros).unwrap_or(0.0),
                duration: milliseconds_to_frames(milliseconds, self.authored_logic_hz),
            }));
        }
        if block.keyword.eq_ignore_ascii_case("OpenGateNugget") {
            return Some(EffectNugget::OpenGate(OpenGateNugget {
                radius: self.number(block, "Radius", macros).unwrap_or(0.0),
            }));
        }
        if block.keyword.eq_ignore_ascii_case("StealMoneyNugget") {
            let amount = self.number(block, "AmountStolenPerAttack", macros)?;
            #[allow(clippy::cast_possible_truncation)]
            let amount = amount.clamp(0.0, f32::from(i16::MAX)) as i32;
            return Some(EffectNugget::StealMoney(StealMoneyNugget { amount }));
        }
        if block.keyword.eq_ignore_ascii_case("SpawnAndFadeNugget") {
            let spawned_object = block.field("SpawnedObjectName").and_then(Field::value)?;
            let offset_args = block
                .field("SpawnOffset")
                .map(|f| f.args.clone())
                .unwrap_or_default();
            let axis = |tag: &str| {
                tagged_value(&offset_args, tag).and_then(|v| parse_float_or_macro(v, macros))
            };
            return Some(EffectNugget::SpawnAndFade(SpawnAndFadeNugget {
                object_target_filter: block
                    .field("ObjectTargetFilter")
                    .map(|field| field.args.clone())
                    .unwrap_or_default(),
                spawned_object: spawned_object.to_owned(),
                offset: [
                    axis("X").unwrap_or(0.0),
                    axis("Y").unwrap_or(0.0),
                    axis("Z").unwrap_or(0.0),
                ],
            }));
        }
        None
    }

    fn number(&mut self, block: &Block, keyword: &str, macros: &MacroTable) -> Option<f32> {
        let field = block.field(keyword)?;
        field.value()?;
        match field_expression(field, macros) {
            Some(value) if value.is_finite() => Some(value),
            _ => {
                self.report.unresolved_fields = self.report.unresolved_fields.saturating_add(1);
                None
            }
        }
    }

    fn boolean(&mut self, block: &Block, keyword: &str) -> bool {
        self.boolean_or(block, keyword, false)
    }

    fn boolean_or(&mut self, block: &Block, keyword: &str, absent: bool) -> bool {
        let Some(raw) = block.field(keyword).and_then(Field::value) else {
            return absent;
        };
        match parse_bool(raw) {
            Some(value) => value,
            None => {
                self.report.unresolved_fields = self.report.unresolved_fields.saturating_add(1);
                absent
            }
        }
    }

    fn fraction(&self, block: &Block, keyword: &str, macros: &MacroTable, absent: f32) -> f32 {
        self.percent(WEAPON_KEYWORD, block, keyword, macros)
            .map_or(absent, |value| value.min(1.0))
    }

    fn percent(
        &self,
        context: &str,
        block: &Block,
        keyword: &str,
        macros: &MacroTable,
    ) -> Option<f32> {
        let field = block.field(keyword)?;
        let value = resolve_percent(&field.args, macros).filter(|v| v.is_finite() && *v >= 0.0);
        if value.is_none() {
            self.value_errors
                .field(context, field, "a non-negative percentage");
        }
        value
    }

    fn non_negative(&self, block: &Block, keyword: &str, macros: &MacroTable) -> Option<f32> {
        let field = block.field(keyword)?;
        let value = field_expression(field, macros).filter(|v| v.is_finite() && *v >= 0.0);
        if value.is_none() {
            self.value_errors
                .field(WEAPON_KEYWORD, field, "a non-negative number");
        }
        value
    }

    fn collide_mask(&self, block: &Block) -> CollideMask {
        let Some(field) = block.field("ProjectileCollidesWith") else {
            return CollideMask::default();
        };
        CollideMask::parse(&field.args).unwrap_or_else(|token| {
            self.value_errors.value(
                WEAPON_KEYWORD,
                field,
                token,
                "a ProjectileCollidesWith class",
            );
            CollideMask::default()
        })
    }

    fn object_filter(&self, block: &Block, keyword: &str) -> Vec<String> {
        let Some(field) = block.field(keyword) else {
            return Vec::new();
        };
        let bare_ok = |index: usize, token: &str| {
            let upper = token.to_ascii_uppercase();
            matches!(
                upper.as_str(),
                "ALLIES" | "ENEMIES" | "NEUTRAL" | "SAME_PLAYER"
            ) || (index == 0 && matches!(upper.as_str(), "ANY" | "ALL" | "NONE"))
        };
        let wrong = field.args.iter().enumerate().find(|(index, token)| {
            let named = token.strip_prefix('+').or_else(|| token.strip_prefix('-'));
            match named {
                Some(name) => name.is_empty(),
                None => !bare_ok(*index, token),
            }
        });
        if field.args.is_empty() {
            self.value_errors
                .field(WEAPON_KEYWORD, field, "an object filter");
            return Vec::new();
        }
        if let Some((_, token)) = wrong {
            self.value_errors
                .value(WEAPON_KEYWORD, field, token, "an object filter token");
            return Vec::new();
        }
        field.args.clone()
    }

    #[must_use]
    pub fn weapon_bonus(&self, block: &Block, macros: &MacroTable) -> WeaponBonusSet {
        let mut set = WeaponBonusSet::default();
        for field in block.fields_named("WeaponBonus") {
            if let Err(expected) = set.read_line(field, macros) {
                self.value_errors.field(&block.keyword, field, expected);
            }
        }
        set
    }

    fn fx_trigger(&self, block: &Block) -> FxTrigger {
        let Some(field) = block.field("FXTrigger") else {
            return FxTrigger::None;
        };
        field.value().and_then(FxTrigger::parse).unwrap_or_else(|| {
            self.value_errors.field(
                WEAPON_KEYWORD,
                field,
                "NONE, CATAPULT_ROCK or TREBUCHET_ROCK",
            );
            FxTrigger::None
        })
    }

    fn yes_no(&self, block: &Block, keyword: &str) -> bool {
        let Some(field) = block.field(keyword) else {
            return false;
        };
        field.value().and_then(parse_bool).unwrap_or_else(|| {
            self.value_errors.field(WEAPON_KEYWORD, field, "Yes or No");
            false
        })
    }

    fn milliseconds(&mut self, block: &Block, keyword: &str, macros: &MacroTable) -> LogicFrames {
        let authored = self.authored_logic_hz;
        self.number(block, keyword, macros)
            .map_or(LogicFrames::ZERO, |ms| milliseconds_to_frames(ms, authored))
    }

    fn duration_range(&mut self, block: &Block, keyword: &str, macros: &MacroTable) -> FrameRange {
        let Some(field) = block.field(keyword) else {
            return FrameRange::ZERO;
        };
        let Some(min) = tagged_value(&field.args, "Min") else {
            return FrameRange::fixed(self.milliseconds(block, keyword, macros));
        };
        let Some(min) = parse_float_or_macro(min, macros).filter(|v| v.is_finite()) else {
            self.report.unresolved_fields = self.report.unresolved_fields.saturating_add(1);
            return FrameRange::ZERO;
        };
        let max = tagged_value(&field.args, "Max")
            .and_then(|raw| parse_float_or_macro(raw, macros))
            .filter(|v| v.is_finite())
            .unwrap_or(min);
        let authored = self.authored_logic_hz;
        FrameRange::new(
            milliseconds_to_frames(min, authored),
            milliseconds_to_frames(max, authored),
        )
    }
}

fn milliseconds_to_frames(
    milliseconds: f32,
    authored: Option<core::num::NonZeroU32>,
) -> LogicFrames {
    let hz = authored
        .or(core::num::NonZeroU32::new(mint_sim::SIMULATION_HZ))
        .unwrap_or(core::num::NonZeroU32::MIN);
    LogicFrames::from_authored_milliseconds_f32(milliseconds, hz)
}

fn retail_frames(count: u32, authored: Option<core::num::NonZeroU32>) -> LogicFrames {
    let per = authored.map_or(1, |hz| {
        mint_sim::SIMULATION_HZ
            .checked_div(hz.get())
            .unwrap_or(1)
            .max(1)
    });
    LogicFrames::from_frames(count.saturating_mul(per))
}

fn requires_upgrade(block: &Block) -> bool {
    block
        .field("RequiredUpgradeNames")
        .is_some_and(|field| field.args.iter().any(|arg| !arg.trim().is_empty()))
}

fn fx_name(block: &Block, keyword: &str) -> Option<String> {
    block
        .field(keyword)
        .and_then(Field::value)
        .filter(|name| !name.eq_ignore_ascii_case("NONE"))
        .map(str::to_owned)
}

fn non_negative_count(value: f32) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 0;
    }
    if value > f32::from(u16::MAX) {
        return u32::from(u16::MAX);
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = value as u32;
    count
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
