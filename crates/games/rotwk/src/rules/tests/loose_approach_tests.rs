use super::*;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Locomotor Trot\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon Bow\n  AttackRange = 100\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 1\n    DamageType = PIERCE\n  End\nEnd\n\
Weapon Mortar\n  AttackRange = 100\n  MinimumAttackRange = 50\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 1\n    DamageType = PIERCE\n  End\nEnd\n\
Weapon Dart\n  AttackRange = 1\n  DelayBetweenShots = 500\n  NoVictimNeeded = Yes\n  \
  DamageNugget\n    Damage = 1\n    Radius = 5\n    DamageType = PIERCE\n  End\nEnd\n\
Object Hunter\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 200\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Bow\n    Weapon = TERTIARY Dart\n    \
    AutoChooseSources = TERTIARY FROM_PLAYER\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 30\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 2\n  GeometryHeight = 16\nEnd\n\
Object Runner\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 10\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
  LocomotorSet\n    Locomotor = Trot\n    Condition = SET_NORMAL\n    Speed = 37\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ChildObject Jogger Runner\n  \
  LocomotorSet\n    Locomotor = Trot\n    Condition = SET_NORMAL\n    Speed = 25\n  End\nEnd\n\
ChildObject Mortarman Hunter\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Mortar\n  End\nEnd\n\
ChildObject Bowman Hunter\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Bow\n  End\nEnd\n\
ChildObject Gunner Hunter\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n    \
    Turret\n      TurretTurnRate = 180\n      ControlledWeaponSlots = PRIMARY\n    End\n  End\nEnd\n\
ChildObject EagerGunner Hunter\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n    \
    Turret\n      TurretTurnRate = 180\n      ControlledWeaponSlots = PRIMARY\n      \
      FiresWhileTurning = Yes\n    End\n  End\nEnd\n";

fn simulation(runner: Vec2) -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    simulation_of("Hunter", "Runner", runner)
}

fn simulation_of(
    hunter: &str,
    quarry: &str,
    runner: Vec2,
) -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = [
        "Hunter",
        "Runner",
        "Jogger",
        "Gunner",
        "EagerGunner",
        "Mortarman",
        "Bowman",
    ]
    .map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawn = |name: &str, at: Vec2, owner: PlayerId| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                at,
                Facing::EAST,
                owner,
            )
            .unwrap()
    };
    let spawns = vec![
        spawn(hunter, Vec2::new(100.0, 300.0), PLAYER),
        spawn(quarry, runner, OTHER),
    ];
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_teams(vec![None, None]);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    (simulation, ids, entities[0], entities[1])
}

fn order(
    simulation: &mut Simulation<RotwkRules>,
    player: PlayerId,
    unit: Entity,
    command: RotwkCommand,
) {
    simulation
        .submit(
            player,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![unit],
            },
        )
        .unwrap();
    simulation.submit(player, command).unwrap();
}

#[test]
fn a_ranged_approach_replans_on_the_second_not_when_the_victim_has_moved() {
    let (mut simulation, ids, hunter, runner) = simulation(Vec2::new(700.0, 300.0));
    order(
        &mut simulation,
        OTHER,
        runner,
        RotwkCommand::MoveTo {
            x: 700.0,
            y: 900.0,
            queue: false,
        },
    );
    order(
        &mut simulation,
        PLAYER,
        hunter,
        RotwkCommand::AttackObject { target: runner },
    );
    let goal = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(ids.path, hunter)
            .filter(|path| path.kind == PathKind::Closing)
            .map(|path| path.goal)
    };
    let mut plans: Vec<u64> = Vec::new();
    let mut last = None;
    for _ in 0..200 {
        simulation.tick().unwrap();
        let now = goal(&simulation);
        if now.is_some() && now != last {
            plans.push(simulation.frame().get());
        }
        last = now;
        if plans.len() >= 2 {
            break;
        }
    }
    assert!(plans.len() >= 2, "plans {plans:?}");
    let gap = plans[1] - plans[0];
    assert!(
        (58..=62).contains(&gap),
        "re-planned {gap} frames after the first plan"
    );
}

#[test]
fn a_ground_attack_ending_close_to_its_route_end_is_placed_on_it() {
    let point = Vec2::new(205.0, 305.0);
    let (mut simulation, ids, hunter, _) = simulation(Vec2::new(700.0, 700.0));
    order(
        &mut simulation,
        PLAYER,
        hunter,
        RotwkCommand::AttackGround {
            slot: 2,
            x: point.x,
            y: point.y,
        },
    );
    let mut route_end = None;
    let mut fired = false;
    for _ in 0..300 {
        simulation.tick().unwrap();
        if let Some(end) = simulation
            .state()
            .get(ids.path, hunter)
            .and_then(|path| path.route.points.last().copied())
        {
            route_end = Some(end);
        }
        fired = simulation
            .state()
            .get(ids.armed, hunter)
            .is_some_and(|armed| armed.state.last_shot().is_some());
        if fired {
            break;
        }
    }
    assert!(
        fired,
        "the hunter never fired at its point: {:?} {:?} {:?}",
        simulation.state().get(ids.body, hunter).unwrap().0.position,
        simulation.state().get(ids.armed, hunter),
        simulation
            .state()
            .get(ids.path, hunter)
            .map(|p| p.route.points.clone())
    );
    let end = route_end.expect("the hunter never walked");
    let at = simulation.state().get(ids.body, hunter).unwrap().0.position;
    assert!(
        at.minus(end).length() < 1e-3,
        "stands at {at:?}, the route ended at {end:?}"
    );
}

#[test]
fn a_turreted_attacker_follows_a_fleeing_victim_in_range() {
    let pursued = |hunter: &str| {
        let (mut simulation, ids, attacker, runner) =
            simulation_of(hunter, "Jogger", Vec2::new(220.0, 300.0));
        order(
            &mut simulation,
            OTHER,
            runner,
            RotwkCommand::MoveTo {
                x: 900.0,
                y: 300.0,
                queue: false,
            },
        );
        order(
            &mut simulation,
            PLAYER,
            attacker,
            RotwkCommand::AttackObject { target: runner },
        );
        let mut slowest = f32::MAX;
        for tick in 0..360 {
            simulation.tick().unwrap();
            if tick >= 240 {
                let body = simulation.state().get(ids.body, attacker).unwrap().0;
                slowest = slowest.min(body.speed);
            }
        }
        (simulation.state().contains(ids.path, attacker), slowest)
    };
    let (moving, speed) = pursued("Gunner");
    assert!(
        moving && speed > 1.0 && speed <= 25.0 * 0.95 + 0.5,
        "gunner {moving} at {speed}"
    );
}

fn damage(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    100_000.0
        - simulation
            .state()
            .get(ids.health, entity)
            .unwrap()
            .0
            .current()
}

#[test]
fn a_turreted_attacker_waits_for_its_turret_to_aim_before_firing() {
    let (mut simulation, ids, gunner, runner) =
        simulation_of("Gunner", "Runner", Vec2::new(100.0, 350.0));
    order(
        &mut simulation,
        PLAYER,
        gunner,
        RotwkCommand::AttackObject { target: runner },
    );
    simulation.tick().unwrap();
    assert_eq!(
        damage(&simulation, ids, runner),
        0.0,
        "the gunner fired before its turret had turned at all"
    );
    let state = simulation.rules().modules.as_ref().unwrap().state;
    let rotating = |simulation: &Simulation<RotwkRules>| {
        simulation.state().get(state, gunner).is_some_and(|s| {
            s.model_conditions
                .iter()
                .any(|name| name.eq_ignore_ascii_case("TURRET_ROTATE"))
        })
    };
    assert!(rotating(&simulation), "the turret was not reported turning");
    for _ in 0..90 {
        simulation.tick().unwrap();
    }
    assert!(
        damage(&simulation, ids, runner) > 0.0,
        "the gunner never fired once its turret had time to aim"
    );
    assert!(
        !rotating(&simulation),
        "the turret still reports turning once aimed and holding on a stationary victim"
    );
}

#[test]
fn fires_while_turning_does_not_let_a_turret_fire_before_it_has_aimed() {
    let (mut simulation, ids, gunner, runner) =
        simulation_of("EagerGunner", "Runner", Vec2::new(100.0, 350.0));
    order(
        &mut simulation,
        PLAYER,
        gunner,
        RotwkCommand::AttackObject { target: runner },
    );
    simulation.tick().unwrap();
    assert_eq!(
        damage(&simulation, ids, runner),
        0.0,
        "FiresWhileTurning let the gunner fire before its turret had turned at all"
    );
}

fn walked_before_striking(hunter: &str, gap: f32) -> Option<f32> {
    let (mut simulation, ids, attacker, runner) =
        simulation_of(hunter, "Runner", Vec2::new(100.0 + gap + 7.0, 300.0));
    order(
        &mut simulation,
        PLAYER,
        attacker,
        RotwkCommand::AttackObject { target: runner },
    );
    let start = simulation
        .state()
        .get(ids.body, attacker)
        .unwrap()
        .0
        .position;
    for _ in 0..90 {
        simulation.tick().unwrap();
        if damage(&simulation, ids, runner) > 0.0 {
            let at = simulation
                .state()
                .get(ids.body, attacker)
                .unwrap()
                .0
                .position;
            return Some(at.minus(start).length());
        }
    }
    None
}

#[test]
fn a_ranged_weapon_reaches_two_and_a_half_short_of_its_range() {
    let rows = [
        ("Hunter", 97.0, true),
        ("Hunter", 98.0, false),
        ("Mortarman", 48.0, true),
        ("Mortarman", 47.0, false),
    ];
    for (hunter, gap, fires_where_it_stands) in rows {
        let walked = walked_before_striking(hunter, gap).unwrap();
        assert_eq!(
            walked < 0.1,
            fires_where_it_stands,
            "{hunter} at an edge gap of {gap} walked {walked} before striking"
        );
    }
}

fn first_goal(head_start: u32) -> Vec2 {
    let (mut simulation, ids, attacker, runner) =
        simulation_of("Hunter", "Runner", Vec2::new(247.0, 300.0));
    if head_start > 0 {
        order(
            &mut simulation,
            OTHER,
            runner,
            RotwkCommand::MoveTo {
                x: 900.0,
                y: 300.0,
                queue: false,
            },
        );
        for _ in 0..head_start {
            simulation.tick().unwrap();
        }
    }
    order(
        &mut simulation,
        PLAYER,
        attacker,
        RotwkCommand::AttackObject { target: runner },
    );
    for _ in 0..30 {
        simulation.tick().unwrap();
        if let Some(path) = simulation.state().get(ids.path, attacker) {
            return path.goal;
        }
    }
    panic!("the hunter never planned a route");
}

#[test]
fn a_ranged_attacker_hops_to_an_attack_spot_unless_the_victim_is_fleeing() {
    let hop = first_goal(0);
    assert!(
        (hop.x - 145.0).abs() < 1.0 && (hop.y - 300.0).abs() < 1.0,
        "a standing victim: the route heads for {hop:?}, not the shortcut's spot (145, 300)"
    );
    let chase = first_goal(30);
    assert!(
        chase.x > 250.0,
        "a fleeing victim: the route heads for {chase:?}, not at the runner"
    );
}

#[test]
fn a_ranged_attackers_route_ends_on_its_attack_spot() {
    let (mut simulation, ids, hunter, runner) = simulation(Vec2::new(241.0, 300.0));
    order(
        &mut simulation,
        PLAYER,
        hunter,
        RotwkCommand::AttackObject { target: runner },
    );
    for _ in 0..30 {
        simulation.tick().unwrap();
        if let Some(path) = simulation.state().get(ids.path, hunter) {
            let end = path.route.points.last().copied().unwrap();
            assert!(
                path.goal.minus(Vec2::new(140.0, 300.0)).length() < 1e-3
                    && end.minus(path.goal).length() < 1e-3,
                "the route to {:?} ends at {end:?}",
                path.goal
            );
            return;
        }
    }
    panic!("the hunter never planned a route");
}

#[test]
fn a_ranged_attacker_walks_its_attack_path_to_the_end() {
    let (mut simulation, ids, hunter, runner) = simulation(Vec2::new(241.0, 300.0));
    order(
        &mut simulation,
        PLAYER,
        hunter,
        RotwkCommand::AttackObject { target: runner },
    );
    for _ in 0..120 {
        simulation.tick().unwrap();
        if damage(&simulation, ids, runner) > 0.0 {
            let at = simulation.state().get(ids.body, hunter).unwrap().0.position;
            assert!(at.x >= 139.0, "fired from {at:?}, short of its spot at 140");
            return;
        }
    }
    panic!("the hunter never fired");
}

#[test]
fn a_ranged_attacker_in_reach_at_a_replan_stops_instead_of_planning() {
    let (mut simulation, ids, hunter, runner) =
        simulation_of("Bowman", "Runner", Vec2::new(600.0, 300.0));
    order(
        &mut simulation,
        OTHER,
        runner,
        RotwkCommand::MoveTo {
            x: 300.0,
            y: 300.0,
            queue: false,
        },
    );
    order(
        &mut simulation,
        PLAYER,
        hunter,
        RotwkCommand::AttackObject { target: runner },
    );
    let mut last_goal = None;
    let mut in_reach_since = None;
    for tick in 0..600 {
        let me = simulation.state().get(ids.body, hunter).unwrap().0.position;
        let them = simulation.state().get(ids.body, runner).unwrap().0.position;
        let in_reach = them.minus(me).length() - 7.0 <= 97.5;
        if in_reach && in_reach_since.is_none() {
            in_reach_since = Some(tick);
        }
        simulation.tick().unwrap();
        let goal = simulation
            .state()
            .get(ids.path, hunter)
            .filter(|path| path.kind == PathKind::Closing)
            .map(|path| path.goal);
        if goal.is_some() && goal != last_goal {
            assert!(
                !in_reach,
                "planned a route to {goal:?} at tick {tick} while in reach (since {in_reach_since:?})"
            );
        }
        last_goal = goal;
    }
    assert!(in_reach_since.is_some(), "the runner never came into reach");
    assert!(
        damage(&simulation, ids, runner) > 0.0,
        "the hunter never fired"
    );
}
