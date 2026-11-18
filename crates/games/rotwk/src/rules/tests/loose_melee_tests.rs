use super::*;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 200\nEnd\n\
Weapon Blade\n  AttackRange = 10\n  DelayBetweenShots = 2000\n  FiringDuration = 1000\n  \
  MeleeWeapon = Yes\n  AcceptableAimDelta = 180\n  \
  DamageNugget\n    Damage = 5\n    DamageType = SLASH\n  End\nEnd\n\
Object Swordsman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 300\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Blade\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 5000\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Post\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ChildObject Runner Post\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 30\n  End\nEnd\n\
Object Straw\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

fn start(spawns: &[(&str, f32, f32, PlayerId)]) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["Swordsman", "Post", "Runner", "Straw"].map(str::to_owned);
    let (kinds, missing) = KindTable::closure(&data, &names);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns: Vec<Spawn> = spawns
        .iter()
        .map(|(name, x, y, owner)| {
            kinds
                .spawn(
                    &data.combat,
                    kinds.kind(name).unwrap(),
                    Vec2::new(*x, *y),
                    Facing::EAST,
                    *owner,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_combat(data.combat.clone())
        .with_guard_ranges(data.guard_ranges)
        .with_construction(Arc::clone(&kinds), production)
        .with_teams(vec![None, None]);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let mut entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    entities.sort_unstable_by_key(|entity| entity.index());
    (simulation, ids, entities)
}

fn attack(
    simulation: &mut Simulation<RotwkRules>,
    player: PlayerId,
    units: &[Entity],
    target: Entity,
) {
    simulation
        .submit(
            player,
            RotwkCommand::SetSelection {
                replace: true,
                units: units.to_vec(),
            },
        )
        .unwrap();
    simulation
        .submit(player, RotwkCommand::AttackObject { target })
        .unwrap();
}

fn position(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> Vec2 {
    simulation.state().get(ids.body, entity).unwrap().0.position
}

fn health(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    simulation
        .state()
        .get(ids.health, entity)
        .unwrap()
        .0
        .current()
}

#[test]
fn loose_melee_attackers_round_one_victim_stand_apart() {
    let mut spawns = vec![("Post", 400.0, 300.0, OTHER)];
    for i in 0..6u8 {
        spawns.push(("Swordsman", 200.0, 270.0 + 12.0 * f32::from(i), PLAYER));
    }
    let (mut simulation, ids, entities) = start(&spawns);
    let post = entities[0];
    let swordsmen = &entities[1..];
    attack(&mut simulation, PLAYER, swordsmen, post);
    for _ in 0..600 {
        simulation.tick().unwrap();
    }
    assert!(health(&simulation, ids, post) < 100_000.0, "nobody struck");
    let at: Vec<Vec2> = swordsmen
        .iter()
        .map(|s| position(&simulation, ids, *s))
        .collect();
    #[allow(clippy::cast_possible_truncation)]
    let mut cells: Vec<(i32, i32)> = at
        .iter()
        .map(|p| ((p.x / 10.0).floor() as i32, (p.y / 10.0).floor() as i32))
        .collect();
    cells.sort_unstable();
    cells.dedup();
    assert_eq!(cells.len(), at.len(), "two attackers share a cell: {at:?}");
}

#[test]
fn a_swing_holds_its_attacker_through_firing_duration() {
    let (mut simulation, ids, entities) = start(&[
        ("Swordsman", 300.0, 300.0, PLAYER),
        ("Post", 312.0, 300.0, OTHER),
    ]);
    let (swordsman, post) = (entities[0], entities[1]);
    attack(&mut simulation, PLAYER, &[swordsman], post);
    let mut struck_at = None;
    for tick in 0..120u32 {
        simulation.tick().unwrap();
        if health(&simulation, ids, post) < 100_000.0 {
            struck_at = Some(tick);
            break;
        }
    }
    assert!(struck_at.is_some(), "never swung");
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![post],
            },
        )
        .unwrap();
    simulation
        .submit(
            OTHER,
            RotwkCommand::MoveTo {
                x: 500.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    let stood = position(&simulation, ids, swordsman);
    for _ in 0..25 {
        simulation.tick().unwrap();
    }
    assert!(
        position(&simulation, ids, swordsman).minus(stood).length() < 0.5,
        "chased during the swing"
    );
    for _ in 0..60 {
        simulation.tick().unwrap();
    }
    assert!(
        position(&simulation, ids, swordsman).minus(stood).length() > 5.0,
        "never chased after the swing"
    );
}

#[test]
fn a_melee_kill_re_acquires_within_two_retail_frames() {
    let (mut simulation, ids, entities) = start(&[
        ("Swordsman", 300.0, 300.0, PLAYER),
        ("Straw", 312.0, 300.0, OTHER),
        ("Post", 300.0, 360.0, OTHER),
    ]);
    let (swordsman, straw, post) = (entities[0], entities[1], entities[2]);
    let mut killed = None;
    for tick in 0..300u32 {
        simulation.tick().unwrap();
        if health(&simulation, ids, straw) <= 0.0 {
            killed = Some(tick);
            break;
        }
    }
    assert!(killed.is_some(), "never killed");
    let frames = mint_sim::SIMULATION_HZ / crate::game_data::RETAIL_LOGIC_HZ;
    for _ in 0..2 * frames + 1 {
        simulation.tick().unwrap();
    }
    assert_eq!(
        simulation.state().get(ids.armed, swordsman).unwrap().target,
        Some(post)
    );
}

#[test]
fn a_melee_kill_does_not_re_acquire_on_the_same_tick() {
    let (mut simulation, ids, entities) = start(&[
        ("Swordsman", 300.0, 300.0, PLAYER),
        ("Straw", 312.0, 300.0, OTHER),
        ("Post", 300.0, 360.0, OTHER),
    ]);
    let (swordsman, straw, post) = (entities[0], entities[1], entities[2]);
    let mut killed_at = None;
    for tick in 0..300u32 {
        simulation.tick().unwrap();
        if health(&simulation, ids, straw) <= 0.0 {
            killed_at = Some(tick);
            break;
        }
    }
    killed_at.expect("never killed");
    simulation.tick().unwrap();
    assert_ne!(
        simulation.state().get(ids.armed, swordsman).unwrap().target,
        Some(post),
        "re-acquired the very next tick after the kill, instead of waiting out the delay"
    );
}

#[test]
fn a_fleeing_victim_is_struck_on_the_move() {
    let (mut simulation, ids, entities) = start(&[
        ("Swordsman", 300.0, 300.0, PLAYER),
        ("Runner", 340.0, 300.0, OTHER),
    ]);
    let (swordsman, runner) = (entities[0], entities[1]);
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![runner],
            },
        )
        .unwrap();
    simulation
        .submit(
            OTHER,
            RotwkCommand::MoveTo {
                x: 900.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    attack(&mut simulation, PLAYER, &[swordsman], runner);
    for _ in 0..300 {
        simulation.tick().unwrap();
        if health(&simulation, ids, runner) < 100_000.0 {
            let speed = simulation.state().get(ids.body, swordsman).unwrap().0.speed;
            assert!(speed > 0.0, "stopped to strike");
            return;
        }
    }
    panic!("never struck");
}
