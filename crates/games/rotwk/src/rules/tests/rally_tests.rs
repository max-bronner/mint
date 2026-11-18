use super::*;

const DATA: &str = "\
Locomotor ZFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Object ZBarracks\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE AUTO_RALLYPOINT\n  \
  CommandSet = ZBarracksSet\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Behavior = QueueProductionExitUpdate Tag_Exit\n    UnitCreatePoint = X:20 Y:0 Z:0\n    \
    NaturalRallyPoint = X:150 Y:0 Z:0\n    ExitDelay = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
Object ZOutpost\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE AUTO_RALLYPOINT\n  \
  CommandSet = ZOutpostSet\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Behavior = QueueProductionExitUpdate Tag_Exit\n    UnitCreatePoint = X:20 Y:0 Z:0\n    \
    NaturalRallyPoint = X:150 Y:0 Z:0\n    NoExitPath = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
Object ZStable\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE AUTO_RALLYPOINT\n  \
  CommandSet = ZStableSet\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Behavior = QueueProductionExitUpdate Tag_Exit\n    UnitCreatePoint = X:20 Y:0 Z:0\n    \
    NaturalRallyPoint = X:150 Y:0 Z:0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
Object ZPost\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE AUTO_RALLYPOINT\n  \
  CommandSet = ZPostSet\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Behavior = QueueProductionExitUpdate Tag_Exit\n    UnitCreatePoint = X:20 Y:0 Z:0\n    \
    NaturalRallyPoint = X:150 Y:0 Z:0\n    ExitDelay = 30\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 2\n  GeometryHeight = 30\nEnd\n\
Object ZKeep\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE AUTO_RALLYPOINT\n  \
  CommandSet = ZKeepSet\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Behavior = QueueProductionExitUpdate Tag_Exit\n    NaturalRallyPoint = X:150 Y:0 Z:0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet ZBarracksSet\n  1 = Build_ZSoldier\n  2 = Build_ZSoldierHorde\nEnd\n\
CommandSet ZPostSet\n  1 = Build_ZSoldierHorde\nEnd\n\
CommandSet ZOutpostSet\n  1 = Build_ZSoldier\nEnd\n\
CommandSet ZStableSet\n  1 = Build_ZSoldierHorde4\n  2 = Build_ZRiderHorde4\nEnd\n\
CommandSet ZKeepSet\n  1 = Build_ZSoldier\nEnd\n\
CommandButton Build_ZSoldier\n  Command = UNIT_BUILD\n  Object = ZSoldier\n  Radial = Yes\nEnd\n\
CommandButton Build_ZSoldierHorde\n  Command = UNIT_BUILD\n  Object = ZSoldierHorde\n  Radial = Yes\nEnd\n\
CommandButton Build_ZSoldierHorde4\n  Command = UNIT_BUILD\n  Object = ZSoldierHorde4\n  Radial = Yes\nEnd\n\
CommandButton Build_ZRiderHorde4\n  Command = UNIT_BUILD\n  Object = ZRiderHorde4\n  Radial = Yes\nEnd\n\
Object ZSoldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZSoldierHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  BuildCost = 20\n  BuildTime = 1\n  \
  Behavior = HordeContain Tag_Horde\n    Slots = 2\n    \
    RankInfo = RankNumber:1 UnitType:ZSoldier Position:X:12 Y:0 Position:X:-12 Y:0\n  End\nEnd\n\
Object ZSoldierHorde4\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  BuildCost = 40\n  BuildTime = 1\n  \
  Behavior = HordeContain Tag_Horde\n    Slots = 4\n    \
    RankInfo = RankNumber:1 UnitType:ZSoldier Position:X:12 Y:0 Position:X:-12 Y:0 \
Position:X:24 Y:0 Position:X:-24 Y:0\n  End\nEnd\n\
Object ZRiderHorde4\n  Side = Men\n  KindOf = SELECTABLE HORDE CAVALRY\n  BuildCost = 40\n  BuildTime = 1\n  \
  Behavior = HordeContain Tag_Horde\n    Slots = 4\n    \
    RankInfo = RankNumber:1 UnitType:ZSoldier Position:X:12 Y:0 Position:X:-12 Y:0 \
Position:X:24 Y:0 Position:X:-24 Y:0\n  End\nEnd\n";

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
    let seeds: Vec<String> = [
        "ZBarracks",
        "ZOutpost",
        "ZStable",
        "ZPost",
        "ZKeep",
        "ZSoldierHorde",
        "ZSoldierHorde4",
        "ZRiderHorde4",
        "ZSoldier",
    ]
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
        .with_hordes(m.data.formations.tables().to_vec(), Vec::new())
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

#[test]
fn a_rally_point_with_no_path_stores_nothing() {
    let m = load();
    let mut simulation =
        Simulation::new(41, rules(&m, &[("ZBarracks", 0.0, 0.0, PLAYER)])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let barracks = placed(&simulation, ids, &m, "ZBarracks")[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetRallyPoint {
                structures: vec![barracks],
                x: 100_000.0,
                y: 100_000.0,
                global: false,
                target: None,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert!(
        simulation
            .state()
            .get(ids.producer, barracks)
            .unwrap()
            .player_rally
            .is_none(),
        "an unreachable rally point was stored"
    );
}

#[test]
fn a_loose_unit_walks_on_to_the_player_rally_point_once_set() {
    let m = load();
    let mut simulation =
        Simulation::new(41, rules(&m, &[("ZBarracks", 0.0, 0.0, PLAYER)])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let barracks = placed(&simulation, ids, &m, "ZBarracks")[0];
    let home = position(&simulation, ids, barracks);
    let rally = home.plus(Vec2::new(150.0, 200.0));
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetRallyPoint {
                structures: vec![barracks],
                x: rally.x,
                y: rally.y,
                global: false,
                target: None,
            },
        )
        .unwrap();
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
    let mut nearest = f32::MAX;
    for _ in 0..400 {
        simulation.tick().unwrap();
        if unit.is_none() {
            unit = simulation
                .state()
                .store(ids.body)
                .unwrap()
                .entities()
                .iter()
                .copied()
                .find(|entity| !before.contains(entity));
        }
        if let Some(unit) = unit {
            nearest = nearest.min(position(&simulation, ids, unit).minus(rally).length());
        }
    }
    assert!(unit.is_some(), "the soldier was never built");
    assert!(
        nearest < 10.0,
        "came only {nearest:.0} from the player rally point"
    );
}

#[test]
fn a_loose_unit_with_no_create_point_still_walks_to_the_player_rally_point() {
    let m = load();
    let mut simulation = Simulation::new(41, rules(&m, &[("ZKeep", 0.0, 0.0, PLAYER)])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let keep = placed(&simulation, ids, &m, "ZKeep")[0];
    let home = position(&simulation, ids, keep);
    let rally = home.plus(Vec2::new(150.0, 200.0));
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetRallyPoint {
                structures: vec![keep],
                x: rally.x,
                y: rally.y,
                global: false,
                target: None,
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![keep],
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
    let mut nearest = f32::MAX;
    for _ in 0..400 {
        simulation.tick().unwrap();
        if unit.is_none() {
            unit = simulation
                .state()
                .store(ids.body)
                .unwrap()
                .entities()
                .iter()
                .copied()
                .find(|entity| !before.contains(entity));
        }
        if let Some(unit) = unit {
            nearest = nearest.min(position(&simulation, ids, unit).minus(rally).length());
        }
    }
    assert!(unit.is_some(), "the soldier was never built");
    assert!(
        nearest < 10.0,
        "came only {nearest:.0} from the player rally point"
    );
}

#[test]
fn no_exit_path_holds_a_loose_unit_at_the_door_without_a_rally_point() {
    let m = load();
    let mut simulation = Simulation::new(41, rules(&m, &[("ZOutpost", 0.0, 0.0, PLAYER)])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let outpost = placed(&simulation, ids, &m, "ZOutpost")[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![outpost],
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
        if let Some(found) = simulation
            .state()
            .store(ids.body)
            .unwrap()
            .entities()
            .iter()
            .copied()
            .find(|entity| !before.contains(entity))
        {
            unit = Some(found);
            break;
        }
    }
    let unit = unit.expect("the soldier was built");
    assert!(
        !simulation.state().contains(ids.path, unit),
        "a NoExitPath unit walked with no rally point set"
    );
}

#[test]
fn a_global_rally_point_reaches_every_owned_auto_rallypoint_structure() {
    let m = load();
    let mut simulation = Simulation::new(
        41,
        rules(
            &m,
            &[
                ("ZBarracks", 0.0, 0.0, PLAYER),
                ("ZBarracks", 500.0, 0.0, PLAYER),
            ],
        ),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let barracks = placed(&simulation, ids, &m, "ZBarracks");
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetRallyPoint {
                structures: Vec::new(),
                x: 300.0,
                y: 0.0,
                global: true,
                target: None,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    for structure in barracks {
        assert!(
            simulation
                .state()
                .get(ids.producer, structure)
                .unwrap()
                .player_rally
                .is_some(),
            "entity {} did not take the global rally point",
            structure.index()
        );
    }
}

#[test]
fn a_horde_member_never_routes_through_the_player_rally_point() {
    let m = load();
    let mut simulation =
        Simulation::new(41, rules(&m, &[("ZBarracks", 0.0, 0.0, PLAYER)])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let barracks = placed(&simulation, ids, &m, "ZBarracks")[0];
    let home = position(&simulation, ids, barracks);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetRallyPoint {
                structures: vec![barracks],
                x: home.x + 500.0,
                y: home.y + 500.0,
                global: false,
                target: None,
            },
        )
        .unwrap();
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
        .submit(PLAYER, RotwkCommand::QueueUnit { button: 1 })
        .unwrap();
    let mut horde = None;
    for _ in 0..90 {
        simulation.tick().unwrap();
        if let Some(found) = simulation
            .state()
            .store(ids.horde)
            .and_then(|store| store.entities().first().copied())
        {
            horde = Some(found);
            break;
        }
    }
    let horde = horde.expect("the horde was produced");
    for _ in 0..6 {
        simulation.tick().unwrap();
    }
    let member = simulation.state().get(ids.horde, horde).unwrap().members[0];
    let path = simulation
        .state()
        .get(ids.path, member)
        .expect("the member walks out");
    assert_eq!(
        path.route.points.len(),
        2,
        "the member's route grew a third waypoint: {:?}",
        path.route.points
    );
}

#[test]
fn a_finished_burst_moves_the_horde_to_the_player_rally_point() {
    let m = load();
    let mut simulation =
        Simulation::new(41, rules(&m, &[("ZBarracks", 0.0, 0.0, PLAYER)])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let barracks = placed(&simulation, ids, &m, "ZBarracks")[0];
    let home = position(&simulation, ids, barracks);
    let rally = home.plus(Vec2::new(400.0, 0.0));
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetRallyPoint {
                structures: vec![barracks],
                x: rally.x,
                y: rally.y,
                global: false,
                target: None,
            },
        )
        .unwrap();
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
        .submit(PLAYER, RotwkCommand::QueueUnit { button: 1 })
        .unwrap();
    let mut horde = None;
    for _ in 0..90 {
        simulation.tick().unwrap();
        if let Some(found) = simulation
            .state()
            .store(ids.horde)
            .and_then(|store| store.entities().first().copied())
        {
            horde = Some(found);
            break;
        }
    }
    let horde = horde.expect("the horde was produced");
    let mut nearest = f32::MAX;
    for _ in 0..250 {
        simulation.tick().unwrap();
        if !simulation.state().contains(ids.production_stream, horde) {
            nearest = nearest.min(position(&simulation, ids, horde).minus(rally).length());
        }
    }
    assert!(
        nearest < 10.0,
        "the horde came only {nearest:.0} from the rally point after its burst ended"
    );
}

#[test]
fn exit_delay_zero_empties_a_horde_in_the_frame_after_its_own() {
    let m = load();
    let mut simulation = Simulation::new(41, rules(&m, &[("ZStable", 0.0, 0.0, PLAYER)])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let stable = placed(&simulation, ids, &m, "ZStable")[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![stable],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::QueueUnit { button: 0 })
        .unwrap();
    let mut horde = None;
    for _ in 0..90 {
        simulation.tick().unwrap();
        if let Some(found) = simulation
            .state()
            .store(ids.horde)
            .and_then(|store| store.entities().first().copied())
        {
            horde = Some(found);
            break;
        }
    }
    let horde = horde.expect("the horde was produced");
    const RETAIL_FRAME_TICKS: u32 = 6;
    for _ in 0..RETAIL_FRAME_TICKS {
        simulation.tick().unwrap();
    }
    assert_eq!(
        simulation
            .state()
            .get(ids.horde, horde)
            .unwrap()
            .members
            .len(),
        4,
        "not every member was out by the retail frame after the horde's own"
    );
    assert!(
        !simulation.state().contains(ids.production_stream, horde),
        "the burst had not finished by the retail frame after the horde's own"
    );
}

fn horde_at_burst_end(button: u8) -> (Simulation<RotwkRules>, Ids, Entity) {
    let m = load();
    let mut simulation = Simulation::new(41, rules(&m, &[("ZStable", 0.0, 0.0, PLAYER)])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let stable = placed(&simulation, ids, &m, "ZStable")[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![stable],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::QueueUnit { button })
        .unwrap();
    for _ in 0..200 {
        simulation.tick().unwrap();
        let finished = simulation
            .state()
            .store(ids.horde)
            .and_then(|store| store.entities().first().copied())
            .filter(|horde| !simulation.state().contains(ids.production_stream, *horde));
        if let Some(horde) = finished {
            return (simulation, ids, horde);
        }
    }
    panic!("no horde finished its burst");
}

#[test]
fn a_member_still_clearing_the_building_keeps_its_exit_route() {
    let (simulation, ids, horde) = horde_at_burst_end(0);
    let members = simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .clone();
    assert_eq!(members.len(), 4);
    for member in members {
        let path = simulation
            .state()
            .get(ids.path, member)
            .expect("a member at the door keeps walking");
        assert_eq!(
            path.route.points.len(),
            2,
            "the member's own exit route was replaced: {:?}",
            path.route.points
        );
    }
}

#[test]
fn a_kept_exit_route_ends_at_the_reformed_slot() {
    let (simulation, ids, horde) = horde_at_burst_end(0);
    let members = simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .clone();
    for member in members {
        let slot =
            crate::horde::member_slot_pose(simulation.state(), RotwkRules::horde_ids(ids), member)
                .expect("a slotted member");
        let path = simulation
            .state()
            .get(ids.path, member)
            .expect("a kept route");
        let end = *path.route.points.last().unwrap();
        assert!(
            end.minus(slot).length() < 0.01,
            "member's route ends {:.1} from its slot",
            end.minus(slot).length()
        );
    }
}

#[test]
fn a_cavalry_member_keeps_its_exit_route_at_burst_end() {
    let (simulation, ids, horde) = horde_at_burst_end(1);
    let centre = position(&simulation, ids, horde);
    let members = simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .clone();
    assert_eq!(members.len(), 4);
    for member in members {
        let path = simulation
            .state()
            .get(ids.path, member)
            .expect("a member at the door keeps walking");
        assert_eq!(path.route.points.len(), 2, "the exit route was replaced");
        assert!(
            path.goal.minus(centre).length() > 1.0,
            "member rides to the horde centre, not its slot"
        );
    }
}

#[test]
fn a_member_clear_of_the_building_at_burst_end_heads_straight_for_its_slot() {
    let m = load();
    let mut simulation = Simulation::new(41, rules(&m, &[("ZPost", 0.0, 0.0, PLAYER)])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let barracks = placed(&simulation, ids, &m, "ZPost")[0];
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
    let mut horde = None;
    for _ in 0..200 {
        simulation.tick().unwrap();
        let found = simulation
            .state()
            .store(ids.horde)
            .and_then(|store| store.entities().first().copied())
            .filter(|horde| !simulation.state().contains(ids.production_stream, *horde));
        if let Some(found) = found {
            horde = Some(found);
            break;
        }
    }
    let horde = horde.expect("the horde finished its burst");
    let members = simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .clone();
    assert_eq!(members.len(), 2);
    let earlier = members[0];
    let slot =
        crate::horde::member_slot_pose(simulation.state(), RotwkRules::horde_ids(ids), earlier)
            .expect("the member holds a slot");
    let path = simulation
        .state()
        .get(ids.path, earlier)
        .expect("the earlier member still walks somewhere");
    assert_eq!(
        path.route.points.len(),
        1,
        "the member kept its original exit route instead of heading straight for its slot: {:?}",
        path.route.points
    );
    assert!(
        path.goal.minus(slot).length() < 0.01,
        "member walks to {:?}, not its own slot {slot:?}",
        path.goal
    );
}

fn sounds_of_last_tick(
    simulation: &Simulation<RotwkRules>,
) -> Vec<(&'static str, PlayerId, f32, f32)> {
    let frame = Frame::new(simulation.frame().get() - 1);
    simulation
        .rules()
        .misc_sounds(simulation.state(), frame)
        .iter()
        .map(|e| (e.sound.field(), e.player, e.at.x, e.at.y))
        .collect()
}

fn set_rally(
    simulation: &mut Simulation<RotwkRules>,
    structures: Vec<Entity>,
    at: Vec2,
    global: bool,
) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetRallyPoint {
                structures,
                x: at.x,
                y: at.y,
                global,
                target: None,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
}

#[test]
fn a_rally_message_raises_its_set_or_unable_sound_at_the_point() {
    let m = load();
    let mut simulation =
        Simulation::new(41, rules(&m, &[("ZBarracks", 0.0, 0.0, PLAYER)])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let barracks = placed(&simulation, ids, &m, "ZBarracks")[0];
    let rally = position(&simulation, ids, barracks).plus(Vec2::new(150.0, 200.0));
    set_rally(&mut simulation, vec![barracks], rally, false);
    assert_eq!(
        sounds_of_last_tick(&simulation),
        [("RallyPointSet", PLAYER, rally.x, rally.y)]
    );
    set_rally(
        &mut simulation,
        vec![barracks],
        Vec2::new(100_000.0, 100_000.0),
        false,
    );
    assert_eq!(
        sounds_of_last_tick(&simulation),
        [("UnableToSetRallyPoint", PLAYER, 100_000.0, 100_000.0)]
    );
}

#[test]
fn a_global_rally_point_plays_its_sound_once() {
    let m = load();
    let mut simulation = Simulation::new(
        41,
        rules(
            &m,
            &[
                ("ZBarracks", 0.0, 0.0, PLAYER),
                ("ZBarracks", 500.0, 0.0, PLAYER),
            ],
        ),
    )
    .unwrap();
    set_rally(&mut simulation, Vec::new(), Vec2::new(300.0, 0.0), true);
    assert_eq!(
        sounds_of_last_tick(&simulation),
        [("RallyPointSet", PLAYER, 300.0, 0.0)]
    );
}
