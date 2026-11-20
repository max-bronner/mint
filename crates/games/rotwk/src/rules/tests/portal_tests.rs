use super::*;

const DATA: &str = "\
Object Ladder\n  Side = Men\n  KindOf = STRUCTURE IMMOBILE\n  \
  Behavior = DynamicPortalBehaviour Tag_Portal\n    GenerateNow = Yes\n    \
    ObjectFilter = ANY +INFANTRY -CAVALRY\n    BonePrefix = Rung\n    NumberOfBones = 2\n    \
    WayPoint = Index:0 Type:Walk\n    WayPoint = Index:1 Type:Walk\n    \
    Link = From:0 To:1\n    Link = From:1 To:0\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 5\n  GeometryMinorRadius = 5\n  GeometryHeight = 5\nEnd\n\
Object Foot\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\nEnd\n\
Object Rider\n  Side = Men\n  KindOf = INFANTRY CAVALRY SELECTABLE\nEnd\n";

struct Fixture {
    simulation: Simulation<RotwkRules>,
    ids: Ids,
    foot: Entity,
    rider: Entity,
    enemy: Entity,
}

fn setup() -> Fixture {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["Ladder", "Foot", "Rider"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawn = |name: &str, at: f32, owner: PlayerId| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                Vec2::new(at, 300.0),
                Facing::EAST,
                owner,
            )
            .unwrap()
    };
    let objects = vec![
        spawn("Ladder", 300.0, PLAYER),
        spawn("Foot", 100.0, PLAYER),
        spawn("Rider", 120.0, PLAYER),
        spawn("Foot", 140.0, OTHER),
    ];
    let mut bones = vec![Vec::new(); 3];
    let ladder = usize::from(kinds.kind("Ladder").unwrap().0);
    bones[ladder] = vec![
        crate::perches::Perch {
            position: [-40.0, 0.0, 0.0],
            facing: Facing::EAST,
        },
        crate::perches::Perch {
            position: [40.0, 0.0, 30.0],
            facing: Facing::EAST,
        },
    ];
    let passability = Passability::build(&flat_map(160, 160), 5.0 / 128.0).unwrap();
    let mut simulation = Simulation::new(
        9,
        RotwkRules::new(passability, objects)
            .with_players(2)
            .with_teams(vec![Some(0), Some(1)])
            .with_combat(data.combat.clone())
            .with_construction(Arc::clone(&kinds), production)
            .with_portal_bones(bones),
    )
    .unwrap();
    simulation.tick().unwrap();
    simulation.tick().unwrap();
    let ids = simulation.rules().ids().unwrap();
    let all = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    Fixture {
        simulation,
        ids,
        foot: all[1],
        rider: all[2],
        enemy: all[3],
    }
}

#[test]
fn a_portal_admits_each_mover_by_its_filter_and_relationship() {
    let f = setup();
    let world = f.simulation.state();
    let rules = f.simulation.rules();
    let layer = crate::portals::layer_in(world, f.ids.portals).unwrap();
    assert_eq!(layer.links().len(), 2, "both `Link` lines are registered");
    let pass = |entity| rules.portal_pass_in(world, f.ids, entity);
    let slot = layer.links().first().unwrap().slot;
    assert!(pass(f.foot).has(slot), "an allied infantryman may climb");
    assert!(!pass(f.rider).has(slot), "the filter forbids CAVALRY");
    assert!(!pass(f.enemy).has(slot), "an enemy of the owner may not");
}

#[test]
fn a_portal_judges_the_movers_own_template_not_its_mount_or_first_member() {
    let f = setup();
    let mut world = World::new();
    world.restore(&f.simulation.state().snapshot());
    let rules = f.simulation.rules();
    let slot = crate::portals::layer_in(&world, f.ids.portals)
        .unwrap()
        .links()
        .first()
        .unwrap()
        .slot;
    let state = rules.upgrade_state_id().unwrap();
    let mut mounted = mint_sage_modules::UpgradeState::default();
    mounted.model_conditions.push("MOUNTED".into());
    world.insert(state, f.foot, mounted).unwrap();
    assert!(
        rules.portal_pass_in(&world, f.ids, f.foot).has(slot),
        "a mounted INFANTRY template is not a CAVALRY template"
    );
    let horde = crate::horde::Horde {
        formation: 0,
        slots: Vec::new(),
        members: vec![f.rider],
        free: Vec::new(),
        banner_slot: None,
        dirty: false,
        centre: Vec2::ZERO,
        climb_wall: None,
        climb_quiet: 0,
        climb_gap: 0.0,
        scaled_wall: false,
        rangefinder: None,
        pools: Vec::new(),
    };
    world.insert(f.ids.horde, f.foot, horde).unwrap();
    assert!(
        rules.portal_pass_in(&world, f.ids, f.foot).has(slot),
        "the horde's own INFANTRY template passes though its first member is CAVALRY"
    );
}

#[test]
fn a_mover_whose_look_ahead_reaches_a_portal_leg_is_not_re_planned() {
    let f = setup();
    let mut world = World::new();
    world.restore(&f.simulation.state().snapshot());
    let tag = mint_movement::Portal {
        id: 1,
        kind: 3,
        leg: mint_movement::Leg::Climb,
        rise: 40.0,
    };
    let route = mint_movement::Route {
        points: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(100.0, 0.0),
            Vec2::new(110.0, 0.0),
        ],
        extra_distance: 0.0,
        unsearched: false,
        portals: vec![None, None, Some(tag)],
    };
    let path = crate::unit::Path {
        route,
        goal: Vec2::new(110.0, 0.0),
        kind: crate::unit::PathKind::Ordered,
        facing: None,
    };
    world.insert(f.ids.path, f.foot, path).unwrap();
    let on_leg = |world: &mut World, x: f32| {
        let body = &mut world.get_mut(f.ids.body, f.foot).unwrap().0;
        body.position = Vec2::new(x, 0.0);
        body.waypoint = 1;
        body.speed = 30.0;
        (
            RotwkRules::on_portal_leg(world, f.ids, f.foot),
            RotwkRules::mid_portal(world, f.ids, f.foot),
        )
    };
    assert_eq!(on_leg(&mut world, 50.0), (false, false));
    assert_eq!(
        on_leg(&mut world, 99.5),
        (true, false),
        "the look-ahead is on the tagged leg before the mover is"
    );
}

const CROSSING: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Object Wall\n  Side = Men\n  KindOf = STRUCTURE IMMOBILE\n  \
  Geometry = BOX\n  GeometryMajorRadius = 10\n  GeometryMinorRadius = 400\n  GeometryHeight = 20\nEnd\n\
Object Ladder\n  Side = Men\n  KindOf = STRUCTURE IMMOBILE\n  \
  Behavior = DynamicPortalBehaviour Tag_Portal\n    GenerateNow = Yes\n    \
    ObjectFilter = ANY +INFANTRY -CAVALRY\n    BonePrefix = Rung\n    NumberOfBones = 2\n    \
    WayPoint = Index:0 Type:Walk\n    WayPoint = Index:1 Type:Walk\n    \
    Link = From:0 To:1\n    Link = From:1 To:0\n  End\n  \
  Geometry = BOX\n  GeometryMajorRadius = 3\n  GeometryMinorRadius = 3\n  GeometryHeight = 5\nEnd\n\
Object Foot\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 4000\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn crossing(bones: bool) -> (Simulation<RotwkRules>, Ids, Entity) {
    crossing_of(bones, false)
}

fn crossing_of(bones: bool, steep_disguiser: bool) -> (Simulation<RotwkRules>, Ids, Entity) {
    crossing_from(CROSSING.to_owned(), bones, steep_disguiser)
}

fn crossing_from(
    mut text: String,
    bones: bool,
    steep_disguiser: bool,
) -> (Simulation<RotwkRules>, Ids, Entity) {
    if steep_disguiser {
        text = text.replace("Type:Walk", "Type:Climb").replace(
            "Object Foot\n  Side = Men\n",
            "Object Foot\n  Side = Men\n  CommandSet = HeroSet\n  \
             Behavior = SpecialDisguiseUpdate Tag_Disguise\n    SpecialPowerTemplate = DisguisePower\n    \
             UnpackTime = 0\n    PreparationTime = 0\n    PackTime = 0\n  End\n",
        );
        text.push_str(
            "SpecialPower DisguisePower\n  ReloadTime = 0\nEnd\n\
             CommandSet HeroSet\n  1 = Fire_Disguise\nEnd\n\
             CommandButton Fire_Disguise\n  Command = SPECIAL_POWER\n  SpecialPower = DisguisePower\nEnd\n",
        );
    }
    let doc =
        mint_sage_ini::Document::parse("t.ini", &text, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["Wall", "Ladder", "Foot"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawn = |name: &str, x: f32| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                Vec2::new(x, 300.0),
                Facing::EAST,
                PLAYER,
            )
            .unwrap()
    };
    let objects = vec![
        spawn("Foot", 400.0),
        spawn("Wall", 500.0),
        spawn("Ladder", 500.0),
    ];
    let mut rungs = vec![Vec::new(); 3];
    if bones {
        rungs[usize::from(kinds.kind("Ladder").unwrap().0)] = vec![
            crate::perches::Perch {
                position: [-40.0, 0.0, 0.0],
                facing: Facing::EAST,
            },
            crate::perches::Perch {
                position: [40.0, 0.0, if steep_disguiser { 80.0 } else { 30.0 }],
                facing: Facing::EAST,
            },
        ];
    }
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let passability = Passability::build(&flat_map(160, 60), 5.0 / 128.0).unwrap();
    let mut simulation = Simulation::new(
        3,
        RotwkRules::new(passability, objects)
            .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
            .with_powers(power_names, power_reload)
            .with_players(1)
            .with_teams(vec![Some(0)])
            .with_structure_footprints(vec![(
                1,
                (-4..=4)
                    .map(|k| mint_sage_passability::FootprintRecord {
                        offset: Vec2::new(0.0, 100.0 * k as f32),
                        footprint: mint_sage_passability::Footprint::Box {
                            major: 10.0,
                            minor: 60.0,
                        },
                    })
                    .collect(),
            )])
            .with_combat(data.combat.clone())
            .with_construction(Arc::clone(&kinds), production)
            .with_portal_bones(rungs),
    )
    .unwrap();
    simulation.tick().unwrap();
    simulation.tick().unwrap();
    let ids = simulation.rules().ids().unwrap();
    let unit = simulation.state().store(ids.body).unwrap().entities()[0];
    (simulation, ids, unit)
}

fn walk_east(simulation: &mut Simulation<RotwkRules>, unit: Entity, ticks: usize) -> f32 {
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
                x: 600.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    let ids = simulation.rules().ids().unwrap();
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
    simulation.state().get(ids.body, unit).unwrap().0.position.x
}

#[test]
fn a_unit_crosses_a_wall_by_its_portal_and_not_without_one() {
    let (mut shut, _, unit) = crossing(false);
    let stuck = walk_east(&mut shut, unit, 600);
    assert!(stuck < 495.0, "no portal, no crossing: x = {stuck}");
    let (mut open, _, unit) = crossing(true);
    let across = walk_east(&mut open, unit, 600);
    assert!(across > 540.0, "through the ladder: x = {across}");
}

#[test]
fn a_fresh_order_given_while_crossing_is_dropped() {
    let (mut simulation, ids, unit) = crossing(true);
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
                x: 600.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    let mut crossing_now = false;
    for _ in 0..600 {
        simulation.tick().unwrap();
        if RotwkRules::on_portal_leg(simulation.state(), ids, unit) {
            crossing_now = true;
            break;
        }
    }
    assert!(crossing_now, "the unit reaches the ladder");
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 420.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..600 {
        simulation.tick().unwrap();
    }
    let x = simulation.state().get(ids.body, unit).unwrap().0.position.x;
    assert!(x > 540.0, "the old order finished: x = {x}");
}

#[test]
fn an_order_given_on_the_tagged_leg_is_held_and_reaches_the_new_point() {
    let (mut simulation, ids, unit) = crossing(true);
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![unit],
        },
        RotwkCommand::MoveTo {
            x: 600.0,
            y: 300.0,
            queue: false,
        },
    ] {
        simulation.submit(PLAYER, command).unwrap();
    }
    let mut on_leg = false;
    for _ in 0..600 {
        simulation.tick().unwrap();
        if RotwkRules::mid_portal(simulation.state(), ids, unit) {
            on_leg = true;
            break;
        }
    }
    assert!(on_leg, "the unit reaches the ladder");
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 420.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert!(simulation.state().contains(ids.deferred_order, unit));
    for _ in 0..900 {
        simulation.tick().unwrap();
    }
    let x = simulation.state().get(ids.body, unit).unwrap().0.position.x;
    assert!((x - 420.0).abs() < 40.0, "the held order ran: x = {x}");
}

const HORDE_OBJECTS: &str = "\
Object Spearman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 100\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 4000\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object PikeHorde\n  Side = Men\n  KindOf = INFANTRY SELECTABLE HORDE\n  CommandSet = HordeSet\n  \
  BuildCost = 100\n  BuildTime = 1\n  CommandPoints = 3\n  VisionRange = 100\n  \
  Behavior = HordeContain Tag_01\n    Slots = 3\n    \
    RankInfo = RankNumber:1 UnitType:Spearman Position:X:10 Y:-10 Position:X:10 Y:10 Position:X:-10 Y:0\n  \
  End\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = CampSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet CampSet\n  1 = Build_PikeHorde\nEnd\n\
CommandSet HordeSet\n  1 = Build_PikeHorde\nEnd\n\
CommandButton Build_PikeHorde\n  Command = UNIT_BUILD\n  Object = PikeHorde\nEnd\n";

fn horde_crossing(bones: bool) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let text = format!("{CROSSING}{HORDE_OBJECTS}");
    let doc =
        mint_sage_ini::Document::parse("t.ini", &text, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["Wall", "Ladder", "Camp", "PikeHorde"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawn = |name: &str, x: f32| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                Vec2::new(x, 300.0),
                Facing::EAST,
                PLAYER,
            )
            .unwrap()
    };
    let objects = vec![spawn("Wall", 500.0), spawn("Ladder", 500.0)];
    let mut rungs = vec![Vec::new(); kinds.len()];
    if bones {
        rungs[usize::from(kinds.kind("Ladder").unwrap().0)] = vec![
            crate::perches::Perch {
                position: [-40.0, 0.0, 0.0],
                facing: Facing::EAST,
            },
            crate::perches::Perch {
                position: [40.0, 0.0, 30.0],
                facing: Facing::EAST,
            },
        ];
    }
    let passability = Passability::build(&flat_map(160, 60), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, objects)
        .with_players(1)
        .with_teams(vec![Some(0)])
        .with_structure_footprints(vec![(
            0,
            (-4..=4)
                .map(|k| mint_sage_passability::FootprintRecord {
                    offset: Vec2::new(0.0, 100.0 * k as f32),
                    footprint: mint_sage_passability::Footprint::Box {
                        major: 10.0,
                        minor: 60.0,
                    },
                })
                .collect(),
        )])
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_portal_bones(rungs);
    let mut simulation = Simulation::new(4, rules).unwrap();
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == "PikeHorde")
        .unwrap()
        .index;
    simulation
        .submit(
            PLAYER,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x: 400.0,
                y: 300.0,
                facing: 0.0,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    simulation.tick().unwrap();
    let ids = simulation.rules().ids().unwrap();
    let horde = simulation.state().store(ids.horde).unwrap().entities()[0];
    let members = simulation
        .state()
        .get(ids.horde, horde)
        .unwrap()
        .members
        .clone();
    (simulation, ids, members)
}

#[test]
fn a_horde_crosses_a_wall_by_its_portal_with_its_members() {
    let east_of_wall = |bones: bool| {
        let (mut simulation, ids, members) = horde_crossing(bones);
        let horde = simulation.state().store(ids.horde).unwrap().entities()[0];
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![horde],
                },
            )
            .unwrap();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::MoveTo {
                    x: 600.0,
                    y: 300.0,
                    queue: false,
                },
            )
            .unwrap();
        let mut staged = false;
        for _ in 0..900 {
            simulation.tick().unwrap();
            staged |= simulation
                .state()
                .store(ids.portal_walk)
                .is_some_and(|store| !store.is_empty());
        }
        let across = members
            .iter()
            .filter(|member| {
                simulation
                    .state()
                    .get(ids.body, **member)
                    .is_some_and(|body| body.0.position.x > 520.0)
            })
            .count();
        (across, staged)
    };
    assert_eq!(east_of_wall(false), (0, false), "no portal, no crossing");
    assert_eq!(
        east_of_wall(true),
        (3, true),
        "every member crossed, the body having staged them"
    );
}

fn holds_condition(simulation: &Simulation<RotwkRules>, entity: Entity, name: &str) -> bool {
    simulation
        .rules()
        .modules
        .as_ref()
        .and_then(|wiring| simulation.state().get(wiring.state, entity))
        .is_some_and(|state| state.has_model_condition(name))
}

#[test]
fn a_disguised_unit_on_a_steep_climb_is_revealed() {
    let (mut simulation, ids, unit) = crossing_of(true, true);
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
            RotwkCommand::FireSpecialPower {
                button: 0,
                target: None,
                at: None,
            },
        )
        .unwrap();
    for _ in 0..3 {
        simulation.tick().unwrap();
    }
    assert!(holds_condition(&simulation, unit, "DISGUISED"));
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 600.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    let mut climbed = false;
    for _ in 0..600 {
        simulation.tick().unwrap();
        climbed |= holds_condition(&simulation, unit, "CLIMBING");
    }
    let _ = ids;
    assert!(climbed, "the steep leg sets CLIMBING");
    assert!(
        !holds_condition(&simulation, unit, "DISGUISED"),
        "the climb dropped the disguise"
    );
}

#[test]
fn a_horde_ordered_during_the_walk_wait_keeps_waiting_for_its_members() {
    let (mut simulation, ids, _) = horde_crossing(true);
    let horde = simulation.state().store(ids.horde).unwrap().entities()[0];
    let order = |simulation: &mut Simulation<RotwkRules>, x: f32| {
        for command in [
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![horde],
            },
            RotwkCommand::MoveTo {
                x,
                y: 300.0,
                queue: false,
            },
        ] {
            simulation.submit(PLAYER, command).unwrap();
        }
    };
    order(&mut simulation, 600.0);
    let open_walk = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .store(ids.portal_walk)
            .is_some_and(|store| store.iter().any(|(_, walk)| walk.until.is_none()))
    };
    let mut waited = false;
    for _ in 0..900 {
        simulation.tick().unwrap();
        if open_walk(&simulation) {
            waited = true;
            break;
        }
    }
    assert!(waited, "the members never started an open-ended walk");
    let before = simulation.state().get(ids.body, horde).unwrap().0.position;
    order(&mut simulation, 300.0);
    for _ in 0..6 {
        simulation.tick().unwrap();
    }
    let after = simulation.state().get(ids.body, horde).unwrap().0.position;
    assert!(
        open_walk(&simulation) && after.minus(before).length() < 1.0,
        "the body moved {} while a member still walked",
        after.minus(before).length()
    );
}

#[test]
fn a_fade_on_portals_unit_fades_only_while_its_route_is_on_the_portal() {
    let text = CROSSING.replace(
        "Object Foot
  Side = Men
",
        "Object Foot
  Side = Men
  Behavior = AIUpdateInterface Tag_AI
    FadeOnPortals = Yes
  End
",
    );
    let (mut simulation, ids, unit) = crossing_from(text, true, false);
    let opacity = |simulation: &Simulation<RotwkRules>| {
        simulation
            .rules()
            .portal_fade_opacity(simulation.state(), unit, simulation.frame())
    };
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
                x: 600.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    simulation.tick().unwrap();
    assert_eq!(opacity(&simulation), 1.0, "opaque on the approach");
    let mut faded = 1.0f32;
    for _ in 0..900 {
        simulation.tick().unwrap();
        faded = faded.min(opacity(&simulation));
    }
    assert!(faded < 0.5, "never faded: least opacity {faded}");
    assert_eq!(opacity(&simulation), 1.0, "opaque again across");
    assert!(!simulation.state().contains(ids.portal_fade, unit));
}
