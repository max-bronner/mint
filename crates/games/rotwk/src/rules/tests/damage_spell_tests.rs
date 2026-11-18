use super::*;

const DATA: &str = "\
#define SPELL_RADIUS 100\n\
Science SCIENCE_MEN\n  PrerequisiteSciences = None\n  SciencePurchasePointCost = 0\n  IsGrantable = No\nEnd\n\
Science SCIENCE_Scavenger\n  PrerequisiteSciences = SCIENCE_MEN\n  SciencePurchasePointCostMP = 1\nEnd\n\
Rank 1\n  SkillPointsNeededDefault = 0\n  SkillPointsNeededCampaign = 0\n  SciencePurchasePointsGranted = 5\nEnd\n\
Rank 2\n  SkillPointsNeededDefault = 100\n  SkillPointsNeededCampaign = 100\n  SciencePurchasePointsGranted = 1\nEnd\n\
Upgrade Upgrade_Call\n  Type = PLAYER\nEnd\n\
SpecialPower SpellFire\n  ReloadTime = 0\nEnd\n\
SpecialPower SpellTaint\n  ReloadTime = 0\nEnd\n\
SpecialPower SpellDark\n  ReloadTime = 0\nEnd\n\
SpecialPower SpellRain\n  ReloadTime = 0\nEnd\n\
SpecialPower SpellClouds\n  ReloadTime = 0\nEnd\n\
SpecialPower SpellDevastate\n  ReloadTime = 0\nEnd\n\
SpecialPower SpellScavenger\n  ReloadTime = 0\n  RequiredSciences = SCIENCE_Scavenger\nEnd\n\
SpecialPower SpellAllegiance\n  ReloadTime = 0\nEnd\n\
SpecialPower SpellHorde\n  ReloadTime = 0\nEnd\n\
SpecialPower SpellPoison\n  ReloadTime = 0\nEnd\n\
SpecialPower SpellStone\n  ReloadTime = 0\nEnd\n\
SpecialPower SpellWood\n  ReloadTime = 0\nEnd\n\
SpecialPower SpellCall\n  ReloadTime = 0\nEnd\n\
ModifierList GroundBuff\n  Category = BUFF\n  Modifier = DAMAGE_MULT 200%\nEnd\n\
ModifierList DarkMod\n  Category = SPELL\n  Modifier = ARMOR 50%\n  Duration = 0\nEnd\n\
ModifierList LeaderMod\n  Category = LEADERSHIP\n  Modifier = DAMAGE_MULT 150%\nEnd\n\
ModifierList StoneMod\n  Category = DEBUFF\n  Modifier = SPEED 0%\n  Duration = 2000\nEnd\n\
ObjectCreationList OCL_Fire\n  CreateObject\n    ObjectNames = FireRain\n  End\nEnd\n\
ObjectCreationList OCL_Poison\n  CreateObject\n    ObjectNames = PoisonCloud\n  End\nEnd\n\
ObjectCreationList OCL_Stone\n  CreateObject\n    ObjectNames = StoneBeam\n  End\nEnd\n\
ObjectCreationList OCL_Seed\n  CreateObject\n    ObjectNames = Seed\n    Count = 2\n  End\nEnd\n\
ObjectCreationList OCL_Grove\n  CreateObject\n    ObjectNames = Sapling\n    Count = 2\n  End\nEnd\n\
Object Sapling\n  KindOf = TREE IMMOBILE\n  IsHarvestable = Yes\nEnd\n\
Armor Plain\n  Armor = DEFAULT 100%\nEnd\n\
Weapon FearWeapon\n  FireFX = FX_Fear\nEnd\n\
Weapon FireWeapon\n  RadiusDamageAffects = ENEMIES NEUTRALS\n  FireFX = FX_FireHit\n  \
  DamageNugget\n    Damage = 50\n    Radius = SPELL_RADIUS\n    DamageType = FLAME\n    \
    DamageScalar = 50% NONE +COMMANDCENTER\n  End\nEnd\n\
Weapon PoisonWeapon\n  RadiusDamageAffects = ENEMIES\n  \
  DOTNugget\n    Damage = 5\n    Radius = 50\n    DamageInterval = 600\n    DamageDuration = 2400\n    \
    DamageType = POISON\n  End\nEnd\n\
Weapon StoneWeapon\n  RadiusDamageAffects = ENEMIES\n  \
  AttributeModifierNugget\n    AttributeModifier = StoneMod\n    Radius = 100\n    \
    SpecialObjectFilter = ANY +MONSTER\n  End\nEnd\n\
Weapon DevastationWeapon\n  \
  DamageNugget\n    SpecialObjectFilter = NONE +Ent ENEMIES\n    Damage = 300\n    Radius = 100\n    \
    DamageType = SLASH\n  End\nEnd\n\
Weapon Club\n  AttackRange = 40\n  DelayBetweenShots = 100\n  MeleeWeapon = Yes\n  \
  DamageNugget\n    Damage = 1000\n    DamageType = CRUSH\n  End\nEnd\n\
Weapon Sling\n  AttackRange = 300\n  DelayBetweenShots = 1000000\n  RadiusDamageAffects = ENEMIES NOT_SIMILAR\n  \
  DamageNugget\n    Damage = 30\n    Radius = 30\n    DamageType = SLASH\n  End\nEnd\n\
Object FireRain\n  KindOf = NO_COLLIDE IMMOBILE UNATTACKABLE INERT\n  \
  Body = ImmortalBody Tag_Body\n    MaxHealth = 1\n  End\n  \
  Behavior = DeletionUpdate Tag_Del\n    MinLifetime = 3000\n    MaxLifetime = 3000\n  End\n  \
  Behavior = FireWeaponUpdate Tag_Fire\n    \
    FireWeaponNugget\n      WeaponName = FearWeapon\n      FireDelay = 0\n      OneShot = Yes\n    End\n    \
    FireWeaponNugget\n      WeaponName = FireWeapon\n      FireDelay = 1000\n      OneShot = Yes\n    End\n  \
  End\nEnd\n\
Object PoisonCloud\n  KindOf = NO_COLLIDE IMMOBILE UNATTACKABLE INERT\n  \
  Behavior = DeletionUpdate Tag_Del\n    MinLifetime = 1000\n    MaxLifetime = 1000\n  End\n  \
  Behavior = FireWeaponUpdate Tag_Fire\n    \
    FireWeaponNugget\n      WeaponName = PoisonWeapon\n      FireDelay = 0\n      OneShot = Yes\n    End\n  \
  End\nEnd\n\
Object StoneBeam\n  KindOf = NO_COLLIDE IMMOBILE UNATTACKABLE INERT\n  \
  Behavior = DeletionUpdate Tag_Del\n    MinLifetime = 1000\n    MaxLifetime = 1000\n  End\n  \
  Behavior = FireWeaponUpdate Tag_Fire\n    \
    FireWeaponNugget\n      WeaponName = StoneWeapon\n      FireDelay = 0\n      OneShot = Yes\n    End\n  \
  End\nEnd\n\
Object TaintGround\n  KindOf = PRELOAD UNATTACKABLE IMMOBILE TAINT\n  \
  Behavior = AttributeModifierAuraUpdate Tag_Aura\n    StartsActive = Yes\n    BonusName = GroundBuff\n    \
    RefreshDelay = 1000\n    Range = SPELL_RADIUS\n    ObjectFilter = ANY +INFANTRY\n  End\n  \
  Behavior = DeletionUpdate Tag_Del\n    MinLifetime = 4000\n    MaxLifetime = 4000\n  End\nEnd\n\
Object Seed\n  KindOf = IMMOBILE UNATTACKABLE TAINTEFFECT\n  \
  Behavior = DeletionUpdate Tag_Del\n    MinLifetime = 100\n    MaxLifetime = 100\n  End\nEnd\n\
Weapon MoatWeapon\n  DelayBetweenShots = 1000\n  RadiusDamageAffects = ENEMIES\n  \
  DamageNugget\n    Damage = 10\n    Radius = 50\n    DamageType = SLASH\n  End\nEnd\n\
Object Moat\n  Side = Men\n  KindOf = STRUCTURE IMMOBILE\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Behavior = FireWeaponUpdate Tag_Fire\n    \
    FireWeaponNugget\n      WeaponName = MoatWeapon\n      FireDelay = 0\n      OneShot = No\n    End\n  \
  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 10\nEnd\n\
Object Bomber\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1\n  End\n  \
  Behavior = SlowDeathBehavior Tag_Death\n    DestructionDelay = 3000\n  End\n  \
  Behavior = FireWeaponUpdate Tag_Fire\n    AliveOnly = Yes\n    \
    FireWeaponNugget\n      WeaponName = MoatWeapon\n      FireDelay = 1000\n      OneShot = No\n    End\n  \
  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
ExperienceLevel CatapultLevel1\n  TargetNames = Catapult\n  RequiredExperience = 1\n  ExperienceAward = 2\n  \
  ExperienceAwardOwnGuysDie = 7\n  Rank = 1\nEnd\n\
ExperienceLevel CatapultLevel2\n  TargetNames = Catapult\n  RequiredExperience = 1000\n  Rank = 2\nEnd\n\
ExperienceLevel TargetLevel1\n  TargetNames = Target\n  RequiredExperience = 1\n  ExperienceAward = 4\n  \
  ExperienceAwardOwnGuysDie = 50\n  Rank = 1\nEnd\n\
Weapon Boulder\n  AttackRange = 150\n  DelayBetweenShots = 1000000\n  RadiusDamageAffects = ALLIES ENEMIES\n  \
  DamageNugget\n    Damage = 500\n    Radius = 40\n    DamageType = SIEGE\n  End\nEnd\n\
Object Catapult\n  Side = Men\n  KindOf = VEHICLE SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Boulder\n  End\n  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 16\nEnd\n\
Object Target\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 25\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Walker\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Sunbeam\n  KindOf = IMMOBILE UNATTACKABLE INERT SCARY\n  \
  Behavior = DeletionUpdate Tag_Del\n    MinLifetime = 5000\n    MaxLifetime = 5000\n  End\nEnd\n\
Object Captain\n  Side = Men\n  KindOf = INFANTRY HERO SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 500\n  End\n  \
  Behavior = AttributeModifierAuraUpdate Tag_Leader\n    StartsActive = No\n    TriggeredBy = Upgrade_Call\n    \
    BonusName = LeaderMod\n    RefreshDelay = 1000\n    Range = 100\n    ObjectFilter = ANY +INFANTRY -HERO\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 50\n  BuildTime = 10\n  \
  BountyValue = 4\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Totem\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ImmortalBody Tag_Body\n    MaxHealth = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Ent\n  Side = Men\n  KindOf = MONSTER SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 1000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Creep\n  Side = Men\n  KindOf = MONSTER SELECTABLE CREEP\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Troll\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Club\n  End\n  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Slinger\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_Body\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Sling\n  End\n  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Fortress\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE COMMANDCENTER\n  \
  BuildCost = 1000\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 5000\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 40\nEnd\n\
Object Barracks\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = BarracksSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_Body\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_Production\n    MaxQueueEntries = 3\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 10\n  GeometryHeight = 30\nEnd\n\
CommandSet BarracksSet\n  1 = Build_Soldier\nEnd\n\
CommandButton Build_Soldier\n  Command = UNIT_BUILD\n  Object = Soldier\nEnd\n\
Object TestSpellBook\n  KindOf = SPELL_BOOK IMMOBILE IGNORES_SELECT_ALL INERT\n  CommandSet = SpellSet\n  \
  Behavior = OCLSpecialPower Tag_Fire\n    SpecialPowerTemplate = SpellFire\n    OCL = OCL_Fire\n    \
    CreateLocation = CREATE_AT_LOCATION\n    TriggerFX = FX_FireCast\n  End\n  \
  Behavior = TaintSpecialPower Tag_Taint\n    SpecialPowerTemplate = SpellTaint\n    TaintObject = TaintGround\n    \
    TaintRadius = SPELL_RADIUS\n    TaintFX = FX_Taint\n    TaintOCL = OCL_Seed\n  End\n  \
  Behavior = DarknessSpecialPower Tag_Dark\n    SpecialPowerTemplate = SpellDark\n    AttributeModifier = DarkMod\n    \
    AttributeModifierAffects = ANY +INFANTRY -HERO ALLIES\n    AttributeModifierWeatherBased = Yes\n    \
    WeatherDuration = 2000\n    ChangeWeather = CLOUDY\n  End\n  \
  Behavior = FreezingRainSpecialPower Tag_Rain\n    SpecialPowerTemplate = SpellRain\n    \
    AttributeModifierAffects = ALL ENEMIES\n    AntiCategory = LEADERSHIP\n    WeatherDuration = 3000\n  End\n  \
  Behavior = CloudBreakSpecialPower Tag_Clouds\n    SpecialPowerTemplate = SpellClouds\n    \
    SunbeamObject = Sunbeam\n    ObjectSpacing = 300\n    AttributeModifierAffects = ANY +INFANTRY -HERO ENEMIES\n    \
    ReEnableAntiCategory = Yes\n    WeatherDuration = 3000\n  End\n  \
  Behavior = DevastateSpecialPower Tag_Dev\n    SpecialPowerTemplate = SpellDevastate\n    Radius = 100\n    \
    FX = FX_Devastate\n    FireWeapon = DevastationWeapon\n    TreeValueMultiplier = 50%\n    \
    TreeValueTotalCap = 1500\n  End\n  \
  Behavior = ScavengerSpecialPower Tag_Scav\n    SpecialPowerTemplate = SpellScavenger\n    BountyPercent = 1.0\n  End\n  \
  Behavior = UntamedAllegianceSpecialPower Tag_Allegiance\n    SpecialPowerTemplate = SpellAllegiance\n    \
    TargetEnemy = Yes\n    AttributeModifierAffects = ANY +CREEP\n    AttributeModifierRange = 60\n  End\n  \
  Behavior = ProductionSpeedBonus Tag_Horde\n    SpecialPowerTemplate = SpellHorde\n    NumberOfFrames = 180\n    \
    SpeedMulitplier = 20.0\n    Type = Soldier\n  End\n  \
  Behavior = OCLSpecialPower Tag_Poison\n    SpecialPowerTemplate = SpellPoison\n    OCL = OCL_Poison\n    \
    CreateLocation = CREATE_AT_LOCATION\n  End\n  \
  Behavior = OCLSpecialPower Tag_Stone\n    SpecialPowerTemplate = SpellStone\n    OCL = OCL_Stone\n    \
    CreateLocation = CREATE_AT_LOCATION\n  End\n  \
  Behavior = PlayerUpgradeSpecialPower Tag_Call\n    SpecialPowerTemplate = SpellCall\n    UpgradeName = Upgrade_Call\n  End\n  \
  Behavior = ElvenWoodSpecialPower Tag_Wood\n    SpecialPowerTemplate = SpellWood\n    \
    ElvenWoodRadius = SPELL_RADIUS\n    ElvenWoodOCL = OCL_Grove\n  End\n\
End\n\
CommandSet SpellSet\n  1 = Cast_Fire\n  2 = Cast_Taint\n  3 = Cast_Dark\n  4 = Cast_Rain\n  5 = Cast_Clouds\n  \
  6 = Cast_Devastate\n  7 = Cast_Scavenger\n  8 = Cast_Allegiance\n  9 = Cast_Horde\n  10 = Cast_Poison\n  \
  11 = Cast_Stone\n  12 = Cast_Call\n  13 = Cast_Wood\nEnd\n\
CommandButton Cast_Fire\n  Command = SPELL_BOOK\n  SpecialPower = SpellFire\n  Options = NEED_TARGET_POS\nEnd\n\
CommandButton Cast_Taint\n  Command = SPELL_BOOK\n  SpecialPower = SpellTaint\n  Options = NEED_TARGET_POS\nEnd\n\
CommandButton Cast_Dark\n  Command = SPELL_BOOK\n  SpecialPower = SpellDark\nEnd\n\
CommandButton Cast_Rain\n  Command = SPELL_BOOK\n  SpecialPower = SpellRain\nEnd\n\
CommandButton Cast_Clouds\n  Command = SPELL_BOOK\n  SpecialPower = SpellClouds\nEnd\n\
CommandButton Cast_Devastate\n  Command = SPELL_BOOK\n  SpecialPower = SpellDevastate\n  Options = NEED_TARGET_POS\nEnd\n\
CommandButton Cast_Scavenger\n  Command = SPELL_BOOK\n  SpecialPower = SpellScavenger\n  Options = NONPRESSABLE\nEnd\n\
CommandButton Cast_Allegiance\n  Command = SPELL_BOOK\n  SpecialPower = SpellAllegiance\n  Options = NEED_TARGET_POS\nEnd\n\
CommandButton Cast_Horde\n  Command = SPELL_BOOK\n  SpecialPower = SpellHorde\nEnd\n\
CommandButton Cast_Poison\n  Command = SPELL_BOOK\n  SpecialPower = SpellPoison\n  Options = NEED_TARGET_POS\nEnd\n\
CommandButton Cast_Stone\n  Command = SPELL_BOOK\n  SpecialPower = SpellStone\n  Options = NEED_TARGET_POS\nEnd\n\
CommandButton Cast_Call\n  Command = SPELL_BOOK\n  SpecialPower = SpellCall\nEnd\n\
CommandButton Cast_Wood\n  Command = SPELL_BOOK\n  SpecialPower = SpellWood\n  Options = NEED_TARGET_POS\nEnd\n\
CommandSet StoreSet\n  1 = Buy_Scavenger\nEnd\n\
CommandButton Buy_Scavenger\n  Command = PURCHASE_SCIENCE\n  Science = SCIENCE_Scavenger\nEnd\n\
PlayerTemplate FactionTest\n  Side = Men\n  IntrinsicSciencesMP = SCIENCE_MEN\n  \
  SpellBookMp = TestSpellBook\n  PurchaseScienceCommandSetMP = StoreSet\n  MaxLevelMP = 150\nEnd\n";

const FIRE: u8 = 0;
const TAINT: u8 = 1;
const DARK: u8 = 2;
const RAIN: u8 = 3;
const CLOUDS: u8 = 4;
const DEVASTATE: u8 = 5;
const ALLEGIANCE: u8 = 7;
const HORDE: u8 = 8;
const POISON: u8 = 9;
const STONE: u8 = 10;
const CALL: u8 = 11;
const WOOD: u8 = 12;

struct Spells {
    data: GameData,
    kinds: Arc<KindTable>,
}

fn load() -> Spells {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = [
        "TestSpellBook",
        "Soldier",
        "Troll",
        "Slinger",
        "Totem",
        "Ent",
        "Creep",
        "Captain",
        "Fortress",
        "Barracks",
        "Moat",
        "Bomber",
        "Catapult",
        "Target",
        "Walker",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let (kinds, missing) = KindTable::closure(&data, &seeds);
    assert!(missing.is_empty(), "missing from the closure: {missing:?}");
    for name in [
        "FireRain",
        "PoisonCloud",
        "StoneBeam",
        "Seed",
        "TaintGround",
        "Sunbeam",
    ] {
        assert!(
            kinds.kind(name).is_some(),
            "the closure follows the spell book to {name}"
        );
    }
    Spells {
        data,
        kinds: Arc::new(kinds),
    }
}

fn simulation(s: &Spells, placed: &[(&str, f32, f32, PlayerId)]) -> (Simulation<RotwkRules>, Ids) {
    simulation_with_trees(s, placed, Vec::new())
}

fn simulation_with_trees(
    s: &Spells,
    placed: &[(&str, f32, f32, PlayerId)],
    trees: Vec<crate::forest::ForestTreeInit>,
) -> (Simulation<RotwkRules>, Ids) {
    let production = s.kinds.production(&s.data);
    let power_names = crate::powers::PowerNames::build(s.kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&s.data.special_powers);
    let faction = s.data.players.get("FactionTest").unwrap().clone();
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
        &s.data,
        &s.kinds,
        &starts,
        crate::game_mode::GameMode::Skirmish,
        false,
    );
    let power_sciences = power_names.required_sciences(&s.data.special_powers, &sciences);
    let spawns: Vec<Spawn> = placed
        .iter()
        .map(|(name, x, y, owner)| {
            let kind = s.kinds.kind(name).unwrap();
            let mut spawn = s
                .kinds
                .spawn(
                    &s.data.combat,
                    kind,
                    Vec2::new(*x, *y),
                    Facing::EAST,
                    *owner,
                )
                .unwrap();
            spawn.producer = s
                .kinds
                .producer(&production, kind, Vec2::new(*x, *y), Facing::EAST);
            spawn
        })
        .collect();
    let map = flat_map(96, 96);
    let passability = Passability::build(&map, 5.0 / 128.0).unwrap();
    let rules = RotwkRules::new(passability, spawns)
        .with_players(2)
        .with_teams(vec![None, None])
        .with_map_players(
            2,
            vec![String::new()],
            vec![crate::alliance::MapRelations::default()],
        )
        .with_hordes(s.data.formations.tables().to_vec(), Vec::new())
        .with_combat(s.data.combat.clone())
        .with_construction(Arc::clone(&s.kinds), production.clone())
        .with_economy(production.buildables.clone(), 1_000)
        .with_menus(production.menus.clone())
        .with_upgrades(s.kinds.upgrades(&s.data), Arc::clone(&s.data.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_experience(Arc::clone(&s.data.experience), s.data.armor_bonus_cap)
        .with_spell_books(sciences, ranks, plans, power_sciences)
        .with_creation_lists(Arc::clone(&s.data.ocls))
        .with_forest_seeds(trees);
    let simulation = Simulation::new(3, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    (simulation, ids)
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
    let report = simulation.tick().unwrap();
    report.frame
}

fn placed(s: &Spells, simulation: &Simulation<RotwkRules>, ids: Ids, name: &str) -> Vec<Entity> {
    let kind = s.kinds.kind(name).unwrap();
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

fn bonus(
    simulation: &Simulation<RotwkRules>,
    ids: Ids,
    entity: Entity,
) -> mint_sage_combat::Attributes {
    simulation
        .state()
        .get(ids.bonuses, entity)
        .map_or(mint_sage_combat::Attributes::IDENTITY, |bonuses| bonuses.0)
}

fn fx_names(
    simulation: &Simulation<RotwkRules>,
    frame: Frame,
) -> Vec<(String, crate::fx::FxSource)> {
    simulation
        .rules()
        .fx_events(simulation.state(), frame)
        .iter()
        .map(|event| {
            (
                simulation
                    .rules()
                    .fx_name(event.list)
                    .unwrap_or("?")
                    .to_owned(),
                event.source,
            )
        })
        .collect()
}

#[test]
fn a_damage_spell_hits_its_enemies_once_at_its_delay_and_then_goes() {
    let s = load();
    let (mut simulation, ids) = simulation(
        &s,
        &[
            ("Soldier", 100.0, 100.0, PLAYER),
            ("Soldier", 110.0, 100.0, OTHER),
            ("Soldier", 400.0, 400.0, OTHER),
            ("Fortress", 140.0, 100.0, OTHER),
        ],
    );
    let soldiers = placed(&s, &simulation, ids, "Soldier");
    let fortress = placed(&s, &simulation, ids, "Fortress")[0];
    cast(&mut simulation, PLAYER, FIRE, Some((100.0, 100.0)));
    let rain = placed(&s, &simulation, ids, "FireRain");
    assert_eq!(rain.len(), 1, "the list created its object");
    run(&mut simulation, 29);
    assert_eq!(
        health(&simulation, ids, soldiers[1]),
        100.0,
        "not before its FireDelay"
    );
    run(&mut simulation, 1);
    assert_eq!(
        health(&simulation, ids, soldiers[1]),
        50.0,
        "the enemy in the radius"
    );
    assert_eq!(
        health(&simulation, ids, soldiers[0]),
        100.0,
        "the caster's own: ENEMIES only"
    );
    assert_eq!(
        health(&simulation, ids, soldiers[2]),
        100.0,
        "outside the radius"
    );
    assert_eq!(
        health(&simulation, ids, fortress),
        4975.0,
        "half to a COMMANDCENTER"
    );
    run(&mut simulation, 30);
    assert_eq!(health(&simulation, ids, soldiers[1]), 50.0, "OneShot: once");
    run(&mut simulation, 29);
    assert_eq!(
        placed(&s, &simulation, ids, "FireRain"),
        rain,
        "still standing at 89"
    );
    run(&mut simulation, 1);
    assert!(
        !simulation.state().is_live(rain[0]),
        "destroyed on its 90th frame, not killed"
    );
}

#[test]
fn a_cast_and_its_weapons_write_fx_events_on_the_frames_they_happen() {
    let s = load();
    let (mut simulation, _ids) = simulation(&s, &[("Soldier", 110.0, 100.0, OTHER)]);
    let cast_on = cast(&mut simulation, PLAYER, FIRE, Some((100.0, 100.0)));
    let fired = fx_names(&simulation, cast_on);
    assert_eq!(
        fired,
        vec![
            ("FX_FireCast".to_owned(), crate::fx::FxSource::Power),
            ("FX_Fear".to_owned(), crate::fx::FxSource::Weapon),
        ]
    );
    let events = simulation.rules().fx_events(simulation.state(), cast_on);
    assert_eq!(
        (events[0].x, events[0].y),
        (100.0, 100.0),
        "where the cast points"
    );
    run(&mut simulation, 29);
    let hit_on = simulation.tick().unwrap().frame;
    assert_eq!(
        fx_names(&simulation, hit_on),
        vec![("FX_FireHit".to_owned(), crate::fx::FxSource::Weapon)]
    );
    assert!(
        fx_names(&simulation, Frame::new(hit_on.get().saturating_sub(1))).is_empty(),
        "a frame that fired nothing reads as nothing"
    );
}

#[test]
fn presentation_collects_each_ticks_effects_once_and_filters_by_rung() {
    let s = load();
    let collected = |shown_rungs: usize| {
        let (mut simulation, _ids) = simulation(&s, &[("Soldier", 110.0, 100.0, OTHER)]);
        let mut fx = crate::fx_present::FxPresenter::default();
        for _ in 0..shown_rungs {
            fx.cycle();
        }
        cast(&mut simulation, PLAYER, FIRE, Some((100.0, 100.0)));
        let first = fx.collect(&simulation).len();
        let again = fx.collect(&simulation).len();
        for _ in 0..30 {
            simulation.tick().unwrap();
            fx.collect(&simulation);
        }
        let names: Vec<String> = fx
            .take()
            .map(|event| {
                simulation
                    .rules()
                    .fx_name(event.list)
                    .unwrap_or("?")
                    .to_owned()
            })
            .collect();
        assert_eq!(fx.take().count(), 0, "a draw takes them all");
        (first, again, names)
    };
    let (first, again, all) = collected(0);
    assert_eq!(
        first, 2,
        "the cast's TriggerFX and the fear weapon's FireFX"
    );
    assert_eq!(again, 0, "a frame read twice plays once");
    assert_eq!(all, ["FX_FireCast", "FX_Fear", "FX_FireHit"]);
    let (_, _, spells) = collected(1);
    assert_eq!(spells, ["FX_FireCast"], "spells only");
    let (_, _, none) = collected(2);
    assert!(none.is_empty(), "off");
}

#[test]
fn killing_your_own_side_gives_the_killers_own_guys_award_and_no_skill_points() {
    let s = load();
    let (mut simulation, ids) = simulation(
        &s,
        &[
            ("Catapult", 100.0, 100.0, PLAYER),
            ("Target", 200.0, 100.0, OTHER),
            ("Walker", 215.0, 100.0, PLAYER),
        ],
    );
    let catapult = placed(&s, &simulation, ids, "Catapult")[0];
    run(&mut simulation, 90);
    assert!(
        placed(&s, &simulation, ids, "Target").is_empty(),
        "the enemy died"
    );
    assert!(
        placed(&s, &simulation, ids, "Walker").is_empty(),
        "so did the walker"
    );
    let points = simulation
        .state()
        .get(ids.experience, catapult)
        .map(|held| held.points);
    assert_eq!(
        points,
        Some(1 + 4 + 7),
        "its first level's 1, the enemy's award 4, its own-guys award 7"
    );
    let skill = simulation
        .rules()
        .science_of(simulation.state(), ids, PLAYER)
        .map(|science| science.skill_points);
    assert_eq!(skill, Some(4), "only the enemy's kill gives skill points");
}

#[test]
fn a_spell_object_is_never_a_target_and_an_immortal_body_never_dies() {
    let s = load();
    let (mut simulation, ids) = simulation(
        &s,
        &[
            ("Troll", 130.0, 100.0, OTHER),
            ("Totem", 400.0, 100.0, PLAYER),
            ("Troll", 420.0, 100.0, OTHER),
        ],
    );
    cast(&mut simulation, PLAYER, FIRE, Some((100.0, 100.0)));
    let rain = placed(&s, &simulation, ids, "FireRain")[0];
    let trolls = placed(&s, &simulation, ids, "Troll");
    run(&mut simulation, 20);
    let armed = simulation.state().get(ids.armed, trolls[0]).unwrap();
    assert_ne!(armed.target, Some(rain), "never a target");
    let totem = placed(&s, &simulation, ids, "Totem")[0];
    assert_eq!(
        health(&simulation, ids, totem),
        1.0,
        "struck for 1000, held at 1"
    );
    assert!(!simulation.state().contains(ids.doomed, totem));
}

#[test]
fn damage_over_time_hits_at_once_then_pulses_every_interval_for_its_duration() {
    let s = load();
    let (mut simulation, ids) = simulation(
        &s,
        &[
            ("Soldier", 110.0, 100.0, OTHER),
            ("Soldier", 90.0, 100.0, PLAYER),
        ],
    );
    let soldiers = placed(&s, &simulation, ids, "Soldier");
    cast(&mut simulation, PLAYER, POISON, Some((100.0, 100.0)));
    run(&mut simulation, 2);
    assert_eq!(
        health(&simulation, ids, soldiers[0]),
        95.0,
        "the landing hit"
    );
    run(&mut simulation, 15);
    assert_eq!(
        health(&simulation, ids, soldiers[0]),
        95.0,
        "the first pulse is an interval on"
    );
    run(&mut simulation, 4);
    assert_eq!(health(&simulation, ids, soldiers[0]), 90.0);
    run(&mut simulation, 72);
    assert_eq!(
        health(&simulation, ids, soldiers[0]),
        75.0,
        "the landing hit and four pulses, then done"
    );
    assert_eq!(health(&simulation, ids, soldiers[1]), 100.0, "ENEMIES only");
    assert!(simulation.state().get(ids.dots, soldiers[0]).is_none());
}

#[test]
fn a_neutral_players_objects_are_reached_only_by_a_mask_naming_neutrals() {
    const NEUTRAL: PlayerId = PlayerId::new(2);
    let s = load();
    let (mut simulation, ids) = simulation(
        &s,
        &[
            ("Soldier", 110.0, 100.0, NEUTRAL),
            ("Soldier", 400.0, 400.0, NEUTRAL),
        ],
    );
    let soldiers = placed(&s, &simulation, ids, "Soldier");
    cast(&mut simulation, PLAYER, POISON, Some((100.0, 100.0)));
    run(&mut simulation, 2);
    assert_eq!(health(&simulation, ids, soldiers[0]), 100.0, "ENEMIES only");
    cast(&mut simulation, PLAYER, FIRE, Some((400.0, 400.0)));
    run(&mut simulation, 31);
    assert_eq!(
        health(&simulation, ids, soldiers[1]),
        50.0,
        "ENEMIES NEUTRALS"
    );
}

#[test]
fn a_modifier_nugget_gives_its_list_to_what_its_filter_passes() {
    let s = load();
    let (mut simulation, ids) = simulation(
        &s,
        &[
            ("Ent", 110.0, 100.0, OTHER),
            ("Soldier", 120.0, 100.0, OTHER),
        ],
    );
    let ent = placed(&s, &simulation, ids, "Ent")[0];
    let soldier = placed(&s, &simulation, ids, "Soldier")[0];
    cast(&mut simulation, PLAYER, STONE, Some((100.0, 100.0)));
    assert_eq!(
        bonus(&simulation, ids, ent).speed,
        0.0,
        "a MONSTER: turned to stone"
    );
    assert_eq!(bonus(&simulation, ids, soldier).speed, 1.0, "not a MONSTER");
    run(&mut simulation, 60);
    assert_eq!(
        bonus(&simulation, ids, ent).speed,
        1.0,
        "Duration 2000 ended"
    );
}

#[test]
fn taint_lays_ground_whose_aura_buffs_friends_in_range_until_it_goes() {
    let s = load();
    let (mut simulation, ids) = simulation(
        &s,
        &[
            ("Soldier", 150.0, 100.0, PLAYER),
            ("Soldier", 400.0, 400.0, PLAYER),
            ("Soldier", 120.0, 100.0, OTHER),
            ("Soldier", 100.0, 205.0, PLAYER),
        ],
    );
    let soldiers = placed(&s, &simulation, ids, "Soldier");
    let cast_on = cast(&mut simulation, PLAYER, TAINT, Some((100.0, 100.0)));
    assert_eq!(
        bonus(&simulation, ids, soldiers[3]).damage_mult,
        1.0,
        "just past the range"
    );
    assert!(fx_names(&simulation, cast_on)
        .contains(&("FX_Taint".to_owned(), crate::fx::FxSource::Power)));
    let ground = placed(&s, &simulation, ids, "TaintGround");
    assert_eq!(ground.len(), 1);
    assert_eq!(
        simulation.state().get(ids.owner, ground[0]).map(|o| o.0),
        Some(PLAYER)
    );
    assert_eq!(
        placed(&s, &simulation, ids, "Seed").len(),
        2,
        "the TaintOCL's trees"
    );
    assert_eq!(
        bonus(&simulation, ids, soldiers[0]).damage_mult,
        2.0,
        "in range, the caster's"
    );
    assert_eq!(
        bonus(&simulation, ids, soldiers[1]).damage_mult,
        1.0,
        "out of range"
    );
    assert_eq!(
        bonus(&simulation, ids, soldiers[2]).damage_mult,
        1.0,
        "the enemy's"
    );
    run(&mut simulation, 100);
    assert_eq!(
        bonus(&simulation, ids, soldiers[0]).damage_mult,
        2.0,
        "renewed each pulse"
    );
    run(&mut simulation, 100);
    assert!(
        placed(&s, &simulation, ids, "TaintGround").is_empty(),
        "its DeletionUpdate"
    );
    assert_eq!(
        bonus(&simulation, ids, soldiers[0]).damage_mult,
        1.0,
        "the last pulse ran out"
    );
}

#[test]
fn darkness_gives_its_list_to_the_casters_side_everywhere_for_its_duration() {
    let s = load();
    let (mut simulation, ids) = simulation(
        &s,
        &[
            ("Soldier", 100.0, 100.0, PLAYER),
            ("Soldier", 2000.0, 2000.0, PLAYER),
            ("Soldier", 110.0, 100.0, OTHER),
        ],
    );
    let soldiers = placed(&s, &simulation, ids, "Soldier");
    cast(&mut simulation, PLAYER, DARK, None);
    assert_eq!(bonus(&simulation, ids, soldiers[0]).armor, 0.5);
    assert_eq!(bonus(&simulation, ids, soldiers[1]).armor, 0.5, "map-wide");
    assert_eq!(
        bonus(&simulation, ids, soldiers[2]).armor,
        0.0,
        "the enemy's"
    );
    run(&mut simulation, 60);
    assert_eq!(
        bonus(&simulation, ids, soldiers[0]).armor,
        0.0,
        "2000 ms ended"
    );
}

#[test]
fn freezing_rain_silences_enemy_leadership_and_cloud_break_restores_it() {
    let s = load();
    let (mut simulation, ids) = simulation(
        &s,
        &[
            ("Captain", 100.0, 100.0, OTHER),
            ("Soldier", 120.0, 100.0, OTHER),
            ("Soldier", 500.0, 500.0, PLAYER),
        ],
    );
    let soldier = placed(&s, &simulation, ids, "Soldier")[0];
    run(&mut simulation, 40);
    assert_eq!(
        bonus(&simulation, ids, soldier).damage_mult,
        1.0,
        "not until its upgrade"
    );
    cast(&mut simulation, OTHER, CALL, None);
    run(&mut simulation, 31);
    assert_eq!(
        bonus(&simulation, ids, soldier).damage_mult,
        1.5,
        "switched on for good"
    );
    cast(&mut simulation, PLAYER, RAIN, None);
    assert_eq!(
        bonus(&simulation, ids, soldier).damage_mult,
        1.0,
        "LEADERSHIP switched off"
    );
    run(&mut simulation, 30);
    assert_eq!(
        bonus(&simulation, ids, soldier).damage_mult,
        1.0,
        "still raining"
    );
    cast(&mut simulation, OTHER, CLOUDS, None);
    assert_eq!(
        bonus(&simulation, ids, soldier).damage_mult,
        1.5,
        "lifted on the caster's side"
    );
    assert!(
        !placed(&s, &simulation, ids, "Sunbeam").is_empty(),
        "a sunbeam over the enemy infantry"
    );
}

#[test]
fn setting_a_new_weather_effect_clears_whatever_was_running() {
    let s = load();
    let (mut simulation, ids) = simulation(&s, &[("Soldier", 100.0, 100.0, OTHER)]);
    let soldier = placed(&s, &simulation, ids, "Soldier")[0];
    cast(&mut simulation, PLAYER, RAIN, None);
    assert!(
        simulation.state().get(ids.suppressed, soldier).is_some(),
        "Freezing Rain's own block"
    );
    cast(&mut simulation, PLAYER, DARK, None);
    assert!(
        simulation.state().get(ids.suppressed, soldier).is_none(),
        "replaced by Darkness, which names no AntiCategory of its own"
    );
}

#[test]
fn an_object_created_after_the_cast_gets_the_active_weather_effect() {
    let s = load();
    let (mut simulation, ids) = simulation(&s, &[]);
    cast(&mut simulation, PLAYER, DARK, None);
    let soldier = s.kinds.kind("Soldier").unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::Debug(DebugCommand::Spawn {
                kind: soldier.0,
                x: 300.0,
                y: 300.0,
                facing: 0.0,
                role: SpawnRole::Normal,
            }),
        )
        .unwrap();
    run(&mut simulation, 1);
    let later = placed(&s, &simulation, ids, "Soldier")[0];
    assert_eq!(
        bonus(&simulation, ids, later).armor,
        0.5,
        "Darkness's list, given at creation"
    );
}

#[test]
fn recasting_the_same_weather_kind_does_not_refresh_its_timer() {
    let s = load();
    let (mut simulation, ids) = simulation(&s, &[]);
    cast(&mut simulation, PLAYER, DARK, None);
    let clock = crate::clock_entity(simulation.state(), ids).unwrap();
    let first = simulation.state().get(ids.weather, clock).copied().unwrap();
    assert_eq!(first.kind, mint_sage_modules::WeatherKind::Cloudy);
    run(&mut simulation, 10);
    let before_recast = simulation.state().get(ids.weather, clock).copied().unwrap();
    cast(&mut simulation, PLAYER, DARK, None);
    let after_recast = simulation.state().get(ids.weather, clock).copied().unwrap();
    assert_eq!(
        after_recast.timer.get(),
        before_recast.timer.get() - 1,
        "only the cast tick's own countdown moved it; the kind was already CLOUDY"
    );
}

#[test]
fn devastation_reaches_only_what_its_weapons_filter_names() {
    let s = load();
    let (mut simulation, ids) = simulation(
        &s,
        &[
            ("Ent", 110.0, 100.0, OTHER),
            ("Ent", 90.0, 100.0, PLAYER),
            ("Soldier", 120.0, 100.0, OTHER),
        ],
    );
    let ents = placed(&s, &simulation, ids, "Ent");
    let soldier = placed(&s, &simulation, ids, "Soldier")[0];
    let cast_on = cast(&mut simulation, PLAYER, DEVASTATE, Some((100.0, 100.0)));
    assert_eq!(health(&simulation, ids, ents[0]), 700.0, "the enemy Ent");
    assert_eq!(
        health(&simulation, ids, ents[1]),
        1000.0,
        "the caster's Ent"
    );
    assert_eq!(health(&simulation, ids, soldier), 100.0, "not an Ent");
    assert!(
        !fx_names(&simulation, cast_on)
            .contains(&("FX_Devastate".to_owned(), crate::fx::FxSource::Power)),
        "0xE818F0 plays FX at each tree felled, and none stands here"
    );
}

fn forest_tree(x: f32, y: f32, harvestable: bool) -> crate::forest::ForestTreeInit {
    crate::forest::ForestTreeInit {
        pos: Vec2::new(x, y),
        kind: crate::unit::Kind(0),
        grabbable: false,
        harvestable,
        supply: 90,
        tainted: false,
        source_index: 0,
    }
}

fn forest_len(simulation: &Simulation<RotwkRules>, ids: Ids) -> usize {
    let entity = crate::forest::singleton_of(simulation.state(), ids.forest).unwrap();
    simulation.state().get(ids.forest, entity).unwrap().len()
}

#[test]
fn devastation_turns_the_harvestable_trees_in_its_radius_into_money() {
    let s = load();
    let (mut simulation, ids) = simulation_with_trees(
        &s,
        &[],
        vec![
            forest_tree(100.0, 100.0, true),
            forest_tree(150.0, 100.0, true),
            forest_tree(120.0, 100.0, false),
            forest_tree(300.0, 100.0, true),
        ],
    );
    let purse = |simulation: &Simulation<RotwkRules>| {
        crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), PLAYER).amount()
    };
    let before = purse(&simulation);
    let cast_on = cast(&mut simulation, PLAYER, DEVASTATE, Some((100.0, 100.0)));
    assert_eq!(purse(&simulation) - before, 900);
    assert_eq!(forest_len(&simulation, ids), 2);
    let played = fx_names(&simulation, cast_on)
        .into_iter()
        .filter(|(name, _)| name == "FX_Devastate")
        .count();
    assert_eq!(played, 2, "FX at each tree felled");
}

#[test]
fn a_creation_list_tree_joins_the_forest_instead_of_spawning() {
    let s = load();
    let (mut simulation, ids) = simulation(&s, &[]);
    cast(&mut simulation, PLAYER, WOOD, Some((200.0, 200.0)));
    assert_eq!(forest_len(&simulation, ids), 2);
    assert!(placed(&s, &simulation, ids, "Sapling").is_empty());
}

#[test]
fn scavenger_pays_a_kills_bounty_value_once_it_is_bought() {
    let s = load();
    let purse = |simulation: &Simulation<RotwkRules>, ids: Ids| {
        crate::economy::purse_of(simulation.state(), RotwkRules::economy_ids(ids), PLAYER).amount()
    };
    let units = [
        ("Troll", 100.0, 100.0, PLAYER),
        ("Soldier", 120.0, 100.0, OTHER),
    ];
    let (mut plain, ids) = simulation(&s, &units);
    run(&mut plain, 30);
    assert!(
        placed(&s, &plain, ids, "Soldier").is_empty(),
        "the troll killed it"
    );
    assert_eq!(purse(&plain, ids), 1_000, "no bounty without the spell");
    let (mut scavenging, ids) = simulation(&s, &units);
    let command = scavenging
        .rules()
        .purchase_command("SCIENCE_Scavenger")
        .unwrap();
    scavenging.submit(PLAYER, command).unwrap();
    run(&mut scavenging, 30);
    assert_eq!(
        purse(&scavenging, ids),
        1_004,
        "the soldier's BountyValue of 4"
    );
}

#[test]
fn untamed_allegiance_takes_the_creeps_in_its_circle() {
    let s = load();
    let (mut simulation, ids) = simulation(
        &s,
        &[
            ("Creep", 120.0, 100.0, OTHER),
            ("Creep", 300.0, 100.0, OTHER),
            ("Soldier", 110.0, 100.0, OTHER),
        ],
    );
    let creeps = placed(&s, &simulation, ids, "Creep");
    let soldier = placed(&s, &simulation, ids, "Soldier")[0];
    cast(&mut simulation, PLAYER, ALLEGIANCE, Some((100.0, 100.0)));
    let owner = |entity| simulation.state().get(ids.owner, entity).map(|o| o.0);
    assert_eq!(owner(creeps[0]), Some(PLAYER));
    assert_eq!(owner(creeps[1]), Some(OTHER), "outside the range");
    assert_eq!(owner(soldier), Some(OTHER), "not a CREEP");
}

#[test]
fn call_the_horde_builds_its_kind_twenty_times_as_fast() {
    let s = load();
    let built = |cast_first: bool| {
        let (mut simulation, ids) = simulation(&s, &[("Barracks", 100.0, 100.0, PLAYER)]);
        let barracks = placed(&s, &simulation, ids, "Barracks")[0];
        if cast_first {
            cast(&mut simulation, PLAYER, HORDE, None);
        }
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: vec![barracks],
                },
            )
            .unwrap();
        simulation
            .submit(PLAYER, RotwkCommand::QueueUnit { button: 0 })
            .unwrap();
        run(&mut simulation, 20);
        placed(&s, &simulation, ids, "Soldier").len()
    };
    assert_eq!(built(false), 0, "300 frames to build");
    assert_eq!(built(true), 1, "15 frames under the bonus");
}

#[test]
fn a_repeating_nugget_fires_on_its_weapons_own_reload() {
    let s = load();
    let (mut simulation, ids) = simulation(
        &s,
        &[
            ("Moat", 100.0, 100.0, PLAYER),
            ("Soldier", 120.0, 100.0, OTHER),
        ],
    );
    let soldier = placed(&s, &simulation, ids, "Soldier")[0];
    run(&mut simulation, 1);
    assert_eq!(health(&simulation, ids, soldier), 90.0, "ready at once");
    run(&mut simulation, 29);
    assert_eq!(health(&simulation, ids, soldier), 90.0, "reloading");
    run(&mut simulation, 1);
    assert_eq!(health(&simulation, ids, soldier), 80.0, "a second later");
}

#[test]
fn an_alive_only_nugget_does_not_fire_while_its_object_dies() {
    let s = load();
    let (mut simulation, ids) = simulation(
        &s,
        &[
            ("Bomber", 100.0, 100.0, PLAYER),
            ("Troll", 120.0, 100.0, OTHER),
        ],
    );
    let troll = placed(&s, &simulation, ids, "Troll")[0];
    run(&mut simulation, 40);
    assert!(
        placed(&s, &simulation, ids, "Bomber").is_empty(),
        "the troll killed it at once"
    );
    assert_eq!(
        health(&simulation, ids, troll),
        100_000.0,
        "dead before its delay, so never"
    );
}

#[test]
fn a_match_casting_every_new_spell_replays_identically() {
    let run_once = || {
        let s = load();
        let (mut simulation, _ids) = simulation(
            &s,
            &[
                ("Soldier", 100.0, 100.0, PLAYER),
                ("Soldier", 110.0, 100.0, OTHER),
                ("Ent", 120.0, 100.0, OTHER),
                ("Creep", 105.0, 110.0, OTHER),
                ("Captain", 130.0, 100.0, OTHER),
                ("Barracks", 600.0, 600.0, PLAYER),
            ],
        );
        let mut hashes = Vec::new();
        let at = Some((100.0, 100.0));
        for (player, button, target) in [
            (PLAYER, FIRE, at),
            (PLAYER, TAINT, at),
            (PLAYER, DARK, None),
            (OTHER, CALL, None),
            (PLAYER, RAIN, None),
            (OTHER, CLOUDS, None),
            (PLAYER, DEVASTATE, at),
            (PLAYER, ALLEGIANCE, at),
            (PLAYER, HORDE, None),
            (PLAYER, POISON, at),
            (PLAYER, STONE, at),
        ] {
            cast(&mut simulation, player, button, target);
            hashes.push(simulation.state_hash());
            run(&mut simulation, 7);
        }
        for _ in 0..120 {
            simulation.tick().unwrap();
            hashes.push(simulation.state_hash());
        }
        hashes
    };
    let first = run_once();
    let second = run_once();
    assert_eq!(first, second);
    assert!(first.windows(2).any(|pair| pair[0] != pair[1]));
}

#[test]
fn a_units_splash_is_centred_on_its_victim_and_spares_its_own_side() {
    let s = load();
    let (mut simulation, ids) = simulation(
        &s,
        &[
            ("Slinger", 100.0, 100.0, PLAYER),
            ("Soldier", 105.0, 100.0, OTHER),
            ("Soldier", 300.0, 100.0, OTHER),
            ("Soldier", 310.0, 100.0, PLAYER),
            ("Soldier", 315.0, 100.0, OTHER),
        ],
    );
    let soldiers = placed(&s, &simulation, ids, "Soldier");
    let slinger = placed(&s, &simulation, ids, "Slinger")[0];
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![slinger],
            },
        )
        .unwrap();
    simulation
        .submit(
            PLAYER,
            RotwkCommand::AttackObject {
                target: soldiers[1],
            },
        )
        .unwrap();
    run(&mut simulation, 30);
    assert_eq!(health(&simulation, ids, soldiers[1]), 70.0, "the victim");
    assert_eq!(
        health(&simulation, ids, soldiers[3]),
        70.0,
        "the enemy beside it"
    );
    assert_eq!(
        health(&simulation, ids, soldiers[2]),
        100.0,
        "the shooter's own, spared"
    );
    assert_eq!(
        health(&simulation, ids, soldiers[0]),
        100.0,
        "beside the shooter, far from the victim"
    );
}
