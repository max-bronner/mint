use super::*;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 1000\nEnd\n\
Object Brute\n  Side = Men\n  KindOf = MONSTER PATH_THROUGH_INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 12\n  GeometryHeight = 30\nEnd\n\
Object Walker\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Wide\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 15\n  GeometryHeight = 16\nEnd\n\
Object Slab\n  Side = Men\n  KindOf = VEHICLE LARGE_RECTANGLE_PATHFIND SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Stander\n  Side = Men\n  KindOf = INFANTRY PATH_THROUGH_EACH_OTHER SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
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
    let counts = crate::obstacles::counts_of(simulation.state(), ids.obstacles).to_vec();
    let stamped = move |(x, y): (i32, i32)| {
        let cell = Cell::new(u16::try_from(x).unwrap(), u16::try_from(y).unwrap());
        let index = usize::try_from(extent.index(cell).unwrap()).unwrap();
        counts.get(index).is_some_and(|count| *count > 0)
    };
    (simulation.state().get(ids.path, units[1]).cloned(), stamped)
}

#[test]
fn a_path_through_infantry_mover_sends_an_idle_ally_aside_clear_of_its_route() {
    let path = walker_route_after_the_brute_is_ordered("Brute").expect("the walker was not moved");
    assert!((path.goal.y - 400.0).abs() > 17.0, "{:?}", path.goal);
    assert!((path.goal.x - 400.0).abs() < 10.0, "{:?}", path.goal);
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
    let ally = spawn_at(300.0, 168.0, KindOf::SELECTABLE, PLAYER);
    let rules = RotwkRules::new(passability, vec![ally]).with_hordes(vec![formation], vec![spawn]);
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
