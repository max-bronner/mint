use std::collections::BTreeMap;

use crate::tree::{Block, Node};
use crate::value;

use super::grammar::{
    fields, index_enum, integer, label, labelled, labels_in_any_order, number, percent, walk, Cx,
    Defined, EnumList, Percent, Prim, Tokens,
};

index_enum! {
    pub enum UnitType ("auto-resolve unit type") {
        Soldier = "AutoResolveUnit_Soldier",
        Archer = "AutoResolveUnit_Archer",
        Pikemen = "AutoResolveUnit_Pikemen",
        Cavalry = "AutoResolveUnit_Cavalry",
        Monster = "AutoResolveUnit_Monster",
        Hero = "AutoResolveUnit_Hero",
        Fortress = "AutoResolveUnit_Fortress",
        Siege = "AutoResolveUnit_Siege",
        Support = "AutoResolveUnit_Support",
        Invalid = "AutoResolveUnit_INVALID",
    }
}

fn compound<'a>(args: &'a [String], labels: &[&str]) -> Option<Vec<Option<&'a str>>> {
    labels_in_any_order(&labelled(args)?, labels)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DamagePerRound {
    pub damage: f32,
    pub against: UnitType,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LevelBonus {
    pub level: i32,
    pub bonus: f32,
}

fields! {
    pub struct AutoResolveWeaponDef {
        reduce_attack_when_hurt: Option<bool> = "ReduceAttackWhenHurt",
        miss_percent_chance: Option<i32> = "MissPercentChance",
    }
    with {
        pub damage_per_round: Vec<DamagePerRound>,
        pub level_bonus: Vec<LevelBonus>,
    }
}

impl Defined for AutoResolveWeaponDef {
    fn def_name(&self) -> &str {
        &self.name
    }
}

impl AutoResolveWeaponDef {
    pub(crate) fn read(block: &Block, cx: &Cx) -> Self {
        let ctx = label(block);
        let mut out = Self {
            name: block.name().unwrap_or_default().to_owned(),
            ..Self::default()
        };
        if block.name().is_none() {
            cx.missing_name(block);
        }
        walk(block, cx, &ctx, |node| {
            let Node::Field(field) = node else {
                return false;
            };
            match field.keyword.as_str() {
                "DamagePerRound" => {
                    let read =
                        compound(&field.args, &["Damage", "Against"]).and_then(|parts| match parts
                            .as_slice()
                        {
                            [Some(damage), Some(against)] => Some(DamagePerRound {
                                damage: number(damage, cx.macros)?,
                                against: UnitType::from_name(against)?,
                            }),
                            _ => None,
                        });
                    match read {
                        Some(entry) => out.damage_per_round.push(entry),
                        None => cx
                            .errors
                            .field(&ctx, field, "Damage:<real> Against:<unit type>"),
                    }
                    true
                }
                "LevelBonus" => {
                    let read = compound(&field.args, &["Level", "Bonus"]).and_then(|parts| {
                        match parts.as_slice() {
                            [Some(level), Some(bonus)] => Some(LevelBonus {
                                level: integer(level, cx.macros)?,
                                bonus: percent(bonus, cx.macros)?,
                            }),
                            _ => None,
                        }
                    });
                    match read {
                        Some(entry) if out.level_bonus.iter().all(|e| e.level != entry.level) => {
                            out.level_bonus.push(entry);
                        }
                        _ => cx.errors.field(
                            &ctx,
                            field,
                            "Level:<int> Bonus:<percent>, each level once",
                        ),
                    }
                    true
                }
                _ => out.read_field(field, cx, &ctx),
            }
        });
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ArmorEntry {
    pub against: UnitType,
    pub taken: f32,
}

fields! {
    pub struct AutoResolveArmorDef {}
    with {
        pub armor: Vec<ArmorEntry>,
    }
}

impl Defined for AutoResolveArmorDef {
    fn def_name(&self) -> &str {
        &self.name
    }
}

impl AutoResolveArmorDef {
    pub(crate) fn read(block: &Block, cx: &Cx) -> Self {
        let ctx = label(block);
        let mut out = Self {
            name: block.name().unwrap_or_default().to_owned(),
            ..Self::default()
        };
        if block.name().is_none() {
            cx.missing_name(block);
        }
        walk(block, cx, &ctx, |node| {
            let Node::Field(field) = node else {
                return false;
            };
            if field.keyword != "Armor" {
                return false;
            }
            let read = match field.args.as_slice() {
                [kind, rest @ ..] => {
                    UnitType::from_name(kind).zip(value::resolve_percent(rest, cx.macros))
                }
                [] => None,
            };
            match read {
                Some((against, taken)) => out.armor.push(ArmorEntry { against, taken }),
                None => cx.errors.field(&ctx, field, "<unit type> <percent>"),
            }
            true
        });
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AtLevel {
    pub level: i32,
    pub amount: f32,
}

fields! {
    pub struct AutoResolveBodyDef {
        can_be_attacked: Option<bool> = "CanBeAttacked",
        leave_in_army_summary: Option<bool> = "LeaveInArmySummary",
    }
    with {
        pub hitpoints_at_level: Vec<AtLevel>,
        pub regenerate_at_level: Vec<AtLevel>,
    }
}

impl Defined for AutoResolveBodyDef {
    fn def_name(&self) -> &str {
        &self.name
    }
}

fn at_level(args: &[String], amount_label: &str, cx: &Cx) -> Option<AtLevel> {
    let parts = compound(args, &[amount_label, "Level"])?;
    match parts.as_slice() {
        [Some(amount), Some(level)] => Some(AtLevel {
            level: integer(level, cx.macros)?,
            amount: number(amount, cx.macros)?,
        }),
        _ => None,
    }
}

impl AutoResolveBodyDef {
    pub(crate) fn read(block: &Block, cx: &Cx) -> Self {
        let ctx = label(block);
        let mut out = Self {
            name: block.name().unwrap_or_default().to_owned(),
            ..Self::default()
        };
        if block.name().is_none() {
            cx.missing_name(block);
        }
        walk(block, cx, &ctx, |node| {
            let Node::Field(field) = node else {
                return false;
            };
            let (label_name, into) = match field.keyword.as_str() {
                "HitpointsAtLevel" => ("Hitpoints", 0),
                "RegenerateAtLevel" => ("Regenerate", 1),
                _ => return out.read_field(field, cx, &ctx),
            };
            let list = if into == 0 {
                &mut out.hitpoints_at_level
            } else {
                &mut out.regenerate_at_level
            };
            match at_level(&field.args, label_name, cx) {
                Some(entry) if list.iter().all(|e| e.level != entry.level) => list.push(entry),
                _ => cx
                    .errors
                    .field(&ctx, field, "<label>:<real> Level:<int>, each level once"),
            }
            true
        });
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainTarget {
    pub target: UnitType,
    pub priority: i32,
}

fields! {
    pub struct AutoResolveCombatChainDef {}
    with {
        pub targets: Vec<ChainTarget>,
    }
}

impl Defined for AutoResolveCombatChainDef {
    fn def_name(&self) -> &str {
        &self.name
    }
}

impl AutoResolveCombatChainDef {
    pub(crate) fn read(block: &Block, cx: &Cx) -> Self {
        let ctx = label(block);
        let mut out = Self {
            name: block.name().unwrap_or_default().to_owned(),
            ..Self::default()
        };
        if block.name().is_none() {
            cx.missing_name(block);
        }
        walk(block, cx, &ctx, |node| {
            let Node::Field(field) = node else {
                return false;
            };
            if field.keyword != "Target" {
                return false;
            }
            let read = compound(&field.args, &["Target", "Priority"]).and_then(|parts| match parts
                .as_slice()
            {
                [Some(target), Some(priority)] => Some(ChainTarget {
                    target: UnitType::from_name(target)?,
                    priority: integer(priority, cx.macros)?,
                }),
                _ => None,
            });
            match read {
                Some(entry) if out.targets.iter().all(|t| t.target != entry.target) => {
                    out.targets.push(entry);
                }
                _ => cx.errors.field(
                    &ctx,
                    field,
                    "Target:<unit type> Priority:<int>, each type once",
                ),
            }
            true
        });
        out
    }
}

fields! {
    pub struct LeadershipBonus {
        min_level: Option<i32> = "MinLevel",
        maximum_units_affected: Option<i32> = "MaximumUnitsAffected",
        priority: Option<i32> = "Priority",
        weapon_multiplier: Option<Percent> = "WeaponMultiplier",
        experience_multiplier: Option<Percent> = "ExperienceMultiplier",
        armor_multiplier: Option<Percent> = "ArmorMultiplier",
    }
}

fields! {
    pub struct AutoResolveLeadershipDef {
        affects: EnumList<UnitType> = "Affects",
        affects_higher_level_first: Option<bool> = "AffectsHigherLevelFirst",
    }
    with {
        pub bonuses: Vec<LeadershipBonus>,
    }
}

impl Defined for AutoResolveLeadershipDef {
    fn def_name(&self) -> &str {
        &self.name
    }
}

impl AutoResolveLeadershipDef {
    pub(crate) fn read(block: &Block, cx: &Cx) -> Self {
        let ctx = label(block);
        let mut out = Self {
            name: block.name().unwrap_or_default().to_owned(),
            ..Self::default()
        };
        if block.name().is_none() {
            cx.missing_name(block);
        }
        walk(block, cx, &ctx, |node| match node {
            Node::Field(field) => out.read_field(field, cx, &ctx),
            Node::Block(child) if child.keyword == "BonusForLevel" => {
                let inner = label(child);
                let mut bonus = LeadershipBonus::default();
                walk(child, cx, &format!("{ctx}/{inner}"), |n| match n {
                    Node::Field(f) => bonus.read_field(f, cx, &inner),
                    _ => false,
                });
                out.bonuses.push(bonus);
                true
            }
            _ => false,
        });
        out
    }
}

fields! {
    pub struct AutoResolveHandicapDef {
        gui_displayed_level: Option<i32> = "GUIDisplayedLevel",
        weapon_multiplier: Option<f32> = "WeaponMultiplier",
        armor_multiplier: Option<f32> = "ArmorMultiplier",
        experience_multiplier: Option<f32> = "ExperienceMultiplier",
    }
}

impl AutoResolveHandicapDef {
    pub(crate) fn read(block: &Block, cx: &Cx) -> Self {
        let ctx = label(block);
        let mut out = Self::default();
        walk(block, cx, &ctx, |node| match node {
            Node::Field(field) => out.read_field(field, cx, &ctx),
            _ => false,
        });
        out
    }
}

fields! {
    pub struct ResourceBonusStep {
        min_resource_bonus: Option<i32> = "MinResourceBonus",
        weapon_multiplier: Option<f32> = "WeaponMultiplier",
        armor_multiplier: Option<f32> = "ArmorMultiplier",
    }
}

fields! {
    pub struct SciencePointBonusStep {
        min_science_purchase_points_for_bonus: Option<i32> = "MinSciencePurchasePointsForBonus",
        weapon_multiplier: Option<f32> = "WeaponMultiplier",
        armor_multiplier: Option<f32> = "ArmorMultiplier",
    }
}

fn read_steps<S: Default>(
    block: &Block,
    cx: &Cx,
    mut field: impl FnMut(&mut S, &crate::tree::Field, &str) -> bool,
    sides: &mut Tokens,
) -> Vec<S> {
    let ctx = label(block);
    let mut steps = Vec::new();
    walk(block, cx, &ctx, |node| match node {
        Node::Field(f) if f.keyword == "Sides" => {
            *sides = Tokens(f.args.clone());
            true
        }
        Node::Block(child) if child.keyword == "Bonus" => {
            let inner = format!("{ctx}/Bonus");
            let mut step = S::default();
            walk(child, cx, &inner, |n| match n {
                Node::Field(f) => field(&mut step, f, &inner),
                _ => false,
            });
            steps.push(step);
            true
        }
        _ => false,
    });
    steps
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResourceBonusDef {
    pub sides: Tokens,
    pub bonuses: Vec<ResourceBonusStep>,
}

impl ResourceBonusDef {
    pub(crate) fn read(block: &Block, cx: &Cx) -> Self {
        let mut sides = Tokens::default();
        let bonuses = read_steps::<ResourceBonusStep>(
            block,
            cx,
            |step, f, ctx| step.read_field(f, cx, ctx),
            &mut sides,
        );
        Self { sides, bonuses }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SciencePointBonusDef {
    pub sides: Tokens,
    pub bonuses: Vec<SciencePointBonusStep>,
}

impl SciencePointBonusDef {
    pub(crate) fn read(block: &Block, cx: &Cx) -> Self {
        let mut sides = Tokens::default();
        let bonuses = read_steps::<SciencePointBonusStep>(
            block,
            cx,
            |step, f, ctx| step.read_field(f, cx, ctx),
            &mut sides,
        );
        Self { sides, bonuses }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScheduleSide {
    Attacker,
    Defender,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReinforcementScheduleDef {
    pub side: ScheduleSide,
    pub rounds: BTreeMap<u32, i32>,
    pub each_remaining: Option<i32>,
}

impl ReinforcementScheduleDef {
    pub(crate) fn read(block: &Block, cx: &Cx) -> Option<Self> {
        let ctx = label(block);
        let side = match block.name() {
            Some(n) if n.eq_ignore_ascii_case("Attacker") => ScheduleSide::Attacker,
            Some(n) if n.eq_ignore_ascii_case("Defender") => ScheduleSide::Defender,
            _ => {
                cx.block_error(&ctx, block, "Attacker or Defender");
                return None;
            }
        };
        let mut out = Self {
            side,
            rounds: BTreeMap::new(),
            each_remaining: None,
        };
        walk(block, cx, &ctx, |node| {
            let Node::Field(field) = node else {
                return false;
            };
            if field.keyword.eq_ignore_ascii_case("EachRemaining") {
                match <i32 as Prim>::parse(&field.args, cx.macros) {
                    Some(v) => out.each_remaining = Some(v),
                    None => cx.errors.field(&ctx, field, "an integer"),
                }
                return true;
            }
            let index = leading_index(&field.keyword);
            let round = <i32 as Prim>::parse(&field.args, cx.macros);
            match (index, round) {
                (Some(index), Some(round)) if !out.rounds.contains_key(&index) => {
                    out.rounds.insert(index, round);
                }
                _ => cx.errors.field(
                    &ctx,
                    field,
                    "a positive army number, once, with an integer round",
                ),
            }
            true
        });
        Some(out)
    }
}

fn leading_index(name: &str) -> Option<u32> {
    if name.starts_with('#') {
        return None;
    }
    let parsed = value::parse_int(name)?;
    u32::try_from(parsed).ok().filter(|index| *index > 0)
}
