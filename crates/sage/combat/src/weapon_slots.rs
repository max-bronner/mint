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
    pub damage: f32,
    pub unresistable: bool,
    pub ready: bool,
    pub inside_minimum: bool,
    pub out_of_ammo: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlotCandidate {
    pub arm: SlotArm,
    pub fitness: SlotFitness,
}

const HUGE_DAMAGE: f32 = 1e10;

#[must_use]
pub fn choose_slot(
    slots: &[Option<SlotCandidate>; SLOT_COUNT],
    victim_kinds: ConditionMask,
    source: CommandSource,
    active_conditions: ConditionMask,
) -> Option<WeaponSlot> {
    let mut best: Option<(f32, usize)> = None;
    let mut backup: Option<(f32, usize)> = None;
    for index in (0..SLOT_COUNT).rev() {
        let Some(Some(candidate)) = slots.get(index) else {
            continue;
        };
        if !candidate.arm.sources.allows(source) {
            continue;
        }
        if candidate.fitness.out_of_ammo {
            continue;
        }
        if !candidate.fitness.class_allowed {
            continue;
        }
        if !active_conditions.contains(candidate.arm.only_in_condition) {
            continue;
        }
        let matches = |mask: ConditionMask| {
            mask != ConditionMask::NONE && victim_kinds.intersection_count(mask) > 0
        };
        if candidate.arm.only_against != ConditionMask::NONE && !matches(candidate.arm.only_against)
        {
            continue;
        }
        let preferred =
            matches(candidate.arm.preferred_against) || matches(candidate.arm.only_against);
        let hurts = candidate.fitness.damage > 0.0;
        if !preferred && !hurts && !candidate.fitness.unresistable {
            continue;
        }
        let (score, ready) = if preferred {
            (HUGE_DAMAGE, !candidate.fitness.inside_minimum)
        } else {
            (if hurts { 1.0 } else { 0.0 }, candidate.fitness.ready)
        };
        let slate = if ready { &mut best } else { &mut backup };
        if slate.is_none_or(|(best_score, _)| score >= best_score) {
            *slate = Some((score, index));
        }
    }
    let chosen = best.or(backup).map(|(_, index)| index)?;
    WeaponSlot::from_index(chosen)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeaponChoiceCriteria {
    PreferMostDamage,
    PreferLongestDamage,
    PreferGrabOverDamage,
    PreferLeastMovement,
    SelectAtRandom,
    UseWeaponSetDefaultCriteria,
}

impl WeaponChoiceCriteria {
    #[must_use]
    pub fn parse(token: &str) -> Option<Self> {
        if token.eq_ignore_ascii_case("PREFER_MOST_DAMAGE") {
            Some(Self::PreferMostDamage)
        } else if token.eq_ignore_ascii_case("PREFER_LONGEST_DAMAGE") {
            Some(Self::PreferLongestDamage)
        } else if token.eq_ignore_ascii_case("PREFER_GRAB_OVER_DAMAGE") {
            Some(Self::PreferGrabOverDamage)
        } else if token.eq_ignore_ascii_case("PREFER_LEAST_MOVEMENT") {
            Some(Self::PreferLeastMovement)
        } else if token.eq_ignore_ascii_case("SELECT_AT_RANDOM") {
            Some(Self::SelectAtRandom)
        } else if token.eq_ignore_ascii_case("USE_WEAPON_SET_DEFAULT_CRITERIA") {
            Some(Self::UseWeaponSetDefaultCriteria)
        } else {
            None
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests;
