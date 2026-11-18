use super::*;
use crate::source::MemorySources;
use crate::table::{BlockTable, NoOpinion};

fn sources(files: &[(&str, &str)]) -> MemorySources {
    let mut sources = MemorySources::new();
    for (path, text) in files {
        sources.insert(path, text.as_bytes().to_vec());
    }
    sources
}

#[test]
fn an_include_is_pasted_where_the_directive_stood() {
    let files = sources(&[
            (
                "data/ini/object/tree.ini",
                "Object Tree\n  Draw = W3DTreeDraw Tag\n    #include \"..\\Includes\\model.inc\"\n  End\nEnd\n",
            ),
            ("data/ini/includes/model.inc", "    ModelName = ATree\n"),
        ]);
    let table = BlockTable::object_model_path();
    let doc = Reader::new(&files)
        .read("data/ini/object/tree.ini", &table)
        .unwrap();

    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    assert_eq!(doc.files().len(), 2);
    let draw = &doc.roots()[0].blocks().next().unwrap();
    assert_eq!(draw.field("ModelName").unwrap().value(), Some("ATree"));
    assert_eq!(
        &*draw.field("ModelName").unwrap().file,
        "data/ini/includes/model.inc"
    );
}

#[test]
fn include_matching_folds_case_on_the_folder_as_well_as_the_file() {
    let files = sources(&[
        (
            "data/ini/a.ini",
            "#include \"Includes\\X.inc\"\n#include \"includes\\x.inc\"\n",
        ),
        ("data/ini/includes/x.inc", "Object Same\nEnd\n"),
    ]);
    let doc = Reader::new(&files)
        .read("data/ini/a.ini", &BlockTable::object_model_path())
        .unwrap();
    assert_eq!(doc.roots().len(), 2, "both spellings resolved");
    assert!(doc.diagnostics().is_empty(), "{:?}", doc.diagnostics());
}

#[test]
fn a_missing_include_is_a_diagnostic_not_an_error() {
    let files = sources(&[("a.ini", "#include \"gone.inc\"\nObject Kept\nEnd\n")]);
    let doc = Reader::new(&files)
        .read("a.ini", &BlockTable::object_model_path())
        .unwrap();
    assert_eq!(doc.roots().len(), 1);
    assert!(matches!(
        doc.diagnostics().first(),
        Some(Diagnostic::IncludeMissing { .. })
    ));
}

#[test]
fn end_with_trailing_tokens_still_closes_the_block() {
    let files = sources(&[(
        "a.ini",
        "Object Rock\n  Draw = W3DTreeDraw Tag\n    ModelName = arock\n  \
             End  StateName = Sword\nEnd\n",
    )]);
    let doc = Reader::new(&files)
        .read("a.ini", &BlockTable::object_model_path())
        .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    assert_eq!(doc.roots().len(), 1);
    assert_eq!(doc.roots()[0].blocks().count(), 1);
}

#[test]
fn end_closes_a_block_whatever_its_case() {
    let files = sources(&[(
        "a.ini",
        "ExperienceScalarTable T\n  Scalars = 1.0\nEND\n\nExperienceScalarTable U\n  \
             Scalars = 1.0\nend\n",
    )]);
    let doc = Reader::new(&files)
        .read("a.ini", &BlockTable::corpus_path())
        .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    assert_eq!(doc.roots().len(), 2);
}

#[test]
fn an_include_cycle_is_an_error_rather_than_a_stack_overflow() {
    let files = sources(&[
        ("a.ini", "#include \"b.ini\"\n"),
        ("b.ini", "#include \"a.ini\"\n"),
    ]);
    let err = Reader::new(&files)
        .read("a.ini", &NoOpinion)
        .expect_err("a cycle must not be read");
    assert!(matches!(err, Error::IncludeCycle { .. }), "{err:?}");
}

#[test]
fn defines_are_captured_with_body_and_location() {
    let files = sources(&[(
        "gamedata.ini",
        "#define ELVEN_MALLORN_TREE_HEALTH 2000\nObject A\nEnd\n",
    )]);
    let doc = Reader::new(&files)
        .read("gamedata.ini", &BlockTable::object_model_path())
        .unwrap();
    assert_eq!(doc.defines().len(), 1);
    assert_eq!(doc.defines()[0].name, "ELVEN_MALLORN_TREE_HEALTH");
    assert_eq!(doc.defines()[0].body, "2000");
    assert_eq!(&*doc.defines()[0].file, "gamedata.ini");
    assert_eq!(doc.roots().len(), 1, "the object after it still parses");
}

#[test]
fn a_define_inside_an_include_is_attributed_to_the_fragment() {
    let files = sources(&[
        ("a.ini", "#include \"consts.inc\"\n"),
        ("consts.inc", "#define X 5\n"),
    ]);
    let doc = Reader::new(&files)
        .read("a.ini", &BlockTable::object_model_path())
        .unwrap();
    assert_eq!(doc.defines().len(), 1);
    assert_eq!(&*doc.defines()[0].file, "consts.inc");
}

#[test]
fn every_prefix_and_every_byte_flip_of_a_document_parses_without_panicking() {
    let valid = "#define SOME_COST 300
LODPreset = Low P4 1800 X 128 500 1024 768
Object Thing
  KindOf = STRUCTURE SELECTABLE
  Behavior = EmotionTrackerUpdate Tag_01
    AddEmotion = Terror_Base
    AddEmotion = OVERRIDE Taunt_Base
    End
  End
  Body = ActiveBody Tag_02
    MaxHealth = SOME_COST
  End
  Draw = W3DScriptedModelDraw Tag_03
    DefaultModelConditionState
      Model = AThing
    End
    AnimationState = MOVING
      BeginScript
        CurDrawablePrevAnimationState()
      EndScript
    End
  End
  Geometry = BOX
  GeometryMajorRadius = 12.0
End
Weapon AWeapon
  DamageNugget
    Damage = 5
  End
End
";
    let table = BlockTable::corpus_path();

    for end in 0..=valid.len() {
        let Some(prefix) = valid.get(..end) else {
            continue;
        };
        let files = sources(&[("t.ini", prefix)]);
        let _ = Reader::new(&files).read("t.ini", &table);
    }

    let bytes = valid.as_bytes();
    for index in 0..bytes.len() {
        for replacement in [b';', b'/', b'=', b'"', b'#', 0x00, 0xFF, b'\n'] {
            let mut corrupted = bytes.to_vec();
            if let Some(slot) = corrupted.get_mut(index) {
                *slot = replacement;
            }
            let files = sources(&[("t.ini", &String::from_utf8_lossy(&corrupted))]);
            let _ = Reader::new(&files).read("t.ini", &table);
        }
    }

    let files = sources(&[("t.ini", valid)]);
    let doc = Reader::new(&files).read("t.ini", &table).unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    assert!(!doc.has_self_nesting());
    assert_eq!(doc.roots().len(), 2, "the definition and the weapon");
}

#[test]
fn nesting_past_the_cap_is_an_error() {
    let limits = Limits {
        max_block_depth: 8,
        ..Limits::default()
    };
    let table = BlockTable::from_rules(&[("*", "Deep", true)]);
    let mut text = String::new();
    for _ in 0..64usize {
        text.push_str("Deep\n");
    }
    for _ in 0..64usize {
        text.push_str("End\n");
    }
    let files = sources(&[("bomb.ini", &text)]);
    let err = Reader::new(&files)
        .with_limits(limits)
        .read("bomb.ini", &table)
        .expect_err("a depth bomb must not be read");
    assert!(matches!(err, Error::TooDeep { limit: 8, .. }), "{err:?}");
}

#[test]
fn self_nesting_is_reported_even_though_the_document_still_balances() {
    let mut table = BlockTable::new();
    table.insert("", "Armor", true);
    table.insert("armor", "Armor", true);
    let doc = Document::parse(
        "armor.ini",
        "Armor First\n  Armor Second\n    DamageScalar = 96%\n  End\nEnd\n",
        &table,
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    assert!(doc.has_self_nesting());
    assert!(doc
        .diagnostics()
        .iter()
        .any(|d| matches!(d, Diagnostic::SelfNested { keyword, .. } if keyword == "Armor")));
}

#[test]
fn ordinary_nesting_of_different_keywords_is_not_self_nesting() {
    let doc = Document::parse(
        "x.ini",
        "Object Foo\n  Draw = W3DTreeDraw Tag\n  End\nEnd\n",
        &BlockTable::object_model_path(),
    )
    .unwrap();
    assert!(!doc.has_self_nesting(), "{:?}", doc.diagnostics());
}

#[test]
fn imbalance_is_reported_and_the_definitions_are_kept() {
    let doc = Document::parse(
        "x.ini",
        "Object A\nEnd\nEnd\nObject B\n",
        &BlockTable::object_model_path(),
    )
    .unwrap();
    assert!(!doc.is_balanced());
    assert_eq!(doc.roots().len(), 2, "both definitions survive");
    assert!(doc
        .diagnostics()
        .iter()
        .any(|d| matches!(d, Diagnostic::StrayEnd { .. })));
    assert!(doc
        .diagnostics()
        .iter()
        .any(|d| matches!(d, Diagnostic::UnclosedBlock { .. })));
}

#[test]
fn a_keyword_no_table_knows_reads_as_a_field_not_a_block() {
    let doc = Document::parse(
        "x.ini",
        "ModdedThing Foo\n\tSomeField = 1\nEnd\n",
        &NoOpinion,
    )
    .unwrap();
    assert!(!doc.is_balanced(), "{:?}", doc.diagnostics());
    assert_eq!(doc.roots().len(), 0);
    assert!(doc.diagnostics().iter().any(
        |d| matches!(d, Diagnostic::FieldAtFileScope { keyword, .. } if keyword == "ModdedThing")
    ));
    assert!(doc
        .diagnostics()
        .iter()
        .any(|d| matches!(d, Diagnostic::StrayEnd { .. })));
}

#[test]
fn every_prefix_of_a_document_is_read_without_panicking() {
    let text = "Object Rock\n  Draw = W3DTreeDraw Tag\n    ModelName = arock\n  End\n  \
                    Draw = W3DScriptedModelDraw T2\n    DefaultModelConditionState\n      \
                    Model = brock\n      BeginScript\n        junk\n      EndScript\n    End\n    \
                    IdleAnimationState\n      StateName = STATE_Idle\n      Animation = IdleA\n        \
                    AnimationName = a.b_IDLA\n        AnimationMode = LOOP\n        \
                    AnimationBlendTime = 15\n        AnimationSpeedFactorRange = 0.7 1.3\n      \
                    End\n    End\n    AnimationState = DYING SPLATTED\n      Animation\n        \
                    AnimationName = a.b_DIEB\n        AnimationMode = ONCE\n        \
                    Distance = 28\n        AnimationPriority = 6\n      End\n      \
                    Flags = RANDOMSTART\n    End\n    TransitionState = TRANS_A_to_B\n      \
                    Animation\n        AnimationName = a.b_ATNA\n      End\n    End\n  \
                    End\nEnd\nChildObject Kid Rock\nEnd\n";
    let table = BlockTable::object_model_path();
    for end in 0..=text.len() {
        let Some(prefix) = text.get(..end) else {
            continue;
        };
        let _ = Document::parse("t.ini", prefix, &table).map(interpret);
    }
}

#[test]
fn a_corrupted_document_is_read_without_panicking() {
    let text = b"Object Rock\n  Draw = W3DScriptedModelDraw T\n    DefaultModelConditionState\n      Model = a\n    End\n    IdleAnimationState\n      Animation\n        AnimationName = a.b\n        AnimationMode = LOOP\n      End\n    End\n    AnimationState = DYING SPLATTED\n      Animation\n        AnimationName = a.c\n        Distance = 28\n      End\n      Flags = RANDOMSTART\n    End\n  End\nEnd\nChildObject Kid Rock\nEnd\n";
    let table = BlockTable::object_model_path();
    for i in 0..text.len() {
        for bit in 0..8u32 {
            let mut bytes = text.to_vec();
            bytes[i] ^= 1u8 << bit;
            let _ = Document::parse("t.ini", &decode_latin1(&bytes), &table).map(interpret);
        }
    }
}

fn interpret(doc: Document) -> Document {
    let mut index = crate::ObjectIndex::new();
    index.insert_document(&doc);
    for name in ["rock", "kid", "", "ROCK"] {
        let _ = index.lookup(name);
        let _ = index.lookup_local(name);
        let _ = index.animation(name);
        let _ = index.animation_local(name);
        for state in index.animation_states(name) {
            let _ = state.is_unconditional();
            let _ = state.transition_name();
            for clip in &state.clips {
                let _ = clip.asset();
                let _ = clip.hierarchy();
            }
        }
    }
    let _ = index.animations().count();
    let _ = index.animation_state_declarations().count();
    doc
}
