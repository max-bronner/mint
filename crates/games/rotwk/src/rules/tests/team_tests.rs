use super::*;
use crate::handicap::Handicap;

fn duel(teams: Option<Vec<Option<u8>>>, handicaps: Vec<Handicap>) -> (Simulation<RotwkRules>, Ids) {
    let (combat, bow, armour) = combat_data();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let spawns = vec![
        armed_spawn(200.0, 200.0, PLAYER, Some(bow), Some(armour), 100.0),
        armed_spawn(300.0, 200.0, OTHER, Some(bow), Some(armour), 100.0),
    ];
    let mut rules = RotwkRules::new(passability, spawns)
        .with_combat(combat)
        .with_handicaps(handicaps);
    if let Some(teams) = teams {
        rules = rules.with_teams(teams);
    }
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    (simulation, ids)
}

fn bodies(simulation: &Simulation<RotwkRules>, ids: Ids) -> Vec<Entity> {
    simulation
        .state()
        .store(ids.body)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default()
}

fn run(simulation: &mut Simulation<RotwkRules>, ticks: u32) {
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
}

#[test]
fn allies_never_acquire_each_other() {
    let (mut enemies, ids) = duel(None, Vec::new());
    run(&mut enemies, 200);
    let units = bodies(&enemies, ids);
    assert!(
        units
            .iter()
            .any(|u| health_of(&enemies, ids, *u).is_some_and(|h| h < 100.0)),
        "the fixture's own premise: enemies fight"
    );
    let (mut allies, ids) = duel(Some(vec![Some(3), Some(3)]), Vec::new());
    run(&mut allies, 200);
    for unit in bodies(&allies, ids) {
        assert_eq!(health_of(&allies, ids, unit), Some(100.0));
    }
}

#[test]
fn an_attack_order_on_an_ally_is_refused() {
    let (mut allies, ids) = duel(Some(vec![Some(0), Some(0)]), Vec::new());
    let units = bodies(&allies, ids);
    let (shooter, ally) = (units[0], units[1]);
    allies
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![shooter],
            },
        )
        .unwrap();
    allies
        .submit(PLAYER, RotwkCommand::AttackObject { target: ally })
        .unwrap();
    run(&mut allies, 200);
    assert_eq!(health_of(&allies, ids, ally), Some(100.0));
    assert!(!allies.is_poisoned());
}

fn other_lost(handicaps: Vec<Handicap>, ticks: u32) -> f32 {
    let (mut simulation, ids) = duel(None, handicaps);
    run(&mut simulation, ticks);
    let units = bodies(&simulation, ids);
    100.0 - health_of(&simulation, ids, units[1]).unwrap_or(0.0)
}

#[test]
fn a_handicap_scales_damage_dealt_and_health() {
    let plain = other_lost(Vec::new(), 130);
    assert!(plain > 0.0, "the fixture's own premise: arrows land");
    let weak_shots = Handicap {
        damage: 0.5,
        ..Handicap::default()
    };
    let dealt = other_lost(vec![weak_shots, Handicap::default()], 130);
    assert!(
        (dealt * 2.0 - plain).abs() < 1e-3,
        "{dealt} against {plain}"
    );
    let frail = Handicap {
        health: 0.5,
        ..Handicap::default()
    };
    let taken = other_lost(vec![Handicap::default(), frail], 130);
    assert!(
        (taken - plain * 2.0).abs() < 1e-3,
        "{taken} against {plain}"
    );
}

#[test]
fn allies_left_alone_win_together() {
    let run_match = |teams: Vec<Option<u8>>| {
        let (data, kinds) = construction_kinds();
        let shed = kinds.kind("Shed").unwrap();
        let attacker = kinds.kind("Attacker").unwrap();
        let at = |x: f32| Vec2::new(x, 100.0);
        let spawns = vec![
            kinds
                .spawn(&data.combat, shed, at(100.0), Facing::EAST, PLAYER)
                .unwrap(),
            kinds
                .spawn(&data.combat, shed, at(700.0), Facing::EAST, OTHER)
                .unwrap(),
            kinds
                .spawn(
                    &data.combat,
                    attacker,
                    at(1300.0),
                    Facing::EAST,
                    PlayerId::new(2),
                )
                .unwrap(),
        ];
        let map = flat_map(160, 60);
        let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
        let filter =
            crate::defeat::ObjectFilter::parse(&["NONE".to_owned(), "+STRUCTURE".to_owned()]);
        let mut simulation = Simulation::new(
            3,
            RotwkRules::new(passability, spawns)
                .with_combat(data.combat.clone())
                .with_victory(Arc::clone(&kinds), filter)
                .with_players(3)
                .with_teams(teams),
        )
        .unwrap();
        run(&mut simulation, 3);
        let ids = simulation.rules().ids().unwrap();
        let clock = crate::clock_entity(simulation.state(), ids).unwrap();
        simulation.state().get(ids.outcome, clock).copied()
    };
    let decided = run_match(vec![Some(1), Some(1), None]).expect("allies alone win");
    assert_eq!(decided.winner, PLAYER);
    assert!(
        run_match(vec![None, None, None]).is_none(),
        "two enemies left"
    );
}

#[test]
fn a_ruleset_carries_the_players_it_is_given() {
    let map = flat_map(32, 32);
    let rules = |n| {
        RotwkRules::new(Passability::build(&map, 5.0 / 128.0).unwrap(), Vec::new())
            .with_players(n)
            .player_count()
    };
    assert_eq!(rules(3), 3);
    assert_eq!(rules(0), 1);
    assert_eq!(rules(12), 8);
    assert_eq!(
        RotwkRules::new(Passability::build(&map, 5.0 / 128.0).unwrap(), Vec::new()).player_count(),
        4
    );
    let mut simulation = Simulation::new(
        1,
        RotwkRules::new(Passability::build(&map, 5.0 / 128.0).unwrap(), Vec::new()).with_players(3),
    )
    .unwrap();
    simulation.tick().unwrap();
    let ids = simulation.rules().ids().unwrap();
    let clock = crate::clock_entity(simulation.state(), ids).unwrap();
    let slots = simulation
        .state()
        .get(ids.shroud, clock)
        .and_then(|s| s.0.as_ref())
        .map(|state| state.grid.players());
    assert_eq!(slots, Some(3));
}

fn horde_with(
    weapon: &str,
    extra: Vec<Spawn>,
    teams: Vec<Option<u8>>,
) -> (Simulation<RotwkRules>, Ids) {
    let (combat, _bow, armour) = combat_data();
    let weapon = combat.weapons.id(weapon).unwrap();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let formation = square_formation();
    let count = u32::try_from(formation.len()).unwrap();
    let spawn = HordeSpawn {
        formation: 0,
        origin: Vec2::new(200.0, 200.0),
        facing: Facing::EAST,
        member: Spawn {
            health: Some(200.0),
            weapon: Some(weapon),
            armour: Some(armour),
            ..spawn_at(0.0, 0.0, KindOf::SELECTABLE, PLAYER)
        },
        count,
        vision: crate::shroud::HORDE_SHROUD_RANGE,
    };
    let rules = RotwkRules::new(passability, extra)
        .with_combat(combat)
        .with_hordes(vec![formation], vec![spawn])
        .with_players(3)
        .with_teams(teams);
    let simulation = Simulation::new(9, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    (simulation, ids)
}

fn first_of(simulation: &Simulation<RotwkRules>, ids: Ids, player: PlayerId) -> Entity {
    simulation
        .state()
        .store(ids.owner)
        .and_then(|store| store.iter().find(|(_, o)| o.0 == player).map(|(e, _)| e))
        .unwrap()
}

#[test]
fn an_idle_horde_never_acquires_an_ally() {
    let (_, bow, armour) = combat_data();
    let neighbour = || {
        vec![armed_spawn(
            300.0,
            200.0,
            PlayerId::new(2),
            Some(bow),
            Some(armour),
            5000.0,
        )]
    };
    let (mut enemies, ids) = horde_with("TestBow", neighbour(), vec![None, None, None]);
    run(&mut enemies, 150);
    let target = first_of(&enemies, ids, PlayerId::new(2));
    assert!(
        health_of(&enemies, ids, target).is_some_and(|h| h < 5000.0),
        "the fixture's own premise: an enemy in range is shot"
    );
    let (mut allies, ids) = horde_with("TestBow", neighbour(), vec![Some(1), None, Some(1)]);
    run(&mut allies, 150);
    let ally = first_of(&allies, ids, PlayerId::new(2));
    assert_eq!(health_of(&allies, ids, ally), Some(5000.0));
    assert_eq!(engaged(&allies, ids, &members_of(&allies, ids)), 0);
}

#[test]
fn a_command_point_factor_scales_every_limit() {
    let limit = |factor: Option<u32>, player: PlayerId| {
        let map = flat_map(32, 32);
        let mut rules = RotwkRules::new(Passability::build(&map, 5.0 / 128.0).unwrap(), Vec::new());
        if let Some(factor) = factor {
            rules = rules.with_command_point_factor(factor);
        }
        let mut simulation = Simulation::new(1, rules).unwrap();
        simulation.tick().unwrap();
        let ids = simulation.rules().ids().unwrap();
        crate::economy::points_of(simulation.state(), RotwkRules::economy_ids(ids), player).limit()
    };
    let (good, evil) = (limit(None, PLAYER), limit(None, OTHER));
    assert!(good > 0 && evil > 0);
    assert_eq!(limit(Some(100), PLAYER), good);
    assert_eq!(limit(Some(200), PLAYER), good * 2);
    assert_eq!(limit(Some(10_000), OTHER), evil * 100);
    assert_eq!(limit(Some(50), OTHER), evil / 2);
}
