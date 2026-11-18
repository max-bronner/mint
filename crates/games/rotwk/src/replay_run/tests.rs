use super::*;

fn round_trip(original: RotwkCommand) {
    let printed = format!("{original:?}");
    let value = Parser::new(&printed).value_to_end().unwrap();
    assert_eq!(command(&value).unwrap(), original, "{printed}");
}

#[test]
fn printed_commands_read_back_as_themselves() {
    let unit = Entity::from_parts(193, 22);
    for original in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![unit, Entity::from_parts(4, 1)],
        },
        RotwkCommand::ClearSelection,
        RotwkCommand::ToggleWeaponSet {
            button: String::from("a \"quoted\" name"),
            lead: Some(unit),
        },
        RotwkCommand::MoveTo {
            x: 1516.752,
            y: -0.1,
            queue: false,
        },
        RotwkCommand::FireSpecialPower {
            button: 0,
            target: Some(unit),
            at: None,
        },
        RotwkCommand::CastSpell {
            button: 2,
            target: None,
            at: Some((1203.4987, 2159.197)),
        },
        RotwkCommand::Debug(DebugCommand::Damage { percent: 25 }),
        RotwkCommand::Debug(DebugCommand::Spawn {
            kind: 3,
            x: 1.0,
            y: 2.0,
            facing: 3.0,
            role: SpawnRole::FightingDummy,
        }),
        RotwkCommand::SetRallyPoint {
            structures: vec![],
            x: 5.5,
            y: 6.25,
            global: false,
            target: None,
        },
    ] {
        round_trip(original);
    }
}

#[test]
fn a_recording_reads_its_header_commands_and_hashes() {
    let text = "mint-rotwk command log 1\nreplay-format 3\nseed 0x00000000005eedc8\n\
                initial-hash 0x0000000000000010\nframes 2\ncommands 1\n\
                # c <frame> <player> <sequence> <command>\n\
                c 1 0 0 MoveTo { x: 1.5, y: 2.0 }\n\
                # h <frame> <state hash after it>\nh 0 0x01\nh 1 0x02\n";
    let recording = parse(text).unwrap();
    assert_eq!(recording.seed, 0x5eedc8);
    assert_eq!(recording.initial_hash, 0x10);
    assert_eq!(recording.hashes, [1, 2]);
    assert_eq!(recording.commands.len(), 1);
    assert!(parse("seed 0x1\ninitial-hash 0x2\nh 1 0x3\n").is_err());
}

#[test]
fn a_truncated_recording_is_refused_rather_than_replayed_short() {
    let text = "mint-rotwk command log 1\nreplay-format 3\nseed 0x00000000005eedc8\n\
                initial-hash 0x0000000000000010\nframes 2\ncommands 1\n\
                # c <frame> <player> <sequence> <command>\n\
                c 1 0 0 MoveTo { x: 1.5, y: 2.0 }\n\
                # h <frame> <state hash after it>\nh 0 0x01\n";
    assert!(
        parse(text).is_err(),
        "the file promises 2 frames but only one `h` line is present"
    );

    let wrong_version = "mint-rotwk command log 1\nreplay-format 999\nseed 0x1\n\
                initial-hash 0x2\nframes 1\ncommands 0\nh 0 0x3\n";
    assert!(parse(wrong_version).is_err(), "unknown replay-format");
}
