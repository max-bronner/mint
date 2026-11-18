use super::*;
use crate::heroes::HeroState;

const DATA: &str = "\
Upgrade Upgrade_Banners\n  Type = OBJECT\n  BuildCost = 100\n  BuildTime = 1\nEnd\n\
Upgrade Upgrade_Healing\n  Type = OBJECT\n  BuildCost = 100\n  BuildTime = 1\nEnd\n\
Upgrade Upgrade_Walls\n  Type = OBJECT\n  BuildCost = 100\n  BuildTime = 1\nEnd\n\
Upgrade Upgrade_Tower\n  Type = OBJECT\n  BuildCost = 100\n  BuildTime = 1\nEnd\n\
Upgrade Upgrade_RingHero\n  Type = PLAYER\nEnd\n\
Upgrade Upgrade_HornOfHero\n  Type = OBJECT\nEnd\n\
SpecialPower SpecialAbilityHorn\n  ReloadTime = 1000\nEnd\n\
SpecialPower SpecialAbilityOil\n  ReloadTime = 1000\nEnd\n\
ModifierList HeroLevelUpDamage1\n  Category = LEVEL\n  Modifier = HEALTH 60\n  \
  Modifier = DAMAGE_ADD 10\n  Duration = 0\nEnd\n\
ExperienceLevel HeroLevel1\n  TargetNames = Hero\n  RequiredExperience = 1\n  \
  ExperienceAward = 20\n  Rank = 1\nEnd\n\
ExperienceLevel HeroLevel2\n  TargetNames = Hero\n  RequiredExperience = 5\n  \
  ExperienceAward = 25\n  Rank = 2\n  AttributeModifiers = HeroLevelUpDamage1\n  \
  Upgrades = Upgrade_HornOfHero\nEnd\n\
ExperienceLevel TargetLevel1\n  TargetNames = Target\n  RequiredExperience = 1\n  \
  ExperienceAward = 3\n  Rank = 1\nEnd\n\
Armor Plain\n  Armor = DEFAULT 100%\nEnd\n\
Weapon Javelin\n  AttackRange = 150\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 10\n    DamageType = PIERCE\n  End\nEnd\n\
Weapon Hammer\n  AttackRange = 200\n  DelayBetweenShots = 1000\n  \
  DamageNugget\n    Damage = 5000\n    DamageType = CRUSH\n  End\nEnd\n\
Object Hero\n  Side = Men\n  KindOf = HERO INFANTRY SELECTABLE\n  CommandSet = HeroSet\n  \
  BuildCost = 1500\n  BuildTime = 10\n  CommandPoints = 50\n  ButtonImage = HIHero\n  \
  Body = RespawnBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = RespawnUpdate Tag_02\n    RespawnRules = AutoSpawn:No Cost:625 Time:3000 Health:100%\n    \
    RespawnRules = AutoSpawn:No Cost:750 Time:3000 Health:80% Level:2\n    \
    AutoRespawnAtObjectFilter = NONE +CASTLE_KEEP\n    \
    DeathFX = FX_HeroDeath\n    InitialSpawnFX = FX_HeroInitialSpawn\n    \
    RespawnFX = FX_HeroRespawn\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Javelin\n  End\n  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Behavior = UnpauseSpecialPowerUpgrade Tag_EnableHorn\n    \
    SpecialPowerTemplate = SpecialAbilityHorn\n    TriggeredBy = Upgrade_HornOfHero\n  End\n  \
  Behavior = SpecialPowerModule Tag_HornStarter\n    \
    SpecialPowerTemplate = SpecialAbilityHorn\n    StartsPaused = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 8\n  GeometryHeight = 19\nEnd\n\
CommandSet HeroSet\n  1 = Fire_Horn\nEnd\n\
CommandButton Fire_Horn\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityHorn\nEnd\n\
Object RingHero\n  Side = Men\n  KindOf = HERO INFANTRY SELECTABLE\n  BuildCost = 3000\n  BuildTime = 10\n  \
  Body = RespawnBody Tag_01\n    MaxHealth = 2000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 8\n  GeometryHeight = 19\nEnd\n\
Object Scout\n  Side = Men\n  KindOf = HERO INFANTRY SELECTABLE\n  BuildCost = 500\n  BuildTime = 10\n  \
  Body = RespawnBody Tag_01\n    MaxHealth = 800\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 8\n  GeometryHeight = 19\nEnd\n\
Object Porter\n  Side = Men\n  KindOf = INFANTRY SELECTABLE DOZER\n  BuildCost = 100\n  BuildTime = 1\n  \
  CommandPoints = 40\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Elite\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 100\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Fortress\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE CASTLE_KEEP\n  \
  CommandSet = FortressSet\n  BuildCost = 5000\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 10000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    \
    ProductionModifier\n      RequiredUpgrade = Upgrade_Banners\n      CostMultiplier = 0.80\n      \
      ModifierFilter = NONE +Porter\n    End\n    \
    ProductionModifier\n      RequiredUpgrade = Upgrade_Banners\n      CostMultiplier = 0.90\n      \
      TimeMultiplier = 0.90\n      HeroPurchase = Yes\n      ModifierFilter = NONE +HERO\n    End\n    \
    ProductionModifier\n      RequiredUpgrade = Upgrade_Healing\n      CostMultiplier = 0.80\n      \
      TimeMultiplier = 0.80\n      HeroRevive = Yes\n      ModifierFilter = NONE +HERO\n    End\n  End\n  \
  Behavior = SubObjectsUpgrade Tag_Banners\n    TriggeredBy = Upgrade_Banners\n    ShowSubObjects = Banners\n  End\n  \
  Behavior = SubObjectsUpgrade Tag_Healing\n    TriggeredBy = Upgrade_Healing\n    ShowSubObjects = Healing\n  End\n  \
  Behavior = SubObjectsUpgrade Tag_Tower\n    TriggeredBy = Upgrade_Tower\n    ShowSubObjects = Tower\n  End\n  \
  Behavior = UnpauseSpecialPowerUpgrade Tag_OilEnabler\n    \
    SpecialPowerTemplate = SpecialAbilityOil\n    TriggeredBy = Upgrade_Healing\n  End\n  \
  Behavior = SpecialPowerModule Tag_OilStarter\n    SpecialPowerTemplate = SpecialAbilityOil\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 40\n  GeometryHeight = 40\nEnd\n\
Object Camp\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  CommandSet = FortressSet\n  BuildCost = 5000\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 10000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 40\n  GeometryHeight = 40\nEnd\n\
CommandSet FortressSet\n  1 = Build_Porter\n  2 = Revive_Ring\n  3 = Revive_1\n  4 = Buy_Banners\n  \
  5 = Buy_Healing\n  6 = Buy_Walls\n  7 = Build_Elite\n  8 = Fire_Oil\n  9 = Buy_Tower\n  10 = Revive_2\nEnd\n\
CommandButton Build_Porter\n  Command = UNIT_BUILD\n  Object = Porter\nEnd\n\
CommandButton Build_Elite\n  Command = UNIT_BUILD\n  Object = Elite\n  Options = NEED_UPGRADE\n  \
  NeededUpgrade = Upgrade_Banners\nEnd\n\
CommandButton Revive_Ring\n  Command = REVIVE\n  Options = HIDE_WHILE_DISABLED NEED_UPGRADE\n  \
  NeededUpgrade = Upgrade_RingHero\nEnd\n\
CommandButton Revive_1\n  Command = REVIVE\n  Options = HIDE_WHILE_DISABLED\nEnd\n\
CommandButton Revive_2\n  Command = REVIVE\n  Options = HIDE_WHILE_DISABLED\nEnd\n\
CommandButton Buy_Banners\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_Banners\nEnd\n\
CommandButton Buy_Healing\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_Healing\nEnd\n\
CommandButton Buy_Walls\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_Walls\nEnd\n\
CommandButton Buy_Tower\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_Tower\n  Options = NEED_UPGRADE\n  \
  NeededUpgrade = Upgrade_Healing\nEnd\n\
CommandButton Fire_Oil\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityOil\n  \
  Options = NEED_UPGRADE\n  NeededUpgrade = Upgrade_Banners\nEnd\n\
Object Target\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 25\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Locomotor TrollFeet\n  Surfaces = GROUND\n  Acceleration = 500\n  Braking = 500\n  TurnTime = 100\nEnd\n\
Object Troll\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Hammer\n  End\n  LocomotorSet\n    Locomotor = TrollFeet\n    Condition = SET_NORMAL\n    Speed = 40\n  End\n  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Upgrade Upgrade_SelfTrigger\n  Type = OBJECT\nEnd\n\
Object Deliverer\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = GrantUpgradeCreate Tag_Grant\n    UpgradeToGrant = Upgrade_RingHero\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object StripRing\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = GrantUpgradeCreate Tag_Self\n    UpgradeToGrant = Upgrade_SelfTrigger\n  End\n  \
  Behavior = RemoveUpgradeUpgrade Tag_Strip\n    TriggeredBy = Upgrade_SelfTrigger\n    \
    UpgradeToRemove = Upgrade_RingHero\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n";

const RING: u8 = 0;
const HERO: u8 = 1;

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
    let seeds: Vec<String> = [
        "Fortress",
        "Camp",
        "Target",
        "Troll",
        "RingHero",
        "Hero",
        "Scout",
        "Deliverer",
        "StripRing",
    ]
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

fn rules(m: &Match, placed: &[(&str, f32, f32, PlayerId)], cash: i64) -> RotwkRules {
    let production = m.kinds.production(&m.data);
    let upgrade_menus = m.kinds.upgrades(&m.data);
    let power_names = crate::powers::PowerNames::build(m.kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&m.data.special_powers);
    let (rosters, hero_buildables, lines) = m.kinds.hero_setup(
        &m.data,
        &[
            vec!["RingHero".to_owned(), "Hero".to_owned(), "Scout".to_owned()],
            vec!["CreateAHero".to_owned()],
        ],
    );
    assert!(lines.is_empty(), "{lines:?}");
    let spawns: Vec<Spawn> = placed
        .iter()
        .map(|(name, x, y, owner)| {
            let kind = m.kinds.kind(name).unwrap();
            let mut spawn = m
                .kinds
                .spawn(
                    &m.data.combat,
                    kind,
                    Vec2::new(*x, *y),
                    Facing::EAST,
                    *owner,
                )
                .unwrap();
            spawn.producer = m
                .kinds
                .producer(&production, kind, Vec2::new(*x, *y), Facing::EAST);
            spawn
        })
        .collect();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_hordes(m.data.formations.tables().to_vec(), Vec::new())
        .with_combat(m.data.combat.clone())
        .with_construction(Arc::clone(&m.kinds), production.clone())
        .with_economy(production.buildables.clone(), cash)
        .with_menus(production.menus.clone())
        .with_menu_needs(production.menu_needs.clone())
        .with_heroes(rosters, hero_buildables)
        .with_upgrades(upgrade_menus, Arc::clone(&m.data.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_experience(Arc::clone(&m.data.experience), m.data.armor_bonus_cap)
}

pub(super) fn run(simulation: &mut Simulation<RotwkRules>, ticks: u32) {
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
}

fn placed(simulation: &Simulation<RotwkRules>, ids: Ids, m: &Match, name: &str) -> Vec<Entity> {
    let kind = m.kinds.kind(name).unwrap();
    simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(entity, k)| **k == kind && simulation.state().is_live(*entity))
        .map(|(entity, _)| entity)
        .collect()
}

fn purse(simulation: &Simulation<RotwkRules>, ids: Ids, player: PlayerId) -> i64 {
    crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), player).amount()
}

fn state(simulation: &Simulation<RotwkRules>, ids: Ids, slot: usize) -> HeroState {
    simulation
        .rules()
        .roster_of(simulation.state(), ids, PLAYER)
        .unwrap()
        .0[slot]
        .state
}

pub(super) fn press(
    simulation: &mut Simulation<RotwkRules>,
    owner: PlayerId,
    structure: Entity,
    command: RotwkCommand,
) {
    simulation
        .submit(
            owner,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![structure],
            },
        )
        .unwrap();
    simulation.submit(owner, command).unwrap();
    run(simulation, 1);
}

fn last_entry(
    simulation: &Simulation<RotwkRules>,
    ids: Ids,
    fortress: Entity,
) -> Option<(i64, u32)> {
    simulation
        .state()
        .get(ids.producer, fortress)?
        .queue
        .entries()
        .last()
        .map(|entry| (entry.paid, entry.frames_total))
}

fn fortress_match(cash: i64) -> (Match, Simulation<RotwkRules>, Ids, Entity) {
    let m = load();
    let simulation =
        Simulation::new(21, rules(&m, &[("Fortress", 200.0, 200.0, PLAYER)], cash)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let fortress = placed(&simulation, ids, &m, "Fortress")[0];
    (m, simulation, ids, fortress)
}

#[test]
fn a_hero_page_press_recruits_the_hero_and_stays_shown_until_fielded() {
    let (m, mut simulation, ids, fortress) = fortress_match(10_000);
    let page = simulation
        .rules()
        .hero_page(simulation.state(), PLAYER, Some(fortress));
    assert_eq!(page.len(), 3);
    assert!(!page[1].fielded && page[1].affordable);
    assert_eq!(page[1].cost, Some(1500));
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::RecruitHero { button: HERO },
    );
    assert_eq!(purse(&simulation, ids, PLAYER), 8_500);
    assert_eq!(last_entry(&simulation, ids, fortress), Some((1500, 300)));
    assert!(matches!(
        state(&simulation, ids, 1),
        HeroState::Queued { revive: None, .. }
    ));
    let page = simulation
        .rules()
        .hero_page(simulation.state(), PLAYER, Some(fortress));
    assert!(
        !page[1].fielded,
        "a hero in production stays on the page with its progress timer"
    );

    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::RecruitHero { button: HERO },
    );
    assert_eq!(purse(&simulation, ids, PLAYER), 8_500);
    run(&mut simulation, 305);
    let heroes = placed(&simulation, ids, &m, "Hero");
    assert_eq!(heroes.len(), 1, "exactly one hero came out");
    assert_eq!(state(&simulation, ids, 1), HeroState::Alive(heroes[0]));
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::RecruitHero { button: HERO },
    );
    assert_eq!(
        placed(&simulation, ids, &m, "Hero").len(),
        1,
        "one of each at a time"
    );
    assert_eq!(purse(&simulation, ids, PLAYER), 8_500);
}

#[test]
fn the_ring_hero_slot_needs_its_upgrade() {
    let (_m, mut simulation, ids, fortress) = fortress_match(10_000);
    assert_eq!(
        simulation
            .rules()
            .hero_order(simulation.state(), ids, PLAYER, fortress, RING),
        Err(crate::heroes::HeroRefusal::NeedsUpgrade)
    );
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::RecruitHero { button: RING },
    );
    assert_eq!(purse(&simulation, ids, PLAYER), 10_000);
    assert_eq!(state(&simulation, ids, 0), HeroState::Available);
}

#[test]
fn losing_the_ring_hero_upgrade_mid_queue_cancels_its_production() {
    let (m, mut simulation, ids, fortress) = fortress_match(10_000);
    let spawn_debug = |simulation: &mut Simulation<RotwkRules>, name: &str| {
        let kind = m.kinds.kind(name).unwrap();
        simulation
            .submit(
                PLAYER,
                RotwkCommand::Debug(DebugCommand::Spawn {
                    kind: kind.0,
                    x: 300.0,
                    y: 300.0,
                    facing: 0.0,
                    role: SpawnRole::Normal,
                }),
            )
            .unwrap();
        run(simulation, 1);
    };
    spawn_debug(&mut simulation, "Deliverer");
    assert_eq!(
        simulation
            .rules()
            .hero_order(simulation.state(), ids, PLAYER, fortress, RING)
            .map(|order| order.cost),
        Ok(3_000)
    );
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::RecruitHero { button: RING },
    );
    assert_eq!(purse(&simulation, ids, PLAYER), 7_000);
    assert!(matches!(
        state(&simulation, ids, 0),
        HeroState::Queued { revive: None, .. }
    ));
    spawn_debug(&mut simulation, "StripRing");
    assert_eq!(
        state(&simulation, ids, 0),
        HeroState::Available,
        "losing Upgrade_RingHero cancels the queued ring hero"
    );
    assert_eq!(purse(&simulation, ids, PLAYER), 10_000, "refunded in full");
}

#[test]
fn a_recruit_discount_applies_behind_its_upgrade() {
    let (_m, mut simulation, ids, fortress) = fortress_match(10_000);
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::ObjectUpgrade { button: 0 },
    );
    run(&mut simulation, 35);
    let before = purse(&simulation, ids, PLAYER);
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::RecruitHero { button: HERO },
    );
    assert_eq!(before - purse(&simulation, ids, PLAYER), 1350, "1500 x 0.9");
    assert_eq!(
        last_entry(&simulation, ids, fortress),
        Some((1350, 270)),
        "300 x 0.9"
    );
}

#[test]
fn an_unaffordable_hero_is_dimmed_and_refused() {
    let (_m, mut simulation, ids, fortress) = fortress_match(1_000);
    let page = simulation
        .rules()
        .hero_page(simulation.state(), PLAYER, Some(fortress));
    assert!(!page[1].fielded && !page[1].affordable);
    assert_eq!(page[1].cost, Some(1500));
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::RecruitHero { button: HERO },
    );
    assert_eq!(purse(&simulation, ids, PLAYER), 1_000);
    assert_eq!(state(&simulation, ids, 1), HeroState::Available);
}

#[test]
fn a_hero_over_the_command_point_limit_is_refused_before_the_queue() {
    let m = load();
    let rules =
        rules(&m, &[("Fortress", 200.0, 200.0, PLAYER)], 10_000).with_command_point_factor(10);
    let mut simulation = Simulation::new(21, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let fortress = placed(&simulation, ids, &m, "Fortress")[0];
    let refusal = simulation
        .rules()
        .hero_order(simulation.state(), ids, PLAYER, fortress, HERO);
    assert!(
        matches!(
            refusal,
            Err(crate::heroes::HeroRefusal::CommandPoints { needed: 50, free }) if free < 50
        ),
        "{refusal:?}"
    );
    let page = simulation
        .rules()
        .hero_page(simulation.state(), PLAYER, Some(fortress));
    assert!(!page[1].affordable && page[1].allowed);
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::RecruitHero { button: HERO },
    );
    assert_eq!(purse(&simulation, ids, PLAYER), 10_000);
    assert_eq!(last_entry(&simulation, ids, fortress), None);
    assert_eq!(state(&simulation, ids, 1), HeroState::Available);
}

#[test]
fn a_queued_entry_does_not_count_against_a_heros_command_points() {
    let m = load();
    let rules =
        rules(&m, &[("Fortress", 200.0, 200.0, PLAYER)], 10_000).with_command_point_factor(20);
    let mut simulation = Simulation::new(21, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let fortress = placed(&simulation, ids, &m, "Fortress")[0];
    let free = |simulation: &Simulation<RotwkRules>| {
        crate::economy::points_of(simulation.state(), RotwkRules::economy_ids(ids), PLAYER).free()
    };
    let before = free(&simulation);
    assert!((50..90).contains(&before), "precondition: {before} free");
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::QueueUnit { button: 0 },
    );
    assert_eq!(free(&simulation), before, "the porter is still queued");
    let order = simulation
        .rules()
        .hero_order(simulation.state(), ids, PLAYER, fortress, HERO);
    assert!(order.is_ok(), "{order:?}");
}

#[test]
fn a_unit_press_is_refused_only_against_living_command_points() {
    let m = load();
    let entries = |simulation: &Simulation<RotwkRules>, ids: Ids, fortress: Entity| {
        simulation
            .state()
            .get(ids.producer, fortress)
            .map_or(0, |producer| producer.queue.entries().len())
    };
    let porter = m.kinds.kind("Porter").unwrap();
    let short =
        rules(&m, &[("Fortress", 200.0, 200.0, PLAYER)], 10_000).with_command_point_factor(5);
    let mut simulation = Simulation::new(21, short).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let fortress = placed(&simulation, ids, &m, "Fortress")[0];
    assert!(!simulation
        .rules()
        .unit_command_points_fit(simulation.state(), PLAYER, porter));
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::QueueUnit { button: 0 },
    );
    assert_eq!(
        entries(&simulation, ids, fortress),
        0,
        "refused, nothing queued"
    );
    assert_eq!(purse(&simulation, ids, PLAYER), 10_000, "nothing charged");

    let room =
        rules(&m, &[("Fortress", 200.0, 200.0, PLAYER)], 10_000).with_command_point_factor(20);
    let mut simulation = Simulation::new(21, room).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let fortress = placed(&simulation, ids, &m, "Fortress")[0];
    let free =
        crate::economy::points_of(simulation.state(), RotwkRules::economy_ids(ids), PLAYER).free();
    assert!((40..80).contains(&free), "precondition: {free} free");
    for _ in 0..2 {
        press(
            &mut simulation,
            PLAYER,
            fortress,
            RotwkCommand::QueueUnit { button: 0 },
        );
    }
    assert_eq!(
        entries(&simulation, ids, fortress),
        2,
        "queued entries never count"
    );
}

#[test]
fn cancelling_a_hero_in_production_puts_its_slot_back() {
    let (_m, mut simulation, ids, fortress) = fortress_match(10_000);
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::RecruitHero { button: HERO },
    );
    assert_eq!(purse(&simulation, ids, PLAYER), 8_500);
    simulation
        .submit(PLAYER, RotwkCommand::CancelProduction)
        .unwrap();
    run(&mut simulation, 1);
    assert_eq!(purse(&simulation, ids, PLAYER), 10_000);
    assert_eq!(state(&simulation, ids, 1), HeroState::Available);
}

fn queue_of(simulation: &Simulation<RotwkRules>, ids: Ids, fortress: Entity) -> Vec<(u32, u32)> {
    simulation
        .state()
        .get(ids.producer, fortress)
        .map(|producer| {
            producer
                .queue
                .entries()
                .iter()
                .map(|entry| (entry.item.index(), entry.frames_done))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn heroes_recruit_side_by_side_while_units_wait_their_turn() {
    let (m, mut simulation, ids, fortress) = fortress_match(10_000);
    let porter = RotwkCommand::QueueUnit { button: 0 };
    press(&mut simulation, PLAYER, fortress, porter.clone());
    press(&mut simulation, PLAYER, fortress, porter);
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::RecruitHero { button: HERO },
    );
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::RecruitHero { button: 2 },
    );
    run(&mut simulation, 4);
    let done: Vec<u32> = queue_of(&simulation, ids, fortress)
        .into_iter()
        .map(|(_, done)| done)
        .collect();
    assert_eq!(done.len(), 4);
    assert!(done[0] > 0 && done[2] > 0 && done[3] > 0, "{done:?}");
    assert_eq!(done[1], 0, "the second porter waits: {done:?}");
    let rules = simulation.rules();
    assert_eq!(rules.build_count(simulation.state(), fortress, 0), 2);
    assert!(rules
        .hero_progress(simulation.state(), fortress, HERO)
        .is_some_and(|f| f > 0.0));
    assert!(rules
        .build_progress(simulation.state(), fortress, 0)
        .is_some_and(|f| f > 0.0));
    assert_eq!(
        crate::hud::production_count(
            rules,
            simulation.state(),
            fortress,
            &crate::command_bar::SlotAction::Revive(HERO),
        ),
        0,
        "recruitable once, like an upgrade: no queued count beside the clock"
    );
    run(&mut simulation, 300);
    assert_eq!(placed(&simulation, ids, &m, "Hero").len(), 1);
    assert_eq!(
        placed(&simulation, ids, &m, "Scout").len(),
        0,
        "not queued behind the first hero, but still held by the revival cooldown"
    );
    run(&mut simulation, 120);
    assert_eq!(
        placed(&simulation, ids, &m, "Scout").len(),
        1,
        "out once the cooldown since the first hero's exit has passed"
    );
}

#[test]
fn a_builder_goes_ahead_of_a_unit_already_building() {
    let (_m, mut simulation, ids, fortress) = fortress_match(10_000);
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::ObjectUpgrade { button: 0 },
    );
    run(&mut simulation, 35);
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::QueueUnit { button: 1 },
    );
    run(&mut simulation, 10);
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::QueueUnit { button: 0 },
    );
    let queue = queue_of(&simulation, ids, fortress);
    assert_eq!(queue.len(), 2, "{queue:?}");
    let elite_done = queue[0].1;
    assert!(elite_done > 0, "the elite had started: {queue:?}");
    run(&mut simulation, 5);
    let later = queue_of(&simulation, ids, fortress);
    assert!(later[1].1 > 0, "the builder runs: {later:?}");
    assert_eq!(later[0].1, elite_done, "the elite waits: {later:?}");
}

#[test]
fn a_right_click_takes_back_the_buttons_last_entry() {
    let (_m, mut simulation, ids, fortress) = fortress_match(10_000);
    let porter = RotwkCommand::QueueUnit { button: 0 };
    press(&mut simulation, PLAYER, fortress, porter.clone());
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::RecruitHero { button: HERO },
    );
    press(&mut simulation, PLAYER, fortress, porter);
    let before = queue_of(&simulation, ids, fortress);
    assert_eq!(before.len(), 3);
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::CancelBuild { button: 0 },
    );
    let after: Vec<u32> = queue_of(&simulation, ids, fortress)
        .into_iter()
        .map(|(item, _)| item)
        .collect();
    assert_eq!(
        after,
        vec![before[0].0, before[1].0],
        "the last porter went"
    );
    assert_eq!(purse(&simulation, ids, PLAYER), 10_000 - 100 - 1500);
    assert!(
        simulation
            .rules()
            .build_progress(simulation.state(), fortress, 0)
            .is_some_and(|f| f > 0.0),
        "the front porter still shows its clock"
    );
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::CancelHero { button: HERO },
    );
    assert_eq!(queue_of(&simulation, ids, fortress).len(), 1);
    assert_eq!(state(&simulation, ids, 1), HeroState::Available);
    assert_eq!(purse(&simulation, ids, PLAYER), 10_000 - 100);
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::CancelHero { button: HERO },
    );
    assert_eq!(
        queue_of(&simulation, ids, fortress).len(),
        1,
        "nothing of the hero's left to cancel"
    );
}

#[test]
fn upgrades_research_one_at_a_time_in_order() {
    let (_m, mut simulation, _ids, fortress) = fortress_match(10_000);
    for button in [0, 1] {
        press(
            &mut simulation,
            PLAYER,
            fortress,
            RotwkCommand::ObjectUpgrade { button },
        );
    }
    run(&mut simulation, 5);
    let rules = simulation.rules();
    let world = simulation.state();
    assert!(rules
        .object_upgrade_progress(world, fortress, 0)
        .is_some_and(|f| f > 0.0));
    assert_eq!(rules.object_upgrade_progress(world, fortress, 1), Some(0.0));
    assert_eq!(rules.object_upgrade_progress(world, fortress, 3), None);
    let researching = rules.upgrade_names_in_production(world, fortress);
    assert_eq!(researching, ["upgrade_banners", "upgrade_healing"]);
}

fn rank(simulation: &Simulation<RotwkRules>, e: Entity) -> Option<u16> {
    simulation.rules().rank_of(simulation.state(), e)
}

fn a_dead_hero(m: &Match) -> (Simulation<RotwkRules>, Ids, Entity) {
    let placed_units = [
        ("Fortress", 200.0, 200.0, PLAYER),
        ("Target", 380.0, 240.0, OTHER),
        ("Target", 382.0, 255.0, OTHER),
        ("Troll", 800.0, 800.0, OTHER),
    ];
    let mut simulation = Simulation::new(22, rules(m, &placed_units, 20_000)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let fortress = placed(&simulation, ids, m, "Fortress")[0];
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::ObjectUpgrade { button: 1 },
    );
    run(&mut simulation, 35);
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::RecruitHero { button: HERO },
    );
    run(&mut simulation, 305);
    let hero = placed(&simulation, ids, m, "Hero")[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![hero],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::MoveTo {
                x: 300.0,
                y: 245.0,
                queue: false,
            },
        )
        .unwrap();
    for _ in 0..600 {
        run(&mut simulation, 1);
        if placed(&simulation, ids, m, "Target").is_empty() {
            break;
        }
    }
    assert_eq!(rank(&simulation, hero), Some(2));
    assert!(simulation
        .rules()
        .power_released(simulation.state(), hero, "SpecialAbilityHorn"));
    let points = simulation
        .rules()
        .experience_of(simulation.state(), ids, hero)
        .unwrap()
        .1;
    assert_eq!(points, 7);
    kill(&mut simulation, ids, hero);
    assert_eq!(state(&simulation, ids, 1), HeroState::Dead { points: 7 });
    (simulation, ids, fortress)
}

#[test]
fn a_dead_hero_is_revived_at_its_respawn_price_with_its_experience() {
    let m = load();
    let (mut simulation, ids, fortress) = a_dead_hero(&m);
    let page = simulation
        .rules()
        .hero_page(simulation.state(), PLAYER, Some(fortress));
    assert!(!page[1].fielded, "a dead hero is back on the page");
    assert_eq!(page[1].cost, Some(600), "750 x 0.8, not the any-level 625");

    let before = purse(&simulation, ids, PLAYER);
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::RecruitHero { button: HERO },
    );
    assert_eq!(before - purse(&simulation, ids, PLAYER), 600);
    assert_eq!(
        last_entry(&simulation, ids, fortress),
        Some((600, 60)),
        "3 s x 0.8 truncates to 2 s"
    );
    assert!(matches!(
        state(&simulation, ids, 1),
        HeroState::Queued {
            revive: Some(7),
            ..
        }
    ));
    run(&mut simulation, 80);
    let revived = placed(&simulation, ids, &m, "Hero");
    assert_eq!(revived.len(), 1);
    let revived = revived[0];
    assert_eq!(state(&simulation, ids, 1), HeroState::Alive(revived));
    assert_eq!(
        rank(&simulation, revived),
        Some(2),
        "its experience came back"
    );
    let health = simulation.state().get(ids.health, revived).unwrap().0;
    assert_eq!((health.current(), health.max()), (848.0, 1060.0));
    assert!(simulation
        .rules()
        .power_released(simulation.state(), revived, "SpecialAbilityHorn"));
}

#[test]
fn a_revived_hero_exits_at_the_castle_keep_not_the_queuing_structure() {
    let m = load();
    let placed_units = [
        ("Fortress", 200.0, 200.0, PLAYER),
        ("Camp", 800.0, 800.0, PLAYER),
    ];
    let mut simulation = Simulation::new(23, rules(&m, &placed_units, 20_000)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let fortress = placed(&simulation, ids, &m, "Fortress")[0];
    let camp = placed(&simulation, ids, &m, "Camp")[0];
    press(
        &mut simulation,
        PLAYER,
        camp,
        RotwkCommand::RecruitHero { button: HERO },
    );
    run(&mut simulation, 305);
    let first = placed(&simulation, ids, &m, "Hero")[0];
    assert!(
        position(&simulation, ids, first)
            .minus(position(&simulation, ids, camp))
            .length()
            < position(&simulation, ids, first)
                .minus(position(&simulation, ids, fortress))
                .length(),
        "a first recruit exits at the structure it was queued at"
    );
    press(&mut simulation, PLAYER, first, RotwkCommand::DebugKill);
    assert!(matches!(state(&simulation, ids, 1), HeroState::Dead { .. }));
    press(
        &mut simulation,
        PLAYER,
        camp,
        RotwkCommand::RecruitHero { button: HERO },
    );
    assert!(matches!(
        state(&simulation, ids, 1),
        HeroState::Queued { structure, revive: Some(_) } if structure == camp
    ));
    run(&mut simulation, 130);
    let revived = placed(&simulation, ids, &m, "Hero");
    assert_eq!(revived.len(), 1);
    let revived = revived[0];
    assert_eq!(state(&simulation, ids, 1), HeroState::Alive(revived));
    let at = position(&simulation, ids, revived);
    assert!(
        at.minus(position(&simulation, ids, fortress)).length()
            < at.minus(position(&simulation, ids, camp)).length(),
        "revived at ({}, {}), fortress at {:?}, camp at {:?}",
        at.x,
        at.y,
        position(&simulation, ids, fortress),
        position(&simulation, ids, camp)
    );
}

#[test]
fn a_revive_whose_fortress_falls_leaves_the_hero_dead_with_its_experience() {
    let m = load();
    let (mut simulation, ids, fortress) = a_dead_hero(&m);
    let health = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(ids.health, fortress)
            .map_or(0.0, |health| health.0.current())
    };
    let troll = placed(&simulation, ids, &m, "Troll")[0];
    for command in [
        RotwkCommand::SetSelection {
            replace: true,
            units: vec![troll],
        },
        RotwkCommand::AttackObject { target: fortress },
    ] {
        simulation.submit(OTHER, command).unwrap();
    }
    let full = health(&simulation);
    let mut blow = 0.0;
    for _ in 0..600 {
        run(&mut simulation, 1);
        let now = health(&simulation);
        if blow == 0.0 && now < full {
            blow = full - now;
        }
        if blow > 0.0 && now <= blow {
            break;
        }
    }
    assert!(health(&simulation) > 0.0 && health(&simulation) <= blow);
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::RecruitHero { button: HERO },
    );
    assert!(matches!(
        state(&simulation, ids, 1),
        HeroState::Queued {
            revive: Some(7),
            ..
        }
    ));
    kill(&mut simulation, ids, fortress);
    assert_eq!(state(&simulation, ids, 1), HeroState::Dead { points: 7 });
}

#[test]
fn another_players_fortress_takes_no_hero_press() {
    let m = load();
    let placed_units = [
        ("Fortress", 200.0, 200.0, PLAYER),
        ("Fortress", 600.0, 600.0, OTHER),
    ];
    let simulation = Simulation::new(25, rules(&m, &placed_units, 10_000)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let theirs = placed(&simulation, ids, &m, "Fortress")
        .into_iter()
        .find(|e| simulation.state().get(ids.owner, *e).map(|o| o.0) == Some(OTHER))
        .unwrap();
    assert_eq!(
        simulation
            .rules()
            .hero_order(simulation.state(), ids, PLAYER, theirs, HERO),
        Err(crate::heroes::HeroRefusal::NotOwned)
    );
    let page = simulation
        .rules()
        .hero_page(simulation.state(), PLAYER, Some(theirs));
    assert_eq!(page[1].cost, None);
}

pub(super) fn kill(simulation: &mut Simulation<RotwkRules>, ids: Ids, victim: Entity) {
    let troll = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .find(|(entity, _)| {
            simulation
                .state()
                .get(ids.owner, *entity)
                .is_some_and(|owner| owner.0 == OTHER)
                && simulation.state().contains(ids.armed, *entity)
        })
        .map(|(entity, _)| entity);
    let troll = troll.expect("a troll placed for the purpose");
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![troll],
            },
        )
        .unwrap();
    simulation
        .submit(OTHER, RotwkCommand::AttackObject { target: victim })
        .unwrap();
    for _ in 0..600 {
        run(simulation, 1);
        if !simulation.state().is_live(victim) || simulation.state().contains(ids.doomed, victim) {
            break;
        }
    }
    assert!(
        !simulation.state().is_live(victim) || simulation.state().contains(ids.doomed, victim),
        "the troll killed it"
    );
    run(simulation, 1);
}

fn run_collect_fx(simulation: &mut Simulation<RotwkRules>, ticks: u32, seen: &mut Vec<String>) {
    for _ in 0..ticks {
        let frame = simulation.tick().unwrap().frame;
        let rules = simulation.rules();
        for event in rules.fx_events(simulation.state(), frame) {
            if let Some(name) = rules.fx_name(event.list) {
                if name.starts_with("FX_Hero") {
                    seen.push(name.to_owned());
                }
            }
        }
    }
}

fn kill_collect_fx(
    simulation: &mut Simulation<RotwkRules>,
    ids: Ids,
    victim: Entity,
    seen: &mut Vec<String>,
) {
    let troll = simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .find(|(entity, _)| {
            simulation
                .state()
                .get(ids.owner, *entity)
                .is_some_and(|owner| owner.0 == OTHER)
                && simulation.state().contains(ids.armed, *entity)
        })
        .map(|(entity, _)| entity);
    let troll = troll.expect("a troll placed for the purpose");
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![troll],
            },
        )
        .unwrap();
    simulation
        .submit(OTHER, RotwkCommand::AttackObject { target: victim })
        .unwrap();
    for _ in 0..600 {
        run_collect_fx(simulation, 1, seen);
        if !simulation.state().is_live(victim) || simulation.state().contains(ids.doomed, victim) {
            break;
        }
    }
    assert!(
        !simulation.state().is_live(victim) || simulation.state().contains(ids.doomed, victim),
        "the troll killed it"
    );
    run_collect_fx(simulation, 1, seen);
}

fn run_until_placed(
    simulation: &mut Simulation<RotwkRules>,
    ids: Ids,
    m: &Match,
    name: &str,
    before: &[Entity],
    seen: &mut Vec<String>,
) -> Entity {
    for _ in 0..600 {
        run_collect_fx(simulation, 1, seen);
        if let Some(entity) = placed(simulation, ids, m, name)
            .into_iter()
            .find(|entity| !before.contains(entity))
        {
            return entity;
        }
    }
    panic!("{name} was never produced");
}

#[test]
fn respawn_update_fx_play_at_production_death_and_revive() {
    let m = load();
    let placed_units = [
        ("Fortress", 200.0, 200.0, PLAYER),
        ("Troll", 800.0, 800.0, OTHER),
    ];
    let mut simulation = Simulation::new(24, rules(&m, &placed_units, 20_000)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let fortress = placed(&simulation, ids, &m, "Fortress")[0];
    let mut seen = Vec::new();

    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::RecruitHero { button: HERO },
    );
    let hero = run_until_placed(&mut simulation, ids, &m, "Hero", &[], &mut seen);
    assert_eq!(
        seen,
        vec!["FX_HeroInitialSpawn".to_owned()],
        "first production"
    );
    seen.clear();

    kill_collect_fx(&mut simulation, ids, hero, &mut seen);
    assert!(matches!(state(&simulation, ids, 1), HeroState::Dead { .. }));
    assert_eq!(seen, vec!["FX_HeroDeath".to_owned()], "first death");
    seen.clear();

    let mut old = vec![hero];
    for cycle in 0..2 {
        press(
            &mut simulation,
            PLAYER,
            fortress,
            RotwkCommand::RecruitHero { button: HERO },
        );
        let revived = run_until_placed(&mut simulation, ids, &m, "Hero", &old, &mut seen);
        old.push(revived);
        assert_eq!(
            seen,
            vec!["FX_HeroRespawn".to_owned()],
            "revive {cycle}: RespawnFX once, not InitialSpawnFX"
        );
        seen.clear();
        kill_collect_fx(&mut simulation, ids, revived, &mut seen);
        assert_eq!(seen, vec!["FX_HeroDeath".to_owned()], "death {cycle}");
        seen.clear();
    }
}

#[test]
fn a_unit_button_needing_an_upgrade_is_refused_until_it_is_held() {
    let (_m, mut simulation, ids, fortress) = fortress_match(10_000);
    let queued = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(ids.producer, fortress)
            .map_or(0, |producer| producer.queue.entries().len())
    };
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::QueueUnit { button: 1 },
    );
    assert_eq!(queued(&simulation), 0, "refused before the banners");
    assert_eq!(purse(&simulation, ids, PLAYER), 10_000);
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::ObjectUpgrade { button: 0 },
    );
    run(&mut simulation, 35);
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::QueueUnit { button: 1 },
    );
    assert_eq!(queued(&simulation), 1, "queued once they are held");
}

#[test]
fn an_object_upgrade_nothing_reacts_to_is_refused() {
    let (m, mut simulation, ids, fortress) = fortress_match(10_000);
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::ObjectUpgrade { button: 2 },
    );
    assert_eq!(purse(&simulation, ids, PLAYER), 10_000, "walls refused");
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::ObjectUpgrade { button: 0 },
    );
    assert_eq!(purse(&simulation, ids, PLAYER), 9_900, "banners queued");
    run(&mut simulation, 35);
    let bit = m.data.upgrade_bits.bit("Upgrade_Banners").unwrap();
    assert!(simulation
        .state()
        .get(ids.object_upgrades, fortress)
        .is_some_and(|held| held.0.contains(bit)));
}

#[test]
fn an_upgrade_button_needing_an_upgrade_is_refused_until_it_is_held() {
    let (_m, mut simulation, ids, fortress) = fortress_match(10_000);
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::ObjectUpgrade { button: 3 },
    );
    assert_eq!(
        purse(&simulation, ids, PLAYER),
        10_000,
        "refused before the healing"
    );
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::ObjectUpgrade { button: 1 },
    );
    run(&mut simulation, 35);
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::ObjectUpgrade { button: 3 },
    );
    assert_eq!(
        purse(&simulation, ids, PLAYER),
        9_800,
        "queued once it is held"
    );
}

#[test]
fn an_upgrade_the_object_holds_is_refused_before_any_gate() {
    let (_m, mut simulation, ids, fortress) = fortress_match(10_000);
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::ObjectUpgrade { button: 0 },
    );
    run(&mut simulation, 35);
    let rules = simulation.rules();
    let open = |_: &World, _: Entity, _: u16, _: u8| None;
    let verdict = crate::economy::upgrade_check(
        simulation.state(),
        RotwkRules::economy_ids(ids),
        &rules.kinds,
        &rules.upgrade_bits,
        PLAYER,
        fortress,
        0,
        false,
        &rules.object_upgrade_menus,
        &rules.upgrade_buildables,
        &open,
        rules.production_cost_change_percent(PLAYER),
        rules.brutal_cheats_for(PLAYER).build_cost_reduction,
        &|world, builder, bit| rules.sub_upgrade_counted(world, ids, builder, bit),
    );
    assert_eq!(verdict.err().as_deref(), Some("already holds this upgrade"));
}

#[test]
fn a_power_button_needing_an_upgrade_is_refused_until_it_is_held() {
    let (_m, mut simulation, ids, fortress) = fortress_match(10_000);
    let fire = RotwkCommand::FireSpecialPower {
        button: 0,
        target: None,
        at: None,
    };
    let recharging = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(ids.power_book, fortress)
            .is_some_and(|book| book.0.iter().any(|p| p.available_on > Frame::ZERO))
    };
    press(&mut simulation, PLAYER, fortress, fire.clone());
    assert!(!recharging(&simulation), "refused before the banners");
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::ObjectUpgrade { button: 0 },
    );
    run(&mut simulation, 35);
    press(&mut simulation, PLAYER, fortress, fire);
    assert!(recharging(&simulation), "fired once they are held");
}

#[test]
fn reset_powers_makes_a_recharging_power_ready() {
    let (_m, mut simulation, ids, fortress) = fortress_match(10_000);
    let recharging = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(ids.power_book, fortress)
            .is_some_and(|book| book.0.iter().any(|p| p.available_on > Frame::ZERO))
    };
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::ObjectUpgrade { button: 0 },
    );
    run(&mut simulation, 35);
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::FireSpecialPower {
            button: 0,
            target: None,
            at: None,
        },
    );
    assert!(recharging(&simulation));
    simulation
        .submit(PLAYER, RotwkCommand::Debug(DebugCommand::ResetPowers))
        .unwrap();
    run(&mut simulation, 1);
    assert!(!recharging(&simulation));
}

#[test]
fn refused_presses_change_nothing_including_the_hash() {
    let build = || {
        let (m, mut simulation, ids, fortress) = fortress_match(10_000);
        press(
            &mut simulation,
            PLAYER,
            fortress,
            RotwkCommand::RecruitHero { button: HERO },
        );
        (m, simulation, ids, fortress)
    };
    let (_m, mut sent, _, fortress) = build();
    let (_m2, mut quiet, _, quiet_fortress) = build();
    for command in [
        RotwkCommand::RecruitHero { button: RING },
        RotwkCommand::RecruitHero { button: HERO },
        RotwkCommand::ObjectUpgrade { button: 2 },
        RotwkCommand::ObjectUpgrade { button: 3 },
        RotwkCommand::QueueUnit { button: 1 },
        RotwkCommand::RecruitHero { button: 9 },
    ] {
        press(&mut sent, PLAYER, fortress, command);
        quiet
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![quiet_fortress],
                },
            )
            .unwrap();
        run(&mut quiet, 1);
    }
    assert_eq!(sent.state_hash(), quiet.state_hash());
}

#[test]
fn the_ai_never_chooses_an_upgrade_it_holds_or_cannot_research() {
    let (_m, mut simulation, ids, fortress) = fortress_match(10_000);
    let choice = |simulation: &Simulation<RotwkRules>| {
        simulation
            .rules()
            .ai_upgrade_choice(simulation.state(), ids, PLAYER)
    };
    assert_eq!(choice(&simulation), Some((fortress, 0, false)));
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::ObjectUpgrade { button: 0 },
    );
    assert_eq!(
        choice(&simulation),
        Some((fortress, 1, false)),
        "banners in research: not chosen again; the tower needs the healing"
    );
    run(&mut simulation, 35);
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::ObjectUpgrade { button: 1 },
    );
    run(&mut simulation, 35);
    assert_eq!(
        choice(&simulation),
        Some((fortress, 3, false)),
        "the healing held: the tower"
    );
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::ObjectUpgrade { button: 3 },
    );
    run(&mut simulation, 35);
    assert_eq!(
        choice(&simulation),
        None,
        "everything held, and the walls refused"
    );
}

fn ai_match(
    m: &Match,
    order: Vec<Option<crate::unit::Kind>>,
    save: f32,
    cash: i64,
) -> Simulation<RotwkRules> {
    let plan = crate::skirmish_ai::ArmyPlan {
        player: PLAYER,
        members: Vec::new(),
        must_use_command_point_percentage: [0.0; 3],
        percent_to_save: [save; 3],
        phase_duration_rush: 0.0,
        phase_duration_mid_game: 0.0,
        hero_build_order: order,
        porter_kind: None,
        porter_target: 0,
    };
    let seed = crate::skirmish_ai::AiSeed {
        player: PLAYER,
        candidates: Vec::new(),
    };
    let placed_units = [
        ("Fortress", 200.0, 200.0, PLAYER),
        ("Troll", 800.0, 800.0, OTHER),
    ];
    Simulation::new(
        23,
        rules(m, &placed_units, cash).with_skirmish_ai(
            vec![seed],
            crate::combat::AiTiming::default(),
            vec![plan],
        ),
    )
    .unwrap()
}

#[test]
fn an_ai_player_recruits_and_revives_its_hero() {
    let m = load();
    let hero = m.kinds.kind("Hero").unwrap();
    let mut simulation = ai_match(&m, vec![Some(hero)], 0.0, 10_000);
    let ids = simulation.rules().ids().unwrap();
    run(&mut simulation, 5);
    assert!(
        matches!(
            state(&simulation, ids, 1),
            HeroState::Queued { revive: None, .. }
        ),
        "recruited from the hero page: {:?}",
        state(&simulation, ids, 1)
    );
    run(&mut simulation, 310);
    let fielded = placed(&simulation, ids, &m, "Hero");
    assert_eq!(fielded.len(), 1);
    kill(&mut simulation, ids, fielded[0]);
    assert!(matches!(state(&simulation, ids, 1), HeroState::Dead { .. }));
    run(&mut simulation, 70);
    assert!(
        matches!(
            state(&simulation, ids, 1),
            HeroState::Queued {
                revive: Some(_),
                ..
            } | HeroState::Alive(_)
        ),
        "revived: {:?}",
        state(&simulation, ids, 1)
    );
}

#[test]
fn an_ai_player_holds_back_its_savings_from_a_hero() {
    let m = load();
    let hero = m.kinds.kind("Hero").unwrap();
    let mut simulation = ai_match(&m, vec![Some(hero)], 0.9, 10_000);
    let ids = simulation.rules().ids().unwrap();
    run(&mut simulation, 5);
    assert_eq!(state(&simulation, ids, 1), HeroState::Available);
}

#[test]
fn an_ai_player_holds_back_its_savings_from_a_revive() {
    let m = load();
    let mut simulation = ai_match(&m, Vec::new(), 0.95, 10_000);
    let ids = simulation.rules().ids().unwrap();
    let fortress = placed(&simulation, ids, &m, "Fortress")[0];
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::RecruitHero { button: HERO },
    );
    run(&mut simulation, 305);
    let hero = placed(&simulation, ids, &m, "Hero")[0];
    kill(&mut simulation, ids, hero);
    run(&mut simulation, 70);
    assert!(
        matches!(state(&simulation, ids, 1), HeroState::Dead { .. }),
        "{:?}",
        state(&simulation, ids, 1)
    );
}

#[test]
fn an_ai_player_waits_on_the_hero_it_cannot_afford_yet() {
    let m = load();
    let hero = m.kinds.kind("Hero").unwrap();
    let scout = m.kinds.kind("Scout").unwrap();
    let mut simulation = ai_match(&m, vec![Some(hero), Some(scout)], 0.0, 1_000);
    let ids = simulation.rules().ids().unwrap();
    run(&mut simulation, 5);
    assert_eq!(state(&simulation, ids, 1), HeroState::Available);
    assert_eq!(state(&simulation, ids, 2), HeroState::Available);
}

#[test]
fn an_ai_player_passes_over_a_hero_it_cannot_recruit_yet() {
    let m = load();
    let ring = m.kinds.kind("RingHero").unwrap();
    let hero = m.kinds.kind("Hero").unwrap();
    let mut simulation = ai_match(&m, vec![None, Some(ring), Some(hero)], 0.0, 10_000);
    let ids = simulation.rules().ids().unwrap();
    run(&mut simulation, 5);
    assert_eq!(state(&simulation, ids, 0), HeroState::Available);
    assert!(
        matches!(
            state(&simulation, ids, 1),
            HeroState::Queued { revive: None, .. }
        ),
        "{:?}",
        state(&simulation, ids, 1)
    );
    let world = simulation.state();
    let player_entity =
        crate::economy::player_entity(world, RotwkRules::economy_ids(ids), PLAYER).unwrap();
    assert_eq!(
        world
            .get(ids.ai_army_progress, player_entity)
            .map(|progress| progress.hero_index),
        Some(1),
        "past the unnamed entry for good; the ring hero stays at the head"
    );
}

#[test]
fn a_match_with_a_recruit_a_death_and_a_revive_replays_identically() {
    let m = load();
    let placed_units = [
        ("Fortress", 200.0, 200.0, PLAYER),
        ("Troll", 800.0, 800.0, OTHER),
    ];
    let mut simulation = Simulation::new(24, rules(&m, &placed_units, 10_000)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let fortress = placed(&simulation, ids, &m, "Fortress")[0];
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::RecruitHero { button: HERO },
    );
    run(&mut simulation, 305);
    let hero = placed(&simulation, ids, &m, "Hero")[0];
    kill(&mut simulation, ids, hero);
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::RecruitHero { button: HERO },
    );
    run(&mut simulation, 100);
    assert!(
        matches!(state(&simulation, ids, 1), HeroState::Alive(_)),
        "the recording must contain the revive it claims to test"
    );
    let final_hash = simulation.state_hash();
    let recording = simulation.into_recording();
    let verified = recording.verify(rules(&m, &placed_units, 10_000)).unwrap();
    assert_eq!(verified.final_hash, final_hash);
}

#[test]
fn an_upgrade_price_takes_brutal_once_and_sums_the_counted_sub_upgrades() {
    let (_m, simulation, ids, fortress) = fortress_match(10_000);
    let rules = simulation.rules();
    let mut upgrade = rules.upgrade_buildables[0].clone();
    upgrade.cost = 300;
    upgrade.subs = Vec::new();
    let price = |upgrade: &crate::economy::UpgradeBuildable, brutal: f32, only: Option<usize>| {
        let counted = |_: &World, _: Option<Entity>, bit: mint_sim::UpgradeBit| {
            only.is_none_or(|index| upgrade.subs.get(index).is_some_and(|sub| sub.bit == bit))
        };
        crate::economy::upgrade_cost(
            simulation.state(),
            RotwkRules::economy_ids(ids),
            &rules.kinds,
            &rules.upgrade_bits,
            PLAYER,
            upgrade,
            Some(fortress),
            0.0,
            brutal,
            &counted,
        )
    };
    assert_eq!(price(&upgrade, 0.15, None), 255);
    assert_eq!(price(&upgrade, 0.0, None), 300);
    let first = upgrade.bit;
    let second = rules.upgrade_buildables[1].bit;
    upgrade.subs = vec![
        crate::economy::SubUpgrade {
            bit: first,
            cost: 100,
        },
        crate::economy::SubUpgrade {
            bit: second,
            cost: 50,
        },
    ];
    assert_eq!(price(&upgrade, 0.0, None), 150, "every sub-upgrade");
    assert_eq!(
        price(&upgrade, 0.0, Some(0)),
        100,
        "only the first is taken"
    );
}

#[test]
fn a_revived_hero_keeps_the_army_it_died_with() {
    use crate::rules::wotr_battle::{WotrSeat, WotrStart, WotrStructure};
    let m = load();
    let plot = |template: &str, x: f32, army| WotrStructure {
        template: template.to_owned(),
        army,
        at: Vec2::new(x, x),
    };
    let start = WotrStart {
        seats: vec![WotrSeat {
            player: PLAYER,
            armies: Vec::new(),
            structures: vec![plot("Fortress", 200.0, 5), plot("Camp", 800.0, 9)],
            modifiers: None,
            purchase_points: 0,
            skill_points: 0.0,
            sciences: Vec::new(),
            upgrades: Vec::new(),
            fallen: Vec::new(),
        }],
        skill_scalars: Vec::new(),
        revival_multipliers: (1.0, 1.0),
    };
    let rules = rules(&m, &[], 20_000).with_map_scripts(
        Some(super::wotr_battle_tests::engine(start)),
        vec![(0, PLAYER)],
    );
    let mut simulation = Simulation::new(24, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    run(&mut simulation, 2);
    let fortress = placed(&simulation, ids, &m, "Fortress")[0];
    let camp = placed(&simulation, ids, &m, "Camp")[0];
    let army_of = |simulation: &Simulation<RotwkRules>, entity| {
        simulation
            .rules()
            .wotr_run(simulation.state())
            .unwrap()
            .army_of(entity)
    };
    assert_eq!(army_of(&simulation, fortress), Some(5));
    assert_eq!(army_of(&simulation, camp), Some(9));
    press(
        &mut simulation,
        PLAYER,
        fortress,
        RotwkCommand::RecruitHero { button: HERO },
    );
    run(&mut simulation, 305);
    let first = placed(&simulation, ids, &m, "Hero")[0];
    assert_eq!(army_of(&simulation, first), Some(5));
    press(&mut simulation, PLAYER, first, RotwkCommand::DebugKill);
    assert!(matches!(state(&simulation, ids, 1), HeroState::Dead { .. }));
    press(
        &mut simulation,
        PLAYER,
        camp,
        RotwkCommand::RecruitHero { button: HERO },
    );
    run(&mut simulation, 130);
    let revived = placed(&simulation, ids, &m, "Hero");
    assert_eq!(revived.len(), 1);
    assert_eq!(army_of(&simulation, revived[0]), Some(5));
}

#[test]
fn a_campaign_fallen_hero_is_offered_for_revival_at_its_seeded_price() {
    use crate::rules::wotr_battle::{WotrSeat, WotrStart, WotrStructure};
    let m = load();
    let start = WotrStart {
        seats: vec![WotrSeat {
            player: PLAYER,
            armies: Vec::new(),
            structures: vec![WotrStructure {
                template: "Fortress".to_owned(),
                army: 5,
                at: Vec2::new(200.0, 200.0),
            }],
            modifiers: None,
            purchase_points: 0,
            skill_points: 0.0,
            sciences: Vec::new(),
            upgrades: Vec::new(),
            fallen: vec![("Hero".to_owned(), 2, 7)],
        }],
        skill_scalars: Vec::new(),
        revival_multipliers: (2.0, 3.0),
    };
    let rules = rules(&m, &[], 20_000).with_map_scripts(
        Some(super::wotr_battle_tests::engine(start)),
        vec![(0, PLAYER)],
    );
    let mut simulation = Simulation::new(24, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    run(&mut simulation, 2);
    assert_eq!(state(&simulation, ids, 1), HeroState::Dead { points: 7 });
    let fortress = placed(&simulation, ids, &m, "Fortress")[0];
    let page = simulation
        .rules()
        .hero_page(simulation.state(), PLAYER, Some(fortress));
    assert_eq!(page[1].cost, Some(1500), "750 x 2");
}
