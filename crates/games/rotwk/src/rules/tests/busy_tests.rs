use super::*;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon Sling\n  AttackRange = 200\n  DelayBetweenShots = 3000\n  HoldAfterFiringDelay = 2000\n  \
  DamageNugget\n    Damage = 1\n    DamageType = CRUSH\n  End\nEnd\n\
Object Slinger\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 300\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Sling\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 100\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Post\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 10\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn simulation() -> (Simulation<RotwkRules>, Ids, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["Slinger", "Post"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns: Vec<Spawn> = [("Slinger", 300.0, PLAYER), ("Post", 400.0, OTHER)]
        .into_iter()
        .map(|(name, x, owner)| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    Vec2::new(x, 300.0),
                    Facing::EAST,
                    owner,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_teams(vec![None, None]);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let slinger = simulation
        .state()
        .store(ids.armed)
        .unwrap()
        .entities()
        .iter()
        .copied()
        .find(|entity| simulation.state().get(ids.owner, *entity).unwrap().0 == PLAYER)
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![slinger],
            },
        )
        .unwrap();
    (simulation, ids, slinger)
}

fn ordered_goal(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> Option<Vec2> {
    simulation
        .state()
        .get(ids.path, entity)
        .filter(|path| path.kind == PathKind::Ordered)
        .map(|path| path.goal)
}

#[test]
fn a_busy_unit_holds_the_newest_order_until_its_hold_ends() {
    let (mut simulation, ids, slinger) = simulation();
    let mut shot = None;
    for _ in 0..120 {
        simulation.tick().unwrap();
        shot = simulation
            .state()
            .get(ids.armed, slinger)
            .and_then(|armed| armed.state.last_shot());
        if shot.is_some() {
            break;
        }
    }
    let shot = shot.expect("the slinger never fired").get();
    for x in [100.0, 150.0] {
        simulation
            .submit(
                PLAYER,
                RotwkCommand::MoveTo {
                    x,
                    y: 300.0,
                    queue: false,
                },
            )
            .unwrap();
        simulation.tick().unwrap();
    }
    let mut set_off = None;
    for _ in 0..120 {
        if let Some(goal) = ordered_goal(&simulation, ids, slinger) {
            set_off = Some((simulation.frame().get(), goal));
            break;
        }
        simulation.tick().unwrap();
    }
    let (frame, goal) = set_off.expect("the held order never ran");
    assert!(frame >= shot + 60, "set off at {frame}, shot at {shot}");
    assert!((goal.x - 150.0).abs() < 1.0, "went for {goal:?}");
}
