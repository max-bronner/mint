use super::*;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 1000\n  \
  FormationPriority = MELEE1\nEnd\n\
Locomotor HeroFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 1000\n  \
  FormationPriority = NO_FORMATION\nEnd\n\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  FormationWidth = 1\n  FormationDepth = 1\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Captain\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  LocomotorSet\n    Locomotor = HeroFeet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

const CLICK: Vec2 = Vec2::new(500.0, 400.0);

fn moved_group() -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
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
    for _ in 0..600 {
        simulation.tick().unwrap();
    }
    (simulation, ids, units)
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
    let rules = RotwkRules::new(passability, spawns)
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
