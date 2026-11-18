use super::*;

const DATA: &str = "\
Locomotor CrowLocomotor\n  Surfaces = AIR\n  Appearance = WINGS\n  Acceleration = 100\n  Braking = 100\n  \
  TurnTime = 500\n  MinSpeed = 50%\n  CirclingRadius = 40\n  PreferredHeight = 50\n  \
  ZAxisBehavior = SURFACE_RELATIVE_HEIGHT\nEnd\n\
Object Crow\n  Side = Men\n  KindOf = AIRCRAFT SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = CrowLocomotor\n    Condition = SET_NORMAL\n    \
    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

#[test]
fn a_flier_that_has_stopped_keeps_circling_its_stopping_point() {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(&data, &["Crow".to_owned()]);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let unit = kinds
        .spawn(
            &data.combat,
            kinds.kind("Crow").unwrap(),
            Vec2::new(300.0, 300.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let rules = RotwkRules::new(
        Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap(),
        vec![unit],
    )
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
                x: 500.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..SIMULATION_HZ * 12 {
        simulation.tick().unwrap();
    }
    let stopped = simulation
        .state()
        .get(ids.maintain_point, entity)
        .map(|point| point.0)
        .expect("the flier never took a circling point");
    let mut farthest = 0.0_f32;
    let mut moved = 0.0_f32;
    let mut last = simulation.state().get(ids.body, entity).unwrap().0.position;
    for _ in 0..SIMULATION_HZ * 10 {
        simulation.tick().unwrap();
        let at = simulation.state().get(ids.body, entity).unwrap().0.position;
        farthest = farthest.max(at.minus(stopped).length());
        moved += at.minus(last).length();
        last = at;
    }
    assert!(
        (270.0..330.0).contains(&moved),
        "moved {moved}, want half the set speed's 30 a second over 10 s"
    );
    assert!(farthest < 80.0, "strayed {farthest}");
}
