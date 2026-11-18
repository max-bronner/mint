use super::*;
use crate::rules::engaged::Engaged;

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon Sling\n  AttackRange = 200\n  DelayBetweenShots = 3000\n  \
  DamageNugget\n    Damage = 1\n    DamageType = CRUSH\n  End\nEnd\n\
Object Slinger\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 300\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Sling\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 100\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 30\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Post\n  Side = Wild\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 10\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

const DEFAULTS: EngagedDefaults = EngagedDefaults {
    max_distance: 33.0,
    timeout_ms: 1000.0,
};

fn simulation(defaults: EngagedDefaults) -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
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
    let spawns: Vec<Spawn> = [
        ("Slinger", 300.0, PLAYER),
        ("Post", 400.0, OTHER),
        ("Post", 1000.0, OTHER),
    ]
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
    let passability = Passability::build(&flat_map(128, 128), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_teams(vec![None, None])
        .with_engaged_defaults(defaults);
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
    let far = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .iter()
        .find(|(_, body)| body.0.position.x > 900.0)
        .map(|(entity, _)| entity)
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
    (simulation, ids, slinger, far)
}

fn engaged(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> bool {
    simulation.state().contains(ids.engaged, entity)
}

fn first_shot(simulation: &mut Simulation<RotwkRules>, ids: Ids, slinger: Entity) -> u64 {
    for _ in 0..200 {
        simulation.tick().unwrap();
        if let Some(shot) = simulation
            .state()
            .get(ids.armed, slinger)
            .and_then(|armed| armed.state.last_shot())
        {
            return shot.get();
        }
    }
    panic!("the slinger never fired");
}

#[test]
fn a_weapon_coming_into_use_engages_the_object() {
    let (mut simulation, ids, slinger, _) = simulation(DEFAULTS);
    simulation.tick().unwrap();
    assert!(
        !engaged(&simulation, ids, slinger),
        "engaged before any attack"
    );
    first_shot(&mut simulation, ids, slinger);
    assert!(engaged(&simulation, ids, slinger), "not engaged on firing");
}

#[test]
fn the_flag_outlasts_the_attack_by_the_timeout_then_clears() {
    let cleared_after = |frames_attacking: u32| {
        let (mut simulation, ids, slinger, _) = simulation(DEFAULTS);
        first_shot(&mut simulation, ids, slinger);
        for _ in 0..frames_attacking {
            simulation.tick().unwrap();
            assert!(
                engaged(&simulation, ids, slinger),
                "dropped while still attacking"
            );
        }
        simulation
            .submit(
                PLAYER,
                RotwkCommand::MoveTo {
                    x: 100.0,
                    y: 300.0,
                    queue: false,
                },
            )
            .unwrap();
        let ordered = simulation.frame().get();
        for _ in 0..400 {
            simulation.tick().unwrap();
            if !engaged(&simulation, ids, slinger) {
                return simulation.frame().get() - ordered;
            }
        }
        panic!("the flag never cleared");
    };
    assert_eq!(
        cleared_after(0),
        1,
        "a flag set this frame has no deadline to outlast"
    );
    let timeout = u64::from(5 * (SIMULATION_HZ / crate::game_data::RETAIL_LOGIC_HZ));
    let held = cleared_after(10);
    assert_eq!(held, timeout, "cleared {held} ticks after the order");
}

#[test]
fn chasing_a_target_beyond_the_distance_keeps_the_flag() {
    let held_through_chase = |defaults| {
        let (mut simulation, ids, slinger, far) = simulation(defaults);
        first_shot(&mut simulation, ids, slinger);
        simulation
            .submit(PLAYER, RotwkCommand::AttackObject { target: far })
            .unwrap();
        (0..240).all(|_| {
            simulation.tick().unwrap();
            engaged(&simulation, ids, slinger)
        })
    };
    assert!(
        held_through_chase(DEFAULTS),
        "dropped while chasing beyond 33"
    );
    assert!(
        !held_through_chase(EngagedDefaults {
            max_distance: 10_000.0,
            timeout_ms: 1000.0,
        }),
        "held with every target nearer than the distance: the chase is not what held it"
    );
}

#[test]
fn the_flag_is_hashed_state() {
    let (mut simulation, ids, slinger, _) = simulation(DEFAULTS);
    first_shot(&mut simulation, ids, slinger);
    let held = *simulation.state().get(ids.engaged, slinger).unwrap();
    let hash = |engaged: Engaged| {
        let mut hasher = StateHasher::new();
        engaged.hash_into(&mut hasher);
        hasher.finish()
    };
    assert_ne!(hash(held), hash(held.later()), "the deadline is not hashed");
}
