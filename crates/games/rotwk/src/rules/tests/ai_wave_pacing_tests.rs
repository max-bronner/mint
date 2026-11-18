use super::*;

const GOAL: Vec2 = Vec2::new(900.0, 150.0);

#[test]
fn a_computer_waves_members_are_capped_by_their_bands() {
    let at = [
        Vec2::new(600.0, 150.0),
        Vec2::new(700.0, 150.0),
        Vec2::new(550.0, 150.0),
        Vec2::new(480.0, 150.0),
    ];
    let passability = Passability::build(&flat_map(128, 128), 5.0 / 128.0).unwrap();
    let spawns = at
        .iter()
        .map(|p| spawn_at(p.x, p.y, KindOf::SELECTABLE, PLAYER))
        .collect();
    let rules = RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_skirmish_ai(
            vec![crate::skirmish_ai::AiSeed {
                player: PLAYER,
                candidates: Vec::new(),
            }],
            crate::combat::AiTiming::default(),
            Vec::new(),
        );
    let mut simulation = Simulation::new(7, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: units.clone(),
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: GOAL.x,
                y: GOAL.y,
                queue: false,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::Debug(crate::rules::debug::DebugCommand::StartAttackWave),
        )
        .unwrap();
    simulation.tick().unwrap();
    simulation.tick().unwrap();
    let cap = |unit: Entity| {
        simulation
            .state()
            .get(ids.formation_speed, unit)
            .map(|cap| cap.speed)
    };
    let slowest = cap(units[0]).expect("the lead is paced at the slowest");
    assert_eq!(cap(units[1]), Some(slowest * 0.7));
    assert_eq!(cap(units[2]), Some(slowest));
    assert_eq!(cap(units[3]), None);
}
