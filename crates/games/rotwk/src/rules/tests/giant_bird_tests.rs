use super::*;

const DATA: &str = "\
Locomotor BirdLocomotor\n  Surfaces = AIR\n  Appearance = GIANT_BIRD\n  Acceleration = 400\n  \
  Braking = 1000\n  TurnTime = 2000\n  PreferredHeight = 60\n  PreferredAttackHeight = 5\n  \
  SwoopStandoffRadius = 310\n  SwoopStandoffHeight = 119\n  AirborneTargetingHeight = 25\n  \
  LookAheadMult = 2.0\n  Lift = 100%\n  ZAxisBehavior = SURFACE_RELATIVE_HEIGHT\nEnd\n\
Weapon Talons\n  AttackRange = 20\n  DelayBetweenShots = 500\n  CanSwoop = Yes\n  \
  AntiGround = Yes\n  AntiAirborneMonster = Yes\n  \
  DamageNugget\n    Damage = 1\n    DamageType = PIERCE\n  End\nEnd\n\
Weapon GrabTalons\n  AttackRange = 20\n  DelayBetweenShots = 30\n  CanSwoop = Yes\n  \
  GrabNugget\n    ContainTargetOnEffect = Yes\n  End\nEnd\n\
Object Bird\n  Side = Men\n  KindOf = MONSTER SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Talons\n  End\n  \
  LocomotorSet\n    Locomotor = BirdLocomotor\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  LocomotorSet\n    Locomotor = BirdLocomotor\n    Condition = SET_SUPERSONIC\n    \
    Speed = 90\n  End\n  \
  Behavior = GiantBirdAIUpdate Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 100\n    FollowThroughDistance = 120\n    \
    FollowThroughCheckStep = 20\n    FollowThroughGradient = 1.0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 16\nEnd\n\
ChildObject Grabber Bird\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY GrabTalons\n  End\n  \
  Behavior = GiantBirdAIUpdate Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 100\n    FollowThroughDistance = 120\n    \
    FollowThroughCheckStep = 20\n    FollowThroughGradient = 1.0\n    \
    GrabTossTimeTrigger = 3.0\n    GrabTossHeightTrigger = 40.0\n  End\n  \
  Behavior = TransportContain Tag_Claws\n    \
    ObjectStatusOfContained = UNSELECTABLE UNATTACKABLE\n    \
    PassengerFilter = ANY +INFANTRY\n    Slots = 1\n    AllowEnemiesInside = Yes\n  End\nEnd\n\
Locomotor DiveLocomotor\n  Surfaces = AIR\n  Appearance = GIANT_BIRD\n  Acceleration = 400\n  \
  Braking = 1000\n  TurnTime = 2000\n  PreferredHeight = 60\n  AirborneTargetingHeight = 25\n  \
  LookAheadMult = 3.0\n  ZAxisBehavior = SURFACE_RELATIVE_HEIGHT\nEnd\n\
SpecialPower ZSwoop\n  ReloadTime = 30000\nEnd\n\
ChildObject PowerBird Bird\n  CommandSet = PowerBirdSet\n  \
  LocomotorSet\n    Locomotor = DiveLocomotor\n    Condition = SET_PANIC\n    Speed = 120\n  End\n  \
  Behavior = FellBeastSwoopPower Tag_Swoop\n    SpecialPowerTemplate = ZSwoop\n    \
    UnpackTime = 1\n    StartAbilityRange = 1000\n  End\nEnd\n\
CommandSet PowerBirdSet\n  1 = Fire_Swoop\nEnd\n\
CommandButton Fire_Swoop\n  Command = SPECIAL_POWER\n  SpecialPower = ZSwoop\n  \
  Options = NEED_TARGET_ENEMY_OBJECT\nEnd\n\
Object Tower\n  Side = Men\n  KindOf = STRUCTURE IMMOBILE SELECTABLE\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 40\n  GeometryHeight = 50\nEnd\n\
Object Prey\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ChildObject Bait Prey\nEnd\n\
ChildObject QuietBird Bird\n  \
  Behavior = GiantBirdAIUpdate Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n    \
    MoodAttackCheckRate = 100\n    FollowThroughDistance = 120\n    \
    FollowThroughCheckStep = 20\n    FollowThroughGradient = 1.0\n  End\nEnd\n\
ChildObject GuardBird Bird\n  \
  Behavior = GiantBirdAIUpdate Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n    \
    AttackPriority = BirdSet\n    MoodAttackCheckRate = 100\n    FollowThroughDistance = 120\n    \
    FollowThroughCheckStep = 20\n    FollowThroughGradient = 1.0\n  End\n  \
  Behavior = StancesBehavior Tag_S\n    StanceTemplate = BirdStances\n  End\nEnd\n\
StanceTemplate BirdStances\n  Stance HoldGround\n  End\nEnd\n\
AIData\n  AttackPriority BirdSet\n    Default = 1\n    Target = Bait 1000\n  End\nEnd\n";

fn ridge(width: u32, height: u32, high: u16) -> HeightMapData {
    HeightMapData {
        width,
        height,
        border_size: 0,
        boundaries: Vec::new(),
        samples: (0..width * height)
            .map(|i| {
                if (40..50).contains(&(i % width)) {
                    high
                } else {
                    0
                }
            })
            .collect(),
    }
}

fn scene(
    units: &[(&str, Vec2, PlayerId)],
    map: &HeightMapData,
) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    scene_with(units, map, |rules| rules)
}

fn scene_with(
    units: &[(&str, Vec2, PlayerId)],
    map: &HeightMapData,
    tune: impl FnOnce(RotwkRules) -> RotwkRules,
) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names: Vec<String> = units
        .iter()
        .map(|(name, _, _)| (*name).to_owned())
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let cast_rules = power_names.cast_rules(&data.special_powers);
    let spawns = units
        .iter()
        .map(|(name, at, owner)| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    *at,
                    Facing::EAST,
                    *owner,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_gravity(data.gravity)
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_power_cast_rules(cast_rules)
        .with_attack_priorities(Arc::clone(&data.attack_priorities))
        .with_stances(Arc::clone(&data.stances))
        .with_players(2);
    let simulation = Simulation::new(5, tune(rules)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    (simulation, ids, entities)
}

fn select_and(simulation: &mut Simulation<RotwkRules>, unit: Entity, command: RotwkCommand) {
    order(simulation, PLAYER, unit, command);
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

fn swoop(
    simulation: &Simulation<RotwkRules>,
    ids: Ids,
    bird: Entity,
) -> (&'static str, &'static str) {
    simulation.state().get(ids.giant_bird, bird).map_or(
        ("none", "none"),
        crate::rules::giant_bird::GiantBird::swoop_name,
    )
}

fn phase(simulation: &Simulation<RotwkRules>, ids: Ids, bird: Entity) -> &'static str {
    simulation
        .state()
        .get(ids.giant_bird, bird)
        .map_or("none", crate::rules::giant_bird::GiantBird::phase_name)
}

fn above(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    let at = position(simulation, ids, entity);
    simulation.state().get(ids.elevation, entity).unwrap().0
        - simulation.rules().ground_height(at.x, at.y)
}

fn wears(simulation: &Simulation<RotwkRules>, entity: Entity, condition: &str) -> bool {
    let state = simulation.rules().upgrade_state_id().unwrap();
    simulation
        .state()
        .get(state, entity)
        .is_some_and(|s| s.model_conditions.iter().any(|n| n.as_ref() == condition))
}

#[test]
fn an_idle_giant_bird_hovers_at_its_preferred_height() {
    let (mut simulation, ids, units) = scene(
        &[("Bird", Vec2::new(300.0, 480.0), PLAYER)],
        &flat_map(96, 96),
    );
    let bird = units[0];
    for _ in 0..(5 * SIMULATION_HZ) {
        simulation.tick().unwrap();
    }
    assert_eq!(phase(&simulation, ids, bird), "hover");
    let height = above(&simulation, ids, bird);
    assert!((height - 60.0).abs() <= 2.5, "hovering {height} up");
    let drifted = position(&simulation, ids, bird)
        .minus(Vec2::new(300.0, 480.0))
        .length();
    assert!(drifted < 1.0, "drifted {drifted} while idle");
}

#[test]
fn a_giant_bird_loops_round_to_a_goal_behind_it_and_hovers_there() {
    let (mut simulation, ids, units) = scene(
        &[("Bird", Vec2::new(500.0, 480.0), PLAYER)],
        &flat_map(96, 96),
    );
    let bird = units[0];
    simulation.tick().unwrap();
    select_and(
        &mut simulation,
        bird,
        RotwkCommand::MoveTo {
            x: 300.0,
            y: 480.0,
            queue: false,
        },
    );
    let per_tick = core::f32::consts::TAU / 2.0 / 30.0 + 1e-4;
    let facing = |simulation: &Simulation<RotwkRules>| {
        simulation.state().get(ids.body, bird).unwrap().0.facing
    };
    let mut last = facing(&simulation);
    let mut cruised = false;
    let mut arrived_at = None;
    for tick in 0..(20 * SIMULATION_HZ) {
        simulation.tick().unwrap();
        let now = facing(&simulation);
        assert!(
            last.delta_to(now).abs() <= per_tick,
            "turned {} in a tick",
            last.delta_to(now)
        );
        last = now;
        cruised |= phase(&simulation, ids, bird) == "cruise";
        if cruised && phase(&simulation, ids, bird) == "hover" {
            arrived_at = Some(tick);
            break;
        }
    }
    assert!(cruised, "never took the order");
    assert!(arrived_at.is_some(), "never arrived");
    assert!(
        !simulation.state().contains(ids.path, bird),
        "the order's path was kept"
    );
    let off = position(&simulation, ids, bird)
        .minus(Vec2::new(300.0, 480.0))
        .length();
    assert!(off < 30.0, "arrived {off} from its goal");
    let speed = simulation.state().get(ids.body, bird).unwrap().0.speed;
    assert!(speed > 0.0, "stopped dead on arrival");
}

#[test]
fn a_giant_bird_swoops_strikes_follows_through_and_comes_round_again() {
    let (mut simulation, ids, units) = scene(
        &[
            ("Bird", Vec2::new(300.0, 480.0), PLAYER),
            ("Prey", Vec2::new(550.0, 480.0), OTHER),
        ],
        &flat_map(96, 96),
    );
    let (bird, prey) = (units[0], units[1]);
    simulation.tick().unwrap();
    select_and(
        &mut simulation,
        bird,
        RotwkCommand::AttackObject { target: prey },
    );
    let mut strikes = 0;
    let mut clawed_before_strike = false;
    let mut fastest_swoop = 0.0f32;
    let mut lowest_strike = f32::INFINITY;
    let mut climbed_to = 0.0f32;
    let mut previous = phase(&simulation, ids, bird);
    for _ in 0..(25 * SIMULATION_HZ) {
        simulation.tick().unwrap();
        let now = phase(&simulation, ids, bird);
        let height = above(&simulation, ids, bird);
        match now {
            "swoop" => {
                fastest_swoop =
                    fastest_swoop.max(simulation.state().get(ids.body, bird).unwrap().0.speed);
                clawed_before_strike |= wears(&simulation, bird, "ABOUT_TO_HIT");
            }
            "attack" if previous != "attack" => {
                strikes += 1;
                lowest_strike = lowest_strike.min(height);
            }
            "follow-through" => climbed_to = climbed_to.max(height),
            _ => {}
        }
        previous = now;
    }
    let health = simulation
        .state()
        .get(ids.health, prey)
        .unwrap()
        .0
        .fraction();
    assert!(strikes >= 2, "{strikes} strike(s) in 25 s");
    assert!(health < 1.0, "the prey was never hurt");
    assert!(clawed_before_strike, "never took its clawing pose");
    assert!(
        fastest_swoop > 60.5,
        "swooped at {fastest_swoop}, no faster than its normal set"
    );
    assert!(lowest_strike < 20.0, "struck from {lowest_strike} up");
    assert!(
        climbed_to > 40.0,
        "climbed only {climbed_to} up in its follow-through"
    );
}

#[test]
fn a_giant_bird_stopped_during_its_follow_through_hovers_afterwards() {
    let (mut simulation, ids, units) = scene(
        &[
            ("QuietBird", Vec2::new(300.0, 480.0), PLAYER),
            ("Prey", Vec2::new(550.0, 480.0), OTHER),
        ],
        &flat_map(96, 96),
    );
    let (bird, prey) = (units[0], units[1]);
    simulation.tick().unwrap();
    select_and(
        &mut simulation,
        bird,
        RotwkCommand::AttackObject { target: prey },
    );
    let mut halted = false;
    for _ in 0..(25 * SIMULATION_HZ) {
        simulation.tick().unwrap();
        if !halted && phase(&simulation, ids, bird) == "follow-through" {
            select_and(&mut simulation, bird, RotwkCommand::Stop);
            halted = true;
            continue;
        }
        if halted && phase(&simulation, ids, bird) == "hover" {
            break;
        }
        assert!(
            !(halted && matches!(phase(&simulation, ids, bird), "swoop" | "attack")),
            "swooped again after the Stop"
        );
    }
    assert!(halted, "never reached a follow-through");
    for _ in 0..(5 * SIMULATION_HZ) {
        simulation.tick().unwrap();
        assert_eq!(phase(&simulation, ids, bird), "hover");
    }
}

#[test]
fn a_grabbing_giant_bird_carries_its_catch_away_and_lets_it_go() {
    let (mut simulation, ids, units) = scene(
        &[
            ("Grabber", Vec2::new(300.0, 480.0), PLAYER),
            ("Prey", Vec2::new(550.0, 480.0), OTHER),
        ],
        &flat_map(96, 96),
    );
    let (bird, prey) = (units[0], units[1]);
    simulation.tick().unwrap();
    select_and(
        &mut simulation,
        bird,
        RotwkCommand::AttackObject { target: prey },
    );
    let held = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(ids.contained, prey)
            .is_some_and(|held| held.container == bird)
    };
    let mut caught = false;
    for _ in 0..(15 * SIMULATION_HZ) {
        simulation.tick().unwrap();
        if held(&simulation) {
            caught = true;
            break;
        }
    }
    assert!(caught, "never caught its prey");
    let mut carried_through = false;
    let mut released = false;
    for _ in 0..(10 * SIMULATION_HZ) {
        simulation.tick().unwrap();
        if !held(&simulation) {
            released = true;
            break;
        }
        carried_through |= phase(&simulation, ids, bird) == "follow-through";
        assert_eq!(
            simulation
                .state()
                .get(ids.armed, bird)
                .and_then(|armed| armed.target),
            None,
            "took a target while carrying"
        );
    }
    assert!(carried_through, "flew no follow-through with its catch");
    assert!(released, "never let its catch go");
}

#[test]
fn a_giant_bird_lifts_its_path_over_a_ridge() {
    let map = ridge(96, 96, 2400);
    let (mut simulation, ids, units) = scene(&[("Bird", Vec2::new(200.0, 480.0), PLAYER)], &map);
    let bird = units[0];
    let top = simulation.rules().ground_height(450.0, 480.0);
    assert!(top > 60.0, "the ridge is only {top} high");
    simulation.tick().unwrap();
    select_and(
        &mut simulation,
        bird,
        RotwkCommand::MoveTo {
            x: 750.0,
            y: 480.0,
            queue: false,
        },
    );
    let mut lowest_over_ridge = f32::INFINITY;
    let mut crossed = false;
    for _ in 0..(20 * SIMULATION_HZ) {
        simulation.tick().unwrap();
        let x = position(&simulation, ids, bird).x;
        if (400.0..500.0).contains(&x) {
            lowest_over_ridge = lowest_over_ridge.min(above(&simulation, ids, bird));
        }
        if x > 550.0 {
            crossed = true;
            break;
        }
    }
    assert!(crossed, "never crossed the ridge");
    assert!(
        lowest_over_ridge > 20.0,
        "crossed the ridge {lowest_over_ridge} above it"
    );
}

#[test]
fn two_giant_birds_fighting_each_other_duel_at_their_own_height() {
    let (mut simulation, ids, units) = scene(
        &[
            ("Bird", Vec2::new(300.0, 480.0), PLAYER),
            ("Bird", Vec2::new(700.0, 480.0), OTHER),
        ],
        &flat_map(96, 96),
    );
    let (ours, theirs) = (units[0], units[1]);
    simulation.tick().unwrap();
    order(
        &mut simulation,
        OTHER,
        theirs,
        RotwkCommand::AttackObject { target: ours },
    );
    simulation.tick().unwrap();
    order(
        &mut simulation,
        PLAYER,
        ours,
        RotwkCommand::AttackObject { target: theirs },
    );
    let mut duelled = false;
    let mut lowest_duel = f32::INFINITY;
    let mut struck = false;
    for _ in 0..(20 * SIMULATION_HZ) {
        simulation.tick().unwrap();
        for bird in [ours, theirs] {
            let (mode, set) = swoop(&simulation, ids, bird);
            if phase(&simulation, ids, bird) == "swoop" && mode == "duel" {
                duelled = true;
                assert!(set == "SUPERSONIC" || set == "WANDER", "duelled on {set}");
                lowest_duel = lowest_duel.min(above(&simulation, ids, bird));
            }
            struck |= mode == "duel" && phase(&simulation, ids, bird) == "attack";
        }
    }
    assert!(duelled, "neither bird duelled");
    assert!(struck, "no duel came to a strike");
    assert!(lowest_duel > 30.0, "a duel dived to {lowest_duel} up");
}

#[test]
fn a_swoop_power_dives_in_on_its_panic_set() {
    let (mut simulation, ids, units) = scene(
        &[
            ("PowerBird", Vec2::new(300.0, 480.0), PLAYER),
            ("Prey", Vec2::new(550.0, 480.0), OTHER),
        ],
        &flat_map(96, 96),
    );
    let (bird, prey) = (units[0], units[1]);
    simulation.tick().unwrap();
    select_and(
        &mut simulation,
        bird,
        RotwkCommand::FireSpecialPower {
            button: 0,
            target: Some(prey),
            at: None,
        },
    );
    let mut powered = false;
    let mut fastest = 0.0f32;
    let mut struck = false;
    for _ in 0..(15 * SIMULATION_HZ) {
        simulation.tick().unwrap();
        let (mode, set) = swoop(&simulation, ids, bird);
        if mode == "power" && phase(&simulation, ids, bird) == "swoop" {
            powered = true;
            assert_eq!(set, "PANIC");
            fastest = fastest.max(simulation.state().get(ids.body, bird).unwrap().0.speed);
        }
        if mode == "power" && phase(&simulation, ids, bird) == "attack" {
            struck = true;
            break;
        }
    }
    assert!(powered, "the power never started a power swoop");
    assert!(
        fastest > 90.5,
        "power-swooped at {fastest}, no faster than SUPERSONIC"
    );
    assert!(struck, "the power swoop never struck");
}

#[test]
fn a_giant_birds_path_keeps_off_a_tower_in_the_way() {
    let (mut simulation, ids, units) = scene(
        &[
            ("Bird", Vec2::new(200.0, 480.0), PLAYER),
            ("Tower", Vec2::new(480.0, 480.0), PLAYER),
        ],
        &flat_map(96, 96),
    );
    let (bird, tower) = (units[0], units[1]);
    simulation.tick().unwrap();
    select_and(
        &mut simulation,
        bird,
        RotwkCommand::MoveTo {
            x: 760.0,
            y: 480.0,
            queue: false,
        },
    );
    let foot = position(&simulation, ids, tower);
    let mut nearest = f32::INFINITY;
    let mut arrived = false;
    for _ in 0..(20 * SIMULATION_HZ) {
        simulation.tick().unwrap();
        let off = position(&simulation, ids, bird).minus(foot).length();
        let up = above(&simulation, ids, bird);
        nearest = nearest.min((off * off + up * up).sqrt());
        if position(&simulation, ids, bird).x > 700.0 && phase(&simulation, ids, bird) == "hover" {
            arrived = true;
            break;
        }
    }
    assert!(arrived, "never arrived past the tower");
    assert!(nearest > 70.0, "passed {nearest} from the tower's foot");
}

#[test]
fn a_giant_bird_ordered_onto_a_tower_hovers_beside_it() {
    let (mut simulation, ids, units) = scene(
        &[
            ("Bird", Vec2::new(200.0, 480.0), PLAYER),
            ("Tower", Vec2::new(480.0, 480.0), PLAYER),
        ],
        &flat_map(96, 96),
    );
    let (bird, tower) = (units[0], units[1]);
    simulation.tick().unwrap();
    select_and(
        &mut simulation,
        bird,
        RotwkCommand::MoveTo {
            x: 480.0,
            y: 480.0,
            queue: false,
        },
    );
    let mut arrived = false;
    for _ in 0..(15 * SIMULATION_HZ) {
        simulation.tick().unwrap();
        if phase(&simulation, ids, bird) == "hover" && !simulation.state().contains(ids.path, bird)
        {
            arrived = true;
            break;
        }
    }
    assert!(arrived, "never arrived");
    let off = position(&simulation, ids, bird)
        .minus(position(&simulation, ids, tower))
        .length();
    assert!(
        off > 80.0,
        "hovers {off} from the tower's middle, inside its reach"
    );
}

fn strikes_in(
    simulation: &mut Simulation<RotwkRules>,
    ids: Ids,
    bird: Entity,
    seconds: u32,
) -> u32 {
    let mut strikes = 0;
    let mut previous = phase(simulation, ids, bird);
    for _ in 0..(seconds * SIMULATION_HZ) {
        simulation.tick().unwrap();
        let now = phase(simulation, ids, bird);
        if now == "attack" && previous != "attack" {
            strikes += 1;
        }
        previous = now;
    }
    strikes
}

#[test]
fn a_giant_bird_strikes_prey_standing_against_a_tower() {
    let (mut simulation, ids, units) = scene(
        &[
            ("Bird", Vec2::new(200.0, 480.0), PLAYER),
            ("Prey", Vec2::new(480.0, 522.0), OTHER),
            ("Tower", Vec2::new(480.0, 480.0), OTHER),
        ],
        &flat_map(96, 96),
    );
    let (bird, prey) = (units[0], units[1]);
    simulation.tick().unwrap();
    select_and(
        &mut simulation,
        bird,
        RotwkCommand::AttackObject { target: prey },
    );
    let strikes = strikes_in(&mut simulation, ids, bird, 15);
    assert!(strikes >= 2, "{strikes} strike(s) in 15 s");
}

#[test]
fn a_giant_bird_following_through_off_the_map_comes_back() {
    let (mut simulation, ids, units) = scene(
        &[
            ("Bird", Vec2::new(600.0, 480.0), PLAYER),
            ("Prey", Vec2::new(900.0, 480.0), OTHER),
        ],
        &flat_map(96, 96),
    );
    let (bird, prey) = (units[0], units[1]);
    simulation.tick().unwrap();
    select_and(
        &mut simulation,
        bird,
        RotwkCommand::AttackObject { target: prey },
    );
    let strikes = strikes_in(&mut simulation, ids, bird, 15);
    assert!(strikes >= 2, "{strikes} strike(s) in 15 s");
}

#[test]
fn a_giant_bird_follows_through_upward_after_a_steep_dive() {
    let (mut simulation, ids, units) = scene(
        &[
            ("Bird", Vec2::new(520.0, 480.0), PLAYER),
            ("Prey", Vec2::new(550.0, 480.0), OTHER),
        ],
        &flat_map(96, 96),
    );
    let (bird, prey) = (units[0], units[1]);
    simulation.tick().unwrap();
    select_and(
        &mut simulation,
        bird,
        RotwkCommand::AttackObject { target: prey },
    );
    let mut passes = Vec::new();
    let mut previous = phase(&simulation, ids, bird);
    for _ in 0..(20 * SIMULATION_HZ) {
        simulation.tick().unwrap();
        let now = phase(&simulation, ids, bird);
        if now == "follow-through" {
            if previous != "follow-through" {
                passes.push((0u32, 0.0f32));
            }
            if let Some((ticks, highest)) = passes.last_mut() {
                *ticks += 1;
                *highest = highest.max(above(&simulation, ids, bird));
            }
        }
        previous = now;
    }
    assert!(
        passes.len() >= 2,
        "{} follow-through(s) in 20 s",
        passes.len()
    );
    for (ticks, highest) in passes {
        assert!(highest > 30.0, "a follow-through climbed only {highest} up");
        assert!(
            ticks < 6 * 40,
            "a follow-through ran {ticks} ticks, into its frame cap"
        );
    }
}

#[test]
fn a_giant_bird_killed_mid_pass_stops_flying() {
    let (mut simulation, ids, units) = scene(
        &[
            ("Bird", Vec2::new(300.0, 480.0), PLAYER),
            ("Prey", Vec2::new(550.0, 480.0), OTHER),
        ],
        &flat_map(96, 96),
    );
    let (bird, prey) = (units[0], units[1]);
    simulation.tick().unwrap();
    select_and(
        &mut simulation,
        bird,
        RotwkCommand::AttackObject { target: prey },
    );
    for _ in 0..(2 * SIMULATION_HZ) {
        simulation.tick().unwrap();
    }
    assert_eq!(phase(&simulation, ids, bird), "swoop");
    simulation.submit(PLAYER, RotwkCommand::DebugKill).unwrap();
    simulation.tick().unwrap();
    let dead_at = position(&simulation, ids, bird);
    for _ in 0..SIMULATION_HZ {
        simulation.tick().unwrap();
    }
    let moved = position(&simulation, ids, bird).minus(dead_at).length();
    assert!(moved < 1.0, "flew {moved} on after its death");
}

#[test]
fn a_guarding_giant_bird_ranks_by_its_set_only_for_a_computer_owner() {
    let nemesis = |owner: PlayerId| {
        let (mut simulation, ids, entities) = scene_with(
            &[
                ("GuardBird", Vec2::new(500.0, 500.0), owner),
                ("Prey", Vec2::new(560.0, 500.0), PlayerId::new(0)),
                ("Bait", Vec2::new(800.0, 500.0), PlayerId::new(0)),
            ],
            &flat_map(160, 160),
            |rules| {
                rules.with_map_players(
                    2,
                    vec![String::from("Wild")],
                    vec![crate::alliance::MapRelations {
                        enemies: vec![PlayerId::new(0)],
                        ..crate::alliance::MapRelations::default()
                    }],
                )
            },
        );
        order(&mut simulation, owner, entities[0], RotwkCommand::Stop);
        for _ in 0..90 {
            simulation.tick().unwrap();
            let found = simulation
                .state()
                .get(ids.guard, entities[0])
                .and_then(|guard| guard.nemesis);
            if let Some(found) = found {
                return entities.iter().position(|&e| e == found);
            }
        }
        None
    };
    assert_eq!(nemesis(PlayerId::new(2)), Some(2));
    assert_eq!(nemesis(PlayerId::new(1)), Some(1));
}
