use super::*;

#[test]
fn percent_strips_and_divides() {
    assert_eq!(parse_percent("50%"), Some(0.5));
    assert_eq!(parse_percent("96%"), Some(0.96));
    assert_eq!(
        parse_percent("50"),
        Some(0.5),
        "the % is optional and meaningless - retail divides either way"
    );
    assert_eq!(parse_percent("not a number"), None);
}

#[test]
fn bool_is_yes_no_only() {
    assert_eq!(parse_bool("Yes"), Some(true));
    assert_eq!(parse_bool("nO"), Some(false));
    assert_eq!(parse_bool("Maybe"), None);
}

#[test]
fn none_is_case_insensitive() {
    assert!(is_none("none"));
    assert!(is_none("NONE"));
    assert!(!is_none("NONETHELESS"));
}

#[test]
fn tagged_value_finds_its_tag_and_ignores_others() {
    let args = ["X:0".to_owned(), "Y:10".to_owned(), "Z:-1.5".to_owned()];
    assert_eq!(tagged_value(&args, "y"), Some("10"));
    assert_eq!(tagged_value(&args, "W"), None);
}

#[test]
fn tagged_value_reads_a_tag_split_from_its_number_by_a_space() {
    let args = [
        "X:".to_owned(),
        "0.0".to_owned(),
        "Y:".to_owned(),
        "0.0".to_owned(),
        "T:5".to_owned(),
    ];
    assert_eq!(tagged_value(&args, "X"), Some("0.0"));
    assert_eq!(tagged_value(&args, "T"), Some("5"));
}

#[test]
fn keyed_pairs_reads_one_bucket_per_repeated_line() {
    let fields = [
        Field {
            keyword: "Armor".to_owned(),
            args: vec!["DEFAULT".to_owned(), "100%".to_owned()],
            file: std::sync::Arc::from("a.ini"),
            line: 1,
        },
        Field {
            keyword: "Armor".to_owned(),
            args: vec!["SIEGE".to_owned(), "50%".to_owned()],
            file: std::sync::Arc::from("a.ini"),
            line: 2,
        },
    ];
    let pairs = keyed_pairs(fields.iter(), "Armor");
    assert_eq!(pairs.len(), 2);
    assert_eq!(pairs[0], ("DEFAULT".to_owned(), vec!["100%".to_owned()]));
    assert_eq!(pairs[1].0, "SIEGE");
}

#[test]
fn a_macro_call_is_recognised_and_left_unexpanded() {
    assert!(is_unexpanded_macro_call("#MULTIPLY(LEGOLAS_BOW_RANGE .01)"));
    assert!(!is_unexpanded_macro_call("LEGOLAS_BOW_RANGE"));
}

fn define(name: &str, body: &str) -> Define {
    Define {
        name: name.to_owned(),
        body: body.to_owned(),
        file: std::sync::Arc::from("gamedata.ini"),
        line: 1,
    }
}

#[test]
fn macro_table_resolves_a_bare_number() {
    let mut table = MacroTable::new();
    table.insert_defines(&[define("ELVEN_MALLORN_TREE_HEALTH", "2000")]);
    assert_eq!(table.get("ELVEN_MALLORN_TREE_HEALTH"), Some(2000.0));
    assert_eq!(table.get("NOT_DEFINED"), None);
}

#[test]
fn macro_table_stores_a_percentage_bodys_raw_number_undivided() {
    let mut table = MacroTable::new();
    table.insert_defines(&[
        define("BERSERKER_RAGE_ARMOR_L1", "50%"),
        define("GONDOR_BARRACKS_LVL2_BUILD_SPEED", "1.10"),
    ]);
    assert_eq!(table.get("BERSERKER_RAGE_ARMOR_L1"), Some(50.0));
    assert!(table.is_percent_bodied("BERSERKER_RAGE_ARMOR_L1"));
    assert_eq!(table.get("GONDOR_BARRACKS_LVL2_BUILD_SPEED"), Some(1.10));
    assert!(
        !table.is_percent_bodied("GONDOR_BARRACKS_LVL2_BUILD_SPEED"),
        "a bare-number body is not percent-bodied - attributemodifier.ini's own \
             GONDOR_BARRACKS_LVL2_BUILD_SPEED, a ready-made multiplier"
    );
    assert!(!table.is_percent_bodied("UNDEFINED"));
}

#[test]
fn resolve_percent_divides_a_macros_raw_value() {
    let mut table = MacroTable::new();
    table.insert_defines(&[
        define("BERSERKER_RAGE_ARMOR_L1", "50%"),
        define("GOBLIN_ARMOR_BONUS", "25"),
    ]);
    assert_eq!(
        resolve_percent(&["BERSERKER_RAGE_ARMOR_L1".to_owned()], &table),
        Some(0.5)
    );
    assert_eq!(
        resolve_percent(&["GOBLIN_ARMOR_BONUS".to_owned()], &table),
        Some(0.25),
        "a bare-bodied macro divides the same way a literal 25 would"
    );
    assert_eq!(
        resolve_percent(&["UNDEFINED_MACRO".to_owned()], &table),
        None
    );
}

#[test]
fn resolve_percent_reads_a_literal_or_an_arithmetic_call_too() {
    let mut table = MacroTable::new();
    table.insert_defines(&[define("BASE", "40")]);
    assert_eq!(resolve_percent(&["100%".to_owned()], &table), Some(1.0));
    assert_eq!(
        resolve_percent(
            &[
                "#MULTIPLY(".to_owned(),
                "BASE".to_owned(),
                "2".to_owned(),
                ")".to_owned()
            ],
            &table
        ),
        Some(0.8),
        "an arithmetic call's result divides too - ini-reference.md §3.4"
    );
}

#[test]
fn parse_percent_shaped_divides_only_what_was_spelled_as_a_percent() {
    let mut table = MacroTable::new();
    table.insert_defines(&[
        define("GONDOR_BARRACKS_LVL2_BUILD_SPEED", "1.10"),
        define("EOWYN_SHIELD_MAIDEN_ARMOR_BONUS_PERCENTAGE", "50%"),
    ]);
    assert_eq!(
        parse_percent_shaped("50", &table),
        Some(50.0),
        "a flat literal, AUTO_HEAL's own shape"
    );
    assert_eq!(parse_percent_shaped("50%", &table), Some(0.5));
    assert_eq!(
        parse_percent_shaped("GONDOR_BARRACKS_LVL2_BUILD_SPEED", &table),
        Some(1.10),
        "a ready-made multiplier, taken as written"
    );
    assert_eq!(
        parse_percent_shaped("EOWYN_SHIELD_MAIDEN_ARMOR_BONUS_PERCENTAGE", &table),
        Some(0.5),
        "a percent-bodied macro divides, with no literal % at the use site"
    );
    assert_eq!(parse_percent_shaped("UNDEFINED", &table), None);
}

#[test]
fn duration_seconds_field_multiplies_by_a_thousand() {
    assert_eq!(
        Duration::parse_seconds_field("3"),
        Some(Duration::Milliseconds(3000.0))
    );
    assert_eq!(
        Duration::parse_seconds_field("0.5"),
        Some(Duration::Milliseconds(500.0))
    );
    assert_eq!(Duration::parse_seconds_field("not a number"), None);
}

#[test]
fn a_macro_name_not_all_upper_case_is_reported_not_refused() {
    let mut table = MacroTable::new();
    table.insert_defines(&[define("Lowercase_Name", "5")]);
    assert_eq!(table.get("Lowercase_Name"), Some(5.0), "still usable");
    assert_eq!(
        table.diagnostics(),
        &[MacroDiagnostic::NotUpperCase {
            name: "Lowercase_Name".to_owned(),
            file: std::sync::Arc::from("gamedata.ini"),
            line: 1,
        }]
    );
}

#[test]
fn a_bit_string_line_mixing_bare_and_signed_names_is_refused() {
    let args = |text: &str| -> Vec<String> { text.split(' ').map(str::to_owned).collect() };
    assert_eq!(
        bit_string_edits(&args("INFANTRY HERO")),
        Ok(vec![
            BitEdit::Clear,
            BitEdit::Set("INFANTRY"),
            BitEdit::Set("HERO")
        ])
    );
    assert_eq!(
        bit_string_edits(&args("+HERO -INFANTRY")),
        Ok(vec![BitEdit::Set("HERO"), BitEdit::Unset("INFANTRY")])
    );
    assert_eq!(
        bit_string_edits(&args("NONE HERO")),
        Ok(vec![BitEdit::Clear])
    );
    assert_eq!(bit_string_edits(&args("INFANTRY +HERO")), Err("+HERO"));
    assert_eq!(bit_string_edits(&args("+HERO INFANTRY")), Err("INFANTRY"));
    assert_eq!(bit_string_edits(&args("HERO NONE")), Err("NONE"));
}

#[test]
fn a_macro_with_no_body_is_reported() {
    let mut table = MacroTable::new();
    table.insert_defines(&[define("EMPTY", "")]);
    assert_eq!(table.get("EMPTY"), None);
    assert_eq!(table.tokens("EMPTY"), None);
    assert_eq!(
        table.diagnostics(),
        &[MacroDiagnostic::NoValue {
            name: "EMPTY".to_owned(),
            file: std::sync::Arc::from("gamedata.ini"),
            line: 1,
        }]
    );
}

#[test]
fn a_redefined_macro_name_is_reported_and_keeps_the_later_value() {
    let mut table = MacroTable::new();
    table.insert_defines(&[define("X", "1"), define("X", "2")]);
    assert_eq!(
        table.get("X"),
        Some(2.0),
        "later declaration wins, as before"
    );
    assert_eq!(
        table.diagnostics(),
        &[MacroDiagnostic::Duplicate {
            name: "X".to_owned(),
            file: std::sync::Arc::from("gamedata.ini"),
            line: 1,
        }]
    );
}

#[test]
fn macro_table_ignores_identifier_lists_and_arithmetic_calls() {
    let mut table = MacroTable::new();
    table.insert_defines(&[
        define("GOOD_FARM", "GondorFarm ArnorFarm"),
        define("CAMPNESS_SUMMONED_CITADEL", "#ADD( CAMPNESS_FORTRESS 0 )"),
    ]);
    assert!(table.is_empty(), "neither body is a bare number");
    assert_eq!(
        table.tokens("GOOD_FARM"),
        Some(&["GondorFarm".to_owned(), "ArnorFarm".to_owned()][..]),
        "a list is kept for expansion"
    );
    assert_eq!(table.get("GOOD_FARM"), None);
}

#[test]
fn macro_table_redefinition_moves_a_name_between_number_and_list() {
    let mut table = MacroTable::new();
    table.insert_defines(&[define("X", "A B"), define("X", "3")]);
    assert_eq!((table.get("X"), table.tokens("X")), (Some(3.0), None));
    table.insert_defines(&[define("X", "C D")]);
    assert_eq!(table.get("X"), None);
    assert_eq!(table.tokens("X").map(<[String]>::len), Some(2));
}

#[test]
fn macro_table_keeps_the_last_definition_of_a_redefined_name() {
    let mut table = MacroTable::new();
    table.insert_defines(&[define("X", "1"), define("X", "2")]);
    assert_eq!(table.get("X"), Some(2.0));
}

#[test]
fn macro_table_lookup_ignores_case() {
    let mut table = MacroTable::new();
    table.insert_defines(&[define("HEALTH", "100")]);
    assert_eq!(table.get("Health"), Some(100.0));
}

#[test]
fn parse_float_or_macro_prefers_a_direct_number() {
    let mut table = MacroTable::new();
    table.insert_defines(&[define("X", "999")]);
    assert_eq!(parse_float_or_macro("5.0", &table), Some(5.0));
    assert_eq!(parse_float_or_macro("X", &table), Some(999.0));
    assert_eq!(parse_float_or_macro("UNDEFINED", &table), None);
}

#[test]
fn expand_token_list_macros_splices_a_list_in_place() {
    let mut table = MacroTable::new();
    table.insert_defines(&[define(
        "GONDOR_RANGER_KINDOF",
        "PRELOAD SELECTABLE INFANTRY ARCHER",
    )]);
    let mut args = vec!["GONDOR_RANGER_KINDOF".to_owned(), "SUMMONED".to_owned()];
    expand_token_list_macros(&mut args, &table);
    assert_eq!(
        args,
        vec!["PRELOAD", "SELECTABLE", "INFANTRY", "ARCHER", "SUMMONED"]
    );
}

#[test]
fn expand_token_list_macros_leaves_unrelated_tokens_alone() {
    let table = MacroTable::new();
    let mut args = vec!["STRUCTURE".to_owned(), "SELECTABLE".to_owned()];
    expand_token_list_macros(&mut args, &table);
    assert_eq!(args, vec!["STRUCTURE", "SELECTABLE"]);
}

#[test]
fn expand_token_list_macros_does_not_touch_a_numeric_macro() {
    let mut table = MacroTable::new();
    table.insert_defines(&[define("ELVEN_MALLORN_TREE_HEALTH", "2000")]);
    let mut args = vec!["ELVEN_MALLORN_TREE_HEALTH".to_owned()];
    expand_token_list_macros(&mut args, &table);
    assert_eq!(args, vec!["ELVEN_MALLORN_TREE_HEALTH"]);
}

#[test]
fn expand_token_list_macros_terminates_on_a_self_referencing_macro() {
    let mut table = MacroTable::new();
    table.insert_defines(&[define("LOOP", "LOOP A")]);
    let mut args = vec!["LOOP".to_owned()];
    expand_token_list_macros(&mut args, &table);
    assert_eq!(args.len(), 1 + MAX_MACRO_EXPANSION_PASSES);
    assert_eq!(args.first(), Some(&"LOOP".to_owned()));
}

#[test]
fn expand_token_list_macros_resolves_a_macro_naming_a_macro() {
    let mut table = MacroTable::new();
    table.insert_defines(&[
        define("GOOD_TROOPS", "GOOD_INFANTRY GOOD_CAVALRY"),
        define("GOOD_INFANTRY", "RohanSpearmen"),
        define("GOOD_CAVALRY", "RohanRider"),
    ]);
    let mut args = vec!["GOOD_TROOPS".to_owned()];
    expand_token_list_macros(&mut args, &table);
    assert_eq!(args, vec!["RohanSpearmen", "RohanRider"]);
}

#[test]
fn an_arithmetic_call_evaluates_its_two_operands() {
    let mut table = MacroTable::new();
    table.insert_defines(&[define("DELTA", "60"), define("GRANTED", "1")]);
    let eval = |text: &str| parse_float_or_expression(text, &table);
    assert_eq!(eval("#MULTIPLY( DELTA 3 )"), Some(180.0));
    assert_eq!(eval("#DIVIDE( 150 GRANTED )"), Some(150.0));
    assert_eq!(eval("#ADD(DELTA 5)"), Some(65.0));
    assert_eq!(eval("#subtract( DELTA 10 )"), Some(50.0));
    assert_eq!(eval("DELTA"), Some(60.0), "a bare macro still reads");
    assert_eq!(eval("#DIVIDE( 1 0 )"), None, "a zero divisor");
    assert_eq!(eval("#MULTIPLY( DELTA )"), None, "one operand");
    assert_eq!(eval("#MULTIPLY( 1 2 3 )"), None, "three operands");
    assert_eq!(eval("#MULTIPLY( MISSING 2 )"), None, "an unknown macro");
    assert_eq!(eval("#POWER( 2 2 )"), None, "an unknown call");
    assert_eq!(eval("#MULTIPLY( 2 2"), None, "no closing parenthesis");
    assert_eq!(eval("#"), None);
}
