use super::*;

const DATA: &str = "\
#define TROOPS Soldier SoldierHorde Veteran VeteranMp\n\
#define TROOP_HP_ADD 20\n\
Upgrade Upgrade_ObjectLevel1\n  Type = OBJECT\nEnd\n\
Upgrade Upgrade_ObjectLevel2\n  Type = OBJECT\nEnd\n\
Upgrade Upgrade_StructureLevel1\n  Type = OBJECT\nEnd\n\
Upgrade Upgrade_Plates\n  Type = OBJECT\n  BuildCost = 100\n  BuildTime = 1\nEnd\n\
Upgrade Upgrade_BarracksLevel2\n  Type = OBJECT\n  BuildCost = 100\n  BuildTime = 1\nEnd\n\
Upgrade Upgrade_Training\n  Type = OBJECT\n  BuildCost = 100\n  BuildTime = 1\nEnd\n\
Upgrade Upgrade_Cheap\n  Type = OBJECT\n  BuildCost = 10\n  BuildTime = 1\nEnd\n\
SpecialPower SpecialAbilityCharge\n  ReloadTime = 30000\nEnd\n\
ModifierList TroopRank2\n  Category = LEVEL\n  Modifier = HEALTH TROOP_HP_ADD\n  \
  Modifier = DAMAGE_ADD 10\n  Duration = 0\nEnd\n\
ModifierList Plates\n  Category = STRUCTURE\n  Modifier = ARMOR 50%\n  Duration = 0\nEnd\n\
ModifierList BarracksRank2\n  Category = LEVEL\n  Modifier = HEALTH 1500\n  \
  Modifier = PRODUCTION 125%\n  Duration = 0\nEnd\n\
ModifierList Harvest\n  Category = STRUCTURE\n  Modifier = PRODUCTION 200%\n  Duration = 0\nEnd\n\
ExperienceLevel FarmLevel1\n  TargetNames = Farm Mill\n  RequiredExperience = 1\n  Rank = 1\n  \
  AttributeModifiers = Harvest\nEnd\n\
ExperienceLevel FarmLevel2\n  TargetNames = Farm Mill\n  RequiredExperience = 60\n  Rank = 2\nEnd\n\
Object Mill\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = AutoDepositUpdate Tag_02\n    DepositTiming = 1000\n    DepositAmount = 10\n    \
    GiveNoXP = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 20\nEnd\n\
Object Farm\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 1000\n  End\n  \
  Behavior = AutoDepositUpdate Tag_02\n    DepositTiming = 1000\n    DepositAmount = 10\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 20\nEnd\n\
ModifierList Swift\n  Category = SPELL\n  Modifier = SPEED 200%\n  Duration = 0\nEnd\n\
ExperienceLevel TroopLevel1\n  TargetNames = TROOPS\n  RequiredExperience = 1\n  \
  ExperienceAward = 3\n  Rank = 1\n  Upgrades = Upgrade_ObjectLevel1\nEnd\n\
ExperienceLevel TroopLevel2\n  TargetNames = TROOPS\n  RequiredExperience = 7\n  \
  ExperienceAward = 4\n  AttributeModifiers = TroopRank2\n  Rank = 2\n  \
  Upgrades = Upgrade_ObjectLevel2\n  SelectionDecal\n    Texture = decal\n  End\nEnd\n\
ExperienceLevel TroopLevel3\n  TargetNames = TROOPS\n  RequiredExperience = 1000\n  \
  ExperienceAward = 5\n  Rank = 3\nEnd\n\
ExperienceLevel BannerLevel1\n  TargetNames = Bannerman\n  RequiredExperience = 1\n  \
  Rank = 1\nEnd\n\
ExperienceLevel BannerLevel2\n  TargetNames = Bannerman\n  RequiredExperience = 7\n  \
  Rank = 2\nEnd\n\
ExperienceLevel TargetLevel1\n  TargetNames = Target Dummy\n  RequiredExperience = 1\n  \
  ExperienceAward = 3\n  Rank = 1\nEnd\n\
ModifierList Frenzy\n  Category = SPELL\n  Modifier = DAMAGE_MULT 50%\n  \
  Modifier = SPELL_DAMAGE 300%\n  Modifier = RATE_OF_FIRE 200%\n  \
  Modifier = SHROUD_CLEARING 100%\n  Duration = 0\nEnd\n\
Weapon Bolt\n  AttackRange = 60\n  DelayBetweenShots = 1000\n  \
  DamageNugget\n    Damage = 10\n    DamageType = MAGIC\n  End\nEnd\n\
Object Frenzied\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  ShroudClearingRange = 100\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Claw\n  End\n  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\n  \
  Behavior = AttributeModifierUpgrade Tag_Frenzy\n    TriggeredBy = Upgrade_ObjectLevel1\n    \
    AttributeModifier = Frenzy\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Caster\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Bolt\n  End\n  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\n  \
  Behavior = AttributeModifierUpgrade Tag_Frenzy\n    TriggeredBy = Upgrade_ObjectLevel1\n    \
    AttributeModifier = Frenzy\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Watcher\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  ShroudClearingRange = 100\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object GuardTower\n  Side = Mordor\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Bolt\n  End\n  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 20\n  GeometryHeight = 20\nEnd\n\
ExperienceLevel TowerLevel1\n  TargetNames = GuardTower\n  RequiredExperience = 1\n  \
  ExperienceAward = 0\n  Rank = 1\nEnd\n\
ExperienceLevel RunnerLevel1\n  TargetNames = Runner Walker Frenzied Caster Watcher\n  RequiredExperience = 1\n  \
  ExperienceAward = 0\n  Rank = 1\n  Upgrades = Upgrade_ObjectLevel1\nEnd\n\
ExperienceLevel BarracksLevel1\n  TargetNames = Barracks VetBarracks\n  RequiredExperience = 1\n  \
  ExperienceAward = 50\n  Rank = 1\n  Upgrades = Upgrade_StructureLevel1\nEnd\n\
ExperienceLevel BarracksLevel2\n  TargetNames = Barracks VetBarracks\n  RequiredExperience = 100\n  \
  ExperienceAward = 60\n  Rank = 2\n  AttributeModifiers = BarracksRank2\n  \
  Upgrades = Upgrade_BarracksLevel2\nEnd\n\
ExperienceLevel BarracksLevel3\n  TargetNames = Barracks VetBarracks\n  RequiredExperience = 1000\n  Rank = 3\nEnd\n\
Armor Plain\n  Armor = DEFAULT 100%\nEnd\n\
Weapon Javelin\n  AttackRange = 150\n  DelayBetweenShots = 500\n  \
  DamageNugget\n    Damage = 10\n    DamageType = PIERCE\n  End\nEnd\n\
Weapon Claw\n  AttackRange = 60\n  DelayBetweenShots = 1000\n  \
  DamageNugget\n    Damage = 10\n    DamageType = SLASH\n  End\nEnd\n\
Object Soldier\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Javelin\n  End\n  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Behavior = AttributeModifierUpgrade Tag_Plates\n    TriggeredBy = Upgrade_Plates\n    \
    AttributeModifier = Plates\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object SoldierHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  CommandSet = SoldierHordeSet\n  \
  VisionRange = 400\n  Behavior = HordeAIUpdate Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  \
  BuildCost = 300\n  BuildTime = 2\n  CommandPoints = 10\n  \
  Behavior = StatusBitsUpgrade ModuleTag_ProductionLegality\n    TriggeredBy = Upgrade_Plates\n  End\n  \
  Behavior = UnpauseSpecialPowerUpgrade Tag_ChargeUnpause\n    \
    SpecialPowerTemplate = SpecialAbilityCharge\n    TriggeredBy = Upgrade_ObjectLevel2\n  End\n  \
  Behavior = SpecialPowerModule Tag_ChargeStarter\n    \
    SpecialPowerTemplate = SpecialAbilityCharge\n    StartsPaused = Yes\n  End\n  \
  Behavior = LevelUpUpgrade Tag_Training\n    TriggeredBy = Upgrade_Training\n    \
    LevelsToGain = 5\n    LevelCap = 2\n  End\n  \
  Behavior = HordeContain Tag_01\n    Slots = 4\n    RanksToReleaseWhenAttacking = 1\n    \
    BannerCarriersAllowed = Bannerman\n    \
    BannerCarrierPosition = UnitType:Soldier Pos:X:30.0 Y:0.0\n    \
    RankInfo = RankNumber:1 UnitType:Soldier Position:X:10 Y:-10 Position:X:10 Y:10 Position:X:-10 Y:-10 Position:X:-10 Y:10\n  \
  End\nEnd\n\
Object GuardHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  BuildCost = 300\n  BuildTime = 2\n  \
  VisionRange = 400\n  Behavior = HordeAIUpdate Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes\n    \
    MoodAttackCheckRate = 500\n  End\n  \
  Behavior = HordeContain Tag_01\n    Slots = 2\n    RanksToReleaseWhenAttacking = 1\n    \
    BannerCarriersAllowed = Bannerman\n    BannerCarrierMinLevel = 0\n    \
    BannerCarrierPosition = UnitType:Soldier Pos:X:-20.0 Y:0.0\n    \
    RankInfo = RankNumber:1 UnitType:Soldier Position:X:10 Y:-10 Position:X:10 Y:10\n  \
  End\nEnd\n\
Object Bannerman\n  Side = Men\n  KindOf = INFANTRY SELECTABLE BANNER\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
CommandSet SoldierHordeSet\n  1 = Buy_Plates\n  2 = Fire_Charge\n  3 = Buy_Training\nEnd\n\
CommandButton Buy_Training\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_Training\nEnd\n\
CommandButton Buy_Plates\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_Plates\nEnd\n\
CommandButton Fire_Charge\n  Command = SPECIAL_POWER\n  SpecialPower = SpecialAbilityCharge\nEnd\n\
Object Target\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 25\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Dummy\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  ArmorSet\n    Conditions = None\n    Armor = Plain\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Striker\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Claw\n  End\n  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Weapon Fist\n  AttackRange = 20\n  DelayBetweenShots = 1000\n  MeleeWeapon = Yes\n  \
  DamageNugget\n    Damage = 10\n    DamageType = CRUSH\n  End\nEnd\n\
Object Smasher\n  Side = Mordor\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Fist\n  End\n  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Weapon Volley\n  AttackRange = 150\n  DelayBetweenShots = 500\n  NoVictimNeeded = Yes\n  \
  DamageNugget\n    Damage = 10\n    Radius = 20\n    DamageType = PIERCE\n  End\nEnd\n\
Object Archer\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Javelin\n    Weapon = TERTIARY Volley\n    \
    AutoChooseSources = TERTIARY FROM_PLAYER\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = No\n  End\n  VisionRange = 400\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object ArcherHorde\n  Side = Men\n  KindOf = SELECTABLE HORDE\n  BuildCost = 300\n  BuildTime = 2\n  \
  CommandPoints = 10\n  VisionRange = 400\n  \
  WeaponSet\n    Conditions = None\n    Weapon = TERTIARY Volley\n    \
    AutoChooseSources = TERTIARY FROM_PLAYER\n  End\n  \
  Behavior = HordeAIUpdate Tag_AI\n    \
    AutoAcquireEnemiesWhenIdle = No\n    MoodAttackCheckRate = 500\n  End\n  \
  Behavior = HordeContain Tag_01\n    Slots = 2\n    RanksToReleaseWhenAttacking = 0\n    \
    RankInfo = RankNumber:0 UnitType:Archer Position:X:10 Y:-10\n    \
    RankInfo = RankNumber:1 UnitType:Archer Position:X:10 Y:10\n  \
  End\nEnd\n\
Object Runner\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = AttributeModifierUpgrade Tag_Swift\n    TriggeredBy = Upgrade_ObjectLevel1\n    \
    AttributeModifier = Swift\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Veteran\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = ExperienceLevelCreate Tag_Create\n    LevelToGrant = 2\n    MPOnly = No\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object VeteranMp\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Behavior = ExperienceLevelCreate Tag_Create\n    LevelToGrant = 2\n    MPOnly = Yes\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Walker\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  BuildCost = 10\n  BuildTime = 1\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\n  GeometryHeight = 16\nEnd\n\
Object Barracks\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  CommandSet = BarracksSet\n  \
  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n  End\n  \
  Behavior = SubObjectsUpgrade Tag_HideAll\n    TriggeredBy = Upgrade_StructureLevel1\n    \
    HideSubObjects = Flag\n  End\n  \
  Behavior = SubObjectsUpgrade Tag_ShowFlag\n    TriggeredBy = Upgrade_Training\n    \
    ShowSubObjects = Flag\n  End\n  \
  Behavior = LevelUpUpgrade Tag_Level2\n    TriggeredBy = Upgrade_BarracksLevel2\n    \
    LevelsToGain = 1\n    LevelCap = 3\n  End\n  \
  Behavior = StatusBitsUpgrade Tag_Cheap\n    TriggeredBy = Upgrade_Cheap\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet BarracksSet\n  1 = Build_SoldierHorde\n  2 = Buy_Level2\n  3 = Build_GuardHorde\n  \
  4 = Buy_Training\n  5 = Build_FakeDozer\n  6 = Buy_Cheap\nEnd\n\
CommandButton Build_SoldierHorde\n  Command = UNIT_BUILD\n  Object = SoldierHorde\n  Radial = Yes\nEnd\n\
CommandButton Build_GuardHorde\n  Command = UNIT_BUILD\n  Object = GuardHorde\nEnd\n\
CommandButton Buy_Level2\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_BarracksLevel2\n  Radial = Yes\nEnd\n\
CommandButton Buy_Training\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_Training\nEnd\n\
CommandButton Build_FakeDozer\n  Command = UNIT_BUILD\n  Object = FakeDozer\nEnd\n\
CommandButton Buy_Cheap\n  Command = OBJECT_UPGRADE\n  Upgrade = Upgrade_Cheap\nEnd\n\
Object FakeDozer\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  \
  BuildCost = 60\n  BuildTime = 50\n  \
  Body = ActiveBody Tag\n    MaxHealth = 50\n  End\n  \
  Behavior = DozerAIUpdate Tag_D\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 5\nEnd\n\
Object VetBarracks\n  Side = Men\n  KindOf = STRUCTURE SELECTABLE IMMOBILE\n  \
  CommandSet = VetBarracksSet\n  BuildCost = 500\n  BuildTime = 10\n  \
  Body = StructureBody Tag_01\n    MaxHealth = 3000\n  End\n  \
  Behavior = ProductionUpdate Tag_02\n    MaxQueueEntries = 3\n    GiveNoXP = Yes\n  End\n  \
  Behavior = StatusBitsUpgrade Tag_Cheap\n    TriggeredBy = Upgrade_Cheap\n  End\n  \
  Geometry = CYLINDER\n  GeometryMajorRadius = 30\n  GeometryHeight = 30\nEnd\n\
CommandSet VetBarracksSet\n  1 = Buy_Cheap\nEnd\n\
ExperienceScalarTable VetScalars\n  Scalars = 1.0 0.5\nEnd\n\
ExperienceLevel ScaledKillerLevel1\n  TargetNames = ScaledKiller\n  RequiredExperience = 1\n  \
  Rank = 1\nEnd\n\
ExperienceLevel ScaledKillerLevel2\n  TargetNames = ScaledKiller\n  RequiredExperience = 2\n  \
  Rank = 2\nEnd\n\
Object ScaledKiller\n  Side = Men\n  KindOf = INFANTRY SELECTABLE\n  ShroudClearingRange = 100\n  \
  ExperienceScalarTable = VetScalars\n  \
  Body = ActiveBody Tag_01\n    MaxHealth = 100000\n  End\n  \
  WeaponSet\n    Conditions = None\n    Weapon = PRIMARY Claw\n  End\n  \
  Behavior = AIUpdateInterface Tag_AI\n    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS\n    \
    MoodAttackCheckRate = 500\n  End\n  VisionRange = 400\nEnd\n";

pub(super) struct Match {
    pub(super) data: GameData,
    pub(super) kinds: Arc<KindTable>,
}

pub(super) fn load() -> Match {
    let doc =
        mint_sage_ini::Document::parse("t.ini", DATA, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    assert!(doc.is_balanced(), "{:?}", doc.diagnostics());
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    let seeds: Vec<String> = [
        "Barracks",
        "Target",
        "Dummy",
        "Striker",
        "Smasher",
        "Runner",
        "Walker",
        "Veteran",
        "VeteranMp",
        "Farm",
        "Mill",
        "Frenzied",
        "Caster",
        "Watcher",
        "GuardTower",
        "SoldierHorde",
        "ArcherHorde",
        "VetBarracks",
        "FakeDozer",
        "ScaledKiller",
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

pub(super) fn rules(m: &Match, placed: &[(&str, f32, f32, PlayerId)]) -> RotwkRules {
    let production = m.kinds.production(&m.data);
    let upgrade_menus = m.kinds.upgrades(&m.data);
    let power_names = crate::powers::PowerNames::build(m.kinds.power_names().map(str::to_owned));
    let power_reload = power_names.reload_table(&m.data.special_powers);
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
        .with_hordes(m.data.formations.tables().to_vec(), Vec::new())
        .with_banners(crate::horde::banner_plans(&m.data.formations, &m.kinds))
        .with_combat(m.data.combat.clone())
        .with_construction(Arc::clone(&m.kinds), production.clone())
        .with_economy(production.buildables.clone(), 100_000)
        .with_menus(production.menus.clone())
        .with_upgrades(upgrade_menus, Arc::clone(&m.data.upgrade_bits))
        .with_powers(power_names, power_reload)
        .with_experience(Arc::clone(&m.data.experience), m.data.armor_bonus_cap)
        .with_experience_scalars(Arc::clone(&m.data.experience_scalars))
}

pub(super) fn run(simulation: &mut Simulation<RotwkRules>, ticks: u32) {
    for _ in 0..ticks {
        simulation.tick().unwrap();
    }
}

fn spawn_horde(
    simulation: &mut Simulation<RotwkRules>,
    ids: Ids,
    x: f32,
    y: f32,
    owner: PlayerId,
) -> (Entity, Vec<Entity>) {
    spawn_horde_of(simulation, ids, "SoldierHorde", x, y, owner)
}

fn spawn_horde_of(
    simulation: &mut Simulation<RotwkRules>,
    ids: Ids,
    label: &str,
    x: f32,
    y: f32,
    owner: PlayerId,
) -> (Entity, Vec<Entity>) {
    let recipe = simulation
        .rules()
        .buildable_catalogue()
        .iter()
        .find(|entry| entry.label == label)
        .unwrap()
        .index;
    let before: Vec<Entity> = simulation
        .state()
        .store(ids.horde)
        .map(|store| store.entities().to_vec())
        .unwrap_or_default();
    simulation
        .submit(
            owner,
            RotwkCommand::DebugSpawnHorde {
                recipe,
                x,
                y,
                facing: 0.0,
            },
        )
        .unwrap();
    run(simulation, 1);
    let store = simulation.state().store(ids.horde).unwrap();
    let entity = store
        .entities()
        .iter()
        .copied()
        .find(|e| !before.contains(e))
        .unwrap();
    let mut members = store.get(entity).unwrap().members.clone();
    members.sort_unstable_by_key(|member| member.index());
    (entity, members)
}

pub(super) fn placed(
    simulation: &Simulation<RotwkRules>,
    ids: Ids,
    m: &Match,
    name: &str,
) -> Vec<Entity> {
    let kind = m.kinds.kind(name).unwrap();
    simulation
        .state()
        .store(ids.kind)
        .unwrap()
        .iter()
        .filter(|(_, k)| **k == kind)
        .map(|(entity, _)| entity)
        .collect()
}

pub(super) fn dead(simulation: &Simulation<RotwkRules>, ids: Ids, of: &[Entity]) -> usize {
    of.iter()
        .filter(|e| {
            !simulation.state().is_live(**e) || simulation.state().contains(ids.doomed, **e)
        })
        .count()
}

pub(super) fn points(simulation: &Simulation<RotwkRules>, ids: Ids, entity: Entity) -> Option<u32> {
    simulation
        .rules()
        .experience_of(simulation.state(), ids, entity)
        .map(|(_, points)| points)
}

pub(super) fn rank(simulation: &Simulation<RotwkRules>, entity: Entity) -> Option<u16> {
    simulation.rules().rank_of(simulation.state(), entity)
}

fn research(simulation: &mut Simulation<RotwkRules>, owner: PlayerId, unit: Entity, button: u8) {
    simulation
        .submit(
            owner,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![unit],
            },
        )
        .unwrap();
    simulation
        .submit(owner, RotwkCommand::ObjectUpgrade { button })
        .unwrap();
    run(simulation, 35);
}

const TARGETS: [(&str, f32, f32, PlayerId); 3] = [
    ("Target", 260.0, 200.0, OTHER),
    ("Target", 262.0, 215.0, OTHER),
    ("Target", 262.0, 185.0, OTHER),
];

#[test]
fn a_kill_by_a_horde_member_gives_the_horde_the_victims_award() {
    let m = load();
    let mut simulation = Simulation::new(11, rules(&m, &TARGETS)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let (horde, members) = spawn_horde(&mut simulation, ids, 200.0, 200.0, PLAYER);
    let targets = placed(&simulation, ids, &m, "Target");
    let mut kills = dead(&simulation, ids, &targets);
    for _ in 0..200 {
        if kills > 0 {
            break;
        }
        run(&mut simulation, 1);
        kills = dead(&simulation, ids, &targets);
    }
    assert!(kills > 0, "the fixture's premise: the horde kills");
    assert!(kills < 3, "and one kill short of all of them");
    let expected = 1 + 3 * u32::try_from(kills).unwrap();
    assert_eq!(points(&simulation, ids, horde), Some(expected));
    for member in &members {
        assert_eq!(
            simulation.state().get(ids.experience, *member),
            None,
            "the member that landed the shot is not credited"
        );
    }
}

#[test]
fn enough_kills_level_the_horde_and_its_members_gain_the_levels_lists() {
    let m = load();
    let mut simulation = Simulation::new(12, rules(&m, &TARGETS)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let (horde, members) = spawn_horde(&mut simulation, ids, 200.0, 200.0, PLAYER);
    let targets = placed(&simulation, ids, &m, "Target");
    assert_eq!(rank(&simulation, horde), Some(1));
    assert!(!simulation
        .rules()
        .power_released(simulation.state(), horde, "SpecialAbilityCharge"));
    for _ in 0..400 {
        run(&mut simulation, 1);
        if dead(&simulation, ids, &targets) >= 2 {
            break;
        }
    }
    assert!(dead(&simulation, ids, &targets) >= 2, "two kills");
    assert_eq!(rank(&simulation, horde), Some(2), "the horde levelled");
    for member in &members {
        assert_eq!(
            rank(&simulation, *member),
            Some(1),
            "a member keeps its own level"
        );
        let health = simulation.state().get(ids.health, *member).unwrap().0;
        assert_eq!(health.max(), 120.0, "TroopRank2's +20 on the maximum");
        let bonuses = simulation.state().get(ids.bonuses, *member).unwrap().0;
        assert_eq!(bonuses.damage_add, 10.0);
    }
    assert!(
        simulation
            .rules()
            .power_released(simulation.state(), horde, "SpecialAbilityCharge"),
        "the level's upgrade released the charge"
    );
}

#[test]
fn a_levelled_unit_hits_harder() {
    let m = load();
    let mut placed_units = TARGETS.to_vec();
    placed_units.push(("Soldier", 200.0, 200.0, PLAYER));
    placed_units.push(("Dummy", 420.0, 200.0, OTHER));
    let mut simulation = Simulation::new(13, rules(&m, &placed_units)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let soldier = placed(&simulation, ids, &m, "Soldier")[0];
    let targets = placed(&simulation, ids, &m, "Target");
    for _ in 0..600 {
        run(&mut simulation, 1);
        if dead(&simulation, ids, &targets) == targets.len() {
            break;
        }
    }
    assert_eq!(points(&simulation, ids, soldier), Some(10), "its own kills");
    assert_eq!(rank(&simulation, soldier), Some(2));
    let dummy = placed(&simulation, ids, &m, "Dummy")[0];
    simulation
        .submit(
            OTHER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![dummy],
            },
        )
        .unwrap();
    simulation
        .submit(
            OTHER,
            RotwkCommand::MoveTo {
                x: 320.0,
                y: 200.0,
                queue: false,
            },
        )
        .unwrap();
    let mut drops = Vec::new();
    let mut last = 100_000.0;
    for _ in 0..240 {
        run(&mut simulation, 1);
        let now = health_of(&simulation, ids, dummy).unwrap();
        if now < last {
            drops.push(last - now);
        }
        last = now;
    }
    assert!(!drops.is_empty(), "the soldier shoots the dummy");
    assert!(
        drops.iter().all(|drop| (*drop - 20.0).abs() < 1e-3),
        "every hit is (10 + 10) x 1, got {drops:?}"
    );
}

#[test]
fn an_armour_modifier_from_an_upgrade_halves_the_hit() {
    let m = load();
    let mut simulation =
        Simulation::new(14, rules(&m, &[("Striker", 240.0, 200.0, OTHER)])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let (horde, members) = spawn_horde(&mut simulation, ids, 200.0, 200.0, PLAYER);
    research(&mut simulation, PLAYER, members[0], 0);
    let bit = m.data.upgrade_bits.bit("Upgrade_Plates").unwrap();
    assert!(simulation
        .state()
        .get(ids.object_upgrades, horde)
        .is_some_and(|held| held.0.contains(bit)));
    for member in &members {
        let bonuses = simulation.state().get(ids.bonuses, *member).unwrap().0;
        assert_eq!(bonuses.armor, 0.5);
    }
    let mut drops = Vec::new();
    let mut last: Vec<f32> = members
        .iter()
        .map(|s| health_of(&simulation, ids, *s).unwrap_or(0.0))
        .collect();
    for _ in 0..120 {
        run(&mut simulation, 1);
        for (at, member) in members.iter().enumerate() {
            let now = health_of(&simulation, ids, *member).unwrap_or(0.0);
            if now < last[at] {
                drops.push(last[at] - now);
            }
            last[at] = now;
        }
    }
    assert!(!drops.is_empty(), "the striker lands hits");
    assert!(
        drops.iter().all(|drop| (*drop - 5.0).abs() < 1e-3),
        "10 x (1 - 0.5), got {drops:?}"
    );
}

#[test]
fn a_structure_levels_by_upgrade_and_its_level_speeds_production() {
    let m = load();
    let mut simulation = Simulation::new(15, rules(&m, &[("Barracks", 0.0, 0.0, PLAYER)])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let barracks = placed(&simulation, ids, &m, "Barracks")[0];
    assert_eq!(rank(&simulation, barracks), Some(1));
    let state = simulation
        .state()
        .get(simulation.rules().upgrade_state_id().unwrap(), barracks)
        .cloned()
        .unwrap();
    assert_eq!(
        state.sub_object("Flag"),
        Some(false),
        "level 1's Upgrade_StructureLevel1 hid it"
    );
    let queue = |simulation: &mut Simulation<RotwkRules>| {
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
        run(simulation, 1);
        let entry = *simulation
            .state()
            .get(ids.producer, barracks)
            .unwrap()
            .queue
            .entries()
            .last()
            .unwrap();
        simulation
            .submit(PLAYER, RotwkCommand::CancelProduction)
            .unwrap();
        run(simulation, 1);
        entry.frames_total
    };
    assert_eq!(queue(&mut simulation), 60, "BuildTime 2 at 30 Hz");
    research(&mut simulation, PLAYER, barracks, 0);
    assert_eq!(rank(&simulation, barracks), Some(2), "LevelUpUpgrade");
    assert_eq!(points(&simulation, ids, barracks), Some(200));
    let health = simulation.state().get(ids.health, barracks).unwrap().0;
    assert_eq!(health.max(), 4500.0);
    assert_eq!(queue(&mut simulation), 48, "60 frames / 1.25");
}

#[test]
fn an_upgrade_researched_under_a_production_bonus_takes_fewer_frames() {
    let m = load();
    let mut simulation = Simulation::new(16, rules(&m, &[("Barracks", 0.0, 0.0, PLAYER)])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let barracks = placed(&simulation, ids, &m, "Barracks")[0];
    research(&mut simulation, PLAYER, barracks, 0);
    assert_eq!(rank(&simulation, barracks), Some(2), "LevelUpUpgrade");
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
        .submit(PLAYER, RotwkCommand::ObjectUpgrade { button: 1 })
        .unwrap();
    run(&mut simulation, 2);
    let entry = *simulation
        .state()
        .get(ids.producer, barracks)
        .unwrap()
        .queue
        .entries()
        .last()
        .unwrap();
    assert_eq!(
        entry.frames_total, 24,
        "BuildTime 1 is 30 frames at 30 Hz, and 30 / 1.25 is 24"
    );
}

#[test]
fn a_speed_modifier_moves_a_unit_faster() {
    let m = load();
    let mut simulation = Simulation::new(
        16,
        rules(
            &m,
            &[
                ("Runner", 100.0, 100.0, PLAYER),
                ("Walker", 100.0, 300.0, PLAYER),
            ],
        ),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let runner = placed(&simulation, ids, &m, "Runner")[0];
    let walker = placed(&simulation, ids, &m, "Walker")[0];
    assert_eq!(
        simulation
            .state()
            .get(ids.bonuses, runner)
            .map(|b| b.0.speed),
        Some(2.0)
    );
    for (unit, y) in [(runner, 100.0), (walker, 300.0)] {
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
                RotwkCommand::MoveTo {
                    x: 800.0,
                    y,
                    queue: false,
                },
            )
            .unwrap();
    }
    run(&mut simulation, 120);
    let travelled = |unit| position(&simulation, ids, unit).x - 100.0;
    let (fast, slow) = (travelled(runner), travelled(walker));
    assert!(slow > 1.0, "the walker moves: {slow}");
    assert!(fast > slow * 1.5, "runner {fast} against walker {slow}");
}

fn hits_near(
    simulation: &mut Simulation<RotwkRules>,
    ids: Ids,
    m: &Match,
    at: &[(f32, f32)],
    ticks: u32,
) -> Vec<Vec<f32>> {
    let dummies: Vec<Entity> = at
        .iter()
        .map(|(x, y)| {
            placed(simulation, ids, m, "Dummy")
                .into_iter()
                .find(|d| {
                    let p = position(simulation, ids, *d);
                    (p.x - x).abs() < 1.0 && (p.y - y).abs() < 1.0
                })
                .unwrap()
        })
        .collect();
    let mut last: Vec<f32> = dummies
        .iter()
        .map(|d| health_of(simulation, ids, *d).unwrap())
        .collect();
    let mut hits = vec![Vec::new(); dummies.len()];
    for _ in 0..ticks {
        run(simulation, 1);
        for (i, dummy) in dummies.iter().enumerate() {
            let now = health_of(simulation, ids, *dummy).unwrap();
            if now < last[i] {
                hits[i].push(last[i] - now);
            }
            last[i] = now;
        }
    }
    hits
}

#[test]
fn rate_of_fire_and_spell_damage_reach_the_weapon() {
    let m = load();
    let mut simulation = Simulation::new(
        19,
        rules(
            &m,
            &[
                ("Frenzied", 200.0, 200.0, OTHER),
                ("Dummy", 240.0, 200.0, PLAYER),
                ("Striker", 200.0, 500.0, OTHER),
                ("Dummy", 240.0, 500.0, PLAYER),
                ("Caster", 600.0, 200.0, OTHER),
                ("Dummy", 640.0, 200.0, PLAYER),
            ],
        ),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let hits = hits_near(
        &mut simulation,
        ids,
        &m,
        &[(240.0, 200.0), (240.0, 500.0), (640.0, 200.0)],
        150,
    );
    let (frenzied, plain, caster) = (&hits[0], &hits[1], &hits[2]);
    assert!(plain.len() >= 3, "the fixture's premise: {plain:?}");
    assert!(
        frenzied.len() >= 2 * plain.len() - 1,
        "frenzied {} hits against plain {}",
        frenzied.len(),
        plain.len()
    );
    assert!(
        frenzied.iter().all(|h| (*h - 5.0).abs() < 1e-3),
        "{frenzied:?}"
    );
    assert!(plain.iter().all(|h| (*h - 10.0).abs() < 1e-3), "{plain:?}");
    assert!(!caster.is_empty());
    assert!(
        caster.iter().all(|h| (*h - 30.0).abs() < 1e-3),
        "{caster:?}"
    );
}

#[test]
fn shroud_clearing_widens_what_a_looker_reveals() {
    let m = load();
    let rules = rules(
        &m,
        &[
            ("Frenzied", 300.0, 300.0, PLAYER),
            ("Watcher", 650.0, 650.0, OTHER),
        ],
    )
    .with_players(2)
    .with_shroud(true);
    let mut simulation = Simulation::new(20, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    run(&mut simulation, 3);
    let shroud = RotwkRules::shroud_ids(ids);
    let (_, _, boosted) = crate::shroud::cell_counts(simulation.state(), shroud, PLAYER);
    let (_, _, plain) = crate::shroud::cell_counts(simulation.state(), shroud, OTHER);
    assert!(plain > 0, "the fixture's premise");
    assert!(
        boosted > 2 * plain,
        "boosted {boosted} against plain {plain}"
    );
}

#[test]
fn a_match_with_kills_and_level_ups_replays_identically() {
    let m = load();
    let mut simulation = Simulation::new(17, rules(&m, &TARGETS)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let (horde, _) = spawn_horde(&mut simulation, ids, 200.0, 200.0, PLAYER);
    run(&mut simulation, 300);
    assert_eq!(
        rank(&simulation, horde),
        Some(2),
        "the recording must contain the level-up it claims to test"
    );
    assert_eq!(
        placed(&simulation, ids, &m, "Bannerman").len(),
        1,
        "and the banner carrier that level-up enlists"
    );
    let final_hash = simulation.state_hash();
    let recording = simulation.into_recording();
    let verified = recording.verify(rules(&m, &TARGETS)).unwrap();
    assert_eq!(verified.final_hash, final_hash);
}

#[test]
fn a_members_power_press_fires_its_hordes_power() {
    let m = load();
    let mut simulation = Simulation::new(22, rules(&m, &[])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let (horde, members) = spawn_horde(&mut simulation, ids, 200.0, 200.0, PLAYER);
    research(&mut simulation, PLAYER, members[0], 1);
    assert!(simulation
        .rules()
        .power_released(simulation.state(), horde, "SpecialAbilityCharge"));
    let power = simulation
        .rules()
        .power_names
        .id("SpecialAbilityCharge")
        .unwrap();
    let ready_on = |simulation: &Simulation<RotwkRules>| {
        simulation
            .state()
            .get(ids.power_book, horde)
            .and_then(|book| book.instance(power))
            .map(|instance| instance.available_on)
    };
    let before = ready_on(&simulation);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: vec![members[0]],
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
    run(&mut simulation, 1);
    assert_ne!(
        ready_on(&simulation),
        before,
        "the horde's charge recharges"
    );
}

#[test]
fn a_horde_gains_its_banner_carrier_on_reaching_rank_two() {
    let m = load();
    let mut simulation = Simulation::new(18, rules(&m, &[])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let (horde, members) = spawn_horde(&mut simulation, ids, 200.0, 200.0, PLAYER);
    assert_eq!(members.len(), 4);
    assert!(
        placed(&simulation, ids, &m, "Bannerman").is_empty(),
        "no carrier at rank 1"
    );
    research(&mut simulation, PLAYER, members[0], 1);
    assert_eq!(rank(&simulation, horde), Some(2));
    let banners = placed(&simulation, ids, &m, "Bannerman");
    assert_eq!(banners.len(), 1, "one carrier at rank 2");
    let banner = banners[0];
    let held = simulation.state().get(ids.horde, horde).unwrap().clone();
    assert!(held.members.contains(&banner));
    assert_eq!(held.slots.len(), 5);
    assert_eq!(
        held.slots[4].offset,
        Vec2::new(24.0, 0.0),
        "the mean is over the rank slots (centred on 0) and the declared \
         banner slot (X:30) together: (10+10-10-10+30)/5 = 6, so the banner \
         ends up at 30-6"
    );
    let member = simulation.state().get(ids.member, banner).copied().unwrap();
    assert_eq!((member.horde, member.slot), (horde, Some(4)));
    assert_eq!(rank(&simulation, banner), Some(2), "the carrier ranks too");
    assert_eq!(
        simulation.state().get(ids.owner, banner).map(|o| o.0),
        Some(PLAYER)
    );
    run(&mut simulation, 90);
    let at = simulation.state().get(ids.body, banner).unwrap().0.position;
    let centre = simulation.state().get(ids.body, horde).unwrap().0.position;
    let (dx, dy) = (at.x - centre.x - 24.0, at.y - centre.y);
    assert!(
        dx * dx + dy * dy < 9.0,
        "at its centred slot: {at:?} vs {centre:?}"
    );
    research(&mut simulation, PLAYER, members[0], 1);
    assert_eq!(
        placed(&simulation, ids, &m, "Bannerman").len(),
        1,
        "only once"
    );
}

#[test]
fn a_member_shows_its_hordes_level_decal() {
    let m = load();
    let mut simulation = Simulation::new(20, rules(&m, &[])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let (horde, members) = spawn_horde(&mut simulation, ids, 200.0, 200.0, PLAYER);
    let decal_of = |simulation: &Simulation<RotwkRules>, entity| {
        simulation
            .rules()
            .rank_decal(simulation.state(), entity)
            .map(|(holder, decal)| (holder, decal.texture.clone()))
    };
    assert_eq!(decal_of(&simulation, members[0]), None, "rank 1 has none");
    research(&mut simulation, PLAYER, members[0], 1);
    assert_eq!(
        decal_of(&simulation, members[1]),
        Some((horde, "decal".to_owned())),
        "the horde's rank-2 decal, under the horde"
    );
}

#[test]
fn debug_experience_goes_to_each_selected_holder_once() {
    let m = load();
    let mut simulation = Simulation::new(21, rules(&m, &[])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let (horde, members) = spawn_horde(&mut simulation, ids, 200.0, 200.0, PLAYER);
    simulation
        .submit(
            PLAYER,
            RotwkCommand::SetSelection {
                replace: true,
                units: members.clone(),
            },
        )
        .unwrap();
    simulation
        .submit(PLAYER, RotwkCommand::DebugExperience { points: 6 })
        .unwrap();
    run(&mut simulation, 1);
    assert_eq!(points(&simulation, ids, horde), Some(7), "1 + 6, once");
    assert_eq!(rank(&simulation, horde), Some(2));
    assert_eq!(placed(&simulation, ids, &m, "Bannerman").len(), 1);
    simulation
        .submit(PLAYER, RotwkCommand::DebugExperience { points: 1000 })
        .unwrap();
    run(&mut simulation, 1);
    assert_eq!(rank(&simulation, horde), Some(3));
    assert_eq!(
        placed(&simulation, ids, &m, "Bannerman").len(),
        1,
        "a later promotion brings no second carrier"
    );
}

#[test]
fn a_banner_carrier_from_level_zero_comes_with_the_horde() {
    let m = load();
    let mut simulation = Simulation::new(19, rules(&m, &[])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let (horde, _) = spawn_horde_of(&mut simulation, ids, "GuardHorde", 300.0, 300.0, PLAYER);
    let held = simulation.state().get(ids.horde, horde).unwrap();
    assert_eq!(held.members.len(), 3, "two soldiers and the carrier");
    assert_eq!(
        held.slots.last().map(|slot| slot.offset),
        Some(Vec2::new(-20.0, 0.0)),
        "the declared X:-20, from the horde's origin like the slots"
    );
    assert_eq!(placed(&simulation, ids, &m, "Bannerman").len(), 1);
}

#[test]
fn a_level_up_upgrade_levels_every_member_once_per_level_and_stops_at_its_cap() {
    let m = load();
    let mut simulation = Simulation::new(18, rules(&m, &[])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let (horde, members) = spawn_horde(&mut simulation, ids, 200.0, 200.0, PLAYER);
    run(&mut simulation, 70);
    research(&mut simulation, PLAYER, members[0], 1);
    assert_eq!(rank(&simulation, horde), Some(2), "capped at rank 2, not 3");
    assert_eq!(
        points(&simulation, ids, horde),
        Some(107),
        "the level's own requirement plus the upgrade's completion XP"
    );
    for member in &members {
        assert_eq!(rank(&simulation, *member), Some(2), "one level each");
        assert!(
            simulation
                .rules()
                .level_feedback_of(simulation.state(), ids, *member)
                .is_empty(),
            "a member's level carries no feedback"
        );
    }
}

#[test]
fn every_level_reached_queues_its_own_feedback_and_none_in_the_first_ten_frames() {
    let gain = |warm_up: u32| {
        let m = load();
        let mut simulation = Simulation::new(21, rules(&m, &[])).unwrap();
        let ids = simulation.rules().ids().unwrap();
        let (horde, members) = spawn_horde(&mut simulation, ids, 200.0, 200.0, PLAYER);
        run(&mut simulation, warm_up);
        simulation
            .submit(
                PLAYER,
                RotwkCommand::SetSelection {
                    replace: true,
                    units: members,
                },
            )
            .unwrap();
        simulation
            .submit(PLAYER, RotwkCommand::DebugExperience { points: 1000 })
            .unwrap();
        run(&mut simulation, 1);
        let events = simulation
            .rules()
            .level_feedback_of(simulation.state(), ids, horde);
        let leveled = simulation
            .rules()
            .modules
            .as_ref()
            .and_then(|wiring| simulation.state().get(wiring.state, horde))
            .is_some_and(|state| state.has_model_condition("LEVELED"));
        (
            events.iter().map(|(_, level)| *level).collect::<Vec<_>>(),
            leveled,
        )
    };
    assert_eq!(gain(70), (vec![1, 2], true));
    assert_eq!(gain(5), (Vec::new(), false));
}

#[test]
fn an_mp_only_level_at_creation_is_not_granted_in_a_campaign() {
    let m = load();
    let place = [("VeteranMp", 100.0, 100.0, PLAYER)];
    let skirmish = Simulation::new(19, rules(&m, &place)).unwrap();
    let ids = skirmish.rules().ids().unwrap();
    let granted = placed(&skirmish, ids, &m, "VeteranMp")[0];
    assert_eq!(rank(&skirmish, granted), Some(2));
    let campaign = Simulation::new(
        19,
        rules(&m, &place).with_script_named_sides(vec![("Gondor".to_owned(), PLAYER)]),
    )
    .unwrap();
    let ids = campaign.rules().ids().unwrap();
    let plain = placed(&campaign, ids, &m, "VeteranMp")[0];
    assert_eq!(rank(&campaign, plain), Some(1), "still at its first level");
}

#[test]
fn an_object_created_at_a_level_starts_there() {
    let m = load();
    let simulation = Simulation::new(19, rules(&m, &[("Veteran", 100.0, 100.0, PLAYER)])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let veteran = placed(&simulation, ids, &m, "Veteran")[0];
    assert_eq!(rank(&simulation, veteran), Some(2));
    let health = simulation.state().get(ids.health, veteran).unwrap().0;
    assert_eq!((health.current(), health.max()), (120.0, 120.0));
}

#[test]
fn a_production_modifier_multiplies_a_farms_income() {
    let m = load();
    let mut simulation = Simulation::new(20, rules(&m, &[("Farm", 300.0, 300.0, PLAYER)])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let economy = RotwkRules::economy_ids(ids);
    let purse = |simulation: &Simulation<RotwkRules>| {
        crate::economy::purse_of(simulation.state(), economy, PLAYER).amount()
    };
    let mut deposits = Vec::new();
    let mut last = purse(&simulation);
    for _ in 0..100 {
        run(&mut simulation, 1);
        let now = purse(&simulation);
        if now != last {
            deposits.push(now - last);
        }
        last = now;
    }
    assert!(!deposits.is_empty(), "the farm pays");
    assert!(
        deposits.iter().all(|d| *d == 20),
        "10 x 2, got {deposits:?}"
    );
}

#[test]
fn a_farm_gains_what_it_deposits_as_experience_unless_it_gives_no_xp() {
    let m = load();
    let placed_here = [
        ("Farm", 300.0, 300.0, PLAYER),
        ("Mill", 500.0, 300.0, PLAYER),
    ];
    let mut simulation = Simulation::new(20, rules(&m, &placed_here)).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let farm = placed(&simulation, ids, &m, "Farm")[0];
    let mill = placed(&simulation, ids, &m, "Mill")[0];
    let start = points(&simulation, ids, farm).unwrap();
    run(&mut simulation, 89);
    assert_eq!(points(&simulation, ids, farm), Some(start + 40));
    assert_eq!(rank(&simulation, farm), Some(1));
    run(&mut simulation, 1);
    assert_eq!(points(&simulation, ids, farm), Some(start + 60));
    assert_eq!(rank(&simulation, farm), Some(2));
    assert_eq!(points(&simulation, ids, mill), Some(start));
    assert_eq!(rank(&simulation, mill), Some(1));
}

#[test]
fn the_ai_presses_a_hordes_upgrade_through_a_member() {
    let m = load();
    let mut simulation = Simulation::new(21, rules(&m, &[])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let (horde, members) = spawn_horde(&mut simulation, ids, 200.0, 200.0, PLAYER);
    let choice = simulation
        .rules()
        .ai_upgrade_choice(simulation.state(), ids, PLAYER);
    assert_eq!(choice, Some((members[0], 0, false)));
    research(&mut simulation, PLAYER, members[0], 0);
    let bit = m.data.upgrade_bits.bit("Upgrade_Plates").unwrap();
    assert!(simulation
        .state()
        .get(ids.object_upgrades, horde)
        .is_some_and(|held| held.0.contains(bit)));
}

fn multiplay_with(unit_xp: f32, building_xp: f32) -> crate::combat::MultiplayMultipliers {
    let text = format!(
        "GameData\n  MultiPlayUnitXPMult = MP1:1.0 MP2:1.0 MP3:1.0 MP4:{unit_xp}\n  \
         MultiPlayBuildingXPMult = MP1:1.0 MP2:1.0 MP3:1.0 MP4:{building_xp}\nEnd\n"
    );
    let doc =
        mint_sage_ini::Document::parse("gd.ini", &text, &mint_sage_ini::BlockTable::corpus_path())
            .unwrap();
    let (data, _) = crate::combat::game_data_from_documents(&[doc], 0, std::time::Instant::now());
    data.multiplay
}

#[test]
fn a_structure_receiver_takes_the_building_scalar_and_a_unit_the_unit_scalar_on_a_kill() {
    let m = load();
    let rules = rules(
        &m,
        &[
            ("Frenzied", 200.0, 200.0, PLAYER),
            ("Target", 230.0, 200.0, OTHER),
            ("GuardTower", 500.0, 500.0, PLAYER),
            ("Target", 530.0, 500.0, OTHER),
        ],
    )
    .with_multiplay_multipliers(multiplay_with(2.0, 5.0));
    let mut simulation = Simulation::new(22, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let unit = placed(&simulation, ids, &m, "Frenzied")[0];
    let structure = placed(&simulation, ids, &m, "GuardTower")[0];
    let targets = placed(&simulation, ids, &m, "Target");
    for _ in 0..300 {
        run(&mut simulation, 1);
        if dead(&simulation, ids, &targets) >= 2 {
            break;
        }
    }
    assert_eq!(dead(&simulation, ids, &targets), 2, "both targets die");
    assert_eq!(
        points(&simulation, ids, unit),
        Some(1 + 3 * 2),
        "the unit's kill scaled by MultiPlayUnitXPMult (2.0)"
    );
    assert_eq!(
        points(&simulation, ids, structure),
        Some(1 + 3 * 5),
        "the structure's kill scaled by MultiPlayBuildingXPMult (5.0)"
    );
}

#[test]
fn dozer_ai_without_dozer_kindof_does_not_preempt() {
    let m = load();
    let mut simulation = Simulation::new(16, rules(&m, &[("Barracks", 0.0, 0.0, PLAYER)])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let barracks = placed(&simulation, ids, &m, "Barracks")[0];
    select(&mut simulation, PLAYER, barracks);
    simulation
        .submit(PLAYER, RotwkCommand::ObjectUpgrade { button: 0 })
        .unwrap();
    run(&mut simulation, 2);
    simulation
        .submit(PLAYER, RotwkCommand::QueueUnit { button: 2 })
        .unwrap();
    run(&mut simulation, 5);
    let entries: Vec<_> = simulation
        .state()
        .get(ids.producer, barracks)
        .unwrap()
        .queue
        .entries()
        .to_vec();
    assert_eq!(entries.len(), 2, "{entries:?}");
    assert!(entries[0].frames_done > 0, "the upgrade keeps progressing");
    assert_eq!(
        entries[1].frames_done, 0,
        "the non-DOZER entry waits its turn: {entries:?}"
    );
}

#[test]
fn upgrade_completion_grants_cost_as_xp() {
    let m = load();
    let mut simulation = Simulation::new(16, rules(&m, &[("Barracks", 0.0, 0.0, PLAYER)])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let barracks = placed(&simulation, ids, &m, "Barracks")[0];
    let before = points(&simulation, ids, barracks).unwrap();
    research(&mut simulation, PLAYER, barracks, 2);
    assert_eq!(
        points(&simulation, ids, barracks),
        Some(before + 10),
        "Upgrade_Cheap's own BuildCost, granted as XP"
    );
}

#[test]
fn upgrade_completion_xp_is_scaled_by_the_building_multiplier() {
    let m = load();
    let rules = rules(&m, &[("Barracks", 0.0, 0.0, PLAYER)])
        .with_multiplay_multipliers(multiplay_with(1.0, 5.0));
    let mut simulation = Simulation::new(16, rules).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let barracks = placed(&simulation, ids, &m, "Barracks")[0];
    let before = points(&simulation, ids, barracks).unwrap();
    research(&mut simulation, PLAYER, barracks, 2);
    assert_eq!(
        points(&simulation, ids, barracks),
        Some(before + 10 * 5),
        "Upgrade_Cheap's BuildCost scaled by MultiPlayBuildingXPMult (5.0)"
    );
}

#[test]
fn give_no_xp_suppresses_upgrade_xp() {
    let m = load();
    let mut simulation =
        Simulation::new(16, rules(&m, &[("VetBarracks", 0.0, 0.0, PLAYER)])).unwrap();
    let ids = simulation.rules().ids().unwrap();
    let vet_barracks = placed(&simulation, ids, &m, "VetBarracks")[0];
    let before = points(&simulation, ids, vet_barracks).unwrap();
    research(&mut simulation, PLAYER, vet_barracks, 0);
    assert_eq!(
        points(&simulation, ids, vet_barracks),
        Some(before),
        "GiveNoXP must suppress the upgrade's completion XP"
    );
}

#[test]
fn a_kills_gain_is_scaled_by_the_table_at_the_killers_own_rank() {
    let m = load();
    let mut simulation = Simulation::new(
        9,
        rules(
            &m,
            &[
                ("ScaledKiller", 50.0, 50.0, PLAYER),
                ("Target", 70.0, 50.0, OTHER),
                ("Target", 70.0, 60.0, OTHER),
            ],
        ),
    )
    .unwrap();
    let ids = simulation.rules().ids().unwrap();
    let killer = placed(&simulation, ids, &m, "ScaledKiller")[0];
    let targets = placed(&simulation, ids, &m, "Target");
    for _ in 0..300 {
        run(&mut simulation, 1);
        if dead(&simulation, ids, &targets) >= 2 {
            break;
        }
    }
    assert_eq!(dead(&simulation, ids, &targets), 2, "both targets die");
    assert_eq!(
        points(&simulation, ids, killer),
        Some(1 + 3 + 1),
        "the second kill is scaled by 0.5 once the killer reaches rank 2"
    );
}
