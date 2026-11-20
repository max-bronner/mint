use super::*;

const DATA: &str = "\
#define WELL_RADIUS 100\n\
GameData\n  GoodCommandPointsMP2 = 100 280\n  EvilCommandPointsMP2 = 100 280\nEnd\n\
Science SCIENCE_MEN\n  PrerequisiteSciences = None\n  SciencePurchasePointCost = 0\n  IsGrantable = No\nEnd\n\
Rank 1\n  SkillPointsNeededDefault = 0\n  SkillPointsNeededCampaign = 0\n  SciencePurchasePointsGranted = 5\nEnd\n\
Upgrade Upgrade_Gift\n  Type = PLAYER\nEnd\n\
SpecialPower SpellHurt\n  ReloadTime = 0\nEnd\n\
SpecialPower SpellRebuild\n  ReloadTime = 0\nEnd\n\
SpecialPower SpellSnow\n  ReloadTime = 0\nEnd\n\
SpecialPower SpellReach\n  ReloadTime = 0\nEnd\n\
SpecialPower SpellOutlaw\n  ReloadTime = 0\nEnd\n\
SpecialPower SpellGift\n  ReloadTime = 0\nEnd\n\
SpecialPower SpellFarm\n  ReloadTime = 0\nEnd\n\
SpecialPower SpellInfantryHeal\n  ReloadTime = 0\nEnd\n\
ModifierList RebuildHeal\n  Category = SPELL\n  Modifier = AUTO_HEAL 50\n  Duration = 120000\nEnd\n\
ModifierList InfantryHeal\n  Category = SPELL\n  Modifier = AUTO_HEAL 1\n  Duration = 120000\nEnd\n\
ModifierList Snowbind\n  Category = SPELL\n  Modifier = PRODUCTION 1%\n  \
  Modifier = INVULNERABLE 0% SLASH PIERCE\n  Duration = 10000\nEnd\n\
ModifierList Reach\n  Category = BUFF\n  Modifier = RANGE 60%\n  Duration = 10000\nEnd\n\
ModifierList Outlaw\n  Category = WEAPON\n  Modifier = BOUNTY_PERCENTAGE 100.0%\n  Duration = 10000\nEnd\n\
ModifierList FarmLevel\n  Category = STRUCTURE\n  Modifier = COMMAND_POINT_BONUS 25\n  Duration = 0\nEnd\n\
ObjectCreationList OCL_Hurt\n  CreateObject\n    ObjectNames = Hurt\n  End\nEnd\n\
Armor Plain\n  Armor = DEFAULT 100%\nEnd\n\
Weapon HurtWeapon\n  RadiusDamageAffects = ENEMIES\n  \
  DamageNugget\n    Damage = 60\n    Radius = 60\n    DamageType = FLAME\n  End\nEnd\n\
Weapon Sword\n  AttackRange = 30\n  DelayBetweenShots = 1000\n  MeleeWeapon = Yes\n  \
  DamageNugget\n    Damage = 40\n    DamageType = SLASH\n  End\nEnd\n\
Weapon Torch\n  AttackRange = 30\n  DelayBetweenShots = 1000\n  MeleeWeapon = Yes\n  \
  DamageNugget\n    Damage = 40\n    DamageType = FLAME\n  End\nEnd\n\
Weapon Sling\n  AttackRange = 100\n  DelayBetweenShots = 1000\n  AcceptableAimDelta = 360\n  \
  DamageNugget\n    Damage = 1\n    DamageType = SLASH\n  End\nEnd\n\
Object Hurt\n  KindOf = NO_COLLIDE IMMOBILE UNATTACKABLE INERT\n  \
  Behavior = DeletionUpdate Tag_Del\n    MinLifetime = 100\n    MaxLifetime = 100\n  End\n  \
  Behavior = FireWeaponUpdate Tag_Fire\n    \
    FireWeaponNugget\n      WeaponName = HurtWeapon\n      FireDelay = 0\n      OneShot = Yes\n    End\n  \
  End\nEnd\n\
Object Hero\n  Side = Men\n  KindOf = INFANTRY HERO SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 500\n  End\n  \
  Behavior = AutoHealBehavior Tag_Heal\n    StartsActive = Yes\n    HealingAmount = 30\n    \
    HealingDelay = 1000\n    StartHealingDelay = 3000\n    HealOnlyIfNotInCombat = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Elf\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 200\n  End\n  \
  Behavior = AutoHealBehavior Tag_Gift\n    StartsActive = No\n    TriggeredBy = Upgrade_Gift\n    \
    HealingAmount = 5\n    HealingDelay = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Well\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = AutoHealBehavior Tag_Well\n    StartsActive = Yes\n    HealingAmount = 10\n    \
    HealingDelay = 500\n    Radius = WELL_RADIUS\n    KindOf = INFANTRY\n    UnitHealPulseFX = FX_WellHeal\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 20\nEnd\n\
Object HealingPost\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = PassiveAreaEffectBehavior Tag_Post\n    EffectRadius = 100\n    PingDelay = 1000\n    \
    HealPercentPerSecond = 10%\n    AllowFilter = ANY +INFANTRY +STRUCTURE\n    HealFX = FX_PostHeal\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 20\nEnd\n\
Object HealingPostSlow\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = PassiveAreaEffectBehavior Tag_PostSlow\n    EffectRadius = 100\n    PingDelay = 5000\n    \
    HealPercentPerSecond = 50%\n    AllowFilter = ANY +INFANTRY\n    NonStackable = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 20\nEnd\n\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  BountyValue = 4\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Veteran\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Behavior = AutoHealBehavior Tag_Self\n    StartsActive = Yes\n    HealingAmount = 1\n    \
    HealingDelay = 400\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Pikeman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Rider\n  Side = Men\n  KindOf = CAVALRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Swordsman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Sword\n  End\n  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Torchbearer\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Torch\n  End\n  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Slinger\n  Side = Men\n  KindOf = INFANTRY SELECTABLE IMMOBILE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Sling\n  End\n  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Barracks\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 3000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
Object Fortress\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE COMMANDCENTER\n  \
  CommandPointBonus = 100\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 5000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 40\nEnd\n\
Object Farm\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandPointBonus = 50\n  \
  CommandSet = FarmSet\n  BuildCost = 200\n  BuildTime = 1\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 50\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 20\nEnd\n\
Object Porter\n  Side = Men\n  KindOf = SELECTABLE INFANTRY DOZER\n  CommandSet = PorterSet\n  \
  BuildCost = 100\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 200\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 8\n  GeometryHeight = 16\nEnd\n\
CommandSet PorterSet\n  1 = Construct_Farm\nEnd\n\
CommandButton Construct_Farm\n  Command = DOZER_CONSTRUCT\n  Object = Farm\nEnd\n\
CommandSet FarmSet\n  1 = Command_Sell\nEnd\n\
CommandButton Command_Sell\n  Command = SELL\n  InPalantir = Yes\nEnd\n\
Object TestSpellBook\n  KindOf = SPELL_BOOK IMMOBILE IGNORES_SELECT_ALL INERT\n  CommandSet = SpellSet\n  \
  Behavior = OCLSpecialPower Tag_Hurt\n    SpecialPowerTemplate = SpellHurt\n    OCL = OCL_Hurt\n    \
    CreateLocation = CREATE_AT_LOCATION\n  End\n  \
  Behavior = SpecialPowerModule Tag_Rebuild\n    SpecialPowerTemplate = SpellRebuild\n    \
    AttributeModifier = RebuildHeal\n    AttributeModifierRange = 200\n    AttributeModifierAffects = ANY +STRUCTURE\n  End\n  \
  Behavior = SpecialPowerModule Tag_Snow\n    SpecialPowerTemplate = SpellSnow\n    \
    AttributeModifier = Snowbind\n    AttributeModifierRange = 100\n    AttributeModifierAffects = ANY +STRUCTURE\n  End\n  \
  Behavior = SpecialPowerModule Tag_Reach\n    SpecialPowerTemplate = SpellReach\n    \
    AttributeModifier = Reach\n    AttributeModifierRange = 50\n    AttributeModifierAffects = ANY +INFANTRY\n  End\n  \
  Behavior = SpecialPowerModule Tag_Outlaw\n    SpecialPowerTemplate = SpellOutlaw\n    \
    AttributeModifier = Outlaw\n    AttributeModifierRange = 50\n    AttributeModifierAffects = ANY +INFANTRY\n  End\n  \
  Behavior = PlayerUpgradeSpecialPower Tag_Gift\n    SpecialPowerTemplate = SpellGift\n    UpgradeName = Upgrade_Gift\n  End\n  \
  Behavior = SpecialPowerModule Tag_Farm\n    SpecialPowerTemplate = SpellFarm\n    \
    AttributeModifier = FarmLevel\n    AttributeModifierRange = 30\n    AttributeModifierAffects = ANY +STRUCTURE\n  End\n  \
  Behavior = SpecialPowerModule Tag_InfHeal\n    SpecialPowerTemplate = SpellInfantryHeal\n    \
    AttributeModifier = InfantryHeal\n    AttributeModifierRange = 50\n    AttributeModifierAffects = ANY +INFANTRY\n  End\n\
End\n\
CommandSet SpellSet\n  1 = Cast_Hurt\n  2 = Cast_Rebuild\n  3 = Cast_Snow\n  4 = Cast_Reach\n  5 = Cast_Outlaw\n  \
  6 = Cast_Gift\n  7 = Cast_Farm\n  8 = Cast_InfHeal\nEnd\n\
CommandButton Cast_Hurt\n  Command = SPELL_BOOK\n  SpecialPower = SpellHurt\n  Options = NEED_TARGET_POS\nEnd\n\
CommandButton Cast_Rebuild\n  Command = SPELL_BOOK\n  SpecialPower = SpellRebuild\n  Options = NEED_TARGET_POS\nEnd\n\
CommandButton Cast_Snow\n  Command = SPELL_BOOK\n  SpecialPower = SpellSnow\n  Options = NEED_TARGET_POS\nEnd\n\
CommandButton Cast_Reach\n  Command = SPELL_BOOK\n  SpecialPower = SpellReach\n  Options = NEED_TARGET_POS\nEnd\n\
CommandButton Cast_Outlaw\n  Command = SPELL_BOOK\n  SpecialPower = SpellOutlaw\n  Options = NEED_TARGET_POS\nEnd\n\
CommandButton Cast_Gift\n  Command = SPELL_BOOK\n  SpecialPower = SpellGift\nEnd\n\
CommandButton Cast_Farm\n  Command = SPELL_BOOK\n  SpecialPower = SpellFarm\n  Options = NEED_TARGET_POS\nEnd\n\
CommandButton Cast_InfHeal\n  Command = SPELL_BOOK\n  SpecialPower = SpellInfantryHeal\n  Options = NEED_TARGET_POS\nEnd\n\
PlayerTemplate FactionTest\n  Side = Men\n  IntrinsicSciencesMP = SCIENCE_MEN\n  \
  SpellBookMp = TestSpellBook\n  MaxLevelMP = 150\nEnd\n";

const HURT: u8 = 0;
const REBUILD: u8 = 1;
const SNOW: u8 = 2;
const REACH: u8 = 3;
const OUTLAW: u8 = 4;
const GIFT: u8 = 5;
const FARM: u8 = 6;

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
        "TestSpellBook",
        "Hero",
        "Elf",
        "Well",
        "HealingPost",
        "HealingPostSlow",
        "Soldier",
        "Veteran",
        "Pikeman",
        "Rider",
        "Swordsman",
        "Torchbearer",
        "Slinger",
        "Barracks",
        "Fortress",
        "Farm",
        "Porter",
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

fn simulation_with(
    m: &Match,
    placed: &[(&str, f32, f32, PlayerId)],
    factor: u32,
) -> (Simulation<RotwkRules>, Ids) {
    simulation_capped(m, placed, factor, Vec::new())
}

fn simulation_capped(
    m: &Match,
    placed: &[(&str, f32, f32, PlayerId)],
    factor: u32,
    caps: Vec<Option<u32>>,
) -> (Simulation<RotwkRules>, Ids) {
    let production = m.kinds.production(&m.data);
    let power_names = crate::powers::PowerNames::build(m.kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&m.data.special_powers);
    let faction = m.data.players.get("FactionTest").unwrap().clone();
    let starts: Vec<crate::skirmish::PlayerStart> = [PLAYER, OTHER]
        .into_iter()
        .map(|player| crate::skirmish::PlayerStart {
            player,
            faction: faction.clone(),
            at: Vec2::ZERO,
            castle: None,
            ai_base_candidates: Vec::new(),
        })
        .collect();
    let (sciences, ranks, plans) = crate::skirmish::spell_setup(
        &m.data,
        &m.kinds,
        &starts,
        crate::game_mode::GameMode::Skirmish,
        false,
    );
    let power_sciences = power_names.required_sciences(&m.data.special_powers, &sciences);
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
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let footprints = placed
        .iter()
        .enumerate()
        .filter_map(|(at, (name, ..))| {
            let entry = m.kinds.get(m.kinds.kind(name)?)?;
            entry
                .record
                .has_kind("STRUCTURE")
                .then(|| (at, crate::construction::footprint_of(&entry.record)))
        })
        .collect();
    let rules = RotwkRules::new(passability, spawns)
        .with_structure_footprints(footprints)
        .with_players(2)
        .with_hordes(m.data.formations.tables().to_vec(), Vec::new())
        .with_combat(m.data.combat.clone())
        .with_construction(Arc::clone(&m.kinds), production.clone())
        .with_economy(production.buildables.clone(), 1_000)
        .with_menus(production.menus.clone())
        .with_upgrades(m.kinds.upgrades(&m.data), Arc::clone(&m.data.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_experience(Arc::clone(&m.data.experience), m.data.armor_bonus_cap)
        .with_spell_books(sciences, ranks, plans, power_sciences)
        .with_creation_lists(Arc::clone(&m.data.ocls))
        .with_command_point_factor(factor)
        .with_command_point_table(m.data.command_points)
        .with_command_point_overrides(caps);
    let simulation = Simulation::new(5, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    (simulation, ids)
}

fn simulation(m: &Match, placed: &[(&str, f32, f32, PlayerId)]) -> (Simulation<RotwkRules>, Ids) {
    simulation_with(m, placed, 100)
}

fn run(simulation: &mut Simulation<RotwkRules>, ticks: u32) {
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
}

fn cast(
    simulation: &mut Simulation<RotwkRules>,
    player: PlayerId,
    button: u8,
    at: Option<(f32, f32)>,
) -> Frame {
    simulation
        .submit(
            player,
            RotwkCommand::CastSpell {
                button,
                target: None,
                at,
            },
        )
        .unwrap();
    simulation.tick().unwrap().frame
}

fn placed(m: &Match, simulation: &Simulation<RotwkRules>, ids: Ids, name: &str) -> Vec<Entity> {
    let kind = m.kinds.kind(name).unwrap();
    simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(entity, k)| {
            **k == kind
                && simulation.state().is_live(*entity)
                && !simulation.state().contains(ids.doomed, *entity)
        })
        .map(|(entity, _)| entity)
        .collect()
}

fn health(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> f32 {
    simulation
        .state()
        .get(ids.health, entity)
        .unwrap()
        .0
        .current()
}

fn limit(simulation: &Simulation<RotwkRules>, ids: Ids, player: PlayerId) -> i32 {
    crate::economy::points_of(simulation.state(), RotwkRules::economy_ids(ids), player).limit()
}

fn money(simulation: &Simulation<RotwkRules>, ids: Ids, player: PlayerId) -> i64 {
    let world = simulation.state();
    let entity =
        crate::economy::player_entity(world, RotwkRules::economy_ids(ids), player).unwrap();
    world.get(ids.purse, entity).unwrap().0.amount()
}

fn until_hurt(
    simulation: &mut Simulation<RotwkRules>,
    ids: Ids,
    entity: Entity,
    from: f32,
) -> Frame {
    for _ in 0..30 {
        let frame = simulation.tick().unwrap().frame;
        if health(simulation, ids, entity) < from {
            return frame;
        }
    }
    panic!("never hurt");
}

#[test]
fn a_hero_heals_itself_only_once_its_start_healing_delay_has_passed() {
    let m = load();
    let (mut simulation, ids) = simulation(&m, &[("Hero", 100.0, 100.0, PLAYER)]);
    let hero = placed(&m, &simulation, ids, "Hero")[0];
    run(&mut simulation, 40);
    cast(&mut simulation, OTHER, HURT, Some((100.0, 100.0)));
    let hit = until_hurt(&mut simulation, ids, hero, 500.0);
    assert_eq!(health(&simulation, ids, hero), 440.0);
    let mut seen = vec![];
    while simulation.frame().get() <= hit.get() + 89 {
        simulation.tick().unwrap();
        seen.push(health(&simulation, ids, hero));
    }
    assert!(
        seen.iter().all(|h| *h == 440.0),
        "no heal inside StartHealingDelay: {seen:?}"
    );
    run(&mut simulation, 30);
    assert_eq!(health(&simulation, ids, hero), 470.0, "one pulse of 30");
    run(&mut simulation, 30);
    assert_eq!(
        health(&simulation, ids, hero),
        500.0,
        "and full, no further"
    );
    run(&mut simulation, 60);
    assert_eq!(health(&simulation, ids, hero), 500.0);
}

#[test]
fn a_triggered_heal_waits_for_its_upgrade() {
    let m = load();
    let (mut simulation, ids) = simulation(&m, &[("Elf", 100.0, 100.0, PLAYER)]);
    let elf = placed(&m, &simulation, ids, "Elf")[0];
    cast(&mut simulation, OTHER, HURT, Some((100.0, 100.0)));
    until_hurt(&mut simulation, ids, elf, 200.0);
    run(&mut simulation, 120);
    assert_eq!(health(&simulation, ids, elf), 140.0, "no gift, no healing");
    cast(&mut simulation, PLAYER, GIFT, None);
    run(&mut simulation, 95);
    let healed = health(&simulation, ids, elf);
    assert!((150.0..=160.0).contains(&healed), "5 a second: {healed}");
}

#[test]
fn a_healing_circle_heals_its_own_infantry_only() {
    let m = load();
    let (mut simulation, ids) = simulation(
        &m,
        &[
            ("Well", 300.0, 300.0, PLAYER),
            ("Soldier", 300.0, 350.0, PLAYER),
            ("Rider", 310.0, 350.0, PLAYER),
            ("Soldier", 300.0, 250.0, OTHER),
        ],
    );
    let soldiers = placed(&m, &simulation, ids, "Soldier");
    let (own, enemy) = (soldiers[0], soldiers[1]);
    let rider = placed(&m, &simulation, ids, "Rider")[0];
    cast(&mut simulation, OTHER, HURT, Some((300.0, 350.0)));
    cast(&mut simulation, PLAYER, HURT, Some((300.0, 250.0)));
    run(&mut simulation, 2);
    assert!(
        health(&simulation, ids, own) <= 50.0,
        "hurt, at most one heal in"
    );
    assert_eq!(health(&simulation, ids, enemy), 40.0);
    assert_eq!(health(&simulation, ids, rider), 40.0);
    let mut fx_frames = 0;
    for _ in 0..45 {
        let frame = simulation.tick().unwrap().frame;
        let heals = simulation
            .rules()
            .fx_events(simulation.state(), frame)
            .iter()
            .filter(|event| simulation.rules().fx_name(event.list) == Some("FX_WellHeal"))
            .count();
        fx_frames += usize::from(heals > 0);
    }
    assert!(health(&simulation, ids, own) >= 60.0, "own infantry healed");
    assert_eq!(
        health(&simulation, ids, rider),
        40.0,
        "cavalry is not INFANTRY"
    );
    assert_eq!(
        health(&simulation, ids, enemy),
        40.0,
        "an enemy is not healed"
    );
    assert!(
        fx_frames >= 2,
        "the pulse FX plays on each heal: {fx_frames}"
    );
}

#[test]
fn two_healing_circles_over_one_object_heal_it_as_one() {
    let m = load();
    let (mut simulation, ids) = simulation(
        &m,
        &[
            ("Well", 300.0, 300.0, PLAYER),
            ("Well", 320.0, 300.0, PLAYER),
            ("Pikeman", 310.0, 360.0, PLAYER),
        ],
    );
    let pike = placed(&m, &simulation, ids, "Pikeman")[0];
    cast(&mut simulation, OTHER, HURT, Some((310.0, 360.0)));
    until_hurt(&mut simulation, ids, pike, 1000.0);
    let hurt = health(&simulation, ids, pike);
    let mut before = hurt;
    for _ in 0..40 {
        simulation.tick().unwrap();
        if health(&simulation, ids, pike) > hurt {
            break;
        }
        before = health(&simulation, ids, pike);
    }
    run(&mut simulation, 29);
    assert_eq!(
        health(&simulation, ids, pike) - before,
        20.0,
        "two heals of 10 in 30 frames, not four"
    );
}

#[test]
fn a_passive_area_heal_converts_percent_of_max_health_per_ping() {
    let m = load();
    let (mut simulation, ids) = simulation(
        &m,
        &[
            ("HealingPost", 300.0, 300.0, PLAYER),
            ("Soldier", 300.0, 350.0, PLAYER),
        ],
    );
    let soldier = placed(&m, &simulation, ids, "Soldier")[0];
    cast(&mut simulation, OTHER, HURT, Some((300.0, 350.0)));
    let hit = until_hurt(&mut simulation, ids, soldier, 100.0);
    let hurt = health(&simulation, ids, soldier);
    assert_eq!(hurt, 40.0, "60 FLAME on 100 max, no armor reduction");
    let mut landed = None;
    for _ in 0..200 {
        let frame = simulation.tick().unwrap().frame;
        let now = health(&simulation, ids, soldier);
        if now != hurt {
            landed = Some((frame, now));
            break;
        }
    }
    let (frame, healed) = landed.expect("the circle heals it eventually");
    assert!(
        frame.get() >= hit.get() + 100,
        "landed at {frame:?}, suspiciously close to the hit at {hit:?}"
    );
    assert_eq!(
        healed - hurt,
        10.0,
        "10% of 100 max over one 1000 ms PingDelay"
    );
}

#[test]
fn a_recently_damaged_target_is_skipped_by_the_circle_heal() {
    let m = load();
    let (mut simulation, ids) = simulation(
        &m,
        &[
            ("HealingPost", 300.0, 300.0, PLAYER),
            ("Soldier", 300.0, 350.0, PLAYER),
        ],
    );
    let soldier = placed(&m, &simulation, ids, "Soldier")[0];
    cast(&mut simulation, OTHER, HURT, Some((300.0, 350.0)));
    let hit = until_hurt(&mut simulation, ids, soldier, 100.0);
    let hurt = health(&simulation, ids, soldier);
    while simulation.frame().get() < hit.get() + 119 {
        simulation.tick().unwrap();
        assert_eq!(
            health(&simulation, ids, soldier),
            hurt,
            "still inside the 4 s recent-damage gate"
        );
    }
    run(&mut simulation, 30);
    assert!(
        health(&simulation, ids, soldier) > hurt,
        "past 4 s, the circle heals it"
    );
}

#[test]
fn a_passive_area_heal_never_heals_its_own_object() {
    let m = load();
    let (mut simulation, ids) = simulation(&m, &[("HealingPost", 300.0, 300.0, PLAYER)]);
    let post = placed(&m, &simulation, ids, "HealingPost")[0];
    cast(&mut simulation, OTHER, HURT, Some((300.0, 300.0)));
    let hurt = health(&simulation, ids, post);
    assert!(hurt < 1000.0, "the post itself took the splash");
    run(&mut simulation, 200);
    assert_eq!(health(&simulation, ids, post), hurt, "never heals itself");
}

#[test]
fn nonstackable_skips_a_target_recently_healed_by_a_different_source() {
    let m = load();
    let (mut simulation, ids) = simulation(
        &m,
        &[
            ("HealingPostSlow", 300.0, 300.0, PLAYER),
            ("Veteran", 300.0, 350.0, PLAYER),
        ],
    );
    let veteran = placed(&m, &simulation, ids, "Veteran")[0];
    cast(&mut simulation, OTHER, HURT, Some((300.0, 350.0)));
    until_hurt(&mut simulation, ids, veteran, 100.0);
    let mut max_step = 0.0f32;
    let mut last = health(&simulation, ids, veteran);
    for _ in 0..200 {
        simulation.tick().unwrap();
        let now = health(&simulation, ids, veteran);
        max_step = max_step.max(now - last);
        last = now;
    }
    assert!(
        max_step <= 1.1,
        "no jump bigger than the veteran's 1 hp pulse - the slow circle's own 50% heal must not land: {max_step}"
    );
}

#[test]
fn invulnerable_stops_only_the_damage_types_it_names() {
    let m = load();
    let (mut simulation, ids) = simulation(
        &m,
        &[
            ("Barracks", 200.0, 200.0, PLAYER),
            ("Swordsman", 200.0, 232.0, OTHER),
            ("Barracks", 240.0, 200.0, PLAYER),
            ("Torchbearer", 240.0, 232.0, OTHER),
            ("Barracks", 600.0, 600.0, PLAYER),
            ("Swordsman", 600.0, 632.0, OTHER),
        ],
    );
    let barracks = placed(&m, &simulation, ids, "Barracks");
    cast(&mut simulation, PLAYER, SNOW, Some((220.0, 200.0)));
    run(&mut simulation, 120);
    assert_eq!(
        health(&simulation, ids, barracks[0]),
        3000.0,
        "SLASH stopped"
    );
    assert!(
        health(&simulation, ids, barracks[1]) < 3000.0,
        "FLAME is not named"
    );
    assert!(health(&simulation, ids, barracks[2]) < 3000.0, "unbound");
}

#[test]
fn range_widens_what_a_weapon_reaches() {
    let m = load();
    let (mut simulation, ids) = simulation(
        &m,
        &[
            ("Slinger", 100.0, 100.0, PLAYER),
            ("Barracks", 100.0, 230.0, OTHER),
        ],
    );
    let target = placed(&m, &simulation, ids, "Barracks")[0];
    run(&mut simulation, 90);
    assert_eq!(health(&simulation, ids, target), 3000.0, "out of reach");
    cast(&mut simulation, PLAYER, REACH, Some((100.0, 100.0)));
    run(&mut simulation, 90);
    assert!(health(&simulation, ids, target) < 3000.0, "in reach at 160");
}

#[test]
fn a_killer_holding_a_bounty_percentage_earns_the_bounty() {
    let m = load();
    let run_kill = |outlaw: bool| {
        let (mut simulation, ids) = simulation(
            &m,
            &[
                ("Swordsman", 100.0, 100.0, PLAYER),
                ("Soldier", 100.0, 115.0, OTHER),
            ],
        );
        if outlaw {
            cast(&mut simulation, PLAYER, OUTLAW, Some((100.0, 100.0)));
        }
        let before = money(&simulation, ids, PLAYER);
        run(&mut simulation, 150);
        assert!(placed(&m, &simulation, ids, "Soldier").is_empty(), "killed");
        money(&simulation, ids, PLAYER) - before
    };
    assert_eq!(run_kill(false), 0, "no bounty without one");
    assert_eq!(run_kill(true), 4, "the victim's BountyValue");
}

#[test]
fn the_command_point_limit_is_the_start_plus_standing_bonuses_up_to_the_maximum() {
    let m = load();
    let layout = [
        ("Fortress", 100.0, 100.0, PLAYER),
        ("Farm", 300.0, 100.0, PLAYER),
        ("Fortress", 600.0, 600.0, OTHER),
        ("Farm", 700.0, 600.0, OTHER),
        ("Farm", 760.0, 600.0, OTHER),
        ("Farm", 820.0, 600.0, OTHER),
    ];
    let (mut simulation, ids) = simulation(&m, &layout);
    run(&mut simulation, 2);
    assert_eq!(limit(&simulation, ids, PLAYER), 250, "100 + 100 + 50");
    assert_eq!(limit(&simulation, ids, OTHER), 280, "350 capped at 280");
    cast(&mut simulation, PLAYER, FARM, Some((300.0, 100.0)));
    run(&mut simulation, 1);
    assert_eq!(limit(&simulation, ids, PLAYER), 275, "a farm level's +25");
    cast(&mut simulation, OTHER, HURT, Some((300.0, 100.0)));
    run(&mut simulation, 5);
    assert!(
        placed(&m, &simulation, ids, "Farm").len() == 3,
        "the farm died"
    );
    assert_eq!(
        limit(&simulation, ids, PLAYER),
        200,
        "its bonus went with it"
    );

    let (mut doubled, ids) = simulation_with(&m, &layout, 200);
    run(&mut doubled, 2);
    assert_eq!(
        limit(&doubled, ids, PLAYER),
        250,
        "the factor scales the cap only"
    );
    assert_eq!(
        limit(&doubled, ids, OTHER),
        350,
        "its sum now clears the unscaled cap"
    );
}

#[test]
fn a_battle_cap_replaces_the_rows_maximum_and_keeps_its_start() {
    let m = load();
    let layout = [
        ("Fortress", 100.0, 100.0, PLAYER),
        ("Farm", 300.0, 100.0, PLAYER),
        ("Fortress", 600.0, 600.0, OTHER),
        ("Farm", 700.0, 600.0, OTHER),
    ];
    let (mut simulation, ids) = simulation_capped(&m, &layout, 100, vec![Some(150), Some(40)]);
    run(&mut simulation, 2);
    assert_eq!(
        limit(&simulation, ids, PLAYER),
        150,
        "100 + 100 + 50 capped at 150"
    );
    assert_eq!(
        limit(&simulation, ids, OTHER),
        40,
        "the cap is below the row's start"
    );

    let bare = [
        ("Soldier", 100.0, 100.0, PLAYER),
        ("Soldier", 600.0, 600.0, OTHER),
    ];
    let (mut simulation, ids) = simulation_capped(&m, &bare, 100, vec![Some(150), None]);
    run(&mut simulation, 2);
    assert_eq!(
        limit(&simulation, ids, PLAYER),
        100,
        "the row's start, not the cap"
    );
}

#[test]
fn a_site_or_a_structure_being_sold_adds_no_command_points() {
    let m = load();
    let (mut simulation, ids) = simulation(
        &m,
        &[
            ("Fortress", 100.0, 100.0, PLAYER),
            ("Porter", 300.0, 300.0, PLAYER),
        ],
    );
    run(&mut simulation, 2);
    assert_eq!(limit(&simulation, ids, PLAYER), 200);
    let porter = placed(&m, &simulation, ids, "Porter")[0];
    let farm = m.kinds.kind("Farm").unwrap();
    let press = |simulation: &mut Simulation<RotwkRules>, on: Entity, command: RotwkCommand| {
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![on],
                },
            )
            .unwrap();
        simulation.submit(PLAYER, command).unwrap();
        simulation.tick().unwrap();
    };
    press(
        &mut simulation,
        porter,
        RotwkCommand::Construct {
            kind: farm.0,
            x: 340.0,
            y: 300.0,
            angle: 0.0,
        },
    );
    let mut site = None;
    for _ in 0..600 {
        site = placed(&m, &simulation, ids, "Farm").first().copied();
        if site.is_some() {
            break;
        }
        simulation.tick().unwrap();
    }
    let site = site.expect("the porter reached the spot and the site appeared");
    assert!(simulation.state().contains(ids.construction, site));
    run(&mut simulation, 2);
    assert_eq!(limit(&simulation, ids, PLAYER), 200, "a site adds nothing");
    for _ in 0..600 {
        if !simulation.state().contains(ids.construction, site) {
            break;
        }
        simulation.tick().unwrap();
    }
    assert!(
        !simulation.state().contains(ids.construction, site),
        "built"
    );
    run(&mut simulation, 1);
    assert_eq!(limit(&simulation, ids, PLAYER), 250, "finished, it adds 50");
    press(&mut simulation, site, RotwkCommand::Sell);
    assert!(simulation.state().contains(ids.doomed, site));
    assert_eq!(
        limit(&simulation, ids, PLAYER),
        200,
        "sold, it adds nothing"
    );
}

#[test]
fn a_match_healing_and_binding_replays_identically() {
    let m = load();
    let layout = [
        ("Hero", 100.0, 100.0, PLAYER),
        ("Well", 300.0, 300.0, PLAYER),
        ("Soldier", 300.0, 350.0, PLAYER),
        ("Barracks", 500.0, 200.0, PLAYER),
        ("Swordsman", 500.0, 232.0, OTHER),
        ("Farm", 700.0, 100.0, PLAYER),
        ("Fortress", 800.0, 800.0, OTHER),
    ];
    let hashes = || {
        let (mut simulation, _) = simulation(&m, &layout);
        let mut out = Vec::new();
        for frame in 0..200u32 {
            match frame {
                3 => {
                    cast(&mut simulation, OTHER, HURT, Some((100.0, 100.0)));
                }
                5 => {
                    cast(&mut simulation, OTHER, HURT, Some((300.0, 350.0)));
                }
                7 => {
                    cast(&mut simulation, PLAYER, SNOW, Some((500.0, 200.0)));
                }
                9 => {
                    cast(&mut simulation, PLAYER, REBUILD, Some((500.0, 200.0)));
                }
                11 => {
                    cast(&mut simulation, PLAYER, FARM, Some((700.0, 100.0)));
                }
                _ => {
                    simulation.tick().unwrap();
                }
            }
            out.push(simulation.state_hash());
        }
        out
    };
    let first = hashes();
    assert_eq!(first, hashes());
    assert!(first.windows(2).any(|pair| pair[0] != pair[1]));
}
