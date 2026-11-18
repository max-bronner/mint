use super::*;

#[test]
fn a_dead_victim_is_replaced_at_once_not_at_the_next_mood_scan() {
    let (combat, bow, armour) = combat_data();
    let map = flat_map(128, 128);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let formation = square_formation();
    let count = u32::try_from(formation.len()).unwrap();
    let spawn = HordeSpawn {
        formation: 0,
        origin: Vec2::new(200.0, 200.0),
        facing: Facing::EAST,
        member: Spawn {
            health: Some(200.0),
            weapon: Some(bow),
            armour: Some(armour),
            ..spawn_at(0.0, 0.0, KindOf::SELECTABLE, PLAYER)
        },
        count,
        vision: crate::shroud::HORDE_SHROUD_RANGE,
    };
    let rules = RotwkRules::new(
        passability,
        vec![
            armed_spawn(220.0, 200.0, OTHER, None, Some(armour), 5.0),
            armed_spawn(400.0, 200.0, OTHER, None, Some(armour), 10_000.0),
        ],
    )
    .with_combat(combat)
    .with_hordes(vec![formation], vec![spawn]);
    let mut simulation = Simulation::new(29, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let members = members_of(&simulation, ids);
    let horde = horde_entity(&simulation, ids).unwrap();
    let enemies = simulation
        .state()
        .store(ids.owner)
        .map(|store| {
            store
                .iter()
                .filter(|(_, owner)| owner.0 == OTHER)
                .map(|(entity, _)| entity)
                .collect::<Vec<_>>()
        })
        .unwrap();
    let (near, far) = (enemies[0], enemies[1]);

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
            RotwkCommand::AttackMove {
                x: 700.0,
                y: 200.0,
                queue: false,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert_eq!(
        simulation
            .state()
            .get(ids.horde_target, horde)
            .and_then(|hordetarget| hordetarget.target),
        Some(near),
        "the fixture's premise: the near enemy is acquired first"
    );

    let mut died_at = None;
    for frame in 0..60 {
        simulation.tick().unwrap();
        if !simulation.state().is_live(near) {
            died_at = Some(frame);
            break;
        }
    }
    died_at.expect("the near enemy must die quickly - the fixture's own 5 HP");
    simulation.tick().unwrap();
    assert_eq!(
        simulation
            .state()
            .get(ids.horde_target, horde)
            .and_then(|hordetarget| hordetarget.target),
        Some(far),
        "the next enemy must be taken the tick after the first dies, not on a later scan"
    );
}

const RANGED_DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon Bow\n  AttackRange = 200\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 5\n    DamageType = PIERCE\n  End\nEnd\n\
Object RangedSoldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 10000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Bow\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object RangedHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  CommandSet = HordeSet\n  \
  BuildCost = 100\n  BuildTime = 1\n  CommandPoints = 3\n  VisionRange = 400\n  \
  Behavior = HordeAIUpdate Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n  End\n  \
  Behavior = HordeContain Tag_01\n    Slots = 2\n    \
    RankInfo = RankNumber:1 UnitType:RangedSoldier Position:X:10 Y:-10 Position:X:10 Y:10\n  \
  End\nEnd\n\
Object Quarry\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 10\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 10000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_RangedHorde\nEnd\n\
CommandSet HordeSet\n  1 = Build_RangedHorde\nEnd\n\
CommandButton Build_RangedHorde\n  Command = UNIT_BUILD\n  Object = RangedHorde\nEnd\n";

#[test]
fn a_human_ranged_horde_on_attack_move_takes_only_a_target_already_in_weapon_range() {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        RANGED_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Camp", "RangedHorde", "Quarry"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let enemy_spawn = kinds
        .spawn(
            &data.combat,
            kinds.kind("Quarry").unwrap(),
            Vec2::new(450.0, 200.0),
            Facing::EAST,
            OTHER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(160, 160), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![enemy_spawn])
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_teams(vec![None, None]);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "RangedHorde")
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 100.0,
                y: 200.0,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let horde = simulation.state().store(ids.horde).unwrap().entities()[0];
    let enemy = simulation
        .state()
        .store(ids.owner)
        .map(|store| {
            store
                .iter()
                .find(|(_, owner)| owner.0 == OTHER)
                .map(|(entity, _)| entity)
        })
        .unwrap()
        .unwrap();

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![horde],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::AttackMove {
                x: 700.0,
                y: 200.0,
                queue: false,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert_eq!(
        simulation
            .state()
            .get(ids.horde_target, horde)
            .and_then(|hordetarget| hordetarget.target),
        None,
        "350 apart is inside vision but outside the bow's 200 - must not be taken yet"
    );

    let mut acquired_at = None;
    let mut gap_at_acquire = None;
    for frame in 0..400 {
        simulation.tick().unwrap();
        if simulation
            .state()
            .get(ids.horde_target, horde)
            .and_then(|hordetarget| hordetarget.target)
            == Some(enemy)
        {
            acquired_at = Some(frame);
            let horde_at = position(&simulation, ids, horde);
            let enemy_at = simulation.state().get(ids.body, enemy).unwrap().0.position;
            gap_at_acquire = Some(horde_at.minus(enemy_at).length());
            break;
        }
    }
    acquired_at.expect("the horde must still take it once within weapon range");
    let gap = gap_at_acquire.unwrap();
    assert!(
        gap <= 250.0,
        "acquired at {gap} apart - too far past the bow's own 200 range to be the weapon-range gate"
    );
}
