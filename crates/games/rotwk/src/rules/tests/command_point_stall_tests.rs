use super::*;

const DATA: &str = "\
Object Fortress\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  CommandSet = FortSet\n  BuildCost = 1000\n  BuildTime = 1\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 10000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 40\n  GeometryHeight = 40\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  CommandSet = CampSet\n  BuildCost = 1000\n  BuildTime = 1\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 10000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 40\n  GeometryHeight = 40\nEnd\n\
CommandSet FortSet\n  1 = Build_Soldier\nEnd\n\
CommandSet CampSet\n  1 = Build_Filler\nEnd\n\
CommandButton Build_Soldier\n  Command = UNIT_BUILD\n  Object = Soldier\nEnd\n\
CommandButton Build_Filler\n  Command = UNIT_BUILD\n  Object = Filler\nEnd\n\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 100\n  \
  BuildTime = 2\n  CommandPoints = 20\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Filler\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 100\n  \
  BuildTime = 1\n  CommandPoints = 15\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn match_with_two_producers() -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let data = crate::game::tests::data_from(DATA);
    let (kinds, missing) = KindTable::closure(
        &data,
        &["Fortress", "Camp", "Soldier", "Filler"].map(str::to_owned),
    );
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawn = |name: &str, x: f32, y: f32| {
        let kind = kinds.kind(name).unwrap();
        let mut spawn = kinds
            .spawn(&data.combat, kind, Vec2::new(x, y), Facing::EAST, PLAYER)
            .unwrap();
        spawn.producer = kinds.producer(&production, kind, Vec2::new(x, y), Facing::EAST);
        spawn
    };
    let spawns = vec![spawn("Fortress", 200.0, 200.0), spawn("Camp", 600.0, 200.0)];
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 10_000)
        .with_menus(production.menus.clone())
        .with_menu_needs(production.menu_needs.clone())
        .with_command_point_factor(10);
    let simulation = Simulation::new(71, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let fortress = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .find(|(entity, &k)| {
            k == kinds.kind("Fortress").unwrap() && simulation.state().is_live(*entity)
        })
        .map(|(entity, _)| entity)
        .unwrap();
    let camp = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .find(|(entity, &k)| {
            k == kinds.kind("Camp").unwrap() && simulation.state().is_live(*entity)
        })
        .map(|(entity, _)| entity)
        .unwrap();
    (simulation, ids, fortress, camp)
}

fn frames_done(simulation: &Simulation<RotwkRules>, ids: Ids, producer: Entity) -> Option<u32> {
    simulation
        .state()
        .get(ids.producer, producer)?
        .queue
        .entries()
        .first()
        .map(|entry| entry.frames_done)
}

#[test]
fn a_unit_whose_command_points_no_longer_fit_stalls_and_then_resumes() {
    let (mut simulation, ids, fortress, camp) = match_with_two_producers();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![fortress],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::QueueUnit { button: 0 })
        .unwrap();
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    let before = frames_done(&simulation, ids, fortress).unwrap();
    assert!(before > 0, "the soldier should have started building");

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![camp],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::QueueUnit { button: 0 })
        .unwrap();
    let mut filler = None;
    for _ in 0..60 {
        simulation.tick().unwrap();
        filler = simulation
            .state()
            .store(ids.kind)
            .unwrap()
            .iter()
            .find(|(entity, _)| {
                simulation.state().get(ids.owner, *entity).map(|o| o.0) == Some(PLAYER)
                    && *entity != fortress
                    && *entity != camp
            })
            .map(|(entity, _)| entity);
        if filler.is_some() {
            break;
        }
    }
    let filler = filler.expect("the filler should have come out of the camp");

    let stalled_at = frames_done(&simulation, ids, fortress).unwrap();
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    assert_eq!(
        frames_done(&simulation, ids, fortress).unwrap(),
        stalled_at,
        "the soldier must stall, not progress, once the filler takes the last free points"
    );

    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![filler],
            },
        )
        .unwrap();
    simulation.submit(PLAYER, RotwkCommand::DebugKill).unwrap();
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    assert!(
        frames_done(&simulation, ids, fortress).is_none_or(|done| done > stalled_at),
        "the soldier must resume once the filler's points are freed"
    );
}
