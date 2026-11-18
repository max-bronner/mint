use super::*;

const DATA: &str = "\
#define WALKER_SPEED 48\n\
Locomotor WalkerLocomotor\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  \
  TurnTime = 500\nEnd\n\
Object Walker\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = WalkerLocomotor\n    Condition = SET_NORMAL\n    \
    Speed = WALKER_SPEED\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

#[test]
fn a_unit_on_a_long_straight_leg_covers_its_declared_speed_each_simulated_second() {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(&data, &["Walker".to_owned()]);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let walker = kinds
        .spawn(
            &data.combat,
            kinds.kind("Walker").unwrap(),
            Vec2::new(100.0, 480.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, vec![walker])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production);
    let mut simulation = Simulation::new(5, rules).unwrap();
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
                x: 850.0,
                y: 480.0,
                queue: false,
            },
        )
        .unwrap();
    let second = SIMULATION_HZ;
    for _ in 0..2 * second {
        simulation.tick().unwrap();
    }
    let from = position(&simulation, ids, unit);
    for _ in 0..second {
        simulation.tick().unwrap();
    }
    let covered = position(&simulation, ids, unit).minus(from).length();
    assert!(
        (covered - 48.0).abs() < 0.01,
        "covered {covered} world units in one simulated second, declared 48"
    );
}

#[test]
fn the_attack_state_uncohere_reach_is_vision_or_600_whichever_is_longer() {
    assert_eq!(RotwkRules::attack_uncohere_reach(100.0), 600.0);
    assert_eq!(RotwkRules::attack_uncohere_reach(600.0), 600.0);
    assert_eq!(RotwkRules::attack_uncohere_reach(900.0), 900.0);
}
