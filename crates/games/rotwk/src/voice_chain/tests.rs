use super::*;

const NOTHING: TargetFacts = TargetFacts {
    kind: 0,
    structure: false,
    machine: false,
    heal_pad: false,
    airborne: false,
    contested: false,
};

fn labels(chain: &[Slot]) -> Vec<&str> {
    chain.iter().map(Slot::label).collect()
}

#[test]
fn an_attack_on_a_structure_asks_for_its_own_line_before_the_charge() {
    let keep = TargetFacts {
        structure: true,
        machine: true,
        ..NOTHING
    };
    assert_eq!(
        labels(&attack(Some(("Keep", keep)), true, false)),
        [
            "VoiceAttackUnitKeep",
            VOICE_ATTACK_STRUCTURE,
            VOICE_ATTACK_CHARGE,
            VOICE_ATTACK
        ]
    );
    let ram = TargetFacts {
        machine: true,
        ..NOTHING
    };
    assert_eq!(
        labels(&attack(Some(("Ram", ram)), false, false)),
        ["VoiceAttackUnitRam", VOICE_ATTACK_MACHINE, VOICE_ATTACK]
    );
}

#[test]
fn an_enter_state_attack_asks_for_the_enter_state_fields() {
    assert_eq!(
        labels(&attack(Some(("Ram", NOTHING)), true, true)),
        [
            "VoiceAttackUnitRam",
            VOICE_ENTER_STATE_ATTACK_CHARGE,
            VOICE_ENTER_STATE_ATTACK,
        ]
    );
}

#[test]
fn a_flier_is_attacked_with_the_air_line_and_the_ground_with_none() {
    let flier = TargetFacts {
        airborne: true,
        ..NOTHING
    };
    assert_eq!(
        labels(&attack(Some(("Eagle", flier)), false, false)),
        ["VoiceAttackUnitEagle", VOICE_ATTACK_AIR, VOICE_ATTACK]
    );
    assert_eq!(labels(&attack(None, false, false)), [VOICE_ATTACK]);
}

#[test]
fn an_enter_picks_one_line_by_what_it_goes_into() {
    let well = TargetFacts {
        heal_pad: true,
        structure: true,
        ..NOTHING
    };
    assert_eq!(
        labels(&enter(Some(("Well", well)))),
        ["VoiceEnterUnitWell", VOICE_GET_HEALED]
    );
    let tower = TargetFacts {
        structure: true,
        ..NOTHING
    };
    assert_eq!(
        labels(&enter(Some(("Tower", tower)))),
        ["VoiceEnterUnitTower", VOICE_GARRISON]
    );
    assert_eq!(
        labels(&enter(Some(("Ship", NOTHING)))),
        ["VoiceEnterUnitShip", VOICE_ENTER]
    );
    assert_eq!(labels(&enter(None)), [VOICE_ENTER]);
}

#[test]
fn camp_holding_picks_retreat_before_camp_when_engaged() {
    assert_eq!(
        labels(&move_order(true, false, true, false)),
        [
            VOICE_RETREAT_TO_CASTLE,
            VOICE_MOVE_TO_CAMP,
            VOICE_MOVE_WHILE_ATTACKING,
            VOICE_MOVE
        ]
    );
    assert_eq!(
        labels(&move_order(true, false, false, false)),
        [VOICE_MOVE_TO_CAMP, VOICE_MOVE]
    );
}

#[test]
fn the_move_chain_tries_higher_ground_then_while_attacking_then_the_plain_line() {
    assert_eq!(
        labels(&move_order(false, true, false, false)),
        [VOICE_MOVE_TO_HIGHER_GROUND, VOICE_MOVE]
    );
    assert_eq!(
        labels(&move_order(false, false, true, false)),
        [VOICE_MOVE_WHILE_ATTACKING, VOICE_MOVE]
    );
    assert_eq!(
        labels(&move_order(false, false, false, false)),
        [VOICE_MOVE]
    );
}

#[test]
fn an_enter_state_move_asks_for_the_enter_state_fields() {
    assert_eq!(
        labels(&move_order(true, false, true, true)),
        [
            VOICE_ENTER_STATE_RETREAT_TO_CASTLE,
            VOICE_ENTER_STATE_MOVE_TO_CAMP,
            VOICE_ENTER_STATE_MOVE_WHILE_ATTACKING,
            VOICE_ENTER_STATE_MOVE,
        ]
    );
}

#[test]
fn an_enter_on_a_contested_structure_plays_the_hostile_line_before_garrison() {
    let contested_structure = TargetFacts {
        structure: true,
        contested: true,
        ..NOTHING
    };
    assert_eq!(
        labels(&enter(Some(("Keep", contested_structure)))),
        ["VoiceEnterUnitKeep", VOICE_ENTER_HOSTILE]
    );
}

#[test]
fn the_charge_waits_out_its_clock_and_never_plays_mid_fight() {
    let mut clocks = ChargeClocks::default();
    assert!(clocks.due(7, false, 0));
    assert!(!clocks.due(7, true, 0));
    clocks.restart(7, 100, 300);
    assert!(!clocks.due(7, false, 399));
    assert!(clocks.due(7, false, 400));
    clocks.retain(&[]);
    assert!(clocks.due(7, false, 101));
}
