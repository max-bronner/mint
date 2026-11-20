use super::*;

const DATA: &str = "\
SpecialPower CloakPower\n  ReloadTime = 1000\nEnd\n\
CommandSet CasterSet\n  1 = Fire_Cloak\n  2 = Fire_Cloak_Blind\nEnd\n\
CommandButton Fire_Cloak\n  Command = SPECIAL_POWER\n  SpecialPower = CloakPower\n  \
  Options = NEED_TARGET_POS\nEnd\n\
CommandButton Fire_Cloak_Blind\n  Command = SPECIAL_POWER\n  SpecialPower = CloakPower\nEnd\n\
Object Caster\n  Side = Men\n  KindOf = HERO SELECTABLE\n  CommandSet = CasterSet\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = InvisibilitySpecialPower Tag_Cloak\n    SpecialPowerTemplate = CloakPower\n    \
    BroadcastRadius = 150\n    ObjectFilter = ANY +INFANTRY ALLIES\n    Duration = 3000\n    \
    InvisibilityNugget\n      InvisibilityType = CAMOUFLAGE\n      DetectionRange = 40\n    End\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Ally\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 400\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Native\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 400\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  \
  Behavior = InvisibilityUpdate Tag_Camo\n    \
    InvisibilityNugget\n      InvisibilityType = CAMOUFLAGE\n      DetectionRange = 40\n      \
      ForbiddenConditions = AWAY_FROM_TREES\n    End\n    UpdatePeriod = 100\n    StartsActive = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Scout\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 400\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Bow\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    \
    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    MoodAttackCheckRate = 200\n  End\n  \
  VisionRange = 400\n  ShroudClearingRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Weapon Bow\n  PrimaryDamage = 10\n  AttackRange = 350\n  DelayBetweenShots = 500\n  \
  WeaponSpeed = 999999\n  \
  DamageNugget\n    Damage = 10\n    Radius = 0\n    DelayTime = 0\n    \
    DamageType = PIERCE\n    DeathType = NORMAL\n  End\nEnd\n";

fn scene() -> (Simulation<RotwkRules>, Ids, Entity, Entity) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (game, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots = ["Caster", "Ally", "Scout", "Native"];
    let (kinds, missing) = KindTable::closure(
        &game,
        &roots
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>(),
    );
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&game);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&game.special_powers);
    let places = [
        Vec2::new(300.0, 300.0),
        Vec2::new(310.0, 300.0),
        Vec2::new(650.0, 300.0),
        Vec2::new(300.0, 320.0),
    ];
    let owners = [PLAYER, PLAYER, OTHER, PLAYER];
    let spawns = roots
        .iter()
        .zip(places)
        .zip(owners)
        .map(|((name, at), owner)| {
            kinds
                .spawn(
                    &game.combat,
                    kinds.kind(name).unwrap(),
                    at,
                    Facing::EAST,
                    owner,
                )
                .unwrap()
        })
        .collect();
    let passability = Passability::build(&flat_map(96, 96), 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_combat(game.combat.clone())
        .with_construction(Arc::clone(&kinds), production)
        .with_upgrades(kinds.upgrades(&game), Arc::clone(&game.upgrade_bits))
        .with_powers(power_names, power_reload);
    let mut simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    simulation.tick().unwrap();
    (simulation, ids, entities[0], entities[1])
}

fn press(simulation: &mut Simulation<RotwkRules>, caster: Entity) {
    press_button(simulation, caster, 0, Some((300.0, 300.0)));
}

fn press_button(
    simulation: &mut Simulation<RotwkRules>,
    caster: Entity,
    button: u8,
    at: Option<(f32, f32)>,
) {
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
                button,
                target: None,
                at,
            },
        )
        .unwrap();
}

fn hidden(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> bool {
    simulation
        .state()
        .get(ids.stealth, entity)
        .is_some_and(|stealth| stealth.hidden)
}

#[test]
fn a_press_hides_an_ally_that_declares_no_hiding_of_its_own() {
    let (mut simulation, ids, caster, ally) = scene();
    assert!(
        !hidden(&simulation, ids, ally),
        "not hidden before the press"
    );
    press(&mut simulation, caster);
    for _ in 0..5 {
        simulation.tick().unwrap();
    }
    assert!(
        hidden(&simulation, ids, ally),
        "the ally must hide once granted"
    );
}

#[test]
fn the_grant_lapses_with_its_own_duration() {
    let (mut simulation, ids, caster, ally) = scene();
    press(&mut simulation, caster);
    for _ in 0..5 {
        simulation.tick().unwrap();
    }
    assert!(
        hidden(&simulation, ids, ally),
        "hidden while the grant is active"
    );
    for _ in 0..120 {
        simulation.tick().unwrap();
    }
    assert!(
        !hidden(&simulation, ids, ally),
        "revealed once Duration elapses"
    );
}

fn native(simulation: &Simulation<RotwkRules>, ids: Ids) -> Entity {
    simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec()[3]
}

#[test]
fn a_grant_hides_an_ally_whose_own_camouflage_is_forbidden_away_from_trees() {
    let (mut simulation, ids, caster, _ally) = scene();
    let native = native(&simulation, ids);
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    assert!(
        !hidden(&simulation, ids, native),
        "its own camouflage hides it nowhere here"
    );
    press(&mut simulation, caster);
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    assert!(hidden(&simulation, ids, native));
}

#[test]
fn a_repeat_grant_adds_one_duration_to_the_end_it_has() {
    let (mut simulation, ids, caster, ally) = scene();
    press(&mut simulation, caster);
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    press(&mut simulation, caster);
    for _ in 0..100 {
        simulation.tick().unwrap();
    }
    assert!(hidden(&simulation, ids, ally), "still hidden at 4.7 s");
    for _ in 0..80 {
        simulation.tick().unwrap();
    }
    assert!(!hidden(&simulation, ids, ally), "ended by 7.3 s");
}

#[test]
fn a_cast_hides_only_around_the_clicked_point() {
    let (mut simulation, ids, caster, ally) = scene();
    press_button(&mut simulation, caster, 0, Some((300.0, 700.0)));
    for _ in 0..40 {
        simulation.tick().unwrap();
    }
    assert!(!hidden(&simulation, ids, ally), "the click was far from it");
    press_button(&mut simulation, caster, 1, None);
    for _ in 0..10 {
        simulation.tick().unwrap();
    }
    assert!(
        !hidden(&simulation, ids, ally),
        "a target-less cast hides nobody"
    );
}
