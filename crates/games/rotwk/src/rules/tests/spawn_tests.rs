use super::*;

const DATA: &str = "\
Object ZDen\n  Side = Neutral\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 2000\n  End\n  \
  Behavior = SpawnBehavior Tag_Spawn\n    SpawnNumber = 2\n    InitialBurst = 2\n    \
    SpawnTemplateName = ZImp ZOgre\n    SpawnReplaceDelay = 1000\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  GeometryHeight = 30\nEnd\n\
Object ZImp\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = SlavedUpdate Tag_Slave\n    GuardMaxRange = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZOgre\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 300\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 8\n  GeometryHeight = 20\nEnd\n";

const WILDS: PlayerId = PlayerId::new(2);

fn den(ticks: u32, kill_at: u32) -> (Vec<u64>, Vec<String>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(&data, &[String::from("ZDen")]);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let den = kinds
        .spawn(
            &data.combat,
            kinds.kind("ZDen").unwrap(),
            Vec2::new(400.0, 400.0),
            Facing::EAST,
            WILDS,
        )
        .unwrap();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![den])
        .with_players(2)
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            vec![String::from("Wilds")],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let spawned = |simulation: &Simulation<RotwkRules>| -> Vec<(Entity, String)> {
        let world = simulation.state();
        world
            .store(ids.kind)
            .unwrap()
            .iter()
            .filter(|(entity, _)| !world.contains(ids.doomed, *entity))
            .filter_map(|(entity, kind)| {
                let name = &kinds.get(*kind)?.record.name;
                (name != "ZDen").then(|| (entity, name.clone()))
            })
            .collect()
    };
    let mut hashes = Vec::new();
    for tick in 0..ticks {
        if tick == kill_at {
            let victim = spawned(&simulation)[0].0;
            for command in [
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![victim],
                },
                RotwkCommand::DebugKill,
            ] {
                simulation.submit(PLAYER, command).unwrap();
            }
        }
        simulation.tick().unwrap();
        hashes.push(simulation.state_hash());
    }
    let mut names: Vec<String> = spawned(&simulation).into_iter().map(|(_, n)| n).collect();
    names.sort();
    (hashes, names)
}

#[test]
fn a_spawn_and_its_replacement_replay_to_the_same_hash_sequence() {
    let (first, living) = den(90, 20);
    assert_eq!(living, ["ZImp", "ZOgre"], "the replacement was made");
    let (second, _) = den(90, 20);
    assert_eq!(first, second);
}

const REQUIRE_SPAWNER_DATA: &str = "\
Object ZLair\n  Side = Neutral\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 2000\n  End\n  \
  Behavior = SpawnBehavior Tag_Spawn\n    SpawnNumber = 2\n    InitialBurst = 2\n    \
    SpawnTemplateName = ZWolf\n    SpawnReplaceDelay = 1000000\n    \
    SpawnedRequireSpawner = Yes\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  GeometryHeight = 30\nEnd\n\
Object ZWolf\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn lair(ticks: u32, kill_at: u32) -> Vec<String> {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        REQUIRE_SPAWNER_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(&data, &[String::from("ZLair")]);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let lair = kinds
        .spawn(
            &data.combat,
            kinds.kind("ZLair").unwrap(),
            Vec2::new(400.0, 400.0),
            Facing::EAST,
            WILDS,
        )
        .unwrap();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![lair])
        .with_players(2)
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            vec![String::from("Wilds")],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let lair_entity = {
        let world = simulation.state();
        world
            .store(ids.kind)
            .unwrap()
            .iter()
            .find(|(_, k)| kinds.get(**k).is_some_and(|e| e.record.name == "ZLair"))
            .map(|(entity, _)| entity)
            .unwrap()
    };
    for tick in 0..ticks {
        if tick == kill_at {
            for command in [
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![lair_entity],
                },
                RotwkCommand::DebugKill,
            ] {
                simulation.submit(PLAYER, command).unwrap();
            }
        }
        simulation.tick().unwrap();
    }
    let world = simulation.state();
    world
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(entity, _)| !world.contains(ids.doomed, *entity))
        .filter_map(|(_, kind)| {
            let name = &kinds.get(*kind)?.record.name;
            (name != "ZLair").then(|| name.clone())
        })
        .collect()
}

#[test]
fn spawned_require_spawner_kills_every_living_spawn_with_its_spawner() {
    let living = lair(40, 30);
    assert_eq!(
        living,
        Vec::<String>::new(),
        "both wolves died with the lair"
    );
}

const SHARE_UPGRADES_DATA: &str = "\
Upgrade Upgrade_Fangs\n  Type = OBJECT\n  BuildCost = 0\n  BuildTime = 1\nEnd\n\
Object ZDen\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = ZDenSet\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 2000\n  End\n  \
  Behavior = SpawnBehavior Tag_Spawn\n    SpawnNumber = 1\n    InitialBurst = 1\n    \
    SpawnTemplateName = ZCub\n    SpawnReplaceDelay = 1000000\n    \
    ShareUpgrades = Yes\n  End\n  \
  Behavior = StatusBitsUpgrade Tag_Fangs\n    TriggeredBy = Upgrade_Fangs\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  GeometryHeight = 30\nEnd\n\
CommandSet ZDenSet\n  1 = Buy_Fangs\nEnd\n\
CommandButton Buy_Fangs\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_Fangs\nEnd\n\
Object ZCub\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = SlavedUpdate Tag_Slave\n    GuardMaxRange = 60\n    GuardWanderRange = 30\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

#[test]
fn share_upgrades_reaches_a_slaved_spawn() {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        SHARE_UPGRADES_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(&data, &[String::from("ZDen")]);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let den = kinds
        .spawn(
            &data.combat,
            kinds.kind("ZDen").unwrap(),
            Vec2::new(400.0, 400.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![den])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    let den_entity = {
        let world = simulation.state();
        world
            .store(ids.kind)
            .unwrap()
            .iter()
            .find(|(_, k)| kinds.get(**k).is_some_and(|e| e.record.name == "ZDen"))
            .map(|(entity, _)| entity)
            .unwrap()
    };
    let cub_entity = {
        let world = simulation.state();
        world
            .store(ids.kind)
            .unwrap()
            .iter()
            .find(|(_, k)| kinds.get(**k).is_some_and(|e| e.record.name == "ZCub"))
            .map(|(entity, _)| entity)
            .unwrap()
    };
    let bit = data.upgrade_bits.bit("Upgrade_Fangs").unwrap();
    let holds = |sim: &Simulation<RotwkRules>, entity: Entity| {
        sim.state()
            .get(ids.object_upgrades, entity)
            .is_some_and(|mask| mask.0.contains(bit))
    };
    assert!(
        !holds(&simulation, cub_entity),
        "the fixture's premise: not researched yet"
    );
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![den_entity],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::ObjectUpgrade { button: 0 })
        .unwrap();
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    assert!(holds(&simulation, den_entity), "the den researched it");
    assert!(
        holds(&simulation, cub_entity),
        "ShareUpgrades should have carried it to the cub"
    );
}

const RECLAIM_ORPHANS_DATA: &str = "\
Object ZLair\n  Side = Neutral\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 2000\n  End\n  \
  Behavior = SpawnBehavior Tag_Spawn\n    SpawnNumber = 1\n    \
    SpawnTemplateName = ZWolf\n    SpawnReplaceDelay = 1000000\n    \
    CanReclaimOrphans = Yes\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 20\n  GeometryMinorRadius = 20\n  GeometryHeight = 30\nEnd\n\
Object ZWolf\n  Side = Neutral\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

#[test]
fn a_rebuilt_lair_reclaims_its_predecessors_surviving_orphan_instead_of_making_a_new_one() {
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        RECLAIM_ORPHANS_DATA,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(&data, &[String::from("ZLair")]);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let lair_kind = kinds.kind("ZLair").unwrap();
    let lair = kinds
        .spawn(
            &data.combat,
            lair_kind,
            Vec2::new(300.0, 300.0),
            Facing::EAST,
            WILDS,
        )
        .unwrap();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![lair])
        .with_players(2)
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            vec![String::from("Wilds")],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits));
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let wolves = |simulation: &Simulation<RotwkRules>| -> Vec<Entity> {
        let world = simulation.state();
        world
            .store(ids.kind)
            .unwrap()
            .iter()
            .filter(|(entity, k)| {
                world.is_live(*entity) && kinds.get(**k).is_some_and(|e| e.record.name == "ZWolf")
            })
            .map(|(entity, _)| entity)
            .collect()
    };
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    assert_eq!(wolves(&simulation).len(), 1, "the first lair's wolf stands");
    let lair_entity = {
        let world = simulation.state();
        world
            .store(ids.kind)
            .unwrap()
            .iter()
            .find(|(entity, k)| {
                world.is_live(*entity) && kinds.get(**k).is_some_and(|e| e.record.name == "ZLair")
            })
            .map(|(entity, _)| entity)
            .unwrap()
    };
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![lair_entity],
        },
        RotwkCommand::DebugKill,
    ] {
        simulation.submit(WILDS, command).unwrap();
    }
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    assert_eq!(
        wolves(&simulation).len(),
        1,
        "the orphaned wolf survives its dead lair"
    );
    simulation
        .submit(
            WILDS,
            RotwkCommand::DebugSpawnStructure {
                kind: lair_kind.0,
                x: 700.0,
                y: 700.0,
                angle: 0.0,
            },
        )
        .unwrap();
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    assert_eq!(
        wolves(&simulation).len(),
        1,
        "the second lair reclaimed the surviving orphan rather than making a new wolf"
    );
}

fn den_placed_over(owner: PlayerId) -> bool {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = [String::from("ZDen"), String::from("ZOgre")];
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let ogre = kinds
        .spawn(
            &data.combat,
            kinds.kind("ZOgre").unwrap(),
            Vec2::new(300.0, 300.0),
            Facing::EAST,
            owner,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![ogre])
        .with_players(2)
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            vec![String::from("Wilds")],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let den = kinds.kind("ZDen").unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnStructure {
                kind: den.0,
                x: 300.0,
                y: 300.0,
                angle: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    let placed = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .any(|(_, kind)| *kind == den);
    placed
}

#[test]
fn a_neutral_unit_in_the_footprint_does_not_refuse_a_placement() {
    assert!(den_placed_over(WILDS), "a neutral map side's unit");
    assert!(
        !den_placed_over(PlayerId::new(1)),
        "an enemy contestant's unit"
    );
}
