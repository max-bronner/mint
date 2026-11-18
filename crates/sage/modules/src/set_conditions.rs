use mint_sage_ini::value::{bit_string_edits, BitEdit};

const WEAPON_SET_FLAGS: [&str; 104] = [
    "VETERAN",
    "ELITE",
    "HERO",
    "PLAYER_UPGRADE",
    "PASSENGER_TYPE_ONE",
    "PASSENGER_TYPE_TWO",
    "GARRISONED",
    "CLOSE_RANGE",
    "RAMPAGE",
    "CONTESTING_BUILDING",
    "WEAPON_RIDER1",
    "WEAPON_RIDER2",
    "WEAPON_RIDER3",
    "WEAPON_RIDER4",
    "WEAPON_RIDER5",
    "WEAPON_RIDER6",
    "WEAPON_RIDER7",
    "WEAPON_RIDER8",
    "SPECIAL_ONE",
    "SPECIAL_TWO",
    "CONTAINED",
    "MOUNTED",
    "ENRAGED",
    "SPECIAL_UPGRADE",
    "WEAPONSET_TOGGLE_1",
    "WEAPONSET_TOGGLE_2",
    "WEAPONSET_TOGGLE_3",
    "WEAPONSET_HERO_MODE",
    "WEAPONSET_ONE_RING_MODE",
    "WEAPONSET_CREATE_A_HERO_WS_01",
    "WEAPONSET_CREATE_A_HERO_WS_02",
    "WEAPONSET_CREATE_A_HERO_WS_03",
    "WEAPONSET_CREATE_A_HERO_WS_04",
    "WEAPONSET_CREATE_A_HERO_WS_05",
    "WEAPONSET_CREATE_A_HERO_WS_06",
    "WEAPONSET_CREATE_A_HERO_WS_07",
    "WEAPONSET_CREATE_A_HERO_WS_08",
    "WEAPONSET_CREATE_A_HERO_WS_09",
    "WEAPONSET_CREATE_A_HERO_WS_10",
    "WEAPONSET_CREATE_A_HERO_WS_11",
    "WEAPONSET_CREATE_A_HERO_WS_12",
    "WEAPONSET_CREATE_A_HERO_WS_13",
    "WEAPONSET_CREATE_A_HERO_WS_14",
    "WEAPONSET_CREATE_A_HERO_WS_15",
    "WEAPONSET_CREATE_A_HERO_WS_16",
    "WEAPONSET_CREATE_A_HERO_WS_17",
    "WEAPONSET_CREATE_A_HERO_WS_18",
    "WEAPONSET_CREATE_A_HERO_WS_19",
    "WEAPONSET_CREATE_A_HERO_WS_20",
    "WEAPONSET_CREATE_A_HERO_WS_21",
    "WEAPONSET_CREATE_A_HERO_WS_22",
    "WEAPONSET_CREATE_A_HERO_WS_23",
    "WEAPONSET_CREATE_A_HERO_WS_24",
    "WEAPONSET_CREATE_A_HERO_WS_25",
    "WEAPONSET_CREATE_A_HERO_WS_26",
    "WEAPONSET_CREATE_A_HERO_WS_27",
    "WEAPONSET_CREATE_A_HERO_WS_28",
    "WEAPONSET_CREATE_A_HERO_WS_29",
    "WEAPONSET_CREATE_A_HERO_WS_30",
    "WEAPONSET_CREATE_A_HERO_WS_31",
    "WEAPONSET_CREATE_A_HERO_WS_32",
    "HIDDEN",
    "ARMORSET_CREATE_A_HERO_01",
    "ARMORSET_CREATE_A_HERO_02",
    "ARMORSET_CREATE_A_HERO_03",
    "ARMORSET_CREATE_A_HERO_04",
    "ARMORSET_CREATE_A_HERO_05",
    "ARMORSET_CREATE_A_HERO_06",
    "ARMORSET_CREATE_A_HERO_07",
    "ARMORSET_CREATE_A_HERO_08",
    "ARMORSET_CREATE_A_HERO_09",
    "ARMORSET_CREATE_A_HERO_10",
    "WEAPONSET_CREATE_A_HERO_WS_33",
    "WEAPONSET_CREATE_A_HERO_WS_34",
    "WEAPONSET_CREATE_A_HERO_WS_35",
    "WEAPONSET_CREATE_A_HERO_WS_36",
    "WEAPONSET_CREATE_A_HERO_WS_37",
    "WEAPONSET_CREATE_A_HERO_WS_38",
    "WEAPONSET_CREATE_A_HERO_WS_39",
    "WEAPONSET_CREATE_A_HERO_WS_40",
    "WEAPONSET_CREATE_A_HERO_WS_41",
    "WEAPONSET_CREATE_A_HERO_WS_42",
    "WEAPONSET_CREATE_A_HERO_WS_43",
    "WEAPONSET_CREATE_A_HERO_WS_44",
    "WEAPONSET_CREATE_A_HERO_WS_45",
    "WEAPONSET_CREATE_A_HERO_WS_46",
    "WEAPONSET_CREATE_A_HERO_WS_47",
    "WEAPONSET_CREATE_A_HERO_WS_48",
    "WEAPONSET_CREATE_A_HERO_WS_49",
    "WEAPONSET_CREATE_A_HERO_WS_50",
    "WEAPONSET_CREATE_A_HERO_WS_51",
    "WEAPONSET_CREATE_A_HERO_WS_52",
    "WEAPONSET_CREATE_A_HERO_WS_53",
    "WEAPONSET_CREATE_A_HERO_WS_54",
    "WEAPONSET_CREATE_A_HERO_WS_55",
    "WEAPONSET_CREATE_A_HERO_WS_56",
    "WEAPONSET_CREATE_A_HERO_WS_57",
    "WEAPONSET_CREATE_A_HERO_WS_58",
    "WEAPONSET_CREATE_A_HERO_WS_59",
    "WEAPONSET_CREATE_A_HERO_WS_60",
    "WEAPONSET_CREATE_A_HERO_WS_61",
    "WEAPONSET_CREATE_A_HERO_WS_62",
    "WEAPONSET_CREATE_A_HERO_WS_63",
    "WEAPONSET_CREATE_A_HERO_WS_64",
];

const ARMOR_SET_FLAGS: [&str; 21] = [
    "VETERAN",
    "ELITE",
    "HERO",
    "PLAYER_UPGRADE",
    "WEAK_VERSUS_BASEDEFENSES",
    "ALTERNATE_FORMATION",
    "MOUNTED",
    "PLAYER_UPGRADE_2",
    "PLAYER_UPGRADE_3",
    "UNBESIEGEABLE",
    "AS_TOWER",
    "CREATE_A_HERO_01",
    "CREATE_A_HERO_02",
    "CREATE_A_HERO_03",
    "CREATE_A_HERO_04",
    "CREATE_A_HERO_05",
    "CREATE_A_HERO_06",
    "CREATE_A_HERO_07",
    "CREATE_A_HERO_08",
    "CREATE_A_HERO_09",
    "CREATE_A_HERO_10",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SetKind {
    Weapon,
    Armor,
}

impl SetKind {
    fn table(self) -> &'static [&'static str] {
        match self {
            Self::Weapon => &WEAPON_SET_FLAGS,
            Self::Armor => &ARMOR_SET_FLAGS,
        }
    }

    fn flag(self, name: &str) -> Option<&'static str> {
        self.table()
            .iter()
            .copied()
            .find(|flag| flag.eq_ignore_ascii_case(name))
    }
}

pub(crate) fn condition_flags(kind: SetKind, tokens: &[String]) -> Option<Vec<String>> {
    let mut held: Vec<&'static str> = Vec::new();
    for edit in bit_string_edits(tokens).ok()? {
        match edit {
            BitEdit::Clear => held.clear(),
            BitEdit::Set(name) => {
                let flag = kind.flag(name)?;
                if !held.contains(&flag) {
                    held.push(flag);
                }
            }
            BitEdit::Unset(name) => {
                let flag = kind.flag(name)?;
                held.retain(|have| *have != flag);
            }
        }
    }
    Some(held.into_iter().map(str::to_owned).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(kind: SetKind, line: &str) -> Option<Vec<String>> {
        let tokens: Vec<String> = line.split_whitespace().map(str::to_owned).collect();
        condition_flags(kind, &tokens)
    }

    #[test]
    fn a_plus_sets_a_flag_and_a_minus_clears_one_from_the_empty_list() {
        assert_eq!(
            read(SetKind::Weapon, "+MOUNTED +CLOSE_RANGE -MOUNTED"),
            Some(vec!["CLOSE_RANGE".to_owned()])
        );
        assert_eq!(read(SetKind::Weapon, "-WEAPONSET_TOGGLE_1"), Some(vec![]));
    }

    #[test]
    fn bare_names_replace_the_list_and_match_without_regard_to_case() {
        assert_eq!(
            read(SetKind::Armor, "player_upgrade Mounted"),
            Some(vec!["PLAYER_UPGRADE".to_owned(), "MOUNTED".to_owned()])
        );
    }

    #[test]
    fn none_first_is_the_empty_list() {
        assert_eq!(read(SetKind::Weapon, "None"), Some(vec![]));
        assert_eq!(read(SetKind::Weapon, "NONE MOUNTED"), Some(vec![]));
    }

    #[test]
    fn none_after_another_token_is_refused() {
        assert_eq!(read(SetKind::Weapon, "MOUNTED NONE"), None);
        assert_eq!(read(SetKind::Weapon, "+MOUNTED NONE"), None);
    }

    #[test]
    fn signed_and_bare_tokens_on_one_line_are_refused() {
        assert_eq!(read(SetKind::Weapon, "+MOUNTED CLOSE_RANGE"), None);
        assert_eq!(read(SetKind::Weapon, "MOUNTED -CLOSE_RANGE"), None);
    }

    #[test]
    fn a_name_outside_the_kinds_table_is_refused() {
        assert_eq!(read(SetKind::Weapon, "NOT_A_FLAG"), None);
        assert_eq!(read(SetKind::Weapon, "+NOT_A_FLAG"), None);
        assert_eq!(read(SetKind::Armor, "WEAPONSET_TOGGLE_1"), None);
        assert_eq!(read(SetKind::Weapon, "+"), None);
    }
}
