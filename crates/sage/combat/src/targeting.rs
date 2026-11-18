use mint_sage_asset::KindOf;

use crate::victim::Relation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    AutoAcquire,
    Order,
    Ability,
    Splash,
    Force,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attacker {
    pub relation: Relation,
    pub class_allowed: bool,
    pub scans_structures: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Victim {
    pub live: bool,
    pub concealed: bool,
    pub contained: bool,
    pub kind_of: KindOf,
    pub is_structure: bool,
    pub seen: bool,
    pub script_targetable: bool,
}

#[must_use]
pub fn can_target(attacker: Attacker, victim: Victim, mode: Mode) -> bool {
    if !victim.live || victim.concealed || victim.contained || !victim.seen {
        return false;
    }
    if victim.kind_of.contains(KindOf::UNATTACKABLE) {
        return false;
    }
    if victim.kind_of.contains(KindOf::NOT_AUTOACQUIRABLE)
        && matches!(mode, Mode::AutoAcquire | Mode::Ability)
    {
        return false;
    }
    match mode {
        Mode::Splash | Mode::Force => {}
        Mode::Order => {
            let linked_neutral = matches!(attacker.relation, Relation::Neutral)
                && victim.kind_of.contains(KindOf::LINKED_TO_FLAG);
            if !matches!(attacker.relation, Relation::Enemies)
                && !linked_neutral
                && !victim.script_targetable
            {
                return false;
            }
        }
        Mode::AutoAcquire | Mode::Ability => {
            if !matches!(attacker.relation, Relation::Enemies) {
                return false;
            }
        }
    }
    if !attacker.class_allowed {
        return false;
    }
    if victim.is_structure && matches!(mode, Mode::AutoAcquire) && !attacker.scans_structures {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn victim(kind_of: KindOf) -> Victim {
        Victim {
            live: true,
            concealed: false,
            contained: false,
            kind_of,
            is_structure: false,
            seen: true,
            script_targetable: false,
        }
    }

    fn attacker(relation: Relation) -> Attacker {
        Attacker {
            relation,
            class_allowed: true,
            scans_structures: false,
        }
    }

    #[test]
    fn a_victim_with_no_live_visible_body_is_never_a_target() {
        let hostile = attacker(Relation::Enemies);
        for mode in [
            Mode::AutoAcquire,
            Mode::Order,
            Mode::Ability,
            Mode::Splash,
            Mode::Force,
        ] {
            let mut dead = victim(KindOf::NONE);
            dead.live = false;
            assert!(!can_target(hostile, dead, mode), "{mode:?} dead");
            let mut concealed = victim(KindOf::NONE);
            concealed.concealed = true;
            assert!(!can_target(hostile, concealed, mode), "{mode:?} concealed");
            let mut contained = victim(KindOf::NONE);
            contained.contained = true;
            assert!(!can_target(hostile, contained, mode), "{mode:?} contained");
            let mut unseen = victim(KindOf::NONE);
            unseen.seen = false;
            assert!(!can_target(hostile, unseen, mode), "{mode:?} unseen");
        }
    }

    #[test]
    fn an_unattackable_object_is_never_a_target() {
        let hostile = attacker(Relation::Enemies);
        let target = victim(KindOf::UNATTACKABLE);
        for mode in [
            Mode::AutoAcquire,
            Mode::Order,
            Mode::Ability,
            Mode::Splash,
            Mode::Force,
        ] {
            assert!(!can_target(hostile, target, mode), "{mode:?}");
        }
    }

    #[test]
    fn a_not_autoacquirable_structure_is_ignored_by_a_scan_but_may_be_ordered() {
        let mut hostile = attacker(Relation::Enemies);
        hostile.scans_structures = true;
        let mut target = victim(KindOf::NOT_AUTOACQUIRABLE);
        target.is_structure = true;
        assert!(!can_target(hostile, target, Mode::AutoAcquire));
        assert!(!can_target(hostile, target, Mode::Ability));
        assert!(can_target(hostile, target, Mode::Order));
    }

    #[test]
    fn a_player_order_admits_a_neutral_only_if_linked_to_a_flag_or_targetable() {
        let linked = victim(KindOf::LINKED_TO_FLAG);
        let plain = victim(KindOf::NONE);
        let mut targetable = plain;
        targetable.script_targetable = true;
        let neutral = attacker(Relation::Neutral);
        let allies = attacker(Relation::Allies);
        assert!(can_target(neutral, linked, Mode::Order));
        assert!(!can_target(neutral, plain, Mode::Order));
        assert!(can_target(neutral, targetable, Mode::Order));
        assert!(can_target(allies, targetable, Mode::Order));
        assert!(!can_target(allies, linked, Mode::Order));
        assert!(can_target(attacker(Relation::Enemies), plain, Mode::Order));
        assert!(!can_target(neutral, linked, Mode::AutoAcquire));
        assert!(!can_target(allies, targetable, Mode::Ability));
        assert!(can_target(neutral, plain, Mode::Splash));
    }

    #[test]
    fn force_accepts_any_relation() {
        let target = victim(KindOf::NONE);
        for relation in [Relation::Allies, Relation::Neutral, Relation::Enemies] {
            assert!(can_target(attacker(relation), target, Mode::Force));
        }
    }

    #[test]
    fn class_and_the_structure_scan_gate_are_checked_last() {
        let mut hostile = attacker(Relation::Enemies);
        hostile.class_allowed = false;
        assert!(!can_target(hostile, victim(KindOf::NONE), Mode::Order));

        let mut structure = victim(KindOf::NONE);
        structure.is_structure = true;
        let scanner = attacker(Relation::Enemies);
        assert!(!scanner.scans_structures);
        assert!(!can_target(scanner, structure, Mode::AutoAcquire));
        assert!(can_target(scanner, structure, Mode::Order));
        let mut building_hunter = scanner;
        building_hunter.scans_structures = true;
        assert!(can_target(building_hunter, structure, Mode::AutoAcquire));
    }
}
