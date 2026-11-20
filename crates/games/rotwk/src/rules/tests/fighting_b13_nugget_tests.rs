use super::*;

fn scene(
    data: &str,
    roots: &[&str],
    units: &[(&str, Vec2, PlayerId)],
) -> (Simulation<RotwkRules>, Arc<KindTable>, Ids) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", data, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots: Vec<String> = roots.iter().map(|name| (*name).to_owned()).collect();
    let (kinds, missing) = KindTable::closure(&data, &roots);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let spawns: Vec<Spawn> = units
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
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(data.combat.clone())
        .with_gravity(data.gravity)
        .with_construction(Arc::clone(&kinds), production)
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            vec!["Creeps".to_owned()],
            vec![crate::alliance::MapRelations {
                universal_hostile: true,
                ..crate::alliance::MapRelations::default()
            }],
        );
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    (simulation, kinds, ids)
}

fn of_kind(
    kinds: &KindTable,
    simulation: &Simulation<RotwkRules>,
    ids: Ids,
    name: &str,
) -> Vec<Entity> {
    let kind = kinds.kind(name).unwrap();
    simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(entity, k)| **k == kind && simulation.state().is_live(*entity))
        .map(|(entity, _)| entity)
        .collect()
}

fn health_lost(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    let health = simulation.state().get(ids.health, entity).unwrap().0;
    health.max() - health.current()
}

fn select(simulation: &mut Simulation<RotwkRules>, player: PlayerId, unit: Entity) {
    simulation
        .submit(
            player,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![unit],
            },
        )
        .unwrap();
}

mod slave_attack {
    use super::*;

    const DATA: &str = "\
Upgrade Upgrade_Call\n  Type = OBJECT\n  BuildCost = 0\n  BuildTime = 0\nEnd\n\
Weapon CallSlaves\n  AttackRange = 200\n  DelayBetweenShots = 4000\n  \
  SlaveAttackNugget\n  End\nEnd\n\
Weapon CallAtPlace\n  AttackRange = 200\n  DelayBetweenShots = 4000\n  NoVictimNeeded = Yes\n  \
  SlaveAttackNugget\n  End\nEnd\n\
Weapon Spit\n  AttackRange = 200\n  DelayBetweenShots = 200\n  NoVictimNeeded = Yes\n  \
  DamageNugget\n    Damage = 1\n    DamageType = PIERCE\n  End\nEnd\n\
Object Caller\n  Side = Wild\n  KindOf = SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY CallSlaves\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    MoodAttackCheckRate = 100\n  End\n  \
  Behavior = GrantUpgradeCreate Tag_Grant\n    UpgradeToGrant = Upgrade_Call\n  End\n  \
  Behavior = SlaveWatcherBehavior Tag_Watch\n  End\n  \
  Behavior = ObjectCreationUpgrade Tag_One\n    TriggeredBy = Upgrade_Call\n    \
    ThingToSpawn = SpitterOne\n    Offset = X:-10.0 Y:10.0 Z:0.0\n  End\n  \
  Behavior = ObjectCreationUpgrade Tag_Two\n    TriggeredBy = Upgrade_Call\n    \
    ThingToSpawn = SpitterTwo\n    Offset = X:-10.0 Y:-10.0 Z:0.0\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 20\nEnd\n\
ChildObject PlaceCaller Caller\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY CallAtPlace\n  End\nEnd\n\
Object SpitterOne\n  Side = Wild\n  KindOf = SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Spit\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ChildObject SpitterTwo SpitterOne\nEnd\n\
Object Dummy\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 4000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

    const ROOTS: [&str; 5] = ["Caller", "PlaceCaller", "SpitterOne", "SpitterTwo", "Dummy"];
    const CALLER_AT: Vec2 = Vec2::new(300.0, 300.0);
    const DUMMY_AT: Vec2 = Vec2::new(340.0, 300.0);

    struct Setup {
        simulation: Simulation<RotwkRules>,
        ids: Ids,
        caller: Entity,
        one: Entity,
        two: Entity,
        dummies: Vec<Entity>,
    }

    fn setup(caller: &str, dummies: &[Vec2]) -> Setup {
        let mut units = vec![(caller, CALLER_AT, OTHER)];
        units.extend(dummies.iter().map(|at| ("Dummy", *at, PLAYER)));
        let (mut simulation, kinds, ids) = scene(DATA, &ROOTS, &units);
        simulation.tick().unwrap();
        Setup {
            caller: of_kind(&kinds, &simulation, ids, caller)[0],
            one: of_kind(&kinds, &simulation, ids, "SpitterOne")[0],
            two: of_kind(&kinds, &simulation, ids, "SpitterTwo")[0],
            dummies: of_kind(&kinds, &simulation, ids, "Dummy"),
            simulation,
            ids,
        }
    }

    fn attackers(setup: &mut Setup, frames: u32) -> (bool, bool) {
        let (mut one, mut two) = (false, false);
        for _ in 0..frames {
            setup.simulation.tick().unwrap();
            let state = setup.simulation.state();
            let busy = |slave| {
                state.contains(setup.ids.attack_order, slave)
                    || state
                        .get(setup.ids.armed, slave)
                        .is_some_and(|armed| armed.target.is_some())
            };
            one |= busy(setup.one);
            two |= busy(setup.two);
        }
        (one, two)
    }

    #[test]
    fn a_slave_attack_orders_only_the_last_registered_slave_for_one_shot() {
        let mut setup = setup("Caller", &[DUMMY_AT]);
        let (one, two) = attackers(&mut setup, 90);
        assert!(!one, "the first slave was ordered");
        assert!(two, "the watched slave was never ordered");
        assert_eq!(
            health_lost(&setup.simulation, setup.ids, setup.dummies[0]),
            1.0
        );
    }

    #[test]
    fn a_slave_already_attacking_is_not_given_the_masters_goal() {
        let far = Vec2::new(300.0, 420.0);
        let mut setup = setup("Caller", &[DUMMY_AT, far]);
        let (near, far) = (setup.dummies[0], setup.dummies[1]);
        select(&mut setup.simulation, OTHER, setup.two);
        setup
            .simulation
            .submit(OTHER, RotwkCommand::AttackObject { target: far })
            .unwrap();
        for _ in 0..90 {
            setup.simulation.tick().unwrap();
        }
        assert!(
            health_lost(&setup.simulation, setup.ids, far) > 1.0,
            "the slave never attacked its own target"
        );
        assert_eq!(health_lost(&setup.simulation, setup.ids, near), 0.0);
    }

    #[test]
    fn a_slave_attack_at_a_place_sends_the_slave_to_the_place() {
        let mut setup = setup("PlaceCaller", &[]);
        let place = Vec2::new(360.0, 300.0);
        select(&mut setup.simulation, OTHER, setup.caller);
        setup
            .simulation
            .submit(
                OTHER,
                RotwkCommand::AttackGround {
                    slot: 0,
                    x: place.x,
                    y: place.y,
                },
            )
            .unwrap();
        let mut sent = false;
        for _ in 0..90 {
            setup.simulation.tick().unwrap();
            sent |= setup
                .simulation
                .state()
                .get(setup.ids.ground_order, setup.two)
                .is_some_and(|order| order.at == place);
        }
        assert!(sent, "the watched slave was never sent at the place");
    }
}
