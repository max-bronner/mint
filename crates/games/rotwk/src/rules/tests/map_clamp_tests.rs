use super::*;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Object Walker\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn after_an_order_to(goal: Vec2) -> Vec2 {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(&data, &["Walker".to_owned()]);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let unit = kinds
        .spawn(
            &data.combat,
            kinds.kind("Walker").unwrap(),
            Vec2::new(100.0, 480.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![unit])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entity = simulation.state().store(ids.body).unwrap().entities()[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![entity],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: goal.x,
                y: goal.y,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..(10 * SIMULATION_HZ) {
        simulation.tick().unwrap();
    }
    position(&simulation, ids, entity)
}

#[test]
fn a_move_goal_past_the_map_edge_is_clamped_to_the_margin() {
    let at = after_an_order_to(Vec2::new(-500.0, 480.0));
    assert!(
        (10.0..30.0).contains(&at.x),
        "must stop near the half-partition-cell margin (20), not at {at:?}"
    );
}

#[test]
fn a_move_goal_inside_the_map_is_not_moved() {
    let at = after_an_order_to(Vec2::new(300.0, 480.0));
    assert!(
        (290.0..310.0).contains(&at.x),
        "an interior goal must not be nudged, landed at {at:?}"
    );
}
