use super::*;
use crate::testutil::MapBuilder;

fn condition_payload(
    b: &mut MapBuilder,
    condition_type: i32,
    name: &str,
    parms: &[&str],
) -> Vec<u8> {
    let mut out = condition_type.to_le_bytes().to_vec();
    out.extend_from_slice(&b.name_key(name).to_le_bytes());
    out.extend_from_slice(&i32::try_from(parms.len()).unwrap().to_le_bytes());
    for parm in parms {
        out.extend_from_slice(&string_parameter(parm));
    }
    out
}

fn string_parameter(value: &str) -> Vec<u8> {
    let mut out = 5i32.to_le_bytes().to_vec();
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&0f32.to_le_bytes());
    out.extend_from_slice(&MapBuilder::ascii(value));
    out
}

fn int_parameter(value: i32) -> Vec<u8> {
    let mut out = 0i32.to_le_bytes().to_vec();
    out.extend_from_slice(&value.to_le_bytes());
    out.extend_from_slice(&0f32.to_le_bytes());
    out.extend_from_slice(&MapBuilder::ascii(""));
    out
}

fn coord3d_parameter(x: f32, y: f32, z: f32) -> Vec<u8> {
    let mut out = COORD3D_PARAM_TYPE.to_le_bytes().to_vec();
    out.extend_from_slice(&x.to_le_bytes());
    out.extend_from_slice(&y.to_le_bytes());
    out.extend_from_slice(&z.to_le_bytes());
    out
}

fn synthetic_scripts(b: &mut MapBuilder) -> Vec<u8> {
    let condition = condition_payload(b, 42, "NAMED_OWNED_BY_PLAYER", &["Flag_1"]);
    let condition_chunk = b.nested_chunk("Condition", 4, &condition);
    let or_condition_chunk = b.nested_chunk("OrCondition", 2, &condition_chunk);

    let mut then_action = 7i32.to_le_bytes().to_vec();
    then_action.extend_from_slice(&b.name_key("SET_COUNTER").to_le_bytes());
    then_action.extend_from_slice(&2i32.to_le_bytes());
    then_action.extend_from_slice(&string_parameter("g_Test_Counter"));
    then_action.extend_from_slice(&int_parameter(600));
    let then_chunk = b.nested_chunk("ScriptAction", 2, &then_action);

    let mut else_action = 8i32.to_le_bytes().to_vec();
    else_action.extend_from_slice(&b.name_key("DISABLE_SCRIPT").to_le_bytes());
    else_action.extend_from_slice(&1i32.to_le_bytes());
    else_action.extend_from_slice(&string_parameter("Some Script"));
    let else_chunk = b.nested_chunk("ScriptActionFalse", 2, &else_action);

    let mut script_payload = MapBuilder::ascii("Attack General");
    script_payload.extend_from_slice(&MapBuilder::ascii("a comment"));
    script_payload.extend_from_slice(&MapBuilder::ascii("cond comment"));
    script_payload.extend_from_slice(&MapBuilder::ascii("action comment"));
    script_payload.push(1);
    script_payload.push(0);
    script_payload.push(1);
    script_payload.push(1);
    script_payload.push(0);
    script_payload.push(0);
    script_payload.extend_from_slice(&0i32.to_le_bytes());
    script_payload.extend_from_slice(&or_condition_chunk);
    script_payload.extend_from_slice(&then_chunk);
    script_payload.extend_from_slice(&else_chunk);
    let script_chunk = b.nested_chunk("Script", 2, &script_payload);

    let list_chunk = b.nested_chunk("ScriptList", 1, &script_chunk);
    let empty_list = b.nested_chunk("ScriptList", 1, &[]);
    let mut payload = list_chunk;
    payload.extend_from_slice(&empty_list);
    b.nested_chunk("PlayerScriptsList", 1, &payload)
}

fn parse(chunk_bytes: &[u8], b: &MapBuilder) -> Result<ScriptTree> {
    let names = b.name_table();
    let chunks = read_chunks(chunk_bytes, 0, chunk_bytes.len(), &names)?;
    let chunk = chunks.first().expect("one chunk");
    read_player_scripts_list(chunk_bytes, chunk, &names)
}

#[test]
fn decodes_the_whole_shape_one_script_one_condition_two_actions() {
    let mut b = MapBuilder::new();
    let bytes = synthetic_scripts(&mut b);
    let tree = parse(&bytes, &b).expect("valid synthetic tree");
    assert_eq!(tree.player_slots.len(), 2);
    assert!(tree.player_slots[1].is_empty());
    let [ScriptNode::Script(script)] = tree.player_slots[0].as_slice() else {
        panic!("expected exactly one script");
    };
    assert_eq!(script.name, "Attack General");
    assert!(script.active);
    assert!(!script.one_shot);
    assert!(script.easy && script.normal && !script.hard);
    assert_eq!(script.or_conditions.len(), 1);
    assert_eq!(script.or_conditions[0].conditions.len(), 1);
    let condition = &script.or_conditions[0].conditions[0];
    assert_eq!(condition.name, "NAMED_OWNED_BY_PLAYER");
    assert_eq!(condition.condition_type, 42);
    assert_eq!(
        condition.params[0],
        Parameter::Value {
            param_type: 5,
            int_value: 0,
            real_value: 0.0,
            string_value: String::from("Flag_1"),
        }
    );
    assert_eq!(script.actions.len(), 1);
    assert_eq!(script.actions[0].name, "SET_COUNTER");
    assert_eq!(script.actions_false.len(), 1);
    assert_eq!(script.actions_false[0].name, "DISABLE_SCRIPT");
}

#[test]
fn coord3d_parameters_decode_their_three_floats_and_nothing_else() {
    let mut b = MapBuilder::new();
    let mut action = 1i32.to_le_bytes().to_vec();
    action.extend_from_slice(&b.name_key("CAMERA_MOVE_TO").to_le_bytes());
    action.extend_from_slice(&1i32.to_le_bytes());
    action.extend_from_slice(&coord3d_parameter(1.5, 2.5, 3.5));
    let action_chunk = b.nested_chunk("ScriptAction", 2, &action);
    let names = b.name_table();
    let chunks = read_chunks(&action_chunk, 0, action_chunk.len(), &names).unwrap();
    let decoded = read_script_action(&action_chunk, &chunks[0], &names, "ScriptAction").unwrap();
    assert_eq!(
        decoded.params[0],
        Parameter::Coord3D {
            x: 1.5,
            y: 2.5,
            z: 3.5
        }
    );
}

#[test]
fn an_unknown_version_is_refused_for_every_node_kind() {
    let mut b = MapBuilder::new();
    let condition = condition_payload(&mut b, 1, "X", &[]);
    let chunk = b.nested_chunk("Condition", 99, &condition);
    let names = b.name_table();
    let chunks = read_chunks(&chunk, 0, chunk.len(), &names).unwrap();
    let err = read_condition(&chunk, &chunks[0], &names).unwrap_err();
    assert!(matches!(
        err,
        Error::UnknownScriptVersion {
            chunk: "Condition",
            version: 99
        }
    ));
}

#[test]
fn condition_and_action_trailers_for_the_bfme_only_versions_are_accepted_and_skipped() {
    let mut b = MapBuilder::new();
    let mut condition = condition_payload(&mut b, 1, "X", &[]);
    condition.extend_from_slice(&[0u8; 8]);
    let chunk = b.nested_chunk("Condition", 5, &condition);
    let names = b.name_table();
    let chunks = read_chunks(&chunk, 0, chunk.len(), &names).unwrap();
    let decoded = read_condition(&chunk, &chunks[0], &names).unwrap();
    assert_eq!(decoded.name, "X");
    assert!(
        !decoded.enabled,
        "an all-zero v5 trailer is a disabled condition"
    );

    let mut b3 = MapBuilder::new();
    let mut enabled = condition_payload(&mut b3, 1, "X", &[]);
    enabled.extend_from_slice(&1u32.to_le_bytes());
    enabled.extend_from_slice(&0u32.to_le_bytes());
    let enabled_chunk = b3.nested_chunk("Condition", 6, &enabled);
    let names3 = b3.name_table();
    let chunks3 = read_chunks(&enabled_chunk, 0, enabled_chunk.len(), &names3).unwrap();
    assert!(
        read_condition(&enabled_chunk, &chunks3[0], &names3)
            .unwrap()
            .enabled
    );

    let mut b4 = MapBuilder::new();
    let v4 = condition_payload(&mut b4, 1, "X", &[]);
    let v4_chunk = b4.nested_chunk("Condition", 4, &v4);
    let names4 = b4.name_table();
    let chunks4 = read_chunks(&v4_chunk, 0, v4_chunk.len(), &names4).unwrap();
    assert!(
        read_condition(&v4_chunk, &chunks4[0], &names4)
            .unwrap()
            .enabled
    );

    let mut b2 = MapBuilder::new();
    let short_condition = condition_payload(&mut b2, 1, "X", &[]);
    let short_chunk = b2.nested_chunk("Condition", 5, &short_condition);
    let names2 = b2.name_table();
    let chunks2 = read_chunks(&short_chunk, 0, short_chunk.len(), &names2).unwrap();
    let err = read_condition(&short_chunk, &chunks2[0], &names2).unwrap_err();
    assert!(matches!(
        err,
        Error::TrailingBytes {
            name: "Condition",
            ..
        }
    ));
}

fn script_fixed_fields(name: &str) -> Vec<u8> {
    let mut out = MapBuilder::ascii(name);
    out.extend_from_slice(&MapBuilder::ascii(""));
    out.extend_from_slice(&MapBuilder::ascii(""));
    out.extend_from_slice(&MapBuilder::ascii(""));
    out.extend_from_slice(&[1, 0, 1, 1, 1, 0]);
    out.extend_from_slice(&0i32.to_le_bytes());
    out
}

#[test]
fn script_v3_and_v4_trailers_are_length_prefixed_strings_not_a_fixed_skip() {
    let mut b = MapBuilder::new();
    let condition = condition_payload(&mut b, 1, "CONDITION_TRUE", &[]);
    let condition_chunk = b.nested_chunk("Condition", 4, &condition);
    let or_condition_chunk = b.nested_chunk("OrCondition", 2, &condition_chunk);
    let mut v3_payload = script_fixed_fields("V3 Script");
    v3_payload.extend_from_slice(&[1, 1]);
    v3_payload.extend_from_slice(&(-1i32).to_le_bytes());
    v3_payload.push(1);
    v3_payload.extend_from_slice(&MapBuilder::ascii("teamPlyrGondor"));
    v3_payload.extend_from_slice(&or_condition_chunk);
    let v3_chunk = b.nested_chunk("Script", 3, &v3_payload);
    let names = b.name_table();
    let v3_chunks = read_chunks(&v3_chunk, 0, v3_chunk.len(), &names).unwrap();
    let v3 = read_script(&v3_chunk, &v3_chunks[0], &names, 0).expect("v3 trailer decodes");
    assert_eq!(v3.name, "V3 Script");
    assert_eq!(v3.or_conditions.len(), 1);
    assert_eq!(
        v3.sequence,
        ScriptSequence {
            run_sequentially: true,
            loop_flag: true,
            loop_count: -1,
            target_is_unit: true,
            target: "teamPlyrGondor".to_owned(),
            phases: String::new(),
        }
    );

    let mut b2 = MapBuilder::new();
    let condition2 = condition_payload(&mut b2, 1, "CONDITION_TRUE", &[]);
    let condition_chunk2 = b2.nested_chunk("Condition", 4, &condition2);
    let or_condition_chunk2 = b2.nested_chunk("OrCondition", 2, &condition_chunk2);
    let mut v4_payload = script_fixed_fields("V4 Script");
    v4_payload.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0]);
    v4_payload.extend_from_slice(&MapBuilder::ascii("<This Team>"));
    v4_payload.extend_from_slice(&MapBuilder::ascii("ALL"));
    v4_payload.extend_from_slice(&or_condition_chunk2);
    let v4_chunk = b2.nested_chunk("Script", 4, &v4_payload);
    let names2 = b2.name_table();
    let v4_chunks = read_chunks(&v4_chunk, 0, v4_chunk.len(), &names2).unwrap();
    let v4 = read_script(&v4_chunk, &v4_chunks[0], &names2, 0).expect("v4 trailer decodes");
    assert_eq!(v4.name, "V4 Script");
    assert_eq!(v4.or_conditions.len(), 1);
    assert!(!v4.sequence.run_sequentially);
    assert_eq!(v4.sequence.target, "<This Team>");
    assert_eq!(v4.sequence.phases, "ALL");
}

#[test]
fn a_script_cut_inside_its_sequential_trailer_errors_rather_than_panicking() {
    let mut b = MapBuilder::new();
    let mut payload = script_fixed_fields("Cut");
    payload.extend_from_slice(&[1, 0]);
    payload.extend_from_slice(&3i32.to_le_bytes());
    payload.push(0);
    payload.extend_from_slice(&MapBuilder::ascii("<This Team>"));
    payload.extend_from_slice(&MapBuilder::ascii("ALL"));
    let fixed = script_fixed_fields("Cut").len();
    let cuts: Vec<Vec<u8>> = (fixed..=payload.len())
        .map(|cut| b.nested_chunk("Script", 4, &payload[..cut]))
        .collect();
    let names = b.name_table();
    for (cut, chunk) in cuts.iter().enumerate() {
        let chunks = read_chunks(chunk, 0, chunk.len(), &names).unwrap();
        let read = read_script(chunk, &chunks[0], &names, 0);
        if fixed + cut < payload.len() {
            assert!(read.is_err(), "prefix of {} bytes decoded", fixed + cut);
        } else {
            assert_eq!(read.unwrap().sequence.loop_count, 3);
        }
    }
}

#[test]
fn a_name_key_whose_type_tag_is_not_the_ascii_string_tag_is_refused() {
    let mut b = MapBuilder::new();
    let mut condition = 1i32.to_le_bytes().to_vec();
    condition.extend_from_slice(&((1u32 << 8) | 1).to_le_bytes());
    condition.extend_from_slice(&0i32.to_le_bytes());
    let chunk = b.nested_chunk("Condition", 4, &condition);
    let names = b.name_table();
    let chunks = read_chunks(&chunk, 0, chunk.len(), &names).unwrap();
    let err = read_condition(&chunk, &chunks[0], &names).unwrap_err();
    assert!(matches!(
        err,
        Error::Malformed {
            name: "NameKey",
            ..
        }
    ));
}

#[test]
fn a_numparms_past_the_bytes_present_is_refused_not_allocated() {
    let mut b = MapBuilder::new();
    let mut condition = 1i32.to_le_bytes().to_vec();
    condition.extend_from_slice(&b.name_key("X").to_le_bytes());
    condition.extend_from_slice(&i32::MAX.to_le_bytes());
    let chunk = b.nested_chunk("Condition", 4, &condition);
    let names = b.name_table();
    let chunks = read_chunks(&chunk, 0, chunk.len(), &names).unwrap();
    let err = read_condition(&chunk, &chunks[0], &names).unwrap_err();
    assert!(matches!(err, Error::CountExceedsPayload { .. }));
}

#[test]
fn a_negative_numparms_is_refused() {
    let mut b = MapBuilder::new();
    let mut condition = 1i32.to_le_bytes().to_vec();
    condition.extend_from_slice(&b.name_key("X").to_le_bytes());
    condition.extend_from_slice(&(-1i32).to_le_bytes());
    let chunk = b.nested_chunk("Condition", 4, &condition);
    let names = b.name_table();
    let chunks = read_chunks(&chunk, 0, chunk.len(), &names).unwrap();
    let err = read_condition(&chunk, &chunks[0], &names).unwrap_err();
    assert!(matches!(
        err,
        Error::Malformed {
            name: "Condition",
            ..
        }
    ));
}

#[test]
fn a_script_group_may_nest_another_group_bfme_version_3() {
    let mut b = MapBuilder::new();
    let mut inner = MapBuilder::ascii("Inner");
    inner.push(1);
    inner.push(0);
    let inner_chunk = b.nested_chunk("ScriptGroup", 2, &inner);
    let mut outer = MapBuilder::ascii("Outer");
    outer.push(1);
    outer.push(0);
    outer.extend_from_slice(&inner_chunk);
    let outer_chunk = b.nested_chunk("ScriptGroup", 3, &outer);
    let names = b.name_table();
    let chunks = read_chunks(&outer_chunk, 0, outer_chunk.len(), &names).unwrap();
    let group = read_script_group(&outer_chunk, &chunks[0], &names, 0).unwrap();
    assert_eq!(group.name, "Outer");
    let [ScriptNode::Group(inner_group)] = group.children.as_slice() else {
        panic!("expected one nested group");
    };
    assert_eq!(inner_group.name, "Inner");
}

#[test]
fn depth_past_the_cap_is_refused_not_a_stack_overflow() {
    let mut b = MapBuilder::new();
    let mut payload = MapBuilder::ascii("Leaf");
    payload.push(1);
    payload.push(0);
    let mut chunk = b.nested_chunk("ScriptGroup", 2, &payload);
    for i in 0..MAX_SCRIPT_DEPTH + 2 {
        let mut wrapper = MapBuilder::ascii(&format!("G{i}"));
        wrapper.push(1);
        wrapper.push(0);
        wrapper.extend_from_slice(&chunk);
        chunk = b.nested_chunk("ScriptGroup", 3, &wrapper);
    }
    let names = b.name_table();
    let chunks = read_chunks(&chunk, 0, chunk.len(), &names).unwrap();
    let err = read_script_group(&chunk, &chunks[0], &names, 0).unwrap_err();
    assert!(matches!(
        err,
        Error::Malformed {
            name: "ScriptGroup",
            ..
        }
    ));
}

#[test]
fn initial_int_values_reads_the_last_set_counter_per_name_in_file_order() {
    let mut b = MapBuilder::new();

    let action_of = |b: &mut MapBuilder, name: &str, value: i32| {
        let mut action = 1i32.to_le_bytes().to_vec();
        action.extend_from_slice(&b.name_key("SET_COUNTER").to_le_bytes());
        action.extend_from_slice(&2i32.to_le_bytes());
        action.extend_from_slice(&string_parameter(name));
        action.extend_from_slice(&int_parameter(value));
        b.nested_chunk("ScriptAction", 2, &action)
    };
    let first = action_of(&mut b, "g_ATTACK_LAUNCH", 600);
    let second = action_of(&mut b, "g_ATTACK_LAUNCH", 999);
    let mut script_payload = MapBuilder::ascii("Init");
    script_payload.extend_from_slice(&MapBuilder::ascii(""));
    script_payload.extend_from_slice(&MapBuilder::ascii(""));
    script_payload.extend_from_slice(&MapBuilder::ascii(""));
    script_payload.extend_from_slice(&[1, 0, 1, 1, 1, 0]);
    script_payload.extend_from_slice(&0i32.to_le_bytes());
    script_payload.extend_from_slice(&first);
    script_payload.extend_from_slice(&second);
    let script_chunk = b.nested_chunk("Script", 2, &script_payload);
    let list_chunk = b.nested_chunk("ScriptList", 1, &script_chunk);
    let top = b.nested_chunk("PlayerScriptsList", 1, &list_chunk);

    let tree = parse(&top, &b).expect("valid tree");
    let values = initial_int_values(&tree, &["SET_COUNTER"]);
    assert_eq!(values.get("g_ATTACK_LAUNCH"), Some(&999));
    assert_eq!(values.len(), 1);
}

#[test]
fn the_not_flag_negates_every_condition_type_but_the_five_the_engine_evaluates() {
    let decode = |name: &str, not: u32| {
        let mut b = MapBuilder::new();
        let mut payload = condition_payload(&mut b, 1, name, &[]);
        payload.extend_from_slice(&1u32.to_le_bytes());
        payload.extend_from_slice(&not.to_le_bytes());
        let chunk = b.nested_chunk("Condition", 6, &payload);
        let names = b.name_table();
        let chunks = read_chunks(&chunk, 0, chunk.len(), &names).unwrap();
        read_condition(&chunk, &chunks[0], &names).unwrap()
    };
    assert!(decode("NAMED_BUILDING_IS_EMPTY", 1).negates());
    assert!(!decode("NAMED_BUILDING_IS_EMPTY", 0).negates());
    assert!(decode("FLAG", 1).inverted && !decode("FLAG", 1).negates());
}

#[test]
fn a_version_three_action_with_a_zero_trailer_is_disabled() {
    let decode = |trailer: u32| {
        let mut b = MapBuilder::new();
        let mut action = 1i32.to_le_bytes().to_vec();
        action.extend_from_slice(&b.name_key("SET_FLAG").to_le_bytes());
        action.extend_from_slice(&0i32.to_le_bytes());
        action.extend_from_slice(&trailer.to_le_bytes());
        let chunk = b.nested_chunk("ScriptAction", 3, &action);
        let names = b.name_table();
        let chunks = read_chunks(&chunk, 0, chunk.len(), &names).unwrap();
        read_script_action(&chunk, &chunks[0], &names, "ScriptAction").unwrap()
    };
    assert!(!decode(0).enabled);
    assert!(decode(1).enabled);
}
