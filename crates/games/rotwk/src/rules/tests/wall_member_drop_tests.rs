use crate::wall_meshes::WallBounds;

use super::*;

const DATA: &str = "\
Locomotor MemberFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 500\nEnd\n\
Locomotor HordeFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 1000\n  MaxTurnWithoutReform = 45\nEnd\n\
Armor Plain\n  Armor = DEFAULT 100%\nEnd\n\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 600\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
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
Object Wall\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 30\n  GeometryMinorRadius = 30\n  \
  GeometryHeight = 40\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_Tight\nEnd\n\
CommandButton Build_Tight\n  Command = UNIT_BUILD\n  Object = Tight\nEnd\n";

const WALL_HEIGHT: f32 = 40.0;
const WALL_AT: Vec2 = Vec2::new(200.0, 200.0);

fn placed_on_a_wall() -> (Simulation<RotwkRules>, Ids, Entity, Entity, Vec<Entity>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Camp", "Tight", "Wall"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let wall_kind = kinds.kind("Wall").unwrap();
    let placed = [("Camp", 500.0, 500.0, PLAYER), ("Wall", 0.0, 0.0, PLAYER)];
    let spawns: Vec<Spawn> = placed
        .iter()
        .map(|(name, x, y, owner)| {
            let kind = kinds.kind(name).unwrap();
            kinds
                .spawn(&data.combat, kind, Vec2::new(*x, *y), Facing::EAST, *owner)
                .unwrap()
        })
        .collect();
    let mut bounds = vec![None; usize::from(wall_kind.0) + 1];
    bounds[usize::from(wall_kind.0)] = Some(WallBounds {
        perimeter: vec![
            (WALL_AT.x - 100.0, WALL_AT.y - 100.0),
            (WALL_AT.x + 100.0, WALL_AT.y - 100.0),
            (WALL_AT.x + 100.0, WALL_AT.y + 100.0),
            (WALL_AT.x - 100.0, WALL_AT.y + 100.0),
        ],
        height: WALL_HEIGHT,
    });
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_structure_footprints(vec![(1, Vec::new())])
        .with_wall_bounds(bounds);
    let mut simulation = Simulation::new(29, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let wall = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .iter()
        .find(|entity| {
            simulation
                .state()
                .get(ids.body, **entity)
                .is_some_and(|body| body.0.position == Vec2::ZERO)
        })
        .copied()
        .unwrap();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "Tight")
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: WALL_AT.x,
                y: WALL_AT.y,
                facing: 0.0,
            },
        )
        .unwrap();
    for _ in 0..5 {
        simulation.tick().unwrap();
    }
    let horde = simulation
        .state()
        .store(ids.horde)
        .unwrap()
        .entities()
        .last()
        .copied()
        .unwrap();
    let members = simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .clone();
    (simulation, ids, horde, wall, members)
}

fn member_elevations(
    simulation: &Simulation<RotwkRules>,
    ids: Ids,
    members: &[Entity],
) -> Vec<f32> {
    members
        .iter()
        .map(|member| simulation.state().get(ids.elevation, *member).unwrap().0)
        .collect()
}

fn kill(simulation: &mut Simulation<RotwkRules>, target: Entity) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![target],
            },
        )
        .unwrap();
    simulation.submit(PLAYER, RotwkCommand::DebugKill).unwrap();
}

#[test]
fn members_hold_on_a_living_wall() {
    let (mut simulation, ids, _horde, _wall, members) = placed_on_a_wall();
    for z in member_elevations(&simulation, ids, &members) {
        assert!(
            (z - WALL_HEIGHT).abs() < 1.0,
            "member at {z}, expected {WALL_HEIGHT}"
        );
    }
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    for z in member_elevations(&simulation, ids, &members) {
        assert!(
            (z - WALL_HEIGHT).abs() < 1.0,
            "member drifted to {z} on a wall that never died"
        );
    }
}

#[test]
fn an_idle_hordes_members_drop_once_their_wall_dies() {
    let (mut simulation, ids, _horde, wall, members) = placed_on_a_wall();
    kill(&mut simulation, wall);
    for _ in 0..8 {
        simulation.tick().unwrap();
    }
    for z in member_elevations(&simulation, ids, &members) {
        assert!(
            z.abs() < 1.0,
            "member still at {z}, expected the ground within a few ticks"
        );
    }
}

#[test]
fn a_moving_hordes_members_hold_until_it_stops() {
    let (mut simulation, ids, horde, wall, members) = placed_on_a_wall();
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
                x: WALL_AT.x + 20.0,
                y: WALL_AT.y,
                queue: false,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    kill(&mut simulation, wall);
    for _ in 0..3 {
        simulation.tick().unwrap();
        assert!(
            simulation.state().contains(ids.path, horde),
            "the horde should still be mid-move for this assertion to test anything"
        );
        for z in member_elevations(&simulation, ids, &members) {
            assert!(
                (z - WALL_HEIGHT).abs() < 1.0,
                "member dropped to {z} mid-move, before the horde stopped"
            );
        }
    }
    for _ in 0..60 {
        simulation.tick().unwrap();
        if !simulation.state().contains(ids.path, horde) {
            break;
        }
    }
    assert!(
        !simulation.state().contains(ids.path, horde),
        "the horde should have stopped by now"
    );
    for _ in 0..8 {
        simulation.tick().unwrap();
    }
    for z in member_elevations(&simulation, ids, &members) {
        assert!(z.abs() < 1.0, "member still at {z} after the horde stopped");
    }
}
