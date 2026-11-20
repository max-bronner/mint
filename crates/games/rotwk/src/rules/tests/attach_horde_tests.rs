use super::*;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Object Pikeman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 4000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object TestHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  CommandSet = HordeSet\n  \
  BuildCost = 100\n  BuildTime = 1\n  CommandPoints = 3\n  VisionRange = 100\n  \
  Behavior = HordeContain Tag_01\n    Slots = 3\n    \
    RankInfo = RankNumber:1 UnitType:Pikeman Position:X:10 Y:-10 Position:X:10 Y:10 Position:X:-10 Y:0\n  \
  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 30
  GeometryMinorRadius = 45\n  GeometryHeight = 20\nEnd\n\
Object Crown\n  Side = Men\n  KindOf = SELECTABLE IMMOBILE\n  \
  Behavior = AttachUpdate Tag_Attach\n    ObjectFilter = ANY\n    ScanRange = 50\n    \
    AnchorToTopOfGeometry = Yes\n  End\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 2\n  GeometryHeight = 2\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = HordeSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet HordeSet\n  1 = Build_TestHorde\nEnd\n\
CommandButton Build_TestHorde\n  Command = UNIT_BUILD\n  Object = TestHorde\nEnd\n";

fn horde_scene(crown_x: f32) -> (Simulation<RotwkRules>, Ids) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds = ["Camp", "TestHorde", "Crown"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns = vec![kinds
        .spawn(
            &data.combat,
            kinds.kind("Crown").unwrap(),
            Vec2::new(crown_x, 300.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap()];
    let passability = Passability::build(&flat_map(160, 160), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone());
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "TestHorde")
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 300.0,
                y: 300.0,
                facing: 0.0,
            },
        )
        .unwrap();
    for _ in 0..4 {
        simulation.tick().unwrap();
    }
    (simulation, ids)
}

#[test]
fn a_crown_on_a_horde_follows_the_members_mean_and_the_hordes_geometry_top() {
    let (mut simulation, ids) = horde_scene(305.0);
    let crown = simulation
        .state()
        .store(ids.attach_link)
        .unwrap()
        .entities()[0];
    let horde = simulation.state().store(ids.horde).unwrap().entities()[0];
    simulation.tick().unwrap();

    let world = simulation.state();
    let members = &world.get(ids.horde, horde).unwrap().members;
    assert_eq!(members.len(), 3);
    let mean = |axis: fn(Vec2) -> f32| {
        members
            .iter()
            .map(|m| axis(world.get(ids.body, *m).unwrap().0.position))
            .sum::<f32>()
            / 3.0
    };
    let (mean_x, mean_y) = (mean(|p| p.x), mean(|p| p.y));
    let (point, _) = RotwkRules::follow_point(world, ids, horde, Vec2::new(-999.0, -999.0));
    assert!((point.x - mean_x).abs() < 1e-3 && (point.y - mean_y).abs() < 1e-3);
    let at = world.get(ids.body, crown).unwrap().0.position;
    assert!((at.x - mean_x).abs() < 1.0 && (at.y - mean_y).abs() < 1.0);
    let mean_z = members
        .iter()
        .map(|m| world.get(ids.elevation, *m).map_or(0.0, |e| e.0))
        .sum::<f32>()
        / 3.0;
    let z = world.get(ids.elevation, crown).unwrap().0;
    assert!(
        (z - (mean_z + 20.0)).abs() < 1.0,
        "z {z} mean feet {mean_z}"
    );
}

#[test]
fn a_horde_is_found_by_the_scan_through_its_own_bounding_circle() {
    let (mut simulation, ids) = horde_scene(366.0);
    for _ in 0..2 {
        simulation.tick().unwrap();
    }
    let crown = simulation.state().store(ids.body).unwrap().entities()[0];
    let horde = simulation.state().store(ids.horde).unwrap().entities()[0];
    let link = simulation.state().get(ids.attach_link, crown).copied();
    assert_eq!(link.map(|l| l.parent), Some(horde));
}
