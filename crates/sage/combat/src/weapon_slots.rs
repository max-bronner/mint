use crate::match_set::ConditionMask;
use crate::weapon::WeaponId;

pub const SLOT_COUNT: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WeaponSlot {
    Primary,
    Secondary,
    Tertiary,
    Quaternary,
    Quinary,
}

impl WeaponSlot {
    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        if token.eq_ignore_ascii_case("PRIMARY") {
            Some(Self::Primary)
        } else if token.eq_ignore_ascii_case("SECONDARY") {
            Some(Self::Secondary)
        } else if token.eq_ignore_ascii_case("TERTIARY") {
            Some(Self::Tertiary)
        } else if token.eq_ignore_ascii_case("QUATERNARY") {
            Some(Self::Quaternary)
        } else if token.eq_ignore_ascii_case("QUINARY") {
            Some(Self::Quinary)
        } else {
            None
        }
    }

    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Primary => 0,
            Self::Secondary => 1,
            Self::Tertiary => 2,
            Self::Quaternary => 3,
            Self::Quinary => 4,
        }
    }

    #[must_use]
    pub const fn from_index(index: usize) -> Option<Self> {
        match index {
            0 => Some(Self::Primary),
            1 => Some(Self::Secondary),
            2 => Some(Self::Tertiary),
            3 => Some(Self::Quaternary),
            4 => Some(Self::Quinary),
            _ => None,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Primary => "PRIMARY",
            Self::Secondary => "SECONDARY",
            Self::Tertiary => "TERTIARY",
            Self::Quaternary => "QUATERNARY",
            Self::Quinary => "QUINARY",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandSource {
    Player,
    Script,
    Ai,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceMask(u8);

impl Default for SourceMask {
    fn default() -> Self {
        Self::ANY
    }
}

impl SourceMask {
    pub const FROM_PLAYER: Self = Self(1 << 0);
    pub const FROM_SCRIPT: Self = Self(1 << 1);
    pub const FROM_AI: Self = Self(1 << 2);
    pub const FROM_DOZER: Self = Self(1 << 3);
    pub const DEFAULT_SWITCH_WEAPON: Self = Self(1 << 4);
    pub const NONE: Self = Self(0);
    pub const ANY: Self = Self(0xff);

    pub fn parse<S: AsRef<str>>(tokens: &[S]) -> Result<Self, String> {
        let mut mask = Self::NONE;
        for token in tokens {
            let token = token.as_ref();
            if token.eq_ignore_ascii_case("NONE") {
                return Ok(Self::NONE);
            }
            let bit = if token.eq_ignore_ascii_case("FROM_PLAYER") {
                Self::FROM_PLAYER
            } else if token.eq_ignore_ascii_case("FROM_SCRIPT") {
                Self::FROM_SCRIPT
            } else if token.eq_ignore_ascii_case("FROM_AI") {
                Self::FROM_AI
            } else if token.eq_ignore_ascii_case("FROM_DOZER") {
                Self::FROM_DOZER
            } else if token.eq_ignore_ascii_case("DEFAULT_SWITCH_WEAPON") {
                Self::DEFAULT_SWITCH_WEAPON
            } else {
                return Err(token.to_owned());
            };
            mask = Self(mask.0 | bit.0);
        }
        Ok(mask)
    }

    #[must_use]
    pub const fn allows(self, source: CommandSource) -> bool {
        let bit = match source {
            CommandSource::Player => Self::FROM_PLAYER.0,
            CommandSource::Script => Self::FROM_SCRIPT.0,
            CommandSource::Ai => Self::FROM_AI.0,
        };
        self.0 & bit != 0 || self.0 & Self::DEFAULT_SWITCH_WEAPON.0 != 0
    }

    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlotArm {
    pub weapon: WeaponId,
    pub sources: SourceMask,
    pub preferred_against: ConditionMask,
    pub only_against: ConditionMask,
    pub only_in_condition: ConditionMask,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlotFitness {
    pub class_allowed: bool,
    pub valid: bool,
    pub unresistable: bool,
    pub ready_status: bool,
    pub out_of_ammo: bool,
    pub range: f32,
    pub distance: f32,
    pub minimum: f32,
    pub melee: bool,
    pub grab: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlotCandidate {
    pub arm: SlotArm,
    pub fitness: SlotFitness,
}

const HUGE_DAMAGE: f32 = 1e10;
const ONLY_AGAINST_DAMAGE: f32 = 1e5;
const HUGE_MOVE: f32 = 1e10;

#[derive(Clone, Copy)]
struct Score {
    damage: f32,
    range: f32,
    movement: f32,
    draw: f32,
    ready: bool,
}

#[derive(Clone, Copy)]
struct Best {
    value: f32,
    index: Option<usize>,
}

impl Score {
    fn hit(&mut self, value: f32, usable: bool) {
        *self = Self {
            damage: value,
            range: value,
            movement: 0.0,
            draw: self.draw + 1.0,
            ready: usable,
        };
    }

    fn miss(&mut self) {
        *self = Self {
            damage: -1.0,
            range: -1.0,
            movement: HUGE_MOVE,
            draw: self.draw - 1.0,
            ready: false,
        };
    }
}

#[must_use]
pub fn choose_slot(
    slots: &[Option<SlotCandidate>; SLOT_COUNT],
    victim_kinds: ConditionMask,
    source: CommandSource,
    active_conditions: ConditionMask,
    criteria: WeaponChoiceCriteria,
    draw: &mut dyn FnMut() -> f32,
) -> Option<WeaponSlot> {
    let initial = if criteria == WeaponChoiceCriteria::PreferLeastMovement {
        HUGE_MOVE
    } else {
        0.0
    };
    let mut ready_best = Best {
        value: initial,
        index: None,
    };
    let mut backup_best = ready_best;
    let matches = |mask: ConditionMask| {
        mask != ConditionMask::NONE && victim_kinds.intersection_count(mask) > 0
    };
    for index in (0..SLOT_COUNT).rev() {
        let Some(Some(candidate)) = slots.get(index) else {
            continue;
        };
        let fitness = &candidate.fitness;
        if !candidate.arm.sources.allows(source)
            || fitness.out_of_ammo
            || !fitness.class_allowed
            || (!fitness.valid && !fitness.unresistable)
        {
            continue;
        }
        let movement = if fitness.melee {
            fitness.distance
        } else if fitness.distance < fitness.minimum {
            fitness.minimum - fitness.distance
        } else if fitness.distance > fitness.range {
            fitness.distance - fitness.range
        } else {
            0.0
        };
        let mut score = Score {
            damage: if fitness.valid { 1.0 } else { 0.0 },
            range: fitness.range,
            movement,
            draw: draw(),
            ready: fitness.ready_status && fitness.distance >= fitness.minimum,
        };
        let usable = fitness.distance >= fitness.minimum;
        let arm = &candidate.arm;
        if arm.only_against != ConditionMask::NONE {
            if matches(arm.only_against) {
                score.hit(ONLY_AGAINST_DAMAGE, usable);
            } else {
                score.miss();
            }
        }
        if matches(arm.preferred_against) {
            score.hit(HUGE_DAMAGE, usable);
        }
        if arm.only_in_condition != ConditionMask::NONE {
            if active_conditions.contains(arm.only_in_condition) {
                score.hit(HUGE_DAMAGE, usable);
            } else {
                score.miss();
            }
        }
        let (value, better): (f32, fn(f32, f32) -> bool) = match criteria {
            WeaponChoiceCriteria::PreferGrabOverDamage if fitness.grab => {
                return WeaponSlot::from_index(index);
            }
            WeaponChoiceCriteria::PreferMostDamage | WeaponChoiceCriteria::PreferGrabOverDamage => {
                (score.damage, |new, best| new >= best)
            }
            WeaponChoiceCriteria::PreferLongestRange => (score.range, |new, best| new > best),
            WeaponChoiceCriteria::PreferLeastMovement => (score.movement, |new, best| new <= best),
            WeaponChoiceCriteria::SelectAtRandom => (score.draw, |new, best| new > best),
            WeaponChoiceCriteria::UseWeaponSetDefaultCriteria => return None,
        };
        let best = if score.ready {
            &mut ready_best
        } else {
            &mut backup_best
        };
        if better(value, best.value) {
            *best = Best {
                value,
                index: Some(index),
            };
        }
    }
    ready_best
        .index
        .or(backup_best.index)
        .and_then(WeaponSlot::from_index)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoVictimSlot {
    pub out_of_ammo: bool,
    pub melee: bool,
    pub no_victim_needed: bool,
}

#[must_use]
pub fn choose_slot_without_victim(
    slots: &[Option<NoVictimSlot>; SLOT_COUNT],
) -> Option<WeaponSlot> {
    slots
        .iter()
        .position(|slot| {
            slot.is_some_and(|slot| !slot.out_of_ammo && !slot.melee && slot.no_victim_needed)
        })
        .and_then(WeaponSlot::from_index)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaponChoiceCriteria {
    PreferMostDamage,
    PreferLongestRange,
    PreferGrabOverDamage,
    PreferLeastMovement,
    SelectAtRandom,
    UseWeaponSetDefaultCriteria,
}

impl WeaponChoiceCriteria {
    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        const NAMES: [(&str, WeaponChoiceCriteria); 6] = [
            ("PREFER_MOST_DAMAGE", WeaponChoiceCriteria::PreferMostDamage),
            (
                "PREFER_LONGEST_RANGE",
                WeaponChoiceCriteria::PreferLongestRange,
            ),
            (
                "PREFER_GRAB_OVER_DAMAGE",
                WeaponChoiceCriteria::PreferGrabOverDamage,
            ),
            (
                "PREFER_LEAST_MOVEMENT",
                WeaponChoiceCriteria::PreferLeastMovement,
            ),
            ("SELECT_AT_RANDOM", WeaponChoiceCriteria::SelectAtRandom),
            (
                "USE_WEAPONSET_DEFAULT_CRITERIA",
                WeaponChoiceCriteria::UseWeaponSetDefaultCriteria,
            ),
        ];
        NAMES
            .iter()
            .find(|(name, _)| token.eq_ignore_ascii_case(name))
            .map(|(_, criteria)| *criteria)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
