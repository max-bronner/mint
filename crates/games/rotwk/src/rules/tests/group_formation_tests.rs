use super::*;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 1000\n  \
  FormationPriority = MELEE1\nEnd\n\
Locomotor HeroFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 1000\n  \
  FormationPriority = NO_FORMATION\nEnd\n\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 700
  FormationWidth = 1\n  FormationDepth = 1\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Captain\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  LocomotorSet\n    Locomotor = HeroFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

const CLICK: Vec2 = Vec2::new(500.0, 400.0);

fn moved_group() -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    moved_group_beside(None, 700)
}

fn moved_group_beside(
    enemy: Option<Vec2>,
    vision: u32,
) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    moved_group_with(enemy, vision, "")
}

fn moved_group_with(
    enemy: Option<Vec2>,
    vision: u32,
    extra: &str,
) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    moved_group_to(enemy, vision, extra, CLICK)
}

fn moved_group_to(
    enemy: Option<Vec2>,
    vision: u32,
    extra: &str,
    click: Vec2,
) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let data_text = DATA
        .replace("VisionRange = 700", &format!("VisionRange = {vision}"))
        .replace(
            "FormationWidth = 1",
            &format!("FormationWidth = 1\n  {extra}"),
        );
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        &data_text,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["Soldier", "Captain"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns: Vec<Spawn> = [
        ("Soldier", 330.0),
        ("Soldier", 365.0),
        ("Captain", 400.0),
        ("Soldier", 435.0),
        ("Soldier", 470.0),
    ]
    .iter()
    .map(|(name, y)| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                Vec2::new(200.0, *y),
                Facing::from_radians(core::f32::consts::FRAC_PI_2),
                PLAYER,
            )
            .unwrap()
    })
    .chain(enemy.map(|at| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind("Soldier").unwrap(),
                at,
                Facing::EAST,
                PlayerId::new(1),
            )
            .unwrap()
    }))
    .collect();
    let passability = Passability::build(&flat_map(128, 128), 5.0 / 128.0).unwrap();
    let rules = group_moves(RotwkRules::new(passability, spawns))
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_teams(vec![None, None])
        .with_players(2)
        .with_shroud(true);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let mut units = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    units.truncate(5);
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
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
                x: click.x,
                y: click.y,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..600 {
        simulation.tick().unwrap();
    }
    (simulation, ids, units)
}

fn formed_column(simulation: &Simulation<RotwkRules>, ids: Ids, units: &[Entity]) -> bool {
    let at = simulation
        .state()
        .get(ids.body, units[0])
        .unwrap()
        .0
        .position;
    (at.x - (CLICK.x - 55.0)).abs() < 6.0
}

#[test]
fn an_enemy_near_the_representative_refuses_the_column() {
    let (simulation, ids, units) = moved_group_beside(Some(Vec2::new(600.0, 470.0)), 700);
    assert!(!formed_column(&simulation, ids, &units));
}

#[test]
fn the_enemy_circle_adds_only_the_enemys_own_radius() {
    let (simulation, ids, units) = moved_group_beside(Some(Vec2::new(707.0, 470.0)), 700);
    assert!(formed_column(&simulation, ids, &units));
}

#[test]
fn an_enemy_in_fog_does_not_refuse_the_column() {
    let (simulation, ids, units) = moved_group_beside(Some(Vec2::new(680.0, 470.0)), 300);
    assert!(formed_column(&simulation, ids, &units));
}

#[test]
fn an_enemy_outside_the_representatives_firing_arc_does_not_refuse_the_column() {
    let enemy = Some(Vec2::new(600.0, 470.0));
    let (simulation, ids, units) = moved_group_with(enemy, 700, "FiringArc = 90");
    assert!(formed_column(&simulation, ids, &units));
    let (simulation, ids, units) = moved_group_with(enemy, 700, "FiringArc = 360");
    assert!(!formed_column(&simulation, ids, &units));
}

#[test]
fn only_a_crushable_bystander_narrows_the_group_path() {
    let bystander = Some(Vec2::new(CLICK.x + 15.0, CLICK.y));
    let (simulation, ids, units) = moved_group_with(bystander, 100, "CrushableLevel = 0");
    assert!(formed_column(&simulation, ids, &units));
    let (simulation, ids, units) = moved_group_with(bystander, 100, "CrushableLevel = 1");
    assert!(!formed_column(&simulation, ids, &units));
}

#[test]
fn the_representative_is_the_last_member_of_its_class() {
    let (simulation, ids, units) = moved_group_beside(Some(Vec2::new(200.0, 975.0)), 700);
    assert!(!formed_column(&simulation, ids, &units));
}

#[test]
fn a_group_move_forms_a_squad_behind_its_captain_facing_the_path() {
    let (simulation, ids, units) = moved_group();
    let bodies: Vec<MoverState> = units
        .iter()
        .map(|u| simulation.state().get(ids.body, *u).unwrap().0)
        .collect();
    let report: Vec<String> = bodies
        .iter()
        .map(|b| {
            format!(
                "({:.1}, {:.1}) {:.3}",
                b.position.x,
                b.position.y,
                b.facing.radians()
            )
        })
        .collect();
    let captain = bodies[2];
    assert!(captain.position.minus(CLICK).length() < 6.0, "{report:?}");
    let mut across: Vec<f32> = Vec::new();
    for body in bodies
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != 2)
        .map(|(_, b)| b)
    {
        assert!(
            (body.position.x - (CLICK.x - 55.0)).abs() < 6.0,
            "{report:?}"
        );
        assert!(
            body.facing.delta_to(Facing::EAST).abs() < 1e-3,
            "{report:?}"
        );
        across.push(body.position.y - CLICK.y);
    }
    across.sort_by(f32::total_cmp);
    for (got, want) in across.iter().zip([-90.0, -30.0, 30.0, 90.0]) {
        assert!((got - want).abs() < 6.0, "{report:?}");
    }
}

#[test]
fn a_recorded_formation_slot_does_not_turn_with_the_heading() {
    let slots = |click: Vec2| {
        let (simulation, ids, units) = moved_group_to(None, 700, "", click);
        let units = units
            .iter()
            .filter_map(|unit| simulation.state().get(ids.formation_slot, *unit))
            .map(|slot| (slot.0.x, slot.0.y))
            .collect::<Vec<_>>();
        let mut set = units;
        set.sort_by(|a, b| a.partial_cmp(b).unwrap());
        set
    };
    let east = slots(CLICK);
    let north = slots(Vec2::new(200.0, 700.0));
    assert_eq!(east.len(), 4, "{east:?}");
    assert_eq!(east, north);
}

#[test]
fn a_narrow_passage_at_the_click_refuses_the_column_formation() {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["Soldier", "Captain"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns: Vec<Spawn> = [
        ("Soldier", 330.0),
        ("Soldier", 365.0),
        ("Captain", 400.0),
        ("Soldier", 435.0),
        ("Soldier", 470.0),
    ]
    .iter()
    .map(|(name, y)| {
        kinds
            .spawn(
                &data.combat,
                kinds.kind(name).unwrap(),
                Vec2::new(200.0, *y),
                Facing::from_radians(core::f32::consts::FRAC_PI_2),
                PLAYER,
            )
            .unwrap()
    })
    .collect();
    let mut passability = Passability::build(&flat_map(128, 128), 5.0 / 128.0).unwrap();
    let start_cell = passability.cell_at(Vec2::new(200.0, 400.0)).unwrap();
    passability
        .grid
        .add_flags(start_cell, mint_sage_passability::NARROW_PASSAGE)
        .unwrap();
    let rules = group_moves(RotwkRules::new(passability, spawns))
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
                units: units.clone(),
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: CLICK.x,
                y: CLICK.y,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..900 {
        simulation.tick().unwrap();
    }
    let bodies: Vec<Vec2> = units
        .iter()
        .map(|u| simulation.state().get(ids.body, *u).unwrap().0.position)
        .collect();
    let report: Vec<String> = bodies
        .iter()
        .map(|p| format!("({:.1}, {:.1})", p.x, p.y))
        .collect();
    let captain = bodies[2];
    assert!(
        captain.minus(CLICK).length() < 6.0,
        "the nearest mover should still walk to the click: {report:?}"
    );
    let column_x = CLICK.x - 55.0;
    for (i, body) in bodies.iter().enumerate().filter(|(i, _)| *i != 2) {
        assert!(
            (body.x - column_x).abs() > 6.0,
            "soldier {i} formed up at the column's shared x {column_x}: {report:?}"
        );
        assert!(
            body.minus(CLICK).length() < 60.0,
            "soldier {i} should end up near the click, not lined up behind it: {report:?}"
        );
    }
}

#[test]
fn a_click_inside_the_groups_own_spread_gathers_it_instead_of_forming_a_column() {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let (kinds, missing) = KindTable::closure(&data, &["Soldier".to_owned()]);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let corners = [
        (280.0, 380.0),
        (320.0, 380.0),
        (280.0, 420.0),
        (320.0, 420.0),
    ];
    let spawns: Vec<Spawn> = corners
        .iter()
        .map(|(x, y)| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind("Soldier").unwrap(),
                    Vec2::new(*x, *y),
                    Facing::EAST,
                    PLAYER,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(128, 128), 5.0 / 128.0).unwrap();
    let rules = group_moves(RotwkRules::new(passability, spawns))
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_teams(vec![None, None]);
    let mut simulation = Simulation::new(6, rules).unwrap();
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
    let click = Vec2::new(300.0, 400.0);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: click.x,
                y: click.y,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..600 {
        simulation.tick().unwrap();
    }
    let bodies: Vec<Vec2> = units
        .iter()
        .map(|u| simulation.state().get(ids.body, *u).unwrap().0.position)
        .collect();
    let report: Vec<String> = bodies
        .iter()
        .map(|p| format!("({:.1}, {:.1})", p.x, p.y))
        .collect();
    let want = [
        Vec2::new(300.0, 400.0),
        Vec2::new(330.0, 400.0),
        Vec2::new(300.0, 430.0),
        Vec2::new(321.21, 421.21),
    ];
    for (got, want) in bodies.iter().zip(want) {
        assert!(got.minus(want).length() < 6.0, "{report:?}, want {want:?}");
    }
}

#[test]
fn only_the_five_kindofmask_fs_bits_make_a_faction_structure() {
    let data_text = "\
Object Forge\n  Side = Men\n  KindOf = STRUCTURE FS_FACTORY\n  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\nEnd\n\
Object Silo\n  Side = Men\n  KindOf = STRUCTURE FS_SUPERWEAPON\n  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\nEnd\n";
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        data_text,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["Forge", "Silo"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns: Vec<Spawn> = ["Forge", "Silo"]
        .iter()
        .map(|name| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    Vec2::new(200.0, 200.0),
                    Facing::EAST,
                    PLAYER,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(128, 128), 5.0 / 128.0).unwrap();
    let rules = group_moves(RotwkRules::new(passability, spawns))
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    let faction = |entity| {
        simulation
            .rules()
            .faction_structure(simulation.state(), ids, entity)
    };
    assert!(faction(units[0]));
    assert!(!faction(units[1]));
}

#[test]
fn a_formation_member_off_the_map_walks_straight_to_a_goal_on_it() {
    for (facing, arc) in [
        (core::f32::consts::FRAC_PI_2, false),
        (-core::f32::consts::FRAC_PI_2, true),
    ] {
        for (unit, path) in off_map_routes(facing) {
            assert!(path
                .route
                .points
                .first()
                .is_some_and(|first| first.minus(unit).length() < 10.0));
            assert_eq!(path.route.points.last(), Some(&path.goal));
            assert!(path.goal.y > 200.0, "stopped short at {:?}", path.goal);
            assert_eq!(path.route.points.len() > 2, arc, "{:?}", path.route.points);
        }
    }
}

fn off_map_routes(facing: f32) -> Vec<(Vec2, Path)> {
    let data_text = DATA.replace(
        "Locomotor Feet\n  Surfaces = GROUND\n",
        "Locomotor Feet\n  Surfaces = GROUND\n  SlowTurnRadius = 20\n",
    );
    let doc = mint_sage_ini::Document::parse(
        "t.ini",
        &data_text,
        &mint_sage_ini::BlockTable::corpus_path(),
    )
    .unwrap();
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["Soldier", "Captain"].map(str::to_owned);
    let (kinds, _) = KindTable::closure(&data, &names);
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns: Vec<Spawn> = [130.0, 165.0, 200.0, 235.0, 270.0]
        .iter()
        .map(|x| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind("Soldier").unwrap(),
                    Vec2::new(*x, -70.0),
                    Facing::from_radians(facing),
                    PLAYER,
                )
                .unwrap()
        })
        .collect();
    let mut map = flat_map(61, 61);
    map.border_size = 10;
    map.boundaries = vec![mint_sage_map::Boundary { x: 39, y: 39 }];
    let mut passability = Passability::builder(&map, 5.0 / 128.0)
        .playable_border(true)
        .build()
        .unwrap();
    for x in 0..50u16 {
        if let Some(cell) = passability.cell_at(Vec2::new(5.0 + 6.5 * f32::from(x), 150.0)) {
            passability
                .grid
                .add_flags(cell, mint_sage_passability::CLIFF)
                .unwrap();
        }
    }
    let rules = group_moves(RotwkRules::new(passability, spawns))
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_teams(vec![None, None])
        .with_players(2);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let units = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: units.clone(),
        },
        RotwkCommand::MoveTo {
            x: 200.0,
            y: 300.0,
            queue: false,
        },
    ] {
        simulation.submit(PLAYER, command).unwrap();
    }
    simulation.tick().unwrap();
    units
        .iter()
        .map(|unit| {
            let at = simulation.state().get(ids.body, *unit).unwrap().0.position;
            (at, simulation.state().get(ids.path, *unit).unwrap().clone())
        })
        .collect()
}
