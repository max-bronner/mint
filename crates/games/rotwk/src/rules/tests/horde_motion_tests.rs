use super::*;

const DATA: &str = "\
Locomotor MemberFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 500\nEnd\n\
Locomotor HordeFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 1000\n  MaxTurnWithoutReform = 45\nEnd\n\
Armor Plain\n  Armor = DEFAULT 100%\nEnd\n\
Weapon Spear\n  AttackRange = 20\n  DelayBetweenShots = 1000\n  \
  DamageNugget\n    Damage = 1\n    DamageType = SLASH\n  End\nEnd\n\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 600\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Spear\n  End\n  \
  LocomotorSet\n    Locomotor = MemberFeet\n    Condition = SET_NORMAL\n    Speed = 45\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Tight\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  BuildCost = 1\n  BuildTime = 1\n  \
  LocomotorSet\n    Locomotor = HordeFeet\n    Condition = SET_NORMAL\n    Speed = 40\n  End\n  \
  Behavior = HordeContain Tag_Contain\n    Slots = 9\n    MeleeAttackLeashDistance = 1\n    \
    RankInfo = RankNumber:1 UnitType:Soldier Position:X:50 Y:0 Position:X:50 Y:20 \
Position:X:50 Y:-20\n    \
    RankInfo = RankNumber:2 UnitType:Soldier Position:X:30 Y:0 Position:X:30 Y:20 \
Position:X:30 Y:-20\n    \
    RankInfo = RankNumber:3 UnitType:Soldier Position:X:10 Y:0 Position:X:10 Y:20 \
Position:X:10 Y:-20\n  End\nEnd\n\
Object Loose\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  BuildCost = 1\n  BuildTime = 1\n  \
  LocomotorSet\n    Locomotor = HordeFeet\n    Condition = SET_NORMAL\n    Speed = 40\n  End\n  \
  Behavior = HordeContain Tag_Contain\n    Slots = 3\n    MeleeAttackLeashDistance = 150\n    \
    RankInfo = RankNumber:1 UnitType:Soldier Position:X:10 Y:0 Position:X:10 Y:20 \
Position:X:10 Y:-20\n  End\nEnd\n\
Object Dummy\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_Tight\n  2 = Build_Loose\nEnd\n\
CommandButton Build_Tight\n  Command = UNIT_BUILD\n  Object = Tight\nEnd\n\
CommandButton Build_Loose\n  Command = UNIT_BUILD\n  Object = Loose\nEnd\n";

fn start(placed: &[(&str, f32, f32, PlayerId)]) -> (Simulation<RotwkRules>, Ids) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Camp", "Tight", "Loose", "Dummy"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns: Vec<Spawn> = placed
        .iter()
        .map(|(name, x, y, owner)| {
            let kind = kinds.kind(name).unwrap();
            kinds
                .spawn(&data.combat, kind, Vec2::new(*x, *y), Facing::EAST, *owner)
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone());
    let simulation = Simulation::new(29, rules).unwrap();
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

fn members(simulation: &Simulation<RotwkRules>, ids: Ids, horde: Entity) -> Vec<Entity> {
    simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .clone()
}

fn select_horde(simulation: &mut Simulation<RotwkRules>, ids: Ids, horde: Entity) {
    let units = members(simulation, ids, horde);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units,
            },
        )
        .unwrap();
}

fn worst_slot_error(simulation: &Simulation<RotwkRules>, ids: Ids, horde: Entity) -> f32 {
    let table = simulation.state().get(ids.horde, horde).unwrap();
    let body = simulation.state().get(ids.body, horde).unwrap().0;
    let poses = crate::horde::slot_poses(table, &body);
    table
        .members
        .iter()
        .map(|member| {
            let slot = simulation
                .state()
                .get(ids.member, *member)
                .unwrap()
                .slot
                .unwrap();
            let at = simulation
                .state()
                .get(ids.body, *member)
                .unwrap()
                .0
                .position;
            at.minus(poses[usize::from(slot)]).length()
        })
        .fold(0.0, f32::max)
}

#[test]
fn a_stopped_horde_stands_exactly_on_its_slots_and_settles() {
    let (mut simulation, ids) = start(&[("Camp", 100.0, 100.0, PLAYER)]);
    let horde = spawn_horde(&mut simulation, ids, "Tight", 300.0);
    select_horde(&mut simulation, ids, horde);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 700.0,
                y: 400.0,
                queue: false,
            },
        )
        .unwrap();
    run(&mut simulation, 30);
    assert!(
        simulation.state().contains(ids.path, horde),
        "the horde must still be marching"
    );
    run(&mut simulation, 600);
    assert!(!simulation.state().contains(ids.path, horde));
    assert_eq!(worst_slot_error(&simulation, ids, horde), 0.0);
    assert!(!simulation.state().get(ids.horde, horde).unwrap().dirty);
    let before: Vec<_> = members(&simulation, ids, horde)
        .iter()
        .map(|member| simulation.state().get(ids.body, *member).unwrap().0)
        .collect();
    run(&mut simulation, 60);
    let after: Vec<_> = members(&simulation, ids, horde)
        .iter()
        .map(|member| simulation.state().get(ids.body, *member).unwrap().0)
        .collect();
    assert_eq!(before, after, "a settled horde's members must not move");
}

#[test]
fn an_about_face_reforms_instead_of_walking_through_itself() {
    let (mut simulation, ids) = start(&[("Camp", 100.0, 100.0, PLAYER)]);
    let horde = spawn_horde(&mut simulation, ids, "Loose", 500.0);
    select_horde(&mut simulation, ids, horde);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 150.0,
                y: 400.0,
                queue: false,
            },
        )
        .unwrap();
    let mut closest = f32::MAX;
    for _ in 0..240 {
        simulation.tick().unwrap();
        let positions: Vec<Vec2> = members(&simulation, ids, horde)
            .iter()
            .map(|member| {
                simulation
                    .state()
                    .get(ids.body, *member)
                    .unwrap()
                    .0
                    .position
            })
            .collect();
        for (at, first) in positions.iter().enumerate() {
            for second in positions.iter().skip(at + 1) {
                closest = closest.min(first.minus(*second).length());
            }
        }
    }
    assert!(
        closest > 10.0,
        "members crossed through each other: closest pair {closest}"
    );
}

#[test]
fn a_closing_horde_parks_members_beyond_its_leash() {
    let (mut simulation, ids) = start(&[
        ("Camp", 100.0, 100.0, PLAYER),
        ("Dummy", 700.0, 400.0, OTHER),
    ]);
    let target = simulation
        .state()
        .store(ids.health)
        .unwrap()
        .entities()
        .iter()
        .copied()
        .find(|entity| simulation.state().get(ids.owner, *entity).map(|o| o.0) == Some(OTHER))
        .unwrap();
    let tight = spawn_horde(&mut simulation, ids, "Tight", 300.0);
    let loose = spawn_horde(&mut simulation, ids, "Loose", 300.0);
    let parked = |simulation: &Simulation<RotwkRules>, horde| {
        members(simulation, ids, horde)
            .iter()
            .filter(|member| simulation.state().get(ids.member, **member).unwrap().parked)
            .count()
    };
    for horde in [tight, loose] {
        select_horde(&mut simulation, ids, horde);
        simulation
            .submit(PLAYER, RotwkCommand::AttackObject { target })
            .unwrap();
    }
    run(&mut simulation, 60);
    assert!(simulation.state().contains(ids.path, tight));
    assert_eq!(parked(&simulation, tight), 9);
    assert_eq!(parked(&simulation, loose), 0);
    run(&mut simulation, 900);
    assert!(!simulation.state().contains(ids.path, tight));
    assert_eq!(parked(&simulation, tight), 0);
}
