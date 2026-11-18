use std::collections::BTreeMap;

use crate::chain::Chain;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum UnitType {
    Soldier,
    Archer,
    Pikemen,
    Cavalry,
    Monster,
    Hero,
    Fortress,
    Siege,
    Support,
    Invalid,
}

impl UnitType {
    pub const ALL: [Self; 10] = [
        Self::Soldier,
        Self::Archer,
        Self::Pikemen,
        Self::Cavalry,
        Self::Monster,
        Self::Hero,
        Self::Fortress,
        Self::Siege,
        Self::Support,
        Self::Invalid,
    ];

    #[must_use]
    pub fn index(self) -> usize {
        self as usize
    }

    pub(crate) fn index_u8(self) -> u8 {
        self as u8
    }

    #[must_use]
    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }
}

pub(crate) fn floor<V: Copy>(map: &BTreeMap<i32, V>, key: i32) -> Option<V> {
    map.range(..=key).next_back().map(|(_, v)| *v)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Weapon {
    pub damage: [f32; 10],
    pub level_bonus: BTreeMap<i32, f32>,
    pub miss_percent: i32,
    pub reduce_attack_when_hurt: bool,
}

impl Default for Weapon {
    fn default() -> Self {
        Self {
            damage: [0.0; 10],
            level_bonus: BTreeMap::new(),
            miss_percent: 50,
            reduce_attack_when_hurt: true,
        }
    }
}

impl Weapon {
    #[must_use]
    pub fn can_attack(&self) -> bool {
        self.miss_percent < 100 && self.damage.iter().any(|d| *d > 0.0)
    }

    #[must_use]
    pub fn damage_against(&self, level: i32, hp: f32, max_hp: f32, target: UnitType) -> f32 {
        if !self.can_attack() {
            return 0.0;
        }
        let mut d = self.damage.get(target.index()).copied().unwrap_or(0.0);
        if self.reduce_attack_when_hurt && max_hp > 0.0 {
            d *= hp / max_hp;
        }
        if let Some(bonus) = floor(&self.level_bonus, level) {
            d *= bonus + 1.0;
        }
        d
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Armor {
    pub taken: [f32; 10],
}

impl Default for Armor {
    fn default() -> Self {
        Self { taken: [1.0; 10] }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Body {
    pub hitpoints: BTreeMap<i32, f32>,
    pub regenerate: BTreeMap<i32, f32>,
    pub can_be_attacked: bool,
    pub leave_in_army_summary: bool,
}

impl Default for Body {
    fn default() -> Self {
        Self {
            hitpoints: BTreeMap::new(),
            regenerate: BTreeMap::new(),
            can_be_attacked: true,
            leave_in_army_summary: false,
        }
    }
}

impl Body {
    pub(crate) fn hitpoints_at(&self, level: i32) -> f32 {
        floor(&self.hitpoints, level).unwrap_or(1.0)
    }

    pub(crate) fn regeneration_at(&self, level: i32) -> f32 {
        floor(&self.regenerate, level).unwrap_or(0.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LeadershipBonus {
    pub maximum_units_affected: i32,
    pub priority: i32,
    pub weapon: f32,
    pub armor: f32,
    pub experience: f32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Leadership {
    pub affects: Vec<UnitType>,
    pub affects_higher_level_first: bool,
    pub bonuses: BTreeMap<i32, LeadershipBonus>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gated {
    pub required: Vec<String>,
    pub excluded: Vec<String>,
    pub name: String,
}

impl Gated {
    pub(crate) fn passes(&self, upgrades: &[String]) -> bool {
        let has = |name: &String| upgrades.iter().any(|u| u.eq_ignore_ascii_case(name));
        self.required.iter().all(has) && !self.excluded.iter().any(has)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExperienceLevel {
    pub required: f32,
    pub award: i32,
    pub rank: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    pub unit_type: UnitType,
    pub weapons: Vec<Gated>,
    pub armors: Vec<Gated>,
    pub body: Option<String>,
    pub leadership: Option<String>,
    pub chain: Option<String>,
    pub trainable: bool,
    pub horde: bool,
    pub structure: bool,
    pub hero: bool,
    pub initial_payload: Vec<(String, i32)>,
    pub levels: Vec<ExperienceLevel>,
}

impl Template {
    pub(crate) fn award_at(&self, experience: i32) -> i32 {
        let mut best: Option<&ExperienceLevel> = None;
        for level in &self.levels {
            if level.required <= experience as f32
                && best.is_none_or(|b| level.required > b.required)
            {
                best = Some(level);
            }
        }
        best.map_or(0, |l| l.award)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Schedule {
    pub rounds: BTreeMap<u32, i32>,
    pub each_remaining: i32,
}

impl Schedule {
    #[must_use]
    pub fn round_of(&self, k: u32) -> i32 {
        let key = k.saturating_add(1);
        let Some((&listed, &round)) = self.rounds.range(..=key).next_back() else {
            return 0;
        };
        let past = i32::try_from(key.saturating_sub(listed)).unwrap_or(i32::MAX);
        round.saturating_add(past.saturating_mul(self.each_remaining))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScienceStep {
    pub min_points: i32,
    pub weapon: f32,
    pub armor: f32,
}

#[derive(Debug, Clone, Default)]
pub struct Tables {
    pub weapons: BTreeMap<String, Weapon>,
    pub armors: BTreeMap<String, Armor>,
    pub bodies: BTreeMap<String, Body>,
    pub chains: BTreeMap<String, Chain>,
    pub leaderships: BTreeMap<String, Leadership>,
    pub default_weapon: Weapon,
    pub default_armor: Armor,
    pub default_body: Body,
    pub default_chain: Chain,
    pub templates: BTreeMap<String, Template>,
    pub schedules: [Schedule; 2],
}

impl Tables {
    pub(crate) fn template(&self, name: &str) -> Option<&Template> {
        self.templates.get(&name.to_ascii_lowercase())
    }
}
