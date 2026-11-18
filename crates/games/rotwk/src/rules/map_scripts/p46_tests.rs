use mint_sage_map::Parameter;
use mint_sim::Simulation;

use super::w3a_tests::{
    act, after, counter_of, counting, named, number, once, simulate, simulate_data, spawn,
    state_of, text, DATA,
};
use super::w3b_tests as w3b;
use crate::rules::RotwkRules;

fn health_share(percent: f32, comparison: i32, value: i32) -> i32 {
    let sim = simulate(
        vec![
            once("Make", vec![spawn("U", "ZLight")]),
            after(
                "Hurt",
                "U",
                vec![act(
                    "TEAM_SET_HEALTH",
                    vec![text("teamPlayer_1"), super::w3a_tests::real(percent)],
                )],
            ),
            counting(
                "seen",
                (
                    "UNIT_HEALTH",
                    vec![text("U"), number(comparison), number(value)],
                ),
            ),
        ],
        60,
    );
    counter_of(&sim, "seen")
}

#[test]
fn unit_health_reads_the_truncated_percent_of_max_health() {
    assert_eq!(health_share(66.7, 2, 66), 1);
    assert_eq!(health_share(66.7, 2, 67), 0);
    assert_eq!(health_share(66.7, 0, 67), 1);
    assert_eq!(health_share(66.7, 9, 66), 0);
    let missing = simulate(
        vec![counting(
            "seen",
            ("UNIT_HEALTH", vec![text("Nobody"), number(3), number(0)]),
        )],
        30,
    );
    assert_eq!(counter_of(&missing, "seen"), 0);
}

#[test]
fn a_named_unit_receives_an_object_upgrade() {
    let data = format!("{DATA}Upgrade ZUp\n  Type = OBJECT\nEnd\n");
    let sim: Simulation<RotwkRules> = simulate_data(
        &data,
        vec![
            once("Make", vec![spawn("U", "ZLight")]),
            after(
                "Give",
                "U",
                vec![act("NAMED_RECEIVE_UPGRADE", vec![text("U"), text("ZUp")])],
            ),
        ],
        60,
    );
    let ids = sim.rules().ids().unwrap();
    let held = sim.state().get(ids.object_upgrades, named(&sim, "U"));
    assert!(held.is_some_and(|held| !held.0.is_empty()));
}

#[test]
fn the_border_shroud_pair_and_screen_shake_leave_view_state() {
    let run = |actions: Vec<(&str, Vec<Parameter>)>| {
        let mut body: Vec<_> = actions.into_iter().map(|(n, p)| act(n, p)).collect();
        body.push(act("INCREMENT_COUNTER", vec![number(1), text("ran")]));
        let sim = simulate(vec![once("Go", body)], 20);
        (counter_of(&sim, "ran"), state_of(&sim).view().effects)
    };
    let (ran, effects) = run(vec![("DISABLE_BORDER_SHROUD", vec![])]);
    assert_eq!((ran, effects.border_clear), (1, true));
    let (ran, effects) = run(vec![
        ("DISABLE_BORDER_SHROUD", vec![]),
        ("ENABLE_BORDER_SHROUD", vec![]),
    ]);
    assert_eq!((ran, effects.border_clear), (1, false));
    let (ran, effects) = run(vec![("SCREEN_SHAKE", vec![number(2)])]);
    assert_eq!((ran, effects.shake_seq, effects.shake_kind), (1, 1, 2));
}

#[test]
fn a_command_bar_button_is_added_only_within_slots_one_to_thirty_three() {
    let data = DATA.replace(
        "Object ZLight
  Side = Neutral
",
        "Object ZLight
  Side = Neutral
  CommandSet = ZSet
",
    ) + "CommandSet ZSet
  1 = ZBtn
End
";
    let added = |slot: i32| {
        let sim = simulate_data(
            &data,
            vec![once(
                "Go",
                vec![
                    act(
                        "COMMANDBAR_ADD_BUTTON_OBJECTTYPE_SLOT",
                        vec![text("ZNew"), text("ZLight"), number(slot)],
                    ),
                    act("INCREMENT_COUNTER", vec![number(1), text("ran")]),
                ],
            )],
            20,
        );
        assert_eq!(counter_of(&sim, "ran"), 1);
        state_of(&sim).view().hud.added_buttons
    };
    let held = added(5);
    assert_eq!(
        held.iter()
            .map(|b| (b.set.as_str(), b.slot, b.button.as_str()))
            .collect::<Vec<_>>(),
        [("ZSet", 5, "ZNew")]
    );
    assert!(added(0).is_empty());
    assert!(added(34).is_empty());
}

#[test]
fn the_most_cost_power_fires_the_ready_caster_at_the_enemy() {
    let data = format!(
        "{}Object Rich
  Side = Men
  KindOf = STRUCTURE SELECTABLE
  BuildCost = 500
           Body = StructureBody Tag_Body
    MaxHealth = 1000
  End
           Geometry = BOX
  GeometryMajorRadius = 10
  GeometryMinorRadius = 10
  GeometryHeight = 10
End
",
        w3b::DATA
    );
    let objects = [
        w3b::placed("Zapper", "ZapA", (50.0, 50.0), w3b::P0),
        w3b::placed("Rich", "Keep", (120.0, 100.0), w3b::P1),
    ];
    let fire = |player: &str| {
        vec![w3b::script(
            "Go",
            None,
            vec![act(
                "SKIRMISH_FIRE_SPECIAL_POWER_AT_MOST_COST",
                vec![w3b::text(player), w3b::text("SpecialAbilityZap")],
            )],
        )]
    };
    let fired = w3b::simulate(&data, &objects, fire("Player_1"), 20);
    assert!(w3b::available_on(&fired, w3b::named(&fired, "ZapA")) > 0);
    let idle = w3b::simulate(&data, &objects, fire("Player_2"), 20);
    assert_eq!(w3b::available_on(&idle, w3b::named(&idle, "ZapA")), 0);
}
