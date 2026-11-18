use super::*;

fn labels() -> RuleLabels {
    RuleLabels {
        factors: COMMAND_POINT_FACTORS
            .iter()
            .map(|(p, _)| format!("f{p}"))
            .collect(),
        default_suffix: " d".to_owned(),
        ..RuleLabels::default()
    }
}

#[test]
fn a_combo_row_marks_its_default_and_selects_the_rules_value() {
    let mut rules = RuleChoices::default().defaults();
    let money = combo(&rules, &labels(), 0).unwrap_or_default();
    let texts: Vec<&str> = money.items.iter().map(|i| i.text.as_str()).collect();
    assert_eq!(&texts[1..4], ["750", "1000 d", "1200"]);
    assert_eq!(money.selected, Some(2));
    rules.command_point_factor = 10_000;
    let factor = combo(&rules, &labels(), 1).unwrap_or_default();
    let texts: Vec<&str> = factor.items.iter().map(|i| i.text.as_str()).collect();
    assert_eq!(&texts[1..4], ["f50", "f100 d", "f200"]);
    assert_eq!(factor.selected, Some(6));
    assert!(combo(&rules, &labels(), 2).is_none(), "two combo rows");
}

#[test]
fn choosing_an_entry_sets_its_rule() {
    let mut rules = RuleChoices::default().defaults();
    assert!(choose(&mut rules, 0, 10));
    assert_eq!(rules.starting_money, 4000);
    assert!(!choose(&mut rules, 0, 10), "already");
    assert!(choose(&mut rules, 1, 6));
    assert_eq!(rules.command_point_factor, 10_000);
    assert!(!choose(&mut rules, 0, 99));
    assert!(!choose(&mut rules, 5, 0));
    assert_eq!(rules.starting_money, 4000);
}

#[test]
fn the_check_rows_are_custom_then_ring_heroes() {
    let mut rules = MatchRules::default();
    assert!(tick(&mut rules, 1, false));
    assert!(!rules.allow_ring_heroes && rules.allow_custom_heroes);
    assert_eq!(check(&rules, 1).map(|c| c.checked), Some(false));
    assert_eq!(check(&rules, 0).map(|c| c.checked), Some(true));
    assert!(!tick(&mut rules, 1, false), "already");
    assert!(tick(&mut rules, 0, false));
    assert!(!rules.allow_custom_heroes);
    assert!(!tick(&mut rules, 2, true));
    assert!(check(&rules, 2).is_none());
    assert_eq!(COMBO_KEYS[0], "RULE:InitialResources");
    assert_eq!(
        CHECK_KEYS,
        ["RULE:AllowCustomHeroes", "RULE:AllowRingHeroes"]
    );
}

#[test]
fn ring_heroes_follow_their_rule() {
    let ring = vec!["Ring".to_owned()];
    let heroes = vec!["A".to_owned(), "B".to_owned()];
    let mut rules = MatchRules::default();
    assert_eq!(heroes_offered(&rules, &ring, &heroes), ["Ring", "A", "B"]);
    rules.allow_ring_heroes = false;
    assert_eq!(heroes_offered(&rules, &ring, &heroes), ["A", "B"]);
}

#[test]
fn the_rows_are_named_by_kind_and_number() {
    assert_eq!(combo_id(1), "RuleComboBox_1");
    assert_eq!(check_id(0), "RuleCheckBox_0");
}
