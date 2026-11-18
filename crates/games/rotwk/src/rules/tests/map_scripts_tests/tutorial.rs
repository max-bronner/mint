use mint_sim::Simulation;

use super::hud::local_sim;
use super::*;
use crate::rules::UiPanel;

const LOCAL: PlayerId = PlayerId::new(0);

fn probe(when: OrCondition) -> ScriptNode {
    ScriptNode::Script(script(
        "Probe",
        true,
        false,
        vec![when],
        vec![action(
            "SET_COUNTER",
            vec![string_param("Open"), int_param(1)],
        )],
        vec![action(
            "SET_COUNTER",
            vec![string_param("Open"), int_param(0)],
        )],
    ))
}

fn run(simulation: &mut Simulation<RotwkRules>, command: Option<RotwkCommand>) -> i32 {
    if let Some(command) = command {
        simulation.submit(LOCAL, command).unwrap();
    }
    for _ in 0..13 {
        simulation.tick().unwrap();
    }
    let ids = simulation.rules().ids().unwrap();
    let state: crate::rules::map_scripts::MapScriptState = simulation
        .state()
        .store(ids.map_script_state)
        .and_then(|store| store.iter().next().map(|(_, s)| s.clone()))
        .unwrap();
    state.counter_value("Wilds", "Open") + state.counter_value("", "Open")
}

fn open(panel: UiPanel, open: bool) -> Option<RotwkCommand> {
    Some(RotwkCommand::SetUiPanelOpen { panel, open })
}

#[test]
fn a_panel_condition_is_false_until_the_presentation_reports_the_panel_open() {
    for (name, panel) in [
        ("SPELL_STORE_IS_OPEN", UiPanel::SpellStore),
        ("OBJECTIVES_SCREEN_IS_OPEN", UiPanel::Objectives),
        ("TOGGLE_STANCE_SUBMENU_IS_OPEN", UiPanel::StanceSubMenu),
    ] {
        let mut simulation = local_sim(vec![probe(condition(name, vec![]))]);
        assert_eq!(run(&mut simulation, None), 0, "{name} before any command");
        assert_eq!(run(&mut simulation, open(panel, true)), 1, "{name} open");
        assert_eq!(run(&mut simulation, open(panel, false)), 0, "{name} closed");
    }
}

#[test]
fn the_scroll_distance_must_be_strictly_greater_and_a_camera_reset_clears_it() {
    let mut simulation = local_sim(vec![probe(condition(
        "CAMERA_SCROLL_DISTANCE",
        vec![real_param(250.0)],
    ))]);
    let scrolled = |distance| Some(RotwkCommand::CameraScrolled { distance });
    assert_eq!(run(&mut simulation, scrolled(250.0)), 0);
    assert_eq!(run(&mut simulation, scrolled(1.0)), 1);
    assert_eq!(run(&mut simulation, Some(RotwkCommand::CameraReset)), 0);
}

#[test]
fn a_formation_order_stays_issued_once_the_drag_command_was_applied() {
    let mut simulation = local_sim(vec![probe(condition(
        "PLAYER_ISSUED_FORMATION_ORDER",
        vec![],
    ))]);
    assert_eq!(run(&mut simulation, None), 0);
    let drag = RotwkCommand::MoveToFormation {
        x: 20.0,
        y: 20.0,
        dx: 5.0,
        dy: 0.0,
        attack: false,
    };
    assert_eq!(run(&mut simulation, Some(drag)), 1);
    assert_eq!(run(&mut simulation, None), 1);
}

const STANCED: &str = "\
ModifierList ZBlind\n  Category = LEADERSHIP\n  Modifier = VISION -90%\nEnd\n\
StanceTemplate ZStances\n  Stance HoldGround\n    AttributeModifier ZBlind\n  End\nEnd\n\
Locomotor ZFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 200\nEnd\n\
Object ZGollum\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  Behavior = StancesBehavior Tag_S\n    StanceTemplate = ZStances\n  End\n  \
  LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 4\n  GeometryHeight = 10\nEnd\n";

#[test]
fn a_unit_is_using_the_stance_the_simulation_holds_for_it() {
    let seen = |stance: &str| {
        let make = one_shot(
            "Make",
            condition("CONDITION_TRUE", vec![]),
            vec![named_at("A", "SpawnPoint_1")],
        );
        let nodes = vec![
            make,
            probe(condition(
                "UNIT_USING_STANCE",
                vec![string_param("A"), string_param(stance)],
            )),
        ];
        let mut simulation = squad_sim_with(
            nodes,
            team_def("Wilds", "Squad", 1, 1),
            STANCED,
            &["ZGollum"],
        );
        run(&mut simulation, None)
    };
    assert_eq!(seen("Battle"), 1);
    assert_eq!(seen("HoldGround"), 0);
}

fn probe_open_after(extra: Vec<ScriptAction>, panel: UiPanel, name: &str) -> i32 {
    let nodes = vec![
        one_shot("Setup", condition("CONDITION_TRUE", vec![]), extra),
        probe(condition(name, vec![])),
    ];
    let mut simulation = local_sim(nodes);
    run(&mut simulation, None);
    run(&mut simulation, open(panel, true))
}

#[test]
fn a_panel_a_script_disabled_cannot_be_opened_until_it_is_enabled_again() {
    for (name, panel, disable, enable) in [
        (
            "SPELL_STORE_IS_OPEN",
            UiPanel::SpellStore,
            "DISABLE_SPELL_STORE",
            "ENABLE_SPELL_STORE",
        ),
        (
            "OBJECTIVES_SCREEN_IS_OPEN",
            UiPanel::Objectives,
            "DISABLE_OBJECTIVES_SCREEN",
            "ENABLE_OBJECTIVES_SCREEN",
        ),
    ] {
        let off = probe_open_after(vec![action(disable, vec![])], panel, name);
        let back = probe_open_after(
            vec![action(disable, vec![]), action(enable, vec![])],
            panel,
            name,
        );
        assert_eq!((off, back), (0, 1), "{name}");
    }
}

#[test]
fn closing_the_objectives_screen_from_a_script_leaves_it_closed() {
    let nodes = vec![
        probe(condition("OBJECTIVES_SCREEN_IS_OPEN", vec![])),
        ScriptNode::Script(script(
            "Close",
            true,
            true,
            vec![condition("OBJECTIVES_SCREEN_IS_OPEN", vec![])],
            vec![action("CLOSE_OBJECTIVES_SCREEN", vec![])],
            Vec::new(),
        )),
    ];
    let mut simulation = local_sim(nodes);
    run(&mut simulation, None);
    assert_eq!(run(&mut simulation, open(UiPanel::Objectives, true)), 0);
}

#[test]
fn sound_verbs_keep_a_sorted_disabled_set_that_enable_all_empties() {
    let nodes = vec![one_shot(
        "Off",
        condition("CONDITION_TRUE", vec![]),
        vec![
            action("SOUND_DISABLE_TYPE", vec![string_param("Zeta")]),
            action("SOUND_DISABLE_TYPE", vec![string_param("Alpha")]),
            action("SOUND_ENABLE_TYPE", vec![string_param("zeta")]),
        ],
    )];
    let mut simulation = local_sim(nodes);
    run(&mut simulation, None);
    let ids = simulation.rules().ids().unwrap();
    let state = simulation
        .state()
        .store(ids.map_script_state)
        .and_then(|store| store.iter().next().map(|(_, s)| s.clone()))
        .unwrap();
    assert!(state.audio.event_disabled("ALPHA") && !state.audio.event_disabled("Zeta"));
    assert_eq!(state.audio.pending.len(), 3);
}
