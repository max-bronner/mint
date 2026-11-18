use super::*;
use mint_sage_formation::{FormationAssistant, FormationTemplate, Selection, UnitDefinition};

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 1000\nEnd\n\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

const ANCHOR: Vec2 = Vec2::new(500.0, 400.0);

fn assistant() -> FormationAssistant {
    FormationAssistant {
        row_padding: 10.0,
        column_padding: 10.0,
        activation_drag_distance: 0.0,
        activation_time: 0.3,
        definitions: vec![unit_definition_accepting_infantry()],
        templates: vec![FormationTemplate {
            name: "Line".into(),
            rows: vec![vec![vec![0], vec![0]]],
            last_row_of_class: vec![Some(0)],
        }],
        selections: vec![Selection {
            template: 0,
            max_drag_length: f32::MAX,
            max_units_selected: u32::MAX,
        }],
        facing_arrow_templates: [None, None, None],
    }
}

fn unit_definition_accepting_infantry() -> UnitDefinition {
    let text = "FormationAssistant\n    UnitDefinition Inf\n        ObjectFilter = NONE +INFANTRY\n    End\nEnd\n";
    let mut sources = mint_sage_ini::MemorySources::new();
    sources.insert("data/ini/formationassistant.ini", text.as_bytes().to_vec());
    let document = mint_sage_ini::Reader::new(&sources)
        .read(
            "data/ini/formationassistant.ini",
            &mint_sage_ini::BlockTable::corpus_path(),
        )
        .unwrap();
    FormationAssistant::find(document.roots())
        .unwrap()
        .unwrap()
        .definitions
        .remove(0)
}

fn dragged_pair() -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["Soldier"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns: Vec<Spawn> = [490.0f32, 410.0]
        .iter()
        .map(|y| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind("Soldier").unwrap(),
                    Vec2::new(500.0, *y),
                    Facing::EAST,
                    PLAYER,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(128, 128), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_drag_formation(Arc::new(assistant()))
        .with_teams(vec![None, None]);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: units.clone(),
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveToFormation {
                x: ANCHOR.x,
                y: ANCHOR.y,
                dx: 100.0,
                dy: 0.0,
                attack: false,
            },
        )
        .unwrap();
    for _ in 0..600 {
        simulation.tick().unwrap();
    }
    (simulation, ids, units)
}

#[test]
fn a_drag_east_places_the_pair_across_the_anchor_facing_east() {
    let (simulation, ids, units) = dragged_pair();
    let bodies: Vec<MoverState> = units
        .iter()
        .map(|u| simulation.state().get(ids.body, *u).unwrap().0)
        .collect();
    for body in &bodies {
        assert!(
            (body.position.x - ANCHOR.x).abs() < 5.0,
            "{:?} not near the anchor's x",
            body.position
        );
        assert!(
            body.facing.radians().abs() < 0.2,
            "{:?} not facing east",
            body.facing
        );
    }
    let dy = (bodies[0].position.y - bodies[1].position.y).abs();
    assert!(dy > 15.0, "the pair collapsed onto one y: {bodies:?}");
}

#[test]
fn a_drag_inside_the_selections_own_spread_tightens_with_no_facing() {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, _) = KindTable::closure(&data, &["Soldier".to_owned()]);
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns: Vec<Spawn> = [490.0f32, 510.0]
        .iter()
        .map(|y| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind("Soldier").unwrap(),
                    Vec2::new(500.0, *y),
                    Facing::EAST,
                    PLAYER,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(128, 128), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_drag_formation(Arc::new(assistant()))
        .with_teams(vec![None, None]);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: units.clone(),
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveToFormation {
                x: 500.0,
                y: 500.0,
                dx: 1.0,
                dy: 0.0,
                attack: false,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    for unit in &units {
        assert!(
            !simulation
                .state()
                .contains(ids.in_formation_template, *unit),
            "a tightened drag must not mark a facing status"
        );
    }
}

const HORDE_DATA: &str = "\
Locomotor MemberFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 500\nEnd\n\
Locomotor HordeFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 500\nEnd\n\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  LocomotorSet\n    Locomotor = MemberFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Squad\n  Side = Men\n  KindOf = SELECTABLE HORDE INFANTRY\n  BuildCost = 1\n  BuildTime = 1\n  \
  LocomotorSet\n    Locomotor = HordeFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Behavior = HordeContain Tag_Contain\n    Slots = 1\n    MeleeAttackLeashDistance = 1\n    \
    RankInfo = RankNumber:1 UnitType:Soldier Position:X:0 Y:0\n  End\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_Squad\nEnd\n\
CommandButton Build_Squad\n  Command = UNIT_BUILD\n  Object = Squad\nEnd\n";

#[test]
fn a_drag_armed_with_attack_move_ends_the_horde_facing_the_drag() {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        HORDE_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["Soldier", "Squad", "Camp"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawn = kinds
        .spawn(
            &data.combat,
            kinds.kind("Camp").unwrap(),
            Vec2::new(500.0, 490.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(128, 128), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![spawn])
        .with_players(2)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_drag_formation(Arc::new(assistant()));
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "Squad")
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 500.0,
                y: 490.0,
                facing: core::f32::consts::FRAC_PI_2,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    simulation.tick().unwrap();
    let horde = simulation
        .state()
        .store(ids.horde)
        .unwrap()
        .entities()
        .last()
        .copied()
        .unwrap();
    let members = simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .clone();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: members,
            },
        )
        .unwrap();
    const HORDE_ANCHOR: Vec2 = Vec2::new(505.0, 405.0);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveToFormation {
                x: HORDE_ANCHOR.x,
                y: HORDE_ANCHOR.y,
                dx: 100.0,
                dy: 0.0,
                attack: true,
            },
        )
        .unwrap();
    for _ in 0..600 {
        simulation.tick().unwrap();
    }
    assert!(
        !simulation.state().contains(ids.attack_move_order, horde),
        "the order should have arrived and cleared itself"
    );
    let body = simulation.state().get(ids.body, horde).unwrap().0;
    assert!(
        body.facing.radians().abs() < 0.2,
        "the horde did not end facing east: {:?}",
        body.facing
    );
}
