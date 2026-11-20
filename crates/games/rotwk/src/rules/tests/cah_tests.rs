use std::sync::Arc;

use mint_sage_cah::{HeroRecord, PowerEntry};

use super::hero_tests::{kill, press, run};
use super::*;
use crate::heroes::HeroState;
use crate::kill_credit::KillCredit;
use crate::lobby::CarriedHero;

const DATA: &str = "\
Upgrade Upgrade_TestCanBuild\n  Type = PLAYER\nEnd\n\
Upgrade Upgrade_TestMap\n  Type = OBJECT\nEnd\n\
Upgrade Upgrade_TestGame\n  Type = OBJECT\nEnd\n\
Upgrade Upgrade_TestClass\n  Type = OBJECT\nEnd\n\
Upgrade Upgrade_TestSub\n  Type = OBJECT\nEnd\n\
Upgrade Upgrade_TestHat\n  Type = OBJECT\nEnd\n\
Upgrade Upgrade_TestBaseGrant\n  Type = OBJECT\nEnd\n\
Upgrade Upgrade_TestUnlockHorn\n  Type = OBJECT\nEnd\n\
SpecialPower SpecialAbilityTestHorn\n  ReloadTime = 1000\nEnd\n\
CreateAHeroSystem\n  CreateAHeroMapModeUpgradeName = Upgrade_TestMap\n  \
  CreateAHeroGameModeUpgradeName = Upgrade_TestGame\n  \
  CanBuildCreateAHeroUpgradeName = Upgrade_TestCanBuild\n  CommandSetTemplate = TestCahTemplate\n  \
  HeroRevivalDiscount = 50\n  SpecialPowerDiscountPerLevel = 10\n  \
  CreateAHeroBling\n    GroupName = G_TestHat\n    BlingUpgradeName = Upgrade_TestHat\n  End\n  \
  CreateAHeroBlingBinder\n    GroupName = G_TestHat\n    UISlot = 1\n    BlingType = APPEARANCE\n  End\n  \
  CreateAHeroClass\n    UpgradeName = Upgrade_TestClass\n    SubClass\n      \
    UpgradeName = Upgrade_TestSub\n      ButtonImage = TestSubImage\n      \
    BlingUpgrades = Upgrade_TestHat\n    End\n  End\nEnd\n\
ExperienceLevel TestCahLevel1\n  TargetNames = CreateAHeroTest\n  RequiredExperience = 1\n  \
  Rank = 1\n  Upgrades = Upgrade_TestBaseGrant\nEnd\n\
ExperienceLevel TestCahLevel2\n  TargetNames = CreateAHeroTest\n  RequiredExperience = 5\n  Rank = 2\nEnd\n\
ExperienceLevel TestCahLevel3\n  TargetNames = CreateAHeroTest\n  RequiredExperience = 50\n  Rank = 3\nEnd\n\
Armor Plain\n  Armor = DEFAULT 100%\nEnd\n\
Weapon TestHammer\n  AttackRange = 200\n  DelayBetweenShots = 1000\n  \
  DamageNugget\n    Damage = 5000\n    DamageType = CRUSH\n  End\nEnd\n\
CommandSet TestCahTemplate\n  1 = Command_TestStop\nEnd\n\
CommandButton Command_TestStop\n  Command = STOP\nEnd\n\
CommandButton Power_TestHorn\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityTestHorn\n  \
  CreateAHeroUIMinimumLevel = 1\n  CreateAHeroUICostIfSelected = 1000\nEnd\n\
Object CreateAHeroTest\n  Side = Men\n  KindOf = HERO CREATE_A_HERO INFANTRY SELECTABLE\n  \
  CommandSet = TestCahTemplate\n  BuildCost = 1500\n  BuildTime = 10\n  CommandPoints = 50\n  \
  Body = RespawnBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = RespawnUpdate Tag_02\n    RespawnRules = AutoSpawn:No Cost:625 Time:3000 Health:100%\n  End\n  \
  Behavior = UnpauseSpecialPowerUpgrade Tag_Unpause\n    SpecialPowerTemplate = SpecialAbilityTestHorn\n    \
    TriggeredBy = Upgrade_TestUnlockHorn\n  End\n  \
  Behavior = SpecialPowerModule Tag_Horn\n    SpecialPowerTemplate = SpecialAbilityTestHorn\n    StartsPaused = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 8\n  GeometryHeight = 19\nEnd\n\
Object TestFortress\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE CASTLE_KEEP\n  \
  CommandSet = TestFortressSet\n  BuildCost = 5000\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 10000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 40\n  GeometryHeight = 40\nEnd\n\
CommandSet TestFortressSet\n  1 = Revive_TestCah\n  2 = Build_TestElite\n  3 = Revive_TestPlain\nEnd\n\
CommandButton Revive_TestPlain\n  Command = REVIVE\nEnd\n\
Object TestPlainHero\n  Side = Men\n  KindOf = HERO INFANTRY SELECTABLE\n  BuildCost = 100\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandButton Build_TestElite\n  Command = UNIT_BUILD\n  Object = TestElite\nEnd\n\
Object TestElite\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 100\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandButton Revive_TestCah\n  Command = REVIVE\n  Options = NEED_UPGRADE\n  \
  NeededUpgrade = Upgrade_TestCanBuild\nEnd\n\
Locomotor TestTrollFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Object TestTroll\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY TestHammer\n  End\n  \
  LocomotorSet\n    Locomotor = TestTrollFeet\n    Condition = SET_NORMAL\n    Speed = 40\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    MoodAttackCheckRate = 500\n  End\n  \
  VisionRange = 400\n  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

const HERO_SLOT: u8 = 0;

fn record() -> HeroRecord {
    let mut record = HeroRecord {
        id: Some(b"TESTID".to_vec()),
        name: "Testa".encode_utf16().collect(),
        blings: vec![(b"G_TestHat".to_vec(), 0)],
        colors: [0xFF10_2030, 0xFF40_5060, 0xFF70_8090],
        ..HeroRecord::default()
    };
    record.powers[2] = PowerEntry {
        button: b"Power_TestHorn".to_vec(),
        exp_level: 2,
        button_index: 1,
    };
    record
}

struct Cah {
    simulation: Simulation<RotwkRules>,
    ids: Ids,
    fortress: Entity,
    kinds: Arc<KindTable>,
}

fn start(holding: bool) -> Cah {
    start_with(holding, DATA)
}

fn start_with(holding: bool, text: &str) -> Cah {
    start_hero(holding.then(|| CarriedHero::new(Arc::new(record()))), text)
}

fn start_hero(hero: Option<CarriedHero>, text: &str) -> Cah {
    let doc =
        mint_sage_ini::Document::parse("t.ini", text, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let mut seeds: Vec<String> = [
        "TestFortress",
        "TestTroll",
        "CreateAHeroTest",
        "TestElite",
        "TestPlainHero",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    if text.contains("Object TestPet") {
        seeds.push("TestPet".to_owned());
    }
    let (mut kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "{missing:?}");
    let heroes: Vec<(PlayerId, CarriedHero)> =
        hero.map(|hero| (PLAYER, hero)).into_iter().collect();
    let (plans, lines) = crate::create_a_hero::plan_heroes(&data, &mut kinds, &heroes);
    assert_eq!(lines.len(), heroes.len());
    let can_build = data
        .create_a_hero
        .system
        .can_build_upgrade
        .as_deref()
        .and_then(|name| data.upgrade_bits.bit(name));
    let kinds = Arc::new(kinds);
    let production = kinds.production(&data);
    let power_names = crate::powers::PowerNames::build(kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&data.special_powers);
    let (rosters, hero_buildables, lines) = kinds.hero_setup(
        &data,
        &[
            vec!["CreateAHeroTest".to_owned(), "TestPlainHero".to_owned()],
            vec!["CreateAHeroTest".to_owned()],
        ],
    );
    assert!(lines.is_empty(), "{lines:?}");
    let placed_units = [
        ("TestFortress", 200.0, 200.0, PLAYER),
        ("TestTroll", 800.0, 800.0, OTHER),
        ("CreateAHeroTest", 600.0, 600.0, OTHER),
        ("CreateAHeroTest", 100.0, 100.0, PLAYER),
    ];
    let spawns: Vec<Spawn> = placed_units
        .iter()
        .map(|(name, x, y, owner)| {
            let kind = kinds.kind(name).unwrap();
            let mut spawn = kinds
                .spawn(&data.combat, kind, Vec2::new(*x, *y), Facing::EAST, *owner)
                .unwrap();
            spawn.producer = kinds.producer(&production, kind, Vec2::new(*x, *y), Facing::EAST);
            spawn
        })
        .collect();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_hordes(data.formations.tables().to_vec(), Vec::new())
        .with_combat(data.combat.clone())
        .with_construction(Arc::clone(&kinds), production.clone())
        .with_economy(production.buildables.clone(), 20_000)
        .with_menus(production.menus.clone())
        .with_menu_needs(production.menu_needs.clone())
        .with_heroes(rosters, hero_buildables)
        .with_create_a_hero(
            plans,
            can_build,
            crate::create_a_hero::revival_discount(&data.create_a_hero.system),
        )
        .with_upgrades(kinds.upgrades(&data), Arc::clone(&data.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_experience(Arc::clone(&data.experience), data.armor_bonus_cap);
    let simulation = Simulation::new(31, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let fortress = placed_of(&simulation, ids, &kinds, "TestFortress")[0];
    Cah {
        simulation,
        ids,
        fortress,
        kinds,
    }
}

fn placed_of(
    simulation: &Simulation<RotwkRules>,
    ids: Ids,
    kinds: &KindTable,
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

fn player_holds(cah: &Cah, player: PlayerId, upgrade: &str) -> bool {
    let rules = cah.simulation.rules();
    let world = cah.simulation.state();
    let bit = rules.upgrade_bits.bit(upgrade).unwrap();
    crate::economy::player_entity(world, RotwkRules::economy_ids(cah.ids), player)
        .and_then(|entity| world.get(cah.ids.player_upgrades, entity))
        .is_some_and(|held| held.0.contains(bit))
}

fn recruit(cah: &mut Cah) -> Entity {
    let fortress = cah.fortress;
    press(
        &mut cah.simulation,
        PLAYER,
        fortress,
        RotwkCommand::RecruitHero { button: HERO_SLOT },
    );
    run(&mut cah.simulation, 305);
    let roster = cah
        .simulation
        .rules()
        .roster_of(cah.simulation.state(), cah.ids, PLAYER)
        .unwrap();
    let alive: Vec<Entity> = roster
        .0
        .iter()
        .filter_map(|entry| match entry.state {
            HeroState::Alive(entity) => Some(entity),
            _ => None,
        })
        .collect();
    assert_eq!(alive.len(), 1, "the recruit came out");
    alive[0]
}

fn unbound_hero(cah: &Cah, bound: Entity) -> Entity {
    placed_of(&cah.simulation, cah.ids, &cah.kinds, "CreateAHeroTest")
        .into_iter()
        .find(|entity| {
            *entity != bound
                && cah
                    .simulation
                    .state()
                    .get(cah.ids.owner, *entity)
                    .is_some_and(|owner| owner.0 == PLAYER)
        })
        .unwrap()
}

fn names(cah: &Cah, hero: Entity) -> Vec<String> {
    cah.simulation
        .rules()
        .effective_upgrade_names(cah.simulation.state(), hero)
}

#[test]
fn the_recruit_upgrade_is_held_until_the_hero_is_bound_and_again_after_it_dies() {
    let mut cah = start(true);
    assert!(player_holds(&cah, PLAYER, "Upgrade_TestCanBuild"));
    assert!(!player_holds(&cah, OTHER, "Upgrade_TestCanBuild"));
    let hero = recruit(&mut cah);
    assert!(!player_holds(&cah, PLAYER, "Upgrade_TestCanBuild"));
    let ids = cah.ids;
    kill(&mut cah.simulation, ids, hero);
    assert!(player_holds(&cah, PLAYER, "Upgrade_TestCanBuild"));
}

#[test]
fn a_player_with_no_hero_is_never_offered_the_recruit() {
    let cah = start(false);
    assert!(!player_holds(&cah, PLAYER, "Upgrade_TestCanBuild"));
}

#[test]
fn a_bound_heros_colours_are_its_records_hair_skin_and_paint_and_nothing_elses() {
    let mut cah = start(true);
    let hero = recruit(&mut cah);
    let rules = cah.simulation.rules();
    let world = cah.simulation.state();
    let byte = |c: u8| f32::from(c) / 255.0;
    assert_eq!(
        rules.cah_colours(world, cah.ids, hero),
        Some([
            [byte(0x10), byte(0x20), byte(0x30), 1.0],
            [byte(0x40), byte(0x50), byte(0x60), 1.0],
            [byte(0x70), byte(0x80), byte(0x90), 1.0],
        ])
    );
    assert_eq!(rules.cah_colours(world, cah.ids, cah.fortress), None);
}

#[test]
fn a_bound_hero_holds_game_mode_bling_class_and_subclass_but_not_map_mode() {
    let mut cah = start(true);
    let hero = recruit(&mut cah);
    let held = names(&cah, hero);
    for upgrade in [
        "upgrade_testgame",
        "upgrade_testhat",
        "upgrade_testclass",
        "upgrade_testsub",
    ] {
        assert!(
            held.iter().any(|name| name.eq_ignore_ascii_case(upgrade)),
            "{upgrade}: {held:?}"
        );
    }
    assert!(!held
        .iter()
        .any(|name| name.eq_ignore_ascii_case("upgrade_testmap")));
}

#[test]
fn a_bound_heros_level_grants_its_picked_power_not_the_template_levels_upgrades() {
    let mut cah = start(true);
    let hero = recruit(&mut cah);
    let has = |cah: &Cah, entity: Entity, upgrade: &str| {
        names(cah, entity)
            .iter()
            .any(|name| name.eq_ignore_ascii_case(upgrade))
    };
    assert!(!has(&cah, hero, "Upgrade_TestBaseGrant"));
    assert!(!has(&cah, hero, "Upgrade_TestUnlockHorn"));
    cah.simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![hero],
            },
        )
        .unwrap();
    cah.simulation
        .submit(PLAYER, RotwkCommand::DebugExperience { points: 100 })
        .unwrap();
    run(&mut cah.simulation, 2);
    assert!(has(&cah, hero, "Upgrade_TestUnlockHorn"));
    assert!(!has(&cah, hero, "Upgrade_TestBaseGrant"));
    let unbound = placed_of(&cah.simulation, cah.ids, &cah.kinds, "CreateAHeroTest")
        .into_iter()
        .find(|entity| *entity != hero)
        .unwrap();
    assert!(has(&cah, unbound, "Upgrade_TestBaseGrant"));
}

#[test]
fn a_bound_heros_command_set_is_rebuilt_per_rank_with_its_power_in_its_slot() {
    let mut cah = start(true);
    let hero = recruit(&mut cah);
    let rules = cah.simulation.rules();
    let first = rules
        .command_set_override(cah.simulation.state(), hero)
        .unwrap()
        .to_owned();
    assert!(first.ends_with("_rank_1"), "{first}");
    let def = rules.cah_set_named(&first).unwrap();
    assert_eq!(def.slot(2), Some("Power_TestHorn"));
    cah.simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![hero],
            },
        )
        .unwrap();
    cah.simulation
        .submit(PLAYER, RotwkCommand::DebugExperience { points: 100 })
        .unwrap();
    run(&mut cah.simulation, 2);
    let later = cah
        .simulation
        .rules()
        .command_set_override(cah.simulation.state(), hero)
        .unwrap();
    assert!(later.ends_with("_rank_3"), "{later}");
}

#[test]
fn the_recruit_price_includes_the_powers_and_the_revive_is_a_share_of_it() {
    let mut cah = start(true);
    let fortress = cah.fortress;
    let price = |cah: &Cah| {
        cah.simulation
            .rules()
            .hero_page(cah.simulation.state(), PLAYER, Some(fortress))[0]
            .cost
    };
    assert_eq!(price(&cah), Some(2300));
    let hero = recruit(&mut cah);
    let ids = cah.ids;
    kill(&mut cah.simulation, ids, hero);
    assert!(matches!(
        cah.simulation
            .rules()
            .roster_of(cah.simulation.state(), ids, PLAYER)
            .unwrap()
            .0[0]
            .state,
        HeroState::Dead { .. }
    ));
    assert_eq!(price(&cah), Some(1150));
}

#[test]
fn a_held_heros_revive_applies_the_discount_before_the_structure_modifier() {
    let data = DATA
        .replace("HeroRevivalDiscount = 50", "HeroRevivalDiscount = 75")
        .replace(
            "  Behavior = ProductionUpdate Tag_02
  End
",
            "  Behavior = ProductionUpdate Tag_02
    ProductionModifier
      CostMultiplier = 0.78
                   HeroRevive = Yes
      ModifierFilter = NONE +HERO
    End
  End
",
        );
    let mut cah = start_with(true, &data);
    let fortress = cah.fortress;
    let price = |cah: &Cah| {
        cah.simulation
            .rules()
            .hero_page(cah.simulation.state(), PLAYER, Some(fortress))[0]
            .cost
    };
    assert_eq!(price(&cah), Some(2300));
    let hero = recruit(&mut cah);
    let ids = cah.ids;
    kill(&mut cah.simulation, ids, hero);
    assert_eq!(price(&cah), Some(1345));
}

#[test]
fn a_plain_hero_of_the_player_holding_a_record_is_not_bound() {
    let mut cah = start(true);
    let bound = recruit(&mut cah);
    let fortress = cah.fortress;
    press(
        &mut cah.simulation,
        PLAYER,
        fortress,
        RotwkCommand::RecruitHero { button: 1 },
    );
    run(&mut cah.simulation, 400);
    let plain = placed_of(&cah.simulation, cah.ids, &cah.kinds, "TestPlainHero");
    assert_eq!(plain.len(), 1, "the plain hero came out");
    let rules = cah.simulation.rules();
    let world = cah.simulation.state();
    assert!(rules.cah_colours(world, cah.ids, bound).is_some());
    assert_eq!(rules.cah_colours(world, cah.ids, plain[0]), None);
    assert_eq!(rules.cah_display_name(world, plain[0]), None);
}

#[test]
fn another_object_of_the_same_kind_and_owner_does_not_take_the_record() {
    let mut cah = start(true);
    let hero = recruit(&mut cah);
    let other = unbound_hero(&cah, hero);
    let rules = cah.simulation.rules();
    let world = cah.simulation.state();
    assert!(rules.cah_display_name(world, hero).is_some());
    assert!(rules.cah_display_name(world, other).is_none());
    assert!(rules.cah_colours(world, cah.ids, other).is_none());
    assert!(rules.hero_table(world, cah.ids, other).is_none());
    assert!(rules.cah_command_set(world, other).is_none());
}

fn with_hat_award() -> String {
    DATA.replacen(
        "  CreateAHeroBling\n    GroupName = G_TestHat\n    BlingUpgradeName = Upgrade_TestHat\n  End\n",
        "  CreateAHeroBling\n    GroupName = G_TestHat\n    BlingUpgradeName = Upgrade_TestHat\n  End\n  \
           CreateAHeroBling\n    GroupName = G_TestHat\n    BlingUpgradeName = Upgrade_TestHatB\n  End\n",
        1,
    )
    .replacen(
        "BlingUpgrades = Upgrade_TestHat\n",
        "BlingUpgrades = Upgrade_TestHat\n      Awards = AwardHat\n",
        1,
    ) + "Upgrade Upgrade_TestHatB\n  Type = OBJECT\nEnd\n\
         AwardSystem\n  ObjectAward AwardHat\n    UpgradeAwards = Upgrade_TestHatB\n    \
           Trigger\n      Stat = StatKills\n      Threshold = 3\n    End\n  End\nEnd\n"
}

fn stat_dir(name: &str, kills: i32) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mint-cah-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let counters = std::collections::BTreeMap::from([("StatKills".to_owned(), kills)]);
    mint_sage_asset::write_stats(&dir, "TESTID", &counters).unwrap();
    dir
}

fn bound_with_stat_dir(dir: Option<&std::path::Path>) -> (Cah, Entity) {
    let mut record = record();
    record.blings = vec![(b"G_TestHat".to_vec(), 1)];
    let mut hero = CarriedHero::new(Arc::new(record));
    if let Some(dir) = dir {
        hero = hero.with_stats_from(dir);
    }
    let mut cah = start_hero(Some(hero), &with_hat_award());
    let bound = recruit(&mut cah);
    (cah, bound)
}

#[test]
fn a_bound_heros_awards_come_from_the_carried_counters_not_from_the_disk() {
    let earned_a = stat_dir("a", 3);
    let earned_b = stat_dir("b", 300);
    let (a, a_hero) = bound_with_stat_dir(Some(&earned_a));
    let (b, b_hero) = bound_with_stat_dir(Some(&earned_b));
    let (none, none_hero) = bound_with_stat_dir(None);
    std::fs::remove_dir_all(earned_a).unwrap();
    std::fs::remove_dir_all(earned_b).unwrap();
    assert!(names(&a, a_hero).contains(&"upgrade_testhatb".to_owned()));
    assert!(!names(&none, none_hero).contains(&"upgrade_testhatb".to_owned()));
    assert_eq!(a.simulation.state_hash(), b.simulation.state_hash());
    assert_ne!(a.simulation.state_hash(), none.simulation.state_hash());
    assert_eq!(names(&a, a_hero), names(&b, b_hero));
}

const PET: &str =
    "Object TestPet\n  Side = Men\n  KindOf = INFANTRY SELECTABLE PASS_EXPERIENCE_TO_PRODUCER\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY TestSniper\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n  End\n  VisionRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
  Weapon TestSniper\n  AttackRange = 5000\n  DelayBetweenShots = 1000\n  \
  DamageNugget\n    Damage = 5000\n    DamageType = PIERCE\n  End\nEnd\n";

fn pet_kills_troll(pet: &str) -> (Vec<KillCredit>, Entity, crate::unit::Kind) {
    let text = DATA
        .replacen("MaxHealth = 100000", "MaxHealth = 100", 1)
        .replacen(
            "  Geometry = CYLINDER\n  GeometryMajorRadius = 8\n  GeometryHeight = 19\nEnd\nObject TestFortress",
            "  Behavior = SpawnBehavior Tag_Spawn\n    SpawnNumber = 1\n    InitialBurst = 1\n    \
               SpawnTemplateName = TestPet\n    SpawnReplaceDelay = 100000\n  End\n  \
               Geometry = CYLINDER\n  GeometryMajorRadius = 8\n  GeometryHeight = 19\nEnd\nObject TestFortress",
            1,
        )
        + pet;
    let mut cah = start_hero(Some(CarriedHero::new(Arc::new(record()))), &text);
    let ids = cah.ids;
    run(&mut cah.simulation, 90);
    let troll_kind = cah.kinds.kind("TestTroll").unwrap();
    let troll = placed_of(&cah.simulation, ids, &cah.kinds, "TestTroll")[0];
    let owned_by_player = |entity: &Entity| {
        cah.simulation
            .state()
            .get(ids.owner, *entity)
            .is_some_and(|owner| owner.0 == PLAYER)
    };
    let pet = placed_of(&cah.simulation, ids, &cah.kinds, "TestPet")
        .into_iter()
        .find(owned_by_player)
        .expect("the hero spawned its pet");
    let hero = placed_of(&cah.simulation, ids, &cah.kinds, "CreateAHeroTest")
        .into_iter()
        .find(owned_by_player)
        .unwrap();
    cah.simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![pet],
            },
        )
        .unwrap();
    cah.simulation
        .submit(PLAYER, RotwkCommand::AttackObject { target: troll })
        .unwrap();
    let mut credits = Vec::new();
    for _ in 0..300 {
        run(&mut cah.simulation, 1);
        let ticked = mint_sim::Frame::new(cah.simulation.frame().get().saturating_sub(1));
        credits.extend_from_slice(
            cah.simulation
                .rules()
                .kill_credits(cah.simulation.state(), ticked),
        );
    }
    assert!(
        !cah.simulation.state().is_live(troll)
            || cah.simulation.state().contains(ids.doomed, troll),
        "the pet killed the troll"
    );
    (credits, hero, troll_kind)
}

#[test]
fn a_kill_by_a_hero_spawned_pet_is_credited_to_the_hero() {
    let (credits, hero, troll_kind) = pet_kills_troll(PET);
    assert_eq!(
        credits,
        vec![KillCredit {
            killer: hero,
            victim: troll_kind
        }]
    );
}

#[test]
fn a_kill_by_a_pet_that_does_not_pass_experience_is_credited_to_nobody() {
    let (credits, _, _) = pet_kills_troll(&PET.replacen(" PASS_EXPERIENCE_TO_PRODUCER", "", 1));
    assert!(credits.is_empty());
}
