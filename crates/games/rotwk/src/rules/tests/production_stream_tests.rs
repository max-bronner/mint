use super::*;

const DATA: &str = "\
Locomotor ZFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Object ZBarracks\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  CommandSet = ZBarracksSet\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Behavior = QueueProductionExitUpdate Tag_Exit\n    UnitCreatePoint = X:20 Y:0 Z:0\n    \
    NaturalRallyPoint = X:150 Y:0 Z:0\n    ExitDelay = 10\n    PlacementViewAngle = 180\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
Object ZGateless\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  CommandSet = ZBarracksSet\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Behavior = QueueProductionExitUpdate Tag_Exit\n    UnitCreatePoint = X:20 Y:0 Z:0\n    \
    NaturalRallyPoint = X:150 Y:0 Z:0\n    ExitDelay = 10\n    NoExitPath = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet ZBarracksSet\n  1 = Build_ZSoldierHorde\n  2 = Build_ZFarHorde\n  3 = Build_ZMixedHorde\nEnd\n\
CommandButton Build_ZSoldierHorde\n  Command = UNIT_BUILD\n  Object = ZSoldierHorde\n  Radial = Yes\nEnd\n\
Object ZSoldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ZSoldierHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  BuildCost = 20\n  BuildTime = 1\n  \
  Behavior = HordeContain Tag_Horde\n    Slots = 2\n    \
    RankInfo = RankNumber:1 UnitType:ZSoldier Position:X:12 Y:0 Position:X:-12 Y:0\n  End\nEnd\n\
CommandButton Build_ZFarHorde\n  Command = UNIT_BUILD\n  Object = ZFarHorde\n  Radial = Yes\nEnd\n\
Object ZFarHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  BuildCost = 20\n  BuildTime = 1\n  \
  Behavior = HordeContain Tag_Horde\n    Slots = 2\n    \
    RankInfo = RankNumber:1 UnitType:ZSoldier Position:X:320 Y:10 Position:X:0 Y:0\n  End\nEnd\n\
Object ZOther\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = ZFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandButton Build_ZMixedHorde\n  Command = UNIT_BUILD\n  Object = ZMixedHorde\n  Radial = Yes\nEnd\n\
Object ZMixedHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  BuildCost = 20\n  BuildTime = 1\n  \
  Behavior = HordeContain Tag_Horde\n    Slots = 2\n    InitialPayload = ZSoldier 2\n    \
    RankInfo = RankNumber:1 UnitType:ZOther Position:X:12 Y:0 Position:X:-12 Y:0\n  End\nEnd\n";

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
        "ZGateless",
        "ZSoldierHorde",
        "ZFarHorde",
        "ZSoldier",
        "ZMixedHorde",
        "ZOther",
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

fn produce_horde(m: &Match) -> (Simulation<RotwkRules>, Ids, Vec2, Entity) {
    produce_horde_at(m, "ZBarracks")
}

fn produce_horde_at(m: &Match, structure: &str) -> (Simulation<RotwkRules>, Ids, Vec2, Entity) {
    let mut simulation = Simulation::new(41, rules(m, &[(structure, 0.0, 0.0, PLAYER)])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let barracks = placed(&simulation, ids, m, structure)[0];
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
    (simulation, ids, home, horde)
}

#[test]
fn a_produced_hordes_body_appears_at_the_rally_point_not_the_door() {
    let m = load();
    let (simulation, ids, home, horde) = produce_horde(&m);
    let at = position(&simulation, ids, horde).minus(home);
    assert!(
        at.minus(Vec2::new(150.0, 0.0)).length() < 5.0,
        "the horde appeared at {at:?}, expected the rally point"
    );
    assert!(
        !simulation.state().contains(ids.path, horde),
        "the horde holds a route of its own - it should already be at the rally point"
    );
}

#[test]
fn a_produced_hordes_members_stream_out_one_per_exit_delay_and_are_unselectable_meanwhile() {
    let m = load();
    let (mut simulation, ids, _home, horde) = produce_horde(&m);
    let members = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(ids.horde, horde)
            .map_or(0, |held| held.members.len())
    };
    assert_eq!(
        members(&simulation),
        0,
        "a member came out with the horde itself"
    );
    let mut counts = Vec::new();
    for _ in 0..6 {
        simulation.tick().unwrap();
        counts.push(members(&simulation));
    }
    assert_eq!(
        counts,
        [0, 0, 0, 0, 0, 1],
        "the first member is one retail frame behind the horde"
    );
    let first_member = simulation.state().get(ids.horde, horde).unwrap().members[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![first_member],
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert!(
        selection(&simulation, ids, PLAYER).is_empty(),
        "a member is selectable while its horde is still streaming"
    );

    let mut counts = Vec::new();
    for _ in 0..40 {
        simulation.tick().unwrap();
        counts.push(
            simulation
                .state()
                .get(ids.horde, horde)
                .map_or(0, |held| held.members.len()),
        );
    }
    assert_eq!(*counts.last().unwrap(), 2, "both slots filled by the end");
    assert_eq!(
        counts[3], 1,
        "the second member exited before a retail frame had passed"
    );
    assert_eq!(
        counts[4], 2,
        "the second member is not one retail frame behind the first"
    );

    assert!(
        !simulation.state().contains(ids.production_stream, horde),
        "the burst never ended"
    );
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![first_member],
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert!(
        !selection(&simulation, ids, PLAYER).is_empty(),
        "a member stays unselectable once its horde's burst has ended"
    );
}

#[test]
fn a_produced_horde_faces_its_exit_modules_placement_view_angle() {
    let m = load();
    let (simulation, ids, _home, horde) = produce_horde(&m);
    let facing = simulation.state().get(ids.body, horde).unwrap().0.facing;
    assert!(
        (facing.radians().abs() - core::f32::consts::PI).abs() < 1e-3,
        "the horde faces {} rad, expected pi",
        facing.radians()
    );
}

#[test]
fn a_member_walks_through_the_midpoint_to_its_own_slot() {
    let m = load();
    let (mut simulation, ids, home, horde) = produce_horde(&m);
    for _ in 0..6 {
        simulation.tick().unwrap();
    }
    let member = simulation.state().get(ids.horde, horde).unwrap().members[0];
    let path = simulation
        .state()
        .get(ids.path, member)
        .expect("the member walks out");
    let points: Vec<Vec2> = path.route.points.iter().map(|p| p.minus(home)).collect();
    let slot =
        crate::horde::member_slot_pose(simulation.state(), RotwkRules::horde_ids(ids), member)
            .unwrap();
    assert!(
        points
            .first()
            .is_some_and(|p| p.minus(Vec2::new(85.0, 0.0)).length() < 1.0),
        "first waypoint {points:?}"
    );
    assert!(
        path.goal.minus(slot).length() < 1e-3,
        "goal {:?}, slot {slot:?}",
        path.goal
    );
}

#[test]
fn a_second_horde_out_of_the_door_moves_the_idle_first_one_aside() {
    let m = load();
    let (mut simulation, ids, _home, first) = produce_horde(&m);
    for _ in 0..120 {
        simulation.tick().unwrap();
    }
    assert!(
        !simulation.state().contains(ids.production_stream, first),
        "the first burst never ended"
    );
    let stood = position(&simulation, ids, first);
    simulation
        .submit(PLAYER, RotwkCommand::QueueUnit { button: 0 })
        .unwrap();
    let mut moved = false;
    for _ in 0..200 {
        simulation.tick().unwrap();
        if simulation.state().contains(ids.path, first)
            || position(&simulation, ids, first).minus(stood).length() > 5.0
        {
            moved = true;
            break;
        }
    }
    assert!(
        moved,
        "the idle first horde never stepped out of the second one's way"
    );
}

fn rules_with_idle_ally(m: &Match, barracks_at: Vec2, ally_at: Vec2) -> RotwkRules {
    let production = m.kinds.production(&m.data);
    let barracks_record = &m
        .kinds
        .get(m.kinds.kind("ZBarracks").unwrap())
        .unwrap()
        .record;
    let footprints = vec![(0usize, crate::construction::footprint_of(barracks_record))];
    let placed_list = [
        ("ZBarracks", barracks_at, PLAYER),
        ("ZSoldier", ally_at, PLAYER),
    ];
    let spawns: Vec<Spawn> = placed_list
        .iter()
        .map(|(name, at, owner)| {
            let kind = m.kinds.kind(name).unwrap();
            let mut spawn = m
                .kinds
                .spawn(&m.data.combat, kind, *at, Facing::EAST, *owner)
                .unwrap();
            spawn.producer = m.kinds.producer(&production, kind, *at, Facing::EAST);
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

fn produce_far_horde_with_ally(m: &Match) -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let mut simulation = Simulation::new(
        41,
        rules_with_idle_ally(m, Vec2::new(50.0, 50.0), Vec2::new(55.0, 47.5)),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let barracks = placed(&simulation, ids, m, "ZBarracks")[0];
    let ally = placed(&simulation, ids, m, "ZSoldier")[0];
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
    let mut member = None;
    for _ in 0..90 {
        simulation.tick().unwrap();
        if let Some(found) = simulation
            .state()
            .store(ids.member)
            .and_then(|store| store.entities().first().copied())
        {
            member = Some(found);
            break;
        }
    }
    let member = member.expect("the member was produced");
    (simulation, ids, ally, member)
}

#[test]
fn a_member_whose_slot_crosses_the_building_still_gets_an_ally_pushed_off_its_way() {
    let m = load();
    let (mut simulation, ids, ally, _member) = produce_far_horde_with_ally(&m);
    let stood = position(&simulation, ids, ally);
    let mut moved = false;
    for _ in 0..30 {
        simulation.tick().unwrap();
        if simulation.state().contains(ids.path, ally)
            || position(&simulation, ids, ally).minus(stood).length() > 1.0
        {
            moved = true;
            break;
        }
    }
    assert!(
        moved,
        "the idle ally on the member's crossing route never stepped aside"
    );
}

#[test]
fn a_mixed_hordes_members_come_from_initial_payload_and_spawn_loose_when_not_equivalent() {
    let m = load();
    let mut simulation =
        Simulation::new(41, rules(&m, &[("ZBarracks", 0.0, 0.0, PLAYER)])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let barracks = placed(&simulation, ids, &m, "ZBarracks")[0];
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
        .submit(PLAYER, RotwkCommand::QueueUnit { button: 2 })
        .unwrap();
    let mut members = Vec::new();
    for _ in 0..200 {
        simulation.tick().unwrap();
        members = simulation
            .state()
            .store(ids.member)
            .map(|store| store.entities().to_vec())
            .unwrap_or_default();
        let soldiers = simulation
            .state()
            .store(ids.kind)
            .map(|store| {
                store
                    .iter()
                    .filter(|(entity, kind)| {
                        simulation.state().is_live(*entity)
                            && **kind == m.kinds.kind("ZSoldier").unwrap()
                    })
                    .count()
            })
            .unwrap_or(0);
        if soldiers >= 2 {
            break;
        }
    }
    let soldier_kind = m.kinds.kind("ZSoldier").unwrap();
    let spawned: Vec<Entity> = simulation
        .state()
        .store(ids.kind)
        .map(|store| {
            store
                .iter()
                .filter(|(entity, kind)| {
                    simulation.state().is_live(*entity) && **kind == soldier_kind
                })
                .map(|(entity, _)| entity)
                .collect()
        })
        .unwrap_or_default();
    assert_eq!(
        spawned.len(),
        2,
        "both members spawn as ZSoldier, the InitialPayload template"
    );
    assert!(
        members.is_empty(),
        "neither member joins a slot - ZSoldier is not equivalent to ZOther"
    );
}

#[test]
fn an_unjoined_member_walks_on_to_the_hordes_own_slot_zero() {
    let m = load();
    let mut simulation =
        Simulation::new(41, rules(&m, &[("ZBarracks", 0.0, 0.0, PLAYER)])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let barracks = placed(&simulation, ids, &m, "ZBarracks")[0];
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
        .submit(PLAYER, RotwkCommand::QueueUnit { button: 2 })
        .unwrap();
    let soldier_kind = m.kinds.kind("ZSoldier").unwrap();
    let mut found = None;
    for _ in 0..90 {
        simulation.tick().unwrap();
        let horde = simulation
            .state()
            .store(ids.horde)
            .and_then(|store| store.entities().first().copied());
        let member = simulation.state().store(ids.kind).and_then(|store| {
            store
                .iter()
                .find(|(entity, kind)| {
                    simulation.state().is_live(*entity) && **kind == soldier_kind
                })
                .map(|(entity, _)| entity)
        });
        if let (Some(horde), Some(member)) = (horde, member) {
            if let Some(path) = simulation.state().get(ids.path, member) {
                found = Some((horde, path.goal));
                break;
            }
        }
    }
    let (horde, goal) = found.expect("an unjoined member spawns with a route");
    let slot_zero =
        crate::horde::horde_slot_pose(simulation.state(), RotwkRules::horde_ids(ids), horde, 0)
            .expect("the horde has a formation");
    assert!(
        goal.minus(slot_zero).length() < 1e-3,
        "goal {goal:?}, expected slot-0 pose {slot_zero:?}"
    );
}

const LEAVING: &str = "IS_LEAVING_FACTORY";

#[test]
fn production_marks_what_it_makes_leaving_the_factory_until_the_exit_ends() {
    let m = load();
    let (mut simulation, ids, _home, horde) = produce_horde(&m);
    let carries = |simulation: &Simulation<RotwkRules>, entity| {
        simulation
            .rules()
            .carries_object_status(simulation.state(), entity, LEAVING)
    };
    assert!(carries(&simulation, horde), "the shell is marked");
    for _ in 0..8 {
        simulation.tick().unwrap();
    }
    let member = simulation.state().get(ids.horde, horde).unwrap().members[0];
    assert!(
        simulation.state().contains(ids.path, member),
        "still walking"
    );
    assert!(carries(&simulation, member), "the member is marked");
    for _ in 0..200 {
        simulation.tick().unwrap();
    }
    assert!(!simulation.state().contains(ids.path, member));
    assert!(!carries(&simulation, member), "its walk ended");
}

#[test]
fn under_no_exit_path_a_streamed_member_gets_no_route_and_no_mark() {
    let m = load();
    let (mut simulation, ids, _home, horde) = produce_horde_at(&m, "ZGateless");
    for _ in 0..8 {
        simulation.tick().unwrap();
    }
    let member = simulation.state().get(ids.horde, horde).unwrap().members[0];
    assert!(!simulation.state().contains(ids.path, member));
    assert!(!simulation
        .rules()
        .carries_object_status(simulation.state(), member, LEAVING));
    assert!(!simulation
        .rules()
        .carries_object_status(simulation.state(), horde, LEAVING));
}

#[test]
fn a_horde_shell_keeps_leaving_the_factory_while_a_member_still_walks_out() {
    let m = load();
    let (mut simulation, ids, _home, horde) = produce_horde(&m);
    let carries = |simulation: &Simulation<RotwkRules>, entity| {
        simulation
            .rules()
            .carries_object_status(simulation.state(), entity, LEAVING)
    };
    let made = simulation.state().get(ids.factory_exit, horde).unwrap().0;
    let second = mint_sim::LogicFrames::from_engine_milliseconds(1000);
    let mut walked_past_a_second = false;
    for _ in 0..300 {
        simulation.tick().unwrap();
        let members = simulation
            .state()
            .get(ids.horde, horde)
            .map(|contain| contain.members.clone())
            .unwrap_or_default();
        let walking = members.iter().any(|member| carries(&simulation, *member));
        let old_enough = made
            .checked_add(second)
            .is_some_and(|due| simulation.frame() > due);
        if walking && old_enough {
            walked_past_a_second = true;
            assert!(carries(&simulation, horde), "the shell cleared early");
        }
    }
    assert!(walked_past_a_second, "no member walked past the second");
}

#[test]
fn a_no_exit_path_member_stands_at_its_create_point_until_the_burst_ends() {
    let m = load();
    let (mut simulation, ids, _home, horde) = produce_horde_at(&m, "ZGateless");
    let mut first: Option<(Entity, Vec2)> = None;
    let mut held_ticks = 0u32;
    for _ in 0..200 {
        simulation.tick().unwrap();
        let streaming = simulation.state().contains(ids.production_stream, horde);
        let members = simulation
            .state()
            .get(ids.horde, horde)
            .unwrap()
            .members
            .clone();
        if let Some(&member) = members.first() {
            let at = position(&simulation, ids, member);
            let (_, start) = *first.get_or_insert((member, at));
            if streaming {
                held_ticks += 1;
                assert!(
                    at.minus(start).length() < 1.0,
                    "walked at {at:?} mid-stream"
                );
            }
        }
        if !streaming && first.is_some() {
            break;
        }
    }
    assert!(held_ticks >= 3, "the member was never watched mid-stream");
    let (member, start) = first.unwrap();
    for _ in 0..300 {
        simulation.tick().unwrap();
    }
    assert!(
        position(&simulation, ids, member).minus(start).length() > 20.0,
        "never walked to its slot after the burst"
    );
}
