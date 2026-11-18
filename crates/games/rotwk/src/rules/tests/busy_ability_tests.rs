use super::*;

const DATA: &str = "\
SpecialPower SpecialAbilityFreeze\n  ReloadTime = 30000\nEnd\n\
Weapon Sword\n  AttackRange = 20\n  DelayBetweenShots = 1000\n  MeleeWeapon = Yes\n  \
  DamageNugget\n    Damage = 10\n    DamageType = SLASH\n  End\nEnd\n\
CommandSet CasterSet\n  1 = Fire_Freeze\nEnd\n\
CommandButton Fire_Freeze\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityFreeze\nEnd\n\
Object Caster\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  CommandSet = CasterSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Sword\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    \
    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\n  \
  Behavior = SpecialAbilityUpdate Tag_Ability\n    SpecialPowerTemplate = SpecialAbilityFreeze\n    \
    UnpackTime = 2000\n    PreparationTime = 2000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Enemy\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

struct Match {
    data: GameData,
    kinds: Arc<KindTable>,
}

fn load() -> Match {
    load_from(DATA)
}

fn load_from(data: &str) -> Match {
    let doc =
        mint_sage_ini::Document::parse("t.ini", data, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Caster", "Enemy"].into_iter().map(str::to_owned).collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    Match {
        data,
        kinds: Arc::new(kinds),
    }
}

fn simulation(m: &Match) -> Simulation<RotwkRules> {
    let production = m.kinds.production(&m.data);
    let power_names = crate::powers::PowerNames::build(m.kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&m.data.special_powers);
    let spawns: Vec<Spawn> = [
        ("Caster", 100.0, 100.0, PLAYER),
        ("Enemy", 110.0, 100.0, OTHER),
    ]
    .iter()
    .map(|(name, x, y, owner)| {
        let kind = m.kinds.kind(name).unwrap();
        m.kinds
            .spawn(
                &m.data.combat,
                kind,
                Vec2::new(*x, *y),
                Facing::EAST,
                *owner,
            )
            .unwrap()
    })
    .collect();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_hordes(m.data.formations.tables().to_vec(), Vec::new())
        .with_combat(m.data.combat.clone())
        .with_construction(Arc::clone(&m.kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_upgrades(m.kinds.upgrades(&m.data), Arc::clone(&m.data.upgrade_bits))
        .with_powers(power_names, power_reload);
    Simulation::new(5, rules).unwrap()
}

fn health_fraction(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    simulation
        .state()
        .get(ids.health, entity)
        .map_or(0.0, |health| health.0.fraction())
}

fn only_owned_by(simulation: &Simulation<RotwkRules>, ids: Ids, owner: PlayerId) -> Entity {
    let mut found: Vec<Entity> = simulation
        .state()
        .store(ids.owner)
        .unwrap()
        .iter()
        .filter(|(_, o)| o.0 == owner)
        .map(|(entity, _)| entity)
        .collect();
    found.sort_unstable_by_key(|entity| entity.index());
    found[0]
}

#[test]
fn a_caster_acquires_and_fires_nothing_while_an_ability_runs() {
    let m = load();
    let mut simulation = simulation(&m);
    let ids = simulation.rules().ids().unwrap();
    let caster = only_owned_by(&simulation, ids, PLAYER);
    let enemy = only_owned_by(&simulation, ids, OTHER);
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    let before_press = health_fraction(&simulation, ids, enemy);
    assert!(
        before_press < 1.0,
        "the fixture's premise: the weapon must have already hit the enemy"
    );
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![caster],
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
    for _ in 0..50 {
        simulation.tick().unwrap();
    }
    let during_ability = health_fraction(&simulation, ids, enemy);
    assert_eq!(
        during_ability, before_press,
        "the weapon must not have fired while the ability is unpacking or preparing"
    );
}

fn position(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> Vec2 {
    simulation.state().get(ids.body, entity).unwrap().0.position
}

#[test]
fn must_finish_ability_does_not_refuse_a_move_mid_cast() {
    let data = DATA.replace(
        "PreparationTime = 2000
",
        "PreparationTime = 2000
    MustFinishAbility = Yes
",
    );
    let m = load_from(&data);
    let mut simulation = simulation(&m);
    let ids = simulation.rules().ids().unwrap();
    let caster = only_owned_by(&simulation, ids, PLAYER);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![caster],
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
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    let cast_from = position(&simulation, ids, caster);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 300.0,
                y: 300.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..30 {
        simulation.tick().unwrap();
    }
    assert_ne!(
        position(&simulation, ids, caster),
        cast_from,
        "a move order mid-cast must be carried out, not refused"
    );
}
