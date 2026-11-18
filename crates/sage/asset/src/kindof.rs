#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KindOf(u32);

impl KindOf {
    pub const NONE: Self = Self(0);
    pub const SELECTABLE: Self = Self(1 << 0);
    pub const SHRUBBERY: Self = Self(1 << 1);
    pub const MINE: Self = Self(1 << 2);
    pub const FORCEATTACKABLE: Self = Self(1 << 3);
    pub const CLICK_THROUGH: Self = Self(1 << 4);
    pub const STRUCTURE: Self = Self(1 << 5);
    pub const ALWAYS_SELECTABLE: Self = Self(1 << 6);
    pub const IMMOBILE: Self = Self(1 << 7);
    pub const NO_COLLIDE: Self = Self(1 << 8);
    pub const DOZER: Self = Self(1 << 9);
    pub const UNATTACKABLE: Self = Self(1 << 10);
    pub const NOT_AUTOACQUIRABLE: Self = Self(1 << 11);
    pub const SCARY: Self = Self(1 << 12);
    pub const CANNOT_RETALIATE: Self = Self(1 << 13);
    pub const AUTO_RALLYPOINT: Self = Self(1 << 14);
    pub const SCALEABLE_WALL: Self = Self(1 << 15);
    pub const CAN_CLIMB_WALLS: Self = Self(1 << 16);
    pub const SPELL_BOOK: Self = Self(1 << 17);
    pub const HORDE: Self = Self(1 << 18);
    pub const MACHINE: Self = Self(1 << 19);
    pub const HERO: Self = Self(1 << 20);
    pub const SIEGE_TOWER: Self = Self(1 << 21);
    pub const MONSTER: Self = Self(1 << 22);
    pub const HAS_HEALTH_BAR: Self = Self(1 << 23);
    pub const INFANTRY: Self = Self(1 << 24);
    pub const CAVALRY: Self = Self(1 << 25);
    pub const ROCK_VENDOR: Self = Self(1 << 26);
    pub const IGNORED_IN_GUI: Self = Self(1 << 27);
    pub const DEFENSIVE_WALL: Self = Self(1 << 28);
    pub const PROJECTILE: Self = Self(1 << 29);
    pub const LINKED_TO_FLAG: Self = Self(1 << 30);

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn bits(self) -> u32 {
        self.0
    }

    pub fn parse(line: &str) -> (Self, usize) {
        let mut flags = Self::NONE;
        let mut unknown = 0usize;
        for token in line.split_whitespace() {
            let named = match token.to_ascii_uppercase().as_str() {
                "SELECTABLE" => Self::SELECTABLE,
                "SHRUBBERY" => Self::SHRUBBERY,
                "MINE" => Self::MINE,
                "FORCEATTACKABLE" => Self::FORCEATTACKABLE,
                "CLICK_THROUGH" => Self::CLICK_THROUGH,
                "STRUCTURE" => Self::STRUCTURE,
                "ALWAYS_SELECTABLE" => Self::ALWAYS_SELECTABLE,
                "IMMOBILE" => Self::IMMOBILE,
                "NO_COLLIDE" => Self::NO_COLLIDE,
                "DOZER" => Self::DOZER,
                "UNATTACKABLE" => Self::UNATTACKABLE,
                "NOT_AUTOACQUIRABLE" => Self::NOT_AUTOACQUIRABLE,
                "SCARY" => Self::SCARY,
                "CANNOT_RETALIATE" => Self::CANNOT_RETALIATE,
                "AUTO_RALLYPOINT" => Self::AUTO_RALLYPOINT,
                "SCALEABLE_WALL" => Self::SCALEABLE_WALL,
                "CAN_CLIMB_WALLS" => Self::CAN_CLIMB_WALLS,
                "SPELL_BOOK" => Self::SPELL_BOOK,
                "HORDE" => Self::HORDE,
                "MACHINE" => Self::MACHINE,
                "HERO" => Self::HERO,
                "SIEGE_TOWER" => Self::SIEGE_TOWER,
                "MONSTER" => Self::MONSTER,
                "HAS_HEALTH_BAR" => Self::HAS_HEALTH_BAR,
                "INFANTRY" => Self::INFANTRY,
                "CAVALRY" => Self::CAVALRY,
                "ROCK_VENDOR" => Self::ROCK_VENDOR,
                "IGNORED_IN_GUI" => Self::IGNORED_IN_GUI,
                "DEFENSIVE_WALL" => Self::DEFENSIVE_WALL,
                "PROJECTILE" => Self::PROJECTILE,
                "LINKED_TO_FLAG" => Self::LINKED_TO_FLAG,
                _ => {
                    unknown = unknown.saturating_add(1);
                    continue;
                }
            };
            flags = flags.union(named);
        }
        (flags, unknown)
    }

    pub const fn pick_type(self) -> PickType {
        let mut pick = PickType::None;
        if self.contains(Self::SELECTABLE) {
            pick = PickType::Selectable;
        }
        if self.contains(Self::SHRUBBERY) {
            pick = PickType::Shrubbery;
        }
        if self.contains(Self::MINE) {
            pick = PickType::Mine;
        }
        if self.contains(Self::FORCEATTACKABLE) {
            pick = PickType::ForceAttackable;
        }
        if self.contains(Self::CLICK_THROUGH) {
            pick = PickType::None;
        }
        pick
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PickType {
    #[default]
    None,
    Selectable,
    Shrubbery,
    Mine,
    ForceAttackable,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shipped_conflicting_pairs_resolve_the_way_the_engine_resolves_them() {
        let cases = [
            ("SELECTABLE SHRUBBERY", PickType::Shrubbery),
            ("SELECTABLE CLICK_THROUGH", PickType::None),
            ("SELECTABLE MINE", PickType::Mine),
        ];
        for (line, expected) in cases {
            let (flags, _) = KindOf::parse(line);
            assert_eq!(flags.pick_type(), expected, "{line}");
        }
    }

    #[test]
    fn the_order_the_tokens_are_written_in_does_not_change_the_answer() {
        let (a, _) = KindOf::parse("CLICK_THROUGH SELECTABLE");
        let (b, _) = KindOf::parse("SELECTABLE CLICK_THROUGH");
        assert_eq!(a.pick_type(), PickType::None);
        assert_eq!(a.pick_type(), b.pick_type());
        let (c, _) = KindOf::parse("SHRUBBERY SELECTABLE");
        assert_eq!(c.pick_type(), PickType::Shrubbery);
    }

    #[test]
    fn an_ordinary_unit_line_is_selectable_and_a_prop_line_is_not() {
        let (unit, _) = KindOf::parse("SELECTABLE INFANTRY SCORE CAN_ATTACK");
        assert_eq!(unit.pick_type(), PickType::Selectable);
        let (prop, _) = KindOf::parse("SHRUBBERY OPTIMIZED_TREE");
        assert_eq!(prop.pick_type(), PickType::Shrubbery);
        let (nothing, _) = KindOf::parse("PROJECTILE UNATTACKABLE");
        assert_eq!(nothing.pick_type(), PickType::None);
    }

    #[test]
    fn tokens_this_engine_does_not_model_are_counted_rather_than_ignored() {
        let (flags, unknown) = KindOf::parse("SELECTABLE INFANTRY SCORE");
        assert_eq!(flags, KindOf::SELECTABLE.union(KindOf::INFANTRY));
        assert_eq!(unknown, 1);
        let (empty, none) = KindOf::parse("");
        assert_eq!(empty, KindOf::NONE);
        assert_eq!(none, 0);
    }

    #[test]
    fn flag_names_are_matched_without_regard_to_case() {
        let (lower, _) = KindOf::parse("selectable structure");
        assert!(lower.contains(KindOf::SELECTABLE));
        assert!(lower.contains(KindOf::STRUCTURE));
    }

    #[test]
    fn immobile_is_read_and_does_not_change_which_pick_bit_an_object_gets() {
        let (flags, unknown) = KindOf::parse("STRUCTURE IMMOBILE SELECTABLE");
        assert!(flags.contains(KindOf::IMMOBILE));
        assert!(flags.contains(KindOf::STRUCTURE));
        assert_eq!(unknown, 0);
        assert_eq!(
            flags.pick_type(),
            KindOf::parse("STRUCTURE SELECTABLE").0.pick_type()
        );
        assert!(!KindOf::parse("STRUCTURE").0.contains(KindOf::IMMOBILE));
    }

    #[test]
    fn not_autoacquirable_is_read_and_does_not_change_which_pick_bit_an_object_gets() {
        let (flags, unknown) = KindOf::parse("STRUCTURE NOT_AUTOACQUIRABLE SELECTABLE");
        assert!(flags.contains(KindOf::NOT_AUTOACQUIRABLE));
        assert_eq!(unknown, 0);
        assert_eq!(
            flags.pick_type(),
            KindOf::parse("STRUCTURE SELECTABLE").0.pick_type()
        );
        assert!(!KindOf::parse("STRUCTURE")
            .0
            .contains(KindOf::NOT_AUTOACQUIRABLE));
    }

    #[test]
    fn no_collide_is_read_and_is_independent_of_structure_and_of_the_pick_bit() {
        let (flags, unknown) = KindOf::parse("STRUCTURE IMMOBILE NO_COLLIDE SELECTABLE");
        assert!(flags.contains(KindOf::NO_COLLIDE));
        assert!(flags.contains(KindOf::STRUCTURE));
        assert_eq!(unknown, 0);
        assert_eq!(
            flags.pick_type(),
            KindOf::parse("STRUCTURE IMMOBILE SELECTABLE").0.pick_type()
        );
        assert!(!KindOf::parse("STRUCTURE IMMOBILE")
            .0
            .contains(KindOf::NO_COLLIDE));
    }
}
