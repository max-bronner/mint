use std::sync::Arc;
use std::time::Instant;

use mint_sage_passability::{Footprint, FootprintRecord, Obstacle, WallGeometry};

use crate::templates::KindTable;

use super::*;

const DATA: &str = "\
Locomotor ClimberLocomotor\n  Surfaces = GROUND OBSTACLE\n  ScalesWalls = Yes\n  ZAxisBehavior = SCALING_WALLS\n  \
  Acceleration = 5000\n  Braking = 5000\n  TurnTime = 1\nEnd\n\
Object Climber\n  Side = Men\n  KindOf = CAN_CLIMB_WALLS SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = ClimberLocomotor\n    Condition = SET_NORMAL\n    \
    Speed = 300\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object HordeBody\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  BuildCost = 1\n  BuildTime = 1\n  \
  LocomotorSet\n    Locomotor = ClimberLocomotor\n    Condition = SET_NORMAL\n    \
    Speed = 300\n  End\n  \
  Behavior = HordeContain Tag_Contain\n    Slots = 1\n    \
    RankInfo = RankNumber:1 UnitType:Climber Position:X:0 Y:0\n  End\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_HordeBody\n  2 = Build_MachineHordeBody\nEnd\n\
CommandButton Build_HordeBody\n  Command = UNIT_BUILD\n  Object = HordeBody\nEnd\n\
CommandButton Build_MachineHordeBody\n  Command = UNIT_BUILD\n  Object = MachineHordeBody\nEnd\n\
Object WallSeg\n  Side = Men\n  KindOf = STRUCTURE IMMOBILE SELECTABLE SCALEABLE_WALL\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 500\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 40\n  GeometryMinorRadius = 10\n  \
  GeometryHeight = 30\nEnd\n\
Weapon GrabClaw\n  AttackRange = 300\n  DelayBetweenShots = 500\n  \
  GrabNugget\n    ContainTargetOnEffect = Yes\n  End\nEnd\n\
Object Grabber\n  Side = Wild\n  KindOf = SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY GrabClaw\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 100\n  End\n  \
  Behavior = TransportContain Tag_Claws\n    \
    ObjectStatusOfContained = UNSELECTABLE UNATTACKABLE\n    \
    PassengerFilter = ANY +CAN_CLIMB_WALLS\n    Slots = 1\n    \
    AllowEnemiesInside = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 20\nEnd\n\
Object MachineMember\n  Side = Men\n  KindOf = CAN_CLIMB_WALLS MACHINE SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = ClimberLocomotor\n    Condition = SET_NORMAL\n    \
    Speed = 300\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object MachineHordeBody\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  BuildCost = 1\n  \
  BuildTime = 1\n  \
  LocomotorSet\n    Locomotor = ClimberLocomotor\n    Condition = SET_NORMAL\n    \
    Speed = 300\n  End\n  \
  Behavior = HordeContain Tag_Contain\n    Slots = 1\n    \
    RankInfo = RankNumber:1 UnitType:MachineMember Position:X:0 Y:0\n  End\nEnd\n";

fn wall_obstacle(height: f32) -> (Obstacle, WallGeometry) {
    let centre = Vec2::new(100.0, 50.0);
    let records = vec![FootprintRecord {
        offset: Vec2::new(0.0, 0.0),
        footprint: Footprint::Box {
            major: 40.0,
            minor: 100.0,
        },
    }];
    (
        Obstacle {
            id: 0,
            centre,
            angle: 0.0,
            records,
        },
        WallGeometry {
            id: 0,
            centre,
            angle: 0.0,
            major_radius: 40.0,
            minor_radius: 10.0,
            height,
        },
    )
}

fn climber_kinds(seeds: &[&str]) -> (crate::combat::GameData, Arc<KindTable>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, Instant::now());
    let owned: Vec<String> = seeds.iter().map(|s| (*s).to_owned()).collect();
    let (kinds, missing) = KindTable::closure(&data, &owned);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    (data, Arc::new(kinds))
}

fn wears(simulation: &Simulation<RotwkRules>, entity: Entity, condition: &str) -> bool {
    let state = simulation.rules().upgrade_state_id().unwrap();
    simulation
        .state()
        .get(state, entity)
        .is_some_and(|s| s.model_conditions.iter().any(|n| n.as_ref() == condition))
}

fn elevation(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    simulation
        .state()
        .get(ids.elevation, entity)
        .map_or(0.0, |e| e.0)
}

#[test]
fn a_climber_reaches_a_thirty_high_wall_s_top_in_the_climb_rate_s_own_frame_count() {
    let (data, kinds) = climber_kinds(&["Climber"]);
    let kind = kinds.kind("Climber").unwrap();
    let climber = kinds
        .spawn(
            &data.combat,
            kind,
            Vec2::new(20.0, 50.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let (obstacle, wall) = wall_obstacle(30.0);
    let passability = Passability::builder(&flat_map(64, 16), 5.0 / 128.0)
        .obstacles(&[obstacle])
        .scaleable_obstacles(&[0])
        .build()
        .unwrap();
    let production = kinds.production(&data);
    let rules = RotwkRules::new(passability, vec![climber])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_wall_geometry(vec![wall]);
    let mut simulation = Simulation::new(11, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let unit = simulation.state().store(ids.body).unwrap().entities()[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![unit],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 200.0,
                y: 50.0,
                queue: false,
            },
        )
        .unwrap();
    let mut climbing_frames = 0u32;
    let mut reached = false;
    let mut previous = elevation(&simulation, ids, unit);
    for _ in 0..300 {
        simulation.tick().unwrap();
        let now = elevation(&simulation, ids, unit);
        if wears(&simulation, unit, "CLIMBING") {
            if now > previous {
                climbing_frames += 1;
            }
            if now >= 30.0 - f32::EPSILON {
                reached = true;
                break;
            }
        }
        previous = now;
    }
    assert!(reached, "the climber never reached the wall's top");
    assert_eq!(
        climbing_frames, 5,
        "ceil(30 / (0.6 * 10)) = 5 climbing frames at this speed"
    );
}

#[test]
fn a_horde_member_never_climbs_on_its_own() {
    let (data, kinds) = climber_kinds(&["Camp", "HordeBody", "Climber"]);
    let camp = kinds.kind("Camp").unwrap();
    let camp_spawn = kinds
        .spawn(
            &data.combat,
            camp,
            Vec2::new(20.0, 400.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let (obstacle, wall) = wall_obstacle(30.0);
    let passability = Passability::builder(&flat_map(96, 96), 5.0 / 128.0)
        .obstacles(&[obstacle])
        .scaleable_obstacles(&[0])
        .build()
        .unwrap();
    let production = kinds.production(&data);
    let rules = RotwkRules::new(passability, vec![camp_spawn])
        .with_players(2)
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_wall_geometry(vec![wall]);
    let mut simulation = Simulation::new(17, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "HordeBody")
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 20.0,
                y: 50.0,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    simulation.tick().unwrap();
    let horde = simulation.state().store(ids.horde).unwrap().entities()[0];
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
            RotwkCommand::MoveTo {
                x: 200.0,
                y: 50.0,
                queue: false,
            },
        )
        .unwrap();
    let members: Vec<Entity> = simulation
        .state()
        .store(ids.member)
        .unwrap()
        .entities()
        .to_vec();
    assert!(
        !members.is_empty(),
        "the horde must have at least one member"
    );
    for _ in 0..300 {
        simulation.tick().unwrap();
        for &member in &members {
            assert!(
                !wears(&simulation, member, "CLIMBING"),
                "a horde member must never climb on its own"
            );
            assert!(
                elevation(&simulation, ids, member) < 1.0,
                "a horde member must never rise off the ground on its own"
            );
        }
    }
}

#[test]
fn a_climber_off_centre_climbs_and_keeps_its_own_offset() {
    let (data, kinds) = climber_kinds(&["Climber"]);
    let kind = kinds.kind("Climber").unwrap();
    let climber = kinds
        .spawn(
            &data.combat,
            kind,
            Vec2::new(20.0, 55.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let (obstacle, wall) = wall_obstacle(30.0);
    let passability = Passability::builder(&flat_map(64, 16), 5.0 / 128.0)
        .obstacles(&[obstacle])
        .scaleable_obstacles(&[0])
        .build()
        .unwrap();
    let production = kinds.production(&data);
    let rules = RotwkRules::new(passability, vec![climber])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_wall_geometry(vec![wall]);
    let mut simulation = Simulation::new(13, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let unit = simulation.state().store(ids.body).unwrap().entities()[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![unit],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 200.0,
                y: 55.0,
                queue: false,
            },
        )
        .unwrap();
    let mut climbed = false;
    for _ in 0..300 {
        simulation.tick().unwrap();
        if wears(&simulation, unit, "CLIMBING") {
            climbed = true;
            break;
        }
    }
    assert!(climbed, "an off-centre climber must still reach the wall");
    let body = simulation.state().get(ids.body, unit).unwrap();
    assert!(
        (body.0.position.y - 55.0).abs() < 1.0,
        "the snap must keep the climber's own across-wall offset, not the wall's centre \
         line: y = {}",
        body.0.position.y
    );
}

#[test]
fn wall_at_rejects_a_probe_far_off_a_collinear_wall_s_line() {
    let (data, kinds) = climber_kinds(&["WallSeg"]);
    let kind = kinds.kind("WallSeg").unwrap();
    let wall_a = kinds
        .spawn(
            &data.combat,
            kind,
            Vec2::new(100.0, 50.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let wall_b = kinds
        .spawn(
            &data.combat,
            kind,
            Vec2::new(300.0, 50.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let records = vec![FootprintRecord {
        offset: Vec2::ZERO,
        footprint: Footprint::Box {
            major: 40.0,
            minor: 10.0,
        },
    }];
    let passability = Passability::build(&flat_map(128, 64), 5.0 / 128.0).unwrap();
    let production = kinds.production(&data);
    let rules = RotwkRules::new(passability, vec![wall_a, wall_b])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_structure_footprints(vec![(0, records.clone()), (1, records)]);
    let simulation = Simulation::new(19, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let far_off_line = Vec2::new(100.0, 200.0);
    assert!(
        simulation
            .rules()
            .wall_at(simulation.state(), ids, far_off_line)
            .is_none(),
        "a probe far off a collinear wall's own line must not resolve to it"
    );
    let against_the_face = Vec2::new(100.0, 55.0);
    assert!(
        simulation
            .rules()
            .wall_at(simulation.state(), ids, against_the_face)
            .is_some(),
        "a probe within both radii of the wall must still resolve to it"
    );
}

#[test]
fn the_probe_cap_is_scaled_from_retail_frames_to_mint_ticks() {
    let (data, kinds) = climber_kinds(&["Climber"]);
    let kind = kinds.kind("Climber").unwrap();
    let climber = kinds
        .spawn(
            &data.combat,
            kind,
            Vec2::new(20.0, 50.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let (obstacle, wall) = wall_obstacle(30.0);
    let passability = Passability::builder(&flat_map(64, 16), 5.0 / 128.0)
        .obstacles(&[obstacle])
        .scaleable_obstacles(&[0])
        .build()
        .unwrap();
    let production = kinds.production(&data);
    let rules = RotwkRules::new(passability, vec![climber])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_wall_geometry(vec![wall]);
    let mut simulation = Simulation::new(23, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let unit = simulation.state().store(ids.body).unwrap().entities()[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![unit],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 200.0,
                y: 50.0,
                queue: false,
            },
        )
        .unwrap();
    let mut last_contact_x = None;
    for _ in 0..300 {
        let before = simulation.state().get(ids.body, unit).unwrap().0.position.x;
        simulation.tick().unwrap();
        if wears(&simulation, unit, "CLIMBING") {
            last_contact_x = Some(before);
            break;
        }
    }
    let last_contact_x =
        last_contact_x.expect("the climber must reach the wall and start climbing");
    assert!(
        last_contact_x > 57.0,
        "with the correctly-scaled per-tick cap (~1.1667), the climber must still be within \
         ~1.1667 of the wall's edge (x = 60) on the last tick before contact: got {last_contact_x}, \
         which the raw retail-per-frame cap (7.0) would have allowed from as far back as x = 53"
    );
}

fn grab_a_member_out_of_its_horde(
    horde_body: &str,
    member: &str,
) -> (Simulation<RotwkRules>, Ids, Entity) {
    let (data, kinds) = climber_kinds(&["Camp", horde_body, member, "Grabber"]);
    let camp = kinds.kind("Camp").unwrap();
    let camp_spawn = kinds
        .spawn(
            &data.combat,
            camp,
            Vec2::new(20.0, 400.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let grabber = kinds.kind("Grabber").unwrap();
    let grabber_spawn = kinds
        .spawn(
            &data.combat,
            grabber,
            Vec2::new(40.0, 50.0),
            Facing::EAST,
            OTHER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let production = kinds.production(&data);
    let rules = RotwkRules::new(passability, vec![camp_spawn, grabber_spawn])
        .with_players(2)
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone());
    let mut simulation = Simulation::new(29, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let grabber_entity = simulation
        .state()
        .store(ids.owner)
        .unwrap()
        .iter()
        .find(|(_, owner)| owner.0 == OTHER)
        .map(|(entity, _)| entity)
        .unwrap();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == horde_body)
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 20.0,
                y: 50.0,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    simulation.tick().unwrap();
    let member_entity = simulation.state().store(ids.member).unwrap().entities()[0];
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![grabber_entity],
            },
        )
        .unwrap();
    simulation
        .submit(
            OTHER,
            RotwkCommand::AttackObject {
                target: member_entity,
            },
        )
        .unwrap();
    for _ in 0..150 {
        simulation.tick().unwrap();
        if simulation
            .state()
            .get(ids.passengers, grabber_entity)
            .is_some_and(|held| !held.0.is_empty())
        {
            return (simulation, ids, member_entity);
        }
    }
    panic!("the grabber never held the member within 150 ticks");
}

#[test]
fn a_grabbed_ordinary_member_keeps_its_sticky_horde_member_mark() {
    let (simulation, ids, member) = grab_a_member_out_of_its_horde("HordeBody", "Climber");
    assert!(
        !simulation.state().contains(ids.member, member),
        "the grabbed member must have left its horde's own Member component"
    );
    assert!(
        simulation.state().contains(ids.horde_member_mark, member),
        "an ordinary member must keep its sticky horde-member mark after leaving alive"
    );
}

#[test]
fn a_grabbed_machine_member_loses_its_sticky_horde_member_mark() {
    let (simulation, ids, member) =
        grab_a_member_out_of_its_horde("MachineHordeBody", "MachineMember");
    assert!(
        !simulation.state().contains(ids.member, member),
        "the grabbed member must have left its horde's own Member component"
    );
    assert!(
        !simulation.state().contains(ids.horde_member_mark, member),
        "a MACHINE member must lose its sticky horde-member mark after leaving alive"
    );
}

#[test]
fn a_horde_member_climbs_a_live_scaleable_wall_and_settles_clear_of_it() {
    let (data, kinds) = climber_kinds(&["Camp", "HordeBody", "Climber", "WallSeg"]);
    let camp = kinds.kind("Camp").unwrap();
    let camp_spawn = kinds
        .spawn(
            &data.combat,
            camp,
            Vec2::new(20.0, 400.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let wall_kind = kinds.kind("WallSeg").unwrap();
    let wall_spawn = kinds
        .spawn(
            &data.combat,
            wall_kind,
            Vec2::new(100.0, 50.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let records = vec![FootprintRecord {
        offset: Vec2::ZERO,
        footprint: Footprint::Box {
            major: 40.0,
            minor: 10.0,
        },
    }];
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let production = kinds.production(&data);
    let rules = RotwkRules::new(passability, vec![camp_spawn, wall_spawn])
        .with_players(2)
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_structure_footprints(vec![(1, records)]);
    let mut simulation = Simulation::new(29, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "HordeBody")
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 20.0,
                y: 50.0,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    simulation.tick().unwrap();
    let horde = simulation.state().store(ids.horde).unwrap().entities()[0];
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
            RotwkCommand::MoveTo {
                x: 200.0,
                y: 50.0,
                queue: false,
            },
        )
        .unwrap();
    let members: Vec<Entity> = simulation
        .state()
        .store(ids.member)
        .unwrap()
        .entities()
        .to_vec();
    assert!(
        !members.is_empty(),
        "the horde must have at least one member"
    );
    let member = members[0];
    let mut climbed = false;
    let mut rose = false;
    for _ in 0..600 {
        simulation.tick().unwrap();
        if wears(&simulation, member, "CLIMBING") {
            climbed = true;
        }
        if elevation(&simulation, ids, member) > 1.0 {
            rose = true;
        }
    }
    assert!(climbed, "the member must climb while crossing the wall");
    assert!(rose, "the member must rise off the ground while climbing");
    assert!(
        !wears(&simulation, member, "SCALING_WALL_HORDE"),
        "the member must have settled clear of the wall by 600 ticks"
    );
    assert!(
        elevation(&simulation, ids, member) < 1.0,
        "the member must be back on the ground by 600 ticks"
    );
}
