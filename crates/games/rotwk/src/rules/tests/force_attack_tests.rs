use super::*;

const THIRD: PlayerId = PlayerId::new(2);

const DATA: &str = "\
Locomotor Feet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Weapon Sword\n  AttackRange = 50\n  DelayBetweenShots = 150\n  \
  DamageNugget\n    Damage = 5\n    DamageType = SLASH\n  End\nEnd\n\
SpecialPower SpecialAbilityDominate\n  ReloadTime = 0\nEnd\n\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 10000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Sword\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Dominator\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  VisionRange = 400\n  \
  CommandSet = DominatorSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  LocomotorSet\n    Locomotor = Feet\n    Condition = SET_NORMAL\n    Speed = 60\n  End\n  \
  Behavior = DominateEnemySpecialPower Tag_Dom\n    SpecialPowerTemplate = SpecialAbilityDominate\n    \
    DominateRadius = 60\n    AttributeModifierAffects = ALL ENEMIES\n    PermanentlyConvert = Yes\n    \
    IgnoreFacingCheck = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet DominatorSet\n  1 = Cast_Dominate\nEnd\n\
CommandButton Cast_Dominate\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityDominate\n  \
  Options = NEED_TARGET_ENEMY_OBJECT\nEnd\n";

struct Match {
    data: GameData,
    kinds: Arc<KindTable>,
}

fn load() -> Match {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = ["Soldier", "Dominator"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    Match {
        data,
        kinds: Arc::new(kinds),
    }
}

fn simulation(
    m: &Match,
    placed: &[(&str, f32, f32, PlayerId)],
    teams: Option<Vec<Option<u8>>>,
) -> Simulation<RotwkRules> {
    let production = m.kinds.production(&m.data);
    let power_names = crate::powers::PowerNames::build(m.kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&m.data.special_powers);
    let cast_rules = power_names.cast_rules(&m.data.special_powers);
    let spawns: Vec<Spawn> = placed
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
    let map = flat_map(160, 160);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let mut rules = RotwkRules::new(passability, spawns)
        .with_players(3)
        .with_combat(m.data.combat.clone())
        .with_construction(Arc::clone(&m.kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_powers(power_names, power_reload)
        .with_power_cast_rules(cast_rules);
    if let Some(teams) = teams {
        rules = rules.with_teams(teams);
    }
    Simulation::new(5, rules).unwrap()
}

fn of_kind_owned(
    m: &Match,
    simulation: &Simulation<RotwkRules>,
    ids: Ids,
    name: &str,
    owner: PlayerId,
) -> Entity {
    let kind = m.kinds.kind(name).unwrap();
    simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .find(|(entity, k)| {
            **k == kind
                && simulation.state().is_live(*entity)
                && simulation
                    .state()
                    .get(ids.owner, *entity)
                    .is_some_and(|o| o.0 == owner)
        })
        .map(|(entity, _)| entity)
        .unwrap()
}

fn select_and_submit(simulation: &mut Simulation<RotwkRules>, unit: Entity, command: RotwkCommand) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![unit],
            },
        )
        .unwrap();
    simulation.submit(PLAYER, command).unwrap();
}

fn health_of(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    simulation
        .state()
        .get(ids.health, entity)
        .map_or(0.0, |health| health.0.current())
}

#[test]
fn force_attack_takes_an_ally_and_keeps_firing_every_cycle() {
    let m = load();
    let mut simulation = simulation(
        &m,
        &[
            ("Soldier", 200.0, 200.0, PLAYER),
            ("Soldier", 230.0, 200.0, OTHER),
        ],
        Some(vec![Some(0), Some(0)]),
    );
    let ids = simulation.rules().ids().unwrap();
    let attacker = of_kind_owned(&m, &simulation, ids, "Soldier", PLAYER);
    let ally = of_kind_owned(&m, &simulation, ids, "Soldier", OTHER);

    select_and_submit(
        &mut simulation,
        attacker,
        RotwkCommand::AttackObject { target: ally },
    );
    simulation.tick().unwrap();
    assert!(
        !simulation.state().contains(ids.attack_order, attacker),
        "the fixture's premise: a plain order refuses an ally"
    );

    select_and_submit(
        &mut simulation,
        attacker,
        RotwkCommand::ForceAttackObject { target: ally },
    );
    let start = health_of(&simulation, ids, ally);
    for _ in 0..600 {
        simulation.tick().unwrap();
    }
    let lost = start - health_of(&simulation, ids, ally);
    assert!(
        lost > 15.0,
        "force-attack must keep damaging the ally every weapon cycle, lost {lost}"
    );
}

#[test]
fn force_attack_ends_when_the_victim_changes_to_a_non_enemy_owner() {
    let m = load();
    let mut simulation = simulation(
        &m,
        &[
            ("Soldier", 200.0, 200.0, PLAYER),
            ("Soldier", 230.0, 200.0, OTHER),
            ("Dominator", 230.0, 230.0, THIRD),
        ],
        Some(vec![Some(0), Some(1), Some(0)]),
    );
    let ids = simulation.rules().ids().unwrap();
    let attacker = of_kind_owned(&m, &simulation, ids, "Soldier", PLAYER);
    let victim = of_kind_owned(&m, &simulation, ids, "Soldier", OTHER);
    let dominator = of_kind_owned(&m, &simulation, ids, "Dominator", THIRD);

    select_and_submit(
        &mut simulation,
        attacker,
        RotwkCommand::ForceAttackObject { target: victim },
    );
    simulation.tick().unwrap();
    assert!(
        simulation.state().contains(ids.attack_order, attacker),
        "the fixture's premise: the forced order took"
    );

    simulation
        .submit(
            THIRD,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![dominator],
            },
        )
        .unwrap();
    simulation
        .submit(
            THIRD,
            RotwkCommand::FireSpecialPower {
                button: 0,
                target: Some(victim),
                at: None,
            },
        )
        .unwrap();
    let mut changed_at = None;
    for frame in 0..300 {
        simulation.tick().unwrap();
        if simulation.state().get(ids.owner, victim).map(|o| o.0) == Some(THIRD) {
            changed_at = Some(frame);
            break;
        }
    }
    changed_at.expect("the fixture's premise: the dominate must take the victim");
    simulation.tick().unwrap();
    assert!(
        !simulation.state().contains(ids.attack_order, attacker),
        "a forced order must end once its victim is no longer attacker-hostile"
    );
}
