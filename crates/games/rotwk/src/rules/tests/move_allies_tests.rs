use super::*;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 1000\nEnd\n\
Object Brute\n  Side = Men\n  KindOf = MONSTER PATH_THROUGH_INFANTRY SELECTABLE\n  CrushableLevel = 0\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 12\n  GeometryHeight = 30\nEnd\n\
Object Titan\n  Side = Men\n  KindOf = MONSTER PATH_THROUGH_INFANTRY SELECTABLE\n  CrushableLevel = 4\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 12\n  GeometryHeight = 30\nEnd\n\
Object Cube\n  Side = Men\n  KindOf = MONSTER PATH_THROUGH_INFANTRY SELECTABLE\n  CrushableLevel = 4\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 10\n  GeometryMinorRadius = 10\n  GeometryHeight = 30\nEnd\n\
Object Stubborn\n  Side = Men\n  KindOf = INFANTRY MOVE_FOR_NOONE SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Walker\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Wide\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 15\n  GeometryHeight = 16\nEnd\n\
Object Slab\n  Side = Men\n  KindOf = VEHICLE LARGE_RECTANGLE_PATHFIND SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Stander\n  Side = Men\n  KindOf = INFANTRY PATH_THROUGH_EACH_OTHER SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn walker_route_after_the_brute_is_ordered(brute: &str) -> Option<Path> {
    pushed_route(brute, "Walker", &[]).0
}

fn pushed_route(
    brute: &str,
    walker: &str,
    walls: &[(Vec2, f32, f32)],
) -> (Option<Path>, impl Fn((i32, i32)) -> bool) {
    let (path, stamped, _, _) = pushed_simulation(brute, walker, walls);
    (path, stamped)
}

fn pushed_simulation(
    brute: &str,
    walker: &str,
    walls: &[(Vec2, f32, f32)],
) -> (
    Option<Path>,
    impl Fn((i32, i32)) -> bool,
    Simulation<RotwkRules>,
    Entity,
) {
    pushed_simulation_walking(brute, walker, walls, None)
}

fn pushed_simulation_walking(
    brute: &str,
    walker: &str,
    walls: &[(Vec2, f32, f32)],
    walks_to: Option<Vec2>,
) -> (
    Option<Path>,
    impl Fn((i32, i32)) -> bool,
    Simulation<RotwkRules>,
    Entity,
) {
    pushed_simulation_after(brute, walker, walls, walks_to, 1, true)
}

fn pushed_simulation_after(
    brute: &str,
    walker: &str,
    walls: &[(Vec2, f32, f32)],
    walks_to: Option<Vec2>,
    settle: u32,
    order_brute: bool,
) -> (
    Option<Path>,
    impl Fn((i32, i32)) -> bool,
    Simulation<RotwkRules>,
    Entity,
) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = [brute.to_owned(), walker.to_owned()];
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let mut spawns: Vec<Spawn> = [(brute, 200.0), (walker, 400.0)]
        .iter()
        .map(|(name, x)| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    Vec2::new(*x, 400.0),
                    Facing::EAST,
                    PLAYER,
                )
                .unwrap()
        })
        .collect();
    let mut footprints = Vec::new();
    for (at, major, minor) in walls {
        footprints.push((
            spawns.len(),
            vec![mint_sage_passability::FootprintRecord {
                offset: Vec2::ZERO,
                footprint: mint_sage_passability::Footprint::Box {
                    major: *major,
                    minor: *minor,
                },
            }],
        ));
        spawns.push(spawn_at(at.x, at.y, KindOf::STRUCTURE, PLAYER));
    }
    let passability = Passability::build(&flat_map(128, 128), 5.0 / 128.0).unwrap();
    let extent = passability.grid.extent();
    let rules = RotwkRules::new(passability, spawns)
        .with_structure_footprints(footprints)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_teams(vec![None, None]);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    if let Some(at) = walks_to {
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![units[1]],
                },
            )
            .unwrap();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::MoveTo {
                    x: at.x,
                    y: at.y,
                    queue: false,
                },
            )
            .unwrap();
        for _ in 0..settle {
            simulation.tick().unwrap();
        }
    }
    if order_brute {
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![units[0]],
                },
            )
            .unwrap();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::MoveTo {
                    x: 600.0,
                    y: 400.0,
                    queue: false,
                },
            )
            .unwrap();
        simulation.tick().unwrap();
    }
    let counts = crate::obstacles::counts_of(simulation.state(), ids.obstacles).to_vec();
    let stamped = move |(x, y): (i32, i32)| {
        let cell = Cell::new(u16::try_from(x).unwrap(), u16::try_from(y).unwrap());
        let index = usize::try_from(extent.index(cell).unwrap()).unwrap();
        counts.get(index).is_some_and(|count| *count > 0)
    };
    (
        simulation.state().get(ids.path, units[1]).cloned(),
        stamped,
        simulation,
        units[1],
    )
}

#[test]
fn a_path_through_infantry_mover_sends_an_idle_ally_aside_clear_of_its_route() {
    let path = walker_route_after_the_brute_is_ordered("Brute").expect("the walker was not moved");
    assert!((path.goal.y - 400.0).abs() > 12.0, "{:?}", path.goal);
    assert!((path.goal.x - 400.0).abs() < 10.0, "{:?}", path.goal);
}

#[test]
fn the_aside_spot_is_the_nearest_cell_whose_box_misses_the_route() {
    let path = walker_route_after_the_brute_is_ordered("Brute").expect("the walker was not moved");
    assert!(
        (path.goal.y - 400.0).abs() < 15.5 && (path.goal.y - 400.0).abs() > 14.5,
        "{:?}",
        path.goal
    );
}

#[test]
fn a_pushed_unit_is_in_the_move_away_state_for_ten_seconds() {
    let (path, _, mut simulation, walker) = pushed_simulation("Brute", "Walker", &[]);
    assert!(path.is_some());
    let ids = simulation.rules().ids().unwrap();
    let now = simulation.frame();
    let at = |ms| {
        now.checked_add(LogicFrames::from_engine_milliseconds(ms))
            .unwrap()
    };
    assert!(RotwkRules::ai_busy(
        simulation.state(),
        ids,
        walker,
        at(9_000)
    ));
    assert!(!RotwkRules::ai_busy(
        simulation.state(),
        ids,
        walker,
        at(10_500)
    ));
    for _ in 0..200 {
        simulation.tick().unwrap();
    }
    assert!(!simulation.state().contains(ids.ai_busy, walker));
}

#[test]
fn a_pushed_unit_steps_aside_where_its_whole_footprint_fits() {
    let (path, stamped) = pushed_route("Brute", "Wide", &[(Vec2::new(400.0, 445.0), 60.0, 10.0)]);
    assert!(
        (40..=46).any(|x| stamped((x, 44))),
        "the wall stamps row 44"
    );
    let goal = path.expect("the walker was not moved").goal;
    #[allow(clippy::cast_possible_truncation)]
    let (cx, cy) = (
        (goal.x / 10.0).floor() as i32,
        (goal.y / 10.0).floor() as i32,
    );
    for y in cy - 1..=cy + 1 {
        for x in cx - 1..=cx + 1 {
            assert!(
                !stamped((x, y)),
                "goal {goal:?}: its footprint covers stamped cell {x},{y}"
            );
        }
    }
}

#[test]
fn a_mover_does_not_push_an_ally_its_search_looks_past() {
    assert!(pushed_route("Slab", "Stander", &[]).0.is_none());
}

#[test]
fn a_mover_pushes_an_idle_ally_its_search_counts() {
    assert!(pushed_route("Walker", "Stander", &[]).0.is_some());
}

#[test]
fn a_horde_blocked_by_its_own_standing_members_pushes_a_loose_ally_beside_its_route() {
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let formation = square_formation();
    let spawn = HordeSpawn {
        formation: 0,
        origin: Vec2::new(150.0, 150.0),
        facing: Facing::EAST,
        member: spawn_at(0.0, 0.0, KindOf::SELECTABLE, PLAYER),
        count: u32::try_from(formation.len()).unwrap(),
        vision: crate::shroud::HORDE_SHROUD_RANGE,
    };
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(&data, &["Walker".to_owned()]);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let ally = kinds
        .spawn(
            &data.combat,
            kinds.kind("Walker").unwrap(),
            Vec2::new(300.0, 168.0),
            Facing::EAST,
            PLAYER,
        )
        .unwrap();
    let rules = RotwkRules::new(passability, vec![ally])
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_hordes(vec![formation], vec![spawn]);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let loose = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .iter()
        .copied()
        .find(|&entity| {
            !simulation.state().contains(ids.member, entity)
                && !simulation.state().contains(ids.horde, entity)
        })
        .unwrap();
    order_horde(
        &mut simulation,
        ids,
        Modes::default(),
        Vec2::new(450.0, 150.0),
    );
    simulation.tick().unwrap();
    assert!(simulation.state().contains(ids.path, loose));
}

const ATTACK_DATA: &str = "\
Weapon Club\n  AttackRange = 60\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 1\n    DamageType = CRUSH\n  End\nEnd\n\
Object Basher\n  Side = Men\n  KindOf = MONSTER PATH_THROUGH_INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Club\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 12\n  GeometryHeight = 30\nEnd\n\
Object Victim\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

#[test]
fn an_attack_approach_sends_an_idle_ally_on_its_line_aside() {
    let text = format!("{DATA}{ATTACK_DATA}");
    let doc =
        mint_sage_ini::Document::parse("t.ini", &text, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["Basher", "Walker", "Victim"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawn = |name: &str, x: f32, owner: PlayerId| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                Vec2::new(x, 400.0),
                Facing::EAST,
                owner,
            )
            .unwrap()
    };
    let spawns = vec![
        spawn("Basher", 200.0, PLAYER),
        spawn("Walker", 400.0, PLAYER),
        spawn("Victim", 700.0, OTHER),
    ];
    let passability = Passability::build(&flat_map(128, 128), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_teams(vec![None, None]);
    let mut simulation = Simulation::new(5, rules).unwrap();
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
                units: vec![units[0]],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: units[2] })
        .unwrap();
    for _ in 0..5 {
        simulation.tick().unwrap();
    }
    let path = simulation
        .state()
        .get(ids.path, units[1])
        .expect("the walker was not moved");
    assert!((path.goal.y - 400.0).abs() > 5.0, "{:?}", path.goal);
}

#[test]
fn only_a_big_path_through_infantry_mover_pushes_a_walking_ally() {
    let goal_after = |pusher: &str| {
        let (path, _, _, _) =
            pushed_simulation_walking(pusher, "Walker", &[], Some(Vec2::new(400.0, 100.0)));
        path.expect("the walker has a route").goal
    };
    assert!((goal_after("Brute").y - 100.0).abs() < 6.0);
    assert!((goal_after("Titan").y - 100.0).abs() > 20.0);
}

#[test]
fn a_move_for_noone_ally_is_not_pushed() {
    assert!(pushed_route("Brute", "Stubborn", &[]).0.is_none());
}

fn walker_after_push_and_resume(goal: Vec2) -> Vec2 {
    let (_, _, mut simulation, walker) =
        pushed_simulation_walking("Titan", "Walker", &[], Some(goal));
    let ids = simulation.rules().ids().unwrap();
    for _ in 0..900 {
        simulation.tick().unwrap();
    }
    simulation.state().get(ids.body, walker).unwrap().0.position
}

#[test]
fn a_pushed_mover_resumes_its_order_after_stepping_aside() {
    let goal = Vec2::new(400.0, 100.0);
    assert!(walker_after_push_and_resume(goal).minus(goal).length() < 15.0);
}

#[test]
fn a_goal_is_close_to_the_pushers_destination_only_strictly_inside_its_radius() {
    let (_, _, simulation, _) = pushed_simulation("Titan", "Walker", &[]);
    let rules = simulation.rules();
    let at = |x| Vec2::new(x, 0.0);
    assert!(rules.within_radius_3d(at(11.9), at(0.0), 12.0));
    assert!(!rules.within_radius_3d(at(12.0), at(0.0), 12.0));
}

#[test]
fn the_pusher_radius_is_the_bounding_circle_of_its_geometry() {
    let (_, _, simulation, _) = pushed_simulation("Cube", "Walker", &[]);
    let ids = simulation.rules().ids().unwrap();
    let cube = simulation.state().store(ids.body).unwrap().entities()[0];
    let radius = simulation
        .rules()
        .pusher_radius(simulation.state(), ids, cube);
    assert!((radius - 200f32.sqrt()).abs() < 1e-3, "{radius}");
}

#[test]
fn a_geometry_upgrade_widens_the_pusher_radius() {
    let text = "\
Upgrade Upgrade_Grow\n  Type = PLAYER\nEnd\n\
Object Grower\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag\n    MaxHealth = 100\n  End\n  \
  Behavior = GeometryUpgrade TagG\n    TriggeredBy = Upgrade_Grow\n    \
    ShowGeometry = Wing\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 10\n  GeometryMinorRadius = 10\n  \
  GeometryHeight = 30\n  \
  AdditionalGeometry = BOX\n  GeometryName = Wing\n  GeometryActive = No\n  \
  GeometryOffset = X:40 Y:0 Z:0\n  GeometryMajorRadius = 10\n  GeometryMinorRadius = 10\n  \
  GeometryHeight = 30\nEnd\n";
    let radius = |granted: bool| {
        let doc = mint_sage_ini::Document::parse(
            "t.ini",
            text,
            &mint_sage_ini::BlockTable::corpus_path(),
        )
        .unwrap();
        let (data, _) =
            crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
        let (kinds, missing) = KindTable::closure(&data, &["Grower".to_owned()]);
        assert!(missing.is_empty(), "{missing:?}");
        let kinds = Arc::new(kinds);
        let production = kinds.production(&data);
        let kind = kinds.kind("Grower").unwrap();
        let spawn = kinds
            .spawn(
                &data.combat,
                kind,
                Vec2::new(100.0, 100.0),
                Facing::EAST,
                PLAYER,
            )
            .unwrap();
        let mut mask = mint_sim::UpgradeMask::EMPTY;
        if granted {
            mask.set(data.upgrade_bits.bit("Upgrade_Grow").unwrap());
        }
        let passability = Passability::build(&flat_map(64, 64), 5.0 / 128.0).unwrap();
        let rules = RotwkRules::new(passability, vec![spawn])
            .with_combat(data.combat.clone())
            .with_construction(Arc::clone(&kinds), production)
            .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
            .with_initial_upgrades(vec![mask]);
        let simulation = Simulation::new(1, rules).unwrap();
        let ids = simulation.rules().ids().unwrap();
        let grower = simulation.state().store(ids.kind).unwrap().entities()[0];
        simulation
            .rules()
            .pusher_radius(simulation.state(), ids, grower)
    };
    let plain = radius(false);
    let grown = radius(true);
    assert!((plain - 200f32.sqrt()).abs() < 1e-3, "{plain}");
    assert!(
        (grown - (50f32 * 50.0 + 10.0 * 10.0).sqrt()).abs() < 1e-3,
        "{grown}"
    );
}

#[test]
fn an_idle_ally_keeps_its_last_goal_until_stopped() {
    let goal = Vec2::new(590.0, 400.0);
    let (_, _, mut simulation, walker) =
        pushed_simulation_after("Titan", "Walker", &[], Some(goal), 300, false);
    let ids = simulation.rules().ids().unwrap();
    assert!(
        !simulation.state().contains(ids.path, walker),
        "still walking"
    );
    let kept = simulation
        .rules()
        .pushed_goal(simulation.state(), ids, walker)
        .expect("the goal was dropped on arrival");
    assert!(kept.minus(goal).length() < 10.0, "{kept:?}");
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![walker],
            },
        )
        .unwrap();
    simulation.submit(PLAYER, RotwkCommand::Stop).unwrap();
    simulation.tick().unwrap();
    assert!(simulation
        .rules()
        .pushed_goal(simulation.state(), ids, walker)
        .is_none());
}

fn pushed_walker_is_busy_for_thirty_seconds(goal: Vec2) -> bool {
    let (_, _, mut simulation, walker) =
        pushed_simulation_after("Titan", "Walker", &[], Some(goal), 400, false);
    let ids = simulation.rules().ids().unwrap();
    let titan = simulation.state().store(ids.body).unwrap().entities()[0];
    let walker_kind = simulation.state().get(ids.kind, walker).unwrap().0;
    for command in [
        RotwkCommand::Debug(crate::rules::debug::DebugCommand::Teleport {
            target: crate::rules::debug::DebugTarget::Near {
                kind: walker_kind,
                x: goal.x,
                y: goal.y,
            },
            x: 400.0,
            y: 400.0,
        }),
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![titan],
        },
        RotwkCommand::MoveTo {
            x: 600.0,
            y: 400.0,
            queue: false,
        },
    ] {
        simulation.submit(PLAYER, command).unwrap();
    }
    simulation.tick().unwrap();
    assert!(
        simulation.state().contains(ids.path, walker),
        "the walker was not pushed"
    );
    let at = simulation
        .frame()
        .checked_add(LogicFrames::from_engine_milliseconds(30_000))
        .unwrap();
    RotwkRules::ai_busy(simulation.state(), ids, walker, at)
}

#[test]
fn a_pushed_ally_whose_goal_is_inside_the_pushers_radius_stays_aside_for_good() {
    assert!(
        pushed_walker_is_busy_for_thirty_seconds(Vec2::new(599.0, 400.0)),
        "a goal 7 from the destination was released"
    );
    assert!(
        !pushed_walker_is_busy_for_thirty_seconds(Vec2::new(560.0, 400.0)),
        "a goal 40 from the destination was held for good"
    );
}

#[test]
fn a_formation_status_ally_refuses_the_push_only_while_idle() {
    let (_, _, simulation, walker) =
        pushed_simulation_after("Brute", "Walker", &[], None, 0, false);
    let ids = simulation.rules().ids().unwrap();
    let mut world = World::new();
    world.restore(&simulation.state().snapshot());
    let rules = simulation.rules();
    assert!(
        !rules.refuses_by_formation_status(&world, ids, walker),
        "no status 89: pushed"
    );
    world
        .insert(
            ids.in_formation_template,
            walker,
            crate::rules::drag_formation::InFormationTemplate,
        )
        .unwrap();
    assert!(rules.refuses_by_formation_status(&world, ids, walker));
    let victim = simulation.state().store(ids.body).unwrap().entities()[0];
    let order = crate::combat::AttackOrder {
        target: victim,
        planned: false,
        horde: None,
        guard: false,
        relation_free: false,
        order_owner: None,
    };
    world.insert(ids.attack_order, walker, order).unwrap();
    assert!(
        !rules.refuses_by_formation_status(&world, ids, walker),
        "a fighting status-89 unit is pushed"
    );
}
