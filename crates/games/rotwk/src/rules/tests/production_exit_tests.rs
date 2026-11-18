use super::*;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Object ZBarracks\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE AUTO_RALLYPOINT\n  \
  CommandSet = ZBarracksSet\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Behavior = QueueProductionExitUpdate Tag_Exit\n    UnitCreatePoint = X:20 Y:0 Z:0\n    \
    NaturalRallyPoint = X:150 Y:0 Z:0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
Object ZYard\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 60\n  GeometryMinorRadius = 60\n  GeometryHeight = 1\nEnd\n\
CommandSet ZBarracksSet\n  1 = Build_ZSoldier\n  2 = Build_ZBird\nEnd\n\
CommandButton Build_ZSoldier\n  Command = UNIT_BUILD\n  Object = ZSoldier\n  Radial = Yes\nEnd\n\
CommandButton Build_ZBird\n  Command = UNIT_BUILD\n  Object = ZBird\n  Radial = Yes\nEnd\n\
Locomotor ZBirdLocomotor\n  Surfaces = AIR\n  Appearance = GIANT_BIRD\n  Acceleration = 400\n  \
  Braking = 1000\n  TurnTime = 2000\n  PreferredHeight = 60\n  LookAheadMult = 2.0\n  Lift = 100%\n  \
  ZAxisBehavior = SURFACE_RELATIVE_HEIGHT\nEnd\n\
Object ZBird\n  Side = Men\n  KindOf = MONSTER SELECTABLE\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  LocomotorSet\n    Locomotor = ZBirdLocomotor\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Behavior = GiantBirdAIUpdate Tag_AI\n    FollowThroughDistance = 120\n    \
    FollowThroughCheckStep = 20\n    FollowThroughGradient = 1.0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 16\nEnd\n\
Object ZTitan\n  Side = Men\n  KindOf = MONSTER PATH_THROUGH_INFANTRY SELECTABLE\n  CrushableLevel = 4\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 12\n  GeometryHeight = 30\nEnd\n\
Object ZSoldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

struct Match {
    data: GameData,
    kinds: Arc<KindTable>,
}

fn load() -> Match {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["ZBarracks", "ZSoldier", "ZYard", "ZBird", "ZTitan"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    Match {
        data,
        kinds: Arc::new(kinds),
    }
}

fn rules(m: &Match, placed: &[(&str, f32, f32, PlayerId)]) -> RotwkRules {
    let production = m.kinds.production(&m.data);
    let footprints = placed
        .iter()
        .enumerate()
        .map(|(at, (name, ..))| {
            let record = &m.kinds.get(m.kinds.kind(name).unwrap()).unwrap().record;
            (at, crate::construction::footprint_of(record))
        })
        .collect();
    let spawns: Vec<Spawn> = placed
        .iter()
        .map(|(name, x, y, owner)| {
            let kind = m.kinds.kind(name).unwrap();
            let mut spawn = m
                .kinds
                .spawn(
                    &m.data.combat,
                    kind,
                    Vec2::new(*x, *y),
                    Facing::EAST,
                    *owner,
                )
                .unwrap();
            spawn.producer = m
                .kinds
                .producer(&production, kind, Vec2::new(*x, *y), Facing::EAST);
            spawn
        })
        .collect();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    RotwkRules::new(passability, spawns)
        .with_structure_footprints(footprints)
        .with_combat(m.data.combat.clone())
        .with_construction(Arc::clone(&m.kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
}

fn placed(simulation: &Simulation<RotwkRules>, ids: Ids, m: &Match, name: &str) -> Vec<Entity> {
    let kind = m.kinds.kind(name).unwrap();
    simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(entity, k)| **k == kind && simulation.state().is_live(*entity))
        .map(|(entity, _)| entity)
        .collect()
}

fn produce_one(
    m: &Match,
    placed_with: &[(&str, f32, f32, PlayerId)],
) -> (Simulation<RotwkRules>, Ids, Vec2, Entity) {
    produce_button(m, placed_with, 0, None)
}

fn produce_button(
    m: &Match,
    placed_with: &[(&str, f32, f32, PlayerId)],
    button: u8,
    rally: Option<Vec2>,
) -> (Simulation<RotwkRules>, Ids, Vec2, Entity) {
    let mut simulation = Simulation::new(41, rules(m, placed_with)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let barracks = placed(&simulation, ids, m, "ZBarracks")[0];
    let home = position(&simulation, ids, barracks);
    if let Some(at) = rally {
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetRallyPoint {
                    structures: vec![barracks],
                    x: at.x,
                    y: at.y,
                    global: false,
                    target: None,
                },
            )
            .unwrap();
    }
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![barracks],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::QueueUnit { button })
        .unwrap();
    let before: Vec<Entity> = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    let mut unit = None;
    for _ in 0..90 {
        simulation.tick().unwrap();
        if let Some(new) = simulation
            .state()
            .store(ids.body)
            .unwrap()
            .entities()
            .iter()
            .copied()
            .find(|entity| !before.contains(entity))
        {
            unit = Some(new);
            break;
        }
    }
    let unit = unit.expect("the soldier was built");
    (simulation, ids, home, unit)
}

fn nearest_to(
    simulation: &mut Simulation<RotwkRules>,
    ids: Ids,
    unit: Entity,
    rally: Vec2,
    ticks: u32,
) -> f32 {
    let mut nearest = f32::MAX;
    for _ in 0..ticks {
        simulation.tick().unwrap();
        nearest = nearest.min(position(simulation, ids, unit).minus(rally).length());
    }
    nearest
}

#[test]
fn a_produced_unit_appears_at_the_create_point_and_holds_a_route_to_the_rally_point() {
    let m = load();
    let (mut simulation, ids, home, unit) = produce_one(&m, &[("ZBarracks", 0.0, 0.0, PLAYER)]);
    let first = position(&simulation, ids, unit).minus(home);
    assert!(
        first.minus(Vec2::new(20.0, 0.0)).length() < 5.0,
        "appeared at {first:?} from the barracks, expected its create point"
    );
    assert!(
        simulation.state().contains(ids.path, unit),
        "holds a route out to the rally point"
    );
    let rally = home.plus(Vec2::new(150.0, 0.0));
    let nearest = nearest_to(&mut simulation, ids, unit, rally, 150);
    assert!(
        nearest < 10.0,
        "came only {nearest:.0} from the rally point"
    );
}

#[test]
fn a_produced_unit_walks_out_of_a_footprint_nested_in_another() {
    let m = load();
    let (mut simulation, ids, home, unit) = produce_one(
        &m,
        &[("ZYard", 0.0, 0.0, PLAYER), ("ZBarracks", 0.0, 0.0, PLAYER)],
    );
    let rally = home.plus(Vec2::new(150.0, 0.0));
    let nearest = nearest_to(&mut simulation, ids, unit, rally, 150);
    assert!(
        nearest < 10.0,
        "came only {nearest:.0} from the rally point"
    );
}

#[test]
fn a_produced_giant_bird_flies_its_exit_route_point_by_point_and_stops_leaving_the_factory() {
    let m = load();
    let player = Vec2::new(150.0, 200.0);
    let (mut simulation, ids, home, bird) =
        produce_button(&m, &[("ZBarracks", 0.0, 0.0, PLAYER)], 1, Some(player));
    let natural = home.plus(Vec2::new(150.0, 0.0));
    let leaving = |simulation: &Simulation<RotwkRules>| {
        simulation
            .rules()
            .carries_object_status(simulation.state(), bird, "IS_LEAVING_FACTORY")
    };
    assert!(leaving(&simulation), "marked on creation");
    let mut nearest = f32::MAX;
    for _ in 0..(40 * SIMULATION_HZ) {
        simulation.tick().unwrap();
        nearest = nearest.min(position(&simulation, ids, bird).minus(natural).length());
        if !simulation.state().contains(ids.path, bird) {
            break;
        }
        assert!(leaving(&simulation), "still on its exit flight");
    }
    assert!(nearest < 40.0, "passed the natural rally {nearest:.0} away");
    assert!(!leaving(&simulation), "the exit flight ended");
    let off = position(&simulation, ids, bird).minus(player).length();
    assert!(off < 60.0, "ended {off:.0} from the player rally");
}

#[test]
fn an_allied_pusher_skips_a_unit_walking_out_of_its_factory_and_pushes_it_after() {
    let m = load();
    let (mut simulation, ids, _home, unit) = produce_one(
        &m,
        &[
            ("ZBarracks", 200.0, 300.0, PLAYER),
            ("ZTitan", 600.0, 300.0, PLAYER),
        ],
    );
    let titan = placed(&simulation, ids, &m, "ZTitan")[0];
    let walk = |simulation: &mut Simulation<RotwkRules>, to: Vec2| {
        for command in [
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![titan],
            },
            RotwkCommand::MoveTo {
                x: to.x,
                y: to.y,
                queue: false,
            },
        ] {
            simulation.submit(PLAYER, command).unwrap();
        }
        simulation.tick().unwrap();
    };
    let aside =
        |simulation: &Simulation<RotwkRules>| simulation.state().contains(ids.ai_busy, unit);
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    let goal = simulation.state().get(ids.path, unit).unwrap().goal;
    assert!(
        simulation
            .rules()
            .carries_object_status(simulation.state(), unit, "IS_LEAVING_FACTORY"),
        "still leaving"
    );
    println!(
        "DBG unit at {:?} titan at {:?}",
        position(&simulation, ids, unit),
        position(&simulation, ids, titan)
    );
    walk(&mut simulation, Vec2::new(280.0, 300.0));
    println!(
        "DBG titan path {:?}",
        simulation
            .state()
            .get(ids.path, titan)
            .map(|p| p.route.points.clone())
    );
    assert!(!aside(&simulation), "pushed during its exit walk");
    assert_eq!(
        simulation.state().get(ids.path, unit).map(|path| path.goal),
        Some(goal),
        "its exit route was replaced"
    );
    for _ in 0..300 {
        simulation.tick().unwrap();
    }
    assert!(!simulation.state().contains(ids.path, unit));
    assert!(!simulation.rules().carries_object_status(
        simulation.state(),
        unit,
        "IS_LEAVING_FACTORY"
    ));
    walk(&mut simulation, Vec2::new(600.0, 300.0));
    assert!(aside(&simulation), "not pushed once its exit walk ended");
}
