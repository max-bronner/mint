use super::*;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Object Footman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CrushableLevel = 0\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Rider\n  Side = Men\n  KindOf = CAVALRY SELECTABLE\n  CrusherLevel = 1\n  \
  CrushableLevel = 2\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 8\n  GeometryHeight = 16\nEnd\n\
Object FootHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  CrusherLevel = 0\n  \
  BuildCost = 300\n  BuildTime = 2\n  \
  Behavior = HordeContain Tag_01\n    Slots = 4\n    \
    RankInfo = RankNumber:1 UnitType:Footman Position:X:20 Y:-15 Position:X:20 Y:15 \
Position:X:0 Y:-15 Position:X:0 Y:15\n  \
  End\nEnd\n\
Object RiderHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  CrusherLevel = 1\n  \
  BuildCost = 300\n  BuildTime = 2\n  \
  Behavior = HordeContain Tag_01\n    Slots = 4\n    \
    RankInfo = RankNumber:1 UnitType:Rider Position:X:20 Y:-15 Position:X:20 Y:15 \
Position:X:0 Y:-15 Position:X:0 Y:15\n  \
  End\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_FootHorde\n  2 = Build_RiderHorde\nEnd\n\
CommandButton Build_FootHorde\n  Command = UNIT_BUILD\n  Object = FootHorde\nEnd\n\
CommandButton Build_RiderHorde\n  Command = UNIT_BUILD\n  Object = RiderHorde\nEnd\n";

const ENEMY_AT: Vec2 = Vec2 { x: 500.0, y: 300.0 };

fn goal_of_a_move_onto_an_enemy_horde(mover: &str) -> Vec2 {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds = ["Camp", "FootHorde", "RiderHorde"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, Vec::new())
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let spawn = |simulation: &mut Simulation<RotwkRules>, label: &str, at: Vec2, owner| {
        let recipe = simulation
            .rules()
            .buildable_catalogue()
            .iter()
            .find(|entry| entry.label == label)
            .unwrap()
            .index;
        let before = simulation.state().store(ids.horde).map_or(0, |s| s.len());
        simulation
            .submit(
                owner,
                RotwkCommand::DebugSpawnHorde {
                    recipe,
                    x: at.x,
                    y: at.y,
                    facing: 0.0,
                },
            )
            .unwrap();
        simulation.tick().unwrap();
        let store = simulation.state().store(ids.horde).unwrap();
        assert_eq!(store.len(), before + 1, "{label} was not spawned");
        let horde = *store.entities().last().unwrap();
        store.get(horde).unwrap().members.clone()
    };
    spawn(&mut simulation, "FootHorde", ENEMY_AT, OTHER);
    let members = spawn(&mut simulation, mover, Vec2::new(200.0, 300.0), PLAYER);
    let horde = simulation
        .state()
        .get(ids.member, members[0])
        .unwrap()
        .horde;
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: members,
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: ENEMY_AT.x,
                y: ENEMY_AT.y,
                queue: false,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    simulation.state().get(ids.path, horde).unwrap().goal
}

#[test]
fn a_crushing_horde_ordered_onto_an_enemy_horde_keeps_its_destination() {
    let goal = goal_of_a_move_onto_an_enemy_horde("RiderHorde");
    assert!(
        goal.minus(ENEMY_AT).length() < 10.0,
        "the crusher's goal was moved off the enemy, to {goal:?}"
    );
}

#[test]
fn a_horde_that_cannot_crush_the_enemy_stops_beside_it() {
    let goal = goal_of_a_move_onto_an_enemy_horde("FootHorde");
    assert!(
        goal.minus(ENEMY_AT).length() > 30.0,
        "the goal stayed on the enemy horde, at {goal:?}"
    );
}

const NEUTRAL: PlayerId = PlayerId::new(2);

fn rules_with_a_neutral_player() -> RotwkRules {
    RotwkRules::new(
        Passability::build(&flat_map(16, 16), 5.0 / 128.0).unwrap(),
        Vec::new(),
    )
    .with_teams(vec![None, None])
    .with_map_players(
        2,
        vec!["Civilians".to_owned()],
        vec![crate::alliance::MapRelations::default()],
    )
}

fn asker(heavy: bool) -> super::super::unit_cells::DestinationAsker {
    super::super::unit_cells::DestinationAsker {
        owner: Some(PLAYER),
        crusher: 0,
        heavy,
        through_infantry: false,
    }
}

fn holder(owner: PlayerId, standing: bool, heavy: bool) -> super::super::unit_cells::GoalHolder {
    super::super::unit_cells::GoalHolder {
        owner: Some(owner),
        standing,
        crushable: 0,
        infantry: true,
        heavy,
    }
}

#[test]
fn a_neutral_units_goal_is_a_non_allys() {
    use super::super::unit_cells::GoalVerdict;
    let rules = rules_with_a_neutral_player();
    assert_eq!(
        rules.goal_verdict(asker(false), holder(NEUTRAL, false, false)),
        GoalVerdict::Clear
    );
    assert_eq!(
        rules.goal_verdict(asker(false), holder(NEUTRAL, true, false)),
        GoalVerdict::Refused
    );
    assert_eq!(
        rules.goal_verdict(asker(false), holder(PLAYER, false, false)),
        GoalVerdict::Ally
    );
}

#[test]
fn a_heavy_hitter_ignores_a_light_allys_goal() {
    use super::super::unit_cells::GoalVerdict;
    let rules = rules_with_a_neutral_player();
    assert_eq!(
        rules.goal_verdict(asker(true), holder(PLAYER, true, false)),
        GoalVerdict::Clear
    );
    assert_eq!(
        rules.goal_verdict(asker(true), holder(PLAYER, true, true)),
        GoalVerdict::Ally
    );
}
