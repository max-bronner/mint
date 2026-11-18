use mint_sage_ini::value::resolve_percent;
use mint_sage_ini::{Field, MacroTable};
use mint_sim::StateHasher;

pub const CONDITION_NAMES: [&str; 22] = [
    "GARRISONED",
    "HORDE",
    "CONTINUOUS_FIRE_MEAN",
    "CONTINUOUS_FIRE_FAST",
    "NATIONALISM",
    "PLAYER_UPGRADE",
    "DRONE_SPOTTING",
    "DEMORALIZED_OBSOLETE",
    "ENTHUSIASTIC",
    "VETERAN",
    "ELITE",
    "HERO",
    "BATTLEPLAN_BOMBARDMENT",
    "BATTLEPLAN_HOLDTHELINE",
    "BATTLEPLAN_SEARCHANDDESTROY",
    "SUBLIMINAL",
    "SOLO_HUMAN_EASY",
    "SOLO_HUMAN_NORMAL",
    "SOLO_HUMAN_HARD",
    "SOLO_AI_EASY",
    "SOLO_AI_NORMAL",
    "SOLO_AI_HARD",
];

pub const FIELD_NAMES: [&str; 6] = [
    "DAMAGE",
    "RADIUS",
    "RANGE",
    "RATE_OF_FIRE",
    "PRE_ATTACK",
    "FIRING",
];

const RANGE: usize = 2;
const RATE_OF_FIRE: usize = 3;
const PRE_ATTACK: usize = 4;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct BonusConditions(pub u32);

impl BonusConditions {
    pub const VETERAN: u32 = 1 << 9;
    pub const ELITE: u32 = 1 << 10;
    pub const HERO: u32 = 1 << 11;

    #[must_use]
    pub const fn after_level_change(self, level: usize) -> Self {
        let rung = match level {
            0 => 0,
            1 => Self::VETERAN,
            2 => Self::ELITE,
            3 => Self::HERO,
            _ => return self,
        };
        Self(self.0 & !(Self::VETERAN | Self::ELITE | Self::HERO) | rung)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WeaponBonus([f32; 6]);

impl Default for WeaponBonus {
    fn default() -> Self {
        Self([1.0; 6])
    }
}

impl WeaponBonus {
    #[must_use]
    pub const fn range(&self) -> f32 {
        self.0[RANGE]
    }

    #[must_use]
    pub const fn rate_of_fire(&self) -> f32 {
        self.0[RATE_OF_FIRE]
    }

    #[must_use]
    pub const fn pre_attack(&self) -> f32 {
        self.0[PRE_ATTACK]
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct WeaponBonusSet(Vec<(u8, [f32; 6])>);

impl WeaponBonusSet {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn read_line(&mut self, field: &Field, macros: &MacroTable) -> Result<(), &'static str> {
        let [condition, name, value @ ..] = field.args.as_slice() else {
            return Err("a condition, a bonus field and a percentage");
        };
        let condition = CONDITION_NAMES
            .iter()
            .position(|known| known.eq_ignore_ascii_case(condition))
            .and_then(|index| u8::try_from(index).ok())
            .ok_or("a weapon bonus condition")?;
        let slot = FIELD_NAMES
            .iter()
            .position(|known| known.eq_ignore_ascii_case(name))
            .ok_or("a weapon bonus field")?;
        let value = resolve_percent(value, macros)
            .filter(|v| v.is_finite())
            .ok_or("a percentage")?;
        let at = match self.0.binary_search_by_key(&condition, |(c, _)| *c) {
            Ok(at) => at,
            Err(at) => {
                self.0.insert(at, (condition, [1.0; 6]));
                at
            }
        };
        if let Some(entry) = self.0.get_mut(at).and_then(|(_, row)| row.get_mut(slot)) {
            *entry = value;
        }
        Ok(())
    }

    pub fn append(&self, conditions: BonusConditions, bonus: &mut WeaponBonus) {
        for (condition, row) in &self.0 {
            if conditions.0 & 1u32.checked_shl(u32::from(*condition)).unwrap_or(0) == 0 {
                continue;
            }
            for (sum, value) in bonus.0.iter_mut().zip(row) {
                *sum += value - 1.0;
            }
        }
    }

    pub fn hash_into(&self, hasher: &mut StateHasher) {
        hasher.write_usize(self.0.len());
        for (condition, row) in &self.0 {
            hasher.write_u8(*condition);
            for value in row {
                hasher.write_f32(*value);
            }
        }
    }
}

#[must_use]
pub fn compute_bonus(
    global: &WeaponBonusSet,
    weapon: &WeaponBonusSet,
    conditions: BonusConditions,
) -> WeaponBonus {
    let mut bonus = WeaponBonus::default();
    if conditions.0 != 0 {
        global.append(conditions, &mut bonus);
        weapon.append(conditions, &mut bonus);
    }
    bonus
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(args: &[&str]) -> Field {
        Field {
            keyword: "WeaponBonus".into(),
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
            file: "test.ini".into(),
            line: 1,
        }
    }

    fn set(lines: &[&[&str]]) -> WeaponBonusSet {
        let mut set = WeaponBonusSet::default();
        for args in lines {
            set.read_line(&line(args), &MacroTable::default()).unwrap();
        }
        set
    }

    #[test]
    fn raised_rows_add_their_excess_over_one_across_the_global_and_weapon_sets() {
        let global = set(&[
            &["VETERAN", "RATE_OF_FIRE", "150%"],
            &["HERO", "RANGE", "200%"],
        ]);
        let weapon = set(&[
            &["ELITE", "RATE_OF_FIRE", "125%"],
            &["ELITE", "PRE_ATTACK", "50%"],
        ]);
        let bonus = compute_bonus(
            &global,
            &weapon,
            BonusConditions(BonusConditions::VETERAN | BonusConditions::ELITE),
        );
        assert_eq!(
            (bonus.rate_of_fire(), bonus.range(), bonus.pre_attack()),
            (1.75, 1.0, 0.5)
        );
    }

    #[test]
    fn an_unknown_condition_or_field_is_refused() {
        let mut set = WeaponBonusSet::default();
        let macros = MacroTable::default();
        assert!(set
            .read_line(&line(&["FRENZY_ONE", "DAMAGE", "120%"]), &macros)
            .is_err());
        assert!(set
            .read_line(&line(&["HERO", "ARMOR", "120%"]), &macros)
            .is_err());
    }

    #[test]
    fn veterancy_above_the_third_level_keeps_the_rung_it_had() {
        let elite = BonusConditions(1).after_level_change(2);
        assert_eq!(elite.after_level_change(5).0, 1 | BonusConditions::ELITE);
    }
}
