use super::*;

const DATA: &str = "\
Locomotor MemberFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 20000\nEnd\n\
Locomotor HordeFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 1000\n  MaxTurnWithoutReform = 45\nEnd\n\
Armor Plain\n  Armor = DEFAULT 100%\nEnd\n\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 600\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  LocomotorSet\n    Locomotor = MemberFeet\n    Condition = SET_NORMAL\n    Speed = 45\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Mounted\n  Side = Men\n  KindOf = SELECTABLE HORDE CAVALRY\n  BuildCost = 1\n  \
  BuildTime = 1\n  \
  LocomotorSet\n    Locomotor = HordeFeet\n    Condition = SET_NORMAL\n    Speed = 40\n  End\n  \
  Behavior = HorseHordeContain Tag_Contain\n    Slots = 3\n    \
    RankInfo = RankNumber:1 UnitType:Soldier Position:X:10 Y:0 Position:X:10 Y:20 \
Position:X:10 Y:-20\n  End\nEnd\n\
Object Wolfpack\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  BuildCost = 1\n  \
  BuildTime = 1\n  \
  LocomotorSet\n    Locomotor = HordeFeet\n    Condition = SET_NORMAL\n    Speed = 40\n  End\n  \
  Behavior = HorseHordeContain Tag_Contain\n    Slots = 3\n    \
    RankInfo = RankNumber:1 UnitType:Soldier Position:X:10 Y:0 Position:X:10 Y:20 \
Position:X:10 Y:-20\n  End\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_Mounted\n  2 = Build_Wolfpack\nEnd\n\
CommandButton Build_Mounted\n  Command = UNIT_BUILD\n  Object = Mounted\nEnd\n\
CommandButton Build_Wolfpack\n  Command = UNIT_BUILD\n  Object = Wolfpack\nEnd\n";

fn start() -> (Simulation<RotwkRules>, Ids) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Camp", "Mounted", "Wolfpack"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let camp = kinds
        .spawn(
            &data.combat,
            kinds.kind("Camp").unwrap(),
            Vec2::new(100.0, 100.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![camp])
        .with_players(2)
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone());
    let simulation = Simulation::new(31, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    (simulation, ids)
}

fn run(simulation: &mut Simulation<RotwkRules>, ticks: u32) {
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
}

fn spawn_horde(simulation: &mut Simulation<RotwkRules>, ids: Ids, label: &str, x: f32) -> Entity {
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == label)
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x,
                y: 400.0,
                facing: 0.0,
            },
        )
        .unwrap();
    run(simulation, 2);
    simulation
        .state()
        .store(ids.horde)
        .unwrap()
        .entities()
        .last()
        .copied()
        .unwrap()
}

fn order_move(simulation: &mut Simulation<RotwkRules>, ids: Ids, horde: Entity, to: (f32, f32)) {
    let units = simulation
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
                units,
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: to.0,
                y: to.1,
                queue: false,
            },
        )
        .unwrap();
}

fn speed(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    simulation.state().get(ids.body, entity).unwrap().0.speed
}

fn fastest_turning_away(label: &str) -> f32 {
    let (mut simulation, ids) = start();
    let horde = spawn_horde(&mut simulation, ids, label, 500.0);
    order_move(&mut simulation, ids, horde, (150.0, 400.0));
    let mut fastest = 0.0_f32;
    for _ in 0..30 {
        simulation.tick().unwrap();
        fastest = fastest.max(speed(&simulation, ids, horde));
    }
    fastest
}

#[test]
fn a_horse_contain_horde_without_cavalry_does_not_creep() {
    let mounted = fastest_turning_away("Mounted");
    let wolves = fastest_turning_away("Wolfpack");
    assert!(mounted <= 10.0, "the cavalry horde must creep: {mounted}");
    assert!(wolves > 20.0, "the wolf horde must not creep: {wolves}");
}
