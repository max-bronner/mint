use super::*;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Locomotor Stuck\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\n  \
  Speed = 0\nEnd\n\
Weapon Bow\n  AttackRange = 100\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 1\n    DamageType = PIERCE\n  End\nEnd\n\
Object Hunter\n  Side = Men\n  KindOf = INFANTRY SELECTABLE ATTACK_NEEDS_LINE_OF_SIGHT\n  \
  VisionRange = 400\n  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Bow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 30\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Thrower\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  VisionRange = 400\n  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Bow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 30\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Sentinel\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  VisionRange = 400\n  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Bow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n    \
    StandGround = Yes\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 30\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Target\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 10\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 30\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ChildObject Stump Hunter\n  \
  LocomotorSet\n    Locomotor = Stuck\n    Condition = SET_NORMAL\n    Speed = 0\n  End\nEnd\n";

#[allow(clippy::reversed_empty_ranges)]
fn simulation(
    hunter_template: &str,
    walled: bool,
) -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let names = ["Hunter", "Thrower", "Sentinel", "Target", "Stump"].map(str::to_owned);
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
        spawn(hunter_template, Vec2::new(100.0, 300.0), PLAYER),
        spawn("Target", Vec2::new(700.0, 300.0), OTHER),
    ];
    let map = if walled {
        wall_map(96, 96, 48, 1..0)
    } else {
        flat_map(96, 96)
    };
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
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

fn order_attack(simulation: &mut Simulation<RotwkRules>, hunter: Entity, target: Entity) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![hunter],
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target })
        .unwrap();
}

fn gave_up_at(
    simulation: &mut Simulation<RotwkRules>,
    ids: Ids,
    hunter: Entity,
    ticks: u32,
) -> Option<u64> {
    for _ in 0..ticks {
        simulation.tick().unwrap();
        if !simulation.state().contains(ids.attack_order, hunter) {
            return Some(simulation.frame().get());
        }
    }
    None
}

#[test]
fn a_walled_off_los_attacker_waits_then_gives_up_its_order() {
    let (mut sim, ids, hunter, target) = simulation("Hunter", true);
    order_attack(&mut sim, hunter, target);
    let at =
        gave_up_at(&mut sim, ids, hunter, 1500).expect("the order never ends behind a sealed wall");
    assert!(at > 100, "gave up too soon, at frame {at}");
    assert!(at < 1500, "gave up too late, at frame {at}");

    let (mut open, open_ids, open_hunter, open_target) = simulation("Hunter", false);
    order_attack(&mut open, open_hunter, open_target);
    assert!(
        gave_up_at(&mut open, open_ids, open_hunter, 400).is_none(),
        "an unwalled attacker should still be closing, not given up"
    );
}

#[test]
fn a_walled_off_non_los_attacker_also_waits_then_gives_up_its_order() {
    let (mut sim, ids, thrower, target) = simulation("Thrower", true);
    order_attack(&mut sim, thrower, target);
    let at = gave_up_at(&mut sim, ids, thrower, 400)
        .expect("a non-LOS attacker behind a sealed wall never gives up its order");
    assert!(at > 100, "gave up too soon, at frame {at}");
    assert!(at < 400, "gave up too late, at frame {at}");
}

#[test]
fn a_stand_ground_attacker_on_a_forced_wall_layer_waits_the_shorter_deadline() {
    force_victim_on_wall_layer(true);
    let (mut sim, ids, sentinel, target) = simulation("Sentinel", true);
    order_attack(&mut sim, sentinel, target);
    let at = gave_up_at(&mut sim, ids, sentinel, 200)
        .expect("a StandGround attacker on a forced wall layer never gives up its order");
    assert!(at < 150, "gave up too late, at frame {at}");
}

#[test]
fn an_attacker_that_cannot_move_gives_up_after_the_shorter_wait() {
    let (mut sim, ids, hunter, target) = simulation("Stump", false);
    order_attack(&mut sim, hunter, target);
    let at = gave_up_at(&mut sim, ids, hunter, 200)
        .expect("a motionless attacker never gives up its order");
    assert!(at < 150, "gave up too late, at frame {at}");
}
