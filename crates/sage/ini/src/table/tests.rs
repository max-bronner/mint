use super::*;

#[test]
fn the_table_tells_a_block_from_a_leaf_in_the_same_context() {
    let table = BlockTable::object_model_path();
    let object = [Scope::new("Object")];
    assert_eq!(table.opens_block(&object, "Draw", &[]), Some(true));
    assert_eq!(
        table.opens_block(&object, "EditorSorting", &[]),
        Some(false)
    );

    let draw = [Scope::new("Object"), Scope::slot("Draw", "W3DTreeDraw")];
    assert_eq!(table.opens_block(&draw, "ModelName", &[]), Some(false));
    assert_eq!(
        table.opens_block(&draw, "DefaultModelConditionState", &[]),
        Some(true)
    );
}

#[test]
fn a_rule_applies_only_in_its_own_context() {
    let table = BlockTable::object_model_path();
    let state = [
        Scope::new("Object"),
        Scope::slot("Draw", "W3DScriptedModelDraw"),
        Scope::new("DefaultModelConditionState"),
    ];
    assert_eq!(table.opens_block(&state, "Model", &[]), Some(false));
    assert_eq!(table.opens_block(&[], "Object", &[]), Some(true));
    assert_eq!(table.opens_block(&[], "Model", &[]), None);
    assert_eq!(table.opens_block(&[], "Weapon", &[]), None);
    assert_eq!(table.opens_block(&state, "Object", &[]), Some(false));
}

#[test]
fn keyword_matching_is_strcmp_exact_but_context_and_selector_still_fold() {
    let table = BlockTable::object_model_path();
    assert_eq!(table.opens_block(&[], "Object", &[]), Some(true));
    assert_eq!(table.opens_block(&[], "OBJECT", &[]), None);
    assert_eq!(
        table.opens_block(&[Scope::new("oBjEcT")], "Draw", &[]),
        Some(true),
        "the parent context still folds case"
    );
    assert_ne!(
        table.opens_block(&[Scope::new("oBjEcT")], "draw", &[]),
        Some(true),
        "but the keyword being classified does not"
    );

    let mut slotted = BlockTable::new();
    slotted.insert("draw:w3dtreedraw", "SpecialField", true);
    let ctx = [Scope::slot("DRAW", "W3DTreeDraw")];
    assert_eq!(slotted.opens_block(&ctx, "SpecialField", &[]), Some(true));
    assert_eq!(slotted.opens_block(&ctx, "specialfield", &[]), None);
}

#[test]
fn the_most_specific_context_wins() {
    let mut table = BlockTable::from_rules(&[
        ("*", "Thing", false),
        ("draw", "Thing", true),
        ("draw:w3dtreedraw", "Thing", false),
    ]);
    table.insert("", "Thing", true);

    assert_eq!(table.opens_block(&[], "Thing", &[]), Some(true));
    assert_eq!(
        table.opens_block(&[Scope::new("Draw")], "Thing", &[]),
        Some(true)
    );
    assert_eq!(
        table.opens_block(&[Scope::slot("Draw", "W3DTreeDraw")], "Thing", &[]),
        Some(false)
    );
    assert_eq!(
        table.opens_block(&[Scope::new("Something")], "Thing", &[]),
        Some(false),
        "falls through to the anywhere rule"
    );
}

#[test]
fn an_empty_table_never_has_an_opinion() {
    let table = BlockTable::new();
    assert!(table.is_empty());
    assert_eq!(table.opens_block(&[], "Object", &[]), None);
    assert_eq!(NoOpinion.opens_block(&[], "Object", &[]), None);
}

#[test]
fn the_generals_condition_state_spellings_are_absent() {
    let table = BlockTable::object_model_path();
    let draw = [Scope::slot("Draw", "W3DScriptedModelDraw")];
    for generals in [
        "DefaultConditionState",
        "ConditionState",
        "AliasConditionState",
    ] {
        assert_eq!(
            table.opens_block(&draw, generals, &[]),
            Some(false),
            "{generals} is not a BFME state keyword, and `draw` is closed"
        );
    }
    assert_eq!(
        table.opens_block(&draw, "DefaultModelConditionState", &[]),
        Some(true)
    );
}

#[test]
fn a_closed_context_answers_leaf_where_an_open_one_defers() {
    let closed = BlockTable::object_model_path();
    let open = BlockTable::from_rules(OBJECT_MODEL_RULES);
    let object = [Scope::new("Object")];
    assert_eq!(open.opens_block(&object, "ModAddedField", &[]), None);
    assert_eq!(
        closed.opens_block(&object, "ModAddedField", &[]),
        Some(false)
    );
    assert!(closed.is_closed("Object"));
    assert!(!closed.is_closed(""), "file scope stays open");
    assert!(!closed.is_closed("Behavior"), "module field sets unknown");
}

#[test]
fn a_wildcard_block_accepts_any_key_and_needs_no_reader_change() {
    let mut table = BlockTable::new();
    table.insert("", "ModAddedWildcard", true);
    table.declare_wildcard("modaddedwildcard");
    assert!(table.is_wildcard("ModAddedWildcard"), "folds case");
    assert!(!table.is_wildcard("UnitSpecificSounds"));

    let block = [Scope::new("ModAddedWildcard")];
    for key in ["AnyKeyAtAll", "TankTurretMove", "SomeUnrelatedTemplateName"] {
        assert_eq!(
            table.opens_block(&block, key, &[]),
            Some(false),
            "{key} is a leaf, not a question for a fixed vocabulary"
        );
    }
}

#[test]
fn unit_specific_sounds_and_fx_are_wildcard_in_the_object_model_path() {
    let table = BlockTable::object_model_path();
    assert!(table.is_wildcard("UnitSpecificSounds"));
    assert!(table.is_wildcard("UnitSpecificFX"));
    let sounds = [Scope::new("UnitSpecificSounds")];
    let fx = [Scope::new("UnitSpecificFX")];
    for key in ["VoiceGarrison", "AKeyNoShippedFileWrites"] {
        assert_eq!(table.opens_block(&sounds, key, &[]), Some(false));
        assert_eq!(table.opens_block(&fx, key, &[]), Some(false));
    }
}

#[test]
fn an_alias_context_inherits_the_rules_it_points_at() {
    let table = BlockTable::object_model_path();
    for spelling in ["Object", "ChildObject", "ObjectReskin"] {
        let ctx = [Scope::new(spelling)];
        assert_eq!(
            table.opens_block(&ctx, "Draw", &[]),
            Some(true),
            "{spelling}"
        );
        assert_eq!(
            table.opens_block(&ctx, "KindOf", &[]),
            Some(false),
            "{spelling}"
        );
    }
    for spelling in ["DefaultModelConditionState", "ModelConditionState"] {
        let ctx = [Scope::new(spelling)];
        assert_eq!(
            table.opens_block(&ctx, "Model", &[]),
            Some(false),
            "{spelling}"
        );
        assert_eq!(
            table.opens_block(&ctx, "Animation", &[]),
            Some(true),
            "{spelling}"
        );
    }
}

#[test]
fn the_corpus_table_never_contradicts_the_object_model_one() {
    let narrow = BlockTable::object_model_path();
    let wide = BlockTable::corpus_path();
    for &(context, keyword, opens) in OBJECT_MODEL_RULES {
        let scope = if context.is_empty() {
            Vec::new()
        } else {
            vec![Scope::new(context)]
        };
        assert_eq!(
            wide.opens_block(&scope, keyword, &[]),
            Some(opens),
            "{context}/{keyword} disagrees"
        );
        assert_eq!(narrow.opens_block(&scope, keyword, &[]), Some(opens));
    }
}

#[test]
fn the_corpus_table_answers_where_the_object_model_one_defers() {
    let narrow = BlockTable::object_model_path();
    let wide = BlockTable::corpus_path();

    assert_eq!(narrow.opens_block(&[], "MappedImage", &[]), None);
    assert_eq!(wide.opens_block(&[], "MappedImage", &[]), Some(true));

    assert_eq!(narrow.opens_block(&[], "LODPreset", &[]), None);
    assert_eq!(wide.opens_block(&[], "LODPreset", &[]), Some(false));

    let module = [Scope::new("Object"), Scope::slot("Behavior", "AnyModule")];
    assert_eq!(
        narrow.opens_block(&module, "SomeMisindentedField", &[]),
        None
    );
    assert_eq!(
        wide.opens_block(&module, "SomeMisindentedField", &[]),
        Some(false)
    );
    assert_eq!(wide.opens_block(&module, "MeleeBehavior", &[]), Some(true));
}

#[test]
fn add_emotion_opens_only_on_its_own_override_argument() {
    let table = BlockTable::corpus_path();
    let module = [
        Scope::new("Object"),
        Scope::slot("Behavior", "EmotionTrackerUpdate"),
    ];
    let name = ["Taunt_Base".to_owned()];
    let override_form = ["OVERRIDE".to_owned(), "Taunt_Base".to_owned()];
    assert_eq!(table.opens_block(&module, "AddEmotion", &[]), Some(false));
    assert_eq!(table.opens_block(&module, "AddEmotion", &name), Some(false));
    assert_eq!(
        table.opens_block(&module, "AddEmotion", &override_form),
        Some(true)
    );
    let shout = ["override".to_owned(), "Taunt_Base".to_owned()];
    assert_eq!(table.opens_block(&[], "AddEmotion", &shout), Some(true));
}

#[test]
fn one_keyword_opens_in_one_context_and_is_a_leaf_in_another() {
    let table = BlockTable::corpus_path();
    assert_eq!(
        table.opens_block(&[Scope::new("RegionCampain")], "Region", &[]),
        Some(true)
    );
    assert_eq!(
        table.opens_block(&[Scope::new("Connection")], "Region", &[]),
        Some(false)
    );
    assert_eq!(
        table.opens_block(&[Scope::new("LargeGroupAudioMap")], "Sound", &[]),
        Some(true)
    );
    assert_eq!(
        table.opens_block(&[Scope::new("Object")], "Sound", &[]),
        Some(false)
    );
}

#[test]
fn closing_a_slot_context_does_not_close_its_siblings() {
    let table = BlockTable::priority_blocks_path();
    let horde = [
        Scope::new("Object"),
        Scope::slot("Behavior", "HordeContain"),
    ];
    assert_eq!(table.opens_block(&horde, "MeleeBehavior", &[]), Some(true));
    for leaf in ["FrontAngle", "ObjectStatusOfContained", "Slots", "RankInfo"] {
        assert_eq!(table.opens_block(&horde, leaf, &[]), Some(false), "{leaf}");
    }
    for spelling in ["HorseHordeContain", "AODHordeContain"] {
        let ctx = [Scope::new("Object"), Scope::slot("Behavior", spelling)];
        assert_eq!(
            table.opens_block(&ctx, "MeleeBehavior", &[]),
            Some(true),
            "{spelling}"
        );
        assert_eq!(
            table.opens_block(&ctx, "FrontAngle", &[]),
            Some(false),
            "{spelling}"
        );
    }

    let other = [
        Scope::new("Object"),
        Scope::slot("Behavior", "InvisibilityUpdate"),
    ];
    assert_eq!(table.opens_block(&other, "InvisibilityNugget", &[]), None);
    let garrison = [
        Scope::new("Object"),
        Scope::slot("Behavior", "HordeGarrisonContain"),
    ];
    assert_eq!(table.opens_block(&garrison, "FrontAngle", &[]), None);
}
