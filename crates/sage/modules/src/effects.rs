use std::sync::Arc;

use mint_sim::module::UpgradeMask;
use mint_sim::{Component, Entity, LogicFrames, StateHasher};

use crate::object_filter::ObjectFilter;

pub type Name = Arc<str>;

pub type Names = Arc<[Name]>;

#[must_use]
pub fn names(list: Vec<String>) -> Names {
    list.into_iter().map(Name::from).collect::<Vec<_>>().into()
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    WeaponSetFlag(Name),
    ClearWeaponSetFlag(Name),
    LockWeaponSlot(u8),
    ReleaseWeaponLock,
    ArmorSetFlag(Name),
    ModelConditions {
        add: Names,
        remove: Names,
    },
    SubObjects {
        show: Names,
        hide: Names,
    },
    Texture {
        swaps: Arc<[(Name, Name)]>,
        exclude: Names,
    },
    CommandSet(Name),
    UnpausePower(Name),
    Geometry {
        show: Names,
        hide: Names,
    },
    StatusBits {
        set: Names,
        clear: Names,
    },
    LevelUp {
        levels: u32,
        cap: Option<u32>,
    },
    AttributeModifier(Name),
    RemoveUpgrades(UpgradeMask, bool),
    CreateObjects {
        list: Name,
        location: CreateLocation,
        at: PowerPlace,
        from: OclFrom,
    },
    AreaHeal {
        at: PowerPlace,
        radius: f32,
        amount: f32,
        as_percent: bool,
        affects: Names,
        fx: Option<Name>,
    },
    GrantUpgrade(Name),
    AreaModifier {
        at: PowerPlace,
        rule: ModifierRule,
    },
    AreaLevelUp {
        at: PowerPlace,
        radius: f32,
        experience: u32,
        affects: Names,
        fx: Option<Name>,
    },
    TriggerModule {
        tag: Name,
        target: Option<Entity>,
        at: Option<(f32, f32)>,
    },
    GrantPlayerUpgrade(Name),
    FireWeapon {
        weapon: Name,
        at: PowerPlace,
        offset: (f32, f32),
        repeat: Option<u8>,
        alive_only: bool,
        reload_slot: Option<u32>,
    },
    LoseStealth,
    PlayWeaponPreFireFX {
        weapon: Name,
    },
    PlayFx {
        list: Name,
        at: PowerPlace,
        on_object: bool,
        secondary: Option<Entity>,
    },
    Teleport {
        to: PowerPlace,
        face: bool,
    },
    StoreObjects {
        power: Name,
        radius: f32,
        at: PowerPlace,
        slot: u32,
    },
    TeleportGroup {
        power: Name,
        radius: f32,
        min_ring: f32,
        max_ring: f32,
        trigger_fx: Option<Name>,
        target_fx: Option<Name>,
        at: PowerPlace,
    },
    ReplaceInArea {
        entries: Arc<[ReplaceAreaEntry]>,
        radius: f32,
        fx: Option<Name>,
        scatter: bool,
        at: PowerPlace,
    },
    CreateObject {
        name: Name,
        at: PowerPlace,
    },
    Aura {
        list: Name,
        radius: f32,
        affects: Names,
        sides: PowerSides,
        allow_self: bool,
        lasts: mint_sim::LogicFrames,
        while_dead: bool,
    },
    SetWeatherEffect {
        list: Option<Name>,
        anti: Names,
        affects: Names,
        affect_good: bool,
        affect_evil: bool,
        lasts: mint_sim::LogicFrames,
    },
    ClearWeatherEffect,
    ChangeWeather {
        kind: WeatherKind,
        lasts: mint_sim::LogicFrames,
    },
    FearEveryoneAdmitted {
        affects: Names,
    },
    SpacedObjects {
        name: Name,
        affects: Names,
        spacing: f32,
    },
    CashBounty(f32),
    AddObject {
        name: Name,
        at: PowerPlace,
    },
    FellTreesForMoney {
        at: PowerPlace,
        radius: f32,
        fx: Option<Name>,
        multiplier: f32,
        cap: f32,
    },
    MorphTrees {
        at: PowerPlace,
        radius: f32,
        tainted: bool,
    },
    Capture {
        at: PowerPlace,
        radius: f32,
        affects: Names,
    },
    Dominate {
        at: PowerPlace,
        radius: f32,
        affects: Names,
        permanent: bool,
        fx: Option<Name>,
    },
    CursePowers {
        at: PowerPlace,
        share: f32,
        fx: Option<Name>,
    },
    DefectFor {
        back_to: mint_sim::PlayerId,
        lasts: mint_sim::LogicFrames,
    },
    JoinMaster {
        to: mint_sim::PlayerId,
    },
    ProductionBonus {
        kinds: Names,
        multiplier: f32,
        lasts: mint_sim::LogicFrames,
    },
    AutoHeal {
        amount: f32,
        radius: f32,
        kinds: Names,
        forbidden: Names,
        skip_self: bool,
        quiet: mint_sim::LogicFrames,
        fx: Option<Name>,
        sole_for: mint_sim::LogicFrames,
        as_percent: bool,
        nonstackable: bool,
        skip_recently_damaged: bool,
        fx_always: bool,
    },
    MoveToObject {
        target: Entity,
    },
    MoveToPosition {
        at: (f32, f32),
    },
    MoveToApproachPoint {
        target: Entity,
        max_range: f32,
    },
    Halt,
    HoldOrders(bool),
    FaceObject {
        target: Entity,
    },
    FacePosition {
        at: (f32, f32),
    },
    StartRecharge(Name),
    MarkSpecialPowerTriggered(Name),
    SelfTimedModifier {
        list: Name,
        lasts: Option<mint_sim::LogicFrames>,
    },
    ClearSelfTimedModifier(Name),
    GrantInvisibility {
        filter: Names,
        radius: f32,
        duration: mint_sim::LogicFrames,
        nugget: Arc<crate::invisibility_update::InvisibilityNugget>,
    },
    TimedModelCondition {
        flag: Name,
        lasts: mint_sim::LogicFrames,
        only_if_absent: bool,
    },
    ToggleModelCondition(Name),
    CastEdge {
        power: Name,
        target: Option<Entity>,
        edge: CastEdge,
    },
    Spawn {
        name: Name,
        requester: mint_sim::Requester,
        ordinal: u32,
    },
    AiMoveTo {
        at: (f32, f32),
        acquires: bool,
    },
    AiAttack {
        target: Entity,
    },
    Die,
    EnterRampage,
    TriggerReward {
        experience: u32,
        skill_points: u32,
    },
    PlayObjectSound(Name),
    StartLoopSound(Name),
    StopLoopSound,
    CaptureFlash {
        target: Entity,
    },
    CreateSlave {
        name: Name,
        offset: (f32, f32, f32),
        destroy_when_sold: bool,
    },
    CommandPointBonus(i32),
    SpellRechargeCount {
        player: Option<mint_sim::PlayerId>,
        delta: i32,
        percentages: Arc<[f32]>,
    },
    Replace {
        template: Name,
        carry: ReplaceCarry,
    },
    ReplaceAndAdd {
        template: Name,
        carry: ReplaceCarry,
        attendants: Names,
    },
    PressCommand(Name),
    CastleUpgrade {
        upgrade: Name,
        wall_radius: Option<f32>,
    },
    InheritUpgrade {
        upgrade: Name,
        radius: f32,
        filter: Arc<ObjectFilter>,
    },
    ShareUpgradesWithSlaves,
    ArrowStorm {
        weapon: Name,
        at: PowerPlace,
        radius: f32,
        shots_per_burst: u32,
        shots_per_target: u32,
        max_shots: u32,
        can_shoot_empty_ground: bool,
    },
    ClearArrowStorm,
    Fling {
        power: Name,
        velocity: [f32; 3],
        landing_warhead: Option<Name>,
    },
    RefreshOtherPowers {
        except: Name,
        affect_allies: bool,
        range: f32,
        affects: Names,
    },
    ReplenishHorde {
        melee_free: LogicFrames,
        fx: Option<Name>,
        nearby: Option<f32>,
        all_nearby: bool,
        upgrade_required: UpgradeMask,
    },
    EmotionPulse {
        at: PowerPlace,
        radius: f32,
        kind: mint_sage_ini::EmotionType,
        filter: Arc<ObjectFilter>,
        lasts: LogicFrames,
    },
    ToggleStealth {
        lasts: Option<LogicFrames>,
        palantir_timer: bool,
    },
    ToggleDisguise {
        force_mounted: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReplaceCarry {
    pub health_fraction: bool,
    pub experience: bool,
    pub selection: bool,
}

impl ReplaceCarry {
    pub const MOUNT: Self = Self {
        health_fraction: true,
        experience: true,
        selection: true,
    };
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReplaceAreaEntry {
    pub filter: Arc<ObjectFilter>,
    pub replacements: Names,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CastEdge {
    Prepare,
    Trigger,
    Abort,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PowerPlace {
    At(f32, f32),
    Object(Entity),
    Source,
}

impl PowerPlace {
    #[must_use]
    pub fn of(activation: &mint_sim::Activation<'_>) -> Self {
        match (activation.at, activation.target) {
            (Some((x, y)), _) => Self::At(x, y),
            (None, Some(target)) => Self::Object(target),
            (None, None) => Self::Source,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OclFrom {
    Location,
    Object {
        secondary: Option<Entity>,
    },
    For {
        primary: Entity,
        min_gap: Option<u32>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum CreateLocation {
    AtLocation,
    AboveLocation,
    UseOwnerObject,
    AtEdgeNearSource,
    AtEdgeNearTarget,
    AtEdgeFarthestFromTarget,
    UseSecondaryObject(Arc<ObjectFilter>),
    AtEdgeNearTargetAndMove,
    Other(Name),
}

impl CreateLocation {
    #[must_use]
    pub fn parse(token: &str) -> Self {
        match token.to_ascii_uppercase().as_str() {
            "CREATE_AT_LOCATION" => Self::AtLocation,
            "CREATE_ABOVE_LOCATION" => Self::AboveLocation,
            "USE_OWNER_OBJECT" => Self::UseOwnerObject,
            "CREATE_AT_EDGE_NEAR_SOURCE" => Self::AtEdgeNearSource,
            "CREATE_AT_EDGE_NEAR_TARGET" => Self::AtEdgeNearTarget,
            "CREATE_AT_EDGE_FARTHEST_FROM_TARGET" => Self::AtEdgeFarthestFromTarget,
            _ => Self::Other(Name::from(token)),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WeatherKind {
    #[default]
    None,
    Cloudy,
    Rainy,
    CloudyRainy,
    Sunny,
}

impl WeatherKind {
    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        match token.to_ascii_uppercase().as_str() {
            "NONE" => Some(Self::None),
            "CLOUDY" => Some(Self::Cloudy),
            "RAINY" => Some(Self::Rainy),
            "CLOUDYRAINY" => Some(Self::CloudyRainy),
            "SUNNY" => Some(Self::Sunny),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PowerSides {
    #[default]
    Friends,
    Enemies,
    All,
    FriendsAndEnemies,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ModifierRule {
    pub list: Option<Name>,
    pub radius: f32,
    pub affects: Names,
    pub affects_self: bool,
    pub sides: PowerSides,
    pub anti: Names,
    pub re_enable: bool,
    pub anti_fx: Option<Name>,
    pub modifier_fx: Option<Name>,
    pub affect_good: bool,
    pub affect_evil: bool,
    pub affect_allies: bool,
}

#[derive(Debug, Default)]
pub(crate) struct ModifierFieldValues {
    pub(crate) list: Option<Name>,
    pub(crate) range: f32,
    pub(crate) affects: Names,
    pub(crate) affects_self: bool,
    pub(crate) all_sides: bool,
    pub(crate) target_enemy: bool,
    pub(crate) anti: Names,
    pub(crate) re_enable: bool,
    pub(crate) anti_fx: Option<Name>,
    pub(crate) modifier_fx: Option<Name>,
    pub(crate) affect_good: bool,
    pub(crate) affect_evil: bool,
    pub(crate) affect_allies: bool,
}

pub(crate) fn build_modifier_rule(fields: ModifierFieldValues) -> Option<ModifierRule> {
    if fields.list.is_none()
        && fields.anti.is_empty()
        && fields.anti_fx.is_none()
        && fields.modifier_fx.is_none()
    {
        return None;
    }
    let sides = modifier_sides(
        fields.all_sides,
        fields.re_enable,
        fields.target_enemy,
        fields.anti.is_empty(),
    );
    Some(ModifierRule {
        list: fields.list,
        radius: fields.range,
        affects: fields.affects,
        affects_self: fields.affects_self,
        sides,
        anti: fields.anti,
        re_enable: fields.re_enable,
        anti_fx: fields.anti_fx,
        modifier_fx: fields.modifier_fx,
        affect_good: fields.affect_good,
        affect_evil: fields.affect_evil,
        affect_allies: fields.affect_allies,
    })
}

fn modifier_sides(
    all_sides: bool,
    re_enable: bool,
    target_enemy: bool,
    anti_empty: bool,
) -> PowerSides {
    if all_sides {
        PowerSides::All
    } else if re_enable {
        PowerSides::FriendsAndEnemies
    } else {
        let base = if target_enemy {
            PowerSides::Enemies
        } else {
            PowerSides::Friends
        };
        if anti_empty {
            base
        } else {
            match base {
                PowerSides::Enemies => PowerSides::Friends,
                PowerSides::Friends => PowerSides::Enemies,
                other => other,
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct EffectLog {
    entries: Vec<(Entity, Effect)>,
}

impl EffectLog {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub fn push(&mut self, entity: Entity, effect: Effect) {
        self.entries.push((entity, effect));
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn drain(&mut self) -> Vec<(Entity, Effect)> {
        core::mem::take(&mut self.entries)
    }

    #[must_use]
    pub fn entries(&self) -> &[(Entity, Effect)] {
        &self.entries
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Changed(u16);

impl Changed {
    pub const NONE: Self = Self(0);
    pub const WEAPON_SET: Self = Self(1);
    pub const ARMOR_SET: Self = Self(1 << 1);
    pub const LOOK: Self = Self(1 << 2);
    pub const COMMAND_SET: Self = Self(1 << 3);
    pub const POWERS: Self = Self(1 << 4);
    pub const CARRIED: Self = Self(1 << 5);
    pub const LEVEL: Self = Self(1 << 6);
    pub const ATTRIBUTES: Self = Self(1 << 7);
    pub const GEOMETRY: Self = Self(1 << 8);

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    #[must_use]
    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextureSwap {
    pub from: Name,
    pub to: Name,
    pub exclude: Names,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UpgradeState {
    pub weapon_set_flags: Vec<Name>,
    pub armor_set_flags: Vec<Name>,
    pub model_conditions: Vec<Name>,
    pub sub_objects: Vec<(Name, bool)>,
    pub textures: Vec<TextureSwap>,
    pub command_set: Option<Name>,
    pub unpaused_powers: Vec<Name>,
    pub geometry: Vec<(Name, bool)>,
    pub status_bits: Vec<Name>,
    pub levels_gained: u32,
    pub attribute_modifiers: Vec<Name>,
    pub weapon_lock: Option<u8>,
    pub revision: u32,
}

const WEAPONSET_PREFIX: &str = "WEAPONSET_";

const WEAPON_RIDER_PREFIX: &str = "WEAPON_RIDER";

fn weapon_set_condition(flag: &str) -> Option<String> {
    if same(flag, "WEAPONSET_ONE_RING_MODE") {
        return Some("ONE_RING".to_string());
    }
    if same(flag, "HIDDEN") {
        return Some("HIDDEN".to_string());
    }
    if starts_with_ignore_case(flag, "ARMORSET_") {
        return None;
    }
    if let Some(rest) = flag.get(WEAPON_RIDER_PREFIX.len()..) {
        if starts_with_ignore_case(flag, WEAPON_RIDER_PREFIX) {
            return Some(format!("WEAPONSET_RIDER{rest}"));
        }
    }
    if starts_with_ignore_case(flag, WEAPONSET_PREFIX) {
        Some(flag.to_string())
    } else {
        Some(format!("WEAPONSET_{flag}"))
    }
}

#[must_use]
pub fn weapon_set_toggle_swap_condition(flag: &str) -> Option<String> {
    let mapped = weapon_set_condition(flag)?;
    for n in 1..=3 {
        if mapped.eq_ignore_ascii_case(&format!("WEAPONSET_TOGGLE_{n}")) {
            return Some(format!("SWAPPING_TO_WEAPONSET_{n}"));
        }
    }
    None
}

fn starts_with_ignore_case(s: &str, prefix: &str) -> bool {
    s.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}

fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn insert(set: &mut Vec<Name>, name: &Name) -> bool {
    if set.iter().any(|held| same(held, name)) {
        return false;
    }
    set.push(Name::clone(name));
    true
}

fn remove(set: &mut Vec<Name>, name: &str) -> bool {
    let before = set.len();
    set.retain(|held| !same(held, name));
    set.len() != before
}

fn show(overrides: &mut Vec<(Name, bool)>, name: &Name, shown: bool) -> bool {
    match overrides.iter_mut().find(|(held, _)| same(held, name)) {
        Some((_, state)) if *state == shown => false,
        Some((_, state)) => {
            *state = shown;
            true
        }
        None => {
            overrides.push((Name::clone(name), shown));
            true
        }
    }
}

impl UpgradeState {
    pub fn apply(&mut self, effect: &Effect) -> Changed {
        let changed = self.apply_unrevised(effect);
        if changed != Changed::NONE {
            self.revision = self.revision.wrapping_add(1);
        }
        changed
    }

    fn apply_unrevised(&mut self, effect: &Effect) -> Changed {
        match effect {
            Effect::WeaponSetFlag(flag) => {
                if insert(&mut self.weapon_set_flags, flag) {
                    return Changed::WEAPON_SET;
                }
            }
            Effect::ClearWeaponSetFlag(flag) => {
                if remove(&mut self.weapon_set_flags, flag) {
                    return Changed::WEAPON_SET;
                }
            }
            Effect::LockWeaponSlot(slot) => {
                if self.weapon_lock != Some(*slot) {
                    self.weapon_lock = Some(*slot);
                    return Changed::WEAPON_SET;
                }
            }
            Effect::ReleaseWeaponLock => {
                if self.weapon_lock.take().is_some() {
                    return Changed::WEAPON_SET;
                }
            }
            Effect::ArmorSetFlag(flag) => {
                if insert(&mut self.armor_set_flags, flag) {
                    return Changed::ARMOR_SET;
                }
            }
            Effect::ModelConditions { add, remove: gone } => {
                let mut changed = false;
                for flag in gone.iter() {
                    changed |= remove(&mut self.model_conditions, flag);
                }
                for flag in add.iter() {
                    changed |= insert(&mut self.model_conditions, flag);
                }
                if changed {
                    return Changed::LOOK;
                }
            }
            Effect::ToggleModelCondition(flag) => {
                let changed = if remove(&mut self.model_conditions, flag) {
                    true
                } else {
                    insert(&mut self.model_conditions, flag)
                };
                if changed {
                    return Changed::LOOK;
                }
            }
            Effect::SubObjects { show: shown, hide } => {
                let mut changed = false;
                for name in shown.iter() {
                    changed |= show(&mut self.sub_objects, name, true);
                }
                for name in hide.iter() {
                    changed |= show(&mut self.sub_objects, name, false);
                }
                if changed {
                    return Changed::LOOK;
                }
            }
            Effect::Texture { swaps, exclude } => {
                let list: Vec<TextureSwap> = swaps
                    .iter()
                    .map(|(from, to)| TextureSwap {
                        from: Name::clone(from),
                        to: Name::clone(to),
                        exclude: Names::clone(exclude),
                    })
                    .collect();
                if self.textures != list {
                    self.textures = list;
                    return Changed::LOOK;
                }
            }
            Effect::CommandSet(set) => {
                let next = (!set.is_empty() && !same(set, "NONE")).then(|| Name::clone(set));
                let unchanged = match (&self.command_set, &next) {
                    (Some(held), Some(new)) => same(held, new),
                    (None, None) => true,
                    _ => false,
                };
                if !unchanged {
                    self.command_set = next;
                    return Changed::COMMAND_SET;
                }
            }
            Effect::UnpausePower(power) => {
                if insert(&mut self.unpaused_powers, power) {
                    return Changed::POWERS;
                }
            }
            Effect::Geometry { show: shown, hide } => {
                let mut changed = false;
                for name in shown.iter() {
                    changed |= show(&mut self.geometry, name, true);
                }
                for name in hide.iter() {
                    changed |= show(&mut self.geometry, name, false);
                }
                if changed {
                    return Changed::GEOMETRY;
                }
            }
            Effect::StatusBits { set, clear } => {
                let mut changed = false;
                for bit in set.iter() {
                    changed |= insert(&mut self.status_bits, bit);
                }
                for bit in clear.iter() {
                    changed |= remove(&mut self.status_bits, bit);
                }
                if changed {
                    return Changed::CARRIED;
                }
            }
            Effect::LevelUp { levels, cap } => {
                let raised = self.levels_gained.saturating_add(*levels);
                let raised = cap.map_or(raised, |cap| raised.min(cap));
                if raised != self.levels_gained {
                    self.levels_gained = raised;
                    return Changed::LEVEL;
                }
            }
            Effect::AttributeModifier(modifier) => {
                if insert(&mut self.attribute_modifiers, modifier) {
                    return Changed::ATTRIBUTES;
                }
            }
            Effect::RemoveUpgrades(..)
            | Effect::CreateObjects { .. }
            | Effect::AreaHeal { .. }
            | Effect::AreaModifier { .. }
            | Effect::AreaLevelUp { .. }
            | Effect::TriggerModule { .. }
            | Effect::GrantPlayerUpgrade(_)
            | Effect::GrantUpgrade(_)
            | Effect::FireWeapon { .. }
            | Effect::PlayFx { .. }
            | Effect::CreateObject { .. }
            | Effect::Aura { .. }
            | Effect::SetWeatherEffect { .. }
            | Effect::ClearWeatherEffect
            | Effect::ChangeWeather { .. }
            | Effect::FearEveryoneAdmitted { .. }
            | Effect::SpacedObjects { .. }
            | Effect::CashBounty(_)
            | Effect::AddObject { .. }
            | Effect::FellTreesForMoney { .. }
            | Effect::MorphTrees { .. }
            | Effect::Capture { .. }
            | Effect::Dominate { .. }
            | Effect::CursePowers { .. }
            | Effect::DefectFor { .. }
            | Effect::JoinMaster { .. }
            | Effect::ProductionBonus { .. }
            | Effect::AutoHeal { .. }
            | Effect::MoveToObject { .. }
            | Effect::MoveToPosition { .. }
            | Effect::MoveToApproachPoint { .. }
            | Effect::Halt
            | Effect::HoldOrders(_)
            | Effect::FaceObject { .. }
            | Effect::FacePosition { .. }
            | Effect::StartRecharge(_)
            | Effect::MarkSpecialPowerTriggered(_)
            | Effect::SelfTimedModifier { .. }
            | Effect::ClearSelfTimedModifier(_)
            | Effect::TimedModelCondition { .. }
            | Effect::CastEdge { .. }
            | Effect::Spawn { .. }
            | Effect::AiMoveTo { .. }
            | Effect::AiAttack { .. }
            | Effect::Die
            | Effect::EnterRampage
            | Effect::TriggerReward { .. }
            | Effect::PlayObjectSound(_)
            | Effect::StartLoopSound(_)
            | Effect::StopLoopSound
            | Effect::CaptureFlash { .. }
            | Effect::CreateSlave { .. }
            | Effect::CommandPointBonus(_)
            | Effect::SpellRechargeCount { .. }
            | Effect::Replace { .. }
            | Effect::ReplaceAndAdd { .. }
            | Effect::PressCommand(_)
            | Effect::CastleUpgrade { .. }
            | Effect::InheritUpgrade { .. }
            | Effect::ShareUpgradesWithSlaves
            | Effect::ArrowStorm { .. }
            | Effect::ClearArrowStorm
            | Effect::Fling { .. }
            | Effect::RefreshOtherPowers { .. }
            | Effect::GrantInvisibility { .. }
            | Effect::ReplenishHorde { .. }
            | Effect::EmotionPulse { .. }
            | Effect::ToggleStealth { .. }
            | Effect::ToggleDisguise { .. }
            | Effect::Teleport { .. }
            | Effect::StoreObjects { .. }
            | Effect::TeleportGroup { .. }
            | Effect::ReplaceInArea { .. }
            | Effect::PlayWeaponPreFireFX { .. }
            | Effect::LoseStealth => {}
        }
        Changed::NONE
    }

    #[must_use]
    pub const fn is_world_effect(effect: &Effect) -> bool {
        matches!(
            effect,
            Effect::CreateObjects { .. }
                | Effect::AreaHeal { .. }
                | Effect::AreaModifier { .. }
                | Effect::AreaLevelUp { .. }
                | Effect::TriggerModule { .. }
                | Effect::GrantPlayerUpgrade(_)
                | Effect::GrantUpgrade(_)
                | Effect::FireWeapon { .. }
                | Effect::PlayFx { .. }
                | Effect::CreateObject { .. }
                | Effect::Aura { .. }
                | Effect::SetWeatherEffect { .. }
                | Effect::ClearWeatherEffect
                | Effect::ChangeWeather { .. }
                | Effect::FearEveryoneAdmitted { .. }
                | Effect::SpacedObjects { .. }
                | Effect::CashBounty(_)
                | Effect::AddObject { .. }
                | Effect::FellTreesForMoney { .. }
                | Effect::MorphTrees { .. }
                | Effect::Capture { .. }
                | Effect::Dominate { .. }
                | Effect::CursePowers { .. }
                | Effect::DefectFor { .. }
                | Effect::JoinMaster { .. }
                | Effect::ProductionBonus { .. }
                | Effect::AutoHeal { .. }
                | Effect::SelfTimedModifier { .. }
                | Effect::ClearSelfTimedModifier(_)
                | Effect::TimedModelCondition { .. }
                | Effect::CastEdge { .. }
                | Effect::Spawn { .. }
                | Effect::CreateSlave { .. }
                | Effect::CommandPointBonus(_)
                | Effect::SpellRechargeCount { .. }
                | Effect::Replace { .. }
                | Effect::PressCommand(_)
                | Effect::ArrowStorm { .. }
                | Effect::ClearArrowStorm
                | Effect::Fling { .. }
                | Effect::RefreshOtherPowers { .. }
                | Effect::GrantInvisibility { .. }
                | Effect::ToggleStealth { .. }
                | Effect::ToggleDisguise { .. }
                | Effect::Teleport { .. }
                | Effect::StoreObjects { .. }
                | Effect::TeleportGroup { .. }
                | Effect::ReplaceInArea { .. }
                | Effect::PlayWeaponPreFireFX { .. }
                | Effect::LoseStealth
        )
    }

    #[must_use]
    pub fn has_weapon_set_flag(&self, flag: &str) -> bool {
        self.weapon_set_flags.iter().any(|held| same(held, flag))
    }

    #[must_use]
    pub fn has_model_condition(&self, name: &str) -> bool {
        self.model_conditions.iter().any(|held| same(held, name))
    }

    #[must_use]
    pub fn model_condition_names(&self) -> Vec<String> {
        let mut out: Vec<String> = self
            .model_conditions
            .iter()
            .map(|flag| flag.to_string())
            .collect();
        for flag in &self.weapon_set_flags {
            let Some(mapped) = weapon_set_condition(flag) else {
                continue;
            };
            if !out.iter().any(|held| held.eq_ignore_ascii_case(&mapped)) {
                out.push(mapped);
            }
        }
        out
    }

    #[must_use]
    pub fn is_unpaused(&self, power: &str) -> bool {
        self.unpaused_powers.iter().any(|held| same(held, power))
    }

    #[must_use]
    pub fn sub_object(&self, name: &str) -> Option<bool> {
        self.sub_objects
            .iter()
            .find(|(held, _)| same(held, name))
            .map(|(_, shown)| *shown)
    }

    #[must_use]
    pub fn touches_look(&self) -> bool {
        !(self.model_conditions.is_empty()
            && self.weapon_set_flags.is_empty()
            && self.sub_objects.is_empty()
            && self.textures.is_empty())
    }
}

fn hash_name(hasher: &mut StateHasher, name: &str) {
    hasher.write_usize(name.len());
    hasher.write_bytes(name.as_bytes());
}

fn hash_names(hasher: &mut StateHasher, list: &[Name]) {
    hasher.write_usize(list.len());
    for name in list {
        hash_name(hasher, name);
    }
}

fn hash_overrides(hasher: &mut StateHasher, list: &[(Name, bool)]) {
    hasher.write_usize(list.len());
    for (name, shown) in list {
        hash_name(hasher, name);
        hasher.write_bool(*shown);
    }
}

impl Component for UpgradeState {
    fn hash_into(&self, hasher: &mut StateHasher) {
        let present = [
            !self.weapon_set_flags.is_empty(),
            !self.armor_set_flags.is_empty(),
            !self.model_conditions.is_empty(),
            !self.sub_objects.is_empty(),
            !self.textures.is_empty(),
            self.command_set.is_some(),
            !self.unpaused_powers.is_empty(),
            !self.geometry.is_empty(),
            !self.status_bits.is_empty(),
            self.levels_gained != 0,
            !self.attribute_modifiers.is_empty(),
            self.revision != 0,
            self.weapon_lock.is_some(),
        ];
        let mask = present
            .iter()
            .rev()
            .fold(0u16, |mask, bit| (mask << 1) | u16::from(*bit));
        hasher.write_u16(mask);
        if mask == 0 {
            return;
        }
        let [weapons, armours, conditions, sub_objects, textures, command_set, powers, geometry, status, levels, modifiers, revision, lock] =
            present;
        if weapons {
            hash_names(hasher, &self.weapon_set_flags);
        }
        if armours {
            hash_names(hasher, &self.armor_set_flags);
        }
        if conditions {
            hash_names(hasher, &self.model_conditions);
        }
        if sub_objects {
            hash_overrides(hasher, &self.sub_objects);
        }
        if textures {
            hasher.write_usize(self.textures.len());
            for swap in &self.textures {
                hash_name(hasher, &swap.from);
                hash_name(hasher, &swap.to);
                hash_names(hasher, &swap.exclude);
            }
        }
        if let (true, Some(set)) = (command_set, &self.command_set) {
            hash_name(hasher, set);
        }
        if powers {
            hash_names(hasher, &self.unpaused_powers);
        }
        if geometry {
            hash_overrides(hasher, &self.geometry);
        }
        if status {
            hash_names(hasher, &self.status_bits);
        }
        if levels {
            hasher.write_u32(self.levels_gained);
        }
        if modifiers {
            hash_names(hasher, &self.attribute_modifiers);
        }
        if revision {
            hasher.write_u32(self.revision);
        }
        if let (true, Some(slot)) = (lock, self.weapon_lock) {
            hasher.write_u8(slot);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(text: &str) -> Name {
        Name::from(text)
    }

    fn list(items: &[&str]) -> Names {
        items
            .iter()
            .map(|item| name(item))
            .collect::<Vec<_>>()
            .into()
    }

    fn hash(state: &UpgradeState) -> u64 {
        let mut hasher = StateHasher::new();
        state.hash_into(&mut hasher);
        hasher.finish()
    }

    #[test]
    fn a_flag_raised_twice_changes_the_state_once() {
        let mut state = UpgradeState::default();
        let effect = Effect::WeaponSetFlag(name("PLAYER_UPGRADE"));
        assert_eq!(state.apply(&effect), Changed::WEAPON_SET);
        assert_eq!(state.apply(&effect), Changed::NONE);
        assert_eq!(
            state.apply(&Effect::WeaponSetFlag(name("player_upgrade"))),
            Changed::NONE,
            "INI tokens compare case-insensitively"
        );
        assert_eq!(state.weapon_set_flags.len(), 1);
        assert!(state.has_weapon_set_flag("Player_Upgrade"));
        assert_eq!(state.revision, 1, "only the change moved the revision");
    }

    #[test]
    fn a_sub_object_keeps_the_last_visibility_written() {
        let mut state = UpgradeState::default();
        state.apply(&Effect::SubObjects {
            show: list(&[]),
            hide: list(&["V1", "V2"]),
        });
        state.apply(&Effect::SubObjects {
            show: list(&["V1"]),
            hide: list(&["V2"]),
        });
        assert_eq!(state.sub_object("v1"), Some(true));
        assert_eq!(state.sub_object("V2"), Some(false));
        state.apply(&Effect::SubObjects {
            show: list(&["V1", "V2"]),
            hide: list(&[]),
        });
        assert_eq!(state.sub_object("V2"), Some(true));
        assert_eq!(state.sub_object("Tower"), None);
    }

    #[test]
    fn a_name_both_shown_and_hidden_by_one_effect_ends_hidden() {
        let mut state = UpgradeState::default();
        state.apply(&Effect::SubObjects {
            show: list(&["Flag"]),
            hide: list(&["Flag"]),
        });
        assert_eq!(state.sub_object("Flag"), Some(false));
    }

    #[test]
    fn the_command_set_override_is_the_last_one_written() {
        let mut state = UpgradeState::default();
        assert_eq!(
            state.apply(&Effect::CommandSet(name("Level2"))),
            Changed::COMMAND_SET
        );
        assert_eq!(
            state.apply(&Effect::CommandSet(name("Level3"))),
            Changed::COMMAND_SET
        );
        assert_eq!(state.command_set.as_deref(), Some("Level3"));
        assert_eq!(
            state.apply(&Effect::CommandSet(name("level3"))),
            Changed::NONE
        );
    }

    #[test]
    fn levels_gained_are_capped() {
        let mut state = UpgradeState::default();
        let effect = Effect::LevelUp {
            levels: 1,
            cap: Some(2),
        };
        assert_eq!(state.apply(&effect), Changed::LEVEL);
        assert_eq!(state.apply(&effect), Changed::LEVEL);
        assert_eq!(state.apply(&effect), Changed::NONE);
        assert_eq!(state.levels_gained, 2);
    }

    #[test]
    fn an_attribute_modifier_list_is_held_once() {
        let mut state = UpgradeState::default();
        let effect = Effect::AttributeModifier(name("Bonus"));
        assert_eq!(state.apply(&effect), Changed::ATTRIBUTES);
        assert_eq!(
            state.apply(&Effect::AttributeModifier(name("BONUS"))),
            Changed::NONE
        );
        assert_eq!(state.attribute_modifiers.len(), 1);
    }

    #[test]
    fn model_conditions_clear_before_they_set() {
        let mut state = UpgradeState::default();
        state.apply(&Effect::ModelConditions {
            add: list(&["USER_1", "USER_2"]),
            remove: list(&[]),
        });
        state.apply(&Effect::ModelConditions {
            add: list(&["USER_2"]),
            remove: list(&["USER_1", "USER_2"]),
        });
        assert_eq!(state.model_condition_names(), ["USER_2"]);
    }

    #[test]
    fn weapon_set_flags_imply_their_model_conditions() {
        let mut state = UpgradeState::default();
        state.apply(&Effect::WeaponSetFlag(name("PLAYER_UPGRADE")));
        state.apply(&Effect::WeaponSetFlag(name("WEAPONSET_TOGGLE_1")));
        state.apply(&Effect::ModelConditions {
            add: list(&["USER_3", "weaponset_player_upgrade"]),
            remove: list(&[]),
        });
        assert_eq!(
            state.model_condition_names(),
            ["USER_3", "weaponset_player_upgrade", "WEAPONSET_TOGGLE_1"],
            "a flag already set by name is not repeated"
        );
        let mut rampage = UpgradeState::default();
        rampage.apply(&Effect::WeaponSetFlag(name("RAMPAGE")));
        assert_eq!(rampage.model_condition_names(), ["WEAPONSET_RAMPAGE"]);
    }

    #[test]
    fn weapon_set_flags_have_the_engine_exceptions() {
        let mut rider = UpgradeState::default();
        rider.apply(&Effect::WeaponSetFlag(name("WEAPON_RIDER3")));
        assert_eq!(rider.model_condition_names(), ["WEAPONSET_RIDER3"]);

        let mut ring = UpgradeState::default();
        ring.apply(&Effect::WeaponSetFlag(name("WEAPONSET_ONE_RING_MODE")));
        assert_eq!(ring.model_condition_names(), ["ONE_RING"]);

        let mut hidden = UpgradeState::default();
        hidden.apply(&Effect::WeaponSetFlag(name("HIDDEN")));
        assert_eq!(hidden.model_condition_names(), ["HIDDEN"]);

        let mut cah = UpgradeState::default();
        cah.apply(&Effect::WeaponSetFlag(name("ARMORSET_CREATE_A_HERO_01")));
        assert!(cah.model_condition_names().is_empty());
    }

    #[test]
    fn model_conditions_add_then_remove() {
        let mut state = UpgradeState::default();
        state.apply(&Effect::ModelConditions {
            add: list(&["USER_1", "USER_2"]),
            remove: list(&[]),
        });
        state.apply(&Effect::ModelConditions {
            add: list(&[]),
            remove: list(&["user_1"]),
        });
        assert_eq!(state.model_conditions, vec![name("USER_2")]);
    }

    #[test]
    fn toggle_model_condition_flips_each_press() {
        let mut state = UpgradeState::default();
        let changed = state.apply(&Effect::ToggleModelCondition(name("MOUNTED")));
        assert_eq!(changed, Changed::LOOK);
        assert_eq!(state.model_conditions, vec![name("MOUNTED")]);
        state.apply(&Effect::ToggleModelCondition(name("MOUNTED")));
        assert!(state.model_conditions.is_empty());
    }

    #[test]
    fn every_field_changes_the_hash() {
        let base = UpgradeState::default();
        let effects = [
            Effect::WeaponSetFlag(name("A")),
            Effect::ArmorSetFlag(name("A")),
            Effect::ModelConditions {
                add: list(&["A"]),
                remove: list(&[]),
            },
            Effect::SubObjects {
                show: list(&["A"]),
                hide: list(&[]),
            },
            Effect::Texture {
                swaps: Arc::from([(name("a.tga"), name("b.tga"))]),
                exclude: list(&[]),
            },
            Effect::CommandSet(name("A")),
            Effect::UnpausePower(name("A")),
            Effect::Geometry {
                show: list(&["A"]),
                hide: list(&[]),
            },
            Effect::StatusBits {
                set: list(&["A"]),
                clear: list(&[]),
            },
            Effect::LevelUp {
                levels: 1,
                cap: None,
            },
            Effect::AttributeModifier(name("A")),
        ];
        let mut seen = vec![hash(&base)];
        for effect in &effects {
            let mut state = base.clone();
            assert_ne!(state.apply(effect), Changed::NONE, "{effect:?}");
            seen.push(hash(&state));
        }
        let count = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), count, "two effects hashed alike");
    }

    #[test]
    fn a_field_hashes_its_contents_not_only_its_presence() {
        let with = |name_: &str| {
            vec![
                Effect::WeaponSetFlag(name(name_)),
                Effect::ArmorSetFlag(name(name_)),
                Effect::ModelConditions {
                    add: list(&[name_]),
                    remove: list(&[]),
                },
                Effect::SubObjects {
                    show: list(&[name_]),
                    hide: list(&[]),
                },
                Effect::Texture {
                    swaps: Arc::from([(name(name_), name("b.tga"))]),
                    exclude: list(&[]),
                },
                Effect::CommandSet(name(name_)),
                Effect::UnpausePower(name(name_)),
                Effect::Geometry {
                    show: list(&[name_]),
                    hide: list(&[]),
                },
                Effect::StatusBits {
                    set: list(&[name_]),
                    clear: list(&[]),
                },
                Effect::LevelUp {
                    levels: u32::try_from(name_.len()).unwrap(),
                    cap: None,
                },
                Effect::AttributeModifier(name(name_)),
            ]
        };
        for (a, b) in with("A").iter().zip(&with("BB")) {
            let (mut left, mut right) = (UpgradeState::default(), UpgradeState::default());
            left.apply(a);
            right.apply(b);
            assert_ne!(hash(&left), hash(&right), "{a:?} against {b:?}");
        }
        let mut once = UpgradeState::default();
        once.apply(&Effect::AttributeModifier(name("A")));
        let mut twice = once.clone();
        twice.revision = 2;
        assert_ne!(hash(&once), hash(&twice), "the revision is hashed");
    }

    #[test]
    fn the_log_keeps_report_order_and_drains_empty() {
        let mut world = mint_sim::World::new();
        let a = world.spawn().unwrap();
        let b = world.spawn().unwrap();
        let mut log = EffectLog::new();
        log.push(b, Effect::CommandSet(name("X")));
        log.push(a, Effect::CommandSet(name("Y")));
        let drained = log.drain();
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].0, b);
        assert!(log.is_empty());
    }
}
