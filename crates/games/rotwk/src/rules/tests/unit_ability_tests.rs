use super::*;

const DATA: &str = "\
Upgrade Upgrade_Oath\n  Type = OBJECT\nEnd\n\
Upgrade Upgrade_ClassA\n  Type = OBJECT\nEnd\n\
Upgrade Upgrade_ClassB\n  Type = OBJECT\nEnd\n\
Weapon BowA\n  AttackRange = 100\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 5\n    DamageType = PIERCE\n  End\nEnd\n\
Weapon BowB\n  AttackRange = 200\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 5\n    DamageType = PIERCE\n  End\nEnd\n\
Object Sentinel\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = SpecialEnemySenseUpdate Tag_Sense\n    SpecialEnemyFilter = ALL\n    \
    ScanRange = 100\n    ScanInterval = 200\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Foe\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Friend\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Locked\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY BowA\n    Weapon = SECONDARY BowB\n  End\n  \
  Behavior = LockWeaponCreate Tag_Lock\n    SlotToLock = SECONDARY\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Oathling\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = ModelConditionUpgrade Tag_Oath\n    TriggeredBy = Upgrade_Oath\n    \
    AddConditionFlags = OATH_FULLFILLED\n  End\n  \
  Behavior = OathbreakersFadeAwayBehavior Tag_Fade\n    FadeOutTime = 2000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Classy\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = ArmorUpgrade Tag_ClassA\n    TriggeredBy = Upgrade_ClassA\n    \
    ArmorSetFlag = CREATE_A_HERO_01\n  End\n  \
  Behavior = ArmorUpgrade Tag_ClassB\n    TriggeredBy = Upgrade_ClassB\n    \
    ArmorSetFlag = CREATE_A_HERO_02\n  End\n  \
  Behavior = RemoveUpgradeUpgrade Tag_DropB\n    TriggeredBy = Upgrade_ClassA\n    \
    UpgradeToRemove = Upgrade_ClassB\n  End\n  \
  Behavior = RemoveUpgradeUpgrade Tag_DropA\n    TriggeredBy = Upgrade_ClassB\n    \
    UpgradeToRemove = Upgrade_ClassA\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

const ROOTS: [&str; 6] = ["Sentinel", "Foe", "Friend", "Locked", "Oathling", "Classy"];

fn scene(placed: &[(&str, f32, f32, PlayerId)]) -> (Simulation<RotwkRules>, Ids, Vec<Entity>) {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (game, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let roots: Vec<String> = ROOTS.iter().map(|name| (*name).to_owned()).collect();
    let (kinds, missing) = KindTable::closure(&game, &roots);
    assert!(missing.is_empty(), "{missing:?}");
    let kinds = Arc::new(kinds);
    let production = kinds.production(&game);
    let spawns = placed
        .iter()
        .map(|(name, x, y, owner)| {
            kinds
                .spawn(
                    &game.combat,
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
        .with_combat(game.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_upgrades(kinds.upgrades(&game), Arc::clone(&game.upgrade_bits));
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let entities = simulation
        .state()
        .store(ids.body)
        .unwrap()
        .entities()
        .to_vec();
    (simulation, ids, entities)
}

fn run(simulation: &mut Simulation<RotwkRules>, ticks: u32) {
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
}

fn grant(simulation: &mut Simulation<RotwkRules>, unit: Entity, upgrade: &str) {
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![unit],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::Debug(DebugCommand::GrantUpgrade {
                upgrade: upgrade.to_owned(),
            }),
        )
        .unwrap();
    run(simulation, 2);
}

fn upgrade_state(
    simulation: &Simulation<RotwkRules>,
    entity: Entity,
) -> Option<mint_sage_modules::UpgradeState> {
    let wiring = simulation.rules().modules.as_ref()?;
    simulation.state().get(wiring.state, entity).cloned()
}

#[test]
fn an_enemy_within_scan_range_raises_the_special_enemy_near_condition() {
    let near = |other: (&str, f32, PlayerId)| {
        let (mut simulation, _, e) = scene(&[
            ("Sentinel", 300.0, 300.0, PLAYER),
            (other.0, 300.0 + other.1, 300.0, other.2),
        ]);
        run(&mut simulation, 40);
        upgrade_state(&simulation, e[0])
            .is_some_and(|state| state.has_model_condition("SPECIAL_ENEMY_NEAR"))
    };
    assert!(near(("Foe", 90.0, OTHER)), "an enemy at 90 of 100");
    assert!(!near(("Foe", 110.0, OTHER)), "an enemy at 110 of 100");
    assert!(!near(("Friend", 50.0, PLAYER)), "an ally at 50");
}

#[test]
fn lock_weapon_create_locks_its_slot_permanently() {
    let (mut simulation, _, e) = scene(&[("Locked", 300.0, 300.0, PLAYER)]);
    run(&mut simulation, 5);
    let state = upgrade_state(&simulation, e[0]).unwrap();
    assert_eq!(state.weapon_lock, Some(1), "SECONDARY");
    assert!(!state.weapon_lock_temporary);
}

#[test]
fn the_oathbreaker_fade_runs_only_while_oath_fullfilled_holds() {
    let (mut simulation, ids, e) = scene(&[("Oathling", 300.0, 300.0, PLAYER)]);
    let oathling = e[0];
    run(&mut simulation, 60);
    assert!(
        simulation.state().get(ids.fade_opacity, oathling).is_none(),
        "no fade without the condition"
    );
    grant(&mut simulation, oathling, "Upgrade_Oath");
    let mut seen = Vec::new();
    let mut unselectable_at_start = None;
    for _ in 0..120 {
        simulation.tick().unwrap();
        if let Some(opacity) = simulation.state().get(ids.fade_opacity, oathling) {
            if unselectable_at_start.is_none() {
                unselectable_at_start = Some(simulation.rules().holds_status(
                    simulation.state(),
                    oathling,
                    "UNSELECTABLE",
                ));
            }
            seen.push(opacity.0);
        }
    }
    assert_eq!(unselectable_at_start, Some(true));
    assert!(
        seen.iter().any(|opacity| (0.4..=0.6).contains(opacity)),
        "half opaque at some point: {seen:?}"
    );
    assert!(
        !simulation.state().is_live(oathling),
        "gone once the fade is spent"
    );
}

#[test]
fn a_second_class_upgrade_takes_the_first_classs_armour_set_back() {
    let (mut simulation, _, e) = scene(&[("Classy", 300.0, 300.0, PLAYER)]);
    let classy = e[0];
    run(&mut simulation, 3);
    grant(&mut simulation, classy, "Upgrade_ClassA");
    let first = upgrade_state(&simulation, classy).unwrap();
    assert!(first
        .armor_set_flags
        .iter()
        .any(|f| &**f == "CREATE_A_HERO_01"));
    grant(&mut simulation, classy, "Upgrade_ClassB");
    run(&mut simulation, 3);
    let second = upgrade_state(&simulation, classy).unwrap();
    assert!(second
        .armor_set_flags
        .iter()
        .any(|f| &**f == "CREATE_A_HERO_02"));
    assert!(
        !second
            .armor_set_flags
            .iter()
            .any(|f| &**f == "CREATE_A_HERO_01"),
        "the first class's armour set is back: {:?}",
        second.armor_set_flags
    );
}
