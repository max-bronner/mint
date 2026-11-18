use mint_sim::Simulation;

use crate::rules::map_scripts::{ScriptView, UiFlashTarget};

use super::*;

fn view_after(nodes: Vec<ScriptNode>, ticks: u32) -> (ScriptView, Simulation<RotwkRules>) {
    let mut simulation = squad_sim(nodes, 1);
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
    let view = simulation.rules().script_view(simulation.state());
    (view, simulation)
}

fn once(actions: Vec<ScriptAction>) -> Vec<ScriptNode> {
    vec![one_shot("Go", condition("CONDITION_TRUE", vec![]), actions)]
}

fn view_of(actions: Vec<ScriptAction>) -> ScriptView {
    view_after(once(actions), 12).0
}

fn cameo(seconds: i32) -> ScriptAction {
    action(
        "CAMEO_FLASH",
        vec![string_param("Button"), int_param(seconds)],
    )
}

#[test]
fn a_cameo_flash_runs_three_flashes_a_second_rounded_up_to_even() {
    for (seconds, flashes) in [(2, 6), (3, 10)] {
        let view = view_of(vec![cameo(seconds)]);
        let flash = view.hud.ui_flashes.last().unwrap();
        assert_eq!(flash.target, UiFlashTarget::Cameo("Button".into()));
        assert_eq!(flash.amount, flashes, "{seconds} s");
    }
}

fn display(verb: &str) -> ScriptAction {
    action(
        verb,
        vec![string_param("Clock"), string_param("LABEL:Clock")],
    )
}

#[test]
fn a_displayed_timer_follows_its_counter_until_hidden_or_the_display_is_disabled() {
    let timer = |extra: Vec<ScriptAction>| {
        let mut actions = vec![
            action("SET_TIMER", vec![string_param("Clock"), int_param(100)]),
            display("DISPLAY_COUNTDOWN_TIMER"),
        ];
        actions.extend(extra);
        view_of(actions).hud.timers
    };
    let shown = timer(vec![]);
    assert_eq!(shown.len(), 1);
    assert!(shown[0].countdown && shown[0].label == "LABEL:Clock");
    assert!(shown[0].value > 0 && shown[0].value <= 100);
    assert!(timer(vec![action(
        "HIDE_COUNTDOWN_TIMER",
        vec![string_param("Clock")]
    )])
    .is_empty());
    assert!(timer(vec![action("DISABLE_COUNTDOWN_TIMER_DISPLAY", vec![])]).is_empty());
    let counter = view_of(vec![
        action("SET_COUNTER", vec![string_param("Clock"), int_param(7)]),
        display("DISPLAY_COUNTER"),
    ])
    .hud
    .timers;
    assert!(!counter[0].countdown && counter[0].value == 7);
}

#[test]
fn house_colour_off_and_an_in_game_movie_reach_the_view() {
    let view = view_of(vec![
        action("ENABLE_HOUSE_COLOR", vec![int_param(0)]),
        action(
            "PLAY_MOVIE_IN_GAME",
            vec![string_param("Clip"), int_param(1)],
        ),
    ]);
    assert!(view.hud.house_color_off);
    let movie = view.hud.movie.unwrap();
    assert_eq!(
        (movie.name.as_str(), movie.skippable, movie.seq),
        ("Clip", true, 1)
    );
    let on = view_of(vec![
        action("ENABLE_HOUSE_COLOR", vec![int_param(0)]),
        action("ENABLE_HOUSE_COLOR", vec![int_param(1)]),
    ]);
    assert!(!on.hud.house_color_off);
}

fn two_walkers() -> Vec<ScriptAction> {
    vec![
        named_at("A", "SpawnPoint_1"),
        named_at("B", "SpawnPoint_2"),
        action(
            "UNIT_SET_TEAM",
            vec![string_param("A"), string_param("Squad")],
        ),
        action(
            "UNIT_SET_TEAM",
            vec![string_param("B"), string_param("Squad")],
        ),
    ]
}

#[test]
fn a_team_radar_event_is_at_the_mean_of_its_members() {
    let mut actions = two_walkers();
    actions.push(action(
        "TEAM_CREATE_RADAR_EVENT",
        vec![string_param("Squad"), int_param(3)],
    ));
    let view = view_after(
        vec![
            one_shot(
                "Make",
                condition("CONDITION_TRUE", vec![]),
                actions[..2].to_vec(),
            ),
            one_shot(
                "Go",
                condition("CONDITION_TRUE", vec![]),
                actions[2..].to_vec(),
            ),
        ],
        40,
    )
    .0;
    let event = view.cine.radar.last().expect("a radar event");
    assert!((event.at.x - 35.0).abs() < 1.0, "{:?}", event.at);
    assert_eq!(event.kind, 3);
}

#[test]
fn a_named_flash_counts_five_fifteenths_of_its_seconds_and_white_is_a_flag() {
    for (seconds, count) in [(2, 0), (3, 1), (6, 2)] {
        let view = view_after(
            vec![
                one_shot(
                    "Make",
                    condition("CONDITION_TRUE", vec![]),
                    vec![named_at("A", "SpawnPoint_1")],
                ),
                one_shot(
                    "Go",
                    condition("CONDITION_TRUE", vec![]),
                    vec![action(
                        "NAMED_FLASH_WHITE",
                        vec![string_param("A"), int_param(seconds)],
                    )],
                ),
            ],
            40,
        )
        .0;
        let flash = view.hud.object_flashes.last().expect("a flash");
        assert_eq!((flash.count, flash.white), (count, true), "{seconds} s");
    }
}

pub(super) fn local_sim(nodes: Vec<ScriptNode>) -> Simulation<RotwkRules> {
    local_sim_with(nodes, WALKER_DATA, &["ZGollum"])
}

fn local_sim_with(
    nodes: Vec<ScriptNode>,
    text: &str,
    templates: &[&str],
) -> Simulation<RotwkRules> {
    let data = crate::game::tests::data_from(text);
    let seeds: Vec<String> = templates.iter().map(|t| (*t).to_owned()).collect();
    let (kinds, _) = KindTable::closure(&data, &seeds);
    let kinds = Arc::new(kinds);
    let waypoints: BTreeMap<String, (Vec2, f32)> =
        [(String::from("SpawnPoint_1"), (Vec2::new(10.0, 10.0), 0.0))]
            .into_iter()
            .collect();
    let tree = ScriptTree {
        player_slots: vec![nodes],
    };
    let engine = MapScriptEngine::build(
        &[tree],
        &[vec![String::from("Wilds")]],
        &waypoints,
        &BTreeMap::new(),
        true,
    );
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_players(2)
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            vec![String::from("Wilds")],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_script_local_player(Some(PlayerId::new(0)))
        .with_map_scripts(
            Some(engine),
            vec![(0, PlayerId::new(0)), (1, PlayerId::new(1))],
        )
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), kinds.production(&data))
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    Simulation::new(5, rules).unwrap()
}

fn selected_flag(deselect: bool) -> i32 {
    selected_flag_of(deselect, "ZGollum", WALKER_DATA)
}

const SQUAD_DATA: &str = "Object ZSquad
  Side = Neutral
  KindOf = SELECTABLE HORDE INFANTRY
    LocomotorSet
    Locomotor = ZFeet
    Condition = SET_NORMAL
    Speed = 60
  End
    Behavior = HordeContain Tag_Contain
    Slots = 2
        RankInfo = RankNumber:1 UnitType:ZGollum Position:X:0 Y:0 Position:X:10 Y:0
  End
End
";

fn selected_flag_of(deselect: bool, template: &str, text: &str) -> i32 {
    let make = one_shot(
        "Make",
        condition("CONDITION_TRUE", vec![]),
        vec![action(
            "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
            vec![
                string_param("A"),
                string_param(template),
                string_param("teamWilds"),
                string_param("SpawnPoint_1"),
            ],
        )],
    );
    let pick = one_shot(
        "Pick",
        condition("NAMED_CREATED", vec![string_param("A")]),
        vec![action(
            "SELECT_OBJECT",
            vec![string_param("<Local Player>"), string_param("A")],
        )],
    );
    let mut actions = vec![action(
        "SET_COUNTER",
        vec![string_param("Seen"), int_param(1)],
    )];
    if deselect {
        actions.insert(0, action("DESELECT", vec![]));
    }
    let probe = ScriptNode::Script(script(
        "Probe",
        true,
        true,
        vec![condition("NAMED_SELECTED", vec![string_param("A")])],
        actions,
        Vec::new(),
    ));
    let mut simulation = local_sim_with(vec![make, pick, probe], text, &[template]);
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    let ids = simulation.rules().ids().unwrap();
    let state: crate::rules::map_scripts::MapScriptState = simulation
        .state()
        .store(ids.map_script_state)
        .and_then(|store| store.iter().next().map(|(_, s)| s.clone()))
        .unwrap();
    let held = simulation.state().store(ids.selected).map_or(0, |store| {
        store
            .iter()
            .filter(|(_, mask)| mask.holds(PlayerId::new(0)))
            .count()
    });
    let held = i32::try_from(held).unwrap_or(0);
    state.counter_value("Wilds", "Seen") + state.counter_value("", "Seen") + 10 * held
}

#[test]
fn select_object_makes_named_selected_true_and_deselect_clears_it() {
    assert_eq!(selected_flag(false), 1 + 10);
    assert_eq!(selected_flag(true), 1);
}

#[test]
fn a_selected_horde_answers_named_selected_by_its_own_name() {
    let text = format!("{WALKER_DATA}{SQUAD_DATA}");
    let got = selected_flag_of(false, "ZSquad", &text);
    assert_eq!(got % 10, 1, "{got}");
}
