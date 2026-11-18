use super::*;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Object ZBarracks\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  CommandSet = ZBarracksSet\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Behavior = QueueProductionExitUpdate Tag_Exit\n    UnitCreatePoint = X:20 Y:0 Z:0\n    \
    NaturalRallyPoint = X:150 Y:0 Z:0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
Object ZYard\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 60\n  GeometryMinorRadius = 60\n  GeometryHeight = 1\nEnd\n\
CommandSet ZBarracksSet\n  1 = Build_ZSoldier\nEnd\n\
CommandButton Build_ZSoldier\n  Command = UNIT_BUILD\n  Object = ZSoldier\n  Radial = Yes\nEnd\n\
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
    let seeds: Vec<String> = ["ZBarracks", "ZSoldier", "ZYard"]
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
    let mut simulation = Simulation::new(41, rules(m, placed_with)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let barracks = placed(&simulation, ids, m, "ZBarracks")[0];
    let home = position(&simulation, ids, barracks);
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
        .submit(PLAYER, RotwkCommand::QueueUnit { button: 0 })
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
