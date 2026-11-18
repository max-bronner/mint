use super::*;

const DATA: &str = "\
Object Gate\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE WALL_SEGMENT\n  \
  Behavior = FakePathfindPortalBehaviour Tag_Portal\n  End\n  \
  Behavior = AIGateUpdate Tag_Trigger\n    TriggerWidthX = 10\n    TriggerWidthY = 10\n  End\n  \
  Behavior = GateOpenAndCloseBehavior Tag_Cycle\n    OpenByDefault = No\n    \
    ResetTimeInMilliseconds = 1000\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 10\n  GeometryMinorRadius = 20\n  GeometryHeight = 20\nEnd\n";

fn setup() -> (Simulation<RotwkRules>, Ids, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(&data, &["Gate".to_owned()]);
    assert!(missing.is_empty(), "template(s) not resolved: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let gate = kinds
        .spawn(
            &data.combat,
            kinds.kind("Gate").unwrap(),
            Vec2::new(50.0, 50.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(60, 60), 5.0 / 128.0).unwrap();
    let mut simulation = Simulation::new(
        71,
        RotwkRules::new(passability, vec![gate])
            .with_combat(data.combat.clone())
            .with_construction(Arc::clone(&kinds), production),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let gate_entity = simulation.state().store(ids.body).unwrap().entities()[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![gate_entity],
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    (simulation, ids, gate_entity)
}

fn door_condition(simulation: &Simulation<RotwkRules>, entity: Entity, flag: &str) -> bool {
    let Some(state_id) = simulation.rules().upgrade_state_id() else {
        return false;
    };
    simulation
        .state()
        .get(state_id, entity)
        .is_some_and(|held| held.has_model_condition(flag))
}

#[test]
fn a_human_gates_toggle_button_holds_until_the_next_press() {
    let (mut simulation, _ids, gate) = setup();
    assert!(
        !door_condition(&simulation, gate, "DOOR_1_OPENING"),
        "resting closed before any press"
    );
    simulation
        .submit(
            PLAYER,
            RotwkCommand::GateCommand {
                action: crate::rules::GateAction::Toggle,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert!(
        door_condition(&simulation, gate, "DOOR_1_OPENING"),
        "the first press starts opening"
    );
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    assert!(
        !door_condition(&simulation, gate, "DOOR_1_CLOSING"),
        "no second press, no box occupant - it must not close itself again"
    );
    simulation
        .submit(
            PLAYER,
            RotwkCommand::GateCommand {
                action: crate::rules::GateAction::Toggle,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert!(
        door_condition(&simulation, gate, "DOOR_1_CLOSING"),
        "the second press starts closing"
    );
}

#[test]
fn a_press_mid_move_is_ignored_until_the_gate_finishes_its_cycle() {
    let (mut simulation, _ids, gate) = setup();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::GateCommand {
                action: crate::rules::GateAction::Open,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert!(
        door_condition(&simulation, gate, "DOOR_1_OPENING"),
        "the press starts opening"
    );

    simulation
        .submit(
            PLAYER,
            RotwkCommand::GateCommand {
                action: crate::rules::GateAction::Close,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert!(
        !door_condition(&simulation, gate, "DOOR_1_CLOSING"),
        "a press mid-move is ignored"
    );

    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    assert!(
        !door_condition(&simulation, gate, "DOOR_1_CLOSING"),
        "the ignored press never takes effect once the gate is open"
    );
}

#[test]
fn an_ai_gates_box_entry_mid_close_does_not_reverse_it() {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        "\
Object Gate\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE WALL_SEGMENT\n  \
  Behavior = FakePathfindPortalBehaviour Tag_Portal\n  End\n  \
  Behavior = AIGateUpdate Tag_Trigger\n    TriggerWidthX = 40\n    TriggerWidthY = 40\n  End\n  \
  Behavior = GateOpenAndCloseBehavior Tag_Cycle\n    OpenByDefault = No\n    \
    ResetTimeInMilliseconds = 3000\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 10\n  GeometryMinorRadius = 20\n  GeometryHeight = 20\nEnd\n\
Object Mover\n  KindOf = SELECTABLE INFANTRY\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 6\n  GeometryHeight = 16\nEnd\n",
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(&data, &["Gate".to_owned(), "Mover".to_owned()]);
    assert!(missing.is_empty(), "template(s) not resolved: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let gate_spawn = kinds
        .spawn(
            &data.combat,
            kinds.kind("Gate").unwrap(),
            Vec2::new(50.0, 50.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let mover_kind = kinds.kind("Mover").unwrap();
    let mover_a = kinds
        .spawn(
            &data.combat,
            mover_kind,
            Vec2::new(65.0, 50.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let mover_b = kinds
        .spawn(
            &data.combat,
            mover_kind,
            Vec2::new(65.0, 125.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(60, 60), 5.0 / 128.0).unwrap();
    let mut simulation = Simulation::new(
        73,
        RotwkRules::new(passability, vec![gate_spawn, mover_a, mover_b])
            .with_combat(data.combat.clone())
            .with_construction(Arc::clone(&kinds), production)
            .with_skirmish_ai(
                vec![crate::skirmish_ai::AiSeed {
                    player: PLAYER,
                    candidates: Vec::new(),
                }],
                crate::combat::AiTiming::default(),
                Vec::new(),
            ),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    let (gate, mover_a, mover_b) = (entities[0], entities[1], entities[2]);

    simulation.tick().unwrap();
    assert!(
        door_condition(&simulation, gate, "DOOR_1_OPENING"),
        "mover_a starts in the box: the gate opens"
    );
    for _ in 0..90 {
        simulation.tick().unwrap();
    }

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![mover_a],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 65.0,
                y: 200.0,
                queue: false,
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![mover_b],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 65.0,
                y: 50.0,
                queue: false,
            },
        )
        .unwrap();

    let gate_at = Vec2::new(50.0, 50.0);
    let mut closing_seen = false;
    let mut cross_tick: Option<u32> = None;
    for i in 0..200u32 {
        simulation.tick().unwrap();
        closing_seen |= door_condition(&simulation, gate, "DOOR_1_CLOSING");
        if cross_tick.is_none() {
            let b_in = simulation
                .state()
                .get(ids.body, mover_b)
                .is_some_and(|body| body.0.position.minus(gate_at).length() <= 20.0);
            if b_in {
                cross_tick = Some(i);
            }
        }
        if cross_tick.is_some_and(|ct| i < ct + 20) {
            assert!(
                !door_condition(&simulation, gate, "DOOR_1_OPENING"),
                "a box entry mid-close must not reverse the gate"
            );
        }
    }
    assert!(closing_seen, "mover_a leaving never asked the gate shut");
    assert!(
        cross_tick.is_some(),
        "mover_b never reached the box - test is vacuous"
    );
    assert!(
        door_condition(&simulation, gate, "DOOR_1_OPENING"),
        "at rest with mover_b inside, the close finished and the box re-opened it"
    );
}

#[test]
fn a_gate_under_construction_ignores_toggle_gate() {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        "\
Object Porter\n  KindOf = SELECTABLE INFANTRY DOZER\n  CommandSet = PorterSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 200\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 8\n  GeometryHeight = 16\nEnd\n\
CommandSet PorterSet\n  1 = Build_GateSite\nEnd\n\
CommandButton Build_GateSite\n  Command = DOZER_CONSTRUCT\n  Object = GateSite\nEnd\n\
Object GateSite\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE WALL_SEGMENT\n  \
  BuildCost = 100\n  BuildTime = 50\n  \
  Behavior = FakePathfindPortalBehaviour Tag_Portal\n  End\n  \
  Behavior = AIGateUpdate Tag_Trigger\n    TriggerWidthX = 10\n    TriggerWidthY = 10\n  End\n  \
  Behavior = GateOpenAndCloseBehavior Tag_Cycle\n    OpenByDefault = No\n    \
    ResetTimeInMilliseconds = 1000\n  End\n  \
  Body = StructureBody Tag_02\n    MaxHealth = 200\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 10\n  GeometryMinorRadius = 20\n  GeometryHeight = 20\nEnd\n",
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(&data, &["Porter".to_owned(), "GateSite".to_owned()]);
    assert!(missing.is_empty(), "template(s) not resolved: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let gate_kind = kinds.kind("GateSite").unwrap();
    let porter = kinds
        .spawn(
            &data.combat,
            kinds.kind("Porter").unwrap(),
            Vec2::new(250.0, 250.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(60, 60), 5.0 / 128.0).unwrap();
    let mut simulation = Simulation::new(
        79,
        RotwkRules::new(passability, vec![porter])
            .with_combat(data.combat.clone())
            .with_construction(Arc::clone(&kinds), production)
            .with_economy(Vec::new(), 10_000),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let porter_entity = simulation.state().store(ids.body).unwrap().entities()[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![porter_entity],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::Construct {
                kind: gate_kind.0,
                x: 250.0,
                y: 250.0,
                angle: 0.0,
            },
        )
        .unwrap();
    let mut site = None;
    for _ in 0..600 {
        simulation.tick().unwrap();
        if let Some(entity) = simulation
            .state()
            .store(ids.construction)
            .and_then(|store| store.entities().first().copied())
        {
            site = Some(entity);
            break;
        }
    }
    let site = site.expect("the porter reached the spot and the site appeared");
    assert!(
        simulation.state().contains(ids.construction, site),
        "still mid-build"
    );

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![site],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::GateCommand {
                action: crate::rules::GateAction::Toggle,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert!(
        !door_condition(&simulation, site, "DOOR_1_OPENING"),
        "a press on a site under construction is ignored"
    );
    assert!(!door_condition(&simulation, site, "DOOR_1_CLOSING"));
}

#[test]
fn a_shut_gate_rests_on_door_closing_and_greys_its_buttons_mid_move() {
    let (mut simulation, ids, gate) = setup();
    assert!(
        door_condition(&simulation, gate, "DOOR_1_CLOSING"),
        "a gate at rest shut reads DOOR_1_CLOSING before any press"
    );
    let buttons = simulation
        .rules()
        .gate_buttons(simulation.state(), ids, gate)
        .unwrap();
    assert!(buttons.may_operate && !buttons.open);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::GateCommand {
                action: crate::rules::GateAction::Toggle,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let moving = simulation
        .rules()
        .gate_buttons(simulation.state(), ids, gate)
        .unwrap();
    assert!(!moving.may_operate, "greyed while it opens");
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    let open = simulation
        .rules()
        .gate_buttons(simulation.state(), ids, gate)
        .unwrap();
    assert!(open.may_operate && open.open);
    assert!(
        door_condition(&simulation, gate, "DOOR_1_OPENING")
            && !door_condition(&simulation, gate, "DOOR_1_CLOSING"),
        "standing open reads DOOR_1_OPENING alone"
    );
}

#[test]
fn an_open_map_players_gate_admits_its_owner_and_neutral_players_but_not_enemies() {
    let owner = PlayerId::new(1);
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, _) = KindTable::closure(&data, &["Gate".to_owned()]);
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let gate = kinds
        .spawn(
            &data.combat,
            kinds.kind("Gate").unwrap(),
            Vec2::new(50.0, 50.0),
            Facing::EAST,
            owner,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(60, 60), 5.0 / 128.0).unwrap();
    let relations = |enemies: Vec<PlayerId>| crate::alliance::MapRelations {
        enemies,
        ..Default::default()
    };
    let mut simulation = Simulation::new(
        71,
        RotwkRules::new(passability, vec![gate])
            .with_players(1)
            .with_teams(vec![None])
            .with_map_players(
                1,
                vec![String::from("Civ"), String::from("Foe")],
                vec![relations(vec![PlayerId::new(2)]), relations(Vec::new())],
            )
            .with_combat(data.combat.clone())
            .with_construction(Arc::clone(&kinds), production),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let gate_entity = simulation.state().store(ids.body).unwrap().entities()[0];
    simulation
        .submit(
            owner,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![gate_entity],
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    simulation
        .submit(
            owner,
            RotwkCommand::GateCommand {
                action: crate::rules::GateAction::Toggle,
            },
        )
        .unwrap();
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    let stamped = crate::portals::masks_of(simulation.state(), ids.portals)
        .iter()
        .fold(0, |all, mask| all | mask);
    assert_eq!(stamped, 0b011);
}
