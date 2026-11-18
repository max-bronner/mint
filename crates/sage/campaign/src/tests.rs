use super::*;

fn started(missions: u32) -> CampaignState {
    let mut state = CampaignState::new(missions, 1);
    assert_eq!(
        state.start(),
        Outcome::StartMission {
            index: 0,
            play_intro_movie: true,
            restore_carryover_snapshot: false
        }
    );
    state
}

#[test]
fn a_won_mission_advances_once_however_often_it_is_asked() {
    let mut state = started(3);
    assert!(state.script_action("QuickVictory"));
    state.match_torn_down();
    state.match_torn_down();
    assert_eq!(state.index(), 1);
    assert_eq!(
        state.next_mission(),
        Outcome::StartMission {
            index: 1,
            play_intro_movie: true,
            restore_carryover_snapshot: false
        }
    );
}

#[test]
fn whether_a_mission_follows_looks_one_past_the_index_until_advanced() {
    let mut state = started(2);
    state.script_action("VICTORY");
    assert!(state.has_next_mission());
    state.match_torn_down();
    assert!(state.has_next_mission());
    assert_eq!(
        state.close_time_line(),
        TimeLineClose::CampaignMenu { autosave: true }
    );
    state.next_mission();
    state.script_action("VICTORY");
    assert!(!state.has_next_mission());
    assert_eq!(state.close_time_line(), TimeLineClose::CampaignComplete);
    state.match_torn_down();
    assert_eq!(state.close_time_line(), TimeLineClose::CampaignComplete);
    assert_eq!(state.next_mission(), Outcome::MainMenu);
}

#[test]
fn last_mission_after_a_defeat_retries_without_the_intro_and_clears_the_defeat() {
    let mut state = started(3);
    state.next_mission();
    assert!(state.script_action("LocalDefeat"));
    state.match_torn_down();
    assert!(state.defeated());
    assert_eq!(
        state.close_time_line(),
        TimeLineClose::CampaignMenu { autosave: false }
    );
    assert_eq!(
        state.last_mission(),
        Outcome::StartMission {
            index: 1,
            play_intro_movie: false,
            restore_carryover_snapshot: true
        }
    );
    assert!(!state.defeated());
}

#[test]
fn a_defeated_mission_that_reaches_mission_start_ends_the_campaign() {
    let mut state = started(3);
    state.script_action("DEFEAT");
    assert_eq!(state.next_mission(), Outcome::MainMenu);
    assert!(!state.defeated());
}

#[test]
fn last_mission_after_a_win_goes_on_like_next() {
    let mut state = started(3);
    state.script_action("VICTORY");
    state.match_torn_down();
    assert_eq!(
        state.last_mission(),
        Outcome::StartMission {
            index: 1,
            play_intro_movie: true,
            restore_carryover_snapshot: false
        }
    );
}

#[test]
fn only_the_four_ending_actions_are_recognised() {
    let mut state = started(1);
    assert!(!state.script_action("DEBUG_MESSAGE_BOX"));
    assert!(!state.victorious() && !state.defeated());
}

fn marked(template: &str, level: i32, kills: i32) -> CapturedObject {
    CapturedObject {
        template: template.to_owned(),
        experience: 10.0,
        level,
        kills_mission: kills,
        kills_total: kills,
        ..CapturedObject::default()
    }
}

fn spawned_names(carry: &mut Carryover, delayed: &[&str]) -> Vec<String> {
    let delayed: Vec<String> = delayed.iter().map(|&name| name.to_owned()).collect();
    let mut names = Vec::new();
    carry.restore(&delayed, |record| {
        names.push(record.template_name.to_string_lossy());
        1
    });
    names
}

#[test]
fn restore_spawns_the_captured_units_and_holds_back_the_delayed_name() {
    let mut carry = Carryover::seed([
        SeedUnit {
            name: "SeedA",
            hero: true,
        },
        SeedUnit {
            name: "SeedB",
            hero: false,
        },
    ]);
    assert_eq!(carry.hero_record_count(), 1);
    carry.capture(&[marked("SeedA", 4, 3), marked("SeedB", 2, 1)], &[], None);
    let names = spawned_names(&mut carry, &["seedb"]);
    assert_eq!(names, ["SeedA", "SeedA"]);
    assert_eq!(carry.used().len(), 2);
    assert!(carry.has_delayed("SeedB"));
    assert!(!carry.has_delayed("SeedA"));
    let held: Vec<_> = carry.available().iter().map(|r| r.level).collect();
    assert_eq!(held, [0, 2]);
}

#[test]
fn capture_keeps_only_grantable_sciences_and_persistent_upgrades() {
    let mut carry = Carryover::default();
    let player = PlayerCapture {
        money: 250.0,
        sciences: vec![
            ScienceFact {
                name: "KeptScience".into(),
                grantable: true,
            },
            ScienceFact {
                name: "DroppedScience".into(),
                grantable: false,
            },
        ],
        upgrades: vec![
            UpgradeFact {
                name: "KeptUpgrade".into(),
                persists: true,
            },
            UpgradeFact {
                name: "DroppedUpgrade".into(),
                persists: false,
            },
        ],
        cost_multiplier: 0.0,
    };
    carry.capture(&[], &[], Some(&player));
    assert_eq!(carry.sciences(), ["KeptScience"]);
    assert_eq!(carry.upgrades(), ["KeptUpgrade"]);
    assert!((carry.money() - 250.0).abs() < f32::EPSILON);
}

#[test]
fn a_captured_hero_is_stored_under_its_respawn_name() {
    let hero = CapturedObject {
        hero: true,
        respawn_name: Some("RespawnNamed".into()),
        ..marked("OwnTemplate", 1, 0)
    };
    let horde = CapturedObject {
        horde: true,
        horde_name: Some(String::new()),
        ..marked("HordeTemplate", 1, 0)
    };
    assert_eq!(hero.record_name(), "RespawnNamed");
    assert_eq!(horde.record_name(), "HordeTemplate");
}

#[test]
fn one_create_takes_a_single_count_and_a_failed_spawn_takes_nothing() {
    let mut carry = Carryover::seed([SeedUnit {
        name: "Pair",
        hero: false,
    }]);
    carry.capture(&[marked("Pair", 1, 0)], &[], None);
    assert!(!carry.create_delayed("Pair", |_| false));
    assert_eq!(carry.available().len(), 2);
    assert!(carry.create_delayed("Pair", |record| record.count == 1));
    assert_eq!(carry.available().len(), 1);
    assert_eq!(carry.used().len(), 1);
}

#[test]
fn loading_advances_an_ended_state_once_and_leaves_an_advanced_one_alone() {
    let ended = mint_sage_save::CampaignState {
        mission_index: 0,
        mission_won: true,
        mission_ended: true,
        ..mint_sage_save::CampaignState::default()
    };
    let mut loaded = CampaignState::from_save(3, 1, &ended);
    assert!(matches!(
        loaded.load(),
        Outcome::StartMission { index: 1, .. }
    ));
    let advanced = mint_sage_save::CampaignState {
        mission_index: 1,
        index_advanced: true,
        ..ended
    };
    let mut loaded = CampaignState::from_save(3, 1, &advanced);
    assert!(matches!(
        loaded.load(),
        Outcome::StartMission { index: 1, .. }
    ));
}

#[test]
fn the_rally_falls_back_to_the_start_and_the_start_moves_along_x() {
    let anchor = walk_on_anchor(Some([100.0, 50.0, 3.0]), None);
    assert_eq!(anchor.start, [110.0, 50.0, 3.0]);
    assert_eq!(anchor.rally, [100.0, 50.0, 3.0]);
    assert!((anchor.angle.abs() - core::f32::consts::PI).abs() < 1e-3);
}

#[test]
fn starting_at_a_mission_begins_there_and_past_the_end_is_the_main_menu() {
    let mut state = CampaignState::new(3, 1);
    assert_eq!(
        state.start_at(2),
        Outcome::StartMission {
            index: 2,
            play_intro_movie: true,
            restore_carryover_snapshot: false
        }
    );
    assert_eq!(state.start_at(3), Outcome::MainMenu);
}
