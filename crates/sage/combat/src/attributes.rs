use std::collections::BTreeMap;

use mint_sage_ini::value::{parse_bool, parse_float_or_macro, parse_percent_shaped};
use mint_sage_ini::{Block, Field, MacroTable};
use mint_sim::StateHasher;

use crate::damage::{DamageTypeId, DamageTypes};

pub const MODIFIER_LIST_KEYWORD: &str = "ModifierList";

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AttributeKind {
    Armor,
    DamageAdd,
    DamageMult,
    Speed,
    Health,
    HealthMult,
    Range,
    Experience,
    Production,
    RechargeTime,
    SpellDamage,
    RateOfFire,
    ShroudClearing,
    AutoHeal,
    CommandPointBonus,
    BountyPercentage,
    Invulnerable,
    Vision,
    CrushDecelerate,
    CrushedDecelerate,
    MinimumCrushVelocity,
    CrushableLevel,
    ResistFear,
    ResistTerror,
    ResistKnockback,
    Unapplied,
}

impl AttributeKind {
    #[must_use]
    pub fn parse(token: &str) -> Self {
        match token.to_ascii_uppercase().as_str() {
            "ARMOR" => Self::Armor,
            "DAMAGE_ADD" => Self::DamageAdd,
            "DAMAGE_MULT" => Self::DamageMult,
            "SPEED" => Self::Speed,
            "HEALTH" => Self::Health,
            "HEALTH_MULT" => Self::HealthMult,
            "RANGE" => Self::Range,
            "EXPERIENCE" => Self::Experience,
            "PRODUCTION" => Self::Production,
            "RECHARGE_TIME" => Self::RechargeTime,
            "SPELL_DAMAGE" => Self::SpellDamage,
            "RATE_OF_FIRE" => Self::RateOfFire,
            "SHROUD_CLEARING" => Self::ShroudClearing,
            "AUTO_HEAL" => Self::AutoHeal,
            "COMMAND_POINT_BONUS" => Self::CommandPointBonus,
            "BOUNTY_PERCENTAGE" => Self::BountyPercentage,
            "INVULNERABLE" => Self::Invulnerable,
            "VISION" => Self::Vision,
            "CRUSH_DECELERATE" => Self::CrushDecelerate,
            "CRUSHED_DECELERATE" => Self::CrushedDecelerate,
            "MINIMUM_CRUSH_VELOCITY" => Self::MinimumCrushVelocity,
            "CRUSHABLE_LEVEL" => Self::CrushableLevel,
            "RESIST_FEAR" => Self::ResistFear,
            "RESIST_TERROR" => Self::ResistTerror,
            "RESIST_KNOCKBACK" => Self::ResistKnockback,
            _ => Self::Unapplied,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Modifier {
    pub kind: AttributeKind,
    pub value: f32,
    pub types: u64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ModifierList {
    pub name: String,
    pub category: Option<String>,
    pub modifiers: Vec<Modifier>,
    pub duration_ms: Option<u32>,
    pub ignore_if_anti_category_active: bool,
}

impl ModifierList {
    #[must_use]
    pub fn is_permanent(&self) -> bool {
        self.duration_ms == Some(0)
    }

    #[must_use]
    pub fn health_change(&self) -> (f32, f32) {
        self.modifiers
            .iter()
            .fold((0.0, 1.0), |(add, mult), modifier| match modifier.kind {
                AttributeKind::Health => (add + modifier.value, mult),
                AttributeKind::HealthMult => (add, mult * modifier.value),
                _ => (add, mult),
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModifierId(u16);

impl ModifierId {
    #[must_use]
    pub const fn index(self) -> u16 {
        self.0
    }

    #[must_use]
    pub const fn from_index(index: u16) -> Self {
        Self(index)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ModifierStore {
    lists: Vec<ModifierList>,
    by_name: BTreeMap<String, u16>,
    unapplied: usize,
    unresolved: usize,
}

impl ModifierStore {
    pub fn load(&mut self, block: &Block, macros: &MacroTable, types: &mut DamageTypes) {
        let Some(name) = block.name() else {
            return;
        };
        let mut list = ModifierList {
            name: name.to_owned(),
            category: block
                .field("Category")
                .and_then(Field::value)
                .map(str::to_owned),
            ignore_if_anti_category_active: block
                .field("IgnoreIfAnticategoryActive")
                .and_then(Field::value)
                .and_then(parse_bool)
                .unwrap_or(false),
            ..ModifierList::default()
        };
        for field in block.fields() {
            if !field.keyword.eq_ignore_ascii_case("Modifier") {
                continue;
            }
            let mut args = field.args.iter();
            let (Some(kind), Some(raw)) = (args.next(), args.next()) else {
                self.unresolved = self.unresolved.saturating_add(1);
                continue;
            };
            let kind = AttributeKind::parse(kind);
            if kind == AttributeKind::Unapplied {
                self.unapplied = self.unapplied.saturating_add(1);
            }
            let mut rest: Vec<&str> = args.map(String::as_str).collect();
            let mut raw = raw.as_str();
            if kind == AttributeKind::Invulnerable {
                if let Some((first, names)) = macros.tokens(raw).and_then(<[String]>::split_first) {
                    raw = first.as_str();
                    rest = names.iter().map(String::as_str).chain(rest).collect();
                }
            }
            let mut mask = 0u64;
            if kind == AttributeKind::Invulnerable {
                for name in rest.iter().filter(|name| **name != "%") {
                    match types.intern(name).map(DamageTypeId::index) {
                        Some(at) if at < 64 => mask |= 1u64 << at,
                        _ => self.unresolved = self.unresolved.saturating_add(1),
                    }
                }
            }
            match value(raw, macros) {
                Some(value) => {
                    list.modifiers.push(Modifier {
                        kind,
                        value,
                        types: mask,
                    });
                }
                None => self.unresolved = self.unresolved.saturating_add(1),
            }
        }
        list.duration_ms = match block.field("Duration").and_then(Field::value) {
            None => Some(0),
            Some(raw) => {
                let resolved = parse_float_or_macro(raw, macros)
                    .filter(|ms| ms.is_finite() && *ms >= 0.0 && *ms <= 86_400_000.0);
                if resolved.is_none() {
                    self.unresolved = self.unresolved.saturating_add(1);
                }
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                resolved.map(|ms| ms as u32)
            }
        };
        let key = name.to_ascii_lowercase();
        if let Some(&at) = self.by_name.get(&key) {
            if let Some(slot) = self.lists.get_mut(usize::from(at)) {
                *slot = list;
            }
            return;
        }
        let Ok(at) = u16::try_from(self.lists.len()) else {
            return;
        };
        self.lists.push(list);
        self.by_name.insert(key, at);
    }

    #[must_use]
    pub fn id(&self, name: &str) -> Option<ModifierId> {
        self.by_name
            .get(&name.to_ascii_lowercase())
            .copied()
            .map(ModifierId)
    }

    #[must_use]
    pub fn get(&self, id: ModifierId) -> Option<&ModifierList> {
        self.lists.get(usize::from(id.0))
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.lists.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lists.is_empty()
    }

    #[must_use]
    pub const fn unapplied(&self) -> usize {
        self.unapplied
    }

    #[must_use]
    pub const fn unresolved(&self) -> usize {
        self.unresolved
    }
}

fn value(raw: &str, macros: &MacroTable) -> Option<f32> {
    parse_percent_shaped(raw, macros).filter(|v| v.is_finite())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Attributes {
    pub damage_add: f32,
    pub damage_mult: f32,
    pub armor: f32,
    pub speed: f32,
    pub range: f32,
    pub experience: f32,
    pub production: f32,
    pub recharge: f32,
    pub spell_damage: f32,
    pub rate_of_fire: f32,
    pub shroud_clearing: f32,
    pub auto_heal: f32,
    pub command_points: f32,
    pub bounty: Option<f32>,
    pub invulnerable: u64,
    pub vision: f32,
    pub crush_decelerate: f32,
    pub crushed_decelerate: f32,
    pub minimum_crush_velocity: Option<f32>,
    pub crushable_level: f32,
    pub resist_fear: f32,
    pub resist_terror: f32,
    pub resist_knockback: f32,
}

impl Default for Attributes {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Attributes {
    pub const IDENTITY: Self = Self {
        damage_add: 0.0,
        damage_mult: 1.0,
        armor: 0.0,
        speed: 1.0,
        range: 0.0,
        experience: 1.0,
        production: 1.0,
        recharge: 1.0,
        spell_damage: 1.0,
        rate_of_fire: 1.0,
        shroud_clearing: 0.0,
        auto_heal: 0.0,
        command_points: 0.0,
        bounty: None,
        invulnerable: 0,
        vision: 0.0,
        crush_decelerate: 1.0,
        crushed_decelerate: 1.0,
        minimum_crush_velocity: None,
        crushable_level: 0.0,
        resist_fear: 0.0,
        resist_terror: 0.0,
        resist_knockback: 0.0,
    };

    pub fn fold(&mut self, list: &ModifierList) {
        for modifier in &list.modifiers {
            let v = modifier.value;
            match modifier.kind {
                AttributeKind::Armor => self.armor += v,
                AttributeKind::DamageAdd => self.damage_add += v,
                AttributeKind::DamageMult => self.damage_mult *= v,
                AttributeKind::Speed => self.speed *= v,
                AttributeKind::Range => self.range += v,
                AttributeKind::Experience => self.experience *= v,
                AttributeKind::Production => self.production *= v,
                AttributeKind::RechargeTime => self.recharge *= v,
                AttributeKind::SpellDamage => self.spell_damage *= v,
                AttributeKind::RateOfFire => self.rate_of_fire *= v,
                AttributeKind::ShroudClearing => self.shroud_clearing += v,
                AttributeKind::AutoHeal => self.auto_heal += v,
                AttributeKind::CommandPointBonus => self.command_points += v,
                AttributeKind::BountyPercentage => {
                    self.bounty = Some(self.bounty.map_or(v, |held| held * v));
                }
                AttributeKind::Invulnerable => self.invulnerable |= modifier.types,
                AttributeKind::Vision => self.vision += v,
                AttributeKind::CrushDecelerate => self.crush_decelerate *= v,
                AttributeKind::CrushedDecelerate => self.crushed_decelerate *= v,
                AttributeKind::MinimumCrushVelocity => {
                    self.minimum_crush_velocity =
                        Some(self.minimum_crush_velocity.map_or(v, |held| held * v));
                }
                AttributeKind::CrushableLevel => self.crushable_level += v,
                AttributeKind::ResistFear => self.resist_fear += v,
                AttributeKind::ResistTerror => self.resist_terror += v,
                AttributeKind::ResistKnockback => self.resist_knockback += v,
                AttributeKind::Health | AttributeKind::HealthMult | AttributeKind::Unapplied => {}
            }
        }
    }

    #[must_use]
    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }

    #[must_use]
    pub fn damage(&self, amount: f32) -> f32 {
        self.damage_of(amount, false, true)
    }

    #[must_use]
    pub fn damage_of(&self, amount: f32, magic: bool, accepts_add: bool) -> f32 {
        if amount.is_nan() || amount <= 0.0 {
            return amount;
        }
        let mult = if magic {
            self.spell_damage
        } else {
            self.damage_mult
        };
        let add = if accepts_add { self.damage_add } else { 0.0 };
        let out = (amount + add) * mult;
        if out.is_finite() {
            out.max(0.0)
        } else {
            amount
        }
    }

    #[must_use]
    pub fn shroud_range(&self, range: f32) -> f32 {
        let out = range * (1.0 + self.shroud_clearing);
        if out.is_finite() {
            out.max(0.0)
        } else {
            range
        }
    }

    #[must_use]
    pub fn crush_slow_factor(crusher: &Self, crushed: &Self) -> f32 {
        let factor = crusher.crush_decelerate * crushed.crushed_decelerate;
        if factor.is_finite() {
            factor.max(0.0)
        } else {
            1.0
        }
    }

    #[must_use]
    pub fn minimum_crush_velocity_factor(&self, min_velocity: f32) -> f32 {
        let Some(a) = self.minimum_crush_velocity else {
            return 1.0;
        };
        let m = min_velocity.clamp(0.05, 0.95);
        let f = (a * m).clamp(0.05, 0.95);
        let factor = (1.0 - m) / (1.0 - f);
        if factor.is_finite() {
            factor.max(0.0)
        } else {
            1.0
        }
    }

    #[must_use]
    pub fn armor_factor(&self, cap: f32) -> f32 {
        if !self.armor.is_finite() {
            return 1.0;
        }
        let protection = self.armor.min(cap);
        let factor = 1.0 - protection;
        if factor.is_finite() {
            factor.max(0.0)
        } else {
            1.0
        }
    }

    #[must_use]
    pub fn range(&self, range: f32) -> f32 {
        let out = range * (1.0 + self.range);
        if out.is_finite() {
            out.max(0.0)
        } else {
            range
        }
    }

    #[must_use]
    pub fn is_invulnerable_to(&self, damage_type: DamageTypeId) -> bool {
        let at = damage_type.index();
        at < 64 && self.invulnerable & (1u64 << at) != 0
    }

    #[must_use]
    pub fn crushable_level_bonus(&self) -> i16 {
        let sum = self.crushable_level;
        if !sum.is_finite() || sum.abs() > 1.0e4 {
            return 0;
        }
        #[allow(clippy::cast_possible_truncation)]
        let whole = sum.trunc() as i16;
        whole
    }

    #[must_use]
    pub fn command_point_bonus(&self) -> i32 {
        let sum = self.command_points;
        if !sum.is_finite() || sum.abs() > 1.0e6 {
            return 0;
        }
        #[allow(clippy::cast_possible_truncation)]
        let whole = sum.round() as i32;
        whole
    }

    pub fn hash_into(&self, hasher: &mut StateHasher) {
        for v in [
            self.damage_add,
            self.damage_mult,
            self.armor,
            self.speed,
            self.range,
            self.experience,
            self.production,
            self.recharge,
            self.spell_damage,
            self.rate_of_fire,
            self.shroud_clearing,
            self.auto_heal,
            self.command_points,
            self.vision,
            self.crush_decelerate,
            self.crushed_decelerate,
            self.crushable_level,
            self.resist_fear,
            self.resist_terror,
            self.resist_knockback,
        ] {
            hasher.write_f32(v);
        }
        match self.minimum_crush_velocity {
            Some(v) => {
                hasher.write_bool(true);
                hasher.write_f32(v);
            }
            None => hasher.write_bool(false),
        }
        match self.bounty {
            Some(v) => {
                hasher.write_bool(true);
                hasher.write_f32(v);
            }
            None => hasher.write_bool(false),
        }
        hasher.write_u64(self.invulnerable);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::float_cmp)]
mod tests {
    use super::*;
    use mint_sage_ini::{BlockTable, Document};

    fn store(text: &str) -> ModifierStore {
        store_with_types(text).0
    }

    fn store_with_types(text: &str) -> (ModifierStore, DamageTypes) {
        let doc =
            Document::parse("attributemodifier.ini", text, &BlockTable::corpus_path()).unwrap();
        assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
        let mut macros = MacroTable::new();
        macros.insert_defines(doc.defines());
        let mut store = ModifierStore::default();
        let mut types = DamageTypes::new();
        for block in doc.roots() {
            if block.keyword.eq_ignore_ascii_case(MODIFIER_LIST_KEYWORD) {
                store.load(block, &macros, &mut types);
            }
        }
        (store, types)
    }

    #[test]
    fn invulnerable_names_its_damage_types_inline_or_through_a_macro() {
        let (s, types) = store_with_types(
            "#define CAH_INVULNERABLE 0% FLAME MAGIC\n\
             ModifierList Snow\n  Modifier = INVULNERABLE 0% SLASH PIERCE\nEnd\n\
             ModifierList Hero\n  Modifier = INVULNERABLE CAH_INVULNERABLE\nEnd\n",
        );
        assert_eq!(s.unapplied(), 0);
        assert_eq!(s.unresolved(), 0);
        let fold = |name: &str| {
            let mut a = Attributes::IDENTITY;
            a.fold(s.get(s.id(name).unwrap()).unwrap());
            a
        };
        let snow = fold("Snow");
        let slash = types.lookup("SLASH").unwrap();
        let flame = types.lookup("FLAME").unwrap();
        assert!(snow.is_invulnerable_to(slash));
        assert!(snow.is_invulnerable_to(types.lookup("PIERCE").unwrap()));
        assert!(!snow.is_invulnerable_to(flame));
        let hero = fold("Hero");
        assert!(hero.is_invulnerable_to(flame));
        assert!(hero.is_invulnerable_to(types.lookup("MAGIC").unwrap()));
        assert!(!hero.is_invulnerable_to(slash));
        assert!(!Attributes::IDENTITY.is_invulnerable_to(slash));
    }

    #[test]
    fn heal_points_and_bounty_fold_as_the_header_says() {
        let s = store(
            "ModifierList Farm\n  Modifier = COMMAND_POINT_BONUS 25\n  \
             Modifier = COMMAND_POINT_BONUS 25\n  Modifier = AUTO_HEAL 50\nEnd\n\
             ModifierList Outlaw\n  Modifier = BOUNTY_PERCENTAGE 100.0%\n  \
             Modifier = BOUNTY_PERCENTAGE 50%\nEnd\n",
        );
        let mut a = Attributes::IDENTITY;
        a.fold(s.get(s.id("Farm").unwrap()).unwrap());
        assert_eq!(a.command_point_bonus(), 50);
        assert_eq!(a.auto_heal, 50.0);
        assert_eq!(a.bounty, None, "no list named a bounty");
        a.fold(s.get(s.id("Outlaw").unwrap()).unwrap());
        assert_eq!(a.bounty, Some(0.5));
        let corrupt = Attributes {
            command_points: f32::INFINITY,
            ..Attributes::IDENTITY
        };
        assert_eq!(corrupt.command_point_bonus(), 0);
    }

    #[test]
    fn a_level_list_resolves_its_macros_and_duration() {
        let s = store(
            "#define TROOP_HP 20\n#define TROOP_DAMAGE 10\n\
             ModifierList TroopRank2\n  Category = LEVEL\n  Modifier = HEALTH TROOP_HP\n  \
             Modifier = DAMAGE_ADD TROOP_DAMAGE\n  Duration = 0\nEnd\n",
        );
        let list = s.get(s.id("troopRANK2").unwrap()).unwrap();
        assert_eq!(list.category.as_deref(), Some("LEVEL"));
        assert!(list.is_permanent());
        assert_eq!(list.modifiers.len(), 2);
        assert_eq!(list.health_change(), (20.0, 1.0));
        let mut a = Attributes::IDENTITY;
        a.fold(list);
        assert_eq!(a.damage_add, 10.0);
        assert_eq!(a.damage(5.0), 15.0);
    }

    #[test]
    fn percentages_divide_and_a_trailing_percent_token_leaves_a_macro_whole() {
        let s = store(
            "#define MILL 1.25\n\
             ModifierList L\n  Modifier = ARMOR 50%\n  Modifier = PRODUCTION MILL %\n  \
             Modifier = DAMAGE_MULT 150%\n  Modifier = SPEED 60%\nEnd\n",
        );
        let list = s.get(s.id("L").unwrap()).unwrap();
        let mut a = Attributes::IDENTITY;
        a.fold(list);
        assert_eq!(a.armor, 0.5);
        assert_eq!(a.production, 1.25);
        assert_eq!(a.damage_mult, 1.5);
        assert_eq!(a.speed, 0.6);
    }

    #[test]
    fn damage_adds_before_it_multiplies() {
        let a = Attributes {
            damage_add: 10.0,
            damage_mult: 2.0,
            ..Attributes::IDENTITY
        };
        assert_eq!(a.damage(5.0), 30.0);
        assert_eq!(a.damage(0.0), 0.0, "nothing is not raised");
    }

    #[test]
    fn armour_is_capped_and_a_negative_sum_raises_damage() {
        let strong = Attributes {
            armor: 1.5,
            ..Attributes::IDENTITY
        };
        assert_eq!(strong.armor_factor(0.75), 0.25);
        let weak = Attributes {
            armor: -0.25,
            ..Attributes::IDENTITY
        };
        assert_eq!(weak.armor_factor(0.75), 1.25);
        assert_eq!(Attributes::IDENTITY.armor_factor(0.75), 1.0);
    }

    #[test]
    fn range_adds_fractions_as_the_header_says() {
        let a = Attributes {
            range: 0.3,
            ..Attributes::IDENTITY
        };
        assert!((a.range(100.0) - 130.0).abs() < 1e-3);
    }

    #[test]
    fn spell_damage_replaces_the_multiplier_on_magic_only() {
        let s = store(
            "ModifierList L\n  Modifier = DAMAGE_MULT 50%\n  Modifier = SPELL_DAMAGE 200%\n  \
             Modifier = RATE_OF_FIRE 200%\n  Modifier = SHROUD_CLEARING 160%\n  \
             Modifier = SHROUD_CLEARING 40%\nEnd\n",
        );
        assert_eq!(s.unapplied(), 0);
        let mut a = Attributes::IDENTITY;
        a.fold(s.get(s.id("L").unwrap()).unwrap());
        assert_eq!(a.damage_of(10.0, false, true), 5.0);
        assert_eq!(a.damage_of(10.0, true, true), 20.0);
        assert_eq!(a.rate_of_fire, 2.0);
        assert!((a.shroud_range(100.0) - 300.0).abs() < 1e-3);
        assert_eq!(Attributes::IDENTITY.damage_of(10.0, true, true), 10.0);
        a.damage_add = 4.0;
        assert_eq!(a.damage_of(10.0, false, true), 7.0);
        assert_eq!(
            a.damage_of(10.0, false, false),
            5.0,
            "AcceptDamageAdd = No skips the add"
        );
    }

    #[test]
    fn minimum_crush_velocity_scales_the_crush_deceleration() {
        let s = store(
            "ModifierList L
  Modifier = MINIMUM_CRUSH_VELOCITY 50%
End
",
        );
        assert_eq!(s.unapplied(), 0);
        let mut a = Attributes::IDENTITY;
        a.fold(s.get(s.id("L").unwrap()).unwrap());
        assert_eq!(a.minimum_crush_velocity, Some(0.5));
        assert!((a.minimum_crush_velocity_factor(0.5) - 2.0 / 3.0).abs() < 1e-6);
        assert!((a.minimum_crush_velocity_factor(0.0) - 1.0).abs() < 1e-6);
        assert_eq!(Attributes::IDENTITY.minimum_crush_velocity_factor(0.5), 1.0);
    }

    #[test]
    fn vision_and_crush_deceleration_fold_as_the_header_says() {
        let s = store(
            "ModifierList L\n  Modifier = VISION 50%\n  Modifier = CRUSH_DECELERATE 80%\n  \
             Modifier = CRUSHED_DECELERATE 90%\nEnd\n",
        );
        assert_eq!(s.unapplied(), 0);
        let mut a = Attributes::IDENTITY;
        a.fold(s.get(s.id("L").unwrap()).unwrap());
        assert_eq!(a.vision, 0.5);
        assert!(
            (a.shroud_range(100.0) - 100.0).abs() < 1e-3,
            "VISION does not widen the reveal"
        );
        assert_eq!(a.crush_decelerate, 0.8);
        assert_eq!(a.crushed_decelerate, 0.9);
        assert_eq!(
            Attributes::crush_slow_factor(&a, &Attributes::IDENTITY),
            0.8
        );
        assert_eq!(
            Attributes::crush_slow_factor(&Attributes::IDENTITY, &a),
            0.9
        );
        assert_eq!(
            Attributes::crush_slow_factor(&Attributes::IDENTITY, &Attributes::IDENTITY),
            1.0
        );
    }

    #[test]
    fn the_saving_throws_sum_across_lists() {
        let s = store(
            "ModifierList L\n  Modifier = RESIST_FEAR 20%\n  Modifier = RESIST_TERROR 100%\n  \
             Modifier = RESIST_KNOCKBACK 75%\nEnd\n",
        );
        assert_eq!(s.unapplied(), 0);
        let list = s.get(s.id("L").unwrap()).unwrap();
        let mut a = Attributes::IDENTITY;
        a.fold(list);
        a.fold(list);
        assert_eq!(
            (a.resist_fear, a.resist_terror, a.resist_knockback),
            (0.4, 2.0, 1.5)
        );
    }

    #[test]
    fn an_unknown_kind_is_counted_and_an_unresolved_value_dropped() {
        let s = store(
            "ModifierList L\n  Modifier = NO_SUCH_KIND 20%\n  Modifier = DAMAGE_ADD NO_SUCH_MACRO\n  \
             Duration = SOME_DELAY\nEnd\n",
        );
        assert_eq!(s.unapplied(), 1);
        assert_eq!(s.unresolved(), 2, "the damage add and the duration");
        let list = s.get(s.id("L").unwrap()).unwrap();
        assert_eq!(list.modifiers.len(), 1);
        assert!(!list.is_permanent());
    }

    #[test]
    fn a_redefinition_replaces_the_list_and_keeps_its_id() {
        let s = store(
            "ModifierList A\n  Modifier = HEALTH 1\nEnd\nModifierList B\nEnd\n\
             ModifierList a\n  Modifier = HEALTH 2\nEnd\n",
        );
        assert_eq!(s.len(), 2);
        let id = s.id("A").unwrap();
        assert_eq!(id.index(), 0);
        assert_eq!(s.get(id).unwrap().health_change(), (2.0, 1.0));
    }

    #[test]
    fn health_mult_multiplies_and_health_adds() {
        let s =
            store("ModifierList L\n  Modifier = HEALTH_MULT 75%\n  Modifier = HEALTH 100\nEnd\n");
        let list = s.get(s.id("L").unwrap()).unwrap();
        assert_eq!(list.health_change(), (100.0, 0.75));
    }

    #[test]
    fn a_hostile_value_is_refused_rather_than_propagated() {
        let a = Attributes {
            damage_add: f32::MAX,
            damage_mult: f32::MAX,
            ..Attributes::IDENTITY
        };
        assert_eq!(a.damage(5.0), 5.0, "an overflow keeps the base");
        let b = Attributes {
            armor: f32::NAN,
            ..Attributes::IDENTITY
        };
        assert_eq!(b.armor_factor(0.75), 1.0);
    }

    #[test]
    fn crushable_level_bonus_truncates_rather_than_rounds() {
        let up = Attributes {
            crushable_level: 1.9,
            ..Attributes::IDENTITY
        };
        assert_eq!(up.crushable_level_bonus(), 1);
        let down = Attributes {
            crushable_level: -1.9,
            ..Attributes::IDENTITY
        };
        assert_eq!(down.crushable_level_bonus(), -1);
    }
}
