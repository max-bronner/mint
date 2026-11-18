use super::*;

const DATA: &str = "\
Locomotor ZFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 200\nEnd\n\
Object ZGollum\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 4\n  GeometryHeight = 10\nEnd\n\
Object ZTower\n  Side = Neutral\n  KindOf = STRUCTURE SELECTABLE IMMOBILE GARRISON\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = HordeGarrisonContain Tag_Garrison\n    \
    ObjectStatusOfContained = UNSELECTABLE CAN_ATTACK ENCLOSED\n    ContainMax = 2\n    \
    PassengerFilter = ANY +INFANTRY\n    AllowOwnPlayerInsideOverride = Yes\n    \
    EntryOffset = X:30.0 Y:0.0 Z:0.0\n    EntryPosition = X:0.0 Y:0.0 Z:0.0\n    \
    ExitOffset = X:30.0 Y:0.0 Z:0.0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  \
  GeometryHeight = 60\nEnd\n";

fn world(scripts: Vec<Script>, ticks: u32) -> Simulation<RotwkRules> {
    let data = crate::game::tests::data_from(DATA);
    let names = ["ZGollum", "ZTower"].map(String::from);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let waypoints: BTreeMap<String, (Vec2, f32)> = [
        ("Inside", (Vec2::new(10.0, 10.0), 0.0)),
        ("Outside", (Vec2::new(60.0, 10.0), 0.0)),
        ("North", (Vec2::new(10.0, 80.0), 0.0)),
        ("Corner", (Vec2::new(4.0, 26.0), 0.0)),
    ]
    .into_iter()
    .map(|(name, at)| (name.to_owned(), at))
    .collect();
    let areas: BTreeMap<String, Vec<(f32, f32)>> = [(
        String::from("Zone"),
        vec![(0.0, 0.0), (30.0, 0.0), (30.0, 30.0), (0.0, 30.0)],
    )]
    .into_iter()
    .collect();
    let tree = ScriptTree {
        player_slots: vec![scripts.into_iter().map(ScriptNode::Script).collect()],
    };
    let engine = MapScriptEngine::build(&[tree], &[], &waypoints, &areas, false)
        .with_team_prototypes(
            TeamProtos::from_defs(&[team_def("teamWilds", true, Vec::new())]).unwrap(),
        );
    let rules = rules_over(128)
        .with_players(2)
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            vec![String::from("Wilds")],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_map_scripts(
            Some(engine),
            vec![(0, PlayerId::new(0)), (1, PlayerId::new(1))],
        )
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), kinds.production(&data))
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(5, rules).unwrap();
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
    simulation
}

fn counter(simulation: &Simulation<RotwkRules>, name: &str) -> i32 {
    let ids = simulation.rules().ids().unwrap();
    simulation
        .state()
        .store(ids.map_script_state)
        .and_then(|store| {
            store
                .iter()
                .next()
                .map(|(_, state)| state.counter_value("", name))
        })
        .unwrap()
}

fn named(simulation: &Simulation<RotwkRules>, name: &str) -> Entity {
    let ids = simulation.rules().ids().unwrap();
    let world = simulation.state();
    world
        .store(ids.script_name)
        .and_then(|store| {
            store
                .iter()
                .find(|(_, tag): &(Entity, &ScriptObjectName)| tag.0 == name)
                .map(|(entity, _)| entity)
        })
        .unwrap()
}

fn make(name: &str, template: &str, at: &str) -> ScriptAction {
    act(
        "CREATE_NAMED_ON_TEAM_AT_WAYPOINT",
        vec![text(name), text(template), text("teamWilds"), text(at)],
    )
}

fn once(
    name: &str,
    condition: Option<(&str, Vec<Parameter>)>,
    actions: Vec<ScriptAction>,
) -> Script {
    script_of(name, true, true, condition, actions)
}

fn every_frame(name: &str, condition: (&str, Vec<Parameter>), counter: &str) -> Script {
    script_of(
        name,
        true,
        false,
        Some(condition),
        vec![act("INCREMENT_COUNTER", vec![number(1), text(counter)])],
    )
}

fn walker_crossing_scripts() -> Vec<Script> {
    vec![
        once("Make", None, vec![make("Walker", "ZGollum", "Outside")]),
        once(
            "In",
            Some(("NAMED_CREATED", vec![text("Walker")])),
            vec![act(
                "MOVE_NAMED_UNIT_TO",
                vec![text("Walker"), text("Inside")],
            )],
        ),
        every_frame(
            "Entered",
            ("NAMED_ENTERED_AREA", vec![text("Walker"), text("Zone")]),
            "entered",
        ),
        every_frame(
            "Exited",
            ("NAMED_EXITED_AREA", vec![text("Walker"), text("Zone")]),
            "exited",
        ),
        once(
            "Out",
            Some(("NAMED_ENTERED_AREA", vec![text("Walker"), text("Zone")])),
            vec![act(
                "MOVE_NAMED_UNIT_TO",
                vec![text("Walker"), text("Outside")],
            )],
        ),
    ]
}

#[test]
fn crossing_an_area_border_is_one_event_each_way() {
    let sim = world(walker_crossing_scripts(), 600);
    assert_eq!(counter(&sim, "entered"), 1);
    assert_eq!(counter(&sim, "exited"), 1);
}

#[test]
fn a_team_entered_entirely_when_the_members_not_entering_now_already_stand_inside() {
    let scripts = vec![
        once(
            "Make",
            None,
            vec![
                make("First", "ZGollum", "Outside"),
                make("Second", "ZGollum", "North"),
            ],
        ),
        once(
            "GoFirst",
            Some(("NAMED_CREATED", vec![text("First")])),
            vec![act(
                "UNIT_TELEPORT_TO_WAYPOINT",
                vec![text("First"), text("Inside")],
            )],
        ),
        once(
            "GoSecond",
            Some(("NAMED_ENTERED_AREA", vec![text("First"), text("Zone")])),
            vec![act(
                "UNIT_TELEPORT_TO_WAYPOINT",
                vec![text("Second"), text("Corner")],
            )],
        ),
        every_frame(
            "Partly",
            (
                "TEAM_ENTERED_AREA_PARTIALLY",
                vec![text("teamWilds"), text("Zone")],
            ),
            "partly",
        ),
        every_frame(
            "Wholly",
            (
                "TEAM_ENTERED_AREA_ENTIRELY",
                vec![text("teamWilds"), text("Zone")],
            ),
            "wholly",
        ),
    ];
    let sim = world(scripts, 900);
    assert_eq!(counter(&sim, "partly"), 2, "one entry per member");
    assert_eq!(
        counter(&sim, "wholly"),
        1,
        "only the second entry finds nobody outside"
    );
}

#[test]
fn a_team_has_fewer_than_x_reads_true_only_above_x() {
    let scripts = vec![
        once(
            "Make",
            None,
            vec![
                make("A", "ZGollum", "Inside"),
                make("B", "ZGollum", "Outside"),
            ],
        ),
        every_frame(
            "AboveOne",
            (
                "TEAM_HAS_FEWER_THAN_X_UNITS_WITH_KINDOF",
                vec![text("teamWilds"), number(1), text("INFANTRY")],
            ),
            "above_one",
        ),
        every_frame(
            "AboveTwo",
            (
                "TEAM_HAS_FEWER_THAN_X_UNITS_WITH_KINDOF",
                vec![text("teamWilds"), number(2), text("INFANTRY")],
            ),
            "above_two",
        ),
    ];
    let sim = world(scripts, 20);
    assert!(counter(&sim, "above_one") > 0);
    assert_eq!(counter(&sim, "above_two"), 0);
}

#[test]
fn a_named_unit_turns_to_face_a_waypoint() {
    let scripts = vec![
        once("Make", None, vec![make("Walker", "ZGollum", "Inside")]),
        once(
            "Face",
            Some(("NAMED_CREATED", vec![text("Walker")])),
            vec![act(
                "NAMED_FACE_WAYPOINT",
                vec![text("Walker"), text("North")],
            )],
        ),
    ];
    let sim = world(scripts, 200);
    let ids = sim.rules().ids().unwrap();
    let walker = named(&sim, "Walker");
    let body = sim.state().get(ids.body, walker).unwrap();
    let wanted = core::f32::consts::FRAC_PI_2;
    assert!(
        (body.0.facing.radians() - wanted).abs() < 0.05,
        "{:?}",
        body.0.facing
    );
}

#[test]
fn a_scripted_guard_of_an_area_holds_its_centre_for_a_unit_with_no_stance() {
    let scripts = vec![
        once("Make", None, vec![make("Walker", "ZGollum", "Outside")]),
        once(
            "Guard",
            Some(("NAMED_CREATED", vec![text("Walker")])),
            vec![act("UNIT_GUARD_AREA", vec![text("Walker"), text("Zone")])],
        ),
    ];
    let sim = world(scripts, 400);
    let ids = sim.rules().ids().unwrap();
    let walker = named(&sim, "Walker");
    let guard = sim.state().get(ids.guard, walker).expect("still guarding");
    assert!(guard.scripted);
    assert_eq!((guard.point.x, guard.point.y), (15.0, 15.0));
    let at = sim.state().get(ids.body, walker).unwrap().0.position;
    assert!(at.minus(guard.point).length() < 10.0, "{at:?}");
}

#[test]
fn a_garrison_order_puts_the_unit_inside_the_named_building_at_once() {
    let scripts = vec![
        once(
            "Make",
            None,
            vec![
                make("Keep", "ZTower", "Outside"),
                make("Walker", "ZGollum", "Inside"),
            ],
        ),
        once(
            "Garrison",
            Some(("NAMED_CREATED", vec![text("Keep")])),
            vec![act(
                "NAMED_GARRISON_SPECIFIC_BUILDING_INSTANTLY",
                vec![text("Walker"), text("Keep")],
            )],
        ),
    ];
    let sim = world(scripts, 20);
    let ids = sim.rules().ids().unwrap();
    let (walker, keep) = (named(&sim, "Walker"), named(&sim, "Keep"));
    let inside = sim
        .state()
        .get(ids.passengers, keep)
        .map(|list| list.0.clone());
    assert_eq!(inside, Some(vec![walker]));
}

#[test]
fn a_script_move_goal_inside_the_map_margin_is_not_clamped() {
    let scripts = vec![
        once("Make", None, vec![make("Walker", "ZGollum", "Outside")]),
        once(
            "Go",
            Some(("NAMED_CREATED", vec![text("Walker")])),
            vec![act(
                "MOVE_NAMED_UNIT_TO",
                vec![text("Walker"), text("Corner")],
            )],
        ),
    ];
    let sim = world(scripts, 600);
    let ids = sim.rules().ids().unwrap();
    let at = sim
        .state()
        .get(ids.body, named(&sim, "Walker"))
        .unwrap()
        .0
        .position;
    assert!(
        at.x < 17.0,
        "a script goal at x 4 must not stop at the margin: {at:?}"
    );
}

#[test]
fn a_hunter_with_no_weapon_is_out_of_ammo_and_stops_hunting() {
    let scripts = vec![
        once("Make", None, vec![make("Walker", "ZGollum", "Outside")]),
        once(
            "Hunt",
            Some(("NAMED_CREATED", vec![text("Walker")])),
            vec![act("TEAM_HUNT", vec![text("teamWilds")])],
        ),
    ];
    let sim = world(scripts, 600);
    let ids = sim.rules().ids().unwrap();
    let walker = named(&sim, "Walker");
    let hunting = sim
        .state()
        .store(ids.map_script_state)
        .and_then(|store| {
            store
                .iter()
                .next()
                .map(|(_, state)| state.rt.hunters.contains(walker))
        })
        .unwrap();
    assert!(!hunting);
}
