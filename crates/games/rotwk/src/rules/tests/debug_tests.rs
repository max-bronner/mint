use super::experience_tests::{load, rank, rules, run, Match};
use super::*;

fn spawn(simulation: &mut Simulation<RotwkRules>, m: &Match, owner: PlayerId, role: SpawnRole) {
    spawn_kind(simulation, m, "SoldierHorde", owner, role);
}

fn spawn_kind(
    simulation: &mut Simulation<RotwkRules>,
    m: &Match,
    kind_name: &str,
    owner: PlayerId,
    role: SpawnRole,
) {
    let kind = m.kinds.kind(kind_name).unwrap();
    simulation
        .submit(
            owner,
            RotwkCommand::Debug(DebugCommand::Spawn {
                kind: kind.0,
                x: 200.0,
                y: 200.0,
                facing: 0.0,
                role,
            }),
        )
        .unwrap();
    run(simulation, 1);
}

fn select(simulation: &mut Simulation<RotwkRules>, player: PlayerId, units: Vec<Entity>) {
    simulation
        .submit(
            player,
            RotwkCommand::SetSelection {
                replace: true,
                units,
            },
        )
        .unwrap();
}

fn attack_first_member(
    m: &Match,
    striker_at: (f32, f32),
    ticks: u32,
) -> (Simulation<RotwkRules>, Ids, Entity, Vec<Entity>) {
    attack_first_member_as(m, "Striker", striker_at, ticks)
}

fn attack_first_member_as(
    m: &Match,
    attacker_kind: &str,
    attacker_at: (f32, f32),
    ticks: u32,
) -> (Simulation<RotwkRules>, Ids, Entity, Vec<Entity>) {
    let mut simulation = Simulation::new(
        3,
        rules(m, &[(attacker_kind, attacker_at.0, attacker_at.1, PLAYER)]),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    spawn(&mut simulation, m, OTHER, SpawnRole::Dummy);
    let attacker = experience_tests::placed(&simulation, ids, m, attacker_kind)[0];
    let members = members_of(&simulation, ids);
    select(&mut simulation, PLAYER, vec![attacker]);
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: members[0] })
        .unwrap();
    run(&mut simulation, ticks);
    (simulation, ids, attacker, members)
}

#[test]
fn a_level_up_reaches_exactly_the_next_threshold_and_stops_at_the_last_level() {
    let m = load();
    let mut simulation = Simulation::new(3, rules(&m, &[])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    spawn(&mut simulation, &m, PLAYER, SpawnRole::Normal);
    let horde = horde_entity(&simulation, ids).unwrap();
    let members = members_of(&simulation, ids);
    select(&mut simulation, PLAYER, members);
    let mut seen = Vec::new();
    for _ in 0..3 {
        simulation
            .submit(
                PLAYER,
                RotwkCommand::Debug(DebugCommand::LevelUp { quarters: 4 }),
            )
            .unwrap();
        run(&mut simulation, 1);
        seen.push((
            rank(&simulation, horde),
            experience_tests::points(&simulation, ids, horde),
        ));
    }
    assert_eq!(
        seen,
        vec![
            (Some(2), Some(7)),
            (Some(3), Some(1000)),
            (Some(3), Some(1000))
        ]
    );
}

#[test]
fn a_dummy_horde_never_fires_back() {
    let m = load();
    let (simulation, ids, striker, _) = attack_first_member(&m, (260.0, 200.0), 90);
    assert_eq!(health_of(&simulation, ids, striker), Some(100_000.0));
}

#[test]
fn a_dummy_horde_does_not_retaliate_when_hit() {
    let m = load();
    let (simulation, ids, _, _) = attack_first_member(&m, (260.0, 200.0), 20);
    let horde = horde_entity(&simulation, ids).unwrap();
    assert!(!simulation.state().contains(ids.horde_target, horde));
}

#[test]
fn a_dummy_horde_member_dies_like_any_other() {
    let m = load();
    let (simulation, ids, _, members) = attack_first_member(&m, (260.0, 200.0), 600);
    let dead = members
        .iter()
        .filter(|member| {
            !(health_of(&simulation, ids, **member).is_some_and(|health| health > 0.0)
                && simulation.state().is_live(**member))
        })
        .count();
    assert!(dead > 0);
}

#[test]
fn a_dummy_horde_moves_for_whichever_player_orders_it() {
    let m = load();
    let mut simulation = Simulation::new(3, rules(&m, &[])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    spawn(&mut simulation, &m, OTHER, SpawnRole::Dummy);
    let members = members_of(&simulation, ids);
    select(&mut simulation, PLAYER, members.clone());
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 500.0,
                y: 200.0,
                queue: false,
            },
        )
        .unwrap();
    run(&mut simulation, 300);
    let end = centre_of(&simulation, ids, &members);
    assert!(
        (end.x - 500.0).abs() < 40.0 && (end.y - 200.0).abs() < 40.0,
        "{end:?}"
    );
}

#[test]
fn the_dummy_toggle_flips_dummies_both_ways_and_leaves_other_objects_alone() {
    let m = load();
    let mut simulation =
        Simulation::new(3, rules(&m, &[("Striker", 260.0, 200.0, PLAYER)])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    spawn(&mut simulation, &m, OTHER, SpawnRole::Dummy);
    let striker = experience_tests::placed(&simulation, ids, &m, "Striker")[0];
    let members = members_of(&simulation, ids);
    let kinds = |simulation: &Simulation<RotwkRules>| {
        let kind = |entity| {
            simulation
                .state()
                .get(ids.health, entity)
                .map(|health| health.0.kind())
        };
        (kind(members[0]), kind(striker))
    };
    let mut seen = Vec::new();
    for on in [false, true] {
        simulation
            .submit(
                PLAYER,
                RotwkCommand::Debug(DebugCommand::DummiesInvincible { on }),
            )
            .unwrap();
        run(&mut simulation, 1);
        seen.push(kinds(&simulation));
    }
    let striker_kind = kinds(&simulation).1;
    assert_ne!(striker_kind, Some(BodyKind::Immortal));
    assert_eq!(
        seen,
        vec![
            (Some(BodyKind::Active), striker_kind),
            (Some(BodyKind::Immortal), striker_kind)
        ]
    );
}

#[test]
fn a_dummy_starts_mortal_and_a_fighting_dummy_may_attack() {
    let m = load();
    for (role, attacks) in [(SpawnRole::Dummy, false), (SpawnRole::FightingDummy, true)] {
        let mut simulation = Simulation::new(3, rules(&m, &[])).unwrap();
        let ids = simulation.rules().ids().unwrap();
        spawn(&mut simulation, &m, OTHER, role);
        let member = members_of(&simulation, ids)[0];
        let state = simulation.state();
        assert_eq!(
            state.get(ids.health, member).map(|health| health.0.kind()),
            Some(BodyKind::Active)
        );
        assert!(state.contains(ids.dummy, member));
        assert_eq!(state.contains(ids.no_attack, member), !attacks, "{role:?}");
    }
}

#[test]
fn four_quarter_steps_from_the_floor_make_exactly_one_level() {
    let presses = |floor: u32, required: u32| {
        let mut points = floor;
        let mut seen = Vec::new();
        for _ in 0..4 {
            points += debug::quarter_gain(floor, required, points, 1);
            seen.push(points);
        }
        seen
    };
    assert_eq!(presses(1, 7), vec![3, 4, 6, 7]);
    assert_eq!(presses(0, 100), vec![25, 50, 75, 100]);
    assert_eq!(debug::quarter_gain(0, 100, 30, 1), 20);
    assert_eq!(debug::quarter_gain(0, 100, 30, 4), 70);
}

fn ordered_target(
    simulation: &Simulation<RotwkRules>,
    ids: Ids,
    striker: Entity,
) -> Option<Entity> {
    simulation
        .state()
        .get(ids.attack_order, striker)
        .map(|order| order.target)
}

#[test]
fn a_non_melee_order_on_a_horde_member_keeps_the_clicked_member() {
    let m = load();
    let (simulation, ids, striker, members) = attack_first_member(&m, (140.0, 200.0), 1);
    let at = |e: Entity| simulation.state().get(ids.body, e).unwrap().0.position;
    let from = at(striker);
    let nearest = members
        .iter()
        .copied()
        .min_by(|a, b| {
            at(*a)
                .minus(from)
                .length_squared()
                .total_cmp(&at(*b).minus(from).length_squared())
        })
        .unwrap();
    assert_ne!(
        nearest, members[0],
        "the clicked member must not be the nearest, or this proves nothing"
    );
    assert_eq!(ordered_target(&simulation, ids, striker), Some(members[0]));
}

#[test]
fn a_melee_order_on_a_horde_member_fights_the_nearest_member_instead() {
    let m = load();
    let (simulation, ids, smasher, members) =
        attack_first_member_as(&m, "Smasher", (140.0, 200.0), 1);
    let at = |e: Entity| simulation.state().get(ids.body, e).unwrap().0.position;
    let from = at(smasher);
    let nearest = members
        .iter()
        .copied()
        .min_by(|a, b| {
            at(*a)
                .minus(from)
                .length_squared()
                .total_cmp(&at(*b).minus(from).length_squared())
        })
        .unwrap();
    assert_ne!(
        nearest, members[0],
        "the clicked member must not be the nearest, or this proves nothing"
    );
    assert_eq!(ordered_target(&simulation, ids, smasher), Some(nearest));
}

#[test]
fn an_order_on_a_horde_picks_another_member_when_its_member_dies() {
    let m = load();
    let (mut simulation, ids, striker, members) = attack_first_member(&m, (260.0, 200.0), 1);
    let first = ordered_target(&simulation, ids, striker).unwrap();
    for _ in 0..900 {
        if !health_of(&simulation, ids, first).is_some_and(|health| health > 0.0) {
            break;
        }
        run(&mut simulation, 1);
    }
    run(&mut simulation, 1);
    let next = ordered_target(&simulation, ids, striker);
    assert!(
        next.is_some_and(|next| next != first && members.contains(&next)),
        "{next:?} after {first:?}"
    );
}

#[test]
fn a_ground_order_on_a_horde_fires_only_the_released_rank() {
    let m = load();
    let mut simulation = Simulation::new(3, rules(&m, &[])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    spawn_kind(
        &mut simulation,
        &m,
        "ArcherHorde",
        PLAYER,
        SpawnRole::Normal,
    );
    let horde = horde_entity(&simulation, ids).unwrap();
    let members = members_of(&simulation, ids);
    assert_eq!(members.len(), 2);
    let released_rank = 0;
    fn rank_of(simulation: &Simulation<RotwkRules>, ids: Ids, member: Entity) -> u32 {
        let entry = simulation.state().get(ids.member, member).unwrap();
        let horde = simulation.state().get(ids.horde, entry.horde).unwrap();
        horde.slots[usize::from(entry.slot.unwrap())].rank
    }
    let first_rank = rank_of(&simulation, ids, members[0]);
    let at = simulation.state().get(ids.body, horde).unwrap().0.position;
    select(&mut simulation, PLAYER, members.clone());
    simulation
        .submit(
            PLAYER,
            RotwkCommand::AttackGround {
                slot: 2,
                x: at.x,
                y: at.y,
            },
        )
        .unwrap();
    run(&mut simulation, 60);
    let fired = |member: Entity| {
        simulation
            .state()
            .get(ids.armed, member)
            .is_some_and(|armed| armed.last_shot_at.is_some())
    };
    let (released, held) = if first_rank == released_rank {
        (members[0], members[1])
    } else {
        (members[1], members[0])
    };
    assert!(fired(released), "the released rank never fired");
    assert!(!fired(held), "an unreleased rank fired the horde's order");
}

#[test]
fn a_melee_repick_with_no_member_in_reach_takes_the_nearest_adjacent_hostile() {
    let m = load();
    let mut simulation = Simulation::new(
        3,
        rules(
            &m,
            &[
                ("Smasher", 215.0, 190.0, PLAYER),
                ("Target", 225.0, 195.0, OTHER),
            ],
        ),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    spawn(&mut simulation, &m, OTHER, SpawnRole::Dummy);
    let smasher = experience_tests::placed(&simulation, ids, &m, "Smasher")[0];
    let target = experience_tests::placed(&simulation, ids, &m, "Target")[0];
    let members = members_of(&simulation, ids);

    select(&mut simulation, PLAYER, vec![smasher]);
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: members[0] })
        .unwrap();
    run(&mut simulation, 1);
    let first = ordered_target(&simulation, ids, smasher).unwrap();
    assert!(
        members.contains(&first),
        "the first pick should be a horde member, got {first:?}"
    );

    for _ in 0..600 {
        if !health_of(&simulation, ids, first).is_some_and(|health| health > 0.0) {
            break;
        }
        run(&mut simulation, 1);
    }
    assert!(
        !health_of(&simulation, ids, first).is_some_and(|health| health > 0.0),
        "the engaged member never died"
    );
    run(&mut simulation, 1);

    let next = ordered_target(&simulation, ids, smasher);
    assert_eq!(
        next,
        Some(target),
        "{next:?} - not the nearest adjacent hostile"
    );
}
